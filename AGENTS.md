# Engineering guidelines

ntarlink is a Starlink-like satellite internet simulation platform, not a clone of proprietary SpaceX protocols.

Default languages: Rust for deterministic packet/event core and network runtime; Python for scientific/orbital integrations and experiment analysis; TypeScript for UI and visualization. Do not introduce Go into the core. Existing C/C++ external simulators remain supported as adapters, not rewrites.

Before code changes read docs/architecture.md and docs/roadmap.md. Each feature needs a runnable scenario, tests and fidelity/assumption documentation. Maintain deterministic virtual time, versioned contracts and explicit physical units. Prefer upstream open-source capabilities and audit licenses. Do not mark features complete without measured evidence.
