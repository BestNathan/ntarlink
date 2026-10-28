# Architecture / boundaries

## End-to-end data plane
User device → terminal LAN/IP → terminal modem/access stack → RF user link → satellite payload → [optional optical ISL hops] → feeder RF link → ground gateway → IP backbone/PoP → internet. Reverse path is symmetric conceptually, not necessarily same route.

## Control plane
Orbit/ephemeris → contact prediction → link establishment / PAT → beam and spectrum scheduler → terminal admission and mobility → dynamic topology and routing → gateway selection. A separate simulation orchestrator controls deterministic virtual time, scenario events, seeds and snapshots.

## Modules (interfaces first)
- `orbit`: SGP4/TLE and numerical propagators, reference frames, time.
- `geometry`: visibility, elevation, Earth obstruction, Doppler, propagation delay.
- `rf`: link budgets, spectrum, beam patterns, interference, modulation/coding abstractions.
- `optical`: inter-satellite laser PAT, capacity, acquisition, loss and link budgets.
- `access`: terminal discovery, admission, scheduling, handover, bearer/session state.
- `topology`: time-varying graph, contact plans, failures.
- `routing`: shortest path, predictive routes, gateway/PoP selection, forwarding tables.
- `packet`: event-driven packet transport, queues, congestion, fragmentation, metrics.
- `gateway`: feeder link termination, terrestrial IP bridge, policy, NAT where applicable.
- `ground`: station antennas, weather, power and backhaul capacity.
- `observability`: traces, metrics, replay, experiment reports.
- `emulation`: Linux namespaces/veth/tc/netem and container nodes, isolated from simulation core.

## Invariants
1. No claims of reverse-engineered Starlink proprietary PHY/MAC.
2. Simulation models and real protocol implementations are separately labeled.
3. Deterministic virtual time and seeded randomness; no wall-clock dependency in core.
4. Explicit units, coordinate frames, and source provenance for physical constants.
5. Data plane and control plane isolated by typed contracts; scenario validation precedes execution.
6. Optional upstream integrations are adapters with pinned versions/licenses; do not vendor large third-party code without review.

## Initial repository organization
`cmd/ntarlink` CLI; `internal/scenario` scenario schema; future `pkg/model` shared types and `internal/{orbit,rf,optical,access,topology,routing,packet,gateway,ground,simulation}` implementations; `web/` interactive constellation and packet-path visualization; `deploy/` emulation stacks.
