# SYS-01 → L1 packet runtime: sampled geometric propagation delay

## Scope and fidelity

The orbital Python batch compiler optionally emits a bounded, time-resolved
**one-way vacuum geometric delay** for each directed user RF, feeder or optical
ISL contact. Rust evaluates this offline-generated model without calling
Python or Skyfield per packet. This is a P1 geometry-to-L1 packet timing
bridge, not a PHY waveform, atmospheric group delay, iterative retarded-time
signal solution or real Starlink implementation.

The existing contact `propagation_delay_ns` is retained as the **midpoint
route-planning weight**. Samples affect only the hop timing and trace; changing
a sample does not install a route or trigger controller convergence.

## Versioned scenario contract

The optional `propagation_delay_samples` array on each `ContactWindow`:

```json
{
  "link_id": "user-iss",
  "start_ns": 10000000000,
  "end_ns": 30000000000,
  "propagation_delay_ns": 1900000,
  "propagation_delay_samples": [
    {"at_ns": 10000000000, "delay_ns": 2200000},
    {"at_ns": 20000000000, "delay_ns": 1900000},
    {"at_ns": 29999999999, "delay_ns": 2400000}
  ]
}
```

All times are **integer nanoseconds relative to the scenario epoch**. A
sample must have positive delay, lie inside its half-open contact, and be
strictly later than the preceding sample. The first timestamp equals
`start_ns`, and the last equals `end_ns - 1`. A one-nanosecond contact can
have one sample. Sampled contacts require a positive midpoint delay. Rust
rejects malformed samples and more than 500,000 samples across a scenario.

The Python compiler accepts optional positive integer
`propagation_delay_sample_step_ns`. It evaluates Earth-fixed endpoint
positions at each requested time and derives delay as
`round(range_m / c × 1e9)` using the existing geometry helper. It always
includes the last representable instant of a contact. It refuses a sample
that is not geometrically visible rather than silently extrapolating through
an unverified interval. This means sample generation should use refined
contact boundaries, not an optimistic coarse setting edge.

The generated orbit manifest records the sample step, count, method,
limitations, TLE input digest and pinned library versions. Without the
optional setting, the compiler emits only the legacy midpoint delay.

## Packet timing and interpolation

For a packet accepted onto a link, the runtime first computes queue delay
and serialization. At **transmission completion** `t_tx`, it selects the
current contact's bracketing samples and linearly interpolates delay using
u128 integer products and nearest-nanosecond rounding:

```text
propagation_ns = round(d0 + (d1 - d0) * (t_tx - t0) / (t1 - t0))
arrived_ns = t_tx + propagation_ns + processing_ns
```

Before the first or after the last sample, delay clamps to the respective
endpoint value. This matters when an admitted transmission completes after
contact closure: the existing P0 `complete_after_departure` convention still
applies, and no sample is extrapolated beyond the contact.

Lookup is O(log samples_per_contact) using a binary partition. No
floating-point math occurs in the packet hot path. Packet traces expose the
effective `propagation_ns`; byte-for-byte replay remains deterministic.
Links without samples retain the existing fixed-delay behavior.

**Limitations:** discrete geometric samples can alias rapid path changes,
and linear interpolation has no physical error bound without additional
trajectory curvature checks. Endpoint positions are evaluated at the same
simulation time; an exact light-time solution would use retarded source/
receiver states. Route selection still uses midpoint weights; dynamic
reoptimization and time-dependent routing remain separate work.

## Verification and performance

- Rust contract tests reject malformed, out-of-order, zero or incomplete
  sample series and enforce the aggregate sample budget.
- Rust packet tests cover interpolation, clamping, packet completion timing,
  hop accounting and deterministic replay.
- Python synthetic ECEF tests verify sample timestamps and exact range/c
  values, optional compatibility, invalid configuration and budget limits.
- CI generates the historical ISS pass with five-second samples, validates
  manifest accounting and checks that the Rust packet report has distinct
  per-packet user-link propagation delays.

To reproduce:

```sh
cargo test --workspace
python -m pip install -e systems/orbital/python
python -m unittest discover -s systems/orbital/python/tests
python -m ntarlink_orbital.contacts scenarios/orbital-pass.json scenarios/orbit-sources.sample.json --out /tmp/orbital.json
cargo run -p ntarlink-cli -- simulate /tmp/orbital.json --json > /tmp/report.json
```

The sample cap bounds artifact memory and preprocessing. A future benchmark
should report contact/sample count, compiler wall time, Rust event throughput
and interpolation cost at 1K/10K satellites. No large-constellation
performance claim is made by the historical ISS fixture.
