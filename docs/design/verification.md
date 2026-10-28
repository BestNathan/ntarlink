# Fidelity, numerical validation, benchmarking and evidence

**Goal:** ensure results mean what they claim. Every reported number must be tied to the model profile, input data, units, experiment seed and reference tests. No estimated throughput should be called a measured SDR transmission rate.

## 1. Scientific truth hierarchy

1. **Defined invariant:** time never regresses, no route over unavailable links, packet accounting balances.
2. **Independent numerical reference:** Vallado/SGP4 regression vectors, geometry transformations, link-budget hand calculations, public published MAC/RF studies.
3. **Cross-engine comparison:** ns-3/SNS3/OpenSAND under equivalent documented inputs; discrepancies explained.
4. **Emulation measurements:** Linux packet capture/iperf/ping with host/kernel and wall-clock limitations reported.
5. **HIL measurement:** controlled RF/SDR with calibrated hardware and regulatory requirements satisfied.

A source of reference data must have a reproducible link, exact revision, relevant configuration and a numeric tolerance. Tolerances should be determined from model/measurement accuracy, **not arbitrarily tuned to pass CI**.

## 2. Reference datasets and golden tests

| Domain | Golden inputs | Assertions |
|---|---|---|
| Orbit | fixed TLE/OMM, epoch, Earth orientation dataset | vector/frame/time conversion within selected reference tolerance |
| Contact | satellite+ground WGS84 sites with known horizon | rise/set interval and Earth occultation consistent with geometry |
| RF | fixed distance, frequency, powers, antenna gains and noise | FSPL/SINR/MCS calculations unit-correct and monotonic |
| Optical ISL | endpoints, range, beam divergence, pointing/attenuation | PAT delay/state, propagation and link budget trends |
| Packet | known sized packets, capacity, processing and queues | transmission finish time and per-hop delivery/drop invariant |
| Routing | contact graph with scheduled failure | path feasibility, loop freedom, convergence and gateway choice |
| Sessions | synthetic subscriber, bearer handover and PoP change | IP/session continuity or explicit failure |
| Emulation | qdisc/netns/FRR environment, test flows | measured impairments and clean teardown |
| Visualization | orbit positions and event timestamps | correct frame/time rendering, replay parity |

## 3. Simulation evidence schema

Each test artifact provides: inputs and SHA256, scenario schema version, run seed, virtual epoch, engine/source git SHA, exact dependency versions/images, coordinate/time conventions, selected model equations and coefficients, route algorithm revision, system CPU/kernel where relevant, fidelity profile and optional stochastic confidence intervals.

Every E2E experiment records packet traces per hop, lost packet/drop reason, link/contact revision, route revision, subscriber session revision, event causation and service-level summary. Aggregate counters should reconcile with trace samples after accounting for explicitly documented sampling.

## 4. Verification pyramid

- **Unit and property tests:** dimensional analysis, frame transforms, event ordering, queues, deterministic RNG, component state machines, idempotence and payload compatibility.
- **Contract tests:** Rust/Python/TypeScript golden fixtures, unsupported schema-major rejection, external adapter input/output checks.
- **Domain integrations:** access-terminal-core, orbital-ISL-controller, feeder-backhaul-PoP, operator-security-spacecraft.
- **Cross-system scenarios:** [E2E-01–12](../system-scenarios.md) with failure/recovery and expected traces.
- **External engines:** pinned release builds; cross-engine scenario comparison and license/maintenance review.
- **Performance/reliability:** bounded memory, long runs, 1K/10K satellites, 1M events, UI bounded rendering, adapter crash cleanup.

## 5. Performance budgets to measure

Metrics to collect rather than assert prematurely: event dispatch events/s, wall-clock time per simulated hour, orbit generation satellites×samples/s, average candidate ISL degree, route computation ms/graph revision, packets/s, bytes and events per trace, peak resident memory, Python worker duration, P95 UI query latency and emulation time skew. Record benchmark machine and configuration; publish change trends with baseline diffs.

## 6. Fidelity and compliance guardrails

Satellite modem/IP routing assumptions, optical budget, environmental link loss, 3GPP NTN and DVB protocols are separate models. A validation report must state where the public standard differs from Starlink-like behavior and must never imply proprietary compliance. A weather attenuation input is a model assumption, not proof of licensed RF performance. Maintain third-party notices and platform-specific deployment controls.