# SYS-01 repeated-TLE SGP4/ITRS propagation benchmark

## Scope and fidelity

The opt-in `ntarlink_orbital.sgp4_scale_benchmark` measures the existing
production Skyfield/SGP4 provider, including TEME-to-ITRS conversion, for
a bounded grid of position requests. It **replicates** source TLE records
under unique satellite IDs. This deliberately tests computation with many
provider objects, **not** a physically valid constellation, independent
orbital truth, packet simulation, RF performance or link contact prediction.

The benchmark is separate from the synthetic JSONL serialization and verified
read suite. No output ephemeris file is generated; a SHA-256 digest covers
satellite IDs, time offsets and packed three-dimensional positions. A repeated
run on the same pinned environment can compare that digest, but a digest
is not an independently validated orbit reference.

## Usage

Install the repository's pinned orbital dependencies, then run:

```sh
python -m pip install -e systems/orbital/python
python -m ntarlink_orbital.sgp4_scale_benchmark scenarios/orbit-sources.sample.json --satellites 10 --samples-per-satellite 10 --report /tmp/sgp4-smoke.json
python -m ntarlink_orbital.sgp4_scale_benchmark scenarios/orbit-sources.sample.json --satellites 100 --samples-per-satellite 1000 --report /tmp/sgp4-100k.json
python -m ntarlink_orbital.sgp4_scale_benchmark scenarios/orbit-sources.sample.json --satellites 500 --samples-per-satellite 1000 --report /tmp/sgp4-500k.json
```

Each sample is separated by exactly one second from the UTC epoch in the
source configuration. The requested catalog is constructed by round-robin
replication of the input TLEs; a 1,000 satellite limit and 500,000 position
sample limit apply before provider construction. The original TLE freshness
policy, optional pinned Earth-orientation inputs and fail-closed position
validation are preserved. No network retrieval is performed by the provider.

The report contains a canonical source-configuration SHA-256, sample count,
position digest, build time, propagation time and samples per second.
Provider construction is timed separately. Propagation time includes Python
loops, Skyfield SGP4/TEME-to-ITRS conversion, finite-value validation and
incremental digest computation, but excludes building the provider and
writing the JSON report. These are single-process wall-clock measurements;
the command does not measure RSS, CPU utilization or peak disk usage.

## Verification and limitations

Unit tests use an injected provider to check replication, sample ordering,
stable digests, CLI output, input bounds and nonfinite vector rejection.
CI also runs a small **real historical ISS TLE** smoke workload through the
production provider and checks only count, frame and positive timing values.
Large 100k/500k runs are manual, not CI performance gates.

The sample is a historical ISS orbit, not a Starlink spacecraft or a
validated multi-satellite ephemeris. Duplicating the TLE does not model
constellation diversity or interaction. Timings on shared GitHub-hosted
runners are not capacity guarantees. Future work should collect repeated
measurements on pinned hardware, distinguish build/propagation RSS, and
benchmark independently sourced, validated multi-orbit catalogs.

## Isolated repeated measurement protocol

The optional `ntarlink_orbital.sgp4_scale_benchmark_suite` wraps the same
production SGP4/ITRS workload in a fresh Python interpreter per repetition.
It verifies that every run has the same source-configuration and position
SHA-256 digests, sample counts, satellite count and frame before reporting
median provider-build time, median propagation time, median propagation
throughput and the maximum process RSS high-water mark.

```sh
python -m ntarlink_orbital.sgp4_scale_benchmark_suite scenarios/orbit-sources.sample.json --satellites 2 --samples-per-satellite 2 --repetitions 2 --report /tmp/sgp4-suite-smoke.json
python -m ntarlink_orbital.sgp4_scale_benchmark_suite scenarios/orbit-sources.sample.json --satellites 100 --samples-per-satellite 1000 --repetitions 3 --report /tmp/sgp4-suite-100k.json
python -m ntarlink_orbital.sgp4_scale_benchmark_suite scenarios/orbit-sources.sample.json --satellites 500 --samples-per-satellite 1000 --repetitions 3 --report /tmp/sgp4-suite-500k.json
```

Work is limited to 1,000 replicated satellites, 500,000 positions per run
and 1-10 repetitions. Each worker has a 1,200-second subprocess timeout.
The report uses Linux `ru_maxrss` in KiB or macOS `ru_maxrss` in bytes,
normalizing both to bytes. On unsupported platforms RSS is null. This is
the **whole process** high-water mark including imports, provider creation
and propagation; it is not separate per-stage RSS or an incremental memory
delta. Timed provider-build and propagation sections exclude interpreter
startup, while RSS includes it. The suite stores no position ephemeris file
and does not measure peak disk or CPU usage.

The CI smoke repeats only a four-position historical ISS workload; 100k/500k
examples are explicitly manual and have not been executed as part of CI.
Replicated TLEs are not an independently sourced constellation catalog,
and no timing or memory result constitutes a hardware-independent SLA.
