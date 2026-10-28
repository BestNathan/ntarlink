# Architecture — Rust / Python / TypeScript

## Goal
Model an entire Starlink-like LEO satellite internet system with clear fidelity levels: analytical models, packet-level discrete-event simulation, Linux packet emulation, and optional real PHY/SDR integrations. No claim of Starlink proprietary compatibility.

## Rust — simulation core and runtime
Deterministic virtual clock; event queue; typed topology; packet queues and forwarding; ISL contact state; user link and feeder link state; routing, handover and gateway selection; link budget interfaces; metrics and traces. Rust core must not depend on Python or browser code.

## Python — scientific workflows and integration
Orbital propagation using maintained libraries, ephemeris and coordinate-frame conversions, scenario generation, experiment orchestration, model validation, plotting and analysis. Interoperate through versioned JSON/Arrow/Parquet or IPC; choose concrete format based on throughput benchmarks.

## TypeScript — UI
Interactive CesiumJS globe, coverage and spot beams, ISL topology, packet journey, scenario editor, telemetry and experiment replay. Frontend consumes versioned experiment snapshots and live event streams, not internal Rust structs directly.

## Existing open-source integrations
Use adapters for OpenSAND, SNS3/ns-3, OAI, GNU Radio, StarryNet/OpenSN, Orekit/SGP4 and Linux netns/veth/tc/netem/FRRouting where useful. Keep independent simulators as external processes; do not rewrite mature PHY/MAC implementations in Rust just to unify language.

## Data plane
UE/LAN → user terminal → RF user link → satellite payload → optional optical ISL → feeder link → gateway → PoP/backhaul → internet sink; reverse direction supported later. Satellite payload may be bent-pipe or regenerative, explicit in scenario.

## Control plane
Orbit/contact prediction → beam/spectrum scheduling → terminal admission/handover → topology/routing → gateway selection → policy and failure injection. All simulation-time behavior deterministic with seeded randomness and explicit units.

## Invariants
- Protocol/physics models are labeled by fidelity and evidence.
- No undocumented Starlink PHY/MAC assumptions stated as fact.
- Unit-safe geometry, timestamps and coordinate frames.
- Versioned scenario contracts; deterministic replay; reproducible tests.
- Adapter dependency licenses and versions reviewed before integration.
