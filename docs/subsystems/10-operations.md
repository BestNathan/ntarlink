# SYS-10 — Network Operations Center and Space Mission Operations

**Mission:** enable operational supervision and auditable human/automation actions across the satellite fleet and network, not replace domain owners' control algorithms or the simulator's virtual clock.

## Operational components

1. **Telemetry ingestion:** spacecraft health, payload availability, station and feeder status, link/route revisions, user service impact and platform run metrics.
2. **Alarm and incident service:** stateful alarm rules, severity, suppression/correlation, impact estimation, incident opening/escalation/closure.
3. **Fleet operations:** commission/retire/safe-mode spacecraft, payload reset and maintenance workflows through the spacecraft domain.
4. **Network operations:** operational constraints, gateway maintenance, approved route policies, beam/site reconfiguration via SYS-05/06/07.
5. **Command workflow:** operator intent, authenticated authorization, policy validation, optional two-person approval, scheduled execution, acknowledgment and audit.
6. **Capacity and planning:** per-region demand, station saturation, beam utilization, ISL topology incidents, service impact reports.
7. **Operations console:** read-only situational view by default; execution commands are explicit actions with run ID and time.

## State machines and execution

An alarm is `CLEAR → FIRING → ACKNOWLEDGED → RESOLVING → CLEARED`, with severity, source evidence, count and deduplication key. An incident is `OPEN → TRIAGED → MITIGATING → MONITORING → RESOLVED`. Operator commands are `REQUESTED → AUTHORIZED → APPROVED → SCHEDULED → APPLIED/REJECTED → AUDITED`. In a simulated experiment, issuance and acknowledgment occur at **simulated timestamps**; UI arrival is wall time and must not move the virtual clock without an explicit command.

Example payload disable: controller/operator requests operation; SYS-11 validates authority; SYS-10 logs approval; SYS-02 changes payload state; SYS-03/05/06 lose associated links; SYS-07 recomputes routes; SYS-13 measures service disruption; SYS-16 archives cause chain. The operations service **must not mutate link tables directly**.

## Interfaces and stored entities

Own `Alarm`, `Incident`, `OperatorAction`, `ChangePlan`, `MaintenanceWindow` and operation history. Consume event/traces from all systems and SYS-13 service assurance. Commands: `AcknowledgeAlarm`, `RequestOperationalAction`, `ApproveChange`, `ScheduleMaintenance`, `CloseIncident`. Events: `AlarmRaised`, `IncidentOpened`, `ActionApproved/Rejected`, `ChangeApplied`, `IncidentResolved`.

Command record requires actor/role, target, expected target revision, justification, approval decision, simulated effective time, result and audit hash. Duplicate action IDs cannot execute twice.

## OSS and implementation

P3 Rust operational state machines and event-correlation rules, TypeScript NOC console and metrics. [Prometheus](https://prometheus.io/), [Grafana](https://grafana.com/) and [OpenTelemetry](https://opentelemetry.io/) for standard observability conventions; [Yamcs](https://yamcs.org/) can be evaluated for mission telemetry/command workflow experiments, not taken as evidence of actual satellite integration. Historical/archived experiment events may be re-indexed without treating them as current live alarms.

## Faults, security and verification

Alarm storm, late telemetry, partial state, failed/duplicated command, permission denial, missing approval, contradictory configuration and false-positive incident. Audit must preserve cause and operator intent.

**Acceptance:** one payload fault creates coherent alarm and packet service impact; denied command leaves spacecraft untouched; repeated command ID is idempotent; maintenance takes feeder site offline only at scheduled time; incident resolves after verified downstream recovery. Metrics: mean time to detect/restore, false alarms, command latency, audit completeness and affected subscriber count.