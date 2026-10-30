"""Compile SGP4/ITRS and sampled attitude into Rust-native SYS-02 steering samples.

Samples are operator-modelled, microsecond-aligned observations. The Rust PAT
planner holds each observation until the next one; no continuous steering
feasibility or physical laser tracking is implied.
"""

from __future__ import annotations

import math

from .optical_geometry import (
    MAX_CONTACTS, TIME_RESOLUTION_NS, _boresight_at, _heads, _unit, _vector,
)

MAX_STEERING_SAMPLES = 10_000


def _angles(link: dict, pat: dict, heads: dict, position, at_ns: int) -> dict:
    """Evaluate both endpoint off-axis angles in the ITRS frame, in degrees."""
    positions = [
        _vector(list(position(link[side], at_ns)), "ITRS position", 3)
        for side in ("from", "to")
    ]
    direction = tuple(b - a for a, b in zip(*positions))
    if not all(math.isfinite(value) for value in direction) or math.hypot(*direction) <= 1.0:
        raise ValueError(f"degenerate optical separation at {at_ns} ns")
    angles = {}
    for side in ("from", "to"):
        ray = direction if side == "from" else tuple(-value for value in direction)
        head = heads[pat[f"{side}_head"]]
        boresight = _boresight_at(head, at_ns)
        cosine = sum(a * b for a, b in zip(
            _unit(ray, "optical line of sight"),
            _unit(tuple(boresight), "optical boresight"),
        ))
        angles[f"{side}_off_axis_deg"] = math.degrees(
            math.acos(max(-1.0, min(1.0, cosine)))
        )
        angles[f"{side}_max_off_axis_deg"] = head["limit"]
    return angles


def compile_native_steering(scenario: dict, compiled: dict, position,
                            probes_per_contact: int, attitude: dict) -> tuple[dict, dict]:
    """Generate bounded steering observations for the Rust PAT runtime.

    Every PAT link gets an actual ITRS observation at t=0, even if its first
    contact starts later. Contact start, interior and final representable
    microsecond observations are added. Administrative faults remain separate.
    """
    if type(probes_per_contact) is not int or probes_per_contact not in (1, 3, 7):
        raise ValueError("probes_per_contact must be 1, 3 or 7")
    if attitude.get("attitude_model") != "sampled_body_to_itrs_quaternion":
        raise ValueError("native steering requires sampled attitude")
    if scenario.get("optical_steering_samples"):
        raise ValueError("existing native steering samples cannot be overwritten")
    if any(fault.get("cause") == "sampled_pointing"
           for fault in scenario.get("faults", [])):
        raise ValueError("native steering cannot mix with sampled_pointing fault edges")
    if "optical_geometry" not in compiled:
        raise ValueError("missing validated optical geometry snapshots")

    duration = scenario["duration_ns"]
    heads = _heads(attitude, duration)
    links = {link["id"]: link for link in scenario["links"]}
    pats = {pat["link_id"]: pat for pat in scenario["optical_pat"]}
    contacts = sorted(
        (contact for contact in scenario["contacts"] if contact["link_id"] in pats),
        key=lambda contact: (contact["start_ns"], contact["link_id"]),
    )
    if len(contacts) > MAX_CONTACTS:
        raise ValueError("optical contact limit exceeded")

    ticks_by_link = {link_id: {0} for link_id in pats}
    reports = []
    for contact in contacts:
        link_id = contact["link_id"]
        start, end = contact["start_ns"], contact["end_ns"]
        last_tick = start + ((end - 1 - start) // TIME_RESOLUTION_NS) * TIME_RESOLUTION_NS
        ticks = {start, last_tick}
        for index in range(1, probes_per_contact + 1):
            offset = ((end - start) // TIME_RESOLUTION_NS) * index // (
                probes_per_contact + 1
            )
            ticks.add(start + offset * TIME_RESOLUTION_NS)
        ticks_by_link[link_id].update(ticks)
        reports.append({
            "link_id": link_id, "start_ns": start, "end_ns": end,
            "sample_count": len(ticks),
        })

    total = sum(len(ticks) for ticks in ticks_by_link.values())
    if total > MAX_STEERING_SAMPLES:
        raise ValueError("native optical steering sample budget exceeded")

    samples = []
    for link_id in sorted(pats):
        for at_ns in sorted(ticks_by_link[link_id]):
            samples.append({
                "link_id": link_id, "at_ns": at_ns,
                **_angles(links[link_id], pats[link_id], heads, position, at_ns),
            })

    result = {key: value for key, value in compiled.items()
              if key != "optical_geometry"}
    result["optical_steering_samples"] = samples
    return result, {
        "probes_per_contact": probes_per_contact,
        "contact_count": len(reports),
        "sample_count": len(samples),
        "contacts": reports,
        "fidelity": "SGP4/ITRS and sampled-attitude observations; discrete sample-and-hold, not continuous steering",
    }
