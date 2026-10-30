"""Compile SGP4/ITRS and spacecraft attitude into Rust-native optical geometry.

All positions and boresights are ITRS observations at discrete virtual-time
instants. The Rust PAT planner holds each observation until the next sample;
this is not a continuous orbit or attitude propagator.
"""

from __future__ import annotations

import math

from .optical_geometry import _boresight_at, _heads, _vector
from .optical_native_steering import _observation_schedule


def compile_native_itrs(scenario: dict, compiled: dict, position,
                        probes_per_contact: int, attitude: dict) -> tuple[dict, dict]:
    """Emit bounded ITRS geometry observations for every PAT link.

    Include a real t=0 observation even if the first contact starts later,
    plus each contact start, finite interior probes and the last representable
    microsecond before its exclusive end. Preserve administrative faults.
    """
    if "optical_geometry_samples" in scenario:
        raise ValueError("existing optical geometry samples cannot be overwritten")
    if any(fault.get("cause") == "sampled_pointing"
           for fault in scenario.get("faults", [])):
        raise ValueError("native ITRS cannot mix with sampled_pointing fault edges")
    if "optical_geometry" not in compiled:
        raise ValueError("missing validated optical geometry snapshots")

    heads = _heads(attitude, scenario["duration_ns"])
    links = {link["id"]: link for link in scenario["links"]}
    pats = {pat["link_id"]: pat for pat in scenario["optical_pat"]}
    ticks_by_link, reports = _observation_schedule(scenario, probes_per_contact)

    samples = []
    for link_id in sorted(pats):
        link = links[link_id]
        pat = pats[link_id]
        for at_ns in sorted(ticks_by_link[link_id]):
            positions = [
                list(_vector(list(position(link[side], at_ns)), "ITRS position", 3))
                for side in ("from", "to")
            ]
            separation = math.dist(*positions)
            if not math.isfinite(separation) or separation <= 1.0:
                raise ValueError(
                    f"degenerate optical separation at {at_ns} ns for {link_id}"
                )
            a = heads[pat["from_head"]]
            b = heads[pat["to_head"]]
            samples.append({
                "link_id": link_id, "at_ns": at_ns, "frame": "ITRS",
                "from_position_m": positions[0],
                "to_position_m": positions[1],
                "from_boresight_itrs": _boresight_at(a, at_ns),
                "to_boresight_itrs": _boresight_at(b, at_ns),
                "from_max_off_axis_deg": a["limit"],
                "to_max_off_axis_deg": b["limit"],
            })

    result = {key: value for key, value in compiled.items()
              if key != "optical_geometry"}
    result["optical_geometry_samples"] = samples
    return result, {
        "probes_per_contact": probes_per_contact,
        "contact_count": len(reports),
        "sample_count": len(samples),
        "contacts": reports,
        "fidelity": "SGP4/ITRS and supplied body attitude; discrete sample-and-hold, not continuous tracking",
    }
