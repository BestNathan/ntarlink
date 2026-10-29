# Implementation status and current fidelity

## Operational P0 and P1 orbital-contact bridge

The existing P0 Rust packet simulator executes a deterministic packet journey across user RF, satellite optical ISL, feeder station, terrestrial backhaul and internet endpoint. The new **P1 orbital bridge** provides offline TLE/SGP4 propagation, Skyfield TEME-to-ITRS transformation, WGS84 elevation and Earth-occlusion geometry, sampled contact windows with optional bisection-refined boundaries and geometric propagation delay, plus optional per-contact analytical range-rate/Doppler observations in a separate sidecar.

**Implemented in code:** contact windows with half-open `[start_ns,end_ns)` bounds and optional midpoint geometric delay; `contact_mode: "scheduled"` prevents space links without visibility from being treated as always available; Rust contact transitions drive link state and delayed route recomputation. The same packet engine reports delivered and dropped packets as contacts open/close. Python runs offline and writes a versioned JSON scenario plus a provenance sidecar.

**Existing P0:** typed nodes/links/flows/faults, deterministic integer-nanosecond event scheduler, shortest-route FIB with controller delay, packet queues, propagation/serialization/processing delay, packet traces, JSON report and CLI.

### Provenance and accuracy

- Python dependency pins: `skyfield==1.55`, `sgp4==2.27`; zero network downloads during propagation (`timescale(builtin=True)`).
- Orbital input TLE and absolute historical `epoch_utc` are explicitly recorded and checked for freshness (configurable maximum 14 days from TLE epoch).
- Skyfield converts SGP4 TEME output to ITRS Earth-fixed coordinates before WGS84 elevation/occlusion calculations. Published 2014 ISS GCRS, WGS84 subpoint, topocentric altitude and range observables are now regression-tested with declared tolerances; this is not an independent EOP/ITRF transform reference (see [frame validation](design/orbital-frame-validation.md)).
- Contacts use a coarse visibility scan; optional bisection refines **detected** rise/set transitions to a configured tolerance, conservatively rounding into visibility. Two transitions within one sample interval can still be missed. The midpoint delay remains a **fixed route-planning weight**; optionally the Python generator emits bounded per-contact ECEF geometric delay samples and Rust interpolates delay **at each packet's transmission completion** using integer nanosecond arithmetic. With no samples, legacy midpoint behavior remains unchanged. This is a geometric approximation, not retarded-time/light-time iteration or a PHY group-delay model. See [delay design](design/time-varying-propagation.md).
- Optional per-link carrier reference frequencies produce midpoint line-of-sight range-rate and first-order one-way Doppler estimates in a separate JSON sidecar. Positive range rate means recession and negative received frequency shift. This does not affect Rust packet forwarding or model RF carrier synchronization.
- A geometric contact is a **candidate**, not proof of beam scheduling, modem synchronization, optical PAT acquisition or resource admission.
- Packet transmissions already admitted before a contact closes finish under the P0 `complete_after_departure` convention. Physical interruption and retransmission models remain future work.
- Linux IP, real PHY/MAC, BER, interference, handover signaling and satellite control remain unimplemented.

### Reproducible scenarios

| Scenario | Input | Verified behavior |
|---|---|---|
| `scenarios/minimal.json` | configured static links | 3 packets delivered over five hops |
| `scenarios/isl-failure.json` | configured link failure | reroute through backup ISL after control delay |
| `scenarios/visibility-handover.json` | synthetic, time-varying RF/ISL contact windows | deterministic contact opens/closes, temporary outage and alternative access route |
| `scenarios/orbital-pass.json` | historic public ISS TLE from Skyfield guide, 2014 epoch | Python generates real geometric contacts, Rust simulates internet delivery only within visibility intervals |

Read [historical pass instructions](../scenarios/ORBITAL-README.md) for the end-to-end command. This demo uses the ISS and a co-located laboratory ground site for reproducibility; it **does not** represent an operational Starlink constellation.

## First SYS-05 analytical RF budget

A separate `systems/access/python` package now consumes the compiled
SYS-01 scheduled-contact artifact, using each directed RF contact's midpoint
`propagation_delay_ns` to recover an approximate geometric range. It
calculates free-space path loss, received power, 290 K thermal noise, C/N0,
C/N and margin against an explicitly configured reference threshold.
The JSON output includes input SHA-256 provenance and per-contact samples.
An optional per-directed-link received aggregate in-band interference
power (dBm) now yields analytical C/(N+I), noise-plus-interference power
and a separately configured SINR reference margin. This interference is
**supplied**, not calculated from beams, channels, reuse or pointing.
The original Rust link capacity and packet decisions are unchanged.
This is **not** an RF waveform, physically predicted interference, beam
allocation or radio bearer acquisition; unconfigured losses remain absent.
See [access subsystem instructions](../systems/access/README.md).

## Time-resolved SYS-05 RF observations

An optional separate `ntarlink_access.rf_time_series` batch command now
evaluates the existing analytical Friis/C/N/SINR equations at every validated
SYS-01 geometric propagation-delay sample. The midpoint RF sidecar and
the Rust data plane remain unchanged. The output carries contact and
scenario-relative sample timestamps, directed link IDs, source delays and
SHA-256 input provenance. Incomplete or malformed RF contact series fail
closed, and the 500,000-sample bound is enforced. This is **discrete sampled
analysis**, not a live RF waveform, continuously interpolated SINR, physical
interference prediction, admission or MAC scheduling.

## Standalone SYS-05 beam-frame resource scheduler

The Rust `systems/access/rust` crate now models a finite-capacity,
single-direction beam-frame allocator using deterministic weighted deficit
round robin. Per-terminal packet queues consume a shared bounded wire-byte
budget, and frame grants account for fixed per-packet overhead. Unit tests
exercise two saturated terminals at 3:1 weight, per-frame byte conservation,
small-frame alternation, deferred large-packet service, invalid input and
stable terminal ordering. The crate is compiled and tested by the existing Rust workspace CI.
An **opt-in** shared-beam bridge now uses this allocator to issue uplink packet
grants; links without an access beam still use independent per-link capacity.
There is no PHY admission, beam steering, RF-derived capacity, subscriber
authorization or complete handover signaling.

## Optional SYS-05 shared-beam packet grant bridge

The opt-in `access_beams[]` scenario contract now connects SYS-05's
deterministic weighted beam-frame scheduler to Rust packet forwarding for
multiple terminal-to-satellite uplinks sharing one finite directional
resource. Packet grants serialize within a common frame budget and account
for framing overhead. Contact loss or link failure cancels ungranted
bearer packets, with bounded shared queue and deterministic drop reasons.
Unconfigured links retain their legacy independent capacity behavior.
See [beam-frame integration design](design/beam-frame-packet-bridge.md)
and `scenarios/beam-uplink-contention.json` for the cross-subsystem fixture.

This remains a **modeled L1 MAC** without RF-derived capacity, bearer
authorization, modem acquisition, downlink scheduling or handover signaling.

## Remaining subsystem work

P1 next: independent Earth-orientation reference vectors, orbit catalog/ephemeris output, physically calibrated time-resolved Doppler/PHY correction, beam/interference RF budget, MAC access/scheduler, optical PAT and spacecraft constraints. Per-packet sampled geometric delay is implemented, but signal travel-time iteration and continuous path prediction are not.

P2/P3: user session/core/IPAM, terrestrial routing emulation, realistic subscriber service and gateway handover, RF weather and interference, NOC/telemetry/security, Cesium console, large-scale event benchmarks and external SNS3/OpenSAND/OAI comparisons. The sixteen-system [engineering blueprint](subsystems/README.md) remains the overall target.

## Test commands

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
python -m pip install -e systems/orbital/python
python -m unittest discover -s systems/orbital/python/tests
python -m ntarlink_orbital.contacts scenarios/orbital-pass.json scenarios/orbit-sources.sample.json --out /tmp/orbital-run.json
cargo run -p ntarlink-cli -- simulate /tmp/orbital-run.json --json > /tmp/orbital-report.json
```
