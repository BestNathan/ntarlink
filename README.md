# ntarlink

**Open-source-first LEO broadband satellite internet simulation and emulation platform.**

ntarlink is organized by **subsystem responsibility**, not by programming language. Rust drives deterministic packet and event simulation, Python provides orbital/geometry experiments, and TypeScript describes console-facing scenario and trace data. Existing ns-3, SNS3, OpenSAND and SDR tools remain potential external adapters, not claimed to be integrated.

> **Current fidelity: L1, static-contact modeled packets.** The simulator now executes packet events across a configured user RF link, optical ISL, feeder gateway, terrestrial backhaul and internet edge, and models per-hop propagation/serialization/processing/queue delay, finite queues, link failure and delayed routing reconvergence. Link availability and timing are scenario inputs; **no orbital propagation, real RF waveform, PAT acquisition, MAC scheduling or Starlink proprietary protocol is implemented.**

## Try it

```sh
cargo test --workspace
cargo run -p ntarlink-cli -- validate scenarios/minimal.json
cargo run -p ntarlink-cli -- simulate scenarios/minimal.json
cargo run -p ntarlink-cli -- simulate scenarios/isl-failure.json --json > report.json
python3 -m unittest discover -s systems/orbital/python/tests
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
scenarios/                   # Minimal and ISL-failure examples
docs/                        # Full system and 16 subsystem engineering designs
```

See [implementation status](docs/implementation-status.md), [engineering documentation](docs/README.md), [subsystems](docs/subsystems/README.md), [contracts](docs/design/contracts.md), [limitations and roadmap](docs/roadmap.md), and [dependency decisions](docs/dependencies.md).

## Scope and licensing

ntarlink is a research and simulation project and does not reproduce undocumented Starlink hardware, modem or protocol implementations. The repository itself has **not yet selected a redistribution license**, as described in [LICENSE-NOTICE.md](LICENSE-NOTICE.md). Third-party license and validation audits are required before integration.

Repository content and engineering documentation are English-only; see [AGENTS.md](AGENTS.md).
