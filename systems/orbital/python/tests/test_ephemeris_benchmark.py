"""Contract and failure tests for the synthetic SYS-01 JSONL scale baseline."""

import json
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from ntarlink_orbital.ephemeris_benchmark import benchmark, main


class EphemerisBenchmarkTests(unittest.TestCase):
    def test_roundtrip_counts_bytes_and_finite_metrics(self):
        report = benchmark(3, 4)
        self.assertEqual(report["workload"], "synthetic_itrs_jsonl_roundtrip")
        self.assertEqual(report["sample_count"], 12)
        self.assertEqual(report["verified_sample_count"], 12)
        self.assertEqual(report["sample_step_ns"], 1_000_000)
        self.assertGreater(report["artifact_bytes"], 0)
        self.assertEqual(len(report["artifact_sha256"]), 64)
        for key in (
            "export_wall_seconds", "verified_read_wall_seconds",
            "export_samples_per_second", "verified_read_samples_per_second",
            "export_python_heap_peak_bytes", "verified_read_python_heap_peak_bytes",
        ):
            self.assertGreater(report[key], 0, key)

    def test_deterministic_artifact_digest_and_scaled_workload(self):
        small = benchmark(2, 3)
        repeat = benchmark(2, 3)
        larger = benchmark(4, 3)
        self.assertEqual(small["artifact_sha256"], repeat["artifact_sha256"])
        self.assertEqual(small["artifact_bytes"], repeat["artifact_bytes"])
        self.assertEqual(larger["sample_count"], 2 * small["sample_count"])
        self.assertGreater(larger["artifact_bytes"], small["artifact_bytes"])

    def test_rejects_invalid_and_over_budget_requests(self):
        for satellites, samples in ((0, 1), (1, 0), (True, 1), (1, False),
                                    (-1, 1), (1, -1), (501, 1000)):
            with self.subTest(satellites=satellites, samples=samples):
                with self.assertRaises(ValueError):
                    benchmark(satellites, samples)

    def test_cli_writes_machine_readable_report(self):
        with tempfile.TemporaryDirectory() as directory:
            report_path = Path(directory) / "report.json"
            main(["--satellites", "2", "--samples-per-satellite", "3",
                  "--report", str(report_path)])
            report = json.loads(report_path.read_text(encoding="utf-8"))
            self.assertEqual(report["verified_sample_count"], 6)
            self.assertEqual(report["satellite_count"], 2)


if __name__ == "__main__":
    unittest.main()
