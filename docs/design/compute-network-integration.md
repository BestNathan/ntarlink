# Compute, network and hardware integration boundary

**Status:** proposed future architecture; no compute execution runtime or hardware worker is implemented by this document. This design complements the 16 existing bounded contexts. The Packet Adapter SYS-02 shared PAT integration remains an independent in-progress task.

## 1. Topology

Treat physical entities, network endpoints and compute nodes as separately identifiable entities.

- A `Satellite`, `GroundStation`, `Gateway`, or `TerrestrialSite` is a physical/location owner.
- A `NetworkEndpoint` attaches to finite directed links and routing/control policies.
- A `ComputeNode` is placed at a physical site and has an explicit network attachment, resource profile and availability state.
- A `ComputeDevice` identifies modeled or measured CPU/GPU/accelerator capacity. A device is never implicitly equivalent to a satellite.
- A `DataArtifact` has byte size, version/content identity, origin and required destination.
- A `Workload` has input artifacts, resource demands, execution-cost model, deadline, outputs, retry rules and placement constraints.

Resource ownership stays explicit: SYS-02 owns spacecraft power/thermal/health constraints; SYS-07 owns route and traffic decisions; SYS-03/05/06/09 own link or gateway constraints; SYS-14 owns virtual time. The future Space Compute & Workload owner controls task state, execution and placement decisions through typed contracts.

## 2. First deterministic scenario contract

A first opt-in compute extension should define typed, versioned, validated records (not a breaking change to existing packet-only scenario v1):

- `compute_nodes[]`: stable ID, site ID, network endpoint ID, compute capacity model, memory bytes, storage bytes, accelerator profile, resource limits and availability.
- `artifacts[]`: ID, source node or data-generating event, size bytes, lineage/hash and retention policy.
- `workloads[]`: ID, input artifact IDs, release/deadline times, required memory/compute service units, output transformation and placement policy.
- `resource_events[]`: modeled power, thermal, hardware failure/recovery and effective resource derating.
- `execution_profile`: simulation, network emulation or hardware/HIL, with explicit time/fidelity contract and manifest.

Simulation execution follows `InputReady -> Queued -> Admitted -> Running -> Completed/Failed -> OutputTransfer -> Delivered/Expired`. No task may start without inputs and admitted compute/memory/power resources. No artifact may arrive without consuming link capacity, transmission time and any modeled loss/retry cost. Require nonnegative resource ledgers and conservation of transferred/produced artifact bytes.

## 3. Baseline placement policies

Compare using identical scenario inputs and deterministic seeds:

1. `ground_only`: transfer raw input to an attached ground compute node and execute there.
2. `onboard_first`: run on the producing satellite when resource admission permits; downlink output.
3. `earliest_feasible_completion`: evaluate local, peer-satellite and ground placements using available contact windows, queues, service rates, power/thermal admission and deadlines.

These are research baselines, not claims of globally optimal scheduling. Capture rejected placements and resource reasons as events.

## 4. Coupling and invariants

- Use the existing packet/network scheduling authority for transport, not a parallel instantaneous bandwidth counter.
- Reserve compute, storage and transfer resources through explicit owner APIs; never mutate another subsystem's state directly.
- Events record causation IDs, virtual timestamps, resource changes, site/node identity, policy/version and input digests.
- A transfer interrupted by loss of contact must be queued, retried, expired or failed by an explicit policy.
- A satellite eclipse or thermal derating can reduce effective compute service and communications capacity through shared resource admission, without inventing physical calibration.
- For the first slice, compute service rates and watts are analytical inputs; published/measured device profiles can be added later with provenance and uncertainty.

## 5. Real compute-hardware attachment (later, optional)

Provide a versioned `ComputeWorkerAdapter` contract: `DiscoverCapabilities`, `ReserveResources`, `StageArtifacts`, `StartWorkload`, `ObserveExecution`, `CancelWorkload`, `CollectArtifacts`, `ReleaseResources`. Declare device identifiers, accelerator runtime, supported container/host execution, CPU/GPU/memory/storage capabilities, authentication and isolation. Use least privilege and never execute untrusted scenario text as shell commands.

A CPU host, GPU workstation or Kubernetes worker can represent a **ground or laboratory-attached** compute node. An orbital compute node is modeled unless actual flight hardware and time-series telemetry are explicitly available. Attach workers to Linux netns/veth/tc/FRRouting emulated network endpoints only in the wall-clock execution profile; enforce flow control, link-loss behavior and clock-skew accounting. Keep GPU execution and the deterministic per-event simulation loop in separate processes. Measure host wall time, queue delay, transport bytes, device usage and attributable energy where instrumentation permits; never label these as deterministic or orbital hardware measurements.

## 6. Required experiments and completion gates

| Milestone | Demonstration | Required comparison/evidence |
| --- | --- | --- |
| C0 | Simulated satellite sensor -> local task -> compressed result -> downlink | Raw-data ground-only baseline; output correctness, deadlines, bytes, time and resource ledgers |
| C1 | Two compute-capable satellites + one ground node under changing contacts | Three placement policies, outage/retry, finite queues, stable seed/hash and explainable placement |
| C2 | Real ping/iperf and application data across time-varying emulated links | End-to-end packet delivery, convergence, time drift and environment manifest |
| C3 | Real CPU/GPU worker attached via a network-emulation endpoint | Task lifecycle, authenticated resource admission, artifact integrity, failure recovery and measured-vs-modeled report |

Do not claim any gate passed from documentation alone; CI and observed experiment artifacts are required.

## 7. Rollout and compatibility

Keep current P0/P1 packet scenarios unchanged. Add opt-in versioned compute records and parser tests first. The current GitHub Pages demo must remain clearly synthetic and its 16-entry navigation must not be changed until a formal subsystem/catalog migration is separately reviewed. A new bounded context designation (candidate SYS-17) and any added UI entry need a dedicated contract/catalog/Pages validation change.

Do not expand the scope of the in-progress Packet Adapter SYS-02 Fault/Head/Observation PR. Land compute/network work through independent milestones and PRs.
