"""Production SGP4/ITRS cross-check against an independent Vallado frame path.

The 2014 EOP rows are fixed excerpts of IERS finals2000A, not an authenticated
full release. The orbit is the historical ISS demonstration TLE, not orbit truth.
"""

import hashlib
import json
import math
import sys
import unittest
from datetime import datetime, timezone
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(Path(__file__).resolve().parent))

from ntarlink_orbital.contacts import build_sgp4_provider
from test_vallado_frame_reference import (
    independent_pef_to_itrf,
    independent_teme_to_pef,
    ut1_julian_date,
)

UTC = timezone.utc
DAY_VALUES = (0.024315, 0.339933, -0.1195153)
NEXT_DAY_VALUES = (0.024443, 0.340741, -0.1207227)
OFFICIAL_EXCERPT_SHA256 = "79af897ec4e9b1d72b3d4d3e95919ed4d824452ed862a4ca5cfa4b78e964e89e"


class ProductionFrameMultiInstantTests(unittest.TestCase):
    def test_official_eop_and_raw_sgp4_match_independent_itrf_across_day(self):
        from sgp4.api import Satrec, jday

        root = Path(__file__).resolve().parents[4]
        config = json.loads((root / "scenarios/orbit-sources.sample.json").read_text())
        excerpt = Path(__file__).resolve().parent / "data/finals2000a-2014-01-22-to-24.txt"
        self.assertEqual(hashlib.sha256(excerpt.read_bytes()).hexdigest(),
                         OFFICIAL_EXCERPT_SHA256)
        config["earth_orientation"] = {
            "schema_version": 1,
            "format": "iers-finals2000a",
            "path": str(excerpt),
            "sha256": OFFICIAL_EXCERPT_SHA256,
            "valid_until_utc": "2014-01-24T00:00:00Z",
        }
        start = datetime(2014, 1, 23, 6, 20, tzinfo=UTC)
        targets = (
            start,
            datetime(2014, 1, 23, 11, 18, 7, tzinfo=UTC),
            datetime(2014, 1, 23, 20, 0, tzinfo=UTC),
        )
        duration_ns = int((targets[-1] - start).total_seconds()) * 1_000_000_000 + 1
        provider = build_sgp4_provider(config, duration_ns)
        self.assertEqual(provider.earth_orientation["sha256"], OFFICIAL_EXCERPT_SHA256)
        self.assertEqual(provider.earth_orientation["predicted_interpolation_days"], 0)
        satellite = config["satellites"][0]
        raw_sgp4 = Satrec.twoline2rv(satellite["tle_line1"], satellite["tle_line2"])
        for instant in targets:
            with self.subTest(instant=instant.isoformat()):
                fraction = ((instant - instant.replace(hour=0, minute=0, second=0,
                                                       microsecond=0)).total_seconds()
                            / 86400.0)
                xp, yp, dut1 = (a + (b - a) * fraction
                                for a, b in zip(DAY_VALUES, NEXT_DAY_VALUES))
                jd, jd_fraction = jday(instant.year, instant.month, instant.day,
                                      instant.hour, instant.minute,
                                      instant.second + instant.microsecond / 1e6)
                error, teme_km, _ = raw_sgp4.sgp4(jd, jd_fraction)
                self.assertEqual(error, 0)
                expected_km = independent_pef_to_itrf(
                    independent_teme_to_pef(teme_km, ut1_julian_date(instant, dut1)),
                    xp, yp,
                )
                offset_ns = int((instant - start).total_seconds()) * 1_000_000_000
                actual_m = provider("iss-demo", offset_ns)
                self.assertLess(math.dist(actual_m, [x * 1000 for x in expected_km]),
                                0.5)
                # Negative controls detect missing UT1 and polar motion.
                wrong_ut1 = independent_pef_to_itrf(
                    independent_teme_to_pef(teme_km, ut1_julian_date(instant, 0)),
                    xp, yp,
                )
                no_polar = independent_teme_to_pef(
                    teme_km, ut1_julian_date(instant, dut1))
                self.assertGreater(math.dist(actual_m, [x * 1000 for x in wrong_ut1]), 10)
                self.assertGreater(math.dist(actual_m, [x * 1000 for x in no_polar]), 5)


if __name__ == "__main__":
    unittest.main()
