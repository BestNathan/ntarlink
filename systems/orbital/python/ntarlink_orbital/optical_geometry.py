"""Offline SYS-01 SGP4 to SYS-02 optical pointing snapshot adapter.

Satellite positions are sampled from Skyfield in ITRS at geometric contact
starts. Optical-head attitudes may be fixed or interpolated from bounded,
time-indexed body-to-ITRS quaternion samples. Optional finite steering probes
fail closed on sampled violations; no continuous tracking is certified.
"""

from __future__ import annotations

import argparse
from bisect import bisect_right
import hashlib
import json
import math
import os
import tempfile
from pathlib import Path

from .contacts import build_sgp4_provider, parse_utc

MAX_INPUT_BYTES = 16_000_000
MAX_CONTACTS = 10_000
TIME_RESOLUTION_NS = 1_000
MAX_ATTITUDE_SAMPLES = 100_000


def _unique_object(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError(f"duplicate JSON key: {key}")
        result[key] = value
    return result


def _read_json(path: Path) -> tuple[dict, bytes]:
    with path.open("rb") as source:
        raw = source.read(MAX_INPUT_BYTES + 1)
    if len(raw) > MAX_INPUT_BYTES:
        raise ValueError(f"input exceeds size limit: {path}")
    try:
        value = json.loads(
            raw.decode("utf-8"), object_pairs_hook=_unique_object,
            parse_constant=lambda value: (_ for _ in ()).throw(
                ValueError(f"nonfinite JSON constant: {value}")
            ),
        )
    except (UnicodeError, json.JSONDecodeError) as exc:
        raise ValueError(f"invalid UTF-8 JSON: {path}") from exc
    if not isinstance(value, dict):
        raise ValueError(f"expected JSON object: {path}")
    return value, raw


def _vector(value, name: str, length: int) -> tuple[float, ...]:
    if (not isinstance(value, list) or len(value) != length
            or any(isinstance(v, bool) or not isinstance(v, (int, float))
                   or not math.isfinite(v) for v in value)):
        raise ValueError(f"{name} must contain {length} finite numeric components")
    return tuple(float(v) for v in value)


def _unit(value: tuple[float, ...], name: str) -> tuple[float, ...]:
    magnitude = math.hypot(*value)
    if not math.isfinite(magnitude) or magnitude <= 0:
        raise ValueError(f"{name} must be nonzero")
    return tuple(v / magnitude for v in value)


def _rotate_body_to_itrs(boresight: tuple[float, ...],
                         quaternion: tuple[float, ...]) -> list[float]:
    w, x, y, z = quaternion
    vx, vy, vz = boresight
    # v' = v + 2 * (w * (q_xyz cross v) + q_xyz cross (q_xyz cross v)).
    cx, cy, cz = y * vz - z * vy, z * vx - x * vz, x * vy - y * vx
    dx, dy, dz = y * cz - z * cy, z * cx - x * cz, x * cy - y * cx
    return list(_unit((
        vx + 2 * (w * cx + dx),
        vy + 2 * (w * cy + dy),
        vz + 2 * (w * cz + dz),
    ), "rotated boresight"))


def _slerp(a: tuple[float, ...], b: tuple[float, ...],
           fraction: float) -> tuple[float, ...]:
    """Shortest-arc unit-quaternion interpolation (q and -q are equivalent)."""
    dot = sum(x * y for x, y in zip(a, b))
    if dot < 0.0:
        b = tuple(-x for x in b)
        dot = -dot
    dot = min(1.0, max(-1.0, dot))
    if dot > 0.9995:
        return _unit(tuple((1.0 - fraction) * x + fraction * y
                           for x, y in zip(a, b)), "interpolated quaternion")
    theta = math.acos(dot)
    denominator = math.sin(theta)
    wa = math.sin((1.0 - fraction) * theta) / denominator
    wb = math.sin(fraction * theta) / denominator
    return _unit(tuple(wa * x + wb * y for x, y in zip(a, b)),
                 "interpolated quaternion")


def _boresight_at(head: dict, at_ns: int) -> list[float]:
    if head["samples"] is None:
        return head["boresight"]
    times = head["times"]
    if at_ns < times[0] or at_ns > times[-1]:
        raise ValueError("attitude samples do not cover requested time")
    index = bisect_right(times, at_ns) - 1
    if index == len(times) - 1 or times[index] == at_ns:
        quaternion = head["samples"][index]
    else:
        fraction = (at_ns - times[index]) / (times[index + 1] - times[index])
        quaternion = _slerp(head["samples"][index], head["samples"][index + 1],
                            fraction)
    return _rotate_body_to_itrs(head["axis"], quaternion)


def _heads(config: dict, duration_ns: int) -> dict[str, dict]:
    model = config.get("attitude_model")
    if config.get("frame") != "ITRS" or model not in (
            "fixed_body_to_itrs_quaternion", "sampled_body_to_itrs_quaternion"):
        raise ValueError("attitude frame/model must use ITRS body-to-ITRS quaternions")
    entries = config.get("heads")
    if not isinstance(entries, list) or not entries or len(entries) > MAX_CONTACTS:
        raise ValueError("heads must be a nonempty bounded list")
    heads = {}
    total_samples = 0
    for head in entries:
        fixed = model == "fixed_body_to_itrs_quaternion"
        expected = {"head_id", "boresight_body", "max_off_axis_deg"}
        expected.add("body_to_itrs_quaternion_wxyz" if fixed else "attitude_samples")
        if not isinstance(head, dict) or set(head) != expected:
            raise ValueError("invalid optical head attitude contract")
        identifier = head["head_id"]
        if not isinstance(identifier, str) or not identifier or identifier in heads:
            raise ValueError("duplicate or invalid optical head ID")
        axis = _unit(_vector(head["boresight_body"], "boresight_body", 3),
                     "boresight_body")
        limit = head["max_off_axis_deg"]
        if (isinstance(limit, bool) or not isinstance(limit, (int, float))
                or not math.isfinite(limit) or not 0 <= limit <= 180):
            raise ValueError("max_off_axis_deg must be finite within [0, 180]")
        samples = None
        times = None
        boresight = None
        if fixed:
            quaternion = _vector(head["body_to_itrs_quaternion_wxyz"],
                                 "quaternion", 4)
            if abs(math.hypot(*quaternion) - 1.0) > 1e-6:
                raise ValueError("body-to-ITRS quaternion must be unit length")
            boresight = _rotate_body_to_itrs(
                axis, _unit(quaternion, "quaternion"))
        else:
            records = head["attitude_samples"]
            if (not isinstance(records, list) or len(records) < 2
                    or len(records) > MAX_ATTITUDE_SAMPLES):
                raise ValueError("attitude_samples must contain 2 to 100000 samples")
            total_samples += len(records)
            if total_samples > MAX_ATTITUDE_SAMPLES:
                raise ValueError("total attitude_samples exceed 100000")
            times = []
            samples = []
            for record in records:
                if (not isinstance(record, dict) or set(record) != {
                        "at_ns", "body_to_itrs_quaternion_wxyz"}):
                    raise ValueError("invalid attitude sample contract")
                at_ns = record["at_ns"]
                if (type(at_ns) is not int or at_ns < 0 or at_ns > duration_ns
                        or at_ns % TIME_RESOLUTION_NS
                        or (times and at_ns <= times[-1])):
                    raise ValueError("attitude sample times must be ordered, unique and microsecond-aligned")
                quaternion = _vector(record["body_to_itrs_quaternion_wxyz"],
                                     "quaternion", 4)
                if abs(math.hypot(*quaternion) - 1.0) > 1e-6:
                    raise ValueError("body-to-ITRS quaternion must be unit length")
                times.append(at_ns)
                samples.append(_unit(quaternion, "quaternion"))
            last_tick = ((duration_ns - 1) // TIME_RESOLUTION_NS) * TIME_RESOLUTION_NS
            if times[0] != 0 or times[-1] < last_tick:
                raise ValueError("attitude samples must cover the entire virtual-time interval")
        heads[identifier] = {
            "axis": axis, "boresight": boresight, "limit": float(limit),
            "times": times, "samples": samples,
        }
    return heads


def compile_optical_geometry(scenario: dict, orbit: dict, attitude: dict, position) -> dict:
    """Compile one immutable ITRS pointing snapshot per PAT geometric contact."""
    if scenario.get("schema_version") != 1 or scenario.get("contact_mode") != "scheduled":
        raise ValueError("requires schema_version 1 scheduled-contact scenario")
    if (scenario.get("optical_geometry") or scenario.get("optical_pointing")
            or scenario.get("optical_steering_samples")):
        raise ValueError("existing optical pointing inputs cannot be overwritten")
    duration = scenario.get("duration_ns")
    if type(duration) is not int or duration <= 0:
        raise ValueError("duration_ns must be a positive integer")
    parse_utc(orbit["epoch_utc"])
    heads = _heads(attitude, duration)
    nodes = {n["id"]: n for n in scenario["nodes"]}
    links = {link["id"]: link for link in scenario["links"]}
    pats = scenario.get("optical_pat")
    if not isinstance(pats, list) or not pats:
        raise ValueError("requires nonempty optical_pat links")
    pat_by_link = {}
    used_heads = set()
    for pat in pats:
        link_id = pat["link_id"]
        if link_id in pat_by_link or link_id not in links:
            raise ValueError("duplicate or unknown optical PAT link")
        link = links[link_id]
        if link["kind"] != "optical_isl":
            raise ValueError("PAT link must be an optical ISL")
        for side in ("from", "to"):
            satellite = link[side]
            head_id = pat[f"{side}_head"]
            if (satellite not in nodes or nodes[satellite]["kind"] != "satellite"
                    or head_id not in heads or not head_id.startswith(satellite + "/")):
                raise ValueError("missing or incorrectly owned optical head")
            used_heads.add(head_id)
        pat_by_link[link_id] = pat
    if used_heads != set(heads):
        raise ValueError("attitude heads must exactly match referenced PAT heads")

    contacts = [c for c in scenario.get("contacts", []) if c["link_id"] in pat_by_link]
    if not contacts or len(contacts) > MAX_CONTACTS:
        raise ValueError("PAT contacts must be nonempty and within the 10000 limit")
    seen = set()
    for contact in contacts:
        link_id = contact["link_id"]
        start, end = contact["start_ns"], contact["end_ns"]
        if (type(start) is not int or type(end) is not int or start < 0
                or start >= end or end > duration or start % TIME_RESOLUTION_NS):
            raise ValueError("invalid contact bounds or sub-microsecond start")
        key = (link_id, start)
        if key in seen:
            raise ValueError("duplicate optical contact start")
        seen.add(key)
    orbit_ids = [entry["id"] for entry in orbit["satellites"]]
    required_ids = {links[c["link_id"]][side] for c in contacts for side in ("from", "to")}
    if len(orbit_ids) != len(set(orbit_ids)) or not required_ids.issubset(orbit_ids):
        raise ValueError("missing or duplicate orbital satellite ID")

    geometry = []
    for contact in sorted(contacts, key=lambda c: (c["start_ns"], c["link_id"])):
        link = links[contact["link_id"]]
        pat = pat_by_link[contact["link_id"]]
        at_ns = contact["start_ns"]
        vectors = []
        for side in ("from", "to"):
            vector = position(link[side], at_ns)
            vectors.append(list(_vector(list(vector), "ITRS position", 3)))
        if math.dist(*vectors) <= 1.0:
            raise ValueError("degenerate optical line-of-sight separation")
        a, b = heads[pat["from_head"]], heads[pat["to_head"]]
        geometry.append({
            "link_id": contact["link_id"], "at_ns": at_ns, "frame": "ITRS",
            "from_position_m": vectors[0], "to_position_m": vectors[1],
            "from_boresight_itrs": _boresight_at(a, at_ns),
            "to_boresight_itrs": _boresight_at(b, at_ns),
            "from_max_off_axis_deg": a["limit"],
            "to_max_off_axis_deg": b["limit"],
        })
    return {**scenario, "optical_geometry": geometry}



def audit_optical_pointing(scenario: dict, compiled: dict, position,
                           probes_per_contact: int, attitude: dict) -> dict:
    """Reject contacts with sampled steering violations before artifact emission.

    Samples include the contact start, evenly spaced interior probes and the
    last representable microsecond before the exclusive contact end. Duplicate
    microsecond instants in short windows are evaluated only once. This is a
    finite-probe audit, not a continuous bound on optical tracking.
    """
    if type(probes_per_contact) is not int or probes_per_contact not in (1, 3, 7):
        raise ValueError("probes_per_contact must be 1, 3 or 7")
    snapshots = {(g["link_id"], g["at_ns"]): g
                 for g in compiled["optical_geometry"]}
    links = {link["id"]: link for link in scenario["links"]}
    heads = _heads(attitude, scenario["duration_ns"])
    pat_by_link = {p["link_id"]: p for p in scenario["optical_pat"]}
    reports = []
    total_samples = 0
    minimum_margin = math.inf
    for contact in sorted(scenario["contacts"],
                          key=lambda c: (c["start_ns"], c["link_id"])):
        key = (contact["link_id"], contact["start_ns"])
        if key not in snapshots:
            continue
        g = snapshots[key]
        start, end = contact["start_ns"], contact["end_ns"]
        last_tick = start + ((end - 1 - start) // TIME_RESOLUTION_NS) * TIME_RESOLUTION_NS
        ticks = {start, last_tick}
        for index in range(1, probes_per_contact + 1):
            offset_ticks = ((end - start) // TIME_RESOLUTION_NS) * index // (
                probes_per_contact + 1)
            ticks.add(start + offset_ticks * TIME_RESOLUTION_NS)
        worst_margin = math.inf
        worst_time = start
        max_angles = [0.0, 0.0]
        for at_ns in sorted(ticks):
            if at_ns == start:
                vectors = [g["from_position_m"], g["to_position_m"]]
            else:
                link = links[contact["link_id"]]
                vectors = [
                    list(_vector(list(position(link[side], at_ns)),
                                 "ITRS position", 3))
                    for side in ("from", "to")
                ]
            direction = tuple(b - a for a, b in zip(*vectors))
            if math.hypot(*direction) <= 1.0 or not all(
                    math.isfinite(component) for component in direction):
                raise ValueError(f"degenerate optical separation at {key} time {at_ns}")
            for side_index, side in enumerate(("from", "to")):
                ray = direction if side_index == 0 else tuple(-x for x in direction)
                boresight = _boresight_at(
                    heads[pat_by_link[key[0]][f"{side}_head"]], at_ns)
                unit_ray = _unit(ray, "optical line of sight")
                unit_axis = _unit(tuple(boresight), "optical boresight")
                cosine = sum(a * b for a, b in zip(unit_ray, unit_axis))
                angle = math.degrees(math.acos(max(-1.0, min(1.0, cosine))))
                max_angles[side_index] = max(max_angles[side_index], angle)
                margin = g[f"{side}_max_off_axis_deg"] - angle
                if margin < worst_margin:
                    worst_margin, worst_time = margin, at_ns
                if margin < -1e-9:
                    raise ValueError(
                        f"optical pointing probe outside envelope: {key} "
                        f"at {at_ns} ns ({side}, {angle:.9f} deg)"
                    )
        total_samples += len(ticks)
        minimum_margin = min(minimum_margin, worst_margin)
        reports.append({
            "link_id": key[0], "start_ns": start, "end_ns": end,
            "sample_count": len(ticks), "worst_sample_ns": worst_time,
            "minimum_steering_margin_deg": worst_margin,
            "max_from_off_axis_deg": max_angles[0],
            "max_to_off_axis_deg": max_angles[1],
        })
    return {
        "probes_per_contact": probes_per_contact,
        "contact_count": len(reports),
        "sample_count": total_samples,
        "minimum_steering_margin_deg": minimum_margin,
        "contacts": reports,
        "fidelity": "finite microsecond-aligned probes; no continuous steering guarantee",
    }


def _atomic_write(path: Path, data: bytes) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = None
    try:
        with tempfile.NamedTemporaryFile(dir=path.parent, prefix=".optical-",
                                         delete=False) as target:
            temporary = Path(target.name)
            target.write(data)
        os.replace(temporary, path)
    finally:
        if temporary is not None:
            temporary.unlink(missing_ok=True)


def main(argv=None):
    parser = argparse.ArgumentParser(description="Compile SGP4 ITRS optical pointing snapshots")
    parser.add_argument("scenario", type=Path)
    parser.add_argument("orbit_config", type=Path)
    parser.add_argument("attitude_config", type=Path)
    parser.add_argument("--out", required=True, type=Path)
    probe_options = parser.add_mutually_exclusive_group()
    probe_options.add_argument("--pointing-probes-per-contact", type=int,
                               choices=(1, 3, 7),
                               help="Fail closed on finite interior/end steering probes")
    probe_options.add_argument("--runtime-pointing-probes-per-contact", type=int,
                               choices=(1, 3, 7),
                               help="Compile sampled steering loss/recovery into PAT link faults")
    probe_options.add_argument("--native-steering-probes-per-contact", type=int,
                               choices=(1, 3, 7),
                               help="Compile SGP4/ITRS and sampled attitude into Rust-native steering observations")
    args = parser.parse_args(argv)
    inputs = [args.scenario, args.orbit_config, args.attitude_config]
    manifest_path = Path(str(args.out) + ".optical-manifest.json")
    if len({p.resolve() for p in [*inputs, args.out, manifest_path]}) != 5:
        raise ValueError("input and output paths must be distinct")
    loaded = [_read_json(path) for path in inputs]
    (scenario, scenario_bytes), (orbit, orbit_bytes), (attitude, attitude_bytes) = loaded
    provider = build_sgp4_provider(orbit, scenario["duration_ns"],
                                  args.orbit_config.resolve().parent)
    compiled = compile_optical_geometry(scenario, orbit, attitude, provider)
    audit = (audit_optical_pointing(scenario, compiled, provider,
                                    args.pointing_probes_per_contact, attitude)
             if args.pointing_probes_per_contact is not None else None)
    tracking_plan = None
    if args.runtime_pointing_probes_per_contact is not None:
        from .optical_tracking import compile_pointing_faults
        compiled, tracking_plan = compile_pointing_faults(
            scenario, compiled, provider, args.runtime_pointing_probes_per_contact,
            attitude,
        )
    native_plan = None
    if args.native_steering_probes_per_contact is not None:
        from .optical_native_steering import compile_native_steering
        compiled, native_plan = compile_native_steering(
            scenario, compiled, provider, args.native_steering_probes_per_contact,
            attitude,
        )
    output = (json.dumps(compiled, indent=2, allow_nan=False) + "\n").encode()
    manifest = {
        "schema_version": 1,
        "generator": "ntarlink_orbital.optical_geometry",
        "artifact": args.out.name,
        "artifact_sha256": hashlib.sha256(output).hexdigest(),
        "scenario_input_sha256": hashlib.sha256(scenario_bytes).hexdigest(),
        "orbit_input_sha256": hashlib.sha256(orbit_bytes).hexdigest(),
        "attitude_input_sha256": hashlib.sha256(attitude_bytes).hexdigest(),
        "frame": "ITRS", "time_scale": "UTC", "epoch_utc": orbit["epoch_utc"],
        "snapshot_count": len(compiled.get("optical_geometry", [])),
        "steering_sample_count": len(compiled.get("optical_steering_samples", [])),
        "native_steering_plan": native_plan,
        "attitude_model": attitude["attitude_model"],
        "pointing_probe_audit": audit,
        "pointing_tracking_plan": tracking_plan,
        "earth_orientation": getattr(provider, "earth_orientation", {"mode": "external_provider"}),
        "limitations": "operator-supplied body-to-ITRS attitude and finite ITRS samples; no continuous tracking, physical steering or optical PHY",
    }
    _atomic_write(args.out, output)
    _atomic_write(manifest_path, (json.dumps(manifest, indent=2, allow_nan=False) + "\n").encode())
    print(f"compiled {manifest['snapshot_count']} ITRS snapshots and "
          f"{manifest['steering_sample_count']} steering samples to {args.out}")


if __name__ == "__main__":
    main()
