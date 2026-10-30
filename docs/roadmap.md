# Engineering roadmap

## P0 — deterministic packet-transport vertical slice

- [x] Subsystem-oriented workspace layout and English-only engineering documentation.
- [x] Scenario version 1, named nodes and typed directed links, packet flows and link faults.
- [x] Deterministic virtual-time engine with stable timestamp/priority/sequence ordering.
- [x] Multi-hop user terminal -> user RF -> space ISL -> feeder ground station -> terrestrial PoP -> internet sink.
- [x] Finite link queues, propagation/serialization/processing delay, per-packet trace and deterministic output.
- [x] Link-down event and delayed route reconvergence, validated through backup ISL fixture.
- [ ] Verified CI on merged `main` branch and performance benchmarks.
- [ ] Generated Rust/Python/TypeScript bindings from canonical schema.

P0 is **static-contact, packet-level simulation**, not physical radio or orbital simulation. See [status](implementation-status.md).

## P1 — physical contact and radio/link realism

- [x] Initial offline historical TLE/SGP4 propagation, Skyfield TEME→ITRS conversion and sampled WGS84 user/feeder/ISL geometric contact windows (SYS-01).
- [x] Scheduled half-open contacts feed Rust link availability and delayed route recomputation; synthetic contact handover and historical ISS pass test fixtures.
- [x] Optional bisection refinement of bracketed geometric contact boundaries (SYS-01); sample step and tolerance are recorded in the manifest. Detected coarse setting edges are now conservative rather than admitting known-invisible time.
- [x] Published historical ISS GCRS/geodetic/topocentric regression and coarse trailing-interval coverage (not independent EOP validation).
- [x] Bounded offline SYS-01 ITRS ephemeris JSON artifact with stable UTC sampling, source hashes and historical ISS CI validation (not interpolation or EOP validation).
- [x] Offline, SHA-256 provenance-checked SYS-01 linear ITRS ephemeris **fixed interior-probe** error audit against the same SGP4 provider, with optional 1/3/7 probes per interval and a configurable CI threshold (not a continuous interpolation bound or independent reference).
- [x] Independent published Vallado/Astropy/Orekit TEME-to-PEF-to-ITRF reference position with pinned historical UT1/polar motion and a separate Skyfield transform regression (test-only; no production EOP ingestion).
- [x] Opt-in versioned, SHA-256-pinned local IERS finals2000A ingestion for production Skyfield timescales, with strict coverage/expiry checks and provenance (no bundled IERS archive or independent multi-epoch calibration).
- [x] Small pinned excerpts of published IERS 2004/2014 observed rows and 2026 predicted rows, with production EOP parsing and two-epoch independent GMST82 comparison (not a complete authenticated archive or external multi-epoch ITRF reference).
- [x] Production SGP4/ITRS adapter cross-checked at three 2014 UTC instants against raw SGP4 TEME and an independent test-only Vallado GMST82/polar-motion path using pinned official IERS excerpt (not independent orbit truth or published multi-epoch ITRF reference).
- [x] Bounded satellite-major JSONL streaming ITRS ephemeris with atomic artifact replacement, SHA-256 provenance and historical ISS CI (not a columnar format or scale benchmark).
- [x] Offline JSONL reader with full-stream schema/grid/digest verification before consumption and bounded disk snapshot (not a trusted-signature or live streaming protocol).
- [x] Reproducible synthetic SYS-01 JSONL exporter/verified-reader throughput and Python-heap baseline with 1k/10k CI smoke workloads (not real SGP4 constellation scaling or a performance SLA).
- [x] Bounded real SGP4/Skyfield ITRS throughput smoke using replicated historical TLEs, with separate provider-build timing and position digest (not a real constellation or large-scale capacity validation).
- [x] Isolated repeated real SGP4/ITRS benchmark with digest consistency, throughput medians and process RSS high-water marks (small CI smoke; no 100k/500k measurement).
- [x] Opt-in distinct supplied-TLE benchmark mode, rejecting repeated TLE pairs and mismatched source counts (single-ISS CI smoke; no multi-orbit catalog bundled).
- [x] Linux current-RSS checkpoints before/after SGP4 provider build and after propagation, aggregated across isolated workers (not per-stage peaks or 100k/500k measurements).
- [x] Bounded offline checksum-validated two-/three-line TLE catalog intake with operator-supplied source/license/retrieval metadata and SHA-256 manifest, feeding the existing unique-mode SGP4 benchmark (no independently sourced multi-orbit catalog or source authentication).
- [x] Opt-in manifest-bound consumption of imported TLE catalogs by isolated unique-mode SGP4 benchmarks, with byte-digest, checksum, identity and age verification before worker propagation (not source authentication).\n- [ ] Independent published multi-epoch ITRF reference verification, certified IERS/leap-second archive handling, columnar ephemerides and large constellation benchmarks.
- [x] First-order Doppler/range-rate observations at contact midpoints, exported as a separate analytical artifact (not PHY carrier recovery).
- [x] Initial SYS-05 analytical RF link-budget sidecar from SYS-01 midpoint contact delays (Friis, thermal C/N, reference margin); no PHY/MAC admission.
- [x] Optional receiver-input aggregate in-band interference assumption and analytical C/(N+I) with separate SINR reference threshold (not a physical beam interference model).
- [x] Optional SYS-01 time-sampled geometric delay compiled into scheduled contacts and interpolated at each Rust packet transmission completion (not light-time iteration or live PHY).
- [x] Optional SYS-05 time-resolved analytical RF sidecar at validated SYS-01 geometric delay sample instants (not continuously interpolated SINR or PHY/MAC).
- [ ] Actual access/optical acquisition feasibility, calibrated link budgets with interference, and validated relativistic/light-time delay models.
- [x] Opt-in SYS-02 spacecraft optical-head outage windows gate SYS-03 PAT acquisition, release shared heads, and require reacquisition after recovery (deterministic preplanned availability, not pointing physics).
- [x] Opt-in SYS-02 static optical pointing-envelope gate using operator-supplied per-link off-boresight angles and consistent per-head limits, with PAT rejection and packet suppression (not ephemeris-derived attitude or steering dynamics).
- [x] Opt-in per-contact frame-tagged ITRS position and boresight snapshots for SYS-02 steering feasibility, gating PAT and packet forwarding (supplied geometry only, no ephemeris adapter or attitude dynamics).
- [x] Offline SYS-01 SGP4-to-SYS-02 ITRS optical snapshot compiler using explicit fixed body-to-ITRS head quaternions, SHA-256 provenance, and a synthetic two-orbit PAT packet-runtime smoke (not time-varying attitude or continuous steering).
- [x] Opt-in finite interior/end microsecond-aligned SGP4 steering probes fail closed before emitting fixed-attitude PAT scenarios, with per-contact provenance and drift tests (not continuous pointing or runtime tracking loss).
- [x] Opt-in bounded, time-indexed body-to-ITRS head quaternions with shortest-arc SLERP, sampled steering audit and synthetic SGP4-to-PAT CI (operator-supplied attitude, not runtime tracking loss or an attitude propagator).
- [x] Opt-in finite sampled SYS-02 pointing loss/recovery compiled into existing SYS-03 PAT link-fault transitions, with exclusive-head release, reacquisition and packet-runtime CI (synthetic ITRS acceptance; not continuous steering or physical loss detection).\n- [ ] Spacecraft payload mode, ephemeris-derived optical pointing, and attitude constraints (SYS-02).
- [x] Standalone deterministic SYS-03 optical PAT contact acquisition planner with exclusive-head reservations, finite slew/acquisition dwell and half-open tracking windows (not yet connected to packet forwarding or calibrated optical physics).
- [x] Opt-in SYS-03 PAT tracking-window gate for scheduled optical contacts in the Rust packet engine, with exclusive-head contention, route reconvergence, packet accounting and deterministic PAT trace (not a live PHY or tracking-fault recovery).
- [x] Opt-in SYS-03 PAT reacquisition after modeled optical-link administrative down/up faults: split geometric contacts, release optical heads on down, reapply acquisition dwell on up, and preserve delayed route convergence (not autonomous tracking degradation or a laser PHY).
- [ ] Optical PAT acquisition/tracking and link-budget models (SYS-03).
- [x] Standalone deterministic SYS-05 weighted deficit round-robin beam-frame scheduler with finite wire-byte capacity and two-terminal tests (not yet wired to packet transport).
- [x] Opt-in shared SYS-05 uplink beam-frame grants now drive packet serialization and finite shared queues; contact loss cancels ungranted bearer packets (L1, no PHY or admission).
- [ ] Terminal modem and beam selection, user RF budget, frequency reuse, full MAC admission and handover (SYS-04/05).
- [ ] Ground antenna/feeder allocation with rain fade and site state (SYS-06/12).

## P2 — core services and network realism

- [ ] Subscriber service authorization, session and IPv4/IPv6 IPAM (SYS-08).
- [ ] Time-dependent/contact routing, gateways and traffic engineering (SYS-07).
- [ ] Terrestrial IP transport, egress, congestion and service assurance (SYS-09/13).
- [ ] Linux netns/tc/FRRouting emulation and actual ping/iperf traffic (SYS-15).

## P3 — operations, observability and advanced fidelity

- [ ] Operations, security, incident/command workflows (SYS-10/11).
- [ ] Versioned archive/visualization and CesiumJS globe, interactive console (SYS-16).
- [ ] OSS integration PoCs: SNS3/ns-3, OpenSAND, OAI/Open5GS NTN profile and GNU Radio.
- [ ] Explicit model validation, scale/performance and failure-mode benchmarks.

Do not mark future subsystem capability complete because its design exists. Each functional milestone requires tested behavior, failure cases, trace/metrics and independently justified physical assumptions.
