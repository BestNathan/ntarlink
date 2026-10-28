# ntarlink system architecture

The primary decomposition is **subsystem responsibility**, not programming language. See [catalog](system-overview.md), [boundaries](subsystem-boundaries.md), and [acceptance scenarios](system-scenarios.md).

## Deployment shape

One **monorepo**, multiple bounded contexts, and configurable execution profiles. A subsystem is not necessarily a standalone microservice: small deterministic runs may link Rust domain modules in one process; orbital calculation may run in Python; a Linux emulation worker may be isolated. Each retains explicit ownership and contracts. Never impose network RPC per simulated packet.

## Target repository organization

```text
systems/
  orbital/              # orbit, frames, ephemeris
  spacecraft/           # fleet inventory, attitude and payload availability
  space-network/        # optical ISL and space forwarding
  terminal/             # user terminal / CPE
  access-network/       # user RF, beams, MAC and mobility
  ground-segment/       # feeder links and stations
  network-control/      # routing, contact graph and traffic engineering
  subscriber-core/      # sessions, policy, IP assignment
  terrestrial-network/  # PoP, backhaul, internet edge
  operations/           # NOC and mission control
  security/             # trust and audit
  environment/          # weather, interference and spectrum policy
  service-assurance/    # service experience and SLA
platform/
  simulation/           # deterministic event engine
  emulation/            # Linux/IP/external runtime adapters
  contracts/            # shared versioned schemas and units
  observability/        # traces, metrics and experiment artifacts
integrations/           # ns3, SNS3, OpenSAND, OAI, SDR, orbital libs
applications/           # web console, CLI
scenarios/              # versioned integrated scenarios
experiments/            # repeatable experiment runs and reports
```

The current Rust/Python/TypeScript bootstrap remains in place until a dedicated code migration. This documentation **does not assert that the target directories or subsystem implementations already exist**.

## Language allocation within domains

Rust: deterministic event runtime, routing, link state, packet queues, performance-sensitive components. Python: orbital/scientific models, experiments, calibration and adapter glue. TypeScript: UI, visualization and scenario editor. Existing C/C++ simulators remain external dependencies/adapters; avoid rewriting their mature PHY/MAC for language uniformity.

## System flows

Broadband user plane: LAN → terminal → access RF → satellite payload → optional optical ISL → feeder ground station → terrestrial backbone/PoP → internet. Control plane: orbital/spacecraft/environment state → contact feasibility → access/network decisions → installed forwarding state → observability/operations. Core/session services provide subscriber policy, not necessarily a 3GPP 5GC.

## Non-goals and rigor

No undocumented SpaceX PHY/MAC reverse-engineering claims. Each subsystem defines authoritative state, interfaces, state machines, units, time semantics, fidelity and acceptance tests. Simulated, emulated and hardware-integrated results must be labeled separately. Every upstream integration requires version/license and model-accuracy review.
