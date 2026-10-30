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

## Faults, validation, and milestones

Faults: power derating, single optical head failure, antenna mispointing, thermal throttling, unavailable compute, safe mode and satellite retirement. Each invalidates advertised link capability and leads to SYS-03/05/06 link changes, SYS-07 route reoptimization and SYS-10 alarms.

**Acceptance:** a disabled optical head prevents ISL acquisition even with clear LOS; power limits reduce beam capacity; safe mode drains/invalidates traffic according to payload model; satellite retirement removes node and resources; same fault schedule yields identical trace. Capacity ledger remains nonnegative; no double assignment to exclusive hardware.

**Build order:** inventory/capability model → payload mode+resource admission → health/fault events → attitude and power/thermal approximations → cross-system capacity/fault tests.