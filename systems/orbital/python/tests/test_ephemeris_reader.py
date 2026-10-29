"""Fail-closed JSONL ephemeris reader tests and exporter round trip."""

import hashlib
import json
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from ntarlink_orbital.ephemeris_reader import iter_verified_positions, main


class EphemerisReaderTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.artifact = Path(self.temp.name) / "ephemeris.jsonl"
        self.manifest = Path(str(self.artifact) + ".ephemeris-manifest.json")
        self.header = {
            "record_type": "header", "schema_version": 1,
            "model": "SGP4/Skyfield geometric position snapshots",
            "frame": "ITRS", "time_scale": "UTC", "epoch_utc": "2014-01-23T06:20:00Z",
            "position_unit": "m", "time_unit": "ns since epoch_utc",
            "time_resolution_ns": 1000, "duration_ns": 2_500_000_000,
            "sample_step_ns": 1_000_000_000, "sample_count": 6,
            "satellite_ids": ["sat-a", "sat-b"],
            "record_order": "satellite_id then at_ns ascending",
        }
        self.positions = [
            {"record_type": "position", "satellite_id": satellite,
             "at_ns": at_ns, "position_m": [6_800_000.0 + i, 1.0, 0.0]}
            for i, (satellite, at_ns) in enumerate(
                (satellite, at_ns) for satellite in ("sat-a", "sat-b")
                for at_ns in (0, 1_000_000_000, 2_000_000_000)
            )
        ]
        self.write()

    def write(self):
        raw = b"".join((json.dumps(row, separators=(",", ":")) + "\n").encode()
                       for row in [self.header, *self.positions])
        self.artifact.write_bytes(raw)
        self.meta = {
            "schema_version": 1, "generator": "ntarlink_orbital.ephemeris_stream",
            "artifact_format": "jsonl", "artifact": self.artifact.name,
            "frame": "ITRS", "time_scale": "UTC", "time_resolution_ns": 1000,
            "sample_count": 6, "satellite_count": 2, "sample_step_ns": 1_000_000_000,
            "byte_count": len(raw), "artifact_sha256": hashlib.sha256(raw).hexdigest(),
        }
        self.manifest.write_text(json.dumps(self.meta))

    def read(self):
        return list(iter_verified_positions(self.artifact))

    def test_valid_order_and_cli(self):
        self.assertEqual(self.read(), self.positions)
        main([str(self.artifact)])

    def test_digest_rejects_tamper_before_first_yield(self):
        raw = self.artifact.read_bytes().replace(b"6800000.0", b"6800001.0", 1)
        self.artifact.write_bytes(raw)
        with self.assertRaisesRegex(ValueError, "SHA-256"):
            next(iter_verified_positions(self.artifact))

    def test_grid_order_missing_extra_and_bad_vectors(self):
        cases = [
            (lambda: self.positions[1].update(at_ns=500_000_000), "timestamp"),
            (lambda: self.positions[0].update(satellite_id="sat-b"), "catalog"),
            (lambda: self.positions[0].update(position_m=[1, True, 3]), "vector"),
            (lambda: self.positions[0].update(position_m=[1, 2]), "vector"),
            (lambda: self.positions.pop(), "count"),
            (lambda: self.positions.append(self.positions[-1].copy()), "unexpected"),
        ]
        for mutate, error in cases:
            with self.subTest(error=error):
                self.setUp()
                mutate()
                self.write()  # Re-pin the digest to ensure schema checks are independent.
                with self.assertRaisesRegex(ValueError, error):
                    self.read()

    def test_header_contract_rejects_invalid_frame_and_count(self):
        for field, value in (("frame", "TEME"), ("sample_count", 7),
                             ("satellite_ids", ["sat-b", "sat-a"]),
                             ("sample_step_ns", 1001)):
            with self.subTest(field=field):
                self.setUp()
                self.header[field] = value
                self.write()
                with self.assertRaises(ValueError):
                    self.read()

    def test_rejects_truncation_duplicate_json_key_and_missing_newline(self):
        raw = self.artifact.read_bytes()
        self.artifact.write_bytes(raw[:-1])
        with self.assertRaisesRegex(ValueError, "newline"):
            self.read()
        self.artifact.write_bytes(raw.replace(b'"record_type":"header"',
                                              b'"record_type":"header","record_type":"header"', 1))
        with self.assertRaisesRegex(ValueError, "duplicate JSON key"):
            self.read()

    def test_manifest_and_resource_budget(self):
        for key, value in (("artifact", "wrong.jsonl"), ("frame", "TEME"),
                           ("byte_count", 128_000_001), ("sample_count", True),
                           ("artifact_sha256", "bad")):
            with self.subTest(key=key):
                self.setUp()
                self.meta[key] = value
                self.manifest.write_text(json.dumps(self.meta))
                with self.assertRaises(ValueError):
                    self.read()
        self.setUp()
        self.manifest.write_bytes(b" " * 1_000_001)
        with self.assertRaisesRegex(ValueError, "size limit"):
            self.read()

    def test_verified_snapshot_is_immutable_after_source_mutation(self):
        reader = iter_verified_positions(self.artifact)
        self.assertEqual(next(reader), self.positions[0])
        self.artifact.write_bytes(b"corrupt source after verification")
        self.assertEqual(list(reader), self.positions[1:])

    def test_bounded_line_rejected(self):
        from unittest.mock import patch
        with patch("ntarlink_orbital.ephemeris_reader.MAX_LINE_BYTES", 20):
            with self.assertRaisesRegex(ValueError, "size or newline"):
                self.read()

    def test_exporter_roundtrip(self):
        try:
            from ntarlink_orbital.ephemeris_stream import stream_ephemeris
        except ModuleNotFoundError as exc:
            if exc.name in ("skyfield", "sgp4", "ntarlink_orbital.ephemeris_stream"):
                self.skipTest("pinned orbital dependencies unavailable locally")
            raise
        scenario = {"schema_version": 1, "duration_ns": 2_500_000_000,
                    "nodes": [{"kind": "satellite", "id": "sat-b"},
                              {"kind": "satellite", "id": "sat-a"}]}
        orbit = {"epoch_utc": "2014-01-23T06:20:00Z",
                 "satellites": [{"id": "sat-a"}, {"id": "sat-b"}]}
        result = stream_ephemeris(scenario, orbit,
                                  lambda satellite, at_ns: (6_800_000, 1, 0),
                                  1_000_000_000, self.artifact)
        self.meta.update(result)
        self.manifest.write_text(json.dumps(self.meta))
        positions = self.read()
        self.assertEqual(len(positions), 6)
        self.assertEqual(positions[0]["satellite_id"], "sat-a")


if __name__ == "__main__":
    unittest.main()
