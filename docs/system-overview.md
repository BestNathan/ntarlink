# System scope and complete subsystem inventory

## Mission

ntarlink is an **open-source-first research and engineering platform** to simulate, analyze and optionally emulate a **Starlink-like low-Earth-orbit (LEO) broadband satellite internet system**: space segment, optical inter-satellite network, ground feeder links, user terminals and radio access, subscriber service control, terrestrial PoPs, network operations, regulatory/environmental constraints, and end-to-end application traffic.

The objective is **behavioral fidelity and traceable evidence**, not imitation of undocumented proprietary SpaceX protocols. Broadband user terminals are the baseline architecture. 3GPP NR-NTN/direct-to-device is a **separate optional profile**, not the default air interface. Physical models are approximations until validated against published references.

## Physical segments and logical subsystems

| Physical or operational segment | Subsystem | Core ownership |
|---|---|---|
| Space and orbital | [SYS-01 Orbital dynamics](subsystems/01-orbital-dynamics.md) | orbit sources, propagation, coordinate frames, visibility and contact geometry |
| Space | [SYS-02 Spacecraft and constellation](subsystems/02-spacecraft.md) | satellite identity, payload modes, attitude/power/thermal/health state |
| Space | [SYS-03 Inter-satellite network](subsystems/03-space-network.md) | optical acquisition, ISL links, space forwarding, onboard queues |
| User | [SYS-04 User terminal and CPE](subsystems/04-terminal.md) | terminal capabilities, antenna/modem state, local LAN, mobility |
| User access | [SYS-05 Radio access network](subsystems/05-access-network.md) | spot beams, RF propagation, MAC/scheduling, attachment and handover |
| Ground | [SYS-06 Ground stations and feeder links](subsystems/06-ground-segment.md) | site and antenna inventory, feeder communications, satellite radio termination |
| Network control | [SYS-07 Network control and traffic engineering](subsystems/07-network-control.md) | feasible graph, contact planning, route programming, gateway selection |
| Service control | [SYS-08 Subscriber core](subsystems/08-subscriber-core.md) | subscriber authorization, service session, IP address, QoS/policy |
| Terrestrial | [SYS-09 Backbone and internet edge](subsystems/09-terrestrial-network.md) | PoPs, backhaul, IP egress, DNS/NAT/IPv6 models, remote internet endpoints |
| Operations | [SYS-10 NOC and mission operations](subsystems/10-operations.md) | incident, telemetry response, command approval, operational state |
| Trust | [SYS-11 Security](subsystems/11-security.md) | trust policy, identity, authorization, security experiments, audit |
| Environment | [SYS-12 Spectrum, weather and regulation](subsystems/12-environment.md) | spectral constraints, interference, weather, regional service eligibility |
| Customer experience | [SYS-13 Service assurance](subsystems/13-service-assurance.md) | availability, RTT/throughput/loss, outage accounting and SLA reports |
| Simulation platform | [SYS-14 Simulation kernel](subsystems/14-simulation.md) | virtual time, events, deterministic randomization, runs, checkpoints |
| Emulation platform | [SYS-15 Network and hardware emulation](subsystems/15-emulation.md) | Linux IP emulation, simulator adapters, SDR/HIL interfaces, time bridge |
| Data/platform/UI | [SYS-16 Data and visualization](subsystems/16-data-observability.md) | evidence store, traces, metrics, experiment reports, interactive visualization |

The logical subsystem count **does not imply sixteen independently deployed services**. A single deterministic Rust process can host multiple subsystem libraries. Python batch workers, C/C++ simulation engines and privileged Linux workers remain optional separate processes. Each subsystem must have an explicit contract regardless of process topology.

## The end-to-end service to be simulated

### Default broadband user-plane path

Customer LAN → CPE/terminal → terminal modem and antenna → user RF uplink → spot beam/MAC → serving satellite payload → zero or more optical ISLs → feeder downlink → feeder ground station → terrestrial backhaul → PoP/internet edge → destination service. Downlink may use a different satellite/ISL/PoP route.

### Service establishment and control path

Orbital ephemerides + spacecraft state + weather/regulatory constraints → feasible contacts → spectrum/beam/link resource allocation → terminal acquisition → subscriber authorization/session/IP assignment → controller computes forwarding and gateway selection → data-plane forwarding begins → handover updates state while monitoring user availability.

### Management path

Telemetry/events → NOC and service assurance → alarm + impact correlation → approved command or automatic policy action → subsystem-specific configuration transitions → audit and experiment trace.

## Scope and exclusion boundaries

**Included as modelable capabilities:** orbital/contact changes; constellation scale; RF/optical link budgets; capacity/interference; scheduling; handover; ISL routing; gateway selection; packet behavior; subscriber service continuity; ground IP traffic; outage/failure/recovery; operations, cost/energy model inputs and performance reports.

**Separate optional profiles:** 3GPP NR-NTN and 5GC, GEO/MEO comparison, protocol-accurate DVB-S2X/RCS2 experiments, SDR/hardware-in-the-loop, real-world telemetry ingestion.

**Explicitly excluded from initial delivery:** manufacture/control of actual spacecraft, operation of licensed transmitters, proprietary Starlink modem compatibility, real-world network interconnection, production-grade billing or subscriber identity systems, regulatory approvals.

## Design priorities

1. Correct bounded contexts and one authoritative source for each fact.
2. End-to-end reproducibility: input, time, seed, modeling assumptions, output and trace.
3. Scenario-driven incremental implementation rather than empty microservice scaffolding.
4. Explicit fidelity levels; avoid treating analytic capacity as measured RF throughput.
5. Pluggable OSS engines with pinned versions, known licensing and validation references.
6. Network/physics co-design: orbital geometry limits feasible links, but does not determine allocation or routing.

See [cross-system acceptance](system-scenarios.md).