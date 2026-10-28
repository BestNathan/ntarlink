# Agent engineering instructions

Goal: reproducible engineering simulation of a Starlink-like LEO satellite internet network, not an undocumented claim of protocol compatibility.

Before implementation: read docs/architecture.md and docs/roadmap.md; define interface boundaries and measurable acceptance tests. Prefer existing well-maintained open-source components, but verify licenses, APIs and feature support. Keep orbital mechanics, access scheduling, ISL routing and packet simulation independently testable. No speculative physics constants without citations. Simulation is not production RF implementation. For each PR include runnable scenario, tests, benchmarks where appropriate, and scope limitations. Never mark roadmap tasks complete without executable evidence.
