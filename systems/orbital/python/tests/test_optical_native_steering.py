"""Contract and regression tests for SGP4-to-native optical steering compilation."""

import hashlib
import json
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from ntarlink_orbital.optical_geometry import compile_optical_geometry, main
from ntarlink_orbital.optical_native_steering import compile_native_steering
from test_optical_attitude_samples import DURATION, attitude, orbit, position, scenario
from test_optical_tracking import drifting_attitude


class NativeSteeringTests(unittest.TestCase):
    def test_sampled_geometry_and_attitude_generate_runtime_observations(self):
        source = scenario()
        config = drifting_attitude()
        snapshots = compile_optical_geometry(source, orbit(), config, position)
        result, plan = compile_native_steering(source, snapshots, position, 3, config)
        self.assertNotIn("optical_geometry", result)
        self.assertEqual(result["faults"], [])
        samples = result["optical_steering_samples"]
        self.assertEqual([s["at_ns"] for s in samples], [
            0, 30_000_000, 60_000_000, 90_000_000, 119_999_000,
            120_000_000, 140_000_000, 160_000_000, 180_000_000, 199_999_000,
        ])
        self.assertEqual(plan["sample_count"], 10)
        self.assertEqual(plan["contact_count"], 2)
        self.assertEqual(samples[0]["from_off_axis_deg"], 0.0)
        self.assertGreater(samples[1]["from_off_axis_deg"], 30.0)
        self.assertGreater(samples[2]["from_off_axis_deg"], 80.0)
        self.assertLess(samples[3]["from_off_axis_deg"], 1e-6)
        self.assertTrue(all(s["to_off_axis_deg"] < 1e-6 for s in samples))
        self.assertEqual((result, plan), compile_native_steering(
            source, snapshots, position, 3, config))
        self.assertNotIn("optical_steering_samples", source)

    def test_preserves_administrative_faults_and_seeds_delayed_contact(self):
        source = scenario()
        source["contacts"][0]["start_ns"] = 20_000_000
        source["faults"] = [
            {"link_id": "isl", "at_ns": 40_000_000, "action": "down"},
            {"link_id": "isl", "at_ns": 70_000_000, "action": "up"},
        ]
        config = drifting_attitude()
        snapshots = compile_optical_geometry(source, orbit(), config, position)
        result, plan = compile_native_steering(source, snapshots, position, 3, config)
        self.assertEqual(result["faults"], source["faults"])
        self.assertEqual(result["optical_steering_samples"][0]["at_ns"], 0)
        self.assertEqual(result["optical_steering_samples"][1]["at_ns"], 20_000_000)
        self.assertEqual(plan["sample_count"], 11)

    def test_rejects_invalid_modes_and_bad_positions(self):
        source = scenario()
        config = drifting_attitude()
        snapshots = compile_optical_geometry(source, orbit(), config, position)
        with self.assertRaisesRegex(ValueError, "probes_per_contact"):
            compile_native_steering(source, snapshots, position, 2, config)
        with patch("ntarlink_orbital.optical_native_steering.MAX_STEERING_SAMPLES", 9):
            with self.assertRaisesRegex(ValueError, "budget"):
                compile_native_steering(source, snapshots, position, 3, config)
        source["faults"] = [
            {"link_id": "isl", "at_ns": 30_000_000,
             "action": "down", "cause": "sampled_pointing"},
        ]
        with self.assertRaisesRegex(ValueError, "sampled_pointing"):
            compile_native_steering(source, snapshots, position, 3, config)
        source["faults"] = []
        source["optical_steering_samples"] = [{}]
        with self.assertRaisesRegex(ValueError, "cannot be overwritten"):
            compile_native_steering(source, snapshots, position, 3, config)
        del source["optical_steering_samples"]

        def invalid_position(satellite, at_ns):
            if satellite == "sat-b" and at_ns == 30_000_000:
                return (float("nan"), 0.0, 0.0)
            return position(satellite, at_ns)

        with self.assertRaisesRegex(ValueError, "ITRS position"):
            compile_native_steering(source, snapshots, invalid_position, 3, config)
        fixed = attitude()
        fixed["attitude_model"] = "fixed_body_to_itrs_quaternion"
        with self.assertRaisesRegex(ValueError, "requires sampled attitude"):
            compile_native_steering(source, snapshots, position, 3, fixed)

    def test_cli_manifest_digests_and_failed_output_atomicity(self):
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
                    "--native-steering-probes-per-contact", "3",
                    "--out", str(output)]
            with patch("ntarlink_orbital.optical_geometry.build_sgp4_provider",
                       return_value=position):
                main(args)
                first = output.read_bytes()
                manifest_path = Path(str(output) + ".optical-manifest.json")
                manifest_bytes = manifest_path.read_bytes()
                manifest = json.loads(manifest_bytes)
                self.assertEqual(manifest["snapshot_count"], 0)
                self.assertEqual(manifest["steering_sample_count"], 10)
                self.assertEqual(manifest["native_steering_plan"]["sample_count"], 10)
                self.assertEqual(manifest["artifact_sha256"],
                                 hashlib.sha256(first).hexdigest())
                self.assertNotIn("optical_geometry", json.loads(first))
                main(args)
                self.assertEqual(output.read_bytes(), first)
                config["attitude_model"] = "fixed_body_to_itrs_quaternion"
                for head in config["heads"]:
                    head.pop("attitude_samples")
                    head["body_to_itrs_quaternion_wxyz"] = [1, 0, 0, 0]
                inputs[2].write_text(json.dumps(config) + "\n")
                with self.assertRaisesRegex(ValueError, "requires sampled attitude"):
                    main(args)
            self.assertEqual(output.read_bytes(), first)
            self.assertEqual(manifest_path.read_bytes(), manifest_bytes)


if __name__ == "__main__":
    unittest.main()
