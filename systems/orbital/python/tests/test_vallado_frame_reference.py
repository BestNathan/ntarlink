"""Independent published TEME/PEF/ITRF position-vector regression (2004 LEO).

Reference: Vallado/Seago/Seidelmann AAS-06-134, as reproduced by
Orekit 13.0 ITRFEquinoxProviderTest and Astropy 7.1.1 test_teme_itrf.
This test does not configure the production Skyfield timescale or claim
that its default built-in EOP is suitable for precision geodesy.
"""

import math
import unittest
from datetime import datetime, timezone

# AAS-06-134, 2004-04-06 07:51:28.386009 UTC. EOP from Orekit 13.0.
UTC = datetime(2004, 4, 6, 7, 51, 28, 386009, tzinfo=timezone.utc)
DUT1_S = -0.4399619
XP_ARCSEC = -0.140682
YP_ARCSEC = 0.333309
TEME_KM = (5094.18016210, 6127.64465050, 6380.34453270)
PEF_KM = (-1033.4750313, 7901.3055856, 6380.3445328)
ITRF_KM = (-1033.4793830, 7901.2952754, 6380.3565958)
ARCSEC_TO_RAD = math.pi / (180.0 * 3600.0)


def ut1_julian_date(utc, dut1_s):
    """Compute Julian UT1 directly from Gregorian UTC and a pinned DUT1."""
    seconds = (utc.hour * 3600 + utc.minute * 60 + utc.second
               + utc.microsecond / 1_000_000 + dut1_s)
    return utc.toordinal() + 1_721_424.5 + seconds / 86_400


def independent_teme_to_pef(teme_km, jd_ut1):
    """Vallado GMST82 rotation; test-only and independent of Skyfield."""
    t = (jd_ut1 - 2_451_545.0) / 36_525.0
    gmst_seconds = (67_310.54841
                    + (876_600 * 3600 + 8_640_184.812866) * t
                    + 0.093104 * t * t - 6.2e-6 * t * t * t)
    theta = (gmst_seconds * math.pi / 43_200.0) % (2 * math.pi)
    x, y, z = teme_km
    return (math.cos(theta) * x + math.sin(theta) * y,
            -math.sin(theta) * x + math.cos(theta) * y, z)


def independent_pef_to_itrf(pef_km, xp_arcsec, yp_arcsec):
    """Transpose of Vallado IAU-1980 polar-motion matrix (PEF->ITRF)."""
    xp, yp = xp_arcsec * ARCSEC_TO_RAD, yp_arcsec * ARCSEC_TO_RAD
    cx, sx, cy, sy = math.cos(xp), math.sin(xp), math.cos(yp), math.sin(yp)
    x, y, z = pef_km
    return (cx * x + sx * sy * y + sx * cy * z,
            cy * y - sy * z,
            -sx * x + cx * sy * y + cx * cy * z)


def distance_m(a, b):
    return math.dist(a, b) * 1000.0


class ValladoEopReferenceTests(unittest.TestCase):
    def test_independent_gmst82_matches_published_pef(self):
        pef = independent_teme_to_pef(TEME_KM, ut1_julian_date(UTC, DUT1_S))
        self.assertLess(distance_m(pef, PEF_KM), 0.03)

    def test_independent_polar_motion_matches_published_itrf(self):
        pef = independent_teme_to_pef(TEME_KM, ut1_julian_date(UTC, DUT1_S))
        itrf = independent_pef_to_itrf(pef, XP_ARCSEC, YP_ARCSEC)
        self.assertLess(distance_m(itrf, ITRF_KM), 0.03)

    def test_pinned_skyfield_teme_to_itrf_matches_external_reference(self):
        # Exercise the pinned dependency's separate transform path against
        # an external published vector, without adding Astropy/Orekit to CI.
        import numpy as np
        from skyfield.sgp4lib import TEME_to_ITRF

        jd_ut1 = ut1_julian_date(UTC, DUT1_S)
        actual, _ = TEME_to_ITRF(
            jd_ut1, np.array(TEME_KM), np.zeros(3),
            XP_ARCSEC * ARCSEC_TO_RAD, YP_ARCSEC * ARCSEC_TO_RAD,
        )
        self.assertLess(distance_m(actual, ITRF_KM), 0.03)

    def test_omitting_eop_is_detectably_wrong(self):
        correct_jd = ut1_julian_date(UTC, DUT1_S)
        correct_pef = independent_teme_to_pef(TEME_KM, correct_jd)
        correct_itrf = independent_pef_to_itrf(correct_pef, XP_ARCSEC, YP_ARCSEC)
        utc_as_ut1 = independent_teme_to_pef(TEME_KM, ut1_julian_date(UTC, 0.0))
        no_polar_motion = correct_pef
        self.assertGreater(distance_m(correct_itrf, utc_as_ut1), 100.0)
        self.assertGreater(distance_m(correct_itrf, no_polar_motion), 10.0)


if __name__ == "__main__":
    unittest.main()
