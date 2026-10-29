# SYS-05: analytical RF link budget and optional interference

This bounded subsystem consumes the **compiled SYS-01 contact artifact** rather
than calling an orbital propagator. Each directed user/feeder RF contact has a
positive `propagation_delay_ns` sampled at its geometric midpoint. The access
adapter converts that integer-nanosecond vacuum delay back into an approximate
range in meters, with at most ~0.15 m quantization error, then computes a
Friis/thermal-noise budget with optional configured in-band interference.

```sh
python -m pip install -e systems/orbital/python -e systems/access/python
python -m ntarlink_orbital.contacts \
  scenarios/orbital-pass.json scenarios/orbit-sources.sample.json \
  --out /tmp/ntarlink-orbital-pass.json
python -m ntarlink_access.rf_budget \
  /tmp/ntarlink-orbital-pass.json scenarios/rf-budgets.sample.json \
  --out /tmp/ntarlink-rf-budget.json
python -m ntarlink_access.rf_time_series \\
  /tmp/ntarlink-orbital-pass.json scenarios/rf-budgets.sample.json \\
  --out /tmp/ntarlink-rf-time-series.json
python -m unittest discover -s systems/access/python/tests
```

The JSON sidecar includes SHA-256 hashes of the exact compiled scenario and RF
configuration, direction, contact interval, midpoint sample, original delay,
derived range, FSPL, received power, thermal noise, C/N0, C/N, configured
reference C/N and reference margin. Optional aggregate received in-band
interference (dBm) is combined with thermal noise in linear power to report
C/(N+I), and an independent optional required SINR threshold yields an
illustrative SINR margin. No interference is assumed when that field is absent.
Inputs are explicit: carrier frequency
(Hz), occupied bandwidth (Hz), transmit power (dBm), directional antenna gains
(dBi), receiver noise figure (dB), other losses (dB), required C/N (dB).
The noise reference is 290 K with exact SI Boltzmann constant.

## Optional time-resolved RF analysis

The separate `python -m ntarlink_access.rf_time_series` command evaluates the
same Friis, thermal C/N and configured-interference SINR equations at **each
actual SYS-01 `propagation_delay_samples` timestamp**, rather than only at
contact midpoints. Its JSON sidecar includes `sample_ns`, contact bounds,
direction, source delay, derived geometric range, RF metrics, sample count and
SHA-256 input digests. This is a deterministic O(samples) batch transform;
the 500,000-sample scenario limit is also enforced by the adapter.

Every configured RF contact must have a complete, strictly ordered series
starting at `start_ns` and ending at `end_ns - 1`; missing or malformed
samples fail closed. The original midpoint RF command and Rust forwarding
remain unchanged. The time-resolved sidecar does not interpolate between RF
observations, simulate interference variations, predict PHY acquisition,
change packet capacity, or claim a continuously valid SINR. Sample times
are scenario-relative virtual nanoseconds, not Unix timestamps.

**Fidelity:** analytical only. Reference margin is not modem admission.
Interference is only an **exogenous, aggregate receiver-input power** assumption;
no beam overlap, antenna patterns, co-channel coupling, adjacent-channel
leakage, fading, rain attenuation (unless manually included as a static loss),
beam steering, spectrum allocation, MODCOD/BLER curve,
carrier recovery, RF waveform, or MAC scheduler. The configured interference
power must already be integrated over the receiver's occupied bandwidth; it
must not be specified as dBm/Hz, and receiver noise figure is applied only
to thermal noise. C/N and its legacy reference margin remain interference-free;
SINR and its separately configured reference threshold are independent.
Existing scenario link
capacity is **not changed** by these numbers. A contact is geometric
potential, not an acquired link. Results are midpoint snapshots, not
continuous time-series or calibrated Starlink performance. Computation scales
linearly with contact count, with O(1) arithmetic per sample; the adapter
never re-runs SGP4 or performs per-packet RF calculations.

See [SYS-05 design](../../docs/subsystems/05-access-network.md) and
[SYS-01 orbital contacts](../../scenarios/ORBITAL-README.md).

## Experimental deterministic MAC frame scheduler (Rust)

The standalone `systems/access/rust` crate (`ntarlink-access-scheduler`)
implements a first bounded **single-direction beam-frame allocator**. It
accepts weighted terminal queues, accounts for fixed per-packet wire overhead,
limits aggregate queued wire bytes and allocates at most
`floor(capacity_bps * frame_duration_ns / 8e9)` bytes per frame. Weighted
deficit round robin accrues credit for backlogged terminals once per frame,
caps credit at one frame, and rotates the first terminal to avoid fixed-order
bias. Stable terminal-ID ordering and integer arithmetic make repeated runs
deterministic. An oversized packet is rejected rather than fragmented.

```sh
cargo test -p ntarlink-access-scheduler
cargo clippy -p ntarlink-access-scheduler --all-targets -- -D warnings
```

The two-terminal saturated test demonstrates a 3:1 weighted grant split
within a 400-byte frame, queue accounting, bounded grants, tight-frame
alternation and eventual service for larger packets. This is a **standalone
L1 MAC scheduling primitive**, not yet connected to the Rust packet runtime,
the SYS-01 contact timeline, subscriber admission or actual RF/SINR outcomes.
It does not create service sessions, model control slots, implement BLER or
transmit RF signals. Each frame currently discards fractional-bit capacity;
unused bytes do not carry over. Per-frame work is proportional to active
terminals plus granted packets, with queue storage bounded in wire bytes.

## Opt-in shared-beam packet runtime bridge

`access_beams[]` can now bind multiple terminal-to-satellite uplink links
onto one shared finite beam resource in the Rust scenario. At a frame boundary,
SYS-05 issues weighted grants that drive packet hop start/completion times.
Shared queue overflow and contact-loss drops are accounted, while unconfigured
links keep legacy independent serialization. See
[beam-frame packet integration](../../docs/design/beam-frame-packet-bridge.md)
and `scenarios/beam-uplink-contention.json` for a reproducible example.
This is a modeled uplink MAC, not a modem, authenticated bearer or PHY.
