# Implementation status and current fidelity

## Operational P0 and P1 orbital-contact bridge

The existing P0 Rust packet simulator executes a deterministic packet journey across user RF, satellite optical ISL, feeder station, terrestrial backhaul and internet endpoint. The new **P1 orbital bridge** provides offline TLE/SGP4 propagation, Skyfield TEME-to-ITRS transformation, WGS84 elevation and Earth-occlusion geometry, sampled contact windows with optional bisection-refined boundaries and propagation delay, plus optional per-contact analytical range-rate/Doppler observations in a separate sidecar.

**Implemented in code:** contact windows with half-open `[start_ns,end_ns)` bounds and optional midpoint geometric delay; `contact_mode: "scheduled"` prevents space links without visibility from being treated as always available; Rust contact transitions drive link state and delayed route recomputation. The same packet engine reports delivered and dropped packets as contacts open/close. Python runs offline and writes a versioned JSON scenario plus a provenance sidecar.

**Existing P0:** typed nodes/links/flows/faults, deterministic integer-nanosecond event scheduler, shortest-route FIB with controller delay, packet queues, propagation/serialization/processing delay, packet traces, JSON report and CLI.

### Provenance and accuracy

- Python dependency pins: `skyfield==1.55`, `sgp4==2.27`; zero network downloads during propagation (`timescale(builtin=True)`).
- Orbital input TLE and absolute historical `epoch_utc` are explicitly recorded and checked for freshness (configurable maximum 14 days from TLE epoch).
- Skyfield converts SGP4 TEME output to ITRS Earth-fixed coordinates before WGS84 elevation/occlusion calculations.
- Contacts use a coarse visibility scan; optional bisection refines **detected** rise/set transitions to a configured tolerance, conservatively rounding into visibility. Two transitions within one sample interval can still be missed. A contact's delay is computed at its midpoint and held constant for that window.
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

## Remaining subsystem work

P1 next: reference-grade contact boundary refinement, orbit catalog/ephemeris output, differential propagation delays, time-resolved Doppler/PHY correction, antenna/beam RF budget, MAC access/scheduler, optical PAT and spacecraft constraints.

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
