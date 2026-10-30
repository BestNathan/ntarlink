"""Contract tests for SGP4-to-Rust-native sampled ITRS optical geometry."""

import hashlib
import json
import math
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from ntarlink_orbital.optical_geometry import compile_optical_geometry, main
from ntarlink_orbital.optical_native_itrs import compile_native_itrs
from test_optical_attitude_samples import DURATION, attitude, orbit, position, scenario
from test_optical_tracking import drifting_attitude


class NativeItrsTests(unittest.TestCase):
    def test_sampled_itrs_geometry_and_attitude_are_deterministic(self):
        source = scenario()
        config = drifting_attitude()
        snapshots = compile_optical_geometry(source, orbit(), config, position)
        calls = []

        def tracked_position(satellite, at_ns):
            calls.append((satellite, at_ns))
            return position(satellite, at_ns)

        result, plan = compile_native_itrs(source, snapshots, tracked_position, 3, config)
        self.assertNotIn("optical_geometry", result)
        self.assertNotIn("optical_steering_samples", result)
        self.assertEqual(result["faults"], [])
        samples = result["optical_geometry_samples"]
        self.assertEqual([s["at_ns"] for s in samples], [
            0, 30_000_000, 60_000_000, 90_000_000, 119_999_000,
            120_000_000, 140_000_000, 160_000_000, 180_000_000, 199_999_000,
        ])
        self.assertEqual(plan["sample_count"], 10)
        self.assertEqual(plan["contact_count"], 2)
        self.assertEqual(len(calls), 20)
        self.assertTrue(all(s["frame"] == "ITRS" for s in samples))
        self.assertEqual(samples[0]["from_position_m"], [7_000_000.0, 0.0, 0.0])
        self.assertEqual(samples[0]["to_position_m"], [7_001_000.0, 0.0, 0.0])
        self.assertAlmostEqual(samples[1]["from_boresight_itrs"][0],
                               math.sqrt(0.5), places=12)
        self.assertAlmostEqual(samples[2]["from_boresight_itrs"][1], 1.0, places=12)
        self.assertAlmostEqual(samples[3]["from_boresight_itrs"][0], 1.0, places=12)
        self.assertEqual((result, plan), compile_native_itrs(
            source, snapshots, position, 3, config))
        self.assertNotIn("optical_geometry_samples", source)

    def test_delayed_contact_and_fixed_attitude_preserve_administrative_faults(self):
        source = scenario()
        source["contacts"][0]["start_ns"] = 20_000_000
        source["faults"] = [
            {"link_id": "isl", "at_ns": 40_000_000, "action": "down"},
            {"link_id": "isl", "at_ns": 70_000_000, "action": "up"},
        ]
        config = attitude()
        config["attitude_model"] = "fixed_body_to_itrs_quaternion"
        for head in config["heads"]:
            del head["attitude_samples"]
            head["body_to_itrs_quaternion_wxyz"] = [1, 0, 0, 0]
        snapshots = compile_optical_geometry(source, orbit(), config, position)
        result, plan = compile_native_itrs(source, snapshots, position, 3, config)
        self.assertEqual(result["faults"], source["faults"])
        self.assertEqual(result["optical_geometry_samples"][0]["at_ns"], 0)
        self.assertEqual(result["optical_geometry_samples"][1]["at_ns"], 20_000_000)
        self.assertEqual(plan["sample_count"], 11)

    def test_rejects_unsafe_composition_invalid_geometry_and_budget(self):
        source = scenario()
        config = drifting_attitude()
        snapshots = compile_optical_geometry(source, orbit(), config, position)
        with self.assertRaisesRegex(ValueError, "probes_per_contact"):
            compile_native_itrs(source, snapshots, position, 2, config)
        with patch("ntarlink_orbital.optical_native_steering.MAX_STEERING_SAMPLES", 9):
            with self.assertRaisesRegex(ValueError, "budget"):
                compile_native_itrs(source, snapshots, position, 3, config)
        source["faults"] = [
            {"link_id": "isl", "at_ns": 30_000_000,
             "action": "down", "cause": "sampled_pointing"},
        ]
        with self.assertRaisesRegex(ValueError, "sampled_pointing"):
            compile_native_itrs(source, snapshots, position, 3, config)
        source["faults"] = []
        source["optical_geometry_samples"] = []
        with self.assertRaisesRegex(ValueError, "cannot be overwritten"):
            compile_native_itrs(source, snapshots, position, 3, config)
        with self.assertRaisesRegex(ValueError, "existing optical pointing"):
            compile_optical_geometry(source, orbit(), config, position)
        del source["optical_geometry_samples"]

        def invalid_position(satellite, at_ns):
            if satellite == "sat-b" and at_ns == 30_000_000:
                return (float("nan"), 0.0, 0.0)
            return position(satellite, at_ns)

        with self.assertRaisesRegex(ValueError, "ITRS position"):
            compile_native_itrs(source, snapshots, invalid_position, 3, config)

        def overlapping_position(satellite, at_ns):
            if at_ns == 30_000_000:
                return (7_000_000.0, 0.0, 0.0)
            return position(satellite, at_ns)

        with self.assertRaisesRegex(ValueError, "degenerate optical separation"):
            compile_native_itrs(source, snapshots, overlapping_position, 3, config)

    def test_short_contact_deduplicates_microsecond_probes(self):
        source = scenario()
        source["contacts"] = [{
            "link_id": "isl", "start_ns": 0, "end_ns": 1000,
            "propagation_delay_ns": 1000,
        }]
        config = attitude()
        snapshots = compile_optical_geometry(source, orbit(), config, position)
        result, plan = compile_native_itrs(source, snapshots, position, 7, config)
        self.assertEqual(plan["sample_count"], 1)
        self.assertEqual([s["at_ns"] for s in result["optical_geometry_samples"]], [0])

    def test_cli_manifest_provenance_and_failed_compile_atomicity(self):
        source = scenario()
        config = drifting_attitude()
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            inputs = [root / name for name in (
                "scenario.json", "orbit.json", "attitude.json",
            )]
            for path, value in zip(inputs, (source, orbit(), config)):
                path.write_text(json.dumps(value) + "\n")
            output = root / "compiled.json"
            args = [*(str(path) for path in inputs),
                    "--native-itrs-probes-per-contact", "3",
                    "--out", str(output)]
            with patch("ntarlink_orbital.optical_geometry.build_sgp4_provider",
                       return_value=position):
                main(args)
                first = output.read_bytes()
                manifest_path = Path(str(output) + ".optical-manifest.json")
                manifest_bytes = manifest_path.read_bytes()
                manifest = json.loads(manifest_bytes)
                self.assertEqual(manifest["snapshot_count"], 0)
                self.assertEqual(manifest["geometry_sample_count"], 10)
                self.assertEqual(manifest["steering_sample_count"], 0)
                self.assertEqual(manifest["native_itrs_plan"]["sample_count"], 10)
                self.assertEqual(manifest["artifact_sha256"],
                                 hashlib.sha256(first).hexdigest())
                self.assertEqual(manifest["scenario_input_sha256"],
                                 hashlib.sha256(inputs[0].read_bytes()).hexdigest())
                self.assertNotIn("optical_geometry", json.loads(first))
                main(args)
                self.assertEqual(output.read_bytes(), first)
                source["optical_geometry_samples"] = []
                inputs[0].write_text(json.dumps(source) + "\n")
                with self.assertRaisesRegex(ValueError, "existing optical pointing"):
                    main(args)
            self.assertEqual(output.read_bytes(), first)
            self.assertEqual(manifest_path.read_bytes(), manifest_bytes)


if __name__ == "__main__":
    unittest.main()
