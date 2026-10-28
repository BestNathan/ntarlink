# Architecture: domain-driven LEO satellite internet simulator

**Architecture status:** in progress. The first deterministic packet slice has moved into `platform/contracts`, `platform/simulation`, `platform/packet-runtime`, `systems/network-control` and `applications/cli`; Python orbital helpers live under `systems/orbital/python` and TypeScript contracts under `applications/console`. These are real modules, not the complete sixteen-subsystem implementation. See [implementation status](implementation-status.md).

## 1. Bounded contexts, segments and planes

Three orthogonal dimensions are kept separate:

- **Physical segments:** space, user access, feeder ground infrastructure, terrestrial backhaul, operations.
- **Bounded contexts:** the sixteen [subsystems](system-overview.md) with specific authoritative state, behavior and APIs.
- **Planes:** user/data plane (packets and link queues), control plane (topology, scheduling, subscriber decisions), management plane (telemetry, configuration, NOC), science/experiment plane (orbits, virtual time, scenario/replay).

Languages are **local implementation choices**, not top-level boundaries. Rust is preferred for the deterministic simulation kernel, queues, routing and packet propagation. Python is preferred for orbit propagation, calibration and scientific/experiment workflows. TypeScript provides console, globe and replay UI. Existing C/C++ engines remain external tools.

## 2. System context and topology

```text
                      [NOC / Mission Control / Analytics]
                                    |
       [Ephemeris] -> [Contact & Capacity Models] <- [Weather / Spectrum]
            |                       |                        |
     [Spacecraft State]        [Network Controller]      [Policy / Trust]
            |                 /           |                  |
   +------------------------ Space Segment -----------------------+
   |  Satellite A == optical ISL == Satellite B == Satellite C   |
   |  User RF antenna + payload + queues + forwarding + feeder   |
   +--------^--------------------------------------------v--------+
            | user uplink                           feeder downlink
     [Terminal / CPE]                             [Ground Station]
            |                                          |
       [Customer LAN]                        [Terrestrial Backhaul]
                                                       |
                              [Subscriber Core] -- [PoP / Internet Edge]
                                                       |
                                                 [Internet Sink]
   --------------------- shared runtime ------------------------------
   [Virtual Clock] [Event Queue] [Scenario] [Faults] [Metrics/Traces]
   [ns-3/SNS3 adapters] [Linux netns/tc] [RF/SDR adapters] [Web UI]
```

A ground station is **not** the same object as an internet PoP. A packet may traverse a station-to-PoP ground link. A subscriber service gateway is another distinct function. The topology graph must represent all three separately.

## 3. System execution architecture

### Embedded deterministic mode (default / first milestone)

- Single Rust runner owns the monotonic event queue, stable ordering, seeded PRNG streams and event journal.
- Rust libraries implement space forwarding, link/queue state, routing and packet scheduling behind explicit domain interfaces.
- Python orbital engine generates deterministic versioned ephemeris/contact artifacts **before** a run; runner loads those artifacts. Online Python callbacks are not on the per-packet hot path.
- Scenario compiler validates referential integrity, geometry, time windows, unique IDs, units and domain profile choices. A compiled immutable scenario snapshot becomes the simulation input.
- Results are appended to an event/packet trace and exported as JSONL first, Parquet/Arrow when benchmarks justify them.
- Web application consumes snapshots/aggregated timeline over an API; rendering is not authoritative.

### Coupled / multi-engine mode (later)

Use an adapter boundary with `PrepareScenario`, `StartRun`, `AdvanceToTime`, `ApplyLinkState`, `StreamTrace`, `StopRun`; external engines have a capability manifest specifying granularity, supported standards, time control, packet format, required privileges and licensing. One engine owns an event at each layer—**do not simulate the same packet PHY loss twice**.

### Real-IP emulation mode (later)

An isolated Linux worker owns namespaces, veth/TAP and `tc netem`, changes impairment parameters from simulated link snapshots at safe synchronization boundaries, attaches FRRouting/traffic generators as needed and captures PCAP. Emulation follows **wall clock**, not accelerated virtual time; report timing skew.

## 4. Internal component and state design

