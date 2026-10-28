# End-to-end system scenarios and acceptance matrix

An end-to-end system is only credible when its subsystem boundaries are exercised by reproducible experiments. Each scenario records a deterministic seed, versioned inputs, model provenance, fidelity profile, and machine-readable traces.

| Scenario | Participating systems | Minimum measurable acceptance |
|---|---|---|
| E2E-01 Basic internet | terminal, access, satellite, ISL, feeder ground, subscriber core, backbone, orchestrator | packet reaches sink; per-hop delay, queue, loss and path trace recorded |
| E2E-02 Moving satellite / handover | orbital, spacecraft, access, terminal, network control | attachment changes with predicted visibility; outage and dropped packets measured |
| E2E-03 ISL failure and reroute | ISL, network control, packet simulation, operations | fault changes graph, route reconverges, loss and recovery time measured |
| E2E-04 Gateway outage | ground, network control, backbone, subscriber core | reroute or explicit service loss; session impact measured |
| E2E-05 Rain fade / interference | environment, RF access, ground, assurance | degradation tied to modeled link budget and weather inputs |
| E2E-06 Capacity saturation | beams, ISL, queues, backbone, assurance | throughput, scheduling fairness, queueing and congestion observable |
| E2E-07 Cross-border service | regulatory, terminal, access, subscriber core | service eligibility changes according to scenario policy |
| E2E-08 Mission operations | spacecraft, operations, security, orbital | operator command audited; payload state affects available network links |
| E2E-09 IP emulation | network emulation, ground/backbone, network control | real ping/iperf traffic crosses changing virtual links with reproducible impairments |
| E2E-10 NTN profile (optional) | NTN access, core adapter, satellite transport | explicitly versioned 3GPP profile, independent of broadband default |

## Delivery gates

- **G0 — architecture baseline**: catalog, boundaries, contracts, scenarios and owner per subsystem (documentation only).
- **G1 — first vertical slice**: E2E-01 on deterministic virtual clock, with actual packet events and assertions.
- **G2 — physics-informed**: E2E-02, E2E-03 with validated orbit and RF/ISL assumptions.
- **G3 — network realism**: E2E-04 through E2E-06 and IP emulation.
- **G4 — integrated platform**: web observability, scenario authoring, provenance, scaling and optional NTN/HIL.

Passing G0 does not mean any of the simulated subsystems is implemented.
