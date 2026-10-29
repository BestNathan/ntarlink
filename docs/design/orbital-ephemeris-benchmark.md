# SYS-01 synthetic JSONL scale baseline

## Purpose and fidelity

This opt-in benchmark measures the existing bounded ITRS JSONL producer and
fail-closed verified reader under controlled synthetic satellite/time grids.
It is an offline serialization, validation and disk-I/O benchmark, **not**
SGP4 propagation, real orbital accuracy, a Starlink constellation model,
or an end-to-end packet/network performance measurement.

Run from the repository root after installing the pinned orbital package:

    python -m pip install -e systems/orbital/python
    python -m ntarlink_orbital.ephemeris_benchmark --satellites 10 --samples-per-satellite 100 --report /tmp/ephemeris-1k.json
    python -m ntarlink_orbital.ephemeris_benchmark --satellites 100 --samples-per-satellite 100 --report /tmp/ephemeris-10k.json
    python -m ntarlink_orbital.ephemeris_benchmark --satellites 100 --samples-per-satellite 1000 --report /tmp/ephemeris-100k.json

Every invocation creates a temporary JSONL artifact and matching SHA-256
manifest, then reads and verifies every record with the production reader.
Both stages run in one process; the temporary directory is removed after
verification. The input satellite IDs, 1 ms UTC-relative grid and synthetic
ITRS meter vectors are deterministic. The exporter and reader enforce the
same 500,000 satellite-time-pair cap and 128 MB artifact limit as production.

## Report contract and interpretation

The JSON report includes schema version, workload/fidelity labels, satellite
and sample counts, verified count, artifact byte size and SHA-256, wall-clock
seconds and sample/s for each stage, and peak traced Python heap bytes.
Producer timing includes JSONL serialization and file writing. Consumer
timing includes full schema/digest validation, a temporary disk snapshot,
and decoding/iteration of all verified position records. The temporary
snapshot can require approximately one additional artifact-sized disk
allocation while the original artifact is present; this is an analytical
upper-bound estimate, not measured peak disk usage.

Peak values from tracemalloc cover Python allocations only, not process
RSS, allocator fragmentation, OS page cache or native Skyfield/SGP4 memory.
Stage peaks include any live Python allocations already present when that
stage begins. Reported wall times are host- and I/O-dependent, and
throughput should be compared only under equivalent hardware, Python
version, workload and filesystem conditions. Do not assert exact timing
or memory thresholds from shared GitHub-hosted runners.

## CI and future work

CI runs 1,000 and 10,000 synthetic sample cases and checks report integrity
and positive measured values, not performance thresholds. A 100,000-sample
case is available on demand for scaling experiments. The test suite covers
round trips, deterministic output hashes, workload scaling and fail-fast
resource validation. The historical ISS provider-to-reader contract remains
a separate real-SGP4 correctness smoke test.

Future work: collect repeated 100k/500k measurements on pinned hardware
with separate process RSS and peak disk metrics, benchmark actual SGP4
propagation at constellation scale, and evaluate columnar alternatives.
These measurements do not establish production scalability.
