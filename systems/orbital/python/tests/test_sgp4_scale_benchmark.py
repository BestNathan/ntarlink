"""Contract and failure tests for the bounded production-provider SGP4 benchmark."""

import json
import math
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from ntarlink_orbital.sgp4_scale_benchmark import _current_rss_bytes, benchmark, main


SOURCE = {
    "epoch_utc": "2014-01-23T06:20:00Z",
    "satellites": [{"id": "source", "tle_line1": "first", "tle_line2": "second"}],
}


class Sgp4ScaleBenchmarkTests(unittest.TestCase):
    @patch("ntarlink_orbital.sgp4_scale_benchmark.build_sgp4_provider")
    def test_provider_calls_and_reproducible_digest(self, provider):
        provider.side_effect = lambda config, duration, directory: (
            lambda satellite, at_ns: (6_800_000.0 + at_ns / 1e9,
                                      float(int(satellite[-5:])), 0.0)
        )
        first = benchmark(SOURCE, 2, 3)
        second = benchmark(SOURCE, 2, 3)
        self.assertEqual(first["sample_count"], 6)
        self.assertEqual(first["position_digest_sha256"], second["position_digest_sha256"])
        self.assertEqual(first["source_config_sha256"], second["source_config_sha256"])
        self.assertEqual(first["frame"], "ITRS")
        self.assertGreater(first["propagation_samples_per_second"], 0)
        self.assertEqual(provider.call_count, 2)
        config, duration, _ = provider.call_args.args
        self.assertEqual(duration, 3_000_000_000)
        self.assertEqual([s["id"] for s in config["satellites"]],
                         ["benchmark-sat-00000", "benchmark-sat-00001"])
        self.assertEqual(config["satellites"][1]["tle_line1"], "first")

    @patch("ntarlink_orbital.sgp4_scale_benchmark.build_sgp4_provider")
    def test_invalid_requests_do_not_build_provider(self, provider):
        for catalog, satellites, samples in (
            (SOURCE, 0, 1), (SOURCE, True, 1), (SOURCE, 1, False),
            (SOURCE, 1001, 1), (SOURCE, 1000, 501),
            ({**SOURCE, "satellites": []}, 1, 1),
            ({**SOURCE, "satellites": [{}]}, 1, 1),
        ):
            with self.subTest(satellites=satellites, samples=samples):
                with self.assertRaises(ValueError):
                    benchmark(catalog, satellites, samples)
        provider.assert_not_called()

    @patch("ntarlink_orbital.sgp4_scale_benchmark.build_sgp4_provider")
    def test_bad_position_fails_closed(self, provider):
        for vector in ((math.nan, 0, 0), (1, 2), (True, 2, 3)):
            with self.subTest(vector=vector):
                provider.return_value = lambda satellite, at_ns: vector
                with self.assertRaisesRegex(ValueError, "invalid ITRS position"):
                    benchmark(SOURCE, 1, 1)

    @patch("ntarlink_orbital.sgp4_scale_benchmark.build_sgp4_provider")
    def test_unique_catalog_uses_distinct_supplied_tles(self, provider):
        provider.return_value = lambda satellite, at_ns: (1.0, 2.0, 3.0)
        sources = {**SOURCE, "satellites": [
            SOURCE["satellites"][0],
            {"id": "second", "tle_line1": "different-first", "tle_line2": "different-second"},
        ]}
        report = benchmark(sources, 2, 3, catalog_mode="unique")
        self.assertEqual(report["catalog_mode"], "unique")
        self.assertEqual(report["unique_source_tle_count"], 2)
        self.assertEqual(report["source_tle_count"], 2)
        self.assertEqual(report["workload"], "supplied_unique_tle_sgp4_itrs_positions")
        config = provider.call_args.args[0]
        self.assertEqual([x["tle_line1"] for x in config["satellites"]],
                         ["first", "different-first"])
        for invalid, count in ((SOURCE, 2), (sources, 1), (
            {**SOURCE, "satellites": [SOURCE["satellites"][0]] * 2}, 2)):
            with self.subTest(invalid=invalid, count=count):
                with self.assertRaisesRegex(ValueError, "distinct TLE"):
                    benchmark(invalid, count, 1, catalog_mode="unique")
        with self.assertRaisesRegex(ValueError, "catalog_mode"):
            benchmark(SOURCE, 1, 1, catalog_mode="invalid")

    @patch("ntarlink_orbital.sgp4_scale_benchmark.build_sgp4_provider")
    @patch("ntarlink_orbital.sgp4_scale_benchmark._current_rss_bytes",
           side_effect=[1024, 2048, 3072])
    def test_samples_rss_at_phase_boundaries(self, rss, provider):
        provider.return_value = lambda satellite, at_ns: (1.0, 2.0, 3.0)
        report = benchmark(SOURCE, 1, 2)
        self.assertEqual(rss.call_count, 3)
        self.assertEqual(
            [report[key] for key in (
                "rss_before_provider_bytes", "rss_after_provider_bytes",
                "rss_after_propagation_bytes",
            )], [1024, 2048, 3072]
        )

    def test_linux_current_rss_parser_fails_closed(self):
        with patch("ntarlink_orbital.sgp4_scale_benchmark.sys.platform", "linux"):
            with patch("ntarlink_orbital.sgp4_scale_benchmark.Path.read_text",
                       return_value="Name:\\tpython\\nVmRSS:\\t42 kB\\n"):
                self.assertEqual(_current_rss_bytes(), 42 * 1024)
            for content in ("VmRSS: 42 MB", "VmRSS: nonsense kB",
                            "VmRSS: -1 kB", "Name: python"):
                with self.subTest(content=content), patch(
                    "ntarlink_orbital.sgp4_scale_benchmark.Path.read_text",
                    return_value=content
                ):
                    self.assertIsNone(_current_rss_bytes())
            with patch("ntarlink_orbital.sgp4_scale_benchmark.Path.read_text",
                       side_effect=OSError):
                self.assertIsNone(_current_rss_bytes())
        with patch("ntarlink_orbital.sgp4_scale_benchmark.sys.platform", "darwin"):
            self.assertIsNone(_current_rss_bytes())

    @patch("ntarlink_orbital.sgp4_scale_benchmark.build_sgp4_provider")
    def test_cli_writes_report(self, provider):
        provider.return_value = lambda satellite, at_ns: (1.0, 2.0, 3.0)
        with tempfile.TemporaryDirectory() as directory:
            source = Path(directory) / "orbit.json"
            output = Path(directory) / "result.json"
            source.write_text(json.dumps(SOURCE), encoding="utf-8")
            main([str(source), "--satellites", "1", "--samples-per-satellite", "2",
                  "--report", str(output)])
            self.assertEqual(json.loads(output.read_text())["sample_count"], 2)


if __name__ == "__main__":
    unittest.main()
