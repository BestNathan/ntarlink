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

## Isolated repeated benchmark and RSS

The separate suite repeats the same production exporter/verified-reader
roundtrip in **fresh Python interpreter processes**. Each child records the
whole-process resident-set high-water mark after completing both stages,
using `resource.getrusage(RUSAGE_SELF).ru_maxrss`. Linux KiB and macOS byte
units are normalized to bytes; unsupported platforms report JSON `null`
rather than an invented measurement. This is not stage-specific RSS and
does not include kernel page cache or a measured peak disk footprint.

    python -m ntarlink_orbital.ephemeris_benchmark_suite --satellites 10 --samples-per-satellite 100 --repetitions 3 --report /tmp/ephemeris-repeat-1k.json
    python -m ntarlink_orbital.ephemeris_benchmark_suite --satellites 100 --samples-per-satellite 1000 --repetitions 3 --report /tmp/ephemeris-repeat-100k.json
    python -m ntarlink_orbital.ephemeris_benchmark_suite --satellites 500 --samples-per-satellite 1000 --repetitions 3 --report /tmp/ephemeris-repeat-500k.json

The suite accepts 1 to 10 repetitions and enforces the original 500,000
sample budget **before** spawning any worker. It verifies the exact artifact
byte count and SHA-256 agree across repetitions, then reports individual
measurements, median export/read throughput and wall time, and maximum
whole-process peak RSS. Worker import/startup allocations count toward RSS
but not the inner benchmark wall times. Run this on pinned hardware with
documented Python version, filesystem, CPU and RAM for comparisons; do not
treat a small GitHub-hosted smoke run as a capacity guarantee. Child
processes have a 20-minute timeout to prevent indefinite benchmark hangs.

## CI and future work

CI runs 1,000 and 10,000 synthetic sample cases and checks report integrity
and positive measured values, not performance thresholds. The discovered
Python test suite additionally runs two small isolated repetitions and
checks deterministic digests, RSS availability on Linux, report output and
fail-fast invalid workloads. Larger 100,000/500,000 sample repeated cases
are opt-in and are **not** CI performance gates. The historical ISS
provider-to-reader contract remains a separate real-SGP4 correctness smoke
test.

Future work: collect repeated 100k/500k measurements on pinned hardware,
measure peak disk footprint separately, benchmark actual SGP4 propagation
at constellation scale, and evaluate columnar alternatives. These
measurements do not establish production scalability.
