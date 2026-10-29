# ntarlink

**Open-source-first LEO broadband satellite internet simulation and emulation platform.**

ntarlink is organized by **subsystem responsibility**, not by programming language. Rust drives deterministic packet and event simulation, Python provides orbital/geometry experiments, and TypeScript describes console-facing scenario and trace data. Existing ns-3, SNS3, OpenSAND and SDR tools remain potential external adapters, not claimed to be integrated.

> **Current fidelity: L1 packet simulation with an optional P1 offline orbital-contact bridge.** The deterministic Rust runner models packet paths, finite queues, per-hop delay, link faults and control-plane convergence. Python now generates geometric contact windows from historical TLEs using SGP4/Skyfield and WGS84, which drive timed link availability and routing changes. This is **not** a real RF modem, laser PAT, access MAC or proprietary Starlink protocol.

## Try it

```sh
cargo test --workspace
cargo run -p ntarlink-cli -- validate scenarios/minimal.json
cargo run -p ntarlink-cli -- simulate scenarios/minimal.json
cargo run -p ntarlink-cli -- simulate scenarios/isl-failure.json --json > report.json
python -m pip install -e systems/orbital/python
python3 -m unittest discover -s systems/orbital/python/tests
python -m ntarlink_orbital.contacts scenarios/orbital-pass.json scenarios/orbit-sources.sample.json --out /tmp/orbital-pass.json
cargo run -p ntarlink-cli -- simulate /tmp/orbital-pass.json --json > /tmp/orbital-report.json
cd applications/console && npm install && npm run check
```

The JSON report includes per-packet status, drop reason, route revisions and a timestamped, hop-by-hop packet trace.

## Repository architecture

```text
platform/contracts/           # Versioned scenario model and schema
platform/simulation/          # Deterministic virtual-time event scheduler
platform/packet-runtime/      # Generic modeled link queues and packet transport
systems/network-control/      # Routing and delayed convergence
systems/orbital/python/       # WGS84 geometry/scenario preprocessing
applications/cli/            # Validate, simulate and export JSON trace
applications/console/        # Typed UI contracts; interactive UI planned
scenarios/                   # Static, fault, contact handover and historical ISS orbit fixtures
docs/                        # Full system and 16 subsystem engineering designs
```

See [implementation status](docs/implementation-status.md), [engineering documentation](docs/README.md), [subsystems](docs/subsystems/README.md), [contracts](docs/design/contracts.md), [limitations and roadmap](docs/roadmap.md), and [dependency decisions](docs/dependencies.md).

## Scope and licensing

ntarlink is a research and simulation project and does not reproduce undocumented Starlink hardware, modem or protocol implementations. The repository itself has **not yet selected a redistribution license**, as described in [LICENSE-NOTICE.md](LICENSE-NOTICE.md). Third-party license and validation audits are required before integration.

Repository content and engineering documentation are English-only; see [AGENTS.md](AGENTS.md).
