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


## 5. C2 adapter selection and evidence gate (2026-11-12)

This is the **decision record** for research issue #5, not a mandate to integrate
every simulator. Source repositories and observed maintenance/license metadata
were checked on 2026-11-12. An upstream repository's latest moving branch is
not a pinned dependency. Exact release, per-file license and transitive notices
remain mandatory **when** a deferred component is selected for adoption.

| Candidate | Decision | Reason and operational boundary |
| --- | --- | --- |
| Linux netns/veth/tc through iproute2 | **Selected: isolated S2 C2 POC** | Real Linux IP forwarding, kernel namespace isolation and impairment control. Separate wall-clock worker; never replace the deterministic S1 event engine |
| iputils-ping, iperf3, tcpdump | **Selected: S2 test tools only** | Real RTT, throughput and PCAP evidence; host processes, no binary vendoring or in-process linking |
| FRRouting | **Deferred until C2 routing step** | [FRRouting/frr](https://github.com/FRRouting/frr) was active on 2026-11-12; protocol routing/convergence is useful but daemon privileges and mixed per-file GPL notices need a pinned release, review and alternate-path experiment first |
| OpenSAND | **Deferred protocol comparison** | DVB emulation is not the baseline broadband PHY; separate GPL/LGPL, Linux runtime and benchmark would be required |
| ns-3 and SNS3 | **Deferred independent packet-reference comparison** | [SNS3](https://github.com/sns3/sns3-satellite) was active in October 2026 and declares GPL-3.0. Its ns-3 ownership and simulation clock must not silently merge with S1 |
| OAI RAN and Open5GS | **Deferred separate NR-NTN profile** | [Open5GS](https://github.com/open5gs/open5gs) was active in November 2026 and declares AGPL-3.0. OAI uses CSSL terms; neither is a prerequisite for the IP gateway reference path |
| GNU Radio | **Deferred isolated SDR/HIL laboratory** | [GNU Radio](https://github.com/gnuradio/gnuradio) is maintained and GPL-3.0, but IQ/SDR and radio compliance are outside the first S2 network result |
| Orekit | **Deferred higher-fidelity orbit adapter** | JVM, time/frame and accuracy comparison with already integrated Skyfield/SGP4 must justify this additional engine |
| CesiumJS | **Deferred optional live evidence display** | [CesiumJS](https://github.com/CesiumGS/cesium) declares Apache-2.0 and is active, but neither map imagery nor a 3D UI is needed to prove Linux IP forwarding; the current Pages demo is synthetic |
| StarryNet and OpenSN | **Reference-only** | [StarryNet](https://github.com/SpaceNetLab/StarryNet) declares MIT and remains available; adoption would introduce a second emulation orchestration layer. OpenSN requires license/version audit **if promoted**, not integration for its own sake |
| Python SGP4 and Skyfield | **Already integrated under SYS-01** | Exact versions pinned in the orbital package and covered by existing Python/scientific reference tests; no new decision required |

### Executable acceptance and reproducibility

- The minimal selected C2 stack has real GitHub-hosted evidence from
  [S2 Linux workflow](https://github.com/BestNathan/ntarlink/actions/runs/43152811922)
  on the reviewed PR #153 code: live 4-namespace/3-link forwarding,
  outage and recovery, nonzero iperf3 received bytes, nonempty real PCAP,
  successful cleanup, and Python dry-run safety tests. The companion CI
  checks Rust, Python, TypeScript and docs independently.
- The input is the versioned [S2 golden scenario](../scenarios/s2-linux-ip-path.json).
  The worker emits its SHA-256, selected routes, timestamps, applied impairment,
  ping/iperf observations, kernel snapshot, and cleanup results. The
  measurement is **S2 wall-clock** and is not an S1 virtual-time packet trace.
- Tested hosted OS: Ubuntu 24.04. The successful host reported Linux kernel
  7.0.0-1017-azure, iproute2 command version 6.1.0 and iperf3 3.16. The
  [host dependency evidence script](../scripts/record_s2_host_deps.py) now
  fails closed if those reviewed command versions drift, records **exact**
  installed Debian package revisions and SHA-256 of each installed package's
  copyright notice, and uploads this manifest alongside ping/PCAP evidence.
- External tools are executed from distro packages, not linked, vendored or
  distributed by ntarlink. GitHub license summary may be NOASSERTION even for
  maintained projects (notably FRR, iperf3 and tcpdump); review the recorded
  **installed** copyright/notice texts and any combined-work/distribution
  obligations before packaging a worker image. An upstream GPL/AGPL license
  cannot be treated as permissive because it runs in another process.
- Ubuntu runner tags and apt indexes are **not content-addressed immutable
  images**. The command-version gate and complete per-run manifest make the
  prototype inspectable and fail closed on drift, but do not provide fully
  hermetic replay. A digest-pinned worker image, audited package lock and
  host-kernel compatibility matrix are required before production deployment.
- Any **newly selected** component, notably FRRouting, must first acquire an
  exact version/commit, copyright/transitive audit, reproducible external
  golden scenario, environment manifest, fault tests, performance baseline
  and explicit clock/authority contract. This research decision closes the
  dependency *selection* question; the engineering acceptance lives in #4.

Do not claim an external dependency is integrated merely because it appears
in this register. No proprietary Starlink air-interface compatibility is implied.
