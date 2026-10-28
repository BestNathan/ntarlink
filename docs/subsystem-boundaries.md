# Subsystem boundaries, state ownership and contracts

## 1. Contract-first rules

- Each authoritative fact has exactly one owning subsystem. Consumers receive immutable snapshots/events with explicit timestamp, source, frame and units.
- Distinguish **simulated time** (virtual event timestamps), **orbital UTC/TAI** (with documented conversions), and **wall time** (experiment execution/telemetry ingestion). Never silently mix clocks.
- All cross-system messages have `schema_version`, `scenario_id`, `run_id`, `sim_time_ns`, `source`, `sequence`, `trace_id`; physical quantities carry unit/frame metadata.
- Separate commands (request a change), events (observed state change), and queries (read state). No subsystem mutates another's state directly.
- Avoid distributed service calls in hot per-packet simulation loops. Use in-process typed adapters, batched event channels or shared snapshots when scale demands; APIs are logical contracts first.
- External simulator adapters must declare fidelity, synchronization strategy, failure semantics and license/version provenance.

## 2. Authoritative state and published events

| Owner | Authoritative entities | Publishes / exposes | Consumes |
|---|---|---|---|
| Orbital | Ephemeris, reference frames, propagation clock mapping | PositionSnapshot, VisibilityWindow, EclipseWindow | spacecraft orbit definitions, environment |
| Spacecraft | Satellite, payload, health, attitude/pointing capability | SatelliteStateChanged, PayloadAvailable | orbital snapshots, operations commands |
| ISL | ISL link, PAT state, space forwarding queues | ISLUp/Down, CapacityChanged, PacketHop | geometry, spacecraft state, route intent |
| Terminal | Terminal, modem/antenna state, LAN endpoint | TerminalDiscovered, TerminalStateChanged | access decisions, customer traffic |
| Access | Beam, channel, bearer, attachment, handover | BeamScheduled, Attached, HandoverCompleted/Failed | terminal state, geometry, policy, spectrum |
| Ground | Site, feeder antenna, feeder contact, radio gateway | FeederLinkUp/Down, GroundSiteDegraded | geometry, environment, network control |
| Network Control | topology view, route plan, gateway choice | RouteInstalled, GatewayChanged | ISL/access/ground link availability, demand |
| Subscriber Core | Subscriber, service session, policy, IP lease | SessionAuthorized, SessionMoved, PolicyUpdated | access attach, security, service plan |
| Backbone | PoP, transport link, egress path, internet endpoint | EgressAvailable, BackhaulCongested | gateway traffic, policy, terrestrial topology |
| Operations | Alarm, incident, operator intent | OperationalCommand, IncidentResolved | telemetry and service assurance |
| Security | Trust policy, credential abstraction, audit | AuthorizationDecision, SecurityEvent | identity and control-plane commands |
| Regulatory/Environment | weather cell, interference field, permitted service region | WeatherChanged, SpectrumConstraintChanged | scenario inputs and observations |
| Assurance | service-level measurement, outage classification | SLAReport, UserExperienceEvent | traces, sessions, link metrics |
| Orchestrator | run, event queue, RNG seed, replay checkpoint | RunStarted/Paused/Finished, InjectedFault | scenario and experiment plan |
| Emulation | emulated endpoint, netns, external adapter instance | PacketCapture, AdapterHealth | modeled link states, run schedule |
| Data/UI | artifact index, visualization state | ExperimentReport, UIActionRequest | all published metrics, traces and snapshots |

## 3. Separation decisions

### Ground station vs internet gateway
Ground station terminates a feeder link and manages antennas/radios. A PoP terminates terrestrial transport and exchanges IP traffic with the internet. They can be colocated but are different roles. A 'gateway' must always specify whether it is **feeder gateway**, **subscriber service gateway**, or **internet egress gateway**.

### Access vs terminal
Terminal owns its own acquisition/antenna/modem state; access owns beam resources, admission, attachment and network-side mobility decisions. A handover is a coordinated state transition, not a one-sided toggle.

### Core network vs 5G core
Subscriber Core is a generic logical service-control domain. A 3GPP 5GC implementation (e.g. Open5GS) is an optional adapter for the NTN profile, not a required or asserted component of proprietary Starlink broadband.

### Orbital vs network control
Orbital produces positions and visibility geometry. Network Control selects links, paths, traffic engineering and gateway policy from feasible contacts and demand. Visibility is not proof that a link is acquired or usable.

### Simulation vs emulation
Simulation computes outcomes using models and virtual time. Emulation runs actual network stacks/packets under controlled link conditions. Bridging requires a defined time synchronization policy and must never imply bit-accurate RF behavior.

## 4. Execution profiles

| Profile | Runs | Intended evidence | Limits |
|---|---|---|---|
| Analytical | orbit, contact and capacity calculations | coverage, geometry, planning | no packet fidelity |
| Discrete-event | Rust event engine and domain models | latency, packet loss, handover, routing | approximated PHY/MAC unless integrated |
| Protocol study | ns-3/SNS3/OpenSAND or other validated stacks | detailed protocol comparisons | costly, model-specific |
| IP emulation | Linux network namespaces, tc/netem, routing | actual IP traffic and apps | no inherent RF waveform fidelity |
| Hardware-in-loop | GNU Radio/SDR and hardware adapters | bounded real equipment experiments | external hardware and legal constraints |

## 5. Repository policy

Organize by **domain subsystem** under `systems/`, shared infrastructure under `platform/`, external upstream adapters under `integrations/`, user applications under `applications/`, and cross-system experiments under `scenarios/` and `experiments/`. Rust/Python/TypeScript live **inside** the relevant subsystem or application. Keep top-level Rust workspace and packaging manifests where tooling requires them. This document describes target structure; migration is a separate change.
