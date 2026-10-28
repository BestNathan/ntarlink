# ntarlink

**An open-source-first engineering platform for the simulation and emulation of complete Starlink-like LEO broadband satellite internet systems.**

> **Status: architecture and early scaffolding.** This repository does **not** implement proprietary Starlink protocols. It does not yet simulate a complete end-to-end packet path, real RF or an operational satellite network.

## Engineering design

The platform is decomposed into sixteen domain-defined subsystems spanning orbit and spacecraft, optical ISLs, terminal access, feeder ground infrastructure, network control and subscriber core, terrestrial routing, operations, regulatory/weather constraints, performance assurance, simulation/emulation and visualization.

- [Complete English documentation index](docs/README.md)
- [Full subsystem catalog](docs/system-overview.md)
- [16 subsystem implementation designs](docs/subsystems/README.md)
- [Architecture, planes and runtime](docs/architecture.md)
- [Interfaces, units, contracts and events](docs/design/contracts.md)
- [Packet journey, PHY/link models and mobility](docs/design/data-plane.md)
- [Simulation runtime, coupling, determinism and scale](docs/design/simulation-runtime.md)
- [External dependencies and license review](docs/dependencies.md)
- [Delivery roadmap and acceptance scenarios](docs/roadmap.md)

## Technology

Rust for packet and event core, Python for orbital/scientific models, TypeScript for UI. Established C/C++/Java stacks (ns-3/SNS3, OpenSAND, Orekit and others) are investigated as external adapters. **Subsystems are the architectural boundaries; programming languages are not.**

## Current bootstrap

```sh
cargo test --workspace
cargo run -p ntarlink-cli -- validate scenarios/minimal.json
cargo run -p ntarlink-cli -- simulate scenarios/minimal.json
python3 -m unittest discover -s python/tests
cd web && npm install && npm run check
```

The `simulate` command presently validates/prints scenario metadata; it **does not** simulate packets. See [phase 1](docs/roadmap.md) for the first real packet-flow milestone.

## Contributions and licensing

Read [AGENTS.md](AGENTS.md) for subsystem-first engineering and **English-only repository** rules. Dependency licenses are tracked separately in [docs/dependencies.md](docs/dependencies.md). The project has not yet selected its own open-source redistribution license; see [LICENSE-NOTICE.md](LICENSE-NOTICE.md).