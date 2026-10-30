"""Bounded ITRS SGP4 throughput benchmark using repeated, explicitly nonphysical TLEs.

The benchmark measures the production Skyfield position provider, not a real
constellation, orbital accuracy, contact detection or JSONL I/O.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import struct
import sys
import time
from pathlib import Path

from .contacts import build_sgp4_provider
from .ephemeris import MAX_SAMPLES, TIME_RESOLUTION_NS, _validate_request

STEP_NS = 1_000_000_000
MAX_SATELLITES = 1_000
CATALOG_MODES = ("replicated", "unique")


def _current_rss_bytes() -> int | None:
    """Sample current Linux VmRSS, not a phase peak or an allocation delta.

    A missing, malformed or unsupported procfs measurement is reported as
    unavailable rather than silently substituting a process high-water mark.
    """
    if sys.platform != "linux":
        return None
    try:
        status = Path("/proc/self/status").read_text(encoding="ascii")
    except OSError:
        return None
    for line in status.splitlines():
        if line.startswith("VmRSS:"):
            fields = line.split()
            if len(fields) != 3 or fields[2] != "kB":
                return None
            try:
                kib = int(fields[1])
            except ValueError:
                return None
            return kib * 1024 if kib > 0 else None
    return None


def _positive_int(value: int, name: str) -> int:
    if type(value) is not int or value <= 0:
        raise ValueError(f"{name} must be a positive integer")
    return value


def benchmark(orbit_config: dict, satellite_count: int, samples_per_satellite: int,
              config_dir: Path | None = None, catalog_mode: str = "replicated") -> dict:
    """Measure production SGP4/TEME-to-ITRS calls with a bounded repeated-TLE catalog.

    Source TLEs are replicated under unique IDs to isolate compute scaling.
    Replicas are *not* independent satellites or physically valid geometry.
    """
    if catalog_mode not in CATALOG_MODES:
        raise ValueError(f"catalog_mode must be one of {CATALOG_MODES}")
    _positive_int(satellite_count, "satellite_count")
    _positive_int(samples_per_satellite, "samples_per_satellite")
    if satellite_count > MAX_SATELLITES or satellite_count * samples_per_satellite > MAX_SAMPLES:
        raise ValueError("SGP4 benchmark satellite or sample budget exceeded")
    if not isinstance(orbit_config, dict) or not isinstance(orbit_config.get("satellites"), list):
        raise ValueError("orbit_config must contain a satellite catalog")
    sources = orbit_config["satellites"]
    if not sources:
        raise ValueError("orbit_config must contain at least one source TLE")
    for source in sources:
        if not isinstance(source, dict) or any(
            not isinstance(source.get(key), str) or not source[key]
            for key in ("tle_line1", "tle_line2")
        ):
            raise ValueError("each source must contain two nonempty TLE lines")

    tle_pairs = [(source["tle_line1"], source["tle_line2"]) for source in sources]
    unique_source_count = len(set(tle_pairs))
    if catalog_mode == "unique" and (
        len(sources) != satellite_count or unique_source_count != satellite_count
    ):
        raise ValueError("unique catalog mode requires exactly one distinct TLE per satellite")

    ids = [f"benchmark-sat-{index:05d}" for index in range(satellite_count)]
    config = {**orbit_config, "satellites": [
        {"id": identifier,
         "tle_line1": sources[index % len(sources)]["tle_line1"],
         "tle_line2": sources[index % len(sources)]["tle_line2"]}
        for index, identifier in enumerate(ids)
    ]}
    duration_ns = samples_per_satellite * STEP_NS
    scenario = {"schema_version": 1, "duration_ns": duration_ns,
                "nodes": [{"id": identifier, "kind": "satellite"} for identifier in ids]}
    _validate_request(scenario, config, STEP_NS)

    rss_before_provider = _current_rss_bytes()
    build_start = time.perf_counter_ns()
    position = build_sgp4_provider(config, duration_ns, config_dir)
    build_seconds = (time.perf_counter_ns() - build_start) / 1e9
    rss_after_provider = _current_rss_bytes()
    digest = hashlib.sha256()
    propagation_start = time.perf_counter_ns()
    for identifier in ids:
        digest.update(identifier.encode("ascii") + b"\0")
        for at_ns in range(0, duration_ns, STEP_NS):
            vector = tuple(position(identifier, at_ns))
            if len(vector) != 3 or any(
                type(component) not in (float, int) or not math.isfinite(component)
                for component in vector
            ):
                raise ValueError(f"nonfinite or invalid ITRS position: {identifier} at {at_ns}")
            digest.update(struct.pack("!qddd", at_ns, *vector))
    propagation_seconds = (time.perf_counter_ns() - propagation_start) / 1e9
    rss_after_propagation = _current_rss_bytes()
    sample_count = satellite_count * samples_per_satellite
    return {
        "schema_version": 1,
        "workload": ("supplied_unique_tle_sgp4_itrs_positions" if catalog_mode == "unique"
                     else "repeated_historical_tle_sgp4_itrs_positions"),
        "fidelity": ("distinct supplied TLE records; not independently validated orbit truth"
                     if catalog_mode == "unique"
                     else "replicated source TLEs; not a physical constellation or orbit truth"),
        "catalog_mode": catalog_mode,
        "frame": "ITRS", "time_scale": "UTC", "position_unit": "m",
        "time_resolution_ns": TIME_RESOLUTION_NS,
        "sample_step_ns": STEP_NS,
        "source_tle_count": len(sources),
        "unique_source_tle_count": unique_source_count,
        "satellite_count": satellite_count,
        "samples_per_satellite": samples_per_satellite,
        "sample_count": sample_count,
        "epoch_utc": config["epoch_utc"],
        "source_config_sha256": hashlib.sha256(
            json.dumps(orbit_config, sort_keys=True, allow_nan=False).encode("utf-8")
        ).hexdigest(),
        "position_digest_sha256": digest.hexdigest(),
        "rss_before_provider_bytes": rss_before_provider,
        "rss_after_provider_bytes": rss_after_provider,
        "rss_after_propagation_bytes": rss_after_propagation,
        "rss_checkpoint_scope": "Linux current VmRSS at phase boundaries; not phase peaks",
        "provider_build_wall_seconds": build_seconds,
        "propagation_wall_seconds": propagation_seconds,
        "propagation_samples_per_second": sample_count / max(propagation_seconds, 1e-9),
        "timing_scope": "provider build separate; per-position Skyfield SGP4 and ITRS, hash included",
    }


def main(argv=None) -> None:
    parser = argparse.ArgumentParser(description="Benchmark repeated-TLE SGP4/ITRS positions")
    parser.add_argument("orbit_config", type=Path)
    parser.add_argument("--satellites", type=int, required=True)
    parser.add_argument("--samples-per-satellite", type=int, required=True)
    parser.add_argument("--catalog-mode", choices=CATALOG_MODES, default="replicated")
    parser.add_argument("--report", type=Path)
    args = parser.parse_args(argv)
    source = json.loads(args.orbit_config.read_text(encoding="utf-8"))
    report = benchmark(source, args.satellites, args.samples_per_satellite,
                       args.orbit_config.resolve().parent, args.catalog_mode)
    rendered = json.dumps(report, sort_keys=True, indent=2) + "\n"
    if args.report is not None:
        args.report.write_text(rendered, encoding="utf-8")
    print(rendered, end="")


if __name__ == "__main__":
    main()
