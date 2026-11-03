"""Ground contact opportunities from validated Earth-fixed ephemeris samples.

This is geometry-only: the caller must supply ECEF/ITRF satellite positions at
simulation-relative nanosecond timestamps. It neither propagates TLEs nor
establishes a radio link. No TEME coordinates may be passed without conversion.
"""
from dataclasses import dataclass
from math import dist, isfinite
from typing import Callable

from .geometry import elevation_deg, ground_ecef_m

PositionEcef = tuple[float, float, float]


@dataclass(frozen=True)
class ContactWindow:
    """Half-open visibility interval on the scenario's virtual-time axis."""

    start_ns: int
    end_ns: int


def predict_ground_contacts(
    position_ecef_m: Callable[[int], PositionEcef],
    *,
    start_ns: int,
    end_ns: int,
    latitude_deg: float,
    longitude_deg: float,
    height_m: float = 0.0,
    min_elevation_deg: float = 10.0,
    sample_step_ns: int = 1_000_000_000,
    refine_tolerance_ns: int = 1_000_000,
) -> list[ContactWindow]:
    """Estimate geometric ground visibility using a coarse scan and bisection.

    The position provider must use the same scenario epoch as the virtual-time
    offsets and return an Earth-fixed satellite position in meters. This method
    assumes no more than one visibility transition within any sample interval:
    shorter contacts can be missed and must be addressed by reducing the step.
    Edges are conservatively rounded inward to the requested tolerance.
    """
    for name, value in (("start_ns", start_ns), ("end_ns", end_ns),
                        ("sample_step_ns", sample_step_ns),
                        ("refine_tolerance_ns", refine_tolerance_ns)):
        if isinstance(value, bool) or not isinstance(value, int):
            raise ValueError(f"{name} must be an integer")
    if start_ns < 0 or end_ns <= start_ns:
        raise ValueError("contact time range must be nonnegative and nonempty")
    if sample_step_ns <= 0 or refine_tolerance_ns <= 0:
        raise ValueError("sample step and refinement tolerance must be positive")

    for name, value, minimum, maximum in (
        ("latitude_deg", latitude_deg, -90.0, 90.0),
        ("longitude_deg", longitude_deg, -180.0, 180.0),
        ("min_elevation_deg", min_elevation_deg, -90.0, 90.0),
    ):
        if not isfinite(value) or not minimum <= value <= maximum:
            raise ValueError(f"{name} must be finite and in [{minimum}, {maximum}]")
    if not isfinite(height_m):
        raise ValueError("height_m must be finite")

    station = ground_ecef_m(latitude_deg, longitude_deg, height_m)
    visibility: dict[int, bool] = {}

    def visible_at(t_ns: int) -> bool:
        if t_ns not in visibility:
            xyz = tuple(position_ecef_m(t_ns))
            if len(xyz) != 3 or not all(isfinite(v) for v in xyz):
                raise ValueError("position provider must return finite ECEF x/y/z in meters")
            if dist(xyz, station) == 0:
                raise ValueError("satellite and station cannot occupy the same point")
            visibility[t_ns] = elevation_deg(
                xyz, latitude_deg, longitude_deg, height_m
            ) >= min_elevation_deg
        return visibility[t_ns]

    def transition(left_ns: int, right_ns: int, left_visible: bool) -> int:
        """Return a conservative visible-side boundary within tolerance."""
        while right_ns - left_ns > refine_tolerance_ns:
            middle_ns = left_ns + (right_ns - left_ns) // 2
            if visible_at(middle_ns) == left_visible:
                left_ns = middle_ns
            else:
                right_ns = middle_ns
        return left_ns if left_visible else right_ns

    windows: list[ContactWindow] = []
    previous_ns = start_ns
    previous_visible = visible_at(start_ns)
    opened_ns: int | None = start_ns if previous_visible else None

    while previous_ns < end_ns:
        current_ns = min(end_ns, previous_ns + sample_step_ns)
        current_visible = visible_at(current_ns)
        if current_visible != previous_visible:
            boundary_ns = transition(previous_ns, current_ns, previous_visible)
            if current_visible:
                opened_ns = boundary_ns
            elif opened_ns is not None:
                if opened_ns < boundary_ns:
                    windows.append(ContactWindow(opened_ns, boundary_ns))
                opened_ns = None
        previous_ns, previous_visible = current_ns, current_visible

    if opened_ns is not None and opened_ns < end_ns:
        windows.append(ContactWindow(opened_ns, end_ns))
    return windows
