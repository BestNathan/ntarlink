"""Contract, failure and reproducibility tests for offline ephemeris exports."""

import hashlib
import json
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from ntarlink_orbital.ephemeris import compile_ephemeris, main


class EphemerisTests(unittest.TestCase):
    def setUp(self):
        self.scenario = {
            "schema_version": 1,
            "duration_ns": 2_500_000_000,
            "nodes": [
                {"id": "sat-b", "kind": "satellite"},
                {"id": "ground", "kind": "terminal"},
                {"id": "sat-a", "kind": "satellite"},
            ],
        }
        self.config = {
            "epoch_utc": "2014-01-23T06:20:00Z",
            "satellites": [{"id": "sat-a"}, {"id": "sat-b"}],
        }
        self.calls = []

    def position(self, satellite_id, at_ns):
        self.calls.append((satellite_id, at_ns))
        return (6_800_000.0 + at_ns / 1_000_000_000, 1.0 if satellite_id == "sat-a" else 2.0, 0.0)

    def test_sorted_satellites_half_open_grid_and_units(self):
        doc = compile_ephemeris(self.scenario, self.config, self.position, 1_000_000_000)
        self.assertEqual(doc["sample_count"], 6)
        self.assertEqual((doc["frame"], doc["time_scale"], doc["position_unit"]), ("ITRS", "UTC", "m"))
        self.assertEqual([s["id"] for s in doc["satellites"]], ["sat-a", "sat-b"])
        self.assertEqual([s["at_ns"] for s in doc["satellites"][0]["samples"]],
                         [0, 1_000_000_000, 2_000_000_000])
        self.assertEqual(doc["satellites"][0]["samples"][1]["position_m"],
                         [6_800_001.0, 1.0, 0.0])
        self.assertEqual(len(self.calls), 6)
        self.assertEqual(doc, compile_ephemeris(self.scenario, self.config, self.position, 1_000_000_000))

    def test_short_horizon_is_sampled_once_at_zero(self):
        self.scenario["duration_ns"] = 1
        doc = compile_ephemeris(self.scenario, self.config, self.position, 1_000)
        self.assertEqual(doc["sample_count"], 2)
        self.assertEqual([s["at_ns"] for s in doc["satellites"][0]["samples"]], [0])

    def test_invalid_inputs_fail_before_position_calls(self):
        for step in (0, -1, 1, 1001, True, 1.5, "1000"):
            with self.subTest(step=step), self.assertRaisesRegex(ValueError, "step_ns"):
                compile_ephemeris(self.scenario, self.config, self.position, step)
        for duration in (0, -1, True, 1.5, "5000"):
            with self.subTest(duration=duration), self.assertRaisesRegex(ValueError, "duration_ns"):
                compile_ephemeris({**self.scenario, "duration_ns": duration}, self.config,
                                  self.position, 1000)
        with self.assertRaisesRegex(ValueError, "schema_version"):
            compile_ephemeris({**self.scenario, "schema_version": 2}, self.config,
                              self.position, 1000)
        with self.assertRaisesRegex(ValueError, "missing TLE"):
            compile_ephemeris(self.scenario, {**self.config, "satellites": [{"id": "sat-a"}]},
                              self.position, 1000)
        with self.assertRaisesRegex(ValueError, "duplicate scenario"):
            compile_ephemeris({**self.scenario, "nodes": [{"id": "sat-a", "kind": "satellite"}] * 2},
                              self.config, self.position, 1000)
        with self.assertRaisesRegex(ValueError, "duplicate orbital"):
            compile_ephemeris(self.scenario, {**self.config, "satellites": [{"id": "sat-a"}] * 2},
                              self.position, 1000)
        with self.assertRaisesRegex(ValueError, "named satellites"):
            compile_ephemeris({**self.scenario, "nodes": []}, self.config, self.position, 1000)
        self.assertEqual(self.calls, [])

    def test_global_sample_budget_rejects_before_propagation(self):
        self.scenario["duration_ns"] = 250_001_000_000
        with self.assertRaisesRegex(ValueError, "sample budget exceeded"):
            compile_ephemeris(self.scenario, self.config, self.position, 1_000_000)
        self.assertEqual(self.calls, [])

    def test_nonfinite_or_malformed_position_fails_closed(self):
        for vector in ((float("nan"), 0, 0), (float("inf"), 0, 0),
                       (0, 0), (True, 0, 0), ("0", 0, 0)):
            with self.subTest(vector=vector), self.assertRaisesRegex(ValueError, "invalid ITRS"):
                compile_ephemeris(self.scenario, self.config, lambda _id, _at: vector,
                                  1_000_000_000)

    def test_cli_writes_exact_input_and_output_digests(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            scenario_path, orbit_path, out = (root / "scenario.json", root / "orbit.json", root / "ephemeris.json")
            scenario_bytes = (json.dumps(self.scenario) + "\n").encode()
            orbit_bytes = (json.dumps(self.config) + "\n").encode()
            scenario_path.write_bytes(scenario_bytes)
            orbit_path.write_bytes(orbit_bytes)
            argv = [str(scenario_path), str(orbit_path), "--step-ns", "1000000000", "--out", str(out)]
            with patch("ntarlink_orbital.ephemeris.build_sgp4_provider", return_value=self.position), \
                 patch("ntarlink_orbital.ephemeris.version", return_value="test-version"):
                main(argv)
                first_output = out.read_bytes()
                main(argv)
            self.assertEqual(first_output, out.read_bytes())
            manifest = json.loads(Path(str(out) + ".ephemeris-manifest.json").read_text())
            self.assertEqual(manifest["artifact_sha256"], hashlib.sha256(first_output).hexdigest())
            self.assertEqual(manifest["scenario_input_sha256"], hashlib.sha256(scenario_bytes).hexdigest())
            self.assertEqual(manifest["orbit_input_sha256"], hashlib.sha256(orbit_bytes).hexdigest())
            self.assertEqual(manifest["sample_count"], 6)


if __name__ == "__main__":
    unittest.main()
