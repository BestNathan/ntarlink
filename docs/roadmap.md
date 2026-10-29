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
- [x] Optional bisection refinement of bracketed geometric contact boundaries (SYS-01); sample step and tolerance are recorded in the manifest.
- [ ] Independent Earth orientation reference vectors, ephemeris artifact batching and large constellation benchmarks.
- [x] First-order Doppler/range-rate observations at contact midpoints, exported as a separate analytical artifact (not PHY carrier recovery).
- [x] Initial SYS-05 analytical RF link-budget sidecar from SYS-01 midpoint contact delays (Friis, thermal C/N, reference margin); no PHY/MAC admission.
- [ ] Actual access/optical acquisition feasibility, calibrated link budgets with interference, and live per-hop varying propagation delay.
- [ ] Spacecraft payload/optical terminal availability and pointing constraints (SYS-02).
- [ ] Optical PAT acquisition/tracking and link-budget models (SYS-03).
- [ ] Terminal modem and beam selection, user RF budget, frequency reuse, MAC scheduling and handover (SYS-04/05).
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
