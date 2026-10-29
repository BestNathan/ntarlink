"""Published historical reference regression for the SYS-01 frame pipeline.

The expected values are from Skyfield's Earth Satellites documentation,
https://rhodesmill.org/skyfield/earth-satellites.html, using the same 2014
ISS TLE. They are *external published output*, not independent Earth-orientation
(EOP) data or a separately implemented TEME->ITRF transform.
"""
import json
import math
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from ntarlink_orbital.contacts import build_sgp4_provider
from ntarlink_orbital.geometry import distance_m, elevation_deg, ground_ecef_m

# Skyfield guide: 2014-01-23 11:18:07 UTC, ISS 2014 historical TLE.
REFERENCE_EPOCH_UTC = "2014-01-23T11:18:07Z"
REFERENCE_GCRS_KM = (-3918.87650458, -1887.64838745, 5209.08801512)
REFERENCE_GEODETIC_DEG = (
    50 + 14 / 60 + 37.4 / 3600,
    -(86 + 23 / 60 + 23.3 / 3600),
)
REFERENCE_BLUFFTON_ALT_DEG = 16 + 16 / 60 + 32.6 / 3600
REFERENCE_BLUFFTON_RANGE_KM = 1168.7

# WGS84 reference constants; independent of the ntarlink geometry module.
WGS84_A_M = 6_378_137.0
WGS84_INV_FLATTENING = 298.257223563


def independent_wgs84_subpoint(position_m):
    """Bowring's direct ECEF->geodetic inversion (radians to degrees).

    This test-only implementation does not call Skyfield's geodetic conversion
    or ntarlink's topocentric geometry helpers.
    """
    x, y, z = position_m
    a = WGS84_A_M
    f = 1.0 / WGS84_INV_FLATTENING
    b = a * (1.0 - f)
    e2 = f * (2.0 - f)
    ep2 = (a * a - b * b) / (b * b)
    p = math.hypot(x, y)
    theta = math.atan2(z * a, p * b)
    latitude = math.atan2(
        z + ep2 * b * math.sin(theta) ** 3,
        p - e2 * a * math.cos(theta) ** 3,
    )
    return math.degrees(latitude), math.degrees(math.atan2(y, x))


class PublishedFrameReferenceTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        root = Path(__file__).resolve().parents[4]
        cls.config = json.loads(
            (root / "scenarios/orbit-sources.sample.json").read_text(encoding="utf-8")
        )
        cls.config["epoch_utc"] = REFERENCE_EPOCH_UTC

    def test_published_gcrs_position_matches_pinned_tle_and_sgp4(self):
        # This checks the pinned TLE/SGP4 state against an external published
        # GCRS vector; it does not by itself validate the Earth-fixed transform.
        from skyfield.api import EarthSatellite, load

        entry = self.config["satellites"][0]
        ts = load.timescale(builtin=True)
        satellite = EarthSatellite(
            entry["tle_line1"], entry["tle_line2"], entry["id"], ts
        )
        position_km = satellite.at(ts.utc(2014, 1, 23, 11, 18, 7)).xyz.km
        for actual, expected in zip(position_km, REFERENCE_GCRS_KM):
            self.assertAlmostEqual(actual, expected, delta=0.1)

    def test_provider_itrs_position_matches_published_geodetic_and_topocentric(self):
        # The production provider must transform TEME to ITRS before returning
        # positions. Compare independent WGS84 inversion and the ntarlink
        # topocentric geometry against separately published guide values.
        provider = build_sgp4_provider(self.config, 1_000_000_000)
        itrs_m = provider("iss-demo", 0)
        self.assertTrue(all(math.isfinite(x) for x in itrs_m))
        latitude, longitude = independent_wgs84_subpoint(itrs_m)
        self.assertAlmostEqual(latitude, REFERENCE_GEODETIC_DEG[0], delta=0.005)
        self.assertAlmostEqual(longitude, REFERENCE_GEODETIC_DEG[1], delta=0.005)

        bluffton = ground_ecef_m(40.8939, -83.8917)
        range_km = distance_m(itrs_m, bluffton) / 1000.0
        altitude_deg = elevation_deg(itrs_m, 40.8939, -83.8917)
        self.assertAlmostEqual(range_km, REFERENCE_BLUFFTON_RANGE_KM, delta=1.0)
        self.assertAlmostEqual(altitude_deg, REFERENCE_BLUFFTON_ALT_DEG, delta=0.03)

    def test_raw_teme_is_not_a_valid_earth_fixed_subpoint(self):
        # A guard against the most damaging frame mistake: raw SGP4 TEME
        # coordinates must not accidentally pass the published ITRS check.
        from sgp4.api import Satrec, jday

        entry = self.config["satellites"][0]
        satrec = Satrec.twoline2rv(entry["tle_line1"], entry["tle_line2"])
        jd, fraction = jday(2014, 1, 23, 11, 18, 7)
        error, teme_km, _velocity_kmps = satrec.sgp4(jd, fraction)
        self.assertEqual(error, 0)
        _lat, wrong_longitude = independent_wgs84_subpoint(
            tuple(value * 1000.0 for value in teme_km)
        )
        self.assertGreater(abs(wrong_longitude - REFERENCE_GEODETIC_DEG[1]), 5.0)


if __name__ == "__main__":
    unittest.main()
