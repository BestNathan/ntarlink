# GitHub Pages system showcase

The static [ntarlink satellite systems lab](https://bestnathan.github.io/ntarlink/) is an interactive engineering showcase for the complete 16-subsystem architecture. Its source is [applications/console/pages/index.html](../applications/console/pages/index.html) and deployment workflow is [.github/workflows/pages.yml](../.github/workflows/pages.yml).

**This is an architecture demonstration, not an operational ground or space network.** Every satellite count, orbit illustration, link state, throughput, RTT, subscriber count and event in the dashboard is **synthetic mock data**. The currently implemented Rust packet runtime and Python SGP4/Skyfield contact compiler are documented separately in [implementation status](implementation-status.md).

## Capabilities

- A responsive mission control interface displaying an illustrative constellation, optical ISLs, ground feeders and telemetry.
- Sixteen individual subsystem panels with their most important capability groups, implementation status, dependency candidates and links to detailed English design documents.
- A clickable end-to-end packet journey connecting terminal, radio access, satellites, ISL, feeder, ground PoP and internet.
- Example mode switching: nominal operation, optical ISL loss, feeder gateway failure and demand surge.
- A scrub-able, optionally playing mock timeline, illustrative network health graph, event feed and key aggregate metrics.
- No external map tiles, no external JavaScript CDNs, no secrets and no real-world telemetry.

## Deployment

1. A repository administrator goes to **Settings → Pages → Build and deployment → Source** and selects **GitHub Actions** if it is not already configured.
2. After merge to `main`, the **Publish System Showcase** workflow automatically uploads the static site and deploys it to the `github-pages` environment. Manual `workflow_dispatch` is also available.
3. Check the workflow deployment output. The default project URL is expected to be `https://bestnathan.github.io/ntarlink/`, but it must **not be described as live until the workflow succeeds**.
4. GitHub Pages can be public even if the repository is private. Do not publish credentials, confidential topology or operational data.

The site is intentionally a self-contained HTML/CSS/JavaScript prototype under the existing TypeScript console application. This keeps first publication independent of framework/build infrastructure. The actual typed console in `applications/console/src` is preserved for future React/CesiumJS work.

## Validation

The existing CI executes `python scripts/check_pages_site.py` to enforce the 16-subsystem document links, explicit mock-data labels and valid client-side JavaScript syntax using `node --check`.

Updates to `applications/console/pages/**` or `.github/workflows/pages.yml` on `main` trigger automatic re-deployment. Changes to engineering documents alone do not trigger the static site workflow unless the presentation itself needs to change.
