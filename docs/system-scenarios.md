# Cross-subsystem scenario catalog and acceptance

Every scenario is a **reproducible engineering experiment** with scenario/schema revision, epoch, units, fixed seed, fidelity profile, model provenance, named actors, fault schedule, expected invariants and machine-readable packet/event traces.

| Case | Subsystems | Input and action | Mandatory evidence / pass criteria |
|---|---|---|---|
| E2E-01 Broadband cold start | 01/02/03/04/05/06/07/08/09/14 | terminal boots, attaches, authorizes, sends packet via A–B ISL and ground/PoP | end-to-end delivery; ordered events; measured per-hop delay, queue and session state; no zero-time teleportation |
| E2E-02 Mobility handover | 01/02/04/05/07/08/13 | satellite visibility and beam load change over time | terminal and network state transitions agree; counted downtime; same logical service session preserved when configured |
| E2E-03 ISL reroute | 03/07/14/13 | fail primary ISL mid-traffic | topology revision, route installation delay, alternate feasible route or explicit outage, no route loop |
| E2E-04 Feeder outage | 06/07/08/09/13 | ground site loses feeder or terrestrial backhaul | distinct feeder-vs-backhaul diagnosis, gateway reselection and service impact |
| E2E-05 Radio rain fade | 05/06/12/13 | controlled weather cell moves over feeder/user beam | reproducible attenuation, capacity/MCS outcome and throughput/loss change |
| E2E-06 Capacity saturation | 03/05/09/13/14 | oversubscribe beam, ISL or PoP link | finite queue, drop reason, fairness/QoS summary, conservation of packet accounting |
| E2E-07 Country/service policy | 04/05/08/11/12 | terminal moves into prohibited service zone | authorization/access denial at correct owner, audit and explicit failure reason |
| E2E-08 Spacecraft anomaly | 02/03/07/10/11 | authorized operator disables payload | audited command, downstream link loss and route change, mission alarm |
| E2E-09 Real-IP network | 07/09/14/15/16 | linux netns traffic across time-varying impaired link | successful ping/iperf with measured delay/loss and clean teardown |
| E2E-10 Replay and provenance | all/14/16 | run twice with same inputs and seed; change a single parameter | identical trace hash for equal inputs; explicit differences and versioned manifest otherwise |
| E2E-11 Orbital validation | 01/02/12 | reference TLE/epoch and WGS84 ground site | frame conversion verified against golden cases, contact windows geometrically feasible |
| E2E-12 Optional NR NTN | 04/05/08/15 | separate NTN configuration and external OAI/Open5GS stack | protocol-specific observability and license provenance, never asserted as baseline Starlink broadband |

## Experiment manifest minimum

```json
{
  "schema_version": "1.0.0",
  "scenario_id": "e2e-01",
  "run_id": "run-e2e-01",
  "seed": 42,
  "epoch_utc": "2026-10-29T00:00:00Z",
  "duration_ns": 120000000000,
  "fidelity_profile": "L1_DISCRETE_EVENT",
  "model_versions": {"orbital": "static-fixture-v1", "rf": "budget-v1"},
  "inputs_sha256": "...",
  "engine_git_sha": "..."
}
```

## General acceptance invariants

- A transmitted packet is accounted as delivered, explicitly dropped/expired or still in-flight—not silently lost.
- An event cannot occur before its causal prerequisite; source and sink network states are coherent.
- Every link and hop exists, is authorized and available at the event's simulation time.
- Propagation and serialization delays are nonnegative and unit-correct; queue sizes cannot exceed declared limits.
- Trace output and aggregate counters reconcile; failure injection produces deterministic results.
- L0 analytic-only experiments may **not** claim packet loss or throughput measured from actual packets.

## Development and review workflow

A subsystem change requires: its contract change (if any), owner and dependent references, unit/contract tests, at least one integration scenario, deterministic outputs and telemetry, performance notes and an external dependency/license decision if used. Preserve failure fixtures as regression tests.