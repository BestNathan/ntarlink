# External dependencies and integration decisions

**Policy:** prioritize existing validated libraries and protocol stacks where they improve scientific fidelity. A dependency is not “adopted” until a reproducible proof-of-concept, pinned version, platform matrix, security review, feature gap analysis, license review and baseline benchmark exist. Links below point to project-maintained sources; licensing statements reflect sources checked during architecture drafting and must be revalidated for the exact pinned revision.

## 1. Candidate dependency register

| Component | Domain(s) | Role and concrete integration surface | Published licensing / important constraint | Decision |
|---|---|---|---|---|
| [python-sgp4](https://github.com/brandon-rhodes/python-sgp4) | SYS-01 | Historical TLE propagation via Skyfield; offline contact compilation | MIT; SGP4 returns TEME, not ITRS | **Integrated**: pinned `sgp4==2.27` |
| [Skyfield](https://rhodesmill.org/skyfield/) | SYS-01 | SGP4 TEME-to-ITRS transform and Earth-fixed WGS84 geometry | MIT; built-in timescale avoids live network downloads | **Integrated**: pinned `skyfield==1.55` |
| [Orekit](https://gitlab.orekit.org/orekit/orekit) | SYS-01/02 | high-fidelity orbit/attitude/event detection; separate Java worker | Apache-2.0; JVM/distribution overhead | **Optional higher-fidelity adapter** |
| [ns-3](https://www.nsnam.org/) | SYS-03/05/09/15 | C++ discrete-event protocol stack and packet-level comparison | GPL-2.0; independent engine owns its simulation time | **Evaluate P2**; never merge two event clocks implicitly |
| [SNS3 satellite](https://github.com/sns3/sns3-satellite) | SYS-03/05/06 | ns-3 extension; DVB satellite link, beams, interference and routing/reference experiments | GPL-3.0; current README documents aligned **ns-3.48** satellite, traffic, stats tags—pin compatible revisions | **Evaluate P2**, not Starlink PHY equivalence |
| [OpenSAND](https://www.opensand.org/overview.html) | SYS-05/06/15 | DVB-S2/RCS2 user/feeder, gateway and real-packet satellite emulation | GPL-3.0/LGPL-3.0 modules; protocol assumptions and Linux runtime | **Optional protocol benchmark/emulator** |
| [OpenAirInterface RAN](https://gitlab.eurecom.fr/oai/openairinterface5g) | SYS-05/15 | separate **3GPP NTN** UE/gNB stack study, RF/HIL | **CSSL v1.0** for current RAN; historical tags had different terms, do not assume generic Apache/MIT | **Optional NTN profile only** |
| [Open5GS](https://github.com/open5gs/open5gs) | SYS-08 | EPC/5GC for independent NTN architecture experiment | **AGPL-3.0** for open-source files; network service obligations must be reviewed | **Optional NTN core**, never baseline broadband core |
| [GNU Radio](https://www.gnuradio.org/) | SYS-05/06/15 | waveform/IQ and SDR host-process interface | copyleft licenses/blocks vary; hardware regulations apply | **Optional HIL lab**, not scale simulator |
| [FRRouting](https://frrouting.org/) | SYS-07/09/15 | BGP/OSPF/IS-IS/BFD and real routing behavior in Linux namespaces | generally GPL-2.0-or-later, mixed per-file licenses | **Preferred IP emulation** |
| [Linux netns/veth/tc/netem](https://man7.org/linux/man-pages/man8/tc-netem.8.html) | SYS-09/15 | packet impairment and host namespace transport | Linux platform privileges, kernel feature availability, no protocol RF fidelity | **Preferred real-IP backend** |
| [CesiumJS](https://cesium.com/platform/cesiumjs/) | SYS-16 | globe, WGS84 positions, orbit/beam/ground visual layers | Apache-2.0; Cesium ion and third-party map asset terms separate | **Preferred UI visualization** |
| [React](https://react.dev/) / [TypeScript](https://www.typescriptlang.org/) | SYS-16 | console components, scenario editor, trace timeline | package-by-package dependency/licensing audit | **Preferred UI implementation** |
| [OpenTelemetry](https://opentelemetry.io/) / [Prometheus](https://prometheus.io/) | SYS-10/13/16 | traces, service health, metrics aggregation | evaluate dependency/collector costs and high-cardinality exposure | **Preferred observability conventions** |
| [StarryNet](https://github.com/SpaceNetLab/StarryNet) / [OpenSN](https://opensn-library.github.io/) | SYS-07/15 | routing, Linux/container-based dynamic constellation emulation patterns | licenses and implementation status **must be checked before adoption** | **Reference/benchmark candidates** |

**Current installed dependencies:** Rust `serde`/`serde_json`, pinned Python `skyfield==1.55` and `sgp4==2.27` in the orbital package, and TypeScript compiler. All other projects in this table are still adapter candidates. This register is a roadmap **not** a declaration that the dependencies are installed.

## 2. Selection and licensing policy

### Classification

- **Tier A (reuse directly):** stable, maintained, documented API, compatible license and determinism/performance.
- **Tier B (run as external adapter):** high fidelity but C/C++/Java/GPL/AGPL or environment-constrained. Prefer process boundary and versioned artifacts, **not** as a blanket legal workaround: distribution, linking, AGPL and combined-work questions still require review.
- **Tier C (reference only):** research implementation, uncertain maintenance, heavy deployment assumptions, missing license or limited reproducibility.
- **Tier D (do not adopt):** proprietary undisclosed internals, conflicting licenses/exports, unvalidated behavior or unsupported operational risk.

### Dependency gate checklist

1. Record repository, exact tag/commit, source URL, maintainer, release date and transitive dependencies.
2. Verify licenses **per file and release**, required notices and redistribution obligations; choose project license separately (currently `UNLICENSED` placeholder, not a granted OSI license).
3. Build/execute containerized reference test with declared host/CPU/kernel, documented data inputs and hashes.
4. Record APIs, error cases, time model, packet/PHY layers modeled and known blind spots.
5. Compare a golden reference scenario against ntarlink baseline with tolerances and explain differences.
6. Capture performance and resource benchmarks; no shared control plane state mutation without versioned contracts.
7. Track upstream changes in CI on a controlled schedule; do not auto-upgrade physics modules.

## 3. Integration architecture by upstream type

**Orbit propagation:** Python batch invocation -> frame-aware `PositionSnapshot` / `ContactWindow` artifact. Validate TEME→ITRF and sidereal/Earth orientation against trusted reference; never treat unconverted TEME coordinates as WGS84 geodetic locations.

**ns-3/SNS3:** isolate scenario build and C++ simulator process. One side owns packet scheduling for a protocol profile; exchange input link/contact files and output packet traces. Phase one uses **offline comparison**, not bidirectional event synchronization.

**OpenSAND:** Linux emulator adapter instantiates ground/user interfaces and channel profile from a constrained, versioned scenario. Capture actual IP traffic; map packets to correlation IDs and compare application metrics.

**OAI/Open5GS:** distinct `profile: nr_ntn`. Maintain a separate device/subscriber core mapping and RAN/core orchestration; this must not change the baseline “broadband proprietary-interface approximation” into an asserted 5G design.

**GNU Radio:** IQ/SDR process runs under bounded realtime hardware conditions; the simulator exchanges link/channel configuration and timestamped observations, not massive baseband samples over the control API.

**FRR/netns:** provision privileged Linux worker; create netns/veth/tc and routing daemons, update qdisc impairments by schedule, collect PCAP, guarantee cleanup on crashes.

**CesiumJS/UI:** receive compressed snapshot/delta timeline with WGS84 coordinates; never read raw packet events at constellation scale into browser memory.

## 4. Known provenance references

- [ns-3 manual/tutorial](https://www.nsnam.org/docs/tutorial/singlehtml/) and [GPL-2.0 statement](https://www.nsnam.org/about/what-is-ns-3/).
- [SNS3 README: license, required modules and ns-3 version alignment](https://github.com/sns3/sns3-satellite).
- [OpenSAND official overview and GPL/LGPL statement](https://www.opensand.org/overview.html).
- [SGP4 implementation, TEME caveat](https://github.com/brandon-rhodes/python-sgp4).
- [Orekit project and declared Apache license](https://gitlab.orekit.org/orekit/orekit).
- [OAI licensing model](https://openairinterface.org/oai-license-model/) and [CSSL](https://openairinterface.org/oai-cssl/).
- [Open5GS license](https://github.com/open5gs/open5gs).
- [FRRouting README and per-file licensing](https://github.com/FRRouting/frr).
- [CesiumJS Apache-2.0](https://cesium.com/platform/cesiumjs/).

External citations/links document initial research; **pinning an exact version is a future engineering task**. Do not infer production Starlink compliance from any selected dependency.