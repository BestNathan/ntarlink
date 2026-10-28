# Implementation status and current fidelity

## Working P0/L1 vertical slice (subject to CI)

The Rust simulation is no longer a placeholder. `ntarlink-cli simulate` executes modeled packets on the graph configured in `scenarios/minimal.json`:

```text
User Terminal -> User RF -> Satellite A -> Optical ISL -> Satellite B
-> Feeder -> Ground Station -> Backhaul -> PoP -> Internet Endpoint
```

**Implemented in code:** JSON scenario v1, typed node/link/flow/fault models and validation, deterministic integer-nanosecond event queue, directed link path computation, explicit initial FIB and delayed route reinstall after faults, finite byte queues, link capacity/serialization, propagation and processing delay, per-hop packet trace, JSON report, cross-domain fixture tests and CLI.

**Models/approximations:** contact topology is **fixed** and link states are scenario-controlled; RF/optical delays and capacity are scenario parameters, not output of a validated link budget. No dynamic orbital calculations or true MAC/PHY/RF. A modeled packet accepted onto a link finishes transmission even if the link goes DOWN later (P0 `complete_after_departure` policy). No routing protocol exchange, 3GPP service core or terminal authentication yet. Traffic sources are simulated packets rather than kernel IP sockets.

**Implementation modules:** `platform/contracts`, `platform/simulation`, `platform/packet-runtime`, `systems/network-control`, `applications/cli`; Python geometry under `systems/orbital/python`, TypeScript trace types under `applications/console`. This is a **vertical slice**, not all sixteen subsystems implemented. Domain-specific access/MAC, space optical PAT, ground station resource managers and subscriber service core remain future independent implementations.

## Reproducible scenarios

| Scenario | What is validated | Expected evidence |
|---|---|---|
| `scenarios/minimal.json` | five physical/link classes and destination delivery | 3 delivered packets, each with exactly 5 hop traces |
| `scenarios/isl-failure.json` | optical link outage, 20ms control-plane convergence, backup path | link_down event, increased route revision, eventual hops via sat-c |
| In-memory unit variations | finite queue and no feeder route | queue_overflow/no_route, zero false deliveries |

## Compatibility and next steps

The old Go and `crates/` bootstrap layouts are removed; the Rust workspace is organized by runtime/domain boundaries, Python by orbital subsystem and TypeScript by application. Schema v1 is a starting point and requires cross-language generated bindings once stabilized.

**Not complete:** full Starlink-like orbital propagation, RF/SNR/beam/MAC models, optical PAT, satellite payload lifecycle, terminal attachment, subscriber core, backhaul routing protocol integration, operations, weather, Linux emulation, Cesium globe, OSS engine adapters and large-scale benchmarking.

See [roadmap](roadmap.md) and [subsystem designs](subsystems/README.md).
