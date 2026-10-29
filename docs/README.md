# ntarlink Engineering Documentation

**Status:** subsystem engineering blueprint with an executable Rust L1 packet engine and an offline Python SGP4/Skyfield orbital-contact compiler. This is not a complete RF/laser/modem/core implementation; see the [current implementation status](implementation-status.md).

## Read in this order

**Interactive product prototype:** [Mission Control Demo information architecture](demo-console.md) and [GitHub Pages deployment](pages.md). The system demo is separate from the platform showcase and contains four routed views per subsystem.

0. [Current implementation status, fidelity and run commands](implementation-status.md).
1. [System mission, functional scope, segment inventory](system-overview.md).
2. [System architecture, control/data/management planes, runtime and target repository design](architecture.md).
3. [Subsystem design index](subsystems/README.md) — 16 detailed, English-only specifications.
4. [Canonical interfaces, schema, time and units](design/contracts.md).
5. [Physical/packet pipeline and cross-system interactions](design/data-plane.md).
6. [Simulation, emulation, fidelity and performance](design/simulation-runtime.md); [shared beam-frame packet grants](design/beam-frame-packet-bridge.md).
7. [Open-source dependency decisions, links, licensing and integration gates](dependencies.md).
8. [Delivery phases, cross-system tests and acceptance evidence](roadmap.md), [scenarios](system-scenarios.md).
9. [Domain boundaries and state ownership](subsystem-boundaries.md); [open-source notes](open-source.md).

## Architectural vocabulary

A **segment** is physical/operational (space, user, ground, terrestrial, operations).
A **subsystem** is a bounded domain with its own authoritative state and commands/events.
A **component** is an independently testable module within a subsystem.
An **adapter** maps an internal contract to an external engine, standard or device.
A **run** is one reproducible virtual-time experiment.
A **fidelity profile** states what has actually been modeled and what has not.

See [design decisions](architecture.md#decisions-and-trade-offs). English is mandatory for source files, documentation, code comments, issue/PR titles created by project agents, identifiers and observable event names.