"""Bounded time-indexed SYS-02 optical-head attitude contract and probe tests."""

import copy
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
    _boresight_at, _heads, _slerp, audit_optical_pointing,
    compile_optical_geometry, main,
)

DURATION = 200_000_000
Q0 = [1.0, 0.0, 0.0, 0.0]
Q90 = [math.sqrt(0.5), 0.0, 0.0, math.sqrt(0.5)]


def sample(at_ns, quaternion):
    return {"at_ns": at_ns, "body_to_itrs_quaternion_wxyz": quaternion}


def attitude():
    return {
        "frame": "ITRS",
        "attitude_model": "sampled_body_to_itrs_quaternion",
        "heads": [
            {"head_id": "sat-a/h1", "boresight_body": [1, 0, 0],
             "max_off_axis_deg": 30,
             "attitude_samples": [sample(0, Q0), sample(DURATION, Q0)]},
            {"head_id": "sat-b/h1", "boresight_body": [-1, 0, 0],
             "max_off_axis_deg": 30,
             "attitude_samples": [sample(0, Q0), sample(DURATION, Q0)]},
        ],
    }


def scenario():
    return {
        "schema_version": 1, "contact_mode": "scheduled",
        "duration_ns": DURATION, "control_delay_ns": 0,
        "nodes": [{"id": "sat-a", "kind": "satellite"},
                  {"id": "sat-b", "kind": "satellite"}],
        "links": [{"id": "isl", "kind": "optical_isl",
                   "from": "sat-a", "to": "sat-b",
                   "capacity_bps": 1_000_000, "queue_limit_bytes": 4096,
                   "propagation_delay_ns": 1000}],
        "contacts": [
            {"link_id": "isl", "start_ns": 0, "end_ns": 120_000_000,
             "propagation_delay_ns": 1000},
            {"link_id": "isl", "start_ns": 120_000_000, "end_ns": DURATION,
             "propagation_delay_ns": 1000},
        ],
        "optical_pat": [{"link_id": "isl", "from_head": "sat-a/h1",
                         "to_head": "sat-b/h1", "slew_ns": 10_000_000,
                         "acquisition_ns": 10_000_000}],
        "flows": [], "faults": [],
    }


def orbit():
    return {"epoch_utc": "2014-01-23T06:20:00Z",
            "satellites": [{"id": "sat-a"}, {"id": "sat-b"}]}


def position(satellite, _at_ns):
    return (7_000_000.0 + (1000.0 if satellite == "sat-b" else 0.0),
            0.0, 0.0)


