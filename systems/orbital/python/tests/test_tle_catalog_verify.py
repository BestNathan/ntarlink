"""Manifest-bound TLE benchmark consumption and tamper rejection tests."""

import hashlib
import json
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from ntarlink_orbital.tle_catalog import import_catalog
from ntarlink_orbital.tle_catalog_verify import load_verified_catalog
from ntarlink_orbital.sgp4_scale_benchmark_suite import run_suite

LINE1 = "1 25544U 98067A   14020.93268519  .00009878  00000-0  18200-3 0  5082"
LINE2 = "2 25544  51.6498 109.4756 0003572  55.9686 274.8005 15.49815350868473"


class VerifiedCatalogTests(unittest.TestCase):
    def setUp(self):
        folder = tempfile.TemporaryDirectory()
        self.addCleanup(folder.cleanup)
        self.config_path = Path(folder.name) / "orbit.json"
        self.manifest_path = Path(folder.name) / "orbit.manifest.json"
        config, manifest = import_catalog(
            (LINE1 + "\n" + LINE2 + "\n").encode("ascii"),
            epoch_utc="2014-01-23T06:20:00Z",
            source_url="https://example.org/historical.tle",
            source_license="operator-asserted; not authenticated",
            retrieved_utc="2026-10-29T20:00:00Z",
        )
        self.config_path.write_text(json.dumps(config, sort_keys=True, indent=2) + "\n",
                                    encoding="utf-8")
        manifest["orbit_config_sha256"] = hashlib.sha256(
            self.config_path.read_bytes()).hexdigest()
        self.manifest_path.write_text(json.dumps(manifest), encoding="utf-8")

    def test_verifies_exact_bytes_and_preserves_catalog(self):
        catalog, digest = load_verified_catalog(self.config_path, self.manifest_path)
        self.assertEqual(len(catalog["satellites"]), 1)
        self.assertEqual(catalog["satellites"][0]["id"], "norad-25544")
        self.assertEqual(digest, hashlib.sha256(self.manifest_path.read_bytes()).hexdigest())

    def test_rejects_changed_config_and_manifest_before_worker(self):
        for target in ("config", "manifest"):
            with self.subTest(target=target):
                path = self.config_path if target == "config" else self.manifest_path
                original = path.read_bytes()
                path.write_bytes(original + b" ")
                with patch("ntarlink_orbital.sgp4_scale_benchmark_suite._run_worker") as worker:
                    with self.assertRaises(ValueError):
                        run_suite(self.config_path, 1, 2, 1, "unique", self.manifest_path)
                    worker.assert_not_called()
                path.write_bytes(original)

    def test_rejects_rehashed_invalid_tle_and_count(self):
        for mutation in ("bad_checksum", "wrong_count", "duplicate_keys"):
            with self.subTest(mutation=mutation):
                config = json.loads(self.config_path.read_text())
                manifest = json.loads(self.manifest_path.read_text())
                if mutation == "bad_checksum":
                    config["satellites"][0]["tle_line1"] = LINE1[:-1] + "9"
                if mutation == "wrong_count":
                    manifest["satellite_count"] = 2
                if mutation == "duplicate_keys":
                    self.config_path.write_text('{"epoch_utc":"x","epoch_utc":"y"}')
                else:
                    self.config_path.write_text(json.dumps(config))
                manifest["orbit_config_sha256"] = hashlib.sha256(
                    self.config_path.read_bytes()).hexdigest()
                self.manifest_path.write_text(json.dumps(manifest))
                with self.assertRaises(ValueError):
                    load_verified_catalog(self.config_path, self.manifest_path)
                self.setUp()

    def test_rejects_invalid_manifest_metadata_and_oversized_files(self):
        for field, value in (("generator", "other"), ("schema_version", True),
                             ("raw_tle_sha256", "not-a-digest"),
                             ("satellite_count", True)):
            with self.subTest(field=field):
                manifest = json.loads(self.manifest_path.read_text())
                manifest[field] = value
                self.manifest_path.write_text(json.dumps(manifest))
                with self.assertRaises(ValueError):
                    load_verified_catalog(self.config_path, self.manifest_path)
                self.setUp()
        self.manifest_path.write_bytes(b" " * 65_537)
        with self.assertRaises(ValueError):
            load_verified_catalog(self.config_path, self.manifest_path)

    def test_verified_unique_suite_worker_contract(self):
        _, digest = load_verified_catalog(self.config_path, self.manifest_path)
        result = {
            "workload": "supplied_unique_tle_sgp4_itrs_positions",
            "catalog_mode": "unique", "frame": "ITRS",
            "sample_count": 2, "satellite_count": 1, "samples_per_satellite": 2,
            "source_tle_count": 1, "unique_source_tle_count": 1,
            "source_config_sha256": "a" * 64,
            "position_digest_sha256": "b" * 64,
            "catalog_manifest_sha256": digest,
            "provider_build_wall_seconds": 0.1,
            "propagation_wall_seconds": 0.2,
            "propagation_samples_per_second": 10.0,
            "process_peak_rss_bytes": 2048,
            "rss_before_provider_bytes": 1024,
            "rss_after_provider_bytes": 1536,
            "rss_after_propagation_bytes": 1792,
        }
        with patch("ntarlink_orbital.sgp4_scale_benchmark_suite._run_worker",
                   return_value=result) as worker:
            report = run_suite(self.config_path, 1, 2, 2, "unique", self.manifest_path)
        self.assertEqual(report["catalog_manifest_sha256"], digest)
        self.assertEqual(worker.call_args.args[-1], self.manifest_path.resolve())
        with patch("ntarlink_orbital.sgp4_scale_benchmark_suite._run_worker",
                   return_value={**result, "catalog_manifest_sha256": "0" * 64}):
            with self.assertRaisesRegex(RuntimeError, "digest"):
                run_suite(self.config_path, 1, 2, 1, "unique", self.manifest_path)
        with self.assertRaisesRegex(ValueError, "unique"):
            run_suite(self.config_path, 1, 2, 1, "replicated", self.manifest_path)

    def test_real_iss_verified_unique_worker(self):
        try:
            import skyfield  # noqa: F401
        except ImportError:
            self.skipTest("Skyfield is unavailable in this local environment")
        report = run_suite(self.config_path, 1, 2, 1, "unique", self.manifest_path)
        self.assertEqual(report["sample_count"], 2)
        self.assertEqual(report["catalog_manifest_sha256"],
                         hashlib.sha256(self.manifest_path.read_bytes()).hexdigest())


if __name__ == "__main__":
    unittest.main()
