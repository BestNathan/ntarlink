"""Bounded, deterministic JSONL streaming of offline ITRS position snapshots.

The output is a sequence of discrete geometric samples, not an interpolator,
continuous accuracy guarantee, orbit truth source, or packet-runtime input.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import os
import tempfile
from importlib.metadata import version
from pathlib import Path
from typing import Callable

from .contacts import build_sgp4_provider
from .ephemeris import TIME_RESOLUTION_NS, _validate_request

MAX_STREAM_BYTES = 128_000_000


def _position_vector(position, satellite_id: str, at_ns: int) -> list[float]:
    try:
        values = tuple(position(satellite_id, at_ns))
    except (TypeError, ValueError) as exc:
        raise ValueError(f"invalid ITRS position for {satellite_id} at {at_ns} ns") from exc
    if (len(values) != 3 or any(
        isinstance(value, bool) or not isinstance(value, (int, float))
        or not math.isfinite(value) for value in values
    )):
        raise ValueError(f"invalid ITRS position for {satellite_id} at {at_ns} ns")
    return [float(value) for value in values]


def stream_ephemeris(
    scenario: dict,
    orbit_config: dict,
    position: Callable[[str, int], tuple[float, float, float]],
    step_ns: int,
    output_path: Path,
) -> dict:
    """Write a satellite-major JSONL artifact without retaining sample arrays.

    The output is atomically replaced only after all samples are validated.
    The exact SHA-256 is computed incrementally over the written UTF-8 bytes.
    """
    satellite_ids, duration_ns, steps = _validate_request(scenario, orbit_config, step_ns)
    output_path = Path(output_path)
    header = {
        "record_type": "header",
        "schema_version": 1,
        "model": "SGP4/Skyfield geometric position snapshots",
        "frame": "ITRS",
        "time_scale": "UTC",
        "epoch_utc": orbit_config["epoch_utc"],
        "position_unit": "m",
        "time_unit": "ns since epoch_utc",
        "time_resolution_ns": TIME_RESOLUTION_NS,
        "duration_ns": duration_ns,
        "sample_step_ns": step_ns,
        "sample_count": steps * len(satellite_ids),
        "satellite_ids": satellite_ids,
        "record_order": "satellite_id then at_ns ascending",
    }
    digest = hashlib.sha256()
    byte_count = 0
    temporary_path = None
    try:
        with tempfile.NamedTemporaryFile(
            mode="wb", dir=output_path.parent, prefix=".ntarlink-ephemeris-",
            suffix=".jsonl.tmp", delete=False,
        ) as stream:
            temporary_path = Path(stream.name)

            def emit(record: dict):
                nonlocal byte_count
                line = (json.dumps(record, separators=(",", ":"), allow_nan=False)
                        + "\n").encode("utf-8")
                if byte_count + len(line) > MAX_STREAM_BYTES:
                    raise ValueError("streaming ephemeris exceeds 128 MB artifact limit")
                stream.write(line)
                digest.update(line)
                byte_count += len(line)

            emit(header)
            for satellite_id in satellite_ids:
                for at_ns in range(0, duration_ns, step_ns):
                    emit({
                        "record_type": "position",
                        "satellite_id": satellite_id,
                        "at_ns": at_ns,
                        "position_m": _position_vector(position, satellite_id, at_ns),
                    })
        os.replace(temporary_path, output_path)
    finally:
        if temporary_path is not None:
            temporary_path.unlink(missing_ok=True)
    return {
        "sample_count": header["sample_count"],
        "satellite_count": len(satellite_ids),
        "byte_count": byte_count,
        "artifact_sha256": digest.hexdigest(),
    }


def main(argv=None):
    parser = argparse.ArgumentParser(description="Stream offline ITRS ephemeris as JSONL")
    parser.add_argument("scenario", type=Path)
    parser.add_argument("orbit_config", type=Path)
    parser.add_argument("--step-ns", type=int, required=True)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--manifest", type=Path)
    args = parser.parse_args(argv)
    manifest_path = args.manifest or Path(str(args.out) + ".ephemeris-manifest.json")
    source_paths = {args.scenario.resolve(), args.orbit_config.resolve()}
    if (args.out.resolve() in source_paths or manifest_path.resolve() in source_paths
            or args.out.resolve() == manifest_path.resolve()):
        raise ValueError("ephemeris output and manifest must not overwrite inputs or each other")

    scenario_raw = args.scenario.read_bytes()
    orbit_raw = args.orbit_config.read_bytes()
    scenario = json.loads(scenario_raw)
    orbit_config = json.loads(orbit_raw)
    _validate_request(scenario, orbit_config, args.step_ns)
    provider = build_sgp4_provider(
        orbit_config, scenario["duration_ns"], args.orbit_config.resolve().parent
    )
    result = stream_ephemeris(scenario, orbit_config, provider, args.step_ns, args.out)
    manifest = {
        "schema_version": 1,
        "generator": "ntarlink_orbital.ephemeris_stream",
        "artifact": args.out.name,
        "artifact_format": "jsonl",
        **result,
        "scenario_input_sha256": hashlib.sha256(scenario_raw).hexdigest(),
        "orbit_input_sha256": hashlib.sha256(orbit_raw).hexdigest(),
        "earth_orientation": getattr(
            provider, "earth_orientation", {"mode": "external_position_provider"}
        ),
        "skyfield_version": version("skyfield"),
        "sgp4_version": version("sgp4"),
        "frame": "ITRS",
        "time_scale": "UTC",
        "time_resolution_ns": TIME_RESOLUTION_NS,
        "sample_step_ns": args.step_ns,
        "limitations": "discrete same-provider geometric positions; no interpolation bound or independent orbit truth",
    }
    manifest_path.write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    print(f"streamed {result['sample_count']} ITRS position samples to {args.out}")


if __name__ == "__main__":
    main()
