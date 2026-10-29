# Interactive Demo Console: UX, navigation and implementation plan

## Product positioning

The GitHub Pages site has two independent experiences:

1. **Platform Showcase** — `/ntarlink/` ([source](../applications/console/pages/index.html)): explain the Starlink-like engineering goals, architecture, implementation status and subsystem boundaries.
2. **Mission Control Demo** — `/ntarlink/demo/` ([source](../applications/console/pages/demo/index.html)): an actual navigable product prototype with a system-level operations overview and 16 subsystem workspaces. All built-in values, entities and operations are **synthetic client-side examples**.

The demo has its own application shell and route state. It is not an extension of the marketing/showcase dashboard. Its initial screen is a network operations workspace, not an introduction.

## Information architecture

```text
/ntarlink/
  Platform overview, architecture, documentation, launch demo CTA
/ntarlink/demo/#/overview
  Mission control
  ├── Network KPIs / synthetic alerts
  ├── Interactive user-terminal → ISL → feeder → PoP topology
  ├── Four experiment scenario presets and virtual-time slider
  ├── End-to-end packet journey and event feed
  ├── Six grouped subsystem entry points (16 systems)
  └── Import local Rust SimulationReport JSON / export mock snapshot
/ntarlink/demo/#/system/{id}/overview
  Subsystem capabilities, internal components, ownership and links to docs
/ntarlink/demo/#/system/{id}/assets
  Domain-specific entity inventory, health/status/reading and inspector
/ntarlink/demo/#/system/{id}/operations
  Domain-specific example actions and local event journal
/ntarlink/demo/#/system/{id}/telemetry
  Domain-specific metric labels, synthetic trend and dependency references
```

The exact URL uses hash-based routing so it works on plain GitHub Pages with no server-side rewrite rules. Every subsystem tab has an address that can be bookmarked. Browser Back/Forward navigation uses native hash history.

## System workspaces

| System | Subsystem-specific entity | Operations demos |
|---|---|---|
| SYS-01 Orbital Dynamics | satellites, orbital shells | recompute passes, inject ephemeris drift |
| SYS-02 Spacecraft | spacecraft and payloads | safe mode, payload resource release |
| SYS-03 Optical ISL | inter-satellite laser links | link loss, PAT reacquisition |
| SYS-04 Terminals | user terminals | modem restart, reacquisition |
| SYS-05 Access | spot beams | beam rebalance, handover |
| SYS-06 Ground | feeder sites and antennas | station outage, feeder switch |
| SYS-07 Network Control | installed routes | recompute, topology failure |
| SYS-08 Subscriber Core | subscriber sessions | suspend session, renew lease |
| SYS-09 Terrestrial | PoPs and backhaul | egress reroute, transit maintenance |
| SYS-10 Operations | incidents and alarms | acknowledge, schedule maintenance |
| SYS-11 Security | policies and credentials | authorization denial, credential rotation |
| SYS-12 Environment | weather, interference, regulatory regions | rain fade, spectrum conflict |
| SYS-13 Assurance | SLA targets and probes | probe, compare reports |
| SYS-14 Simulation | scenario runs and event queues | launch mock run, inject fault |
| SYS-15 Emulation | protocol lab adapters | provision lab, apply delay |
| SYS-16 Observability | trace and report artifacts | replay trace, export snapshot |

The navigator groups these logical bounded contexts by space, access, ground/core, network control, operations and platform. They are **not assumed to be sixteen deployed microservices**.

## Interaction and mock-data contracts

The demo uses `applications/console/pages/demo/data.js` as a local illustrative catalog. Its `version`, `groups`, `systems` and `scenarios` are read by `app.js`. No HTTP requests are made to a simulation server or operator network. UI mock actions only change an in-memory status override, append a local event and display a confirmation. Reloading the browser resets this state.

Changing scenario presets updates aggregate sample KPI values and visual topology degradation. The clock slider and play control advance a *synthetic* time label; they do not advance the Rust event scheduler. The inventory tables, charts and mock actions are explicitly labeled. Status "WORKING COMPONENTS" denotes code that exists in the repository and **does not mean the displayed dashboard figures came from that code**.

### Import a real simulation report

The mission overview can optionally read a `ntarlink-cli simulate scenarios/minimal.json --json` report directly from a local file chooser. The browser validates a minimal `SimulationReport` shape and shows the imported packet totals, fidelity and first packet journeys in a **separate, clearly marked panel**. The file is not uploaded or written to storage. It does not overwrite mocked satellite, gateway, subscriber or RF metrics.

This separation is intentional. The future backend can provide a run manifest and domain-specific telemetry, but should not convert mock metrics into "live" metrics by changing their label.

## Later adapter architecture

The initial app is plain HTML/CSS/JavaScript, served as a static Pages subdirectory. A future React/TypeScript console should keep these architectural boundaries:

- `CatalogProvider`: subsystem registry, ownership, views and capability descriptors.
- `ScenarioProvider`: scenario configuration and source type (synthetic vs real experiment).
- `SimulationRunsProvider`: read-only runs/trace/manifest fetch, start/pause/fault commands to the authorized run API.
- `OrbitalProvider`: propagated ephemeris snapshots and frame-aware positions.
- `SubsystemProvider`: per-domain typed queries, state snapshots and authorized operations.
- `ArtifactProvider`: trace query, download and provenance (not unverified aggregation).

System pages should be **shell + domain plugin views**, with source badges per panel, typed schemas, loading/error states and scope-based authorization. No per-packet RPC to backend services is implied. A Pages deployment is permanently read-only/demo unless an explicitly authorized remote API is configured; do not embed secrets in static assets.

## Deployment

The Pages workflow [`pages.yml`](../.github/workflows/pages.yml) copies **the complete** `applications/console/pages/` tree to the Pages artifact, preserving both the root showcase and the nested `demo/` directory. Relative paths support project Pages under `/ntarlink/`.

Default expected URLs:

- Showcase: `https://bestnathan.github.io/ntarlink/`
- Demo: `https://bestnathan.github.io/ntarlink/demo/`
- Example subsystem: `https://bestnathan.github.io/ntarlink/demo/#/system/space-network/assets`

Public availability requires repository Pages to have **GitHub Actions** selected as deployment source and a successful Pages deployment. Source-code existence or a green PR CI is not proof the public website is live.

## Validation

Run:

```bash
python scripts/check_pages_site.py
node --check applications/console/pages/demo/data.js
node --check applications/console/pages/demo/app.js
node scripts/test_demo_console.cjs
```

The dependency-free Node smoke test mounts the client application in a stub DOM, checks the home screen, navigates through all 16 systems and all four routed tabs (64 subsystem screens), and confirms labels, inventory tables, mock actions and charts. These tests gate both PR CI and Pages publication. A later browser E2E suite should test real click interactions and responsive screenshots against desktop/mobile viewports.

## Next design iterations

Prioritize real orbital/route/packet reports, CesiumJS globe over authoritative WGS84 snapshots, interactive route/path comparison, beam footprints, optical ISL acquisition telemetry, gateway station maps, subscriber session drill-down, operational incident flows and shared run IDs. Feature depth should follow actual subsystem implementation, not invent backend capabilities for a visually rich dashboard.
