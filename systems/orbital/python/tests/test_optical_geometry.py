"""Contract and cross-subsystem tests for SGP4-derived optical pointing snapshots."""

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
    _rotate_body_to_itrs, audit_optical_pointing, compile_optical_geometry, main,
)


class OpticalGeometryAdapterTests(unittest.TestCase):
    def setUp(self):
        self.scenario = {
            "schema_version": 1, "name": "two-satellite-pat", "duration_ns": 200_000_000,
            "control_delay_ns": 0, "contact_mode": "scheduled",
            "nodes": [
                {"id": "sat-a", "kind": "satellite"},
                {"id": "sat-b", "kind": "satellite"},
            ],
            "links": [
                {"id": "isl-a-b", "from": "sat-a", "to": "sat-b", "kind": "optical_isl",
                 "capacity_bps": 1_000_000, "queue_limit_bytes": 4096,
                 "propagation_delay_ns": 1000},
            ],
            "flows": [], "faults": [],
            "contacts": [
                {"link_id": "isl-a-b", "start_ns": 120_000_000, "end_ns": 200_000_000,
                 "propagation_delay_ns": 1000},
                {"link_id": "isl-a-b", "start_ns": 0, "end_ns": 120_000_000,
                 "propagation_delay_ns": 1000},
            ],
            "optical_pat": [
                {"link_id": "isl-a-b", "from_head": "sat-a/h1", "to_head": "sat-b/h1",
                 "slew_ns": 10_000_000, "acquisition_ns": 10_000_000},
            ],
        }
        self.orbit = {
            "epoch_utc": "2014-01-23T06:20:00Z",
            "satellites": [{"id": "sat-a"}, {"id": "sat-b"}],
        }
        self.attitude = {
            "frame": "ITRS", "attitude_model": "fixed_body_to_itrs_quaternion",
            "heads": [
                {"head_id": "sat-a/h1", "boresight_body": [1, 0, 0],
                 "body_to_itrs_quaternion_wxyz": [1, 0, 0, 0],
                 "max_off_axis_deg": 30},
                {"head_id": "sat-b/h1", "boresight_body": [-1, 0, 0],
                 "body_to_itrs_quaternion_wxyz": [1, 0, 0, 0],
                 "max_off_axis_deg": 30},
            ],
        }
        self.calls = []

    def position(self, satellite, at_ns):
        self.calls.append((satellite, at_ns))
        if satellite == "sat-a":
            return (7_000_000.0, 0.0, 0.0)
        if at_ns == 0:
            return (7_001_000.0, 0.0, 0.0)
        return (7_000_000.0, 1_000.0, 0.0)

    def test_per_contact_positions_are_sampled_and_sorted(self):
        result = compile_optical_geometry(self.scenario, self.orbit,
                                          self.attitude, self.position)
        snapshots = result["optical_geometry"]
        self.assertEqual([x["at_ns"] for x in snapshots], [0, 120_000_000])
        self.assertEqual(snapshots[0]["to_position_m"], [7_001_000.0, 0.0, 0.0])
        self.assertEqual(snapshots[1]["to_position_m"], [7_000_000.0, 1_000.0, 0.0])
        self.assertEqual(snapshots[0]["from_boresight_itrs"], [1.0, 0.0, 0.0])
        self.assertEqual(len(self.calls), 4)
        self.assertNotIn("optical_geometry", self.scenario)
        self.assertEqual(result, compile_optical_geometry(
            self.scenario, self.orbit, self.attitude, self.position))

    def test_body_to_itrs_quaternion_rotation(self):
        q = [math.sqrt(0.5), 0, 0, math.sqrt(0.5)]
        rotated = _rotate_body_to_itrs((1, 0, 0), q)
        self.assertAlmostEqual(rotated[0], 0.0, places=12)
        self.assertAlmostEqual(rotated[1], 1.0, places=12)
        self.attitude["heads"][0]["body_to_itrs_quaternion_wxyz"] = q
        result = compile_optical_geometry(self.scenario, self.orbit,
                                          self.attitude, self.position)
        self.assertAlmostEqual(result["optical_geometry"][0]["from_boresight_itrs"][1],
                               1.0, places=12)

    def test_rejects_invalid_attitude_and_head_ownership(self):
        cases = [
            ("frame", lambda: self.attitude.update(frame="TEME")),
            ("quaternion", lambda: self.attitude["heads"][0].update(
                body_to_itrs_quaternion_wxyz=[2, 0, 0, 0])),
            ("boresight_body", lambda: self.attitude["heads"][0].update(
                boresight_body=[0, 0, 0])),
            ("max_off_axis_deg", lambda: self.attitude["heads"][0].update(
                max_off_axis_deg=float("nan"))),
            ("incorrectly owned optical head", lambda: self.attitude["heads"][0].update(
                head_id="sat-b/wrong")),
        ]
        for message, modify in cases:
            with self.subTest(message=message):
                saved = json.loads(json.dumps(self.attitude))
                modify()
                with self.assertRaisesRegex(ValueError, message):
                    compile_optical_geometry(self.scenario, self.orbit,
                                             self.attitude, self.position)
                self.attitude = saved

    def test_rejects_invalid_contact_and_orbit_contract(self):
        cases = [
            ("sub-microsecond", lambda: self.scenario["contacts"][0].update(start_ns=120_000_001)),
            ("duplicate optical contact", lambda: self.scenario["contacts"].append(
                dict(self.scenario["contacts"][1]))),
            ("existing optical pointing", lambda: self.scenario.update(optical_pointing=[{}])),
            ("missing or duplicate orbital", lambda: self.orbit.update(satellites=[{"id": "sat-a"}])),
        ]
        for message, modify in cases:
            with self.subTest(message=message):
                scenario = json.loads(json.dumps(self.scenario))
                orbit = json.loads(json.dumps(self.orbit))
                modify()
                with self.assertRaisesRegex(ValueError, message):
                    compile_optical_geometry(self.scenario, self.orbit,
                                             self.attitude, self.position)
                self.scenario, self.orbit = scenario, orbit

    def test_degenerate_and_nonfinite_provider_positions_fail_closed(self):
        with self.assertRaisesRegex(ValueError, "degenerate"):
            compile_optical_geometry(self.scenario, self.orbit, self.attitude,
                                     lambda _sat, _at: (7_000_000, 0, 0))
        with self.assertRaisesRegex(ValueError, "ITRS position"):
            compile_optical_geometry(self.scenario, self.orbit, self.attitude,
                                     lambda _sat, _at: (float("nan"), 0, 0))

    def test_cli_manifest_digests_and_reproducibility(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            paths = [root / name for name in ("scenario.json", "orbit.json", "attitude.json")]
            for path, document in zip(paths, (self.scenario, self.orbit, self.attitude)):
                path.write_text(json.dumps(document) + "\n")
            output = root / "compiled.json"
            args = [*(str(path) for path in paths), "--out", str(output)]
            with patch("ntarlink_orbital.optical_geometry.build_sgp4_provider",
                       return_value=self.position):
                main(args)
                first = output.read_bytes()
                main(args)
            self.assertEqual(output.read_bytes(), first)
            manifest = json.loads(Path(str(output) + ".optical-manifest.json").read_text())
            self.assertEqual(manifest["artifact_sha256"], hashlib.sha256(first).hexdigest())
            self.assertEqual(manifest["snapshot_count"], 2)
            for key, path in zip(("scenario_input_sha256", "orbit_input_sha256",
                                  "attitude_input_sha256"), paths):
                self.assertEqual(manifest[key], hashlib.sha256(path.read_bytes()).hexdigest())
            with self.assertRaisesRegex(ValueError, "distinct"):
                main([*(str(path) for path in paths), "--out", str(paths[0])])


    def aligned_position(self, satellite, at_ns):
        return (7_000_000.0 + (1_000.0 if satellite == "sat-b" else 0.0),
                0.0, 0.0)

    def test_pointing_probe_audit_passes_aligned_contacts_and_is_deterministic(self):
        compiled = compile_optical_geometry(self.scenario, self.orbit,
                                            self.attitude, self.aligned_position)
        first = audit_optical_pointing(self.scenario, compiled, self.aligned_position, 1, self.attitude)
        self.assertEqual(first, audit_optical_pointing(
            self.scenario, compiled, self.aligned_position, 1, self.attitude))
        self.assertEqual(first["contact_count"], 2)
        self.assertEqual(first["sample_count"], 6)
        self.assertEqual([c["sample_count"] for c in first["contacts"]], [3, 3])
        self.assertAlmostEqual(first["minimum_steering_margin_deg"], 30.0)
        self.assertEqual(first["contacts"][0]["end_ns"], 120_000_000)
        with self.assertRaisesRegex(ValueError, "probes_per_contact"):
            audit_optical_pointing(self.scenario, compiled, self.aligned_position, 0, self.attitude)

    def test_probe_audit_catches_interior_and_last_microsecond_drift(self):
        for violation_ns in (60_000_000, 119_999_000):
            with self.subTest(violation_ns=violation_ns):
                def drifting_position(satellite, at_ns):
                    if satellite == "sat-b" and at_ns == violation_ns:
                        return (7_000_000.0, 1_000.0, 0.0)
                    return self.aligned_position(satellite, at_ns)
                compiled = compile_optical_geometry(
                    self.scenario, self.orbit, self.attitude, drifting_position)
                # Start-only snapshots cannot observe the subsequent violation.
                self.assertEqual(len(compiled["optical_geometry"]), 2)
                with self.assertRaisesRegex(ValueError, "outside envelope"):
                    audit_optical_pointing(self.scenario, compiled, drifting_position, 1, self.attitude)

    def test_short_contact_probe_times_are_unique_and_half_open(self):
        self.scenario["contacts"] = [{
            "link_id": "isl-a-b", "start_ns": 0, "end_ns": 1_000,
            "propagation_delay_ns": 1_000,
        }]
        seen = []
        def position(satellite, at_ns):
            seen.append(at_ns)
            return self.aligned_position(satellite, at_ns)
        compiled = compile_optical_geometry(self.scenario, self.orbit,
                                            self.attitude, position)
        report = audit_optical_pointing(self.scenario, compiled, position, 7, self.attitude)
        self.assertEqual(report["sample_count"], 1)
        self.assertEqual(set(seen), {0})

    def test_cli_probe_manifest_and_failure_before_output(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            paths = [root / name for name in ("scenario.json", "orbit.json", "attitude.json")]
            for path, document in zip(paths, (self.scenario, self.orbit, self.attitude)):
                path.write_text(json.dumps(document) + "\n")
            output = root / "compiled.json"
            args = [*(str(path) for path in paths), "--out", str(output),
                    "--pointing-probes-per-contact", "1"]
            with patch("ntarlink_orbital.optical_geometry.build_sgp4_provider",
                       return_value=self.aligned_position):
                main(args)
            manifest = json.loads(Path(str(output) + ".optical-manifest.json").read_text())
            self.assertEqual(manifest["pointing_probe_audit"]["sample_count"], 6)
            self.assertEqual(manifest["pointing_probe_audit"]["contact_count"], 2)
            original = output.read_bytes()
            def drift(satellite, at_ns):
                if satellite == "sat-b" and at_ns == 60_000_000:
                    return (7_000_000.0, 1_000.0, 0.0)
                return self.aligned_position(satellite, at_ns)
            with patch("ntarlink_orbital.optical_geometry.build_sgp4_provider",
                       return_value=drift):
                with self.assertRaisesRegex(ValueError, "outside envelope"):
                    main(args)
            self.assertEqual(output.read_bytes(), original)


if __name__ == "__main__":
    unittest.main()
