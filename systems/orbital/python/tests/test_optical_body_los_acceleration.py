"""SYS-02 finite spacecraft-body LOS angular-acceleration audit tests."""

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
from test_optical_attitude_samples import orbit, scenario
from test_optical_body_los_rate import fixed_attitude, rotating_position


def reversing_position(satellite, at_ns):
    if satellite == "sat-a":
        return (7_000_000.0, 0.0, 0.0)
    if 30_000_000 <= at_ns < 60_000_000:
        return (7_000_000.0, 1_000.0, 0.0)
    return (7_001_000.0, 0.0, 0.0)


class BodyLosAccelerationTests(unittest.TestCase):
    def compile(self, position, acceleration_cap, rate_cap=None):
        source = scenario()
        config = fixed_attitude()
        snapshots = compile_optical_geometry(source, orbit(), config, position)
        return compile_native_itrs(
            source, snapshots, position, 3, config, rate_cap, acceleration_cap
        )

    def test_speed_step_exceeds_acceleration_limit_and_records_peak(self):
        with self.assertRaisesRegex(
                ValueError, "body-frame LOS acceleration exceeds.*sat-a/h1"):
            self.compile(rotating_position, 99_999.0)
        result, plan = self.compile(rotating_position, 100_000.0)
        self.assertEqual(len(result["optical_geometry_samples"]), 10)
        self.assertIsNone(plan["body_los_rate_audit"])
        audit = plan["body_los_acceleration_audit"]
        self.assertEqual(audit["reference_frame"], "spacecraft_body")
        self.assertEqual(
            [x["head_id"] for x in
             audit["peak_sampled_acceleration_by_head_deg_per_s2"]],
            ["sat-a/h1", "sat-b/h1"],
        )
        for head in audit["peak_sampled_acceleration_by_head_deg_per_s2"]:
            self.assertAlmostEqual(
                head["peak_acceleration_deg_per_s2"], 100_000.0, places=6)
        self.assertEqual((result, plan),
                         self.compile(rotating_position, 100_000.0))
        self.assertIsNone(self.compile(rotating_position, None)[1]
                          ["body_los_acceleration_audit"])

    def test_direction_reversal_is_not_missed_by_equal_scalar_speeds(self):
        # +90 degrees then -90 degrees in equal intervals: scalar speeds
        # match, but the sampled angular velocity vector reverses.
        with self.assertRaisesRegex(ValueError, "body-frame LOS acceleration"):
            self.compile(reversing_position, 150_000.0)
        _, plan = self.compile(reversing_position, 200_001.0, 3_000.0)
        self.assertIsNotNone(plan["body_los_rate_audit"])
        peaks = plan["body_los_acceleration_audit"][
            "peak_sampled_acceleration_by_head_deg_per_s2"]
        self.assertTrue(all(math.isclose(
            head["peak_acceleration_deg_per_s2"], 200_000.0,
            rel_tol=1e-12) for head in peaks))

    def test_invalid_limits_and_antiparallel_axis_fail_closed(self):
        for limit in (-1, True, "1", float("nan"), float("inf")):
            with self.subTest(limit=limit):
                with self.assertRaisesRegex(
                        ValueError, "max_body_los_accel_deg_per_s2"):
                    self.compile(rotating_position, limit)

        def antiparallel_position(satellite, at_ns):
            if satellite == "sat-a":
                return (7_000_000.0, 0.0, 0.0)
            return ((6_999_000.0 if at_ns >= 30_000_000
                     else 7_001_000.0), 0.0, 0.0)

        with self.assertRaisesRegex(ValueError, "ambiguous 180-degree"):
            self.compile(antiparallel_position, 1_000_000.0)

    def test_cli_provenance_and_failed_output_preservation(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            paths = [root / name for name in (
                "scenario.json", "orbit.json", "attitude.json")]
            for path, data in zip(paths, (
                    scenario(), orbit(), fixed_attitude())):
                path.write_text(json.dumps(data) + "\n")
            output = root / "compiled.json"
            args = [*(str(p) for p in paths),
                    "--native-itrs-probes-per-contact", "3",
                    "--native-itrs-max-body-los-accel-deg-per-s2", "100000",
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
                audit = manifest["native_itrs_plan"]["body_los_acceleration_audit"]
                self.assertEqual(audit["max_body_los_accel_deg_per_s2"], 100000)
                self.assertEqual(len(
                    audit["peak_sampled_acceleration_by_head_deg_per_s2"]), 2)
                with self.assertRaisesRegex(ValueError, "acceleration exceeds"):
                    main([value if value != "100000" else "99999"
                          for value in args])
                with self.assertRaisesRegex(
                        ValueError, "requires --native-itrs"):
                    main([*(str(p) for p in paths),
                          "--native-itrs-max-body-los-accel-deg-per-s2", "100",
                          "--out", str(output)])
            self.assertEqual(output.read_bytes(), original)
            self.assertEqual(manifest_path.read_bytes(), original_manifest)


if __name__ == "__main__":
    unittest.main()
