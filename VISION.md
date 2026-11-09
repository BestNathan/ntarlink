# Vision: Space Computing & Networking Infrastructure Simulator

## Long-term mission

ntarlink is an open-source-first **Space Computing & Networking Infrastructure Simulator**: a reproducible research platform for communication, data transport, compute placement, workload execution, and resource constraints across orbital and terrestrial infrastructure.

A Starlink-like LEO broadband constellation remains the **first network reference architecture and engineering foundation**, not the limit of the platform. Beyond forwarding packets, the platform should support attaching compute nodes to satellite, ground-station, gateway, and terrestrial-network entities; eventually it should be possible to connect real computing hosts and accelerator hardware through explicitly bounded emulation/hardware-in-the-loop (HIL) adapters.

The objective is behavioral and causal fidelity backed by transparent evidence, not reproduction of undocumented SpaceX protocols, operating real spacecraft, or claiming that an analytical GPU model is measured orbital hardware.

## Product capabilities, layered

1. **Orbital and physical infrastructure:** ephemerides, contacts, satellite payload state, power and thermal limits, RF/optical links and gateways.
2. **Communication and network infrastructure:** finite queues, spectrum/beam and ISL capacity, gateway/PoP transport, routing, failure/recovery, and actual IP traffic in optional emulation mode.
3. **Computing and storage infrastructure:** typed compute-node inventory, CPU/accelerator/memory/storage resource profiles, admission, task queues, execution time, energy use, and data artifact movement.
4. **Compute-network orchestration:** contact-aware task placement, constrained transfers, scheduling, retries, and comparison of onboard, ground-only and distributed execution.
5. **Real-hardware integration:** opt-in workers can attach real CPU/GPU devices or compute clusters behind stable adapters and a controlled network-emulation boundary. Their measurements are wall-clock evidence, not deterministic simulated timestamps.
6. **Observability and experiments:** replayable simulations, labeled synthetic data, hardware provenance, trace comparison, uncertainty, resource accounting, and user-visible experiment reports.

Network links and compute facilities are independent but coupled resources. A compute node can be colocated with a satellite, gateway, ground station or terrestrial data center; it is not an unbounded processing-delay field on a packet link. Workloads must explicitly account for input movement, output movement, queueing, capacity, compute execution, storage, failures, and possible missed contact windows.

## Execution modes and evidence

| Mode | Authority and clock | Intended evidence | Honest limitation |
| --- | --- | --- | --- |
| **S1: deterministic modeling** | Single virtual-time simulation authority | Reproducible network, workload and resource outcomes | Analytical service rates and power curves are assumptions, not GPU measurements |
| **S2: network emulation** | Wall-clock Linux network worker, with explicit timing/synchronization contract | Real IP traffic over applied time-varying links | Host timing skew and Linux constraints; no claim of physical RF behavior |
| **S3: compute hardware/HIL** | Real CPU/GPU/accelerator hosts and their own wall clocks | Measured compute time, power where observable, data movement and failure behavior | Non-deterministic measurements; repeat runs and report hardware/software versions and uncertainty |

Never silently mix an S1 virtual timestamp with S2/S3 wall time; mixed experiments require a declared synchronization boundary and fidelity label. Real GPUs need not be present for the first compute milestone.

## Architecture and ownership

The existing 16 subsystem blueprint remains the communications/infrastructure foundation, with authoritative physical/network ownership unchanged. A **proposed future Space Compute & Workload bounded context (candidate SYS-17)** should own workload lifecycle, placement and compute execution state, not spacecraft electrical/thermal facts (SYS-02), contact/link state (SYS-01/03/05/06), routes (SYS-07), or simulation time (SYS-14). Compute resource claims must be admitted against physical owners; network transfers must traverse finite network capacity.

A logical subsystem is not an automatic microservice. Implement the first modeled workloads as Rust domain components under the shared deterministic kernel. External hardware workers, if added, are separately isolated adapters. Do not add empty service skeletons, change the existing 16-page showcase contract, or claim a new subsystem is implemented until its design, contracts, and end-to-end acceptance exist.

See [compute/network/hardware integration boundary](docs/design/compute-network-integration.md), the [architecture](docs/architecture.md), and the [engineering roadmap](docs/roadmap.md).

## Milestone-driven acceptance

- **Foundation:** Preserve the packet/geometry/PAT baseline; converge the currently open SYS-02/Packet Adapter integration without reimplementing proven invariants.
- **Single-node orbital edge computing:** One input data artifact, an explicit simulated task and spacecraft resource budget, local execution, reduced output artifact and scheduled downlink. Compare against ground-only with the same scenario and source data.
- **Multi-node orbital and ground computing:** Several satellites, a ground facility and shared communication constraints; compare local, ground and contact-aware placement policies, including outages and missed deadlines.
- **Network and real-hardware bridge:** Run real IP traffic through time-varying emulated links and attach a real CPU/GPU worker through a versioned adapter; declare skew, provenance, failure boundaries and reproducibility limits.
- **Advanced research:** Calibrated thermal/power models, stronger accelerator profiling, optical-compute fabrics, distributed training/replication and scale studies only when supported by evidence.

Every milestone needs end-to-end tests, observability, resource conservation, fault cases, performance notes, and a clearly declared fidelity level.

## Non-goals of the initial compute milestones

No orbital GPU cluster implementation, real satellite control, proprietary Starlink compatibility, simulated claims about measured FLOPS or watts, real-time guarantees from the virtual-time engine, or premature Kubernetes deployment as the simulation's source of truth.
