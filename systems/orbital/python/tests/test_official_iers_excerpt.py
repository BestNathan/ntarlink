"""Regression using small, dated excerpts of published IERS finals2000A rows.

These source excerpts are not a complete authenticated IERS archive. The
independent GMST82/IAU-1980 reference is test-only, not an orbit solution.
"""

import hashlib
import math
import sys
import unittest
from datetime import datetime, timedelta, timezone
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(Path(__file__).resolve().parent))

from ntarlink_orbital.eop import build_timescale
from test_vallado_frame_reference import (
    ARCSEC_TO_RAD,
    independent_pef_to_itrf,
    independent_teme_to_pef,
    ut1_julian_date,
)

TEME_KM = (5094.18016210, 6127.64465050, 6380.34453270)


class OfficialIersExcerptTests(unittest.TestCase):
    def load(self, filename, epoch, allow_predictions=False):
        path = Path(__file__).resolve().parent / "data" / filename
        raw = path.read_bytes()
        config = {
            "schema_version": 1,
            "format": "iers-finals2000a",
            "path": str(path),
            "sha256": hashlib.sha256(raw).hexdigest(),
            "valid_until_utc": (epoch.replace(hour=0, minute=0, second=0,
                                              microsecond=0) + timedelta(days=1)).strftime(
                "%Y-%m-%dT%H:%M:%SZ"
            ),
            "allow_predictions": allow_predictions,
        }
        return build_timescale(config, epoch, 1_000_000_000)

    def test_observed_epochs_match_independent_gmst82_and_polar_motion(self):
        import numpy as np
        from skyfield.sgp4lib import TEME_to_ITRF

        for filename, epoch, values_today, values_tomorrow in (
            (
                "finals2000a-2004-04-05-to-07.txt",
                datetime(2004, 4, 6, 7, 51, 28, 386009, tzinfo=timezone.utc),
                (-0.140722, 0.333536, -0.4399498),
                (-0.140160, 0.336396, -0.4414071),
            ),
            (
                "finals2000a-2014-01-22-to-24.txt",
                datetime(2014, 1, 23, 11, 18, 7, tzinfo=timezone.utc),
                (0.024315, 0.339933, -0.1195153),
                (0.024443, 0.340741, -0.1207227),
            ),
        ):
            with self.subTest(epoch=epoch.isoformat()):
                ts, provenance = self.load(filename, epoch)
                instant = ts.from_datetime(epoch)
                fraction = (epoch - epoch.replace(hour=0, minute=0, second=0,
                                                  microsecond=0)).total_seconds() / 86400
                expected = tuple(a + (b - a) * fraction
                                 for a, b in zip(values_today, values_tomorrow))
                xp, yp, dut1 = expected
                self.assertAlmostEqual(instant.dut1, dut1, delta=0.0001)
                _, actual_xp, actual_yp = instant.polar_motion_angles()
                self.assertAlmostEqual(actual_xp, xp, delta=0.00001)
                self.assertAlmostEqual(actual_yp, yp, delta=0.00001)
                independent = independent_pef_to_itrf(
                    independent_teme_to_pef(TEME_KM, ut1_julian_date(epoch, dut1)),
                    xp, yp,
                )
                actual, _ = TEME_to_ITRF(
                    instant.ut1, np.array(TEME_KM), np.zeros(3),
                    actual_xp * ARCSEC_TO_RAD, actual_yp * ARCSEC_TO_RAD,
                )
                self.assertLess(math.dist(independent, actual) * 1000, 0.15)
                self.assertEqual(provenance["predicted_interpolation_days"], 0)
                self.assertEqual(provenance["sample_count"], 3)

    def test_official_2026_forecast_rows_require_explicit_opt_in(self):
        epoch = datetime(2026, 10, 23, 12, tzinfo=timezone.utc)
        filename = "finals2000a-2026-10-22-to-24.txt"
        with self.assertRaisesRegex(ValueError, "predicted EOP"):
            self.load(filename, epoch)
        ts, provenance = self.load(filename, epoch, allow_predictions=True)
        self.assertTrue(math.isfinite(ts.from_datetime(epoch).dut1))
        self.assertEqual(provenance["predicted_interpolation_days"], 2)
        self.assertEqual(provenance["prediction_policy"], "allow")


if __name__ == "__main__":
    unittest.main()
