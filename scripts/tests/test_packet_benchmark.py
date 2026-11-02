"""Contract tests for the packet-runtime benchmark harness."""
from __future__ import annotations

import copy
import json
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from scripts.benchmark_packet_runtime import (
    DEFAULT_SCENARIO,
    benchmark,
    canonical_bytes,
    make_workload,
    run_once,
    sha256,
    validate_report,
)


class PacketBenchmarkTests(unittest.TestCase):
    def setUp(self) -> None:
        self.base = json.loads(DEFAULT_SCENARIO.read_text(encoding="utf-8"))

    def test_workload_scales_without_mutating_baseline(self) -> None:
        original = copy.deepcopy(self.base)
        scaled = make_workload(self.base, 1000, 1_000_000)
        self.assertEqual(self.base, original)
        self.assertEqual(scaled["flows"][0]["packet_count"], 1000)
        self.assertEqual(scaled["flows"][0]["interval_ns"], 1_000_000)
        self.assertGreater(scaled["duration_ns"], 999_000_000)
        self.assertEqual(scaled["links"], self.base["links"])
        self.assertEqual(scaled["nodes"], self.base["nodes"])

    def test_workload_rejects_invalid_or_overflowing_inputs(self) -> None:
        for count, interval in ((0, 1), (1_000_001, 1), (2, 0), (2, 2**64)):
            with self.subTest(count=count, interval=interval):
                with self.assertRaises(ValueError):
                    make_workload(self.base, count, interval)
        multiple_flows = copy.deepcopy(self.base)
        multiple_flows["flows"].append(copy.deepcopy(multiple_flows["flows"][0]))
        with self.assertRaisesRegex(ValueError, "exactly one flow"):
            make_workload(multiple_flows, 1, 1)

    def test_canonical_hash_ignores_object_key_order(self) -> None:
        self.assertEqual(sha256(canonical_bytes({"b": 1, "a": 2})),
                         sha256(canonical_bytes({"a": 2, "b": 1})))

    def test_report_rejects_missing_and_unfinished_packets(self) -> None:
        valid = {
            "injected": 2, "delivered": 1, "dropped": 1,
            "processed_events": 5,
            "packets": [{"status": "delivered"}, {"status": "dropped"}],
        }
        validate_report(valid, 2)
        for changed in (
            {"injected": 3},
            {"dropped": 0},
            {"processed_events": 0},
            {"packets": [{"status": "in_flight"}, {"status": "dropped"}]},
        ):
            with self.subTest(changed=changed):
                invalid = {**valid, **changed}
                with self.assertRaisesRegex(ValueError, "accounting"):
                    validate_report(invalid, 2)

    def test_run_once_checks_json_report_and_wall_time(self) -> None:
        report = {
            "injected": 1, "delivered": 1, "dropped": 0,
            "processed_events": 4, "packets": [{"status": "delivered"}],
        }
        fake_result = type("Completed", (), {"stdout": json.dumps(report)})()
        with patch("scripts.benchmark_packet_runtime.subprocess.run", return_value=fake_result) as runner:
            with patch("scripts.benchmark_packet_runtime.time.perf_counter_ns", side_effect=[10, 50]):
                elapsed, digest, observed = run_once(Path("/tmp/cli"), Path("/tmp/input"), 1)
        self.assertEqual(elapsed, 40)
        self.assertEqual(digest, sha256(canonical_bytes(report)))
        self.assertEqual(observed, report)
        runner.assert_called_once()

    def test_benchmark_rejects_nondeterministic_repetitions(self) -> None:
        report = {"processed_events": 4, "injected": 1, "delivered": 1, "dropped": 0}
        with tempfile.TemporaryDirectory() as directory:
            binary = Path(directory) / "cli"
            binary.write_bytes(b"mock-binary")
            source = Path(directory) / "base.json"
            source.write_text(json.dumps(self.base), encoding="utf-8")
            with patch("scripts.benchmark_packet_runtime.run_once",
                       side_effect=[(10, "first", report), (20, "second", report)]):
                with self.assertRaisesRegex(ValueError, "differs"):
                    benchmark(binary, source, 1, 1, 0, 2)

    def test_benchmark_summary_excludes_warmup(self) -> None:
        report = {"processed_events": 4, "injected": 1, "delivered": 1, "dropped": 0}
        with tempfile.TemporaryDirectory() as directory:
            binary = Path(directory) / "cli"
            binary.write_bytes(b"mock-binary")
            source = Path(directory) / "base.json"
            source.write_text(json.dumps(self.base), encoding="utf-8")
            with patch("scripts.benchmark_packet_runtime.run_once",
                       side_effect=[(1_000, "stable", report), (100, "stable", report),
                                    (200, "stable", report), (300, "stable", report)]):
                summary = benchmark(binary, source, 1, 1, 1, 3)
        self.assertEqual(summary["wall_time_ns"], [100, 200, 300])
        self.assertEqual(summary["median_wall_time_ns"], 200)
        self.assertEqual(summary["min_wall_time_ns"], 100)
        self.assertEqual(summary["max_wall_time_ns"], 300)
        self.assertEqual(summary["sample_runs"], 3)
        self.assertEqual(summary["deterministic_report_sha256"], "stable")


if __name__ == "__main__":
    unittest.main()
