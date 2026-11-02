"""Reproducible end-to-end packet simulator throughput benchmark.

This measures a compiled CLI including JSON serialization and process startup,
not isolated Rust event-loop throughput. Virtual time is never wall time.
"""
from __future__ import annotations

import argparse
import copy
import hashlib
import json
import os
import platform
import statistics
import subprocess
import sys
import tempfile
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
DEFAULT_SCENARIO = ROOT / "scenarios/minimal.json"


def canonical_bytes(value: object) -> bytes:
    return json.dumps(value, sort_keys=True, separators=(",", ":"), allow_nan=False).encode("utf-8")


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def make_workload(base: dict, packet_count: int, interval_ns: int) -> dict:
    """Scale only the single-flow baseline without changing its topology."""
    if not 1 <= packet_count <= 1_000_000:
        raise ValueError("packet_count must be between 1 and 1,000,000")
    if not 1 <= interval_ns < 2**64:
        raise ValueError("interval_ns must be a positive u64")
    if len(base.get("flows", [])) != 1:
        raise ValueError("benchmark requires exactly one flow")
    scenario = copy.deepcopy(base)
    flow = scenario["flows"][0]
    last_injection = flow["start_ns"] + (packet_count - 1) * interval_ns
    if last_injection >= 2**64 - 1_000_000_000:
        raise ValueError("workload virtual-time range overflows u64")
    scenario["name"] = "packet-throughput-benchmark"
    scenario["duration_ns"] = max(scenario["duration_ns"], last_injection + 1_000_000_000)
    flow["packet_count"] = packet_count
    flow["interval_ns"] = interval_ns
    return scenario


def validate_report(report: dict, packet_count: int) -> None:
    injected = report["injected"]
    delivered = report["delivered"]
    dropped = report["dropped"]
    packets = report["packets"]
    if (
        injected != packet_count
        or delivered + dropped != injected
        or len(packets) != injected
        or report["processed_events"] <= 0
        or any(packet["status"] not in ("delivered", "dropped") for packet in packets)
    ):
        raise ValueError("packet benchmark accounting or completion invariant failed")


def run_command(command: list[str]) -> tuple[str, int | None, int | None, int | None]:
    """Capture a child's own peak RSS and CPU usage on Linux.

    wait4 reports resource usage for this exact PID, unlike cumulative
    RUSAGE_CHILDREN or a /proc polling loop that can miss short-lived CLIs.
    Redirect output to temporary files to avoid a pipe deadlock before wait4.
    Other platforms retain wall-time measurements and report resources as null.
    """
    if sys.platform != "linux" or not hasattr(os, "wait4"):
        completed = subprocess.run(command, check=True, capture_output=True, text=True)
        return completed.stdout, None, None, None

    with tempfile.TemporaryFile(mode="w+t", encoding="utf-8") as stdout:
        with tempfile.TemporaryFile(mode="w+t", encoding="utf-8") as stderr:
            process = subprocess.Popen(command, stdout=stdout, stderr=stderr, text=True)
            try:
                _, status, usage = os.wait4(process.pid, 0)
            except BaseException:
                process.wait()
                raise
            process.returncode = os.waitstatus_to_exitcode(status)
            stdout.seek(0)
            stderr.seek(0)
            output, errors = stdout.read(), stderr.read()
            if process.returncode != 0:
                raise subprocess.CalledProcessError(
                    process.returncode, command, output=output, stderr=errors,
                )
            return (
                output,
                int(usage.ru_maxrss),  # Linux reports ru_maxrss in KiB.
                round(usage.ru_utime * 1_000_000_000),
                round(usage.ru_stime * 1_000_000_000),
            )


def run_once(
    binary: Path, scenario_path: Path, packet_count: int,
) -> tuple[int, str, dict, int | None, int | None, int | None]:
    started = time.perf_counter_ns()
    output, peak_rss_kib, user_cpu_ns, system_cpu_ns = run_command(
        [str(binary), "simulate", str(scenario_path), "--json"],
    )
    elapsed_ns = time.perf_counter_ns() - started
    if elapsed_ns <= 0:
        raise ValueError("packet benchmark wall time must be positive")
    report = json.loads(output)
    validate_report(report, packet_count)
    return (
        elapsed_ns, sha256(canonical_bytes(report)), report,
        peak_rss_kib, user_cpu_ns, system_cpu_ns,
    )


