# SYS-01 bounded ITRS ephemeris artifact

The offline ephemeris exporter uses the **same pinned SGP4/Skyfield provider**
as the contact predictor. It publishes reproducible **position snapshots**,
not a new propagator, interpolator, independent TEME/ITRS implementation, or
physical proof of RF/optical acquisition.

## Generate a historical artifact

```sh
python -m pip install -e systems/orbital/python
python -m ntarlink_orbital.ephemeris \
  scenarios/orbital-pass.json scenarios/orbit-sources.sample.json \
  --step-ns 5000000000 --out /tmp/ntarlink-ephemeris.json
```

The JSON contract has `schema_version: 1`, `frame: "ITRS"`,
`time_scale: "UTC"`, `position_unit: "m"`, an absolute `epoch_utc`,
`duration_ns`, `sample_step_ns`, and stable satellite-ID-sorted
`satellites[].samples[]` entries with `at_ns` and `position_m: [x,y,z]`.
Times are integer nanoseconds relative to the UTC epoch; positions are
Earth-fixed ITRS meters at the specified sample time. The grid starts at
zero and includes multiples of the step **strictly before** the exclusive
scenario duration; it does not extrapolate to the final endpoint.

The provider converts nanoseconds through Python microsecond datetimes:
`sample_step_ns` must be a positive multiple of 1,000 ns and
`time_resolution_ns: 1000` is recorded. The exporter rejects missing or
duplicate satellite IDs, invalid time grids, nonfinite positions, and more
than 500,000 total satellite-time pairs **before propagation**. Unreferenced
orbit-config satellites are not exported.

The companion `.ephemeris-manifest.json` records SHA-256 hashes of the
exact scenario bytes, orbit-config bytes, and output artifact, plus pinned
Skyfield/SGP4 versions, frame, time scale and sample count. Identical
inputs, dependency versions and execution environment produce byte-stable
JSON; no network download is performed during propagation.

## Fidelity, integration and performance

This is an **offline analytical** artifact. It does not modify the Rust
packet runtime, contact schedules, route weights, RF budgets or beam grants.
The existing SYS-01 contact generator and this exporter share the same
Earth-fixed position provider. CI generates a real historical ISS artifact
and checks its time grid, ITRS units, finite orbital coordinates, changing
positions and input/output digests.

Memory and runtime grow linearly with the number of satellite-time pairs.
The 500,000-pair cap bounds accidental large exports, but is **not** a
measured constellation-scale performance benchmark. There is no streaming
or columnar export, interpolation error bound, velocity/attitude data,
independent EOP/UT1/polar-motion validation or light-time correction.
These remain separate acceptance milestones.
