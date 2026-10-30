"""Compile sampled SYS-02 pointing loss into existing SYS-03 link-fault events.

This is a bounded offline virtual-time approximation. A sampled violation
interrupts PAT tracking at that sample, and a later passing sample permits
reacquisition. It does not model continuous pointing or a laser PHY.
"""

from __future__ import annotations

import math

from .optical_geometry import (
    MAX_CONTACTS, TIME_RESOLUTION_NS, _boresight_at, _heads, _unit, _vector,
)

MAX_TRACKING_FAULTS = 10_000
ANGLE_TOLERANCE_DEG = 1e-9


def compile_pointing_faults(scenario: dict, compiled: dict, position,
                            probes_per_contact: int, attitude: dict) -> tuple[dict, dict]:
    """Map sampled per-link pointing feasibility to down/up PAT fault edges.

    A contact must be feasible at its start, because the existing Rust
    optical_geometry contract admits or rejects a whole geometric contact
    using its start snapshot. Later probes may interrupt and restore it.
    Administrative faults on the same PAT link cannot safely be composed
    with derived pointing faults, so that combination is rejected.
    """
    if type(probes_per_contact) is not int or probes_per_contact not in (1, 3, 7):
        raise ValueError("probes_per_contact must be 1, 3 or 7")
    if attitude.get("attitude_model") != "sampled_body_to_itrs_quaternion":
        raise ValueError("runtime pointing faults require sampled attitude")
    duration = scenario["duration_ns"]
    heads = _heads(attitude, duration)
    links = {link["id"]: link for link in scenario["links"]}
    pats = {pat["link_id"]: pat for pat in scenario["optical_pat"]}
    if any(fault["link_id"] in pats for fault in scenario.get("faults", [])):
        raise ValueError("existing PAT link faults cannot be combined with sampled pointing faults")
    snapshots = {
        (snapshot["link_id"], snapshot["at_ns"]): snapshot
        for snapshot in compiled["optical_geometry"]
    }
    contacts = sorted(
        (contact for contact in scenario["contacts"] if contact["link_id"] in pats),
        key=lambda contact: (contact["start_ns"], contact["link_id"]),
    )
    if len(contacts) > MAX_CONTACTS:
        raise ValueError("optical contact limit exceeded")
    events = []
    reports = []
    total_probes = 0
    for contact in contacts:
        link_id = contact["link_id"]
        start, end = contact["start_ns"], contact["end_ns"]
        key = (link_id, start)
        if key not in snapshots:
            raise ValueError(f"missing optical geometry snapshot: {key}")
        snapshot = snapshots[key]
        last_tick = start + ((end - 1 - start) // TIME_RESOLUTION_NS) * TIME_RESOLUTION_NS
        ticks = {start, last_tick}
        for index in range(1, probes_per_contact + 1):
            offset = ((end - start) // TIME_RESOLUTION_NS) * index // (
                probes_per_contact + 1
            )
            ticks.add(start + offset * TIME_RESOLUTION_NS)

        link = links[link_id]
        pat = pats[link_id]
        available = True
        contact_edges = []
        for at_ns in sorted(ticks):
            if at_ns == start:
                vectors = [snapshot["from_position_m"], snapshot["to_position_m"]]
            else:
                vectors = [
                    list(_vector(list(position(link[side], at_ns)), "ITRS position", 3))
                    for side in ("from", "to")
                ]
            direction = tuple(b - a for a, b in zip(*vectors))
            if (not all(math.isfinite(value) for value in direction)
                    or math.hypot(*direction) <= 1.0):
                raise ValueError(f"degenerate optical separation at {key} time {at_ns}")
            feasible = True
            for side in ("from", "to"):
                ray = direction if side == "from" else tuple(-value for value in direction)
                boresight = _boresight_at(heads[pat[f"{side}_head"]], at_ns)
                cosine = sum(a * b for a, b in zip(
                    _unit(ray, "optical line of sight"),
                    _unit(tuple(boresight), "optical boresight"),
                ))
                angle = math.degrees(math.acos(max(-1.0, min(1.0, cosine))))
                if angle > snapshot[f"{side}_max_off_axis_deg"] + ANGLE_TOLERANCE_DEG:
                    feasible = False
            if at_ns == start and not feasible:
                raise ValueError(f"optical contact starts outside pointing envelope: {key}")
            if feasible != available:
                event = {
                    "at_ns": at_ns, "link_id": link_id,
                    "action": "up" if feasible else "down",
                }
                events.append(event)
                contact_edges.append(event)
                available = feasible
        # Restore administrative availability at the geometric boundary so
        # the next independently evaluated contact can acquire normally.
        if not available and end < duration:
            event = {"at_ns": end, "link_id": link_id, "action": "up"}
            events.append(event)
            contact_edges.append(event)
        total_probes += len(ticks)
        reports.append({
            "link_id": link_id, "start_ns": start, "end_ns": end,
            "sample_count": len(ticks), "edges": contact_edges,
        })
        if len(events) > MAX_TRACKING_FAULTS:
            raise ValueError("sampled pointing fault event limit exceeded")
    events.sort(key=lambda event: (event["at_ns"], event["link_id"]))
    result = {**compiled, "faults": [*scenario.get("faults", []), *events]}
    return result, {
        "probes_per_contact": probes_per_contact,
        "contact_count": len(reports),
        "sample_count": total_probes,
        "derived_fault_count": len(events),
        "contacts": reports,
        "fidelity": "finite sampled pointing transitions; no continuous tracking guarantee",
    }
