"""Contract and failure tests for the bounded production-provider SGP4 benchmark."""

import json
import math
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from ntarlink_orbital.sgp4_scale_benchmark import benchmark, main


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
