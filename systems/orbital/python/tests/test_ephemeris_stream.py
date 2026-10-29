"""Deterministic JSONL ephemeris streaming, failure and historical ISS tests."""

import hashlib
import json
import math
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from ntarlink_orbital.ephemeris_stream import main, stream_ephemeris


class EphemerisStreamTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.output = Path(self.temp.name) / "ephemeris.jsonl"
        self.scenario = {
            "schema_version": 1,
            "duration_ns": 2_500_000_000,
            "nodes": [
                {"id": "sat-b", "kind": "satellite"},
                {"id": "ground", "kind": "terminal"},
                {"id": "sat-a", "kind": "satellite"},
            ],
        }
        self.orbit = {
            "epoch_utc": "2014-01-23T06:20:00Z",
            "satellites": [{"id": "sat-b"}, {"id": "sat-a"}],
        }
        self.calls = []

    def position(self, satellite_id, at_ns):
        self.calls.append((satellite_id, at_ns))
        return (6_800_000.0 + at_ns / 1e9, 1.0 if satellite_id == "sat-a" else 2.0, 0.0)

    def test_deterministic_satellite_major_grid_and_sha256(self):
        result = stream_ephemeris(self.scenario, self.orbit, self.position,
                                  1_000_000_000, self.output)
        first_bytes = self.output.read_bytes()
        rows = [json.loads(line) for line in first_bytes.splitlines()]
        self.assertEqual(rows[0]["record_type"], "header")
        self.assertEqual(rows[0]["frame"], "ITRS")
        self.assertEqual(rows[0]["time_scale"], "UTC")
        self.assertEqual(rows[0]["position_unit"], "m")
        self.assertEqual(rows[0]["sample_count"], 6)
        self.assertEqual(rows[0]["satellite_ids"], ["sat-a", "sat-b"])
        self.assertEqual(
            [(row["satellite_id"], row["at_ns"]) for row in rows[1:]],
            [(satellite_id, at_ns) for satellite_id in ("sat-a", "sat-b")
             for at_ns in (0, 1_000_000_000, 2_000_000_000)],
        )
        self.assertEqual(rows[2]["position_m"], [6_800_001.0, 1.0, 0.0])
        self.assertEqual(result["byte_count"], len(first_bytes))
        self.assertEqual(result["artifact_sha256"], hashlib.sha256(first_bytes).hexdigest())
        self.assertEqual(result["sample_count"], 6)
        self.assertEqual(len(self.calls), 6)
        self.assertEqual(stream_ephemeris(self.scenario, self.orbit, self.position,
                                          1_000_000_000, self.output), result)
        self.assertEqual(self.output.read_bytes(), first_bytes)

    def test_short_half_open_horizon(self):
        self.scenario["duration_ns"] = 1
        result = stream_ephemeris(self.scenario, self.orbit, self.position,
                                  1000, self.output)
        self.assertEqual(result["sample_count"], 2)
        rows = [json.loads(line) for line in self.output.read_text().splitlines()]
        self.assertEqual([row["at_ns"] for row in rows[1:]], [0, 0])

    def test_validation_fails_before_position_calls(self):
        with self.assertRaisesRegex(ValueError, "sample budget"):
            stream_ephemeris({**self.scenario, "duration_ns": 250_001_000_000},
                             self.orbit, self.position, 1_000_000, self.output)
        with self.assertRaisesRegex(ValueError, "step_ns"):
            stream_ephemeris(self.scenario, self.orbit, self.position, 1001, self.output)
        with self.assertRaisesRegex(ValueError, "duplicate orbital"):
            stream_ephemeris(self.scenario, {**self.orbit, "satellites":
                             [{"id": "sat-a"}, {"id": "sat-a"}]},
                             self.position, 1000, self.output)
        self.assertEqual(self.calls, [])
        self.assertFalse(self.output.exists())

    def test_failed_provider_preserves_previous_artifact_and_cleans_temp(self):
        self.output.write_bytes(b"previous complete artifact\n")
        def broken(_satellite_id, at_ns):
            return (math.nan, 0, 0) if at_ns == 1_000_000_000 else (1, 2, 3)
        with self.assertRaisesRegex(ValueError, "invalid ITRS position"):
            stream_ephemeris(self.scenario, self.orbit, broken,
                             1_000_000_000, self.output)
        self.assertEqual(self.output.read_bytes(), b"previous complete artifact\n")
        self.assertEqual(list(self.output.parent.glob(".ntarlink-ephemeris-*")), [])

    def test_artifact_byte_limit_is_enforced_without_partial_output(self):
        with patch("ntarlink_orbital.ephemeris_stream.MAX_STREAM_BYTES", 100):
            with self.assertRaisesRegex(ValueError, "128 MB artifact limit"):
                stream_ephemeris(self.scenario, self.orbit, self.position,
                                 1_000_000_000, self.output)
        self.assertFalse(self.output.exists())
        self.assertEqual(list(self.output.parent.glob(".ntarlink-ephemeris-*")), [])

    def test_cli_manifest_digests_and_input_alias_protection(self):
        scenario_path = self.output.parent / "scenario.json"
        orbit_path = self.output.parent / "orbit.json"
        scenario_raw = (json.dumps(self.scenario) + "\n").encode()
        orbit_raw = (json.dumps(self.orbit) + "\n").encode()
        scenario_path.write_bytes(scenario_raw)
        orbit_path.write_bytes(orbit_raw)
        argv = [str(scenario_path), str(orbit_path), "--step-ns", "1000000000",
                "--out", str(self.output)]
        with patch("ntarlink_orbital.ephemeris_stream.build_sgp4_provider",
                   return_value=self.position), \
             patch("ntarlink_orbital.ephemeris_stream.version", return_value="test"):
            main(argv)
        manifest = json.loads(Path(str(self.output) + ".ephemeris-manifest.json").read_text())
        self.assertEqual(manifest["artifact_format"], "jsonl")
        self.assertEqual(manifest["generator"], "ntarlink_orbital.ephemeris_stream")
        self.assertEqual(manifest["sample_count"], 6)
        self.assertEqual(manifest["artifact_sha256"],
                         hashlib.sha256(self.output.read_bytes()).hexdigest())
        self.assertEqual(manifest["scenario_input_sha256"],
                         hashlib.sha256(scenario_raw).hexdigest())
        self.assertEqual(manifest["orbit_input_sha256"],
                         hashlib.sha256(orbit_raw).hexdigest())
        with self.assertRaisesRegex(ValueError, "must not overwrite"):
            main([str(scenario_path), str(orbit_path), "--step-ns", "1000000000",
                  "--out", str(scenario_path)])
        self.assertEqual(scenario_path.read_bytes(), scenario_raw)

    def test_historical_iss_real_provider_smoke(self):
        root = Path(__file__).resolve().parents[4]
        scenario_path = root / "scenarios/orbital-pass.json"
        orbit_path = root / "scenarios/orbit-sources.sample.json"
        main([str(scenario_path), str(orbit_path), "--step-ns", "5000000000",
              "--out", str(self.output)])
        rows = [json.loads(line) for line in self.output.read_text().splitlines()]
        manifest = json.loads(Path(str(self.output) + ".ephemeris-manifest.json").read_text())
        self.assertEqual(rows[0]["sample_count"], 240)
        self.assertEqual(len(rows), 241)
        self.assertEqual(rows[0]["satellite_ids"], ["iss-demo"])
        self.assertEqual(rows[1]["at_ns"], 0)
        self.assertEqual(rows[-1]["at_ns"], 1_195_000_000_000)
        self.assertTrue(all(6_000_000 < math.dist(row["position_m"], [0, 0, 0]) < 8_000_000
                            for row in rows[1:]))
        self.assertEqual(manifest["artifact_sha256"],
                         hashlib.sha256(self.output.read_bytes()).hexdigest())
        self.assertEqual(manifest["sample_count"], 240)


if __name__ == "__main__":
    unittest.main()
