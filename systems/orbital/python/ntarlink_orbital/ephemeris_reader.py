"""Verify bounded ITRS JSONL ephemerides before yielding any position samples.

A temporary disk snapshot prevents time-of-check/time-of-use changes and keeps
sample memory bounded. This reader does not interpolate or propagate orbits.
"""

from __future__ import annotations

import argparse
import hashlib
import hmac
import json
import math
import tempfile
from datetime import datetime
from pathlib import Path
from typing import Iterator

MAX_ARTIFACT_BYTES = 128_000_000
MAX_MANIFEST_BYTES = 1_000_000
MAX_LINE_BYTES = 16_000_000
MAX_SAMPLES = 500_000


def _integer(value, name: str, minimum: int = 0) -> int:
    if isinstance(value, bool) or not isinstance(value, int) or value < minimum:
        raise ValueError(f"{name} must be an integer >= {minimum}")
    return value


def _unique_object(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError(f"duplicate JSON key: {key}")
        result[key] = value
    return result


def _parse_json(raw: bytes, name: str) -> dict:
    try:
        value = json.loads(raw.decode("utf-8"), object_pairs_hook=_unique_object,
                           parse_constant=lambda value: (_ for _ in ()).throw(
                               ValueError(f"nonfinite JSON constant: {value}")))
    except (UnicodeError, json.JSONDecodeError) as exc:
        raise ValueError(f"invalid UTF-8 JSON in {name}") from exc
    if not isinstance(value, dict):
        raise ValueError(f"{name} must be a JSON object")
    return value


def _manifest(path: Path, artifact: Path) -> dict:
    with path.open("rb") as source:
        raw = source.read(MAX_MANIFEST_BYTES + 1)
    if len(raw) > MAX_MANIFEST_BYTES:
        raise ValueError("ephemeris manifest exceeds size limit")
    value = _parse_json(raw, "manifest")
    for key, expected in (
        ("schema_version", 1), ("generator", "ntarlink_orbital.ephemeris_stream"),
        ("artifact_format", "jsonl"), ("artifact", artifact.name),
        ("frame", "ITRS"), ("time_scale", "UTC"), ("time_resolution_ns", 1000),
    ):
        if type(value.get(key)) is not type(expected) or value[key] != expected:
            raise ValueError(f"manifest {key} mismatch")
    for key in ("sample_count", "satellite_count", "byte_count", "sample_step_ns"):
        _integer(value.get(key), f"manifest {key}", 1)
    if value["sample_count"] > MAX_SAMPLES or value["byte_count"] > MAX_ARTIFACT_BYTES:
        raise ValueError("ephemeris manifest exceeds resource budget")
    digest = value.get("artifact_sha256")
    if not isinstance(digest, str) or len(digest) != 64 or any(
        character not in "0123456789abcdef" for character in digest
    ):
        raise ValueError("invalid manifest artifact_sha256")
    return value


def _header(value: dict, manifest: dict) -> tuple[list[str], int, int]:
    for key, expected in (
        ("record_type", "header"), ("schema_version", 1), ("frame", "ITRS"),
        ("time_scale", "UTC"), ("position_unit", "m"),
        ("time_unit", "ns since epoch_utc"), ("time_resolution_ns", 1000),
        ("record_order", "satellite_id then at_ns ascending"),
    ):
        if type(value.get(key)) is not type(expected) or value[key] != expected:
            raise ValueError(f"header {key} mismatch")
    epoch = value.get("epoch_utc")
    if not isinstance(epoch, str) or not epoch.endswith("Z"):
        raise ValueError("header epoch_utc must be UTC")
    try:
        datetime.fromisoformat(epoch[:-1] + "+00:00")
    except ValueError as exc:
        raise ValueError("invalid header epoch_utc") from exc
    duration = _integer(value.get("duration_ns"), "duration_ns", 1)
    step = _integer(value.get("sample_step_ns"), "sample_step_ns", 1000)
    if step % 1000:
        raise ValueError("sample_step_ns must align to microseconds")
    ids = value.get("satellite_ids")
    if not isinstance(ids, list) or not ids or any(
        not isinstance(item, str) or not item for item in ids
    ) or ids != sorted(set(ids)):
        raise ValueError("header satellite_ids must be sorted and unique")
    count = _integer(value.get("sample_count"), "sample_count", 1)
    steps = (duration - 1) // step + 1
    if count != steps * len(ids) or count > MAX_SAMPLES:
        raise ValueError("header sample_count/grid mismatch or resource budget exceeded")
    for key, expected in (
        ("sample_count", count), ("satellite_count", len(ids)),
        ("sample_step_ns", step),
    ):
        if type(manifest.get(key)) is not type(expected) or manifest[key] != expected:
            raise ValueError(f"manifest {key} does not match header")
    return ids, steps, step


def _position(value: dict, ids: list[str], steps: int, step: int, index: int) -> None:
    if value.get("record_type") != "position" or index >= len(ids) * steps:
        raise ValueError("unexpected ephemeris position record")
    if value.get("satellite_id") != ids[index // steps]:
        raise ValueError("satellite order/catalog mismatch")
    if type(value.get("at_ns")) is not int or value["at_ns"] != index % steps * step:
        raise ValueError("ephemeris timestamp grid mismatch")
    vector = value.get("position_m")
    if not isinstance(vector, list) or len(vector) != 3 or any(
        isinstance(component, bool) or not isinstance(component, (int, float))
        or not math.isfinite(component) for component in vector
    ):
        raise ValueError("invalid finite ITRS position_m vector")


def iter_verified_positions(
    artifact_path: Path, manifest_path: Path | None = None,
) -> Iterator[dict]:
    """Yield positions only after full schema, grid, byte-count and SHA-256 validation.

    A bounded disk snapshot is verified before iteration, so changing the
    source after verification cannot change the yielded samples. Closing the
    iterator releases the snapshot; no positions are trusted on partial reads.
    """
    artifact = Path(artifact_path)
    manifest = _manifest(Path(manifest_path) if manifest_path is not None else
                         Path(str(artifact) + ".ephemeris-manifest.json"), artifact)
    digest = hashlib.sha256()
    byte_count = 0
    index = 0
    with tempfile.TemporaryFile(mode="w+b") as snapshot:
        with artifact.open("rb") as source:
            first = True
            while raw := source.readline(MAX_LINE_BYTES + 1):
                byte_count += len(raw)
                if (byte_count > MAX_ARTIFACT_BYTES or len(raw) > MAX_LINE_BYTES
                        or not raw.endswith(b"\n")):
                    raise ValueError("ephemeris JSONL size or newline limit exceeded")
                snapshot.write(raw)
                digest.update(raw)
                record = _parse_json(raw, "ephemeris JSONL record")
                if first:
                    ids, steps, step = _header(record, manifest)
                    first = False
                else:
                    _position(record, ids, steps, step, index)
                    index += 1
        if first or index != manifest["sample_count"]:
            raise ValueError("ephemeris JSONL record count mismatch")
        if byte_count != manifest["byte_count"] or not hmac.compare_digest(
            digest.hexdigest(), manifest["artifact_sha256"]
        ):
            raise ValueError("ephemeris artifact byte count or SHA-256 mismatch")
        snapshot.seek(0)
        snapshot.readline()  # Validated header.
        for raw in snapshot:
            yield json.loads(raw)


def main(argv=None):
    parser = argparse.ArgumentParser(description="Verify and read an ITRS ephemeris JSONL")
    parser.add_argument("artifact", type=Path)
    parser.add_argument("--manifest", type=Path)
    args = parser.parse_args(argv)
    count = sum(1 for _ in iter_verified_positions(args.artifact, args.manifest))
    print(f"verified {count} ITRS position samples in {args.artifact}")


if __name__ == "__main__":
    main()
