# SYS-01 ephemeris fixed interior-probe interpolation audit

## Scope

The optional offline `ntarlink_orbital.ephemeris_audit` command checks
whether **linear interpolation** between adjacent ITRS position snapshots
agrees with the same pinned SGP4/Skyfield position provider at fixed
**held-out interior points** of every complete adjacent sample interval.
The default is one midpoint; optional 3- and 7-probe modes sample evenly
spaced interior points, including the midpoint. It reports both midpoint-only
and all-probe maximum/RMS three-dimensional position errors in meters,
globally and per satellite. The configured threshold is an
experiment-specific gate, **not**
a certified interpolation accuracy or a continuous bound.

This is an analytical reproducibility/quality probe, not a replacement for
the orbital propagator, an independent orbit truth source, or a PHY model.
It does not modify packet forwarding or orbital contact prediction.

## Reproduce the historical ISS check

```sh
python -m pip install -e systems/orbital/python
python -m ntarlink_orbital.ephemeris \
  scenarios/orbital-pass.json scenarios/orbit-sources.sample.json \
  --step-ns 5000000000 --out /tmp/ntarlink-ephemeris.json
python -m ntarlink_orbital.ephemeris_audit \
  scenarios/orbital-pass.json scenarios/orbit-sources.sample.json \
  /tmp/ntarlink-ephemeris.json \
  --source-manifest /tmp/ntarlink-ephemeris.json.ephemeris-manifest.json \
  --probes-per-interval 3 --max-probe-error-m 250 \
  --out /tmp/ntarlink-ephemeris-audit.json
```

The example threshold of 250 m is a permissive CI regression gate for the
historical ISS fixture, not a mission accuracy requirement. Tighten it only
after measuring the relevant orbit and sampling conditions.

## Contracts and limitations

- Input source manifest must match the **exact SHA-256 bytes** of the
  ephemeris artifact, scenario and orbit configuration. Satellite IDs,
  absolute UTC epoch and duration must match those source inputs.
- Only version 1, UTC, ITRS, meter-based position snapshots with a regular,
  half-open grid are accepted. The sample step must be a multiple of 2,000 ns
  so its midpoint is representable at the provider's microsecond resolution
  in default mode. With N probes per interval, the step must be divisible
  by 1,000 × (N+1) nanoseconds, so every interior probe is microsecond-aligned.
- A minimum of two samples per satellite and a maximum of 500,000
  satellite-time pairs, plus a separate cap of 500,000 total interior probes,
  are enforced before propagation. The ephemeris artifact and manifest are
  each limited to 128 MB. Malformed/nonfinite positions and irregular sample
  timestamps are rejected before the provider is invoked.
- Every complete adjacent sample pair has 1, 3 or 7 evenly spaced interior
  probes; the trailing partial interval after the final snapshot is **not**
  extrapolated. The gate uses the **maximum across all selected probes**.
  The legacy `--max-midpoint-error-m` CLI alias remains supported; use
  `--max-probe-error-m` for multi-probe mode. Reports preserve the original
  midpoint fields and add all-probe fields and counts. A report with an
  observed error greater than the threshold is still written, but the
  CLI exits nonzero.
- The same SGP4 provider and Earth-orientation configuration are used as
  the exporter. This measures linear interpolation self-consistency only.
  **No error between probes, independent ITRF accuracy, EOP provenance
  authentication, leap-second freshness or constellation-scale performance
  is established.** Production use needs denser probes or a mathematical
  error bound, plus independent external references.
