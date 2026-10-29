"""Bounded, offline ITRS ephemeris snapshots for reproducible experiments.

The samples are Earth-fixed geometric positions, not interpolated trajectories,
spacecraft attitude, velocity vectors, or continuously valid link geometry.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import math
from importlib.metadata import version
from pathlib import Path
from typing import Callable

from .contacts import build_sgp4_provider, parse_utc

MAX_SAMPLES = 500_000
TIME_RESOLUTION_NS = 1_000  # Skyfield provider uses Python microsecond datetimes.


def _validate_request(scenario: dict, orbit_config: dict, step_ns: int):
    """Check the frame/time contract and bound work before propagation."""
    if scenario.get("schema_version") != 1:
        raise ValueError("scenario schema_version must be 1")
    duration_ns = scenario.get("duration_ns")
    if isinstance(duration_ns, bool) or not isinstance(duration_ns, int) or duration_ns <= 0:
        raise ValueError("duration_ns must be a positive integer")
    if (isinstance(step_ns, bool) or not isinstance(step_ns, int)
            or step_ns < TIME_RESOLUTION_NS or step_ns % TIME_RESOLUTION_NS):
        raise ValueError("step_ns must be a positive multiple of 1000 nanoseconds")
    parse_utc(orbit_config["epoch_utc"])

    scenario_ids = [node["id"] for node in scenario["nodes"] if node["kind"] == "satellite"]
    orbit_ids = [entry["id"] for entry in orbit_config["satellites"]]
    if not scenario_ids or any(not isinstance(x, str) or not x for x in scenario_ids):
        raise ValueError("scenario must contain named satellites")
    if len(set(scenario_ids)) != len(scenario_ids):
        raise ValueError("duplicate scenario satellite ID")
    if any(not isinstance(x, str) or not x for x in orbit_ids) or len(set(orbit_ids)) != len(orbit_ids):
        raise ValueError("missing or duplicate orbital satellite ID")
    missing = sorted(set(scenario_ids) - set(orbit_ids))
    if missing:
        raise ValueError(f"missing TLE for scenario satellites: {', '.join(missing)}")

    steps = (duration_ns - 1) // step_ns + 1
    if steps * len(scenario_ids) > MAX_SAMPLES:
        raise ValueError(f"ephemeris sample budget exceeded ({MAX_SAMPLES} satellite-time pairs)")

    return sorted(scenario_ids), duration_ns, steps


def compile_ephemeris(
    scenario: dict,
    orbit_config: dict,
    position: Callable[[str, int], tuple[float, float, float]],
    step_ns: int,
) -> dict:
    """Sample a deterministic half-open UTC grid of ITRS satellite positions."""
    satellite_ids, duration_ns, steps = _validate_request(scenario, orbit_config, step_ns)
    satellites = []
    for satellite_id in satellite_ids:
        samples = []
        for at_ns in range(0, duration_ns, step_ns):
            vector = tuple(position(satellite_id, at_ns))
            if (len(vector) != 3 or any(
                isinstance(value, bool) or not isinstance(value, (int, float))
                or not math.isfinite(value) for value in vector
            )):
                raise ValueError(f"invalid ITRS position for {satellite_id} at {at_ns} ns")
            samples.append({"at_ns": at_ns, "position_m": list(vector)})
        satellites.append({"id": satellite_id, "samples": samples})

    return {
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
        "satellites": satellites,
    }


def main(argv=None):
    parser = argparse.ArgumentParser(description="Offline ITRS satellite ephemeris exporter")
    parser.add_argument("scenario", type=Path)
    parser.add_argument("orbit_config", type=Path)
    parser.add_argument("--step-ns", type=int, required=True)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--manifest", type=Path)
    args = parser.parse_args(argv)

    scenario_bytes = args.scenario.read_bytes()
    orbit_bytes = args.orbit_config.read_bytes()
    scenario = json.loads(scenario_bytes)
    orbit_config = json.loads(orbit_bytes)
    # Validate the time grid and satellite catalog before invoking SGP4.
    _validate_request(scenario, orbit_config, args.step_ns)
    provider = build_sgp4_provider(
        orbit_config, scenario["duration_ns"], args.orbit_config.resolve().parent
    )
    document = compile_ephemeris(scenario, orbit_config, provider, args.step_ns)
    output = json.dumps(document, indent=2, allow_nan=False) + "\n"
    args.out.write_text(output, encoding="utf-8")

    manifest = {
        "schema_version": 1,
        "generator": "ntarlink_orbital.ephemeris",
        "artifact": args.out.name,
        "artifact_sha256": hashlib.sha256(output.encode("utf-8")).hexdigest(),
        "scenario_input_sha256": hashlib.sha256(scenario_bytes).hexdigest(),
        "orbit_input_sha256": hashlib.sha256(orbit_bytes).hexdigest(),
        "earth_orientation": provider.earth_orientation,
        "skyfield_version": version("skyfield"),
        "sgp4_version": version("sgp4"),
        "frame": "ITRS",
        "time_scale": "UTC",
        "time_resolution_ns": TIME_RESOLUTION_NS,
        "sample_step_ns": args.step_ns,
        "sample_count": document["sample_count"],
        "limitations": "discrete geometric positions; no interpolation accuracy, velocity, or multi-epoch independent EOP calibration",
    }
    manifest_path = args.manifest or Path(str(args.out) + ".ephemeris-manifest.json")
    manifest_path.write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    print(f"exported {document['sample_count']} ITRS position samples to {args.out}")


if __name__ == "__main__":
    main()
