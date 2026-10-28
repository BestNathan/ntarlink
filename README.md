# ntarlink

**Open-source-first LEO satellite internet digital twin and network emulator.** This project models the engineering components of a Starlink-like system; it does not claim to implement SpaceX proprietary protocols.

## Goals
- End-to-end IP packet journey: user terminal → spot beam → satellite → inter-satellite links → gateway → terrestrial internet, and reverse.
- Reproducible orbital dynamics, visibility, handover, beam scheduling, RF/optical link budgets, topology, routing and failure injection.
- Deterministic simulation first, then Linux network emulation and optional SDR / 5G NTN integrations.

## Quick start

```sh
go run ./cmd/ntarlink validate -scenario scenarios/minimal.yaml
go run ./cmd/ntarlink simulate -scenario scenarios/minimal.yaml
 go test ./...
```

The initial executable is a **scaffold**: validates scenario structure and prints the declared topology; it does not yet propagate or route packets. See [architecture](docs/architecture.md), [roadmap](docs/roadmap.md), and [open-source inventory](docs/open-source.md).