class SampledAttitudeTests(unittest.TestCase):
    def test_slerp_halfway_and_hemisphere_equivalence(self):
        heads = _heads(attitude(), DURATION)
        axis = _boresight_at(heads["sat-a/h1"], 100_000_000)
        self.assertEqual(axis, [1.0, 0.0, 0.0])
        changed = attitude()
        changed["heads"][0]["attitude_samples"] = [
            sample(0, Q0), sample(DURATION, [0, 0, 0, 1])]
        head = _heads(changed, DURATION)["sat-a/h1"]
        halfway = _boresight_at(head, 100_000_000)
        self.assertAlmostEqual(halfway[0], 0.0, places=12)
        self.assertAlmostEqual(halfway[1], 1.0, places=12)
        changed["heads"][0]["attitude_samples"][-1] = sample(
            DURATION, [0, 0, 0, -1])
        halfway_negative = _boresight_at(
            _heads(changed, DURATION)["sat-a/h1"], 100_000_000)
        for a, b in zip(halfway, halfway_negative):
            self.assertAlmostEqual(a, b, places=12)
        self.assertEqual(_slerp(tuple(Q0), tuple(-v for v in Q0), 0.5),
                         tuple(Q0))

    def test_compiled_start_snapshots_use_attitude_at_each_contact(self):
        config = attitude()
        config["heads"][0]["attitude_samples"] = [
            sample(0, Q0), sample(120_000_000, Q90), sample(DURATION, Q90)]
        compiled = compile_optical_geometry(scenario(), orbit(), config, position)
        snapshots = compiled["optical_geometry"]
        self.assertEqual([g["at_ns"] for g in snapshots], [0, 120_000_000])
        self.assertEqual(snapshots[0]["from_boresight_itrs"], [1.0, 0.0, 0.0])
        self.assertAlmostEqual(snapshots[1]["from_boresight_itrs"][1], 1.0)
        self.assertEqual(compiled, compile_optical_geometry(
            scenario(), orbit(), config, position))

    def test_sampled_attitude_probe_rejects_interior_excursion(self):
        config = attitude()
        config["heads"][0]["attitude_samples"] = [
            sample(0, Q0), sample(60_000_000, Q90),
            sample(120_000_000, Q0), sample(DURATION, Q0)]
        compiled = compile_optical_geometry(scenario(), orbit(), config, position)
        self.assertEqual(compiled["optical_geometry"][0]["from_boresight_itrs"],
                         [1.0, 0.0, 0.0])
        with self.assertRaisesRegex(ValueError, "outside envelope"):
            audit_optical_pointing(scenario(), compiled, position, 1, config)
        # A start-only audit does not establish dynamic steering validity.
        self.assertEqual(audit_optical_pointing(
            scenario(), compiled, position, 1)["contact_count"], 2)

    def test_valid_sampled_probe_is_deterministic(self):
        config = attitude()
        compiled = compile_optical_geometry(scenario(), orbit(), config, position)
        first = audit_optical_pointing(scenario(), compiled, position, 3, config)
        self.assertEqual(first["sample_count"], 10)
        self.assertEqual(first, audit_optical_pointing(
            scenario(), compiled, position, 3, config))
        self.assertEqual(first["minimum_steering_margin_deg"], 30.0)

    def test_rejects_bad_sample_contract_and_coverage(self):
        cases = [
            ("ordered", lambda c: c["heads"][0]["attitude_samples"].insert(
                1, sample(0, Q0))),
            ("microsecond", lambda c: c["heads"][0]["attitude_samples"].insert(
                1, sample(1001, Q0))),
            ("entire virtual-time", lambda c: c["heads"][0]["attitude_samples"].__setitem__(
                1, sample(100_000_000, Q0))),
            ("unit length", lambda c: c["heads"][0]["attitude_samples"][1].update(
                body_to_itrs_quaternion_wxyz=[2, 0, 0, 0])),
            ("finite", lambda c: c["heads"][0]["attitude_samples"][1].update(
                body_to_itrs_quaternion_wxyz=[float("nan"), 0, 0, 0])),
            ("invalid attitude sample", lambda c: c["heads"][0]["attitude_samples"][1].update(
                extra=1)),
            ("invalid optical head", lambda c: c["heads"][0].update(
                body_to_itrs_quaternion_wxyz=Q0)),
        ]
        for expected, mutate in cases:
            with self.subTest(expected=expected):
                config = attitude()
                mutate(config)
                with self.assertRaisesRegex(ValueError, expected):
                    compile_optical_geometry(scenario(), orbit(), config, position)
        config = attitude()
        config["heads"][0]["attitude_samples"][0]["at_ns"] = True
        with self.assertRaisesRegex(ValueError, "sample times"):
            compile_optical_geometry(scenario(), orbit(), config, position)
        config = attitude()
        config["attitude_model"] = "unsupported"
        with self.assertRaisesRegex(ValueError, "frame/model"):
            compile_optical_geometry(scenario(), orbit(), config, position)

    def test_cli_manifest_and_fail_closed_no_output_mutation(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            config = attitude()
            paths = [root / name for name in ("scenario.json", "orbit.json", "attitude.json")]
            for path, value in zip(paths, (scenario(), orbit(), config)):
                path.write_text(json.dumps(value) + "\n")
            output = root / "compiled.json"
            args = [*(str(path) for path in paths), "--out", str(output),
                    "--pointing-probes-per-contact", "1"]
            with patch("ntarlink_orbital.optical_geometry.build_sgp4_provider",
                       return_value=position):
                main(args)
                original = output.read_bytes()
                manifest_path = Path(str(output) + ".optical-manifest.json")
                manifest_bytes = manifest_path.read_bytes()
                main(args)
                self.assertEqual(original, output.read_bytes())
                manifest = json.loads(manifest_path.read_text())
                self.assertEqual(manifest["attitude_model"],
                                 "sampled_body_to_itrs_quaternion")
                self.assertEqual(manifest["pointing_probe_audit"]["sample_count"], 6)
                self.assertEqual(manifest["artifact_sha256"],
                                 hashlib.sha256(original).hexdigest())
                config["heads"][0]["attitude_samples"] = [
                    sample(0, Q0), sample(60_000_000, Q90),
                    sample(120_000_000, Q0), sample(DURATION, Q0)]
                paths[2].write_text(json.dumps(config) + "\n")
                with self.assertRaisesRegex(ValueError, "outside envelope"):
                    main(args)
            self.assertEqual(output.read_bytes(), original)
            self.assertEqual(manifest_path.read_bytes(), manifest_bytes)


if __name__ == "__main__":
    unittest.main()
