"""Compile SGP4/ITRS and spacecraft attitude into Rust-native optical geometry.

All positions and boresights are ITRS observations at discrete virtual-time
instants. The Rust PAT planner holds each observation until the next sample;
this is not a continuous orbit or attitude propagator.
"""

from __future__ import annotations

import math

from .optical_geometry import (
    _boresight_at, _heads, _quaternion_at, _rotate_body_to_itrs, _unit, _vector,
)
from .optical_native_steering import _observation_schedule


def _body_frame_los(positions: list[list[float]], side: str,
                    head: dict, at_ns: int) -> tuple[float, ...]:
    """Transform an ITRS LOS into the spacecraft body frame using q inverse."""
    ray = tuple(b - a for a, b in zip(*positions))
    if side == "to":
        ray = tuple(-value for value in ray)
    unit_ray = _unit(ray, "ITRS line of sight")
    q = _quaternion_at(head, at_ns)
    inverse = (q[0], -q[1], -q[2], -q[3])
    return tuple(_rotate_body_to_itrs(unit_ray, inverse))


def _angular_distance_deg(a: tuple[float, ...],
                          b: tuple[float, ...]) -> float:
    """Stable small-angle separation of two body-frame unit vectors."""
    cross = (
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    )
    dot = sum(x * y for x, y in zip(a, b))
    return math.degrees(math.atan2(math.hypot(*cross), dot))



def _angular_velocity_deg_per_s(a: tuple[float, ...],
                                b: tuple[float, ...], delta_ns: int) -> tuple[float, ...]:
    """Sampled body-frame LOS rotation vector in degrees per virtual second.

    A 180-degree flip has no unique rotation axis and fails closed when the
    optional acceleration audit is enabled.
    """
    cross = (
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    )
    sine = math.hypot(*cross)
    dot = sum(x * y for x, y in zip(a, b))
    if dot < 0.0 and sine < 1e-12:
        raise ValueError("ambiguous 180-degree body-frame LOS rotation axis")
    if sine == 0.0:
        return (0.0, 0.0, 0.0)
    angle_deg = math.degrees(math.atan2(sine, dot))
    scale = angle_deg * (1_000_000_000.0 / delta_ns) / sine
    return tuple(value * scale for value in cross)


def compile_native_itrs(scenario: dict, compiled: dict, position,
                        probes_per_contact: int, attitude: dict,
                        max_body_los_rate_deg_per_s: float | None = None,
                        max_body_los_accel_deg_per_s2: float | None = None) -> tuple[dict, dict]:
    """Emit bounded ITRS geometry observations for every PAT link.

    Include a real t=0 observation even if the first contact starts later,
    plus each contact start, finite interior probes and the last representable
    microsecond before its exclusive end. Preserve administrative faults.
    """
    if max_body_los_rate_deg_per_s is not None:
        if (type(max_body_los_rate_deg_per_s) not in (int, float)
                or not math.isfinite(max_body_los_rate_deg_per_s)
                or max_body_los_rate_deg_per_s < 0):
            raise ValueError("max_body_los_rate_deg_per_s must be finite and nonnegative")
        max_body_los_rate_deg_per_s = float(max_body_los_rate_deg_per_s)
    if max_body_los_accel_deg_per_s2 is not None:
        if (type(max_body_los_accel_deg_per_s2) not in (int, float)
                or not math.isfinite(max_body_los_accel_deg_per_s2)
                or max_body_los_accel_deg_per_s2 < 0):
            raise ValueError("max_body_los_accel_deg_per_s2 must be finite and nonnegative")
        max_body_los_accel_deg_per_s2 = float(max_body_los_accel_deg_per_s2)
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
    peak_rates = {}
    peak_accelerations = {}
    for link_id in sorted(pats):
        link = links[link_id]
        pat = pats[link_id]
        previous_los = {}
        previous_velocity = {}
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
            if (max_body_los_rate_deg_per_s is not None
                    or max_body_los_accel_deg_per_s2 is not None):
                for side, head in (("from", a), ("to", b)):
                    head_id = pat[f"{side}_head"]
                    current = _body_frame_los(positions, side, head, at_ns)
                    if side in previous_los:
                        previous_at, previous_ray = previous_los[side]
                        rate = (_angular_distance_deg(previous_ray, current)
                                * 1_000_000_000.0 / (at_ns - previous_at))
                        if max_body_los_rate_deg_per_s is not None:
                            peak_rates[head_id] = max(peak_rates.get(head_id, 0.0), rate)
                            if (rate > max_body_los_rate_deg_per_s
                                    and not math.isclose(
                                        rate, max_body_los_rate_deg_per_s,
                                        rel_tol=1e-12, abs_tol=1e-9)):
                                raise ValueError(
                                    f"body-frame LOS rate exceeds max_body_los_rate_deg_per_s "
                                    f"for {head_id} at {at_ns} ns"
                                )
                        if max_body_los_accel_deg_per_s2 is not None:
                            span_ns = at_ns - previous_at
                            velocity = _angular_velocity_deg_per_s(
                                previous_ray, current, span_ns)
                            if side in previous_velocity:
                                prior_velocity, prior_span_ns = previous_velocity[side]
                                midpoint_interval_s = (span_ns + prior_span_ns) * 0.5e-9
                                acceleration = (math.dist(velocity, prior_velocity)
                                                / midpoint_interval_s)
                                peak_accelerations[head_id] = max(
                                    peak_accelerations[head_id], acceleration)
                                if (acceleration > max_body_los_accel_deg_per_s2
                                        and not math.isclose(
                                            acceleration, max_body_los_accel_deg_per_s2,
                                            rel_tol=1e-12, abs_tol=1e-6)):
                                    raise ValueError(
                                        "body-frame LOS acceleration exceeds "
                                        f"max_body_los_accel_deg_per_s2 for {head_id} "
                                        f"at {at_ns} ns"
                                    )
                            previous_velocity[side] = (velocity, span_ns)
                    else:
                        peak_rates.setdefault(head_id, 0.0)
                    if max_body_los_accel_deg_per_s2 is not None:
                        peak_accelerations.setdefault(head_id, 0.0)
                    previous_los[side] = (at_ns, current)
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
        "body_los_rate_audit": (
            {
                "max_body_los_rate_deg_per_s": max_body_los_rate_deg_per_s,
                "peak_sampled_rate_by_head_deg_per_s": [
                    {"head_id": head_id, "peak_rate_deg_per_s": peak_rates[head_id]}
                    for head_id in sorted(peak_rates)
                ],
                "reference_frame": "spacecraft_body",
                "fidelity": "finite sampled mean LOS rate; not gimbal acceleration or continuous tracking",
            }
            if max_body_los_rate_deg_per_s is not None else None
        ),
        "body_los_acceleration_audit": (
            {
                "max_body_los_accel_deg_per_s2": max_body_los_accel_deg_per_s2,
                "peak_sampled_acceleration_by_head_deg_per_s2": [
                    {"head_id": head_id,
                     "peak_acceleration_deg_per_s2": peak_accelerations[head_id]}
                    for head_id in sorted(peak_accelerations)
                ],
                "reference_frame": "spacecraft_body",
                "fidelity": "finite difference of sampled LOS rotation vectors at interval midpoints; not actuator dynamics or continuous tracking",
            }
            if max_body_los_accel_deg_per_s2 is not None else None
        ),
        "fidelity": "SGP4/ITRS and supplied body attitude; discrete sample-and-hold, not continuous tracking",
    }
