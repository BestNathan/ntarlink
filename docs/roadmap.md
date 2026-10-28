# Implementation roadmap and engineering gates

The repository is currently a **minimal Rust/Python/TypeScript scaffold** with a scenario validator, not an end-to-end satellite simulator. Target subdirectories, protobuf/JSON schema contracts and third-party adapters are architectural proposals until implemented. Phases are ordered by **measurable integration**, not by the count of directories.

## Phase 0 — Architecture and contract foundation

**Deliverables:** all subsystem specifications in English; system/plane map; dependency/license register; state and clock conventions; scenario versioning; typed IDs and units; API envelope prototype; toolchain and formatting checks. A tiny versioned schema with cross-language golden fixtures must accompany the first code PR.

**Gate:** scenario rejects broken references, impossible coordinates and invalid payload-mode combinations; CI verifies docs English-only and broken relative links (outside vendor files), Rust/Python/TypeScript build/tests. Docs completion alone is not simulation completion.

## Phase 1 — Deterministic packet-path vertical slice (SYS-14/03/05/06/07/08/09)

**Build:** stable virtual-clock event queue; directional links and finite byte queues; packet identifiers/trace; a validated minimal topology: terminal → RF user link → Satellite A → optical ISL → Satellite B → feeder ground station → backhaul/PoP → internet sink; reverse direction; configurable gateway, drop/loss, link failure and routes.

**Gate:** deterministic repeated run yields same event hash; delivered/dropped/in-flight accounting balances; packet path trace includes per-hop propagation, serialization, processing and queue delay; fault shows explicit reason and recovery. This phase may use **static geometry/contact fixtures**, clearly labeled.

## Phase 2 — Orbit-informed contact, RF/access and optical link state (SYS-01/02/03/04/05/06/12)

**Build:** Python SGP4 + time/frame conversion with reference vectors; precompute contact windows; realistic acquisition windows; antenna and beam geometry; RF link budgets and Doppler; rain/interference model; optical PAT and link-quality state machine; terminal attachment/handover; spacecraft payload availability.

**Gate:** orbit reference tests, elevation/LOS invariants, contact boundary tests; predicted handover triggers measurable interruption; link capacity and packet loss change consistently with explicit model assumptions.

## Phase 3 — Network control, subscriber continuity, terrestrial IP (SYS-07/08/09/13/15)

**Build:** time-dependent route/gateway selection; link and control convergence latency; session/IP anchoring; reverse routing; congestion and QoS; real Linux network namespaces with tcp/udp, ping and iperf; FRR experiment option.

**Gate:** ISL and feeder failure reroute, gateway failover, congestion fairness/latency baseline, measured emulated RTT and packet loss vs configured channel, full resource cleanup.

## Phase 4 — Operations, security, environment and visualization (SYS-10/11/12/13/16)

**Build:** incident/command state machines, authorization and audit, operational telemetry and service reporting; Cesium globe with virtual-time timeline, beams, ISL edges, packet path and downloadable run manifests.

**Gate:** replay packet trace and outage cause by run/time; command requires authority and produces audited state transition; region/weather eligibility changes service; UI state matches archived artifacts.

## Phase 5 — Protocol and hardware fidelity (SYS-05/06/08/15)

**Build:** compare against SNS3/ns-3 (and optionally OpenSAND), independently instrument a 3GPP NTN profile with OAI/Open5GS where justified, SDR/IQ bridge, contact/PHY calibration, scalability/performance research.

**Gate:** reproducible external-tool build with pinned rev/license, numeric cross-engine comparisons and documented differences; realtime/HIL runs label their wall-clock constraints.

## Cross-phase nonfunctional requirements

- Determinism and versioned input + seed + git revision on every run.
- Physical units, reference frames and time scales verified by tests.
- Security: scenario parser bounds, privileged emulator isolation, audit and cleanup.
- Performance: event throughput, contact computation scaling, memory per satellite/terminal, trace cardinality and UI responsiveness measured.
- Scope honesty: do not mark a capability done because a directory or external product name appears in a document.
- Licensing: no third-party vendoring until review, especially GPL-2/3, AGPL and OAI CSSL terms.

## Suggested first deliverable PR sequence

1. Contract `scenario.schema.json` with strongly typed Rust/Python/TypeScript fixtures.
2. Rust `VirtualClock`, `EventQueue`, `Packet` and `Link` primitives with deterministic unit tests.
3. Static 7-hop network traffic fixture + event trace and latency assertions.
4. Python orbital/contact precompute and reference frame regression.
5. Onboard/feeder/access link acquisition, terminal attach and control-plane route installation.
6. Packet journey web timeline + fault injection reproducibility.
