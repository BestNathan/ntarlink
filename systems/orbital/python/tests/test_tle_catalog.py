"""Contract, failure and production-provider tests for offline TLE catalog intake."""

import hashlib
import importlib.util
import json
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from ntarlink_orbital.tle_catalog import import_catalog, main
from ntarlink_orbital.sgp4_scale_benchmark import benchmark

LINE1 = "1 25544U 98067A   14020.93268519  .00009878  00000-0  18200-3 0  5082"
LINE2 = "2 25544  51.6498 109.4756 0003572  55.9686 274.8005 15.49815350868473"
OPTIONS = {
    "epoch_utc": "2014-01-23T06:20:00Z",
    "source_url": "https://example.org/archived-tle.txt",
    "source_license": "operator-reported permission; not verified",
    "retrieved_utc": "2026-10-29T20:00:00Z",
}


def tle_bytes(*lines):
    return ("\n".join(lines) + "\n").encode("ascii")


def renumber(line, number):
    body = line[:2] + f"{number:05d}" + line[7:68]
    checksum = sum(int(c) if c.isdigit() else 1 if c == "-" else 0 for c in body) % 10
    return body + str(checksum)


class TleCatalogTests(unittest.TestCase):
    def test_import_iss_with_sha_and_epoch_provenance(self):
        raw = tle_bytes(LINE1, LINE2)
        config, manifest = import_catalog(raw, **OPTIONS)
        self.assertEqual(config["satellites"], [{
            "id": "norad-25544", "tle_line1": LINE1, "tle_line2": LINE2,
        }])
        self.assertEqual(config["epoch_utc"], OPTIONS["epoch_utc"])
        self.assertEqual(manifest["raw_tle_sha256"], hashlib.sha256(raw).hexdigest())
        self.assertEqual(manifest["satellite_count"], 1)
        self.assertTrue(manifest["first_tle_epoch_utc"].startswith("2014-01-20T"))
        self.assertIn("not independently authenticated", manifest["provenance_scope"])

    def test_named_two_satellite_catalog_works_with_unique_benchmark(self):
        raw = tle_bytes("0 HISTORICAL ISS", LINE1, LINE2,
                        "0 SYNTHETIC ID VARIANT", renumber(LINE1, 25545),
                        renumber(LINE2, 25545))
        config, manifest = import_catalog(raw, **OPTIONS)
        self.assertEqual(manifest["satellite_count"], 2)
        self.assertEqual([s["id"] for s in config["satellites"]],
                         ["norad-25544", "norad-25545"])
        with patch("ntarlink_orbital.sgp4_scale_benchmark.build_sgp4_provider",
                   return_value=lambda sat, at_ns: (1.0, 2.0, 3.0)):
            report = benchmark(config, 2, 2, catalog_mode="unique")
        self.assertEqual(report["sample_count"], 4)
        self.assertEqual(report["unique_source_tle_count"], 2)

    def test_invalid_tle_pairs_fail_closed(self):
        cases = [
            tle_bytes(LINE1[:-1] + "9", LINE2),
            tle_bytes(LINE1, LINE2[:-1] + "9"),
            tle_bytes(LINE1, renumber(LINE2, 25545)),
            tle_bytes(LINE1),
            tle_bytes("0 ", LINE1, LINE2),
            tle_bytes(LINE1, LINE2, LINE1, LINE2),
            tle_bytes(LINE2, LINE1),
            tle_bytes(LINE1.replace("14020.", "14999."), LINE2),
            tle_bytes("0 TEST", LINE1, LINE2, "0 TRAILING"),
            b"\xff",
            b"",
            b"x" * 1_000_001,
        ]
        for raw in cases:
            with self.subTest(raw=raw[:24]), self.assertRaises(ValueError):
                import_catalog(raw, **OPTIONS)

    def test_stale_epoch_and_metadata_fail_closed(self):
        invalid = [
            {"epoch_utc": "2026-10-29T00:00:00Z"},
            {"epoch_utc": "2014-01-23T06:20:00+00:00"},
            {"retrieved_utc": "invalid"},
            {"source_url": "http://example.org/tle"},
            {"source_url": "https://user:password@example.org/tle"},
            {"source_license": ""},
            {"max_tle_age_days": float("nan")},
            {"max_tle_age_days": True},
            {"max_tle_age_days": -1},
        ]
        for change in invalid:
            with self.subTest(change=change), self.assertRaises(ValueError):
                import_catalog(tle_bytes(LINE1, LINE2), **{**OPTIONS, **change})

    def test_cli_writes_digest_verified_compatible_config(self):
        with tempfile.TemporaryDirectory() as folder:
            source = Path(folder) / "source.tle"
            output = Path(folder) / "orbit.json"
            manifest = Path(folder) / "manifest.json"
            source.write_bytes(tle_bytes(LINE1, LINE2))
            main([str(source), "--epoch-utc", OPTIONS["epoch_utc"],
                  "--source-url", OPTIONS["source_url"],
                  "--source-license", OPTIONS["source_license"],
                  "--retrieved-utc", OPTIONS["retrieved_utc"],
                  "--out", str(output), "--manifest", str(manifest)])
            result = json.loads(output.read_text())
            provenance = json.loads(manifest.read_text())
            self.assertEqual(result["satellites"][0]["id"], "norad-25544")
            self.assertEqual(provenance["orbit_config_sha256"],
                             hashlib.sha256(output.read_bytes()).hexdigest())
            with self.assertRaisesRegex(ValueError, "distinct"):
                main([str(source), "--epoch-utc", OPTIONS["epoch_utc"],
                      "--source-url", OPTIONS["source_url"],
                      "--source-license", OPTIONS["source_license"],
                      "--retrieved-utc", OPTIONS["retrieved_utc"],
                      "--out", str(source), "--manifest", str(manifest)])

    @unittest.skipUnless(importlib.util.find_spec("skyfield"), "Skyfield not installed")
    def test_real_historical_iss_provider_from_imported_catalog(self):
        config, _ = import_catalog(tle_bytes(LINE1, LINE2), **OPTIONS)
        report = benchmark(config, 1, 2, catalog_mode="unique")
        self.assertEqual(report["sample_count"], 2)
        self.assertEqual(report["catalog_mode"], "unique")
        self.assertEqual(len(report["position_digest_sha256"]), 64)


if __name__ == "__main__":
    unittest.main()
