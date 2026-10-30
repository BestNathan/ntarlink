# SYS-02 — Spacecraft, Constellation and Onboard Resource Models

**Mission:** represent the physical satellite fleet, spacecraft operating modes and constraints that make communication hardware available. **Not responsible** for computing orbital positions (SYS-01), choosing routes (SYS-07) or delivering packet frames (SYS-03/05).

## Components and owned state

| Module | State / model | Consumers |
|---|---|---|
| Fleet catalog | satellite identity, shell, plane, orbital slot, deployment/retirement state | orbit, operations, network planning |
| Spacecraft capability | payload inventory, optical terminals, beam count, feeder radios, packet processor | ISL/access/feeder |
| Attitude and pointing | attitude mode, steering limits, slew time and pointing error | optical PAT, ground antenna alignment |
| Power and thermal | solar/eclipse power availability, energy/storage proxy, temperature and duty-cycle budget | link resource admission |
| Onboard processing | payload mode, CPU/queue capacity, forwarding/processing availability | space forwarding, metrics |
| Telemetry and health | fault flags, subsystem health, maintenance modes, command lifecycle | NOC, network control |

## Technical model

Define a stable `SatelliteId`, spacecraft state `PLANNED → DEPLOYED → COMMISSIONING → ACTIVE → DEGRADED → SAFE_MODE → RETIRED` and independent payload/optical-terminal health states. A modeled payload may be:

- **Transparent / bent-pipe:** frequency translation/amplification and relay without arbitrary IP forwarding on board.
- **Regenerative (PHY/MAC or IP):** specified regeneration layer, processing cost and packet forwarding capabilities. Only regenerative packet-capable satellites may host generic IP routing nodes.
- **Hybrid:** multiple onboard functions modeled separately, never implicitly combined.

Resource admission checks share a single spacecraft capacity ledger: active beam count, optical head count, feeder antenna occupancy, available watts, thermal budget and pointing constraints. A link can be geometrically feasible and still unavailable because the spacecraft has insufficient resources or is in safe mode. Represent approximate solar/eclipsed power and thermal penalties as explicit parameterized models rather than unsubstantiated production figures.

Spacecraft pointing model: target vector from propagated position; capability envelope from mechanical/electronic steering constraints; acquisition dwell/slew time; reject conflicting optical or feeder assignments to a single exclusive terminal. SYS-03 owns optical PAT acquisition state; SYS-02 only owns whether a terminal is physically present/available.

## Interfaces

Owned: `Constellation`, `OrbitalShell`, `Satellite`, `Payload`, `OpticalHead`, `AttitudeState`, `PowerBudget`, `HealthStatus`. Inputs: orbit definitions/ephemerides (SYS-01), operator commands (SYS-10), environment/eclipses (SYS-12), resource requests (SYS-03/05/06). Outputs: revisioned `SpacecraftCapabilitySnapshot` and events `SatelliteCommissioned`, `PayloadEnabled/Disabled`, `PowerDerated`, `AttitudeModeChanged`, `SpacecraftAnomaly`.

Commands: `SetPayloadMode`, `EnterSafeMode`, `AllocatePayloadResource`, `ReleasePayloadResource`. Command authorization belongs to SYS-11 and operations approval belongs to SYS-10; state mutation belongs here. All reservations have expiry and resource IDs; reject overbooking deterministically.

## Technology and external dependencies

Implement domain/resource ledger in Rust as pure state transitions and event outputs; Python/Orekit may provide sophisticated attitude/eclipse models as an optional artifact producer. cFS/Yamcs are **reference candidates for flight/mission operations experiments**, not dependencies or proof of flight-qualified ntarlink software. No spacecraft-specific proprietary telemetry assumed.

## Implemented first availability slice

`systems/spacecraft/rust` implements an indexed, deterministic optical-head
outage ledger. The optional scenario `optical_head_outages[]` declares
maintenance/failure intervals for heads owned by `optical_pat[]` endpoints.
Both heads must be available throughout PAT acquisition and tracking.
A head outage interrupts tracking and releases reservations; when it recovers,
SYS-03 performs a fresh acquisition. The packet runtime and delayed routing
control consume the resulting acquired contact windows. This is a modeled
availability contract, not a physical attitude/pointing or power budget.

## Implemented static pointing-envelope slice

The optional `optical_pointing[]` scenario contract provides operator-supplied,
constant per-link off-boresight angles and per-head steering limits in degrees.
When enabled, every PAT link must have a pointing entry; head limits must agree
across links sharing a head. SYS-02 rejects a contact if either endpoint's
assumed angle exceeds its inclusive limit, before SYS-03 reserves heads or
starts acquisition. The packet report records
`pointing_outside_envelope` rejection events. The deterministic
`scenarios/optical-pointing-envelope.json` fixture verifies that geometric
visibility alone does not permit forwarding. This is a **static assumed-angle
feasibility gate**, not an ephemeris-derived target vector, spacecraft attitude
solution, time-varying steering trajectory or measured pointing error.

## Implemented ITRS pointing snapshot slice

The optional `optical_geometry[]` contract supplies one snapshot for every
configured PAT contact, keyed by directed link ID and contact start time
(`at_ns`, scenario-relative virtual nanoseconds). Both satellite positions
are Cartesian ITRS meters; both optical-head boresight directions are
dimensionless vectors expressed in the same ITRS axes. The gate computes
reciprocal line-of-sight vectors and checks both off-boresight angles against
inclusive per-head limits in degrees. Nonfinite, zero, coincident, wrong-frame,
missing/duplicate or inconsistent shared-head snapshots are rejected.
`optical_geometry` and static `optical_pointing` are mutually exclusive.

The snapshot is **held fixed for the entire geometric contact**. This does
not interpolate orbit positions, transform spacecraft body-frame attitude,
model tracking error or certify line-of-sight pointing throughout a window.
A future SYS-01 adapter must populate these vectors from authenticated,
time-aligned ephemerides and an explicit attitude/installation model.
The synthetic `scenarios/optical-itrs-pointing.json` fixture verifies
contact-by-contact PAT admission, rejection and packet suppression.

## Faults, validation, and milestones

Faults: power derating, single optical head failure, antenna mispointing, thermal throttling, unavailable compute, safe mode and satellite retirement. Each invalidates advertised link capability and leads to SYS-03/05/06 link changes, SYS-07 route reoptimization and SYS-10 alarms.

**Acceptance:** a disabled optical head prevents ISL acquisition even with clear LOS; power limits reduce beam capacity; safe mode drains/invalidates traffic according to payload model; satellite retirement removes node and resources; same fault schedule yields identical trace. Capacity ledger remains nonnegative; no double assignment to exclusive hardware.

**Build order:** inventory/capability model → payload mode+resource admission → health/fault events → attitude and power/thermal approximations → cross-system capacity/fault tests.