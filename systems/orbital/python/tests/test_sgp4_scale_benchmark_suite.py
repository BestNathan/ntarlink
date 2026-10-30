"""Contract, failure and production smoke tests for isolated SGP4 repetitions."""

import json
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from ntarlink_orbital.sgp4_scale_benchmark_suite import main, run_suite


SOURCE = {
    "epoch_utc": "2014-01-23T06:20:00Z",
    "satellites": [{"id": "iss", "tle_line1": "first", "tle_line2": "second"}],
}


def worker_result(digest="a" * 64):
    return {
        "workload": "repeated_historical_tle_sgp4_itrs_positions",
        "catalog_mode": "replicated",
        "frame": "ITRS",
        "sample_count": 6, "satellite_count": 2, "samples_per_satellite": 3,
        "source_tle_count": 1, "unique_source_tle_count": 1,
        "source_config_sha256": "b" * 64,
        "position_digest_sha256": digest,
        "provider_build_wall_seconds": 0.1,
        "propagation_wall_seconds": 0.2,
        "propagation_samples_per_second": 30.0,
        "process_peak_rss_bytes": 2048,
        "rss_before_provider_bytes": 1024,
        "rss_after_provider_bytes": 1536,
        "rss_after_propagation_bytes": 1792,
    }


class Sgp4ScaleBenchmarkSuiteTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.source = Path(self.directory.name) / "orbit.json"
        self.source.write_text(json.dumps(SOURCE), encoding="utf-8")

    def test_aggregates_consistent_isolated_results(self):
        with patch("ntarlink_orbital.sgp4_scale_benchmark_suite._run_worker",
                   side_effect=[worker_result(), {
                       **worker_result(), "propagation_samples_per_second": 60.0,
                       "process_peak_rss_bytes": 4096,
                   }]) as worker:
            report = run_suite(self.source, 2, 3, 2)
        self.assertEqual(worker.call_count, 2)
        self.assertEqual(report["sample_count"], 6)
        self.assertEqual(report["repetitions"], 2)
        self.assertEqual(report["median_propagation_samples_per_second"], 45.0)
        self.assertEqual(report["max_process_peak_rss_bytes"], 4096)
        self.assertEqual(report["max_rss_before_provider_bytes"], 1024)
        self.assertEqual(report["max_rss_after_provider_bytes"], 1536)
        self.assertEqual(report["max_rss_after_propagation_bytes"], 1792)
        self.assertEqual(report["position_digest_sha256"], "a" * 64)

    def test_invalid_workloads_fail_before_spawning(self):
        for args in ((0, 3, 1), (True, 3, 1), (2, False, 1),
                     (1001, 1, 1), (1000, 501, 1),
                     (2, 3, 0), (2, 3, 11)):
            with self.subTest(args=args), patch(
                "ntarlink_orbital.sgp4_scale_benchmark_suite._run_worker"
            ) as worker:
                with self.assertRaises(ValueError):
                    run_suite(self.source, *args)
                worker.assert_not_called()

    def test_rejects_invalid_catalog_before_spawning(self):
        for catalog in ({}, {"satellites": []}, {"satellites": [{}]}):
            self.source.write_text(json.dumps(catalog), encoding="utf-8")
            with patch("ntarlink_orbital.sgp4_scale_benchmark_suite._run_worker") as worker:
                with self.assertRaises(ValueError):
                    run_suite(self.source, 1, 2, 1)
                worker.assert_not_called()

    def test_rejects_changed_digest_and_nonfinite_metrics(self):
        for altered in (
            {**worker_result(), "position_digest_sha256": "c" * 64},
            {**worker_result(), "propagation_wall_seconds": float("nan")},
            {**worker_result(), "process_peak_rss_bytes": None},
            {**worker_result(), "position_digest_sha256": "invalid"},
            {**worker_result(), "rss_after_provider_bytes": None},
            {**worker_result(), "rss_after_provider_bytes": True},
            {**worker_result(), "rss_after_provider_bytes": -1},
            {**worker_result(), "rss_after_propagation_bytes": 4096},
        ):
            with self.subTest(altered=altered), patch(
                "ntarlink_orbital.sgp4_scale_benchmark_suite._run_worker",
                side_effect=[worker_result(), altered]
            ):
                with self.assertRaises(RuntimeError):
                    run_suite(self.source, 2, 3, 2)

    def test_cli_writes_summary(self):
        output = Path(self.directory.name) / "report.json"
        with patch("ntarlink_orbital.sgp4_scale_benchmark_suite._run_worker",
                   return_value=worker_result()):
            main([str(self.source), "--satellites", "2",
                  "--samples-per-satellite", "3", "--repetitions", "1",
                  "--report", str(output)])
        report = json.loads(output.read_text(encoding="utf-8"))
        self.assertEqual(report["sample_count"], 6)
        self.assertEqual(report["repetitions"], 1)
        self.assertEqual(report["max_rss_after_propagation_bytes"], 1792)

    def test_allows_unavailable_checkpoints_on_unsupported_platforms(self):
        unavailable = {**worker_result(),
                       "rss_before_provider_bytes": None,
                       "rss_after_provider_bytes": None,
                       "rss_after_propagation_bytes": None}
        with patch("ntarlink_orbital.sgp4_scale_benchmark_suite._run_worker",
                   return_value=unavailable):
            report = run_suite(self.source, 2, 3, 2)
        self.assertIsNone(report["max_rss_after_provider_bytes"])
        self.assertEqual(report["max_process_peak_rss_bytes"], 2048)

    def test_unique_mode_rejects_duplicates_before_worker(self):
        with patch("ntarlink_orbital.sgp4_scale_benchmark_suite._run_worker") as worker:
            with self.assertRaisesRegex(ValueError, "distinct TLE"):
                run_suite(self.source, 2, 3, 1, catalog_mode="unique")
            worker.assert_not_called()

    def test_unique_mode_checks_worker_contract(self):
        self.source.write_text(json.dumps({**SOURCE, "satellites": [
            SOURCE["satellites"][0],
            {"id": "other", "tle_line1": "other-first", "tle_line2": "other-second"},
        ]}), encoding="utf-8")
        unique = {**worker_result(),
                  "workload": "supplied_unique_tle_sgp4_itrs_positions",
                  "catalog_mode": "unique", "source_tle_count": 2,
                  "unique_source_tle_count": 2}
        with patch("ntarlink_orbital.sgp4_scale_benchmark_suite._run_worker",
                   return_value=unique) as worker:
            report = run_suite(self.source, 2, 3, 2, catalog_mode="unique")
        self.assertEqual(report["catalog_mode"], "unique")
        self.assertEqual(report["unique_source_tle_count"], 2)
        self.assertEqual(worker.call_args.args[-1], "unique")
        with patch("ntarlink_orbital.sgp4_scale_benchmark_suite._run_worker",
                   return_value={**unique, "unique_source_tle_count": 1}):
            with self.assertRaisesRegex(RuntimeError, "cardinality"):
                run_suite(self.source, 2, 3, 1, catalog_mode="unique")

    def test_real_historical_iss_small_workload(self):
        """Exercise the actual provider and subprocess boundary in installed CI."""
        try:
            import skyfield  # noqa: F401
        except ImportError:
            self.skipTest("Skyfield is unavailable in this local environment")
        source = Path(__file__).resolve().parents[4] / "scenarios/orbit-sources.sample.json"
        report = run_suite(source, 2, 2, 2)
        self.assertEqual(report["sample_count"], 4)
        self.assertEqual(report["repetitions"], 2)
        self.assertEqual(len(report["position_digest_sha256"]), 64)
        self.assertGreater(report["median_propagation_samples_per_second"], 0)
        if sys.platform == "linux":
            self.assertGreater(report["max_process_peak_rss_bytes"], 0)
            self.assertGreater(report["max_rss_before_provider_bytes"], 0)
            self.assertGreater(report["max_rss_after_provider_bytes"], 0)
            self.assertGreater(report["max_rss_after_propagation_bytes"], 0)
            self.assertLessEqual(report["max_rss_after_propagation_bytes"],
                                 report["max_process_peak_rss_bytes"])
        unique = run_suite(source, 1, 2, 2, catalog_mode="unique")
        self.assertEqual(unique["catalog_mode"], "unique")
        self.assertEqual(unique["unique_source_tle_count"], 1)
        self.assertEqual(unique["sample_count"], 2)


if __name__ == "__main__":
    unittest.main()