def git_revision() -> str:
    try:
        return subprocess.check_output(
            ["git", "rev-parse", "HEAD"], cwd=ROOT, text=True,
            stderr=subprocess.DEVNULL,
        ).strip()
    except (OSError, subprocess.CalledProcessError):
        return "unavailable"


def benchmark(
    binary: Path, source: Path, packet_count: int, interval_ns: int,
    warmup: int, samples: int,
) -> dict:
    if warmup < 0 or not 1 <= samples <= 100:
        raise ValueError("warmup must be nonnegative and samples must be between 1 and 100")
    if not binary.is_file():
        raise ValueError(f"compiled CLI not found: {binary}")
    workload = make_workload(json.loads(source.read_text(encoding="utf-8")), packet_count, interval_ns)
    encoded = json.dumps(workload, sort_keys=True, indent=2, allow_nan=False) + "\n"
    with tempfile.TemporaryDirectory(prefix="ntarlink-packet-benchmark-") as directory:
        scenario_path = Path(directory) / "workload.json"
        scenario_path.write_text(encoded, encoding="utf-8")
        measurements = [
            run_once(binary, scenario_path, packet_count)
            for _ in range(warmup + samples)
        ]
    reference_digest = measurements[0][1]
    if any(measurement[1] != reference_digest for measurement in measurements):
        raise ValueError("packet benchmark report differs between repeated runs")
    timings = [measurement[0] for measurement in measurements[warmup:]]
    peak_rss = [measurement[3] for measurement in measurements[warmup:]]
    user_cpu = [measurement[4] for measurement in measurements[warmup:]]
    system_cpu = [measurement[5] for measurement in measurements[warmup:]]
    median_ns = statistics.median(timings)
    report = measurements[0][2]
    return {
        "schema_version": 2,
        "measurement": "compiled_cli_wall_time_including_json_and_process_startup",
        "resource_measurement": (
            "linux_wait4_per_child" if sys.platform == "linux" and hasattr(os, "wait4")
            else "unavailable_on_this_platform"
        ),
        "binary_sha256": sha256(binary.read_bytes()),
        "git_revision": git_revision(),
        "python_version": platform.python_version(),
        "platform": platform.platform(),
        "workload": {
            "source_scenario": source.name,
            "scenario_sha256": sha256(encoded.encode("utf-8")),
            "packet_count": packet_count,
            "interval_ns": interval_ns,
            "duration_ns": workload["duration_ns"],
        },
        "warmup_runs": warmup,
        "sample_runs": samples,
        "wall_time_ns": timings,
        "peak_rss_kib": peak_rss,
        "median_peak_rss_kib": (
            statistics.median(peak_rss) if all(value is not None for value in peak_rss)
            else None
        ),
        "user_cpu_ns": user_cpu,
        "system_cpu_ns": system_cpu,
        "median_wall_time_ns": median_ns,
        "min_wall_time_ns": min(timings),
        "max_wall_time_ns": max(timings),
        "median_packets_per_second": round(packet_count * 1_000_000_000 / median_ns, 2),
        "deterministic_report_sha256": reference_digest,
        "processed_events": report["processed_events"],
        "injected": report["injected"],
        "delivered": report["delivered"],
        "dropped": report["dropped"],
    }


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=ROOT / "target/release/ntarlink-cli")
    parser.add_argument("--scenario", type=Path, default=DEFAULT_SCENARIO)
    parser.add_argument("--packet-count", type=int, default=1000)
    parser.add_argument("--interval-ns", type=int, default=1_000_000)
    parser.add_argument("--warmup", type=int, default=1)
    parser.add_argument("--samples", type=int, default=3)
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    result = benchmark(
        args.binary.resolve(), args.scenario.resolve(), args.packet_count,
        args.interval_ns, args.warmup, args.samples,
    )
    serialized = json.dumps(result, indent=2, sort_keys=True) + "\n"
    if args.output:
        args.output.write_text(serialized, encoding="utf-8")
    print(serialized, end="")


if __name__ == "__main__":
    try:
        main()
    except (ValueError, OSError, subprocess.CalledProcessError, json.JSONDecodeError) as exc:
        print(f"packet benchmark failed: {exc}", file=sys.stderr)
        raise SystemExit(1) from exc
