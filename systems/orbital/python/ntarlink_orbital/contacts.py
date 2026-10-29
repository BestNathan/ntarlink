"""Offline SGP4 / WGS84 contact predictor for ntarlink JSON scenarios.

This module produces sampled *geometric* contacts, not acquired RF/optical
links. Time bounds are half-open and quantized by step_ns. Python never runs
in the Rust per-packet hot path.
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


def generate_contact_windows(scenario: dict, config: dict, position) -> list[dict]:
    """Generate sample-aligned, half-open contact windows for supported links.

    Contact boundaries have sampling uncertainty up to step_ns; propagation
    delay is estimated from the midpoint and held constant for each window.
    """
    if scenario.get("schema_version") != 1:
        raise ValueError("scenario schema_version must be 1")
    duration_ns = int(scenario["duration_ns"])
    step_ns = int(config["step_ns"])
    if duration_ns <= 0 or step_ns <= 0:
        raise ValueError("duration_ns and step_ns must be positive")
    if (duration_ns + step_ns - 1) // step_ns > 500_000:
        raise ValueError("orbital sampling budget exceeded (500,000 steps)")
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
        start = None
        for time_ns in range(0, duration_ns + step_ns, step_ns):
            t = min(time_ns, duration_ns)
            open_now = False if t == duration_ns else link_geometry_at(
                link, nodes, t, position, config
            )[0]
            if open_now and start is None:
                start = t
            elif not open_now and start is not None:
                midpoint = start + (t - start) // 2
                _, delay_ns = link_geometry_at(link, nodes, midpoint, position, config)
                if delay_ns is None:
                    raise ValueError(f"invalid contact midpoint for {link['id']}")
                windows.append({
                    "link_id": link["id"],
                    "start_ns": start,
                    "end_ns": t,
                    "propagation_delay_ns": delay_ns,
                })
                start = None
            if t == duration_ns:
                break
    windows.sort(key=lambda w: (w["start_ns"], w["link_id"]))
    return windows


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
    args.out.write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
    manifest_path = args.manifest or Path(str(args.out) + ".orbit-manifest.json")
    manifest = {
        "schema_version": 1,
        "generator": "ntarlink_orbital.contacts",
        "model": "SGP4 with Skyfield TEME-to-ITRS and WGS84 sampled geometry",
        "epoch_utc": config["epoch_utc"],
        "step_ns": config["step_ns"],
        "contact_boundary_resolution_ns": config["step_ns"],
        "propagation_model": "vacuum geometric delay sampled per contact midpoint",
        "orbit_input_sha256": hashlib.sha256(raw_config).hexdigest(),
        "skyfield_version": version("skyfield"),
        "sgp4_version": version("sgp4"),
        "generated_contacts": len(result["contacts"]),
    }
    manifest_path.write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    print(f"generated {len(result['contacts'])} contact windows in {args.out}")


if __name__ == "__main__":
    main()
