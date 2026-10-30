"""Sampled SYS-02 steering loss compiled into existing SYS-03 PAT faults."""

import hashlib
import json
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from ntarlink_orbital.optical_geometry import compile_optical_geometry, main
from ntarlink_orbital.optical_tracking import compile_pointing_faults
from test_optical_attitude_samples import (
    DURATION, Q0, Q90, attitude, orbit, position, sample, scenario,
)


def drifting_attitude():
    config = attitude()
    config["heads"][0]["attitude_samples"] = [
        sample(0, Q0), sample(60_000_000, Q90),
        sample(90_000_000, Q0), sample(DURATION, Q0),
    ]
    return config


class SampledTrackingTests(unittest.TestCase):
    def test_loss_and_recovery_edges_are_deterministic(self):
        source = scenario()
        config = drifting_attitude()
        compiled = compile_optical_geometry(source, orbit(), config, position)
        result, plan = compile_pointing_faults(source, compiled, position, 3, config)
        self.assertEqual(result["faults"], [
            {"at_ns": 30_000_000, "link_id": "isl", "action": "down", "cause": "sampled_pointing"},
            {"at_ns": 90_000_000, "link_id": "isl", "action": "up", "cause": "sampled_pointing"},
        ])
        self.assertEqual(plan["sample_count"], 10)
        self.assertEqual(plan["derived_fault_count"], 2)
        self.assertTrue(all(edge["cause"] == "sampled_pointing"
                            for edge in plan["contacts"][0]["edges"]))
        self.assertEqual(result["optical_geometry"], compiled["optical_geometry"])
        self.assertEqual((result, plan), compile_pointing_faults(
            source, compiled, position, 3, config))
        self.assertEqual(source["faults"], [])

    def test_trailing_violation_restores_at_contact_end(self):
        source = scenario()
        source["contacts"] = source["contacts"][:1]
        config = attitude()
        config["heads"][0]["attitude_samples"] = [
            sample(0, Q0), sample(60_000_000, Q90),
            sample(DURATION, Q90),
        ]
        compiled = compile_optical_geometry(source, orbit(), config, position)
        result, plan = compile_pointing_faults(source, compiled, position, 3, config)
        self.assertEqual(result["faults"][0], {
            "at_ns": 30_000_000, "link_id": "isl", "action": "down", "cause": "sampled_pointing",
        })
        self.assertEqual(result["faults"][-1], {
            "at_ns": 120_000_000, "link_id": "isl", "action": "up", "cause": "sampled_pointing",
        })
        self.assertEqual(plan["sample_count"], 5)

    def test_short_half_open_contact_has_one_probe(self):
        source = scenario()
        source["contacts"] = [{
            "link_id": "isl", "start_ns": 0, "end_ns": 1000,
            "propagation_delay_ns": 1000,
        }]
        config = attitude()
        compiled = compile_optical_geometry(source, orbit(), config, position)
        result, plan = compile_pointing_faults(source, compiled, position, 7, config)
        self.assertEqual(result["faults"], [])
        self.assertEqual(plan["sample_count"], 1)

    def test_rejects_unsafe_composition_and_invalid_start(self):
        source = scenario()
        config = drifting_attitude()
        compiled = compile_optical_geometry(source, orbit(), config, position)
        with self.assertRaisesRegex(ValueError, "probes_per_contact"):
            compile_pointing_faults(source, compiled, position, 0, config)
        source["faults"] = [{"at_ns": 50_000_000, "link_id": "isl", "action": "down"}]
        with self.assertRaisesRegex(ValueError, "existing PAT link faults"):
            compile_pointing_faults(source, compiled, position, 3, config)
        source["faults"] = []
        config["heads"][0]["attitude_samples"] = [
            sample(0, Q90), sample(DURATION, Q0),
        ]
        compiled = compile_optical_geometry(source, orbit(), config, position)
        with self.assertRaisesRegex(ValueError, "starts outside"):
            compile_pointing_faults(source, compiled, position, 3, config)
        config["attitude_model"] = "fixed_body_to_itrs_quaternion"
        with self.assertRaisesRegex(ValueError, "require sampled"):
            compile_pointing_faults(source, compiled, position, 3, config)

    def test_cli_manifest_and_failed_compile_do_not_replace_output(self):
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
                    "--runtime-pointing-probes-per-contact", "3",
                    "--out", str(output)]
            with patch("ntarlink_orbital.optical_geometry.build_sgp4_provider",
                       return_value=position):
                main(args)
                first = output.read_bytes()
                manifest_path = Path(str(output) + ".optical-manifest.json")
                manifest_bytes = manifest_path.read_bytes()
                manifest = json.loads(manifest_bytes)
                self.assertIsNone(manifest["pointing_probe_audit"])
                self.assertEqual(manifest["pointing_tracking_plan"]["derived_fault_count"], 2)
                self.assertEqual(manifest["artifact_sha256"],
                                 hashlib.sha256(first).hexdigest())
                main(args)
                self.assertEqual(output.read_bytes(), first)
                config["heads"][0]["attitude_samples"] = [
                    sample(0, Q90), sample(DURATION, Q0),
                ]
                inputs[2].write_text(json.dumps(config) + "\n")
                with self.assertRaisesRegex(ValueError, "starts outside"):
                    main(args)
            self.assertEqual(output.read_bytes(), first)
            self.assertEqual(manifest_path.read_bytes(), manifest_bytes)


if __name__ == "__main__":
    unittest.main()
