"""Isolated-process repeatability and RSS protocol for synthetic SYS-01 JSONL I/O.

Each repetition launches a fresh interpreter so RSS high-water marks are not
carried across repetitions. This does not benchmark SGP4 propagation.
"""

from __future__ import annotations

import argparse
import json
import statistics
import subprocess
import sys
import tempfile
from pathlib import Path

MAX_REPETITIONS = 10
MAX_WORKER_SECONDS = 1200


def _positive_int(value: int, name: str) -> int:
    if type(value) is not int or value <= 0:
        raise ValueError(f"{name} must be a positive integer")
    return value


def _process_peak_rss_bytes() -> int | None:
    """Return whole-process RSS high-water mark, not per-stage resident memory.

    Linux reports ru_maxrss in KiB; macOS reports bytes. Other platforms are
    intentionally unsupported rather than assigning an incorrect unit.
    """
    if sys.platform not in ("linux", "darwin"):
        return None
    import resource

    value = int(resource.getrusage(resource.RUSAGE_SELF).ru_maxrss)
    return value * 1024 if sys.platform == "linux" else value


def _run_worker(satellites: int, samples: int) -> dict:
    """Execute one real producer/reader pair in a separate Python process."""
    with tempfile.TemporaryDirectory(prefix="ntarlink-benchmark-run-") as directory:
        report = Path(directory) / "run.json"
        command = [
            sys.executable, "-m", "ntarlink_orbital.ephemeris_benchmark_suite",
            "--worker", "--satellites", str(satellites),
            "--samples-per-satellite", str(samples), "--report", str(report),
        ]
        subprocess.run(command, check=True, capture_output=True, text=True,
                       timeout=MAX_WORKER_SECONDS)
        return json.loads(report.read_text(encoding="utf-8"))


def run_suite(satellites: int, samples_per_satellite: int, repetitions: int = 3) -> dict:
    """Repeat a bounded synthetic JSONL roundtrip in isolated processes.

    Report medians rather than using wall time as a CI gate. Each repetition
    includes startup/import overhead in its process RSS, but wall times are
    measured inside the existing benchmark after imports.
    """
    _positive_int(satellites, "satellites")
    _positive_int(samples_per_satellite, "samples_per_satellite")
    _positive_int(repetitions, "repetitions")
    if repetitions > MAX_REPETITIONS:
        raise ValueError(f"repetitions must be <= {MAX_REPETITIONS}")
    from .ephemeris import MAX_SAMPLES

    if satellites * samples_per_satellite > MAX_SAMPLES:
        raise ValueError(f"ephemeris sample budget exceeded ({MAX_SAMPLES} pairs)")

    runs = [_run_worker(satellites, samples_per_satellite) for _ in range(repetitions)]
    first = runs[0]
    expected_count = satellites * samples_per_satellite
    required_metrics = (
        "export_wall_seconds", "verified_read_wall_seconds",
        "export_samples_per_second", "verified_read_samples_per_second",
        "export_python_heap_peak_bytes", "verified_read_python_heap_peak_bytes",
    )
    for run in runs:
        if (run.get("sample_count") != expected_count
                or run.get("verified_sample_count") != expected_count
                or run.get("satellite_count") != satellites
                or run.get("samples_per_satellite") != samples_per_satellite
                or run.get("artifact_sha256") != first.get("artifact_sha256")
                or run.get("artifact_bytes") != first.get("artifact_bytes")):
            raise RuntimeError("benchmark runs disagree on deterministic artifact contract")
        if any(type(run.get(metric)) not in (int, float) or run[metric] <= 0
               for metric in required_metrics):
            raise RuntimeError("benchmark run has invalid measured metrics")

    rss_values = [run.get("process_peak_rss_bytes") for run in runs]
    if any(value is not None and (type(value) is not int or value <= 0)
           for value in rss_values):
        raise RuntimeError("benchmark run has invalid RSS measurement")
    if any((value is None) != (rss_values[0] is None) for value in rss_values):
        raise RuntimeError("benchmark runs disagree on RSS availability")

    return {
        "schema_version": 1,
        "workload": "synthetic_itrs_jsonl_isolated_repetitions",
        "fidelity": "synthetic coordinates; no SGP4 propagation or orbit truth",
        "satellite_count": satellites,
        "samples_per_satellite": samples_per_satellite,
        "sample_count": expected_count,
        "repetitions": repetitions,
        "artifact_bytes": first["artifact_bytes"],
        "artifact_sha256": first["artifact_sha256"],
        "median_export_samples_per_second": statistics.median(
            run["export_samples_per_second"] for run in runs),
        "median_verified_read_samples_per_second": statistics.median(
            run["verified_read_samples_per_second"] for run in runs),
        "median_export_wall_seconds": statistics.median(
            run["export_wall_seconds"] for run in runs),
        "median_verified_read_wall_seconds": statistics.median(
            run["verified_read_wall_seconds"] for run in runs),
        "max_process_peak_rss_bytes": max(rss_values) if rss_values[0] is not None else None,
        "rss_scope": "whole worker process high-water RSS; not per-stage or page-cache usage",
        "runs": runs,
    }


def main(argv=None) -> None:
    parser = argparse.ArgumentParser(
        description="Repeat synthetic JSONL export/read in isolated Python workers"
    )
    parser.add_argument("--satellites", type=int, required=True)
    parser.add_argument("--samples-per-satellite", type=int, required=True)
    parser.add_argument("--repetitions", type=int, default=3)
    parser.add_argument("--report", type=Path)
    parser.add_argument("--worker", action="store_true", help=argparse.SUPPRESS)
    args = parser.parse_args(argv)
    if args.worker:
        from .ephemeris_benchmark import benchmark

        output = benchmark(args.satellites, args.samples_per_satellite)
        output["process_peak_rss_bytes"] = _process_peak_rss_bytes()
    else:
        output = run_suite(args.satellites, args.samples_per_satellite, args.repetitions)
    rendered = json.dumps(output, indent=2, sort_keys=True) + "\n"
    if args.report is not None:
        args.report.write_text(rendered, encoding="utf-8")
    print(rendered, end="")


if __name__ == "__main__":
    main()
