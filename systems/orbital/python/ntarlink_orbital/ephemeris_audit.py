"""Held-out midpoint audit for offline ITRS ephemeris interpolation.

This tests linear interpolation against the same SGP4 provider at selected
instants. It is not an independent orbit reference or a continuous error bound.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import math
from pathlib import Path

MAX_ARTIFACT_BYTES = 128_000_000
MAX_PAIRS = 500_000
MAX_PROBES = 500_000


def _digest(raw: bytes) -> str:
    return hashlib.sha256(raw).hexdigest()


def _vector(value, label: str) -> tuple[float, float, float]:
    if (not isinstance(value, list) or len(value) != 3
            or any(isinstance(x, bool) or not isinstance(x, (int, float))
                   or not math.isfinite(x) for x in value)):
        raise ValueError(f"invalid ITRS position: {label}")
    return tuple(float(x) for x in value)


def audit_interpolation(document: dict, position, max_midpoint_error_m: float,
                        probes_per_interval: int = 1) -> dict:
    """Audit fixed interior probes on every complete sample interval."""
    if (isinstance(max_midpoint_error_m, bool)
            or not isinstance(max_midpoint_error_m, (int, float))
            or not math.isfinite(max_midpoint_error_m) or max_midpoint_error_m <= 0):
        raise ValueError("max_midpoint_error_m must be finite and positive")
    if isinstance(probes_per_interval, bool) or probes_per_interval not in (1, 3, 7):
        raise ValueError("probes_per_interval must be 1, 3, or 7")
    if (document.get("schema_version") != 1 or document.get("frame") != "ITRS"
            or document.get("time_scale") != "UTC"
            or document.get("position_unit") != "m"
            or document.get("time_resolution_ns") != 1000):
        raise ValueError("unsupported ITRS ephemeris contract")
    step = document.get("sample_step_ns")
    duration = document.get("duration_ns")
    quantum = 1000 * (probes_per_interval + 1)
    if (isinstance(step, bool) or not isinstance(step, int) or step < quantum
            or step % quantum != 0):
        raise ValueError(f"sample_step_ns must be a positive multiple of {quantum} for probes")
    if isinstance(duration, bool) or not isinstance(duration, int) or duration <= 0:
        raise ValueError("duration_ns must be positive")
    satellites = document.get("satellites")
    if not isinstance(satellites, list) or not satellites:
        raise ValueError("ephemeris requires satellites")
    expected_count = (duration - 1) // step + 1
    if expected_count < 2 or expected_count * len(satellites) > MAX_PAIRS:
        raise ValueError("ephemeris midpoint audit requires 2+ samples and at most 500000 pairs")
    if (expected_count - 1) * len(satellites) * probes_per_interval > MAX_PROBES:
        raise ValueError("ephemeris audit exceeds 500000 interior probe limit")
    if document.get("sample_count") != expected_count * len(satellites):
        raise ValueError("ephemeris sample_count mismatch")
    ids = [s.get("id") for s in satellites if isinstance(s, dict)]
    if (len(ids) != len(satellites) or any(not isinstance(x, str) or not x for x in ids)
            or ids != sorted(set(ids))):
        raise ValueError("ephemeris satellite IDs must be unique and sorted")

    # Validate the complete snapshot before invoking an expensive propagator.
    validated = []
    for sat in satellites:
        samples = sat.get("samples")
        if not isinstance(samples, list) or len(samples) != expected_count:
            raise ValueError("ephemeris sample grid length mismatch")
        vectors = []
        for index, sample in enumerate(samples):
            if (not isinstance(sample, dict)
                    or isinstance(sample.get("at_ns"), bool)
                    or sample.get("at_ns") != index * step):
                raise ValueError("ephemeris sample grid is not regular")
            vectors.append(_vector(sample.get("position_m"), sat["id"]))
        validated.append((sat["id"], vectors))

    results = []
    midpoint_sq = 0.0
    probe_sq = 0.0
    midpoint_max = 0.0
    probe_max = 0.0
    midpoint_index = (probes_per_interval + 1) // 2
    for sat_id, vectors in validated:
        midpoint_errors = []
        probe_errors = []
        for i in range(len(vectors) - 1):
            for k in range(1, probes_per_interval + 1):
                at_ns = i * step + (step // (probes_per_interval + 1)) * k
                actual = _vector(list(position(sat_id, at_ns)), sat_id)
                fraction = k / (probes_per_interval + 1)
                predicted = [a + (b - a) * fraction
                             for a, b in zip(vectors[i], vectors[i + 1])]
                error = math.dist(actual, predicted)
                if not math.isfinite(error):
                    raise ValueError("nonfinite interpolation error")
                probe_errors.append(error)
                if k == midpoint_index:
                    midpoint_errors.append(error)
        sat_midpoint_sq = sum(e * e for e in midpoint_errors)
        sat_probe_sq = sum(e * e for e in probe_errors)
        midpoint_sq += sat_midpoint_sq
        probe_sq += sat_probe_sq
        midpoint_max = max(midpoint_max, max(midpoint_errors))
        probe_max = max(probe_max, max(probe_errors))
        results.append({"id": sat_id, "midpoint_count": len(midpoint_errors),
                        "max_error_m": max(midpoint_errors),
                        "rms_error_m": math.sqrt(sat_midpoint_sq / len(midpoint_errors)),
                        "probe_count": len(probe_errors),
                        "max_probe_error_m": max(probe_errors),
                        "rms_probe_error_m": math.sqrt(sat_probe_sq / len(probe_errors))})
    midpoint_count = sum(x["midpoint_count"] for x in results)
    probe_count = midpoint_count * probes_per_interval
    return {"schema_version": 1, "model": "held-out linear interpolation interior probe audit",
            "frame": "ITRS", "error_unit": "m", "sample_step_ns": step,
            "probes_per_interval": probes_per_interval, "probe_count": probe_count,
            "max_probe_error_m": probe_max,
            "rms_probe_error_m": math.sqrt(probe_sq / probe_count),
            "midpoint_count": midpoint_count, "max_midpoint_error_m": midpoint_max,
            "rms_midpoint_error_m": math.sqrt(midpoint_sq / midpoint_count),
            "threshold_m": float(max_midpoint_error_m),
            "passed": probe_max <= max_midpoint_error_m,
            "limitations": "same SGP4 provider; fixed interior probes only; no continuous error guarantee",
            "satellites": results}


def main(argv=None):
    parser = argparse.ArgumentParser(description="Audit ITRS ephemeris interpolation at held-out interior probes")
    parser.add_argument("scenario", type=Path)
    parser.add_argument("orbit_config", type=Path)
    parser.add_argument("ephemeris", type=Path)
    parser.add_argument("--source-manifest", type=Path, required=True)
    parser.add_argument("--max-midpoint-error-m", "--max-probe-error-m",
                        dest="max_probe_error_m", type=float, required=True)
    parser.add_argument("--probes-per-interval", type=int, choices=(1, 3, 7), default=1)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args(argv)
    for path in (args.ephemeris, args.source_manifest):
        if path.stat().st_size > MAX_ARTIFACT_BYTES:
            raise ValueError("ephemeris artifact or manifest exceeds 128 MB")
    scenario_raw = args.scenario.read_bytes()
    orbit_raw = args.orbit_config.read_bytes()
    ephemeris_raw = args.ephemeris.read_bytes()
    manifest = json.loads(args.source_manifest.read_text(encoding="utf-8"))
    if (manifest.get("generator") != "ntarlink_orbital.ephemeris"
            or manifest.get("artifact_sha256") != _digest(ephemeris_raw)
            or manifest.get("scenario_input_sha256") != _digest(scenario_raw)
            or manifest.get("orbit_input_sha256") != _digest(orbit_raw)):
        raise ValueError("ephemeris source manifest provenance mismatch")
    scenario = json.loads(scenario_raw)
    orbit_config = json.loads(orbit_raw)
    document = json.loads(ephemeris_raw)
    if (document.get("epoch_utc") != orbit_config.get("epoch_utc")
            or document.get("duration_ns") != scenario.get("duration_ns")):
        raise ValueError("ephemeris epoch or duration differs from source inputs")
    expected_ids = sorted(node["id"] for node in scenario["nodes"]
                          if node["kind"] == "satellite")
    actual_ids = [sat.get("id") for sat in document.get("satellites", [])]
    if actual_ids != expected_ids:
        raise ValueError("ephemeris satellite IDs differ from source scenario")
    # Import only after the cheap contract and provenance checks.
    from .contacts import build_sgp4_provider
    provider = build_sgp4_provider(orbit_config, document["duration_ns"],
                                   args.orbit_config.resolve().parent)
    result = audit_interpolation(document, provider, args.max_probe_error_m,
                                 args.probes_per_interval)
    result.update({"ephemeris_sha256": _digest(ephemeris_raw),
                   "scenario_input_sha256": _digest(scenario_raw),
                   "orbit_input_sha256": _digest(orbit_raw),
                   "earth_orientation": getattr(provider, "earth_orientation", None)})
    args.out.write_text(json.dumps(result, indent=2, allow_nan=False) + "\n", encoding="utf-8")
    print(f"Audited {result['probe_count']} ITRS interior probes: "
          f"max error {result['max_probe_error_m']:.6f} m; "
          f"passed={result['passed']}")
    if not result["passed"]:
        raise SystemExit(1)


if __name__ == "__main__":
    main()
