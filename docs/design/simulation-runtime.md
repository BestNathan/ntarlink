# Simulation, emulation and experiment execution

## 1. Why a custom small Rust engine?

The first goal is a **deterministic, reproducible cross-domain packet path**, not a second general-purpose ns-3. A compact Rust engine provides ownership of run clock, domain state transitions and trace formats. Protocol-level PHY/MAC study should use ns-3/SNS3/OpenSAND in isolated adapters rather than replicating validated external implementations. Migration to an external primary scheduler remains a benchmark-driven option.

## 2. Deterministic event scheduler

```rust
// Proposed interface, not code that currently exists.
struct EventKey {
    sim_time_ns: u64,
    priority: u16,
    insertion_sequence: u64,
}
enum Event {
    OrbitSnapshotReady { revision: u64 },
    ContactWindowOpened { link_id: LinkId },
    LinkAcquired { link_id: LinkId },
    PacketReady { packet_id: PacketId, node: NodeId },
    PacketArrived { packet_id: PacketId, node: NodeId },
    RouteInstallCompleted { revision: u64 },
    InjectFault { target: EntityId, kind: FaultKind },
}
```

Event ordering by `(sim_time_ns, priority, insertion_sequence)` must be total, stable and independent of hashmap iteration. Reject timestamps earlier than current simulated time; model simultaneous events with documented priority order. Store event origin and causation for replay. All randomness uses derived per-subsystem/per-entity streams from a recorded run seed so adding an unrelated component does not perturb all outcomes.

### Queue and packet engine

Represent packets by compact IDs and shared immutable payload metadata, avoiding whole-buffer copies. Links contain transmission-availability timestamp, directional bitrate, delay model, packet-error model and finite queue limits. An event schedules transmission start/completion and arrival; if link goes DOWN mid-flight, apply a documented delivery/drop policy. Source and sink packet counts reconcile with delivered, dropped, expired and in-flight counts.

### Virtual vs wall time

- Fast simulations: virtual clock advances to next event, independent of wall time.
- Real-IP emulation/SDR: wall time is primary; clock bridge tracks deadline misses and drift.
- External simulator coupling: either offline exchange precomputed traces, conservative time-window synchronization with bounded lookahead, or one engine as authoritative scheduler. No uncontrolled dual virtual clocks.

## 3. Model and fidelity profiles

| Profile | Key behavior | What results are valid | Not automatically valid |
|---|---|---|---|
| L0 analytical | orbit, visibility, aggregate beam/ISL capacity | coverage/contact plans | packet delay/loss |
| L1 packet discrete event | queue/forwarding, modeled PHY/link outages | traceable end-to-end metrics | exact waveform/standard conformance |
| L2 protocol study | SNS3/ns-3/OpenSAND models | specified protocol behavior of selected model | native Starlink compatibility |
| L3 Linux IP emulation | netns/tc/routing daemons, actual apps | application/network stack response | RF bit accuracy |
| L4 HIL/SDR | selected real devices/waveforms | bounded lab experiments | global constellation scale |

Subsystems declare which fidelity profiles they support. A mixed run must record per-link fidelity and prevent double-application of the same loss/delay model.

## 4. Scaling and performance

Start with event counts and memory budget before distributed simulation:

- Precompute contact windows, then generate events at contact **boundaries**, not per-satellite-pair-per-millisecond.
- Spatially filter candidate satellites by shell, orbital plane, horizon and range; avoid naive all-pairs O(N²) every tick.
- Use batch ephemeris evaluation and caching with explicit interpolation error bounds.
- Use sparse topology and incremental route changes; limit trace cardinality and optionally sample only aggregate metrics.
- Queue memory uses configured packet/byte caps; telemetry/export is asynchronous and backpressure-aware.
- Measure 1K/10K satellite contact generation, 10K/1M event execution, packet throughput, memory per entity, and time spent in orbit vs routing vs trace writing. Targets must be benchmark-derived.

Only consider parallel/distributed partitioning after profiling. Partition by causality-safe simulation domains and enforce conservative synchronization; otherwise results may depend on worker execution order.

## 5. Scenario and artifact lifecycle

```text
author YAML/JSON -> schema validate -> resolve orbit inputs
 -> compile immutable scenario manifest -> precompute ephemeris/contact windows
 -> validate scenario geometry and model compatibility -> run virtual events
 -> record packet/event/metrics -> summarize -> compare golden experiment -> archive
```

Artifact manifest: `scenario_hash`, `schema_version`, `run_id`, `seed`, `epoch_utc`, `duration_ns`, git SHA, Rust/Python tool versions, external adapter image digests, input ephemerides/license, declared model/fidelity and generated artifact hashes. A checkpoint includes virtual time, queue contents, RNG stream state, domain revisions and pending events; checkpoint binary format should be explicitly versioned.

## 6. Process/runtime boundaries

Python worker: batch astronomy/orbit calculations; typed JSON/Arrow/Parquet artifact bridge. Rust runner: event state and packets. Linux emulation worker: isolated CAP_NET_ADMIN privileges, lifecycle cleanup, PCAP/metrics. TypeScript console: API/stream viewer. Adapter failure triggers a run failure or explicit degraded-fidelity status; no silent upgrade/downgrade of evidence.

## 7. Validation gates

Determinism with same seed and same inputs; all queue accounting balanced; no packet arrives before transmitted; no RF/ISL contact outside geometric/policy window; bounded link capacity, physical units and frame conversions; failure/recovery scenarios; calibrated reference test vectors; benchmarks and CI matrix. See [validation and roadmap](../roadmap.md).