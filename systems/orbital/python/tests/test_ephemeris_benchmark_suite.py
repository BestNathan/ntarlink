"""Regression tests for isolated synthetic JSONL benchmark repetitions."""

import json
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from ntarlink_orbital.ephemeris_benchmark_suite import main, run_suite


class EphemerisBenchmarkSuiteTests(unittest.TestCase):
    def test_isolated_roundtrips_are_reproducible(self):
        report = run_suite(2, 3, repetitions=2)
        self.assertEqual(report["sample_count"], 6)
        self.assertEqual(report["repetitions"], 2)
        self.assertEqual(len(report["runs"]), 2)
        self.assertEqual(len(report["artifact_sha256"]), 64)
        self.assertGreater(report["median_export_samples_per_second"], 0)
        self.assertGreater(report["median_verified_read_samples_per_second"], 0)
        for run in report["runs"]:
            self.assertEqual(run["verified_sample_count"], 6)
            self.assertEqual(run["artifact_sha256"], report["artifact_sha256"])
        if sys.platform == "linux":
            self.assertGreater(report["max_process_peak_rss_bytes"], 0)

    def test_invalid_workloads_fail_before_spawning(self):
        for args in ((0, 2, 1), (2, 0, 1), (2, 3, 0), (2, 3, 11),
                     (True, 3, 1), (501, 1000, 1)):
            with self.subTest(args=args), patch(
                "ntarlink_orbital.ephemeris_benchmark_suite._run_worker"
            ) as worker:
                with self.assertRaises(ValueError):
                    run_suite(*args)
                worker.assert_not_called()

    def test_rejects_inconsistent_worker_results(self):
        valid = {
            "sample_count": 6, "verified_sample_count": 6,
            "satellite_count": 2, "samples_per_satellite": 3,
            "artifact_sha256": "a" * 64, "artifact_bytes": 100,
            "export_wall_seconds": 1, "verified_read_wall_seconds": 1,
            "export_samples_per_second": 6, "verified_read_samples_per_second": 6,
            "export_python_heap_peak_bytes": 20,
            "verified_read_python_heap_peak_bytes": 20,
            "process_peak_rss_bytes": 1024,
        }
        tampered = {**valid, "artifact_sha256": "b" * 64}
        with patch("ntarlink_orbital.ephemeris_benchmark_suite._run_worker",
                   side_effect=[valid, tampered]):
            with self.assertRaisesRegex(RuntimeError, "deterministic artifact"):
                run_suite(2, 3, 2)

    def test_cli_writes_summary(self):
        with tempfile.TemporaryDirectory() as directory:
            report_path = Path(directory) / "suite.json"
            main(["--satellites", "1", "--samples-per-satellite", "2",
                  "--repetitions", "2", "--report", str(report_path)])
            report = json.loads(report_path.read_text(encoding="utf-8"))
            self.assertEqual(report["repetitions"], 2)
            self.assertEqual(report["sample_count"], 2)


if __name__ == "__main__":
    unittest.main()
