"""WGS84 ground geometry. This module is not an SGP4 orbit propagator."""
from math import atan2, cos, degrees, radians, sin, sqrt

A = 6378137.0
E2 = 6.69437999014e-3


def ground_ecef_m(latitude_deg: float, longitude_deg: float, height_m: float = 0.0):
    lat, lon = radians(latitude_deg), radians(longitude_deg)
    n = A / sqrt(1.0 - E2 * sin(lat) ** 2)
    return ((n + height_m) * cos(lat) * cos(lon),
            (n + height_m) * cos(lat) * sin(lon),
            (n * (1.0 - E2) + height_m) * sin(lat))


def elevation_deg(satellite_ecef_m, latitude_deg: float, longitude_deg: float, height_m=0.0):
    """Topocentric elevation in degrees, given an Earth-fixed satellite position."""
    origin = ground_ecef_m(latitude_deg, longitude_deg, height_m)
    lat, lon = radians(latitude_deg), radians(longitude_deg)
    east = (-sin(lon), cos(lon), 0.0)
    north = (-sin(lat) * cos(lon), -sin(lat) * sin(lon), cos(lat))
    up = (cos(lat) * cos(lon), cos(lat) * sin(lon), sin(lat))
    delta = [satellite_ecef_m[i] - origin[i] for i in range(3)]
    e = sum(delta[i] * east[i] for i in range(3))
    n = sum(delta[i] * north[i] for i in range(3))
    u = sum(delta[i] * up[i] for i in range(3))
    return degrees(atan2(u, sqrt(e * e + n * n)))


"""WGS84 line of sight and geometric propagation models.

Positions are Earth-centered Earth-fixed (ITRS/ECEF) meters, not raw SGP4 TEME.
"""
from math import isfinite, sqrt

EQUATORIAL_RADIUS_M = 6_378_137.0
POLAR_RADIUS_M = 6_356_752.314245
SPEED_OF_LIGHT_MPS = 299_792_458.0


def distance_m(a, b):
    return sqrt(sum((float(a[i]) - float(b[i])) ** 2 for i in range(3)))


def earth_occludes_segment(a, b, clearance_m=0.0):
    """Return whether the 3D segment intersects an inflated WGS84 ellipsoid.

    Tangency counts as occlusion. The clearance margin is a geometric
    approximation rather than an atmospheric/optical pointing model.
    """
    if not isfinite(clearance_m) or clearance_m < 0.0:
        raise ValueError("clearance_m must be finite and nonnegative")
    if any(not isfinite(float(point[i])) for point in (a, b) for i in range(3)):
        raise ValueError("segment endpoints must contain finite ECEF coordinates")
    radii = (
        EQUATORIAL_RADIUS_M + clearance_m,
        EQUATORIAL_RADIUS_M + clearance_m,
        POLAR_RADIUS_M + clearance_m,
    )
    p = [float(a[i]) / radii[i] for i in range(3)]
    d = [(float(b[i]) - float(a[i])) / radii[i] for i in range(3)]
    squared = sum(v * v for v in d)
    if squared == 0.0:
        return True
    t = max(0.0, min(1.0, -sum(p[i] * d[i] for i in range(3)) / squared))
    q = sum((p[i] + t * d[i]) ** 2 for i in range(3))
    return q <= 1.0


def geometric_delay_ns(range_m):
    if range_m <= 0:
        raise ValueError("geometric range must be positive")
    return max(1, round(range_m / SPEED_OF_LIGHT_MPS * 1_000_000_000))
