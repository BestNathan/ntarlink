"""Midpoint audit contract, failure and provenance regression tests."""

import hashlib
import json
import math
import sys
import tempfile
import types
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from ntarlink_orbital.ephemeris_audit import audit_interpolation, main


class EphemerisAuditTests(unittest.TestCase):
    def setUp(self):
        self.doc = {
            "schema_version": 1, "frame": "ITRS", "time_scale": "UTC",
            "position_unit": "m", "time_resolution_ns": 1000,
            "epoch_utc": "2014-01-23T06:20:00Z", "duration_ns": 5_000_000_000,
            "sample_step_ns": 2_000_000_000, "sample_count": 3,
            "satellites": [{"id": "sat-a", "samples": [
                {"at_ns": t * 1_000_000_000, "position_m": [float(t * t), 0.0, 0.0]}
                for t in (0, 2, 4)]}],
        }
        self.calls = []

    def quadratic(self, sat_id, at_ns):
        self.calls.append((sat_id, at_ns))
        t = at_ns / 1_000_000_000
        return (t * t, 0.0, 0.0)

    def test_quadratic_midpoint_error_and_threshold(self):
        result = audit_interpolation(self.doc, self.quadratic, 1.0)
        self.assertEqual(self.calls, [("sat-a", 1_000_000_000),
                                      ("sat-a", 3_000_000_000)])
        self.assertEqual(result["midpoint_count"], 2)
        self.assertEqual(result["max_midpoint_error_m"], 1.0)
        self.assertEqual(result["rms_midpoint_error_m"], 1.0)
        self.assertTrue(result["passed"])
        self.assertFalse(audit_interpolation(self.doc, self.quadratic, 0.5)["passed"])

    def test_linear_interpolation_and_stable_satellite_order(self):
        self.doc["satellites"][0]["samples"] = [
            {"at_ns": t * 1_000_000_000, "position_m": [float(t), 1.0, 0.0]}
            for t in (0, 2, 4)]
        result = audit_interpolation(self.doc,
                                     lambda _id, t: (t / 1_000_000_000, 1.0, 0.0), 0.01)
        self.assertEqual(result["max_midpoint_error_m"], 0.0)
        self.assertTrue(result["passed"])

    def test_quarter_probes_detect_error_hidden_from_midpoints(self):
        # Both endpoints and midpoints agree with zero, but quarter points do not.
        self.doc["satellites"][0]["samples"] = [
            {"at_ns": t * 1_000_000_000, "position_m": [0.0, 0.0, 0.0]}
            for t in (0, 2, 4)]
        def oscillation(_sat_id, at_ns):
            return (math.sin(math.pi * at_ns / 1_000_000_000), 0.0, 0.0)

        midpoint = audit_interpolation(self.doc, oscillation, 0.5)
        self.assertTrue(midpoint["passed"])
        self.assertEqual(midpoint["probe_count"], 2)
        dense = audit_interpolation(self.doc, oscillation, 0.5, probes_per_interval=3)
        self.assertEqual(dense["midpoint_count"], 2)
        self.assertEqual(dense["probe_count"], 6)
        self.assertAlmostEqual(dense["max_midpoint_error_m"], 0.0)
        self.assertAlmostEqual(dense["max_probe_error_m"], 1.0)
        self.assertAlmostEqual(dense["rms_probe_error_m"], math.sqrt(2 / 3))
        self.assertFalse(dense["passed"])
        self.assertEqual(dense["satellites"][0]["probe_count"], 6)

    def test_probe_grid_and_count_fail_before_propagation(self):
        for probes in (0, 2, 8, True, "3"):
            with self.subTest(probes=probes), self.assertRaisesRegex(
                    ValueError, "probes_per_interval"):
                audit_interpolation(self.doc, self.quadratic, 1.0, probes)
        bad = {**self.doc, "sample_step_ns": 2_002_000}
        with self.assertRaisesRegex(ValueError, "multiple of 4000"):
            audit_interpolation(bad, self.quadratic, 1.0, 3)
        self.assertEqual(self.calls, [])

    def test_three_and_seven_probes_are_deterministic(self):
        for probes in (3, 7):
            with self.subTest(probes=probes):
                result = audit_interpolation(self.doc, self.quadratic, 1.0, probes)
                self.assertEqual(result["probe_count"], 2 * probes)
                self.assertEqual(result["midpoint_count"], 2)
                self.assertGreaterEqual(result["max_probe_error_m"],
                                        result["max_midpoint_error_m"])
                expected_times = [i * 2_000_000_000
                                  + k * 2_000_000_000 // (probes + 1)
                                  for i in range(2) for k in range(1, probes + 1)]
                self.assertEqual([t for _, t in self.calls[-2 * probes:]],
                                 expected_times)

    def test_invalid_contract_fails_before_propagation(self):
        for change, pattern in (
            ({"frame": "TEME"}, "contract"),
            ({"sample_step_ns": 1000}, "multiple of 2000"),
            ({"sample_count": 2}, "sample_count"),
            ({"duration_ns": True}, "duration_ns"),
            ({"satellites": []}, "satellites"),
        ):
            with self.subTest(change=change), self.assertRaisesRegex(ValueError, pattern):
                audit_interpolation({**self.doc, **change}, self.quadratic, 1.0)
        for threshold in (0, -1, math.nan, math.inf, True):
            with self.subTest(threshold=threshold), self.assertRaisesRegex(ValueError, "finite"):
                audit_interpolation(self.doc, self.quadratic, threshold)
        self.assertEqual(self.calls, [])

    def test_malformed_grid_and_positions_fail_before_propagation(self):
        bad = json.loads(json.dumps(self.doc))
        bad["satellites"][0]["samples"][1]["at_ns"] += 1
        with self.assertRaisesRegex(ValueError, "regular"):
            audit_interpolation(bad, self.quadratic, 1)
        bad = json.loads(json.dumps(self.doc))
        bad["satellites"][0]["samples"][1]["position_m"][0] = math.nan
        with self.assertRaisesRegex(ValueError, "invalid ITRS"):
            audit_interpolation(bad, self.quadratic, 1)
        self.assertEqual(self.calls, [])

    def test_cli_provenance_success_and_threshold_failure(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            scenario = {"schema_version": 1, "duration_ns": 5_000_000_000,
                        "nodes": [{"id": "sat-a", "kind": "satellite"}]}
            orbit = {"epoch_utc": self.doc["epoch_utc"], "satellites": [{"id": "sat-a"}]}
            paths = [root / name for name in ("scenario.json", "orbit.json", "ephemeris.json")]
            for path, obj in zip(paths, (scenario, orbit, self.doc)):
                path.write_text(json.dumps(obj) + "\n")
            sha = lambda p: hashlib.sha256(p.read_bytes()).hexdigest()
            manifest = root / "source-manifest.json"
            manifest.write_text(json.dumps({
                "generator": "ntarlink_orbital.ephemeris",
                "artifact_sha256": sha(paths[2]),
                "scenario_input_sha256": sha(paths[0]),
                "orbit_input_sha256": sha(paths[1]),
            }))
            out = root / "audit.json"
            args = [*(str(p) for p in paths), "--source-manifest", str(manifest),
                    "--max-midpoint-error-m", "1.0", "--out", str(out)]
            module = types.ModuleType("ntarlink_orbital.contacts")
            module.build_sgp4_provider = lambda *_: self.quadratic
            with patch.dict(sys.modules, {"ntarlink_orbital.contacts": module}):
                main(args)
                first = out.read_bytes()
                main(args)
                self.assertEqual(first, out.read_bytes())
                self.assertEqual(json.loads(first)["ephemeris_sha256"], sha(paths[2]))
                args[args.index("1.0")] = "0.1"
                with self.assertRaises(SystemExit) as exit_result:
                    main(args)
                self.assertEqual(exit_result.exception.code, 1)
                self.assertFalse(json.loads(out.read_text())["passed"])
                paths[2].write_text(paths[2].read_text() + " ")
                with self.assertRaisesRegex(ValueError, "provenance mismatch"):
                    main(args)


if __name__ == "__main__":
    unittest.main()
