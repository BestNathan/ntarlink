"""Pinned offline EOP contract, interpolation and production frame integration.

The short finals2000A-formatted tables below are synthetic fixtures, NOT
official IERS releases. The 2004 central values are published Vallado EOP.
"""

import hashlib
import json
import math
import sys
import tempfile
import unittest
from datetime import datetime, timedelta, timezone
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from ntarlink_orbital.contacts import build_sgp4_provider
from ntarlink_orbital.eop import MJD_EPOCH, build_timescale


def finals_row(day, xp, yp, dut1):
    mjd = (day - MJD_EPOCH).days
    return (f"      {mjd:9.2f} I {xp:9.6f}{' ' * 9} "
            f"{yp:9.6f}{' ' * 9}  I{dut1:10.7f}\n")


class PinnedEopTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.day = datetime(2004, 4, 6, tzinfo=timezone.utc)
        self.rows = [
            (self.day - timedelta(days=1), -0.140682, 0.333309, -0.4399619),
            (self.day, -0.140682, 0.333309, -0.4399619),
            (self.day + timedelta(days=1), -0.140682, 0.333309, -0.4399619),
        ]

    def config(self, rows=None, until=None):
        raw = "".join(finals_row(*row) for row in (rows or self.rows)).encode()
        path = Path(self.temp.name) / "finals2000A.all"
        path.write_bytes(raw)
        return {
            "schema_version": 1,
            "format": "iers-finals2000a",
            "path": path.name,
            "sha256": hashlib.sha256(raw).hexdigest(),
            "valid_until_utc": (until or self.day + timedelta(days=1)).strftime(
                "%Y-%m-%dT%H:%M:%SZ"
            ),
        }

    def timescale(self, config=None, epoch=None, duration_ns=1_000_000_000):
        return build_timescale(
            config or self.config(), epoch or self.day, duration_ns,
            Path(self.temp.name),
        )

    def test_vallado_ut1_polar_motion_and_frame_reference(self):
        import numpy as np
        from skyfield.sgp4lib import TEME_to_ITRF

        ts, info = self.timescale()
        instant = ts.from_datetime(self.day + timedelta(
            hours=7, minutes=51, seconds=28, microseconds=386009
        ))
        self.assertAlmostEqual(instant.dut1, -0.4399619, delta=0.0001)
        _, xp, yp = instant.polar_motion_angles()
        self.assertAlmostEqual(xp, -0.140682, delta=0.000001)
        self.assertAlmostEqual(yp, 0.333309, delta=0.000001)
        actual, _ = TEME_to_ITRF(
            instant.ut1,
            np.array((5094.18016210, 6127.64465050, 6380.34453270)),
            np.zeros(3), xp * math.pi / 648000, yp * math.pi / 648000,
        )
        reference = (-1033.4793830, 7901.2952754, 6380.3565958)
        self.assertLess(math.dist(actual, reference) * 1000, 0.1)
        self.assertEqual(info["sample_count"], 3)
        self.assertEqual(info["mode"], "pinned_iers_finals2000a")

    def test_daily_interpolation_and_no_extrapolation(self):
        rows = [
            (self.day - timedelta(days=1), -0.1, 0.1, -0.4),
            (self.day, -0.2, 0.2, -0.2),
            (self.day + timedelta(days=1), -0.3, 0.3, 0.0),
        ]
        ts, _ = self.timescale(self.config(rows))
        instant = ts.from_datetime(self.day + timedelta(hours=12))
        self.assertAlmostEqual(instant.dut1, -0.1, delta=0.0001)
        _, xp, yp = instant.polar_motion_angles()
        self.assertAlmostEqual(xp, -0.25, delta=0.00001)
        self.assertAlmostEqual(yp, 0.25, delta=0.00001)
        with self.assertRaisesRegex(ValueError, "coverage"):
            self.timescale(self.config(rows, self.day + timedelta(days=2)),
                           self.day + timedelta(days=1, hours=1))

    def test_sha_expiry_gaps_and_invalid_values_fail_closed(self):
        bad_digest = self.config()
        bad_digest["sha256"] = "0" * 64
        with self.assertRaisesRegex(ValueError, "SHA-256 mismatch"):
            self.timescale(bad_digest)
        with self.assertRaisesRegex(ValueError, "expired"):
            self.timescale(self.config(until=self.day), self.day, 86_400_000_000_000)
        with self.assertRaisesRegex(ValueError, "consecutive"):
            self.timescale(self.config([self.rows[0], self.rows[2]]))
        with self.assertRaisesRegex(ValueError, "bounded"):
            invalid = list(self.rows)
            invalid[1] = (self.day, 3.0, 0.333309, -0.4399619)
            self.timescale(self.config(invalid))
        with self.assertRaisesRegex(ValueError, "version 1"):
            self.timescale({"schema_version": 2})
        with self.assertRaisesRegex(ValueError, "exceeds the last"):
            self.timescale(self.config(until=self.day + timedelta(days=2)))

    def test_sgp4_provider_uses_custom_timescale_and_records_provenance(self):
        root = Path(__file__).resolve().parents[4]
        config = json.loads((root / "scenarios/orbit-sources.sample.json").read_text())
        start = datetime(2014, 1, 23, tzinfo=timezone.utc)
        rows = [(start + timedelta(days=d), 0.3, -0.3, -0.4)
                for d in (-1, 0, 1)]
        config["earth_orientation"] = self.config(rows, start + timedelta(days=1))
        custom = build_sgp4_provider(config, 1_000_000_000, Path(self.temp.name))
        baseline = build_sgp4_provider(
            {k: v for k, v in config.items() if k != "earth_orientation"},
            1_000_000_000,
        )
        self.assertEqual(custom.earth_orientation["mode"], "pinned_iers_finals2000a")
        self.assertTrue(all(math.isfinite(x) for x in custom("iss-demo", 0)))
        self.assertGreater(math.dist(custom("iss-demo", 0), baseline("iss-demo", 0)), 0.1)
        self.assertEqual(baseline.earth_orientation["mode"], "skyfield_builtin")


if __name__ == "__main__":
    unittest.main()