Subdomain libraries expose typed commands, domain events, queries and snapshots. An authoritative state transition is performed in one owner; subscribers receive event copies or versioned snapshots. Examples:

- SYS-01 owns ephemeris and geometric contact; SYS-07 owns route decisions; SYS-03 owns actual ISL acquisition/queue state.
- SYS-04 owns terminal hardware/modem state; SYS-05 owns beam/admission/scheduling decisions; SYS-08 owns subscriber authorization and session state.
- SYS-06 owns feeder radio gateways; SYS-09 owns IP egress and PoPs; SYS-07 chooses among their advertised feasible capacities.
- SYS-10 issues commands; SYS-11 authorizes/audits; the target subsystem performs state mutation.

Every entity uses stable IDs and a revision. Events carry the run/scenario identifier, source, event/causation IDs, simulation timestamp, stable sequence and schema version. Quantities use SI units with explicit reference frames.

See [contracts](design/contracts.md) for the message shapes and [data-plane](design/data-plane.md) for the packet/control sequences.

## 5. Target monorepo organization

```text
ntarlink/
  systems/
    orbital/ spacecraft/ space-network/ terminal/ access-network/
    ground-segment/ network-control/ subscriber-core/
    terrestrial-network/ operations/ security/ environment/
    service-assurance/
  platform/
    simulation/ contracts/ emulation/ observability/
  integrations/
    ns3/ sns3/ opensand/ openairinterface/ gnuradio/ orbital/
  applications/
    cli/ console/ api/
  scenarios/
    minimal/ handover/ isl-failure/ gateway-failure/ load/
  experiments/
    baselines/ reports/
  docs/
    subsystems/ design/
  .github/workflows/
  Cargo.toml
```

At migration time, use a Rust workspace at repository root, Python `pyproject.toml` within orbital/experiments or adapters, TypeScript packages under applications. Shared contracts live in platform/contracts (schemas first, generated language bindings later); application code cannot become the owner of simulation domain state. Do **not** create empty modules solely to mirror this diagram.

## 6. Shared decisions and anti-patterns

| Decision | Chosen initial approach | Reason / rejected shortcut |
|---|---|---|
| Source layout | monorepo by bounded context | splitting immediately increases versioning/integration overhead |
| Run model | single deterministic scheduler | per-packet microservice RPC breaks time determinism and scale |
| Orbit | precomputed Python ephemeris/contact snapshots | avoids Python in hot path; online updates are a future adapter |
| Physical layer | analytic/link-budget model with labeled assumptions | do not fake validated proprietary MAC/PHY |
| Network | Rust packet/queue pipeline; ns-3/SNS3 optional comparison | reuse mature tools without duplicating clocks |
| Web | read-only experiment state + commands via API | browser is not the authoritative simulator |
| Time | int64 nanoseconds for virtual event ordering; UTC/TAI conversion at geometry boundary | never mix wall-clock and virtual clock |
| Datastore | JSON/JSONL baseline; columnar artifacts only as justified | do not prematurely distribute high-rate events to a DB |
| Egress | feeder gateway and terrestrial edge modeled separately | avoids unrealistic instantaneous ground connectivity |
| Core | generic subscriber core; optional 3GPP adapter | actual Starlink broadband core internals are not public |
| Scaling | batch contacts, event coalescing, sparsity and profiling first | do not distribute by default |

## 7. Security and reliability of the simulation platform

Treat scenarios and imported orbital data as untrusted inputs: validate schemas, sizes and ranges; bound maximum satellites, terminals, events and queue memory; never execute shell commands from scenario text. External runtimes run with least privileges; privileged netns operations stay in dedicated workers. Artifact/checkpoint manifests must record model versions, input hashes, seed, runtime version and git SHA. Property tests verify monotonic event time, conservation/accounting of packets, no impossible links and referential integrity.

## 8. Architecture completion criteria

Architecture is reviewable when every subsystem specification identifies entities, algorithms, lifecycle, data/control interfaces, metrics, failure/recovery and upstream candidates; contracts and sample artifacts validate; E2E scenarios bind at least two contexts. This document is not evidence of implementation. Implementation follows the phase gates in [roadmap](roadmap.md).