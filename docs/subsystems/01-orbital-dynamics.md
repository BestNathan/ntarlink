# SYS-01 — Orbital Dynamics, Ephemerides and Contact Geometry

**Mission:** be the single authoritative provider of physical positions, reference-frame transformations and geometric contact opportunities. **Not responsible** for deciding which network link is allocated, acquired or routed.

## Component architecture

| Component | Responsibility | Input → output |
|---|---|---|
| Orbit catalog | ingest orbital shells, satellites, TLE/OMM/trajectory definitions | source version + epoch → validated orbit records |
| Propagator | position/velocity over time | orbit + time scale → position/velocity in declared frame |
| Coordinate/time service | TEME/ITRF/geodetic, UTC/TAI and Earth orientation | frame-tagged state → frame-tagged transformed state |
| Geometry engine | Earth occultation, elevation, range and relative velocity | spacecraft + terminals/sites → geometric relations |
| Contact predictor | contiguous visibility windows, event boundary refinement | geometry + constraints → contact intervals |
| Ephemeris artifact exporter | batched snapshots and interpolation metadata | time grid → versioned columnar/JSON artifact |

## Technical design

**Implemented P1 slice:** offline Python batch generator `ntarlink_orbital.contacts` integrates pinned Skyfield and python-sgp4 for historical TLE propagation, explicit Skyfield TEME→ITRS transformation, WGS84 elevation and ellipsoid segment occultation, and a geometric contact schedule with optional bisection-refined boundaries. See [historical ISS example](../../scenarios/ORBITAL-README.md). Full error-calibrated ephemeris interpolation remains future work. Parse CelesTrak-style TLE or CCSDS OMM with metadata and source checksums. An SGP4 position is in **TEME**, *not* automatically a conventional ECEF/ITRF vector. Explicit time/frame conversion is required before computing ground-site elevation. A high-fidelity alternative is [Orekit](https://gitlab.orekit.org/orekit/orekit) in an isolated JVM worker.

Propagate position `r(t)` and velocity `v(t)`; compute geometric range `d = ||r_sat - r_site||`, line-of-sight elevation above a WGS84 local horizon and radial relative velocity `v_r = ((v_sat-v_site)·(r_sat-r_site))/d`. The simplest Earth occultation test evaluates the closest point of the satellite-to-satellite segment to the Earth; a WGS84 ellipsoid-aware alternative is needed for precision. Include occultation margin and configurable minimum ground elevation.

Current contact computation samples known scenario edges at a configurable step and emits half-open `[start_ns, end_ns)` windows. For detected rising/setting transitions, optional bisection refines the bracket to a declared nanosecond tolerance and rounds the interval inward to geometric visibility; the scan can still miss short contacts with multiple transitions per step. Propagation delay is fixed at the window midpoint. Future work: constellation-scale candidate filtering, independent frame reference validation and continuous delay updates. Store step size, refinement tolerance, interpolation error and propagation model revision. Avoid O(N²) satellite-pair scans at every simulation tick.

The orbital model produces **geometric potential**, never a claim that a beam, optical terminal or feeder modem has acquired the target. SYS-03/05/06 convert opportunities to usable link state.

## Entities, interfaces and contracts

Authoritative: `OrbitDefinition` (source/epoch, TLE/OMM), `EphemerisSegment`, `PositionSnapshot` (frame, m, m/s), `ContactWindow` (link category, endpoints, interval, elevation/range constraints). Subscriptions: spacecraft catalog, environment masks and scenario time. Consumers: SYS-03/05/06/07/12/16.

Commands/queries: `ImportOrbitSet`, `PropagateBatch(satellite_ids, times, frame)`, `PredictContacts(endpoints, time_range, constraints)`, `GetEphemerisManifest`. Events: `OrbitSetUpdated`, `EphemerisGenerated`, `VisibilityWindowOpened/Closed`. Never publish a location without epoch and reference frame.

## Runtime and dependencies

**Preferred:** Python-SGP4 (MIT) for fast batch propagation; a tested astronomy/time library for TEME/ITRF and Earth orientation. **Optional:** Orekit (Apache-2.0) for maneuvers/attitude/event detection. Store precomputed artifacts for Rust consumption; avoid calling Python once per data packet. Pin TLE source snapshots and Earth orientation data to make runs reproducible. See [dependency decisions](../dependencies.md).

## Failure and performance behavior

Stale/missing TLE, propagation error codes, invalid epoch, coordinate singularity near poles, wrong time scale, leap-second changes and malformed site coordinates must surface explicit failures. A missing ephemeris interval makes affected contacts *unknown*, not automatically available.

**Acceptance:** SGP4 reference vectors with tolerances; TEME↔ITRF↔geodetic regression; no satellite–ground contact below elevation threshold; ISL never crosses Earth; reproducible rising/setting times; batch performance at 1K/10K satellites and contact window count scaling. Logs record source, frame, epoch, propagation uncertainty and artifact hash.

**Build order:** catalog+SGP4 → frame/time golden tests → ground/ISL contact engine → cached artifact contracts → optional Orekit comparison.