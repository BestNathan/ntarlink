# ntarlink

**Starlink-like LEO satellite internet simulation and emulation platform.** Open-source-first, reproducible and explicitly not an implementation of proprietary Starlink protocols.

## Stack
- **Rust**: deterministic event-driven packet/network simulation core, typed topology and link models.
- **Python**: orbital analysis, scenario generation, experiments, external scientific integrations.
- **TypeScript**: web visualization and interactive experiment UI.
- **Adapters**: ns-3/SNS3, OpenSAND, OpenAirInterface, GNU Radio, Linux netns/tc and FRRouting where practical; adapters do not dictate core language.

## Run the bootstrap

```sh
cargo test --workspace
cargo run -p ntarlink-cli -- validate scenarios/minimal.json
cargo run -p ntarlink-cli -- simulate scenarios/minimal.json
python3 -m unittest discover -s python/tests
cd web && npm install && npm run check
```

`simulate` currently validates and reports the scenario; it **does not yet simulate packets**. See [architecture](docs/architecture.md) and [roadmap](docs/roadmap.md).
