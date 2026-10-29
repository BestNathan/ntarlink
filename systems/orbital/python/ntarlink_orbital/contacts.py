"""Offline SGP4 / WGS84 contact predictor for ntarlink JSON scenarios.

This module produces sampled *geometric* contacts, not acquired RF/optical
links. Time bounds are half-open and optionally boundary-refined. Python
never runs in the Rust per-packet hot path.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import math
from datetime import datetime, timedelta, timezone
from functools import lru_cache
from importlib.metadata import version
from pathlib import Path

from .kinematics import estimate_range_rate_mps, first_order_doppler_shift_hz

from .geometry import (
    distance_m,
    earth_occludes_segment,
    elevation_deg,
    geometric_delay_ns,
    ground_ecef_m,
)

GEOMETRIC_LINKS = {"user_rf", "feeder", "optical_isl"}
GROUND_KINDS = {"terminal", "ground_station"}


def parse_utc(iso_timestamp: str) -> datetime:
    if not iso_timestamp.endswith("Z"):
        raise ValueError("epoch_utc must be an ISO-8601 UTC timestamp with Z")
    value = datetime.fromisoformat(iso_timestamp.replace("Z", "+00:00"))
    if value.utcoffset() != timedelta(0):
        raise ValueError("epoch_utc must be UTC")
    return value.astimezone(timezone.utc)


def build_sgp4_provider(config: dict, duration_ns: int):
    """Return a cached position provider yielding ITRS x/y/z in meters.

    Skyfield performs the TEME -> ITRS transformation; raw TEME positions
    must never be interpreted as Earth-fixed coordinates.
    """
    from skyfield.api import EarthSatellite, load
    from skyfield.framelib import itrs

    epoch = parse_utc(config["epoch_utc"])
    max_tle_age_days = float(config.get("max_tle_age_days", 14))
    if not math.isfinite(max_tle_age_days) or max_tle_age_days <= 0:
        raise ValueError("max_tle_age_days must be positive and finite")

    timescale = load.timescale(builtin=True)
    satellites = {}
    for entry in config["satellites"]:
        identifier = str(entry["id"])
        if not identifier or identifier in satellites:
            raise ValueError(f"duplicate or missing orbital satellite ID: {identifier}")
        satellite = EarthSatellite(
            entry["tle_line1"], entry["tle_line2"], identifier, timescale
        )
        tle_epoch = satellite.epoch.utc_datetime()
        age_start = abs((epoch - tle_epoch).total_seconds()) / 86400
        age_end = abs(
            (epoch + timedelta(microseconds=duration_ns / 1000) - tle_epoch).total_seconds()
        ) / 86400
        if max(age_start, age_end) > max_tle_age_days:
            raise ValueError(
                f"stale TLE for {identifier}: more than {max_tle_age_days} days from epoch"
            )
        satellites[identifier] = satellite

    @lru_cache(maxsize=16_384)
    def position(satellite_id: str, time_ns: int):
        if satellite_id not in satellites:
            raise ValueError(f"missing TLE for satellite: {satellite_id}")
        t = timescale.from_datetime(epoch + timedelta(microseconds=time_ns / 1000))
        state = satellites[satellite_id].at(t)
        result = tuple(float(v) for v in state.frame_xyz(itrs).m)
        if not all(math.isfinite(v) for v in result):
            raise ValueError(f"SGP4 propagation failed: {satellite_id}, t={time_ns}")
        return result

    return position


def _geo_endpoint(node: dict, time_ns: int, position):
    kind = node["kind"]
    if kind == "satellite":
        return position(node["id"], time_ns)
    if kind in GROUND_KINDS:
        return ground_ecef_m(
            float(node["latitude_deg"]),
            float(node["longitude_deg"]),
            float(node.get("height_m", 0.0)),
        )
    raise ValueError(f"unsupported orbital endpoint kind: {kind}")


def link_geometry_at(link: dict, nodes: dict, time_ns: int, position, config: dict):
    """Return (visible, propagation_delay_ns) for a geometric link."""
    if link["kind"] not in GEOMETRIC_LINKS:
        raise ValueError(f"unsupported geometry link: {link['kind']}")
    a, b = nodes[link["from"]], nodes[link["to"]]
    xyz_a = _geo_endpoint(a, time_ns, position)
    xyz_b = _geo_endpoint(b, time_ns, position)
    length = distance_m(xyz_a, xyz_b)
    if not math.isfinite(length) or length <= 1.0:
        return False, None
    if link.get("max_range_m") is not None and length > float(link["max_range_m"]):
        return False, None
    if link["kind"] == "optical_isl":
        if a["kind"] != "satellite" or b["kind"] != "satellite":
            raise ValueError("optical ISL must join two satellites")
        clearance = float(config.get("isl_clearance_m", 0.0))
        visible = not earth_occludes_segment(xyz_a, xyz_b, clearance)
    else:
        satellite, ground = (a, b) if a["kind"] == "satellite" else (b, a)
        if satellite["kind"] != "satellite" or ground["kind"] not in GROUND_KINDS:
            raise ValueError("user/feeder RF link requires satellite and ground endpoint")
        sat_xyz = xyz_a if a["kind"] == "satellite" else xyz_b
        angle = elevation_deg(
            sat_xyz,
            float(ground["latitude_deg"]),
            float(ground["longitude_deg"]),
            float(ground.get("height_m", 0.0)),
        )
        min_elevation = float(link.get(
            "min_elevation_deg", config.get("min_elevation_deg", 10.0)
        ))
        visible = angle >= min_elevation
    return visible, geometric_delay_ns(length) if visible else None


def _refine_transition(evaluate, left_ns: int, right_ns: int,
                       left_visible: bool, tolerance_ns: int) -> int:
    """Conservatively bracket a visibility edge within a coarse sample.

    The returned boundary lies on the visible side of the crossing: rising
    edges round forward, setting edges round backward. The caller must have
    verified opposite visibility at the endpoints.
    """
    while right_ns - left_ns > tolerance_ns:
        midpoint = left_ns + (right_ns - left_ns) // 2
        if evaluate(midpoint) == left_visible:
            left_ns = midpoint
        else:
            right_ns = midpoint
    return left_ns if left_visible else right_ns


def generate_contact_windows(scenario: dict, config: dict, position) -> list[dict]:
    """Generate half-open geometric contact windows for configured links.

    Coarse samples bracket candidate transitions; optional bisection refines
    their boundaries to refine_tolerance_ns. This cannot detect two or more
    transitions inside a single sample interval. The midpoint propagation
    delay is the routing weight; optional time samples allow Rust to interpolate
    a distinct geometric delay at each packet's transmission completion.
    """
    if scenario.get("schema_version") != 1:
        raise ValueError("scenario schema_version must be 1")
    duration_ns = int(scenario["duration_ns"])
    step_ns = config["step_ns"]
    tolerance_ns = config.get("refine_tolerance_ns", step_ns)
    if any(isinstance(value, bool) or not isinstance(value, int)
           for value in (step_ns, tolerance_ns)):
        raise ValueError("step_ns and refine_tolerance_ns must be integer nanoseconds")
    if duration_ns <= 0 or step_ns <= 0:
        raise ValueError("duration_ns and step_ns must be positive")
    if not 0 < tolerance_ns <= step_ns:
        raise ValueError("refine_tolerance_ns must be within (0, step_ns]")
    if (duration_ns + step_ns - 1) // step_ns > 500_000:
        raise ValueError("orbital sampling budget exceeded (500,000 steps)")
    delay_step_ns = config.get("propagation_delay_sample_step_ns")
    if delay_step_ns is not None and (
        isinstance(delay_step_ns, bool) or not isinstance(delay_step_ns, int)
        or delay_step_ns <= 0
    ):
        raise ValueError("propagation_delay_sample_step_ns must be positive integer nanoseconds")
    delay_sample_total = 0
    nodes = {node["id"]: node for node in scenario["nodes"]}
    orbital_ids = {sat["id"] for sat in config["satellites"]}
    windows = []

    for link in scenario["links"]:
        if link["kind"] not in GEOMETRIC_LINKS:
            continue
        if not any(nodes[link[endpoint]]["kind"] == "satellite"
                   for endpoint in ("from", "to")):
            raise ValueError(f"missing space endpoint on {link['id']}")
        for endpoint in ("from", "to"):
            node = nodes[link[endpoint]]
            if node["kind"] == "satellite" and node["id"] not in orbital_ids:
                raise ValueError(f"TLE not provided for {node['id']}")

        def visible_at(time_ns):
            return link_geometry_at(link, nodes, time_ns, position, config)[0]

        def record_window(opened_ns, closed_ns):
            nonlocal delay_sample_total
            if opened_ns >= closed_ns:
                return
            midpoint = opened_ns + (closed_ns - opened_ns) // 2
            _, delay_ns = link_geometry_at(link, nodes, midpoint, position, config)
            if delay_ns is None:
                raise ValueError(f"invalid contact midpoint for {link['id']}")
            window = {
                "link_id": link["id"],
                "start_ns": opened_ns,
                "end_ns": closed_ns,
                "propagation_delay_ns": delay_ns,
            }
            if delay_step_ns is not None:
                sample_count = (closed_ns - opened_ns - 1) // delay_step_ns + 1
                if opened_ns + (sample_count - 1) * delay_step_ns != closed_ns - 1:
                    sample_count += 1
                delay_sample_total += sample_count
                if delay_sample_total > 500_000:
                    raise ValueError("propagation delay sample budget exceeded (500,000)")
                sample_times = list(range(opened_ns, closed_ns, delay_step_ns))
                if sample_times[-1] != closed_ns - 1:
                    sample_times.append(closed_ns - 1)
                samples = []
                for sample_ns in sample_times:
                    visible, sampled_delay = link_geometry_at(
                        link, nodes, sample_ns, position, config
                    )
                    if not visible or sampled_delay is None:
                        raise ValueError(
                            f"propagation delay sample outside contact visibility: {link['id']}"
                        )
                    samples.append({"at_ns": sample_ns, "delay_ns": sampled_delay})
                window["propagation_delay_samples"] = samples
            windows.append(window)

        def transition_boundary(left_ns, right_ns, left_visible):
            # Without bisection, a setting edge must never extend the
            # contact into an interval known to contain invisible samples.
            # Preserve an exact right-endpoint set when right_ns - 1 is
            # still visible; otherwise use the last known visible sample.
            if tolerance_ns == step_ns:
                if left_visible:
                    return right_ns if visible_at(right_ns - 1) else left_ns
                return right_ns
            return _refine_transition(
                visible_at, left_ns, right_ns, left_visible, tolerance_ns
            )

        previous_ns = 0
        previous_visible = visible_at(0)
        opened_ns = 0 if previous_visible else None
        for time_ns in range(step_ns, duration_ns, step_ns):
            current_visible = visible_at(time_ns)
            if current_visible != previous_visible:
                boundary_ns = transition_boundary(
                    previous_ns, time_ns, previous_visible
                )
                if current_visible:
                    opened_ns = boundary_ns
                elif opened_ns is not None:
                    record_window(opened_ns, boundary_ns)
                    opened_ns = None
            previous_ns, previous_visible = time_ns, current_visible

        # The scenario end is an exclusive cutoff, not a geometric sample.
        # A final partial interval must still be inspected to catch a contact
        # that starts or ends between the last coarse sample and duration.
        if previous_ns < duration_ns - 1:
            final_ns = duration_ns - 1
            final_visible = visible_at(final_ns)
            if final_visible != previous_visible:
                boundary_ns = transition_boundary(
                    previous_ns, final_ns, previous_visible
                )
                if final_visible:
                    opened_ns = boundary_ns
                elif opened_ns is not None:
                    record_window(opened_ns, boundary_ns)
                    opened_ns = None
        if opened_ns is not None:
            record_window(opened_ns, duration_ns)

    windows.sort(key=lambda w: (w["start_ns"], w["link_id"]))
    return windows

def generate_contact_kinematics(
    scenario: dict, config: dict, position, contacts: list[dict]
) -> list[dict]:
    """Sample first-order one-way Doppler at each geometric contact midpoint.

    This produces an observational sidecar, not MAC/PHY synchronization or
    per-packet Doppler. Reference frequencies are explicitly configured per
    directed link; all positions must be Earth-fixed at the same epoch.
    """
    frequencies = config.get("doppler_reference_frequencies_hz")
    if frequencies is None:
        return []
    if not isinstance(frequencies, dict):
        raise ValueError("doppler_reference_frequencies_hz must be a link-to-Hz object")
    links = {link["id"]: link for link in scenario["links"]}
    nodes = {node["id"]: node for node in scenario["nodes"]}
    for link_id, frequency in frequencies.items():
        if link_id not in links or links[link_id]["kind"] not in GEOMETRIC_LINKS:
            raise ValueError(f"unknown or nongeometric Doppler link: {link_id}")
        if (isinstance(frequency, bool) or not isinstance(frequency, (int, float))
                or not math.isfinite(frequency) or frequency <= 0):
            raise ValueError(f"invalid Doppler reference frequency for {link_id}")

    requested_span_ns = config.get("doppler_half_span_ns", 500_000_000)
    if (isinstance(requested_span_ns, bool) or not isinstance(requested_span_ns, int)
            or requested_span_ns <= 0):
        raise ValueError("doppler_half_span_ns must be positive integer nanoseconds")

    samples = []
    for window in contacts:
        link_id = window["link_id"]
        if link_id not in frequencies:
            continue
        start_ns, end_ns = window["start_ns"], window["end_ns"]
        midpoint_ns = start_ns + (end_ns - start_ns) // 2
        half_span_ns = min(
            requested_span_ns, midpoint_ns - start_ns, end_ns - 1 - midpoint_ns
        )
        if half_span_ns <= 0:
            raise ValueError(f"contact too short to estimate Doppler: {link_id}")
        link = links[link_id]

        def range_at(time_ns):
            return distance_m(
                _geo_endpoint(nodes[link["from"]], time_ns, position),
                _geo_endpoint(nodes[link["to"]], time_ns, position),
            )

        rate = estimate_range_rate_mps(range_at, midpoint_ns, half_span_ns)
        samples.append({
            "link_id": link_id,
            "contact_start_ns": start_ns,
            "contact_end_ns": end_ns,
            "sample_ns": midpoint_ns,
            "sample_half_span_ns": half_span_ns,
            "range_rate_mps": round(rate, 6),
            "reference_frequency_hz": frequencies[link_id],
            "doppler_shift_hz": round(
                first_order_doppler_shift_hz(rate, frequencies[link_id]), 6
            ),
        })
    return samples


def generate_scenario(scenario: dict, config: dict, position):
    if scenario.get("contacts"):
        raise ValueError("input scenario must not already contain contacts")
    contacts = generate_contact_windows(scenario, config, position)
    updated = dict(scenario)
    updated["contacts"] = contacts
    return updated


def main(argv=None):
    parser = argparse.ArgumentParser(description="Offline SGP4 contact window compiler")
    parser.add_argument("scenario", type=Path)
    parser.add_argument("orbit_config", type=Path)
    parser.add_argument("--out", required=True, type=Path)
    parser.add_argument("--manifest", type=Path)
    args = parser.parse_args(argv)
    scenario = json.loads(args.scenario.read_text(encoding="utf-8"))
    raw_config = args.orbit_config.read_bytes()
    config = json.loads(raw_config)
    provider = build_sgp4_provider(config, int(scenario["duration_ns"]))
    result = generate_scenario(scenario, config, provider)
    kinematics = generate_contact_kinematics(scenario, config, provider, result["contacts"])
    args.out.write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
    manifest_path = args.manifest or Path(str(args.out) + ".orbit-manifest.json")
    manifest = {
        "schema_version": 1,
        "generator": "ntarlink_orbital.contacts",
        "model": "SGP4 with Skyfield TEME-to-ITRS and WGS84 sampled geometry",
        "epoch_utc": config["epoch_utc"],
        "step_ns": config["step_ns"],
        "contact_boundary_resolution_ns": config.get(
            "refine_tolerance_ns", config["step_ns"]
        ),
        "contact_boundary_method": (
            "sampled"
            if config.get("refine_tolerance_ns", config["step_ns"]) == config["step_ns"]
            else "bracketed_bisection"
        ),
        "sampling_step_ns": config["step_ns"],
        "contact_detection_limit": "Multiple visibility transitions within one sampling interval may be missed",
        "propagation_model": (
            "piecewise linear geometric delay at TX completion, midpoint for route weights"
            if config.get("propagation_delay_sample_step_ns") is not None
            else "vacuum geometric delay sampled per contact midpoint"
        ),
        "propagation_delay_sampling": {
            "step_ns": config.get("propagation_delay_sample_step_ns"),
            "sample_count": sum(
                len(window.get("propagation_delay_samples", []))
                for window in result["contacts"]
            ),
            "method": "ECEF geometric range / vacuum c, linear interpolation in Rust",
            "boundary": "first contact instant and last representable instant",
            "limitations": "not light-time iteration, PHY group delay, or route-weight refresh",
        },
        "orbit_input_sha256": hashlib.sha256(raw_config).hexdigest(),
        "skyfield_version": version("skyfield"),
        "sgp4_version": version("sgp4"),
        "generated_contacts": len(result["contacts"]),
    }
    if config.get("doppler_reference_frequencies_hz"):
        kinematics_path = Path(str(args.out) + ".orbit-kinematics.json")
        kinematics_document = {
            "schema_version": 1,
            "model": "one-way first-order Doppler from finite-difference ECEF range",
            "epoch_utc": config["epoch_utc"],
            "samples": kinematics,
        }
        kinematics_text = json.dumps(kinematics_document, indent=2, allow_nan=False) + "\n"
        kinematics_path.write_text(kinematics_text, encoding="utf-8")
        manifest["kinematics"] = {
            "artifact": kinematics_path.name,
            "sha256": hashlib.sha256(kinematics_text.encode("utf-8")).hexdigest(),
            "sample_count": len(kinematics),
            "reference_frequencies_hz": config["doppler_reference_frequencies_hz"],
            "requested_half_span_ns": config.get("doppler_half_span_ns", 500_000_000),
            "method": "central_range_difference_at_contact_midpoint",
            "limitations": "not a continuous Doppler trace or PHY frequency correction",
        }
    manifest_path.write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    print(f"generated {len(result['contacts'])} contact windows in {args.out}")


if __name__ == "__main__":
    main()
