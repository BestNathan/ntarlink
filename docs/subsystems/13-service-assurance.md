# SYS-13 — 服务质量与用户体验

> Status: design specification; implementation not implied. Logical bounded context, not necessarily an independently deployed service.

## 1. Position and mission

**Position:** 从终端视角量化网络质量与可用性.

**Purpose:** Provide a separately testable domain with explicit authority over its state and measurable contracts. It participates in a whole-system Starlink-like broadband experiment while avoiding assumptions about undisclosed SpaceX implementations.

## 2. Core domain entities

- `ServiceProbe`
- `SLAObjective`
- `Outage`
- `QualityReport`
- `CustomerExperience`

Each entity must eventually have a schema, identifier, lifecycle and ownership rules. Physical quantities require units and coordinate/time conventions.

## 3. Internal modules and detailed capabilities

### 3.1 端到端吞吐/RTT/jitter/loss

Implement as an independently verifiable capability: specify inputs, output/state transition, physical or protocol assumptions, error handling, metrics and reference tests. Choose analytical approximations before protocol-accurate models where appropriate.

### 3.2 业务可用性

Implement as an independently verifiable capability: specify inputs, output/state transition, physical or protocol assumptions, error handling, metrics and reference tests. Choose analytical approximations before protocol-accurate models where appropriate.

### 3.3 切换中断时长

Implement as an independently verifiable capability: specify inputs, output/state transition, physical or protocol assumptions, error handling, metrics and reference tests. Choose analytical approximations before protocol-accurate models where appropriate.

### 3.4 拥塞根因关联

Implement as an independently verifiable capability: specify inputs, output/state transition, physical or protocol assumptions, error handling, metrics and reference tests. Choose analytical approximations before protocol-accurate models where appropriate.

### 3.5 按用户/区域/波束聚合

Implement as an independently verifiable capability: specify inputs, output/state transition, physical or protocol assumptions, error handling, metrics and reference tests. Choose analytical approximations before protocol-accurate models where appropriate.

### 3.6 QoS达标分析

Implement as an independently verifiable capability: specify inputs, output/state transition, physical or protocol assumptions, error handling, metrics and reference tests. Choose analytical approximations before protocol-accurate models where appropriate.

### 3.7 主动探测

Implement as an independently verifiable capability: specify inputs, output/state transition, physical or protocol assumptions, error handling, metrics and reference tests. Choose analytical approximations before protocol-accurate models where appropriate.

### 3.8 SLA报表

Implement as an independently verifiable capability: specify inputs, output/state transition, physical or protocol assumptions, error handling, metrics and reference tests. Choose analytical approximations before protocol-accurate models where appropriate.

### 3.9 实验基线对比

Implement as an independently verifiable capability: specify inputs, output/state transition, physical or protocol assumptions, error handling, metrics and reference tests. Choose analytical approximations before protocol-accurate models where appropriate.

## 4. Boundaries and dependencies

- **Primary upstream:** terminal, access-network, space-network, ground-segment, subscriber-core, terrestrial-network, observability.
- **Primary downstream:** operations, applications.
- **Boundary:** this subsystem owns only its domain state and behavior; external orchestration, shared event timing, visualization and experiment storage belong to platform services. Avoid implementing neighboring domains inside this module.
- **Control plane:** receives versioned commands and configuration; emits state transition events.
- **Data plane:** if relevant, operates on typed link/packet/bearer state without cross-process RPC per packet.

## 5. Interfaces and contracts

**Initial API surfaces:** 端到端trace;业务探测;SLA规则;报告导出.

**Domain events:** `ProbeResult`, `ServiceDegraded`, `OutageStarted`, `OutageEnded`, `SLAReportReady`.

Define each message's producer, consumer, schema, timestamps, idempotency and failure semantics before implementation. Consumers must tolerate delayed, duplicated and stale events where execution mode permits.

## 6. Lifecycle and failure modes

**Expected failure/edge cases:** 观测缺口; 采样偏差; 时钟不一致; 归因错误.

State transitions must be observable. For each fault, specify detection, propagation to dependent systems, impact on traffic, recovery and user-visible outcome. Fault injection is an explicit test input.

## 7. Technology and open-source integration

**Candidates:** OpenTelemetry/Prometheus/Grafana作为采集展示.

Use Rust for performance-sensitive state/event/packet logic, Python for scientific/orbital/experiment workflows and TypeScript for UI when applicable. Existing C/C++ stacks remain independent adapters. These are candidates, not approved or verified dependencies.

## 8. Verification and acceptance

**Priority acceptance cases:** 端到端路径时延分解; 故障时可用性下降; 重复实验指标一致.

Each case needs reproducible input, fixed seed where stochastic, expected invariants, per-hop/per-entity trace, and explicit fidelity label. Cross-system acceptance is indexed in [system scenarios](../system-scenarios.md).

## Architectural contract and implementation requirements

### Domain model and ownership
- Own the entities above as authoritative state; external systems consume snapshots or events, not direct mutation.
- Explicitly document each state machine (initial, active, degraded, failed, recovering) and legal transitions. Avoid global singleton mutable state.
- All measurements include units; geometry includes reference frame and epoch; time-bearing events distinguish simulated time, orbital UTC and wall clock.

### Interfaces and consistency
- Version messages with `schema_version`, `run_id`, `scenario_id`, `sim_time_ns`, `sequence`, `source`, `trace_id`.
- Separate queries, commands and domain events. Define idempotency keys and error codes for mutating commands; allow batched snapshots for large constellations.
- In deterministic runs, process events in stable timestamp/priority/sequence order. Hot-path packet handling must not depend on synchronous remote RPC.

### Fidelity and performance
- **L0 analytical:** geometry/aggregate capacity without packets; **L1 discrete event:** packets, queues, modeled links; **L2 protocol:** external validated PHY/MAC stacks; **L3 emulation:** real Linux/IP packets; **L4 HIL:** optional SDR/hardware under legal constraints.
- Every model records assumptions, source, validation reference, version, seed and expected scalability. Report measured performance, not hypothetical throughput.

### Test and operational requirements
- Unit tests cover model invariants and invalid inputs; contract tests cover versioning; integration tests cover upstream/downstream events; E2E tests cover failure and recovery.
- Emit domain events, per-entity metrics, trace correlation and structured error reasons; capture artifacts for reproducibility.
- Never claim proprietary Starlink compatibility; third-party integration requires license, maintenance and model-fidelity review.

## 9. Delivery milestones

- **M0 Contract:** entities, states, interfaces, units, scenario fixtures and failure semantics reviewed.
- **M1 Analytical:** deterministic simplified model with unit and contract tests.
- **M2 Integrated:** participates in a whole-system E2E scenario with trace and fault injection.
- **M3 Fidelity:** reference-calibrated physical/protocol model or reviewed upstream adapter.
- **M4 Scale/Operations:** benchmarks, observability, resource limits and recovery validated.

M0 documentation does **not** mean M1–M4 is implemented.
