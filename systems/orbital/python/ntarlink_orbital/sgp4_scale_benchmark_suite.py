"""Isolated, repeated production SGP4/ITRS throughput and process RSS measurements.

This measures replicated historical TLE computation, not a physical constellation.
Each repetition starts a fresh Python interpreter to isolate RSS high-water marks.
"""

from __future__ import annotations

import argparse
import json
import math
import statistics
import subprocess
import sys
import tempfile
from pathlib import Path

from .ephemeris import MAX_SAMPLES
from .ephemeris_benchmark_suite import (
    MAX_REPETITIONS, MAX_WORKER_SECONDS, _positive_int, _process_peak_rss_bytes,
)
from .sgp4_scale_benchmark import CATALOG_MODES, MAX_SATELLITES

WORKLOADS = {
    "replicated": "repeated_historical_tle_sgp4_itrs_positions",
    "unique": "supplied_unique_tle_sgp4_itrs_positions",
}


def _validate_workload(source: Path, satellites: int, samples: int,
                       repetitions: int, catalog_mode: str = "replicated") -> None:
    if catalog_mode not in CATALOG_MODES:
        raise ValueError(f"catalog_mode must be one of {CATALOG_MODES}")
    _positive_int(satellites, "satellites")
    _positive_int(samples, "samples_per_satellite")
    _positive_int(repetitions, "repetitions")
    if repetitions > MAX_REPETITIONS:
        raise ValueError(f"repetitions must be <= {MAX_REPETITIONS}")
    if satellites > MAX_SATELLITES or satellites * samples > MAX_SAMPLES:
        raise ValueError("SGP4 benchmark satellite or sample budget exceeded")
    catalog = json.loads(source.read_text(encoding="utf-8"))
    if not isinstance(catalog, dict) or not isinstance(catalog.get("satellites"), list):
        raise ValueError("orbit config must contain a satellite catalog")
    if not catalog["satellites"] or any(
        not isinstance(row, dict) or any(
            not isinstance(row.get(key), str) or not row[key]
            for key in ("tle_line1", "tle_line2")
        ) for row in catalog["satellites"]
    ):
        raise ValueError("orbit config must contain nonempty source TLE lines")
    if catalog_mode == "unique":
        pairs = [(row["tle_line1"], row["tle_line2"]) for row in catalog["satellites"]]
        if len(pairs) != satellites or len(set(pairs)) != satellites:
            raise ValueError("unique catalog mode requires exactly one distinct TLE per satellite")


def _run_worker(source: Path, satellites: int, samples: int,
                catalog_mode: str = "replicated") -> dict:
    with tempfile.TemporaryDirectory(prefix="ntarlink-sgp4-suite-") as directory:
        report = Path(directory) / "run.json"
        command = [
            sys.executable, "-m", "ntarlink_orbital.sgp4_scale_benchmark_suite",
            str(source), "--worker", "--satellites", str(satellites),
            "--samples-per-satellite", str(samples), "--catalog-mode", catalog_mode,
            "--report", str(report),
        ]
        subprocess.run(command, check=True, capture_output=True, text=True,
                       timeout=MAX_WORKER_SECONDS)
        return json.loads(report.read_text(encoding="utf-8"))


def _positive_finite(value: object) -> bool:
    return type(value) in (int, float) and math.isfinite(value) and value > 0


def run_suite(orbit_config: Path, satellites: int, samples_per_satellite: int,
              repetitions: int = 3, catalog_mode: str = "replicated") -> dict:
    """Validate a workload, repeat it in isolated processes, and aggregate medians.

    Timing excludes interpreter startup and imports; process RSS includes both.
    Linux VmRSS checkpoints are current values, not per-stage peaks or deltas.
    """
    source = Path(orbit_config).resolve()
    _validate_workload(source, satellites, samples_per_satellite, repetitions, catalog_mode)
    runs = [_run_worker(source, satellites, samples_per_satellite, catalog_mode)
            for _ in range(repetitions)]
    first = runs[0]
    sample_count = satellites * samples_per_satellite
    for run in runs:
        if (run.get("workload") != WORKLOADS[catalog_mode]
                or run.get("catalog_mode") != catalog_mode
                or run.get("frame") != "ITRS"
                or run.get("sample_count") != sample_count
                or run.get("satellite_count") != satellites
                or run.get("samples_per_satellite") != samples_per_satellite
                or run.get("source_config_sha256") != first.get("source_config_sha256")
                or run.get("position_digest_sha256") != first.get("position_digest_sha256")
                or run.get("source_tle_count") != first.get("source_tle_count")
                or run.get("unique_source_tle_count") != first.get("unique_source_tle_count")):
            raise RuntimeError("SGP4 repetitions disagree on deterministic workload or digest")
        if any(not _positive_finite(run.get(key)) for key in (
            "provider_build_wall_seconds", "propagation_wall_seconds",
            "propagation_samples_per_second",
        )):
            raise RuntimeError("SGP4 repetition has invalid timing metrics")
        digest = run["position_digest_sha256"]
        if not isinstance(digest, str) or len(digest) != 64 or any(
            c not in "0123456789abcdef" for c in digest
        ):
            raise RuntimeError("SGP4 repetition has invalid position digest")
    if (type(first.get("source_tle_count")) is not int
            or first["source_tle_count"] <= 0
            or type(first.get("unique_source_tle_count")) is not int
            or not 1 <= first["unique_source_tle_count"] <= first["source_tle_count"]
            or (catalog_mode == "unique" and
                first["unique_source_tle_count"] != satellites)):
        raise RuntimeError("SGP4 worker reported invalid source TLE cardinality")
    rss = [run.get("process_peak_rss_bytes") for run in runs]
    if any(value is not None and (type(value) is not int or value <= 0)
           for value in rss) or any((value is None) != (rss[0] is None) for value in rss):
        raise RuntimeError("SGP4 repetitions disagree on RSS measurement availability")

    checkpoints = (
        "rss_before_provider_bytes",
        "rss_after_provider_bytes",
        "rss_after_propagation_bytes",
    )
    checkpoint_available = first.get(checkpoints[0]) is not None
    for run in runs:
        values = [run.get(key) for key in checkpoints]
        if any((value is None) == checkpoint_available for value in values):
            raise RuntimeError("SGP4 repetitions disagree on RSS checkpoint availability")
        if any(value is not None and (type(value) is not int or value <= 0)
               for value in values):
            raise RuntimeError("SGP4 repetition has invalid RSS checkpoint")
        if (rss[0] is not None and
                any(value is not None and value > run["process_peak_rss_bytes"]
                    for value in values)):
            raise RuntimeError("SGP4 checkpoint exceeds process peak RSS")

    return {
        "schema_version": 1,
        "workload": "isolated_" + WORKLOADS[catalog_mode],
        "fidelity": ("distinct supplied TLEs; not independently validated orbit truth"
                     if catalog_mode == "unique"
                     else "replicated historical TLEs; not a physical constellation or orbit truth"),
        "catalog_mode": catalog_mode,
        "frame": "ITRS", "position_unit": "m", "time_scale": "UTC",
        "satellite_count": satellites,
        "samples_per_satellite": samples_per_satellite,
        "sample_count": sample_count,
        "source_tle_count": first["source_tle_count"],
        "unique_source_tle_count": first["unique_source_tle_count"],
        "source_config_sha256": first["source_config_sha256"],
        "position_digest_sha256": first["position_digest_sha256"],
        "repetitions": repetitions,
        "median_provider_build_wall_seconds": statistics.median(
            run["provider_build_wall_seconds"] for run in runs),
        "median_propagation_wall_seconds": statistics.median(
            run["propagation_wall_seconds"] for run in runs),
        "median_propagation_samples_per_second": statistics.median(
            run["propagation_samples_per_second"] for run in runs),
        "max_process_peak_rss_bytes": max(rss) if rss[0] is not None else None,
        "max_rss_before_provider_bytes": (
            max(run["rss_before_provider_bytes"] for run in runs)
            if checkpoint_available else None
        ),
        "max_rss_after_provider_bytes": (
            max(run["rss_after_provider_bytes"] for run in runs)
            if checkpoint_available else None
        ),
        "max_rss_after_propagation_bytes": (
            max(run["rss_after_propagation_bytes"] for run in runs)
            if checkpoint_available else None
        ),
        "rss_scope": "whole isolated worker process high-water RSS; includes imports and provider",
        "rss_checkpoint_scope": "Linux current VmRSS at phase boundaries; not phase peaks",
        "timing_scope": "provider build separate from SGP4/ITRS positions and hashing",
        "runs": runs,
    }


def main(argv=None) -> None:
    parser = argparse.ArgumentParser(
        description="Repeat real SGP4/ITRS position benchmarks in isolated Python workers"
    )
    parser.add_argument("orbit_config", type=Path)
    parser.add_argument("--satellites", type=int, required=True)
    parser.add_argument("--samples-per-satellite", type=int, required=True)
    parser.add_argument("--repetitions", type=int, default=3)
    parser.add_argument("--catalog-mode", choices=CATALOG_MODES, default="replicated")
    parser.add_argument("--report", type=Path)
    parser.add_argument("--worker", action="store_true", help=argparse.SUPPRESS)
    args = parser.parse_args(argv)
    if args.worker:
        from .sgp4_scale_benchmark import benchmark

        source = args.orbit_config.resolve()
        _validate_workload(source, args.satellites, args.samples_per_satellite, 1,
                           args.catalog_mode)
        output = benchmark(json.loads(source.read_text(encoding="utf-8")),
                           args.satellites, args.samples_per_satellite, source.parent,
                           args.catalog_mode)
        output["process_peak_rss_bytes"] = _process_peak_rss_bytes()
    else:
        output = run_suite(args.orbit_config, args.satellites,
                           args.samples_per_satellite, args.repetitions,
                           args.catalog_mode)
    rendered = json.dumps(output, sort_keys=True, indent=2, allow_nan=False) + "\n"
    if args.report is not None:
        args.report.write_text(rendered, encoding="utf-8")
    print(rendered, end="")


if __name__ == "__main__":
    main()
