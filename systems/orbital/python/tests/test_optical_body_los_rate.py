"""SYS-02 finite spacecraft-body LOS rate audit for native ITRS compilation."""

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
from test_optical_attitude_samples import (
    DURATION, Q0, Q90, attitude, orbit, sample, scenario,
)


def rotating_position(satellite, at_ns):
    if satellite == "sat-a":
        return (7_000_000.0, 0.0, 0.0)
    if at_ns < 30_000_000:
        return (7_001_000.0, 0.0, 0.0)
    return (7_000_000.0, 1_000.0, 0.0)


def fixed_attitude():
    config = attitude()
    config["attitude_model"] = "fixed_body_to_itrs_quaternion"
    for head in config["heads"]:
        del head["attitude_samples"]
        head["body_to_itrs_quaternion_wxyz"] = Q0
    return config


class BodyLosRateTests(unittest.TestCase):
    def compile(self, config, cap):
        source = scenario()
        snapshots = compile_optical_geometry(source, orbit(), config, rotating_position)
        return compile_native_itrs(source, snapshots, rotating_position, 3, config, cap)

    def test_fixed_body_rejects_rapid_target_motion_and_records_peak(self):
        config = fixed_attitude()
        with self.assertRaisesRegex(ValueError, "body-frame LOS rate exceeds.*sat-a/h1"):
            self.compile(config, 2_999.0)
        result, plan = self.compile(config, 3_000.0)
        self.assertEqual(len(result["optical_geometry_samples"]), 10)
        audit = plan["body_los_rate_audit"]
        self.assertEqual(audit["reference_frame"], "spacecraft_body")
        self.assertEqual([x["head_id"] for x in
                          audit["peak_sampled_rate_by_head_deg_per_s"]],
                         ["sat-a/h1", "sat-b/h1"])
        for head in audit["peak_sampled_rate_by_head_deg_per_s"]:
            self.assertAlmostEqual(head["peak_rate_deg_per_s"], 3_000.0, places=8)
        self.assertEqual((result, plan), self.compile(config, 3_000.0))
        self.assertIsNone(self.compile(config, None)[1]["body_los_rate_audit"])

    def test_co_rotating_body_has_zero_relative_los_rate(self):
        config = attitude()
        for head in config["heads"]:
            head["attitude_samples"] = [
                sample(0, Q0), sample(30_000_000, Q90),
                sample(DURATION, [-value for value in Q90]),
            ]
        _, plan = self.compile(config, 0.0)
        for head in plan["body_los_rate_audit"]["peak_sampled_rate_by_head_deg_per_s"]:
            self.assertLess(head["peak_rate_deg_per_s"], 1e-8)

    def test_invalid_rate_and_mode_fail_closed(self):
        for cap in (-1, True, None, "100", float("nan"), float("inf")):
            if cap is None:
                continue
            with self.subTest(cap=cap):
                with self.assertRaisesRegex(ValueError, "max_body_los_rate_deg_per_s"):
                    self.compile(fixed_attitude(), cap)
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            paths = [root / name for name in ("scenario.json", "orbit.json", "attitude.json")]
            for path, data in zip(paths, (scenario(), orbit(), fixed_attitude())):
                path.write_text(json.dumps(data) + "\n")
            output = root / "compiled.json"
            args = [*(str(p) for p in paths),
                    "--native-itrs-probes-per-contact", "3",
                    "--native-itrs-max-body-los-rate-deg-per-s", "3000",
                    "--out", str(output)]
            with patch("ntarlink_orbital.optical_geometry.build_sgp4_provider",
                       return_value=rotating_position):
                main(args)
                original = output.read_bytes()
                manifest_path = Path(str(output) + ".optical-manifest.json")
                original_manifest = manifest_path.read_bytes()
                manifest = json.loads(original_manifest)
                self.assertEqual(manifest["artifact_sha256"],
                                 hashlib.sha256(original).hexdigest())
                self.assertEqual(manifest["native_itrs_plan"]["body_los_rate_audit"]
                                 ["max_body_los_rate_deg_per_s"], 3000.0)
                main(args)
                self.assertEqual(output.read_bytes(), original)
                args[args.index("3000")] = "2999"
                with self.assertRaisesRegex(ValueError, "body-frame LOS rate exceeds"):
                    main(args)
                with self.assertRaisesRegex(ValueError, "requires --native-itrs"):
                    main([*(str(p) for p in paths),
                          "--native-itrs-max-body-los-rate-deg-per-s", "100",
                          "--out", str(output)])
            self.assertEqual(output.read_bytes(), original)
            self.assertEqual(manifest_path.read_bytes(), original_manifest)


if __name__ == "__main__":
    unittest.main()
