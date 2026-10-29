"""Reproducible synthetic throughput and memory baseline for SYS-01 JSONL artifacts.

This benchmark exercises serialization and verified disk-snapshot reading, not
SGP4 propagation or any real satellite constellation. Wall time is not a gate.
"""

from __future__ import annotations

import argparse
import json
import sys
import tempfile
import time
import tracemalloc
from pathlib import Path

from .ephemeris import MAX_SAMPLES
from .ephemeris_reader import iter_verified_positions
from .ephemeris_stream import stream_ephemeris

STEP_NS = 1_000_000


def _positive_integer(value: int, name: str) -> int:
    if isinstance(value, bool) or not isinstance(value, int) or value <= 0:
        raise ValueError(f"{name} must be a positive integer")
    return value


def benchmark(satellite_count: int, samples_per_satellite: int) -> dict:
    """Measure a deterministic synthetic producer and fully verified consumer.

    Both stages operate on the same bounded JSONL artifact. The measured
    consumer includes the full validation pass, disk snapshot and iteration.
    tracemalloc observes Python allocations, not native SGP4 or total RSS.
    """
    _positive_integer(satellite_count, "satellite_count")
    _positive_integer(samples_per_satellite, "samples_per_satellite")
    total = satellite_count * samples_per_satellite
    if total > MAX_SAMPLES:
        raise ValueError(f"ephemeris sample budget exceeded ({MAX_SAMPLES} pairs)")

    ids = [f"sat-{index:05d}" for index in range(satellite_count)]
    scenario = {
        "schema_version": 1,
        "duration_ns": samples_per_satellite * STEP_NS,
        "nodes": [{"kind": "satellite", "id": satellite_id} for satellite_id in ids],
    }
    orbit = {
        "epoch_utc": "2014-01-23T06:20:00Z",
        "satellites": [{"id": satellite_id} for satellite_id in ids],
    }

    def synthetic_position(satellite_id: str, at_ns: int) -> tuple[float, float, float]:
        satellite_index = int(satellite_id[4:])
        sample_index = at_ns // STEP_NS
        return (
            6_800_000.0 + satellite_index * 10 + sample_index % 1000,
            float(satellite_index),
            float(sample_index % 100),
        )

    with tempfile.TemporaryDirectory(prefix="ntarlink-ephemeris-benchmark-") as directory:
        artifact = Path(directory) / "synthetic-itrs.jsonl"
        tracemalloc.start()
        try:
            start = time.perf_counter_ns()
            export = stream_ephemeris(scenario, orbit, synthetic_position, STEP_NS, artifact)
            export_seconds = (time.perf_counter_ns() - start) / 1_000_000_000
            export_peak = tracemalloc.get_traced_memory()[1]
            manifest = {
                "schema_version": 1,
                "generator": "ntarlink_orbital.ephemeris_stream",
                "artifact_format": "jsonl",
                "artifact": artifact.name,
                "frame": "ITRS",
                "time_scale": "UTC",
                "time_resolution_ns": 1000,
                "sample_step_ns": STEP_NS,
                **export,
            }
            Path(str(artifact) + ".ephemeris-manifest.json").write_text(
                json.dumps(manifest, sort_keys=True) + "\n", encoding="utf-8"
            )
            tracemalloc.reset_peak()
            start = time.perf_counter_ns()
            verified_count = sum(1 for _ in iter_verified_positions(artifact))
            verified_read_seconds = (time.perf_counter_ns() - start) / 1_000_000_000
            verified_read_peak = tracemalloc.get_traced_memory()[1]
        finally:
            tracemalloc.stop()

        if verified_count != total or export["sample_count"] != total:
            raise RuntimeError("verified ephemeris count differs from requested sample count")
        if artifact.stat().st_size != export["byte_count"]:
            raise RuntimeError("ephemeris artifact byte count changed during benchmark")

    return {
        "schema_version": 1,
        "workload": "synthetic_itrs_jsonl_roundtrip",
        "fidelity": "synthetic coordinates; no SGP4, RF or orbit truth",
        "satellite_count": satellite_count,
        "samples_per_satellite": samples_per_satellite,
        "sample_count": total,
        "verified_sample_count": verified_count,
        "sample_step_ns": STEP_NS,
        "artifact_bytes": export["byte_count"],
        "artifact_sha256": export["artifact_sha256"],
        "export_wall_seconds": export_seconds,
        "verified_read_wall_seconds": verified_read_seconds,
        "export_samples_per_second": total / export_seconds,
        "verified_read_samples_per_second": total / verified_read_seconds,
        "export_python_heap_peak_bytes": export_peak,
        "verified_read_python_heap_peak_bytes": verified_read_peak,
        "python_version": sys.version.split()[0],
        "platform": sys.platform,
    }


def main(argv=None) -> None:
    parser = argparse.ArgumentParser(
        description="Measure synthetic ITRS JSONL export and verified read, without SGP4"
    )
    parser.add_argument("--satellites", type=int, required=True)
    parser.add_argument("--samples-per-satellite", type=int, required=True)
    parser.add_argument("--report", type=Path, help="Optional JSON report destination")
    args = parser.parse_args(argv)
    report = benchmark(args.satellites, args.samples_per_satellite)
    output = json.dumps(report, indent=2, sort_keys=True) + "\n"
    if args.report is not None:
        args.report.write_text(output, encoding="utf-8")
    print(output, end="")


if __name__ == "__main__":
    main()
