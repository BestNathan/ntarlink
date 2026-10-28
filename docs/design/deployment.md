# Execution topology, repository layout and integration deployment

## 1. Process topology by fidelity mode

### Developer baseline (minimal Rust/Python/TypeScript)

```text
workstation
  ├─ Python orbit/experiment batch processor (offline snapshots)
  ├─ Rust CLI + simulation runner
  │     ├─ Event Clock / Run Controller / Fault Injector
  │     ├─ Space + Access + Feeder + Core + Routing state modules
  │     ├─ Packet / Queue / Link Data Plane
  │     └─ Event Trace + Artifact Export
  └─ TypeScript Console (snapshot + timeline + command API)
```

The runner uses **in-process typed events** for high-rate packet operations. Python is a preprocessing worker with explicit artifacts. Console actions go through a control API; it does not own authoritative simulation state.

### Protocol study

```text
scenario compiler -> offline adapter -> ns-3 / SNS3 / OpenSAND
                                             |
                  imported packet/metric trace + model provenance
                                             |
                                 comparison / calibration reports
```

Only one simulator executes packet events in a given protocol study. If cross-engine synchronous coupling is eventually implemented, introduce a formal time coordinator and safe lookahead barriers.

### Real-IP network emulation

```text
run controller -> authenticated privileged Linux worker
                       ├─ namespace(s) / veth / qdisc / routes
                       ├─ FRRouting daemon(s) (optional)
                       ├─ ping / iperf / synthetic endpoints
                       ├─ packet capture and interface statistics
                       └─ deterministic teardown journal
```

Use dedicated worker capabilities and per-run resource IDs; never grant the web/API service CAP_NET_ADMIN. Real-IP mode runs on wall time, and must report skew between target and effective impairment schedule.

## 2. Target monorepo organization and packages

```text
systems/
  orbital/{python,models,tests}            # SYS-01
  spacecraft/{rust,tests}                  # SYS-02
  space-network/{rust,adapters,tests}      # SYS-03
  terminal/{rust,adapters,tests}           # SYS-04
  access-network/{rust,adapters,tests}     # SYS-05
  ground-segment/{rust,adapters,tests}     # SYS-06
  network-control/{rust,tests}            # SYS-07
  subscriber-core/{rust,tests}            # SYS-08
  terrestrial-network/{rust,adapters}     # SYS-09
  operations/{rust,tests}                 # SYS-10
  security/{rust,tests}                   # SYS-11
  environment/{python,rust,tests}         # SYS-12
  service-assurance/{rust,python,tests}   # SYS-13
platform/
  contracts/{schemas,fixtures,generated}
  simulation/{rust,tests}
  emulation/{linux,adapters,tests}
  observability/{rust,schemas}
integrations/{ns3,sns3,opensand,oai,open5gs,gnuradio,orekit,frr}
applications/{cli,api,console}
scenarios/{minimal,handover,isl-failure,gateway-failure}
experiments/{benchmarks,baselines,reports}
docs/{subsystems,design}
```

Directories are **target layout**, not a requirement to create dozens of empty packages now. Rust's root `Cargo.toml` remains a workspace manifest with path-based crates. Python uses `pyproject.toml` for orbital/scientific packages. TypeScript uses application packages with generated client schema bindings.

## 3. Dependency direction rules

- `platform/contracts` defines canonical schema **without importing domain modules**.
- Domain libraries import contract types and simulation interfaces, not browser/HTTP handlers.
- `platform/simulation` dispatches typed domain events but does not import a domain's internal policy into generic kernel algorithms.
- `integrations/*` depends inward on contracts/domains; domain state machines must work without an external adapter.
- `applications/*` calls run/domain API; no direct writer access to other domain stores.
- Cross-domain algorithms requiring state from multiple owners use snapshots and explicit revision, not shared mutable globals.
- Protocol adapters may be independent containers/processes. They are not allowed to change a subsystem's authoritative state without a contract command.

## 4. Build, test and deployment matrices

| Target | Runtime | CI expectation |
|---|---|---|
| Rust runner/libs | supported Linux/macOS, pinned stable toolchain | `cargo fmt`, clippy, unit/contract/E2E tests |
| Python science | pinned supported CPython and dependencies | lint, unit, orbit golden tests and frame conversions |
| TypeScript console | Node LTS, browser | typecheck, unit and headless visualization smoke test |
| Documentation/contracts | no runtime dependency | English-only lint, local links, schema/example validation |
| Linux emulation | privileged disposable Linux CI runner | gated integration jobs, cleanup and kernel version matrix |
| ns-3/SNS3/OpenSAND | pinned Linux containers/binary versions | scheduled/nightly integration and numerical comparison |
| HIL/SDR | dedicated lab only | manual/controlled gate, no standard PR requirement |

## 5. Operational deployment configuration

Separate **immutable experiment input** from runtime secrets and worker infrastructure. A run manifest includes scenario schema revision, model selections, orbital input hash, toolchain/dependency digest, seed, clock profile, resource caps and artifact destination. Control API auth and per-run permissions restrict fault injection/operational commands. Never expose privileged emulator container or management keys to web clients.

## 6. Scaling strategy and exit conditions

Start with an embedded runner and compact artifact store. Profile event queue size, contact density, packet throughput and trace storage before considering process separation. Split a subsystem only if there is an independent ownership/release, fault isolation, scaling or hardware reason. For constellation-scale performance, reduce contact candidate graph first and use batch orbital artifacts; distributed execution is a researched extension, not a default requirement.