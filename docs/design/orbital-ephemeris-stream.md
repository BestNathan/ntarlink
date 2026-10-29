# SYS-01 bounded streaming ITRS ephemeris (JSONL)

## Scope and usage

The optional `ntarlink_orbital.ephemeris_stream` CLI writes **discrete**
ITRS/ECEF position snapshots in satellite-major, timestamp-ascending JSONL
order. Unlike the original nested JSON exporter, it does not retain all
samples in Python memory. It reuses the pinned SGP4/Skyfield position provider,
UTC grid validation, EOP configuration and 500,000 satellite-time-pair budget.

```sh
python -m pip install -e systems/orbital/python
python -m ntarlink_orbital.ephemeris_stream \
  scenarios/orbital-pass.json scenarios/orbit-sources.sample.json \
  --step-ns 5000000000 --out /tmp/iss-itrs.jsonl
```

The output starts with one `record_type: "header"` JSON object, followed
by exactly `sample_count` `record_type: "position"` objects, one per line.
The header includes `schema_version: 1`, `frame: "ITRS"`,
`time_scale: "UTC"`, `position_unit: "m"`, `epoch_utc`,
`duration_ns`, `sample_step_ns`, `time_resolution_ns: 1000`,
sorted `satellite_ids` and `record_order`. Each position record includes
`satellite_id`, relative integer `at_ns` and `position_m: [x, y, z]`.
The time grid is half-open: `0 <= at_ns < duration_ns`, with no
extrapolated endpoint.

This is a distinct JSONL contract; the original `ephemeris` exporter and
`ephemeris_audit` JSON input remain unchanged. The audit cannot directly
consume JSONL and no downstream packet-runtime integration is implied.

## Reproducibility, failure handling and performance

- Each emitted UTF-8 line has deterministic compact JSON encoding, followed
  by LF. The companion `*.ephemeris-manifest.json` records exact artifact,
  scenario and orbit-source SHA-256 digests, dependency versions, frame and
  EOP provenance, sample count, byte count and output format.
- The exporter checks the satellite catalog, UTC epoch, step alignment and
  total sample budget before propagation. It rejects malformed/nonfinite
  coordinates and caps the final artifact at 128 MB.
- A temporary file in the destination directory is replaced atomically only
  after all position samples pass validation. A failed provider, budget check
  or write leaves an existing artifact intact and removes the temporary file.
  The companion manifest is written after the artifact; consumers must verify
  both digests before trusting a completed export.
- Streaming reduces sample-storage memory from O(satellite-time pairs) to
  O(satellite catalog + one JSON record), excluding the SGP4 provider's own
  bounded cache. Runtime and output bytes remain O(satellite-time pairs).
  This is a complexity claim, **not** a measured constellation-scale
  performance benchmark.

The historical ISS CLI is executed in Python CI, alongside deterministic
ordering, input/output SHA-256, malformed vector, failure cleanup, size cap
and alias-protection tests. The existing JSON exporter is not replaced.

## Fidelity limits and follow-up

JSONL records are **sampled analytical ITRS positions**, not continuous
trajectories or independently verified orbit truth. Neither a continuous
interpolation error bound nor externally certified IERS/leap-second source
is established. Follow-up work includes a streaming reader with schema and
digest validation, independent frame references, scalable benchmarks and
optional columnar formats with explicit memory/performance measurements.
