# System scope and subsystem catalog

## 1. Mission and simulation boundaries

**Mission:** reproduce and experimentally study the full engineering behavior of a Starlink-like LEO broadband network, from orbital dynamics and spectrum/beam planning to terminal acquisition, satellite forwarding, inter-satellite links, feeder links, ground network, user sessions, internet connectivity, operations, failures, and lifecycle economics where useful.

**Not the mission:** reproduce undocumented proprietary SpaceX PHY/MAC, encryption, authentication or ground-station internals; operate real RF transmitters without licensing; assert every Starlink deployment uses a particular 3GPP architecture. We distinguish documented standards, approximations, assumptions, and experimental integrations.

The system has **space, user, ground, terrestrial, operations, and simulation-platform segments**. Each catalog entry has one primary owner. A satellite may be a bent-pipe or regenerative payload; choose explicitly per scenario. The direct-to-cell/3GPP NTN profile is optional and must not be conflated with broadband user terminals.

## 2. Subsystem catalog

| ID | Subsystem | Position / primary responsibility | Owns | Does not own |
|---|---|---|---|---|
| SYS-01 | Orbital Dynamics & Ephemeris | Physical truth of constellation positions and time-varying geometry | TLE/ephemeris, propagators, ECI/ECEF transforms, time scales, Earth geometry, orbit events | IP routing or beam scheduling |
| SYS-02 | Constellation & Spacecraft | Inventory and spacecraft lifecycle | satellite/payload models, orbital shells, attitude/pointing capability, energy/thermal constraints, onboard availability, health | orbital propagation algorithms, network path policy |
| SYS-03 | Space Network / ISL | Space-to-space communications and forwarding | optical ISL acquisition/tracking, LOS, capacities, satellite forwarding plane, per-hop queues, link failure | global network policy, terrestrial internet |
| SYS-04 | User Terminal / CPE | Subscriber-side hardware and LAN interface | terminal location/mobility, antenna steering, modem states, LAN/Wi-Fi abstraction, terminal telemetry | constellation-wide beam allocation |
| SYS-05 | User Access Network | User-to-satellite access and mobility | spot beams, spectrum reuse, user RF link, PHY/MAC models, resource allocation, terminal admission, handover | internet egress, feeder station operations |
| SYS-06 | Ground Stations & Feeder Network | Space-to-ground feeder infrastructure | ground site inventory, antennas, feeder RF/optical link models, tracking, site availability, gateway radio termination | user subscriber core, internet peering |
| SYS-07 | Network Control & Traffic Engineering | Global network decisions over a changing graph | contact topology, route computation/distribution, gateway selection, load balancing, resilience policy, network-wide capacity planning | authoritative orbit propagation, actual PHY execution |
| SYS-08 | Subscriber Core & Service Control | Subscriber identity/session and service continuity | authentication *models*, entitlement, session state, IP allocation, mobility anchoring abstraction, QoS/policy, accounting models | assuming Starlink uses 5GC, beam PHY |
| SYS-09 | Terrestrial Backbone & Internet Edge | Ground IP transport and public connectivity | PoPs, backhaul, IP routing/BGP model, NAT/CGNAT or IPv6 policy, DNS abstraction, internet traffic sinks/sources | satellite feeder PHY, subscriber authorization |
| SYS-10 | Operations / NOC & Mission Control | Human/operator oversight and lifecycle control | fleet telemetry, alarms, service health, command workflows, network changes, capacity and fault management | simulation virtual-time engine, data-plane forwarding |
| SYS-11 | Security & Trust | Cross-segment security policies and threat models | identity/trust model, key lifecycle abstraction, secure management channels, isolation, audit, adversarial scenarios | inventing proprietary cryptography |
| SYS-12 | Regulatory, Spectrum & Environment | External constraints on operations | country/region eligibility, spectrum allocations, interference, weather/rain fade, atmospheric loss, regulatory scenario inputs | PHY scheduling decisions or regulatory approval |
| SYS-13 | Service Assurance & Customer Experience | User-facing performance and SLA experiments | service availability, throughput/RTT/jitter/loss, outage classification, service-level reports | raw event scheduling or packet forwarding |
| SYS-14 | Simulation Orchestrator | Run reproducible whole-system experiments | virtual time, event scheduler, seeds, scenario lifecycle, snapshots, replay, distributed run coordination | domain decisions for every subsystem |
| SYS-15 | Network & Hardware Emulation | Execute packets/protocols against modeled links | Linux netns/veth/tc, external ns-3/SNS3/OpenSAND/SDR adapters, time/fidelity bridges | orbital truth or product policy |
| SYS-16 | Data, Observability & Visualization | Shared evidence and human inspection | schema registry, experiment artifacts, metrics/traces, time-series, Cesium globe, dashboards and packet journey | authoritative domain state of other systems |

**Cross-cutting capabilities**: versioned contracts and units, configuration/provenance, model calibration, test/benchmark infrastructure, deployment and supply-chain management. They are platform responsibilities, not additional competing domain owners.

## 3. Required engineering dimensions per subsystem

Every subsystem specification must define: purpose and non-goals; entities and ownership; commands/queries/events; state machine; time model; accuracy/fidelity level; performance limits; dependencies; failure modes; security and observability; reproducible acceptance scenarios; upstream components and licensing. A folder alone does not constitute an implemented subsystem.

## 4. Core end-to-end paths

**Broadband user plane:** customer LAN → user terminal → access RF/beam → satellite payload → zero or more optical ISLs → feeder link → ground station → terrestrial PoP/backbone → internet destination. Reverse path may use a different route.

**Network control:** orbit and spacecraft state → visibility/contact opportunities → beam/access admission and ISL acquisition → network graph/route and gateway selection → forwarding rules → event/telemetry feedback.

**Operations:** ground/space telemetry → health and incident detection → approved operations actions → system-specific control APIs → state transitions and audit.

**Optional NTN:** handset/UE → NTN radio access → satellite/ground transport → 3GPP core adapter. This is a separate architecture profile, not the default broadband user-plane assumption.

## 5. Logical vs physical decomposition

Subsystems are bounded contexts, **not automatically one microservice each**. A small deterministic run can embed several domains in one Rust process; an orbital Python worker can be a separate process; real-IP emulation may need privileged Linux workers. They still exchange the same versioned domain contracts. Monorepo initially, independently testable packages and optional process isolation; split repositories only for independent release cadence, governance, scale or security needs.
