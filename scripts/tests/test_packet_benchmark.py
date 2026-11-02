"""Contract tests for the packet-runtime benchmark harness."""
from __future__ import annotations

import copy
import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from scripts.benchmark_packet_runtime import (
    DEFAULT_SCENARIO,
    benchmark,
    canonical_bytes,
    make_workload,
    run_command,
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
        with patch("scripts.benchmark_packet_runtime.run_command",
                   return_value=(json.dumps(report), 42, 5, 6)) as runner:
            with patch("scripts.benchmark_packet_runtime.time.perf_counter_ns", side_effect=[10, 50]):
                elapsed, digest, observed, rss, user, system = run_once(
                    Path("/tmp/cli"), Path("/tmp/input"), 1,
                )
        self.assertEqual(elapsed, 40)
        self.assertEqual(digest, sha256(canonical_bytes(report)))
        self.assertEqual(observed, report)
        self.assertEqual((rss, user, system), (42, 5, 6))
        runner.assert_called_once()

    def test_benchmark_rejects_nondeterministic_repetitions(self) -> None:
        report = {"processed_events": 4, "injected": 1, "delivered": 1, "dropped": 0}
        with tempfile.TemporaryDirectory() as directory:
            binary = Path(directory) / "cli"
            binary.write_bytes(b"mock-binary")
            source = Path(directory) / "base.json"
            source.write_text(json.dumps(self.base), encoding="utf-8")
            with patch("scripts.benchmark_packet_runtime.run_once",
                       side_effect=[(10, "first", report, 10, 2, 1),
                                    (20, "second", report, 20, 3, 1)]):
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
                       side_effect=[(1_000, "stable", report, 99, 50, 10),
                                    (100, "stable", report, 40, 5, 2),
                                    (200, "stable", report, 60, 6, 3),
                                    (300, "stable", report, 50, 7, 4)]):
                summary = benchmark(binary, source, 1, 1, 1, 3)
        self.assertEqual(summary["wall_time_ns"], [100, 200, 300])
        self.assertEqual(summary["median_wall_time_ns"], 200)
        self.assertEqual(summary["min_wall_time_ns"], 100)
        self.assertEqual(summary["max_wall_time_ns"], 300)
        self.assertEqual(summary["sample_runs"], 3)
        self.assertEqual(summary["schema_version"], 2)
        self.assertEqual(summary["peak_rss_kib"], [40, 60, 50])
        self.assertEqual(summary["median_peak_rss_kib"], 50)
        self.assertEqual(summary["user_cpu_ns"], [5, 6, 7])
        self.assertEqual(summary["system_cpu_ns"], [2, 3, 4])
        self.assertEqual(summary["deterministic_report_sha256"], "stable")


    @unittest.skipUnless(sys.platform == "linux" and hasattr(os, "wait4"),
                         "per-child resource usage requires Linux wait4")
    def test_linux_process_resources_are_per_child(self) -> None:
        output, rss, user, system = run_command(
            [sys.executable, "-c", "print('packet-benchmark-child')"],
        )
        self.assertEqual(output.strip(), "packet-benchmark-child")
        self.assertIsInstance(rss, int)
        self.assertGreater(rss, 0)
        self.assertGreaterEqual(user, 0)
        self.assertGreaterEqual(system, 0)

    @unittest.skipUnless(sys.platform == "linux" and hasattr(os, "wait4"),
                         "per-child resource usage requires Linux wait4")
    def test_linux_child_failure_retains_exit_and_stderr(self) -> None:
        with self.assertRaises(subprocess.CalledProcessError) as caught:
            run_command([
                sys.executable, "-c",
                "import sys; print('expected failure', file=sys.stderr); sys.exit(7)",
            ])
        self.assertEqual(caught.exception.returncode, 7)
        self.assertIn("expected failure", caught.exception.stderr)

    def test_non_linux_fallback_does_not_invent_resource_metrics(self) -> None:
        fake_result = type("Completed", (), {"stdout": "ok"})()
        with patch("scripts.benchmark_packet_runtime.sys.platform", "darwin"):
            with patch("scripts.benchmark_packet_runtime.subprocess.run",
                       return_value=fake_result) as runner:
                self.assertEqual(
                    run_command(["example"]), ("ok", None, None, None),
                )
        runner.assert_called_once_with(
            ["example"], check=True, capture_output=True, text=True,
        )

    def test_benchmark_resource_metrics_unavailable_on_fallback(self) -> None:
        report = {"processed_events": 4, "injected": 1, "delivered": 1, "dropped": 0}
        with tempfile.TemporaryDirectory() as directory:
            binary = Path(directory) / "cli"
            binary.write_bytes(b"mock-binary")
            source = Path(directory) / "base.json"
            source.write_text(json.dumps(self.base), encoding="utf-8")
            with patch("scripts.benchmark_packet_runtime.run_once",
                       return_value=(100, "stable", report, None, None, None)):
                summary = benchmark(binary, source, 1, 1, 0, 2)
        self.assertEqual(summary["peak_rss_kib"], [None, None])
        self.assertIsNone(summary["median_peak_rss_kib"])
        self.assertEqual(summary["user_cpu_ns"], [None, None])
        self.assertEqual(summary["system_cpu_ns"], [None, None])


if __name__ == "__main__":
    unittest.main()
