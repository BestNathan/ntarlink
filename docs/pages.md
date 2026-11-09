# GitHub Pages: Platform Showcase and Mission Control Demo

The [Platform Showcase](https://bestnathan.github.io/ntarlink/) introduces the long-term Space Computing & Networking Infrastructure mission and the current 16-subsystem LEO communication foundation. The separate [Mission Control Demo](https://bestnathan.github.io/ntarlink/demo/) is a multi-view system prototype with its own overview, navigation and subsystem pages. Sources are [pages/index.html](../applications/console/pages/index.html) and [pages/demo/index.html](../applications/console/pages/demo/index.html), published together by [.github/workflows/pages.yml](../.github/workflows/pages.yml). See the detailed [Demo Console design](demo-console.md).

**This is an architecture demonstration, not an operational ground or space network.** Every satellite count, orbit illustration, link state, throughput, RTT, subscriber count and event in the dashboard is **synthetic mock data**. The currently implemented Rust packet runtime and Python SGP4/Skyfield contact compiler are documented separately in [implementation status](implementation-status.md).

## Platform Showcase capabilities

The showcase now separates the existing satellite-network foundation from the **planned** orbital compute/workload and hardware attachment milestones. The separate [Compute Fabric Lab](https://bestnathan.github.io/ntarlink/demo/#/compute) is a synthetic analytical comparison workspace, not a live compute cluster or Rust workload executor.

- A responsive mission control interface displaying an illustrative constellation, optical ISLs, ground feeders and telemetry.
- Sixteen individual subsystem panels with their most important capability groups, implementation status, dependency candidates and links to detailed English design documents.
- A clickable end-to-end packet journey connecting terminal, radio access, satellites, ISL, feeder, ground PoP and internet.
- Example mode switching: nominal operation, optical ISL loss, feeder gateway failure and demand surge.
- A scrub-able, optionally playing mock timeline, illustrative network health graph, event feed and key aggregate metrics.
- No external map tiles, no external JavaScript CDNs, no secrets and no real-world telemetry.

## Demo Console features

- A dedicated hash-routed Compute Fabric Lab shows synthetic compute-capable satellite and ground nodes, three workloads, network fault conditions and ground/onboard/peer placement alternatives. Its estimates and resource profiles are disclosed illustrative assumptions; no GPU, CPU worker, or real orbital AI execution is connected.

- A separate Mission Control overview with synthetic constellation health, conceptual network topology, a clickable packet journey and mock scenario presets.
- Sixteen subsystem workspaces grouped by their architectural segment; each has four deep-linkable views: Overview, Assets & Inventory, Operations and Telemetry & Analytics.
- Distinct mock inventory and example actions for orbital, spacecraft, ISL, user access, ground feeder, subscriber core, terrestrial routing, network control, operations, security, environment, assurance and platform subsystems.
- Mock actions visibly change local inventory state and append a simulated event, without contacting a backend.
- A local-only browser JSON importer for real Rust packet simulation reports, kept separate from all synthetic dashboard values.
- A fully static, responsive implementation without backend infrastructure or secrets. The demo uses optional web fonts but no external JavaScript services.

## Deployment

1. A repository administrator goes to **Settings → Pages → Build and deployment → Source** and selects **GitHub Actions** if it is not already configured.
2. After merge to `main`, the **Publish ntarlink Showcase and Demo Console** workflow automatically uploads the complete `applications/console/pages/` directory and deploys it to the `github-pages` environment. Manual `workflow_dispatch` is also available.
3. Check the workflow deployment output. The expected routes are `https://bestnathan.github.io/ntarlink/` and `https://bestnathan.github.io/ntarlink/demo/`, but **do not describe either as live until deployment succeeds and the URL responds**.
4. GitHub Pages can be public even if the repository is private. Do not publish credentials, confidential topology or operational data.

The site is intentionally a self-contained HTML/CSS/JavaScript prototype under the existing TypeScript console application. This keeps first publication independent of framework/build infrastructure. The actual typed console in `applications/console/src` is preserved for future React/CesiumJS work.

## Validation

CI executes `python scripts/check_pages_site.py` for the showcase and `node scripts/test_demo_console.cjs` for the demo. The latter tests all 16 subsystems × 4 routed views (64 screens), overview, synthetic compute-fabric interactions and link-fault behavior, mock-data labels and graph generation. Both checks gate the Pages workflow. Browser-level interaction and responsive screenshot tests are planned follow-ups.

Updates to `applications/console/pages/**` or `.github/workflows/pages.yml` on `main` trigger automatic re-deployment. Changes to engineering documents alone do not trigger the static site workflow unless the presentation itself needs to change.
