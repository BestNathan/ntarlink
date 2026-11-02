# P0 packet-runtime performance baseline

## Scope and fidelity

This is an **L1 packet-discrete-event**, end-to-end **compiled CLI** benchmark,
not an RF/PHY experiment, a real-time satellite emulator, or an isolated
event-loop microbenchmark. It measures wall-clock process startup, JSON
serialization, packet simulation and report generation together. The simulated
nanosecond timestamps are virtual time and must never be interpreted as
measured execution latency.

The [benchmark harness](../../scripts/benchmark_packet_runtime.py) derives a
single-flow workload from [minimal.json](../../scenarios/minimal.json), keeping
its six-node, five-link topology, queue capacities, propagation delays and
routing unchanged. The default workload injects 1,000 packets at 1 ms virtual
intervals. The duration is extended to allow every injected packet to finish.
The source scenario is not modified. The generated scenario is canonicalized
and hashed before execution.

## Run

From the repository root:

~~~sh
cargo build --release -p ntarlink-cli
python3 -m unittest discover -s scripts/tests -v
python3 scripts/benchmark_packet_runtime.py \
  --binary target/release/ntarlink-cli \
  --packet-count 1000 --interval-ns 1000000 \
  --warmup 1 --samples 3 \
  --output packet-benchmark.json
~~~

The [benchmark workflow](../../.github/workflows/packet-benchmark.yml)
runs this release-mode smoke baseline on relevant PR/main changes and can also
be dispatched manually. The JSON result appears in the workflow logs and job
summary. The optional local output file is not a tracked benchmark fixture.

## Measurements and reproducibility

The output records the Git revision (when available), SHA-256 of the compiled
binary, SHA-256 of the generated scenario, Python/platform information, number
of warmup and measured runs, every measured wall duration in nanoseconds, the
minimum/median/maximum wall duration, median end-to-end packets per second,
event count and packet accounting. The SHA-256 of the canonicalized simulation
report must match across **all warmup and measured runs**. Every run must
account for all injected packets as delivered or dropped, with no in-flight
packets at the end of the scenario.

The harness fails closed on a nonzero CLI exit, invalid JSON, incomplete
accounting, nonpositive wall time, missing binary, invalid workload or
nondeterministic reports. Python unit tests exercise workload generation,
validation, digest stability, timing and the warmup exclusion rule.

## Interpretation and limits

These measurements provide a reproducible **method**, not a portable
performance SLA. Compare runs only when hardware, runner load, toolchain,
binary hash, input hash and sample count are controlled. GitHub-hosted runners
vary; no wall-time pass/fail threshold is imposed in CI. The report does not
measure peak RSS, energy, per-event latency or Rust-only scheduler throughput.
The benchmark is a first P0 packet-path baseline; additional scale points,
memory profiling, regressions against pinned hardware and separate PAT/fault
workloads remain future work.
