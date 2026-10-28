# Roadmap

## P0 — executable foundation
- [x] Rust workspace, Python experiment package, TypeScript UI package, cross-language CI scaffold.
- [ ] Deterministic Rust event clock, typed link graph, packet forwarding and trace.
- [ ] Minimal packet path UE → sat A → sat B → gateway → internet sink, latency and loss tests.
- [ ] Versioned scenario schema shared across languages.

## P1 — orbital and link realism
- [ ] Python SGP4/orbital adapter with reference cases and explicit frames.
- [ ] RF link budgets, spot beam scheduling, Doppler, terminal attach and handover.
- [ ] Optical ISL PAT, topology changes and routing.

## P2 — network emulation and UI
- [ ] Linux netns/veth/tc/netem, FRRouting, real IP packets, gateway/PoP.
- [ ] React/CesiumJS constellation, coverage, packet trace and experiment replay.
- [ ] Performance and fidelity benchmarks; fault injection and observability.

## P3 — external stacks
- [ ] Compare/integrate SNS3, OpenSAND, OAI NTN and GNU Radio with pinned versions/licenses.
