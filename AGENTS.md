# ntarlink engineering guidelines

ntarlink models a complete Starlink-like LEO broadband internet system, not proprietary SpaceX protocol compatibility.

## Architecture first
- Read docs/README.md, docs/system-overview.md, docs/subsystem-boundaries.md and docs/system-scenarios.md before designing features.
- Organize by bounded **subsystem responsibility**, not by Rust/Python/TypeScript language. Target structure is documented in docs/architecture.md; code migration is separate.
- Each subsystem owns its domain state, publishes versioned contracts, and documents explicit non-goals. A logical subsystem need not be a network microservice.
- Keep simulation, protocol study, Linux IP emulation and HIL fidelity profiles distinct.

## Engineering
- Rust for deterministic event and packet runtime, Python for scientific/orbital/experiments, TypeScript for visualization and UI. External C/C++ upstream simulators remain adapters.
- Use deterministic virtual time, explicit units/reference frames, seed and model provenance. Do not perform per-packet remote RPC by default.
- Every change includes executable acceptance evidence, test/benchmark plan and failure modes. Do not mark subsystem capability complete because a folder exists.
- Audit upstream versions/licenses before integrating, and avoid unsupported assertions about Starlink proprietary internals.
