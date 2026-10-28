# SYS-13 — Service Assurance, Reliability, Quality and Customer Experience

**Mission:** evaluate whether a user actually receives acceptable connectivity and explain its degradation through correlated packet, link, session and infrastructure evidence. This system does not own the raw packet engine or issue network routes.

## Components and measurement points

| Component | Technical role |
|---|---|
| Probe engine | synthetic ICMP-like echo, HTTP/TCP/UDP application flows, DNS and service-specific SLA probes |
| Metric collector | per-flow TX/RX counts, goodput, RTT, one-way delay, jitter, loss, queue delay and handover gap |
| Availability engine | classify user online/offline/degraded by explicit policy and sample cadence |
| Outage detector | start/end intervals, severity, impacted terminals/region/beam, root-cause hints |
| SLA evaluator | thresholds and percentile windows for region, subscriber class and experiment |
| Correlation engine | link/route/session/ground events tied to observable user impact via trace IDs |
| Report builder | per-user/beam/gateway/region summaries, experiment comparisons, provenance and artifact links |

## Measurement methodology

Define every metric precisely. **Goodput** counts delivered application payload, not RF coding/framing bits. **Packet loss** needs denominators and expiry/measurement window; packets still in flight at run end are not automatically dropped. **RTT** covers application/transport round trip and possibly asymmetric paths. **Jitter** should be a declared metric (e.g. inter-arrival variation or delay-variation percentile), not an unqualified number. Availability uses explicit probe cadence, outage debounce and policy for missing samples.

Attribution chain: `TerminalDisconnected`, `BeamResourceExhausted`, `IslLinkLost`, `FeederLinkLost`, `BackhaulLost`, `RouteChanged`, `SessionSuspended` → associated affected packets, probes and application sessions. Root-cause classification distinguishes radio vs optical vs subscriber vs terrestrial vs regulatory issues. If several failures overlap, record multiple candidates and an uncertainty flag instead of asserting a single root cause.

For an end-to-end service objective, record both numerator/denominator and observation period. Report error bars or stochastic seeds for simulation ensembles. Aggregation windows should use **simulated time** during deterministic runs, not the wall-clock time required to run a large simulation.

## Interfaces and ownership

Owned entities: `ServiceProbe`, `MetricSeries`, `AvailabilityWindow`, `OutageIncident`, `SlaObjective`, `ExperienceReport`. Commands: `ScheduleProbe`, `SetSlaObjective`, `EvaluateRun`, `CompareRunBaselines`. Events: `ProbeCompleted`, `ServiceDegraded`, `OutageDetected/Recovered`, `SlaObjectiveViolated`, `ExperienceReportPublished`. Input data is read-only from all networking subsystems and SYS-16 trace storage; SYS-10 consumes alarms and incident impact.

Report output includes fidelity profile, model/software revision, seed, confidence limits, missing observations and exact time interval. Avoid treating an L0 aggregate model as measured application throughput.

## Technology and dependencies

Rust streaming calculators for high-rate counters/latency sketches, Python pandas/NumPy optional postprocessing for experiments, OpenTelemetry/Prometheus conventions for exporting metrics, Grafana dashboards optional. Keep high-cardinality packet identifiers in trace/artifact store rather than Prometheus labels; store rollups by selected cardinality budget. Version metrics and aggregation rules.

## Failure conditions and acceptance

Probe gaps, clock mismatch, event out-of-order, incomplete trace, high-cardinality overload, partial service recovery, DNS dependency failure and noisy handover. Error reporting must state insufficient evidence where appropriate.

**Tests:** exact packet accounting; known fixed-delay path produces expected RTT; injected handover gives outage duration matching attachment and packet trace; concurrent backhaul and RF failures do not produce an invented single root cause; same run yields identical histogram/percentiles. KPIs: service availability %, p50/p95/p99 latency, loss by reason, goodput, interruptions/hour, failure recovery interval and regional demand-weighted experience.