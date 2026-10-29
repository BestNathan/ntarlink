# Implementation status and current fidelity

## Operational P0 and P1 orbital-contact bridge

The existing P0 Rust packet simulator executes a deterministic packet journey across user RF, satellite optical ISL, feeder station, terrestrial backhaul and internet endpoint. The new **P1 orbital bridge** provides offline TLE/SGP4 propagation, Skyfield TEME-to-ITRS transformation, WGS84 elevation and Earth-occlusion geometry, sampled contact windows with optional bisection-refined boundaries and geometric propagation delay, plus optional per-contact analytical range-rate/Doppler observations in a separate sidecar.

**Implemented in code:** contact windows with half-open `[start_ns,end_ns)` bounds and optional midpoint geometric delay; `contact_mode: "scheduled"` prevents space links without visibility from being treated as always available; Rust contact transitions drive link state and delayed route recomputation. The same packet engine reports delivered and dropped packets as contacts open/close. Python runs offline and writes a versioned JSON scenario plus a provenance sidecar.

**Existing P0:** typed nodes/links/flows/faults, deterministic integer-nanosecond event scheduler, shortest-route FIB with controller delay, packet queues, propagation/serialization/processing delay, packet traces, JSON report and CLI.

### Provenance and accuracy

- Python dependency pins: `skyfield==1.55`, `sgp4==2.27`; zero network downloads during propagation (`timescale(builtin=True)`).
- Orbital input TLE and absolute historical `epoch_utc` are explicitly recorded and checked for freshness (configurable maximum 14 days from TLE epoch).
- Skyfield converts SGP4 TEME output to ITRS Earth-fixed coordinates before WGS84 elevation/occlusion calculations. Published 2014 ISS GCRS, WGS84 subpoint, topocentric altitude and range observables are now regression-tested with declared tolerances; this is not an independent EOP/ITRF transform reference (see [frame validation](design/orbital-frame-validation.md)).
- Contacts use a coarse visibility scan; detected rise/set transitions now conservatively round into visibility even without bisection (coarse sets use the previous visible sample), and optional bisection refines those bounds to a configured tolerance. Coarse sets may discard up to one sampling interval of valid visibility. Two transitions within one sample interval can still be missed. The midpoint delay remains a **fixed route-planning weight**; optionally the Python generator emits bounded per-contact ECEF geometric delay samples and Rust interpolates delay **at each packet's transmission completion** using integer nanosecond arithmetic. With no samples, legacy midpoint behavior remains unchanged. This is a geometric approximation, not retarded-time/light-time iteration or a PHY group-delay model. See [delay design](design/time-varying-propagation.md).
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

## Bounded offline SYS-01 ephemeris snapshots

The `ntarlink_orbital.ephemeris` CLI exports sorted, frame-tagged ITRS
satellite position snapshots on a bounded, half-open UTC time grid. It
reuses the existing pinned SGP4/Skyfield position provider and writes an
independent JSON artifact with exact input/output SHA-256 provenance and
dependency versions. A global 500,000 satellite-time-pair cap and explicit
microsecond-resolution time grid prevent unbounded work and false
nanosecond precision. Unit tests cover deterministic ordering, time
boundaries, invalid IDs, malformed positions, resource limits and digests;
CI generates a real historical ISS ephemeris.

This offline analytical export does **not** modify the Rust packet runtime,
contact windows or RF budgets. It has no interpolation accuracy bound,
velocity output, EOP calibration or constellation-scale benchmark.
See [ephemeris artifact design](design/orbital-ephemeris-artifacts.md).

## Offline SYS-01 ephemeris fixed interior-probe interpolation audit

The optional `ntarlink_orbital.ephemeris_audit` command validates a pinned
ITRS snapshot against the same SGP4/Skyfield provider at 1, 3 or 7
evenly spaced interior probes per complete adjacent sample pair. It reports
midpoint and all-probe per-satellite/global maximum and RMS position errors
in meters, and fails its CLI gate when the selected probes exceed the
configured threshold. The audit verifies the exporter manifest's exact
scenario/orbit/artifact SHA-256 provenance and rejects malformed time grids,
coordinates and satellite catalogs before propagation. Historical ISS
three-probe checks run in CI. This is a **same-provider, finite-probe**
self-consistency check; it neither proves continuous interpolation accuracy
nor independently validates orbit truth or EOP. See
[ephemeris audit design](design/orbital-ephemeris-audit.md).

## Bounded SYS-01 JSONL streaming ephemeris

The optional `ntarlink_orbital.ephemeris_stream` CLI emits a versioned,
frame-tagged, satellite-major JSONL artifact without retaining all position
samples in memory. It shares the original exporter's UTC grid validation,
500,000 satellite-time-pair limit and pinned SGP4/EOP provider. Each stream
has a header plus one record per sample, a 128 MB byte limit, atomic
replacement after successful validation, and a companion SHA-256 provenance
manifest. Python CI exercises a real historical ISS stream, deterministic
ordering, failure cleanup and resource limits. This does not replace the
original nested JSON exporter or its interpolation audit, and is neither
columnar output nor a measured constellation-scale benchmark. A separate
`ntarlink_orbital.ephemeris_reader` validates the full JSONL schema, ordering,
UTC grid, finite positions, byte count and SHA-256 manifest before yielding
records. It uses a bounded temporary disk snapshot to prevent source changes
between validation and consumption; this is not authenticated provenance,
a live feed or a measured throughput benchmark. See
[streaming design](design/orbital-ephemeris-stream.md).

## Synthetic SYS-01 ephemeris scale baseline

The optional `ntarlink_orbital.ephemeris_benchmark` command creates a
bounded, deterministic synthetic satellite/time grid, exports JSONL using
the production streaming writer and validates every record through the
production snapshotting reader. It reports artifact bytes/digest, measured
wall-clock throughput and peak traced Python heap for each stage. CI checks
1,000 and 10,000 position pairs without timing gates. This is a synthetic
serialization and verified-read baseline, not a real constellation, SGP4
propagation benchmark, process-RSS measurement or performance guarantee.
See [benchmark protocol](design/orbital-ephemeris-benchmark.md).

## Opt-in production SYS-01 Earth-orientation input

The SGP4 provider can now load a local, SHA-256-pinned IERS finals2000A
file into a Skyfield timescale for both contact prediction and ITRS
ephemeris export. Daily UT1−UTC and xp/yp polar motion are interpolated
with strict source coverage and an explicit operator-supplied expiry;
leap seconds still use Skyfield 1.55's bundled schedule. Both output
manifests record EOP source provenance. The default remains the built-in
timescale, and no IERS file is bundled or fetched. Synthetic EOP fixtures
exercise the production adapter and Vallado single-epoch reference.
The opt-in loader now checks both daily IERS observation/prediction flags,
rejects forecast rows in the interpolation window by default and requires
explicit operator opt-in to use predicted EOP. Quality-policy counts are
recorded in the frame-input manifest; this does not authenticate the source.
EOP coverage, expiry and forecast-quality checks conservatively round the
last included simulation nanosecond upward to Skyfield's microsecond
resolution, preventing submicrosecond end-of-coverage leakage.
This is not independent multi-epoch calibration, authenticated IERS data,
or validation of future leap seconds. See [EOP design](design/earth-orientation-data.md).

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

## Official IERS excerpt regression

Small fixed-width excerpts transcribed from the published IERS finals2000A rolling file (2004, 2014 and 2026) are pinned as test fixtures. Production EOP parsing and two-epoch independent GMST82/IAU-1980 transform agreement are exercised, along with forecast rejection/opt-in. See [source and limitations](design/iers-observed-excerpts.md). These are not an authenticated complete IERS archive or independently published multi-epoch ITRF vectors.

## Production frame cross-check at multiple instants

The production SGP4/Skyfield ITRS provider is regression-tested at three
2014-01-23 UTC instants against raw SGP4 TEME positions transformed by a
separate test-only Vallado GMST82/IAU-1980 implementation. It consumes
the pinned official 2014 IERS excerpt and checks SHA-256 provenance,
observed EOP policy, three-dimensional position agreement within 0.5 m
and negative controls for omitted UT1 or polar motion. The comparison
tests frame wiring, not independent orbital truth. It does not supply
externally published ITRF vectors for several years, a complete certified
IERS archive or independently maintained leap-second tables. See
[frame validation](design/orbital-frame-validation.md).

## Remaining subsystem work

P1 next: externally archived IERS source and independent multi-epoch frame verification (the production provider now accepts pinned EOP, but no authentic IERS archive is bundled or multi-epoch calibrated), large-scale orbit catalog and columnar ephemerides with benchmarks, physically calibrated time-resolved Doppler/PHY correction, beam/interference RF budget, MAC access/scheduler, optical PAT and spacecraft constraints. Per-packet sampled geometric delay is implemented, but signal travel-time iteration and continuous path prediction are not.

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
