# SYS-07 — Network Control, Contact Planning, Routing and Traffic Engineering

**Mission:** decide which feasible contacts to activate, which next hops to install and which feeder/PoP paths to use as the network changes. The controller is logically centralized in the first experiment, but **does not own physical link state, data-plane queues, satellite positions or subscriber identities**.

## Functional components

| Module | Description |
|---|---|
| Topology aggregator | merge orbit contact candidates, acquired ISLs, feeder state, access load and terrestrial reachability into a revisioned graph |
| Contact scheduler | choose link acquisition/teardown and scarce optical/antenna resource allocation over time windows |
| Routing planner | shortest-path baseline followed by time-dependent/contact-aware routing with deadlines and constraints |
| Traffic engineer | capacity-aware path cost, multi-path planning, congestion response, service class priorities |
| Gateway/PoP selector | separate feeder candidate, ground backhaul and internet egress cost/policy |
| Rule distributor | propagate FIB revisions to regenerative satellites/routers with control latency and acknowledgments |
| Failure manager | topology debounce, fast reroute vs global recomputation, convergence measurements |
| Policy/compiler | compile operator, region, security and subscriber constraints into route feasibility predicates |

## Time-dependent graph representation

Vertices include satellites, feeder ground stations, PoPs and endpoint attachment nodes. Edges are **directed** and have `[start_ns,end_ns)` validity, acquired state, effective bitrate, processing/propagation delay, monetary/energy cost proxy, queue estimate and policy tags. An access beam and optical ISL are different edge kinds. Only physically and operationally usable links enter the active forwarding graph; predicted contacts are available to the planner as **future** possibilities.

Baseline P1: compute Dijkstra on a snapshot using `cost = propagation + processing + queue_estimate + configurable_penalty`; reject zero capacity, inaccessible destinations and expired contacts. For more accurate satellite networks, use time-expanded graphs or earliest-arrival/contact-graph routing with a link reservation feasibility check. Global updates must preserve monotonic topology revisions and avoid forwarding loops; paths can become stale during control propagation.

Routing is not instantaneous: `contact event → telemetry/control propagation → topology aggregation → computation → rule distribution → FIB commit`. Each stage has explicit virtual-time cost. Use a route epoch identifier so packets report which forwarding revision selected each hop. If a downstream link expires after route computation, SYS-03 drops/queues according to policy; SYS-07 recomputes.

## Gateway and traffic engineering

Selecting a ground exit minimizes **space route + feeder acquisition/capacity + ground backhaul + PoP egress + policy** under availability constraints, not merely geometric distance to the nearest ground station. Prefer gateway diversity; simulate load and terminal handover affecting path demand. Support explicit link disjointness or risk groups for resilience. Avoid sending an internet packet to an unavailable feeder while reporting a successful route.

Policy separates subscriber service eligibility (SYS-08), regulatory region (SYS-12), operational approval (SYS-10) and path engineering (SYS-07). Controller cannot override denied authorization; it may only select among permitted paths.

## Interfaces

Own `TopologyRevision`, `ContactPlan`, `RoutePlan`, `RouteRevision`, `GatewaySelection`, `TrafficDemandMatrix`. Subscribe to `VisibilityWindow`, `IslLinkEstablished/Lost`, `BeamCapacityChanged`, `FeederLinkEstablished/Lost`, `BackhaulCongested`, `PolicyChanged`. Commands: `AcquireIsl`/`RequestFeederAcquisition` to owners, `InstallForwardingRules` to data-plane nodes, `SwitchGateway`. Events: `TopologyRevisionPublished`, `RoutePlanComputed`, `RouteInstalled`, `GatewayReselected`, `ConvergenceCompleted`.

## OSS and runtime

Rust routing library with bounded graph revisions and deterministic event integration. [StarryNet](https://github.com/SpaceNetLab/StarryNet)/[OpenSN](https://opensn-library.github.io/) inform topology emulation, not necessarily replace it. [FRRouting](https://frrouting.org/) is used only for real Linux IP emulation/protocol experiments and may not implement space contact-graph routing. Cross-system messages defined in [contracts](../design/contracts.md).

## Verification and metrics

Route correctness on an acquired static graph; multi-hop ISL path; contact expiry path invalidation; link fault and no-loop reconvergence; gateway failure including independent backhaul issue; constrained selection vs shortest geographic path; worst-case convergence bound. Observe route computation duration, event-to-rule-install latency, packet blackhole intervals, path stretch, capacity headroom, gateway switches and packet loss during reconvergence.

**Build order:** graph and static Dijkstra → FIB install events → gateway selection → dynamic contacts/faults → time-expanded planning and traffic engineering.