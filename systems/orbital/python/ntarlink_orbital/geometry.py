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
