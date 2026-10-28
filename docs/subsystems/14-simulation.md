# SYS-14 — Deterministic Simulation Kernel and Experiment Orchestration

**Mission:** advance virtual time, schedule causally ordered events, own run lifecycle, deterministic randomness, fault injection, checkpoints and reproducible artifacts. It is the **execution platform**, not the owner of orbital physics, PHY/MAC, subscriber policy or route decision algorithms.

## Kernel components

1. **Virtual clock:** monotonic `u64` nanoseconds from declared run epoch, separate from UTC/TAI and wall clock.
2. **Stable event priority queue:** ordering tuple `(timestamp_ns, priority, sequence)`, efficient binary heap, cancellation/tombstone support, no accidental hashmap-order nondeterminism.
3. **Event dispatch/router:** typed destination subsystem and operation, no unchecked dynamic code execution from user scenarios.
4. **Domain state store:** subsystem-specific state owned by respective library, snapshot/revision access for tests and adapters.
5. **Seeded random streams:** reproducible stochastic packet error/weather/traffic with independent per-entity RNG streams.
6. **Scenario compiler:** validates schema, IDs, references, time range, frequencies, geometry, resource budgets and fidelity compatibility.
7. **Traffic generator:** modeled UDP/TCP-like flows, packet sizes, arrivals (constant/Poisson/bursty), flow expiry; actual transport behavior belongs to external protocol/emulation adapters.
8. **Fault injector:** deterministic target/time/severity/recovery event schedule.
9. **Trace/checkpoint writer:** append events with causation, state digests and artifact manifests; safe bounded buffers.
10. **Performance and distributed coordinator:** event counters, hot-path profiling and future conservative time-partitioning option.

## Core algorithms and ownership

Proposed API:

```rust
struct EventKey { time_ns: u64, priority: u16, insertion_sequence: u64 }
enum ScheduledEvent {
    ContactOpened { link: LinkId },
    LinkAcquired { link: LinkId },
    PacketTransmit { packet: PacketId, link: LinkId },
    PacketArrived { packet: PacketId, node: NodeId },
    InstallRoute { revision: u64 },
    InjectFault { target: EntityId, kind: FaultKind },
}
trait DomainHandler {
    fn on_event(&mut self, ctx: &mut EventContext, event: DomainEvent)
        -> Result<(), SimulationError>;
}
```

This is **proposed** API, not current code. Domains schedule future events through context; they cannot move clock backwards. Same-time event priorities are documented: exogenous link-down/expiration before new transmissions, link-up before eligible subsequent transmissions, with a stable sequence tie-breaker. Choose explicit handling for frames already in flight when a link drops; never silently erase packets. Track `injected = delivered + dropped + expired + in_flight` with defined treatment of retransmitted packet copies.

Choose a deterministic integer-time representation, avoiding accumulation of floating-point seconds in event keys. Physics may use f64 with finite checks and deterministic rounding at event-scheduling boundary. Time-to-wall-clock bridging is **not** enabled in accelerated runs.

## Run lifecycle

`CREATED → VALIDATING → READY → RUNNING ↔ PAUSED → COMPLETED / FAILED / CANCELLED`. A run is immutable with respect to compiled scenario; controls such as fault injection are explicit timestamped event commands appended to a separate action log. Checkpoints include event heap, domain state snapshots, RNG stream states, packet store, clock, counters and adapter revisions. Replay verifies hashes against expected sequence.

A small experiment can use one Rust executable and precomputed Python ephemeris snapshots; it must not issue remote RPC per packet. For external ns-3, an adapter can produce offline packet trace comparison first; later a single-authority clock coupling requires designed lookahead/barriers. See [runtime design](../design/simulation-runtime.md).

## Contracts and dependencies

Own `SimulationRun`, `ScenarioManifest`, `VirtualClock`, `ScheduledEvent`, `Checkpoint`, `FaultPlan`. Commands: `CreateRun`, `StartRun`, `PauseRun`, `AdvanceUntil`, `InjectFault`, `CancelRun`, `ReplayRun`. Events: `RunStarted/Paused/Completed/Failed`, `CheckpointCreated`, `FaultApplied`, `EventBudgetExceeded`. Use Rust core with serde only at boundaries; external Python/TypeScript are artifact/API consumers. No general-purpose parallel framework before profiling.

## Reliability, performance and acceptance

Malicious scenario sizes, infinite zero-delay loops, priority starvation, packet queue exhaustion, trace buffer overload, nonfinite RF model output, skipped cancellation, duplicated action and external adapter crash must fail with explicit run status. Set maximum events, simulated duration, objects, heap bytes and trace bytes.

**Tests:** deterministic same-seed trace checksum; clock never moves backward; stable same-time ordering; causality/packet accounting; state checkpoint and resumed trace equal uninterrupted trace; fault injection at exact virtual timestamp; execution budget prevents infinite cycles. Benchmarks: events/sec, peak event queue size, memory/packet, disk trace throughput, contact preprocessing time and CPU attribution.

**Build order:** clock/event heap → packet/link primitives → run/scenario lifecycle → fault+trace → checkpoint → profiling/distributed feasibility study.