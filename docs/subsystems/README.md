# Detailed subsystem engineering design

**All descriptions, interfaces and code examples are in English.** These documents define *target* components and implementation plans. The first L1 packet-transport vertical slice is implemented; this is not equivalent to delivering all sixteen subsystems. See [implementation status](../implementation-status.md).

| ID | System | Primary technical components | Main candidate dependencies | Detailed specification |
|---|---|---|---|---|
| SYS-01 | Orbital dynamics | SGP4/TLE, frames, geometry, contact intervals | python-sgp4, Orekit | [Orbital](01-orbital-dynamics.md) |
| SYS-02 | Spacecraft/constellation | fleet registry, payload, attitude, power, health | Orekit optional | [Spacecraft](02-spacecraft.md) |
| SYS-03 | Optical ISL/network | PAT, optical budget, directional ISL, space queues/FIB | ns-3/SNS3 comparison | [Space Network](03-space-network.md) |
| SYS-04 | User terminal/CPE | modem, phased-array steering, LAN, mobility, telemetry | Linux netns/TAP optional | [Terminal](04-terminal.md) |
| SYS-05 | User RF access | spot beams, SINR/MODCOD, MAC/RRM, handover | SNS3, OpenSAND, OAI NTN optional | [Access](05-access-network.md) |
| SYS-06 | Feeder stations | ground antennas, tracking, feeder radio, handoff | OpenSAND, GNU Radio | [Ground](06-ground-segment.md) |
| SYS-07 | Network control | contact graph, time-dependent routing, gateway/TE | FRRouting, StarryNet/OpenSN | [Network Control](07-network-control.md) |
| SYS-08 | Subscriber core | auth/session, IPAM, QoS, session continuity | Open5GS optional NTN | [Core](08-subscriber-core.md) |
| SYS-09 | Terrestrial network | backhaul, PoP, BGP, IPv6, CGNAT, internet edge | FRRouting, Linux tc/netns | [Terrestrial](09-terrestrial-network.md) |
| SYS-10 | NOC/operations | alarms, incidents, approvals, spacecraft command workflow | Prometheus, Grafana, Yamcs reference | [Operations](10-operations.md) |
| SYS-11 | Security | trust, authorization, credential lifecycle, audit | Rust TLS ecosystem | [Security](11-security.md) |
| SYS-12 | Environment/regulatory | rain fade, interference, spectrum, geography | ITU-R models, weather inputs | [Environment](12-environment.md) |
| SYS-13 | Service assurance | probes, SLA, outage correlation, customer KPIs | OpenTelemetry, Prometheus | [Assurance](13-service-assurance.md) |
| SYS-14 | Simulation kernel | virtual time, event queue, RNG, fault, checkpoint | Rust/serde | [Simulation](14-simulation.md) |
| SYS-15 | Emulation/adapters | Linux netns/tc, ns-3, OpenSAND, OAI, SDR | FRRouting, SNS3, GNU Radio | [Emulation](15-emulation.md) |
| SYS-16 | Data/UI | trace store, event stream, 3D globe, run replay | React, TypeScript, CesiumJS | [Data and UI](16-data-observability.md) |

## Standard design expectations

Each specification describes responsibility and non-goals; internal components; physics/algorithms and state machine; authoritative data and commands/events; dependencies and license/adoption strategy; failure handling; test evidence and build order.

### Cross-system decisions

- [System architecture](../architecture.md)
- [Data and message contracts](../design/contracts.md)
- [End-to-end packet/control sequences](../design/data-plane.md)
- [Simulation/emulation coupling](../design/simulation-runtime.md)
- [Deployment and subsystem ownership](../design/deployment.md)
- [Fidelity, validation and reproducibility](../design/verification.md)
- [External dependency matrix](../dependencies.md)

A subsystem is a **bounded context**, not an automatic microservice. Common source code can be linked into one deterministic process, with external runtimes only where needed. Cross-subsystem state cannot be directly mutated.