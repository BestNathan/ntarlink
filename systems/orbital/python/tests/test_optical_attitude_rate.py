"""SYS-02 optional body-to-ITRS sampled attitude angular-rate guard."""

import hashlib
import json
import math
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from ntarlink_orbital.optical_geometry import (
    _heads, _shortest_arc_rate_deg_per_s, main,
)
from test_optical_attitude_samples import (
    DURATION, Q0, Q90, attitude, orbit, position, sample, scenario,
)


class SampledAttitudeRateTests(unittest.TestCase):
    def test_exact_slerp_rate_limit_and_rejection(self):
        config = attitude()
        config["heads"][0]["attitude_samples"] = [
            sample(0, Q0), sample(100_000_000, Q90),
            sample(DURATION, Q90),
        ]
        config["heads"][0]["max_body_attitude_rate_deg_per_s"] = 900.0
        heads = _heads(config, DURATION)
        self.assertAlmostEqual(
            heads["sat-a/h1"]["peak_attitude_rate_deg_per_s"], 900.0, places=8)
        self.assertEqual(heads["sat-a/h1"]["max_attitude_rate_deg_per_s"], 900.0)
        config["heads"][0]["max_body_attitude_rate_deg_per_s"] = 899.0
        with self.assertRaisesRegex(ValueError, "rate exceeds.*sat-a/h1"):
            _heads(config, DURATION)

    def test_equivalent_quaternion_hemispheres_and_tiny_angles(self):
        config = attitude()
        config["heads"][0]["attitude_samples"] = [
            sample(0, Q0), sample(DURATION, [-value for value in Q0]),
        ]
        config["heads"][0]["max_body_attitude_rate_deg_per_s"] = 0
        heads = _heads(config, DURATION)
        self.assertEqual(heads["sat-a/h1"]["peak_attitude_rate_deg_per_s"], 0.0)
        self.assertEqual(_shortest_arc_rate_deg_per_s(
            tuple(Q0), tuple(-value for value in Q0), 1000), 0.0)
        small = math.radians(0.001) / 2.0
        q = (math.cos(small), 0.0, 0.0, math.sin(small))
        self.assertAlmostEqual(
            _shortest_arc_rate_deg_per_s(tuple(Q0), q, 1_000_000),
            1.0, places=9)

    def test_invalid_limits_and_fixed_model_fail_closed(self):
        for invalid in (-1, True, None, "100", float("nan"), float("inf")):
            with self.subTest(invalid=invalid):
                config = attitude()
                config["heads"][0]["max_body_attitude_rate_deg_per_s"] = invalid
                with self.assertRaisesRegex(
                        ValueError, "max_body_attitude_rate_deg_per_s"):
                    _heads(config, DURATION)
        config = attitude()
        config["attitude_model"] = "fixed_body_to_itrs_quaternion"
        for head in config["heads"]:
            del head["attitude_samples"]
            head["body_to_itrs_quaternion_wxyz"] = Q0
        config["heads"][0]["max_body_attitude_rate_deg_per_s"] = 10
        with self.assertRaisesRegex(ValueError, "invalid optical head attitude"):
            _heads(config, DURATION)

    def test_cli_rate_audit_provenance_and_failure_atomicity(self):
        source = scenario()
        config = attitude()
        config["heads"][0]["max_body_attitude_rate_deg_per_s"] = 900.0
        config["heads"][0]["attitude_samples"] = [
            sample(0, Q0), sample(100_000_000, Q90),
            sample(DURATION, Q90),
        ]
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
                artifact = output.read_bytes()
                manifest_path = Path(str(output) + ".optical-manifest.json")
                manifest_bytes = manifest_path.read_bytes()
                manifest = json.loads(manifest_bytes)
                self.assertEqual(
                    manifest["artifact_sha256"], hashlib.sha256(artifact).hexdigest())
                rates = {head["head_id"]: head for head in
                         manifest["attitude_rate_audit"]}
                self.assertAlmostEqual(
                    rates["sat-a/h1"]["peak_interpolated_rate_deg_per_s"],
                    900.0, places=8)
                self.assertEqual(
                    rates["sat-a/h1"]["max_body_attitude_rate_deg_per_s"], 900.0)
                self.assertIsNone(
                    rates["sat-b/h1"]["max_body_attitude_rate_deg_per_s"])
                main(args)
                self.assertEqual(output.read_bytes(), artifact)
                self.assertEqual(manifest_path.read_bytes(), manifest_bytes)
                config["heads"][0]["max_body_attitude_rate_deg_per_s"] = 899.0
                inputs[2].write_text(json.dumps(config) + "\n")
                with self.assertRaisesRegex(ValueError, "rate exceeds"):
                    main(args)
            self.assertEqual(output.read_bytes(), artifact)
            self.assertEqual(manifest_path.read_bytes(), manifest_bytes)


if __name__ == "__main__":
    unittest.main()
