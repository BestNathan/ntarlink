# Interface, schema, time and unit contracts

**Status:** proposed normative cross-subsystem contract; exact wire format and versioning will be stabilized before first adapter integration.

## 1. Principles

- Each subsystem owns its mutable state. Other systems issue commands and consume read-only snapshots and domain events.
- A command is an intent requiring validation and an acknowledgment/rejection; an event is a fact **after** commit.
- A message envelope is consistent across Rust, Python, TypeScript and external C/C++ adapters, but the transport is deployment-specific.
- The event engine must not perform JSON serialization per packet on its critical path. In-process typed structures are canonical; serialization occurs at adapter, log or API boundaries.
- IDs identify a logical run, object and ordered revision. Timestamp, frame and units are not implicit.

## 2. Common envelope

```json
{
  "schema_version": "1.0.0",
  "run_id": "run-demo-001",
  "scenario_id": "scenario-leo-01",
  "event_id": "event-000031",
  "causation_id": "event-000029",
  "trace_id": "packet-42",
  "source": "space-network",
  "entity_id": "isl:sat-a:sat-b",
  "entity_revision": 17,
  "sim_time_ns": 12500000000,
  "sequence": 31,
  "type": "IslLinkEstablished",
  "payload": {
    "link_id": "isl:sat-a:sat-b",
    "capacity_bps": 2000000000,
    "propagation_delay_ns": 3100000
  }
}
```

Clock rules: `sim_time_ns` is a nonnegative integer relative to run epoch, not Unix time; `epoch_utc` and a declared time scale live in scenario metadata. Orbital propagators return frame-tagged TEME/ITRF positions at a real epoch. Conversion between UTC, TAI and simulation offset uses a tested explicit adapter; do not treat TEME as a generic ECI frame.

Recommended message taxonomy: `*Requested`/`*Accepted`/`*Rejected` commands, `*Changed`/`*Established`/`*Lost` events, `*Snapshot` query results. A `command_id` and `expected_revision` support idempotency/concurrency control.

## 2.1 Implemented scenario v1 contact extension

The current Rust `platform/contracts` crate and `platform/contracts/schemas/scenario.schema.json` accept optional `contact_mode: "static" | "scheduled"` and `contacts`:

```json
{
  "contact_mode": "scheduled",
  "contacts": [
    {
      "link_id": "user-iss",
      "start_ns": 340000000000,
      "end_ns": 500000000000,
      "propagation_delay_ns": 2100000
    }
  ]
}
```

In `static` mode, orbital links without schedules retain their scenario `up` state; in `scheduled` mode, user RF, feeder and optical ISL links are **unavailable unless a window is active**. Terrestrial links remain static unless faults explicitly change them. `up` represents administrative state and cannot override missing geometric visibility. Windows are nonoverlapping and half-open. Their midpoint propagation delay is fixed per interval in the current L1 runtime. These optional fields extend scenario v1; the broader system envelope remains a forward-looking interface proposal.

## 3. Canonical identifiers and primitive types

| Type | Definition | Invariants |
|---|---|---|
| `SatelliteId` | opaque stable string within a scenario | unique and not reused after retirement |
| `TerminalId` | stable terminal identifier | never assume a subscriber equals a terminal |
| `BeamId` | satellite payload + beam identifier | beam validity tied to spacecraft and time |
| `LinkId` | endpoints + technology + unique instance | direction explicit; an optical ISL is not a feeder link |
| `GroundSiteId` | feeder location and radio infrastructure | separate from PoP and edge gateway |
| `PopId` | terrestrial ingress/egress location | may be co-located with ground site |
| `SessionId` | service-core session identity | lifecycle independent of radio bearer |
| `PacketId` | per-run unique packet trace key | parent/fragment correlation supported |
| `Position` | x/y/z in meters, frame, epoch | valid frame enum, finite values |
| `Velocity` | vx/vy/vz in m/s, same frame/time | explicit frame |
| `Capacity` | bps | finite, nonnegative |
| `Frequency` | Hz | finite, positive |
| `Power` | dBm or W explicitly tagged | do not mix dB and linear units |
| `Delay` | integer ns | finite, nonnegative |

Physical location APIs expose WGS84 longitude/latitude in degrees, geodetic altitude in meters; default geometry should consider WGS84 ellipsoid rather than a unit sphere. If a spherical Earth is used in a cheap model, label it. Link budget fields use dB/dBi/dBW and documented conversions.

## 4. Core shared domain records

```text
Scenario {
  version, epoch_utc, duration_ns, seed,
  fidelity_profile, orbital_model, earth_model,
  satellites[], terminals[], ground_sites[], pops[],
  subscriber_profiles[], links[], traffic_flows[], fault_injections[]
}
ContactWindow {
  link_id, start_ns, end_ns, geometry_source,
  elevation_min_deg?, range_min_m?, range_max_m?, confidence
}
LinkState {
  id, kind: USER_RF|FEEDER_RF|OPTICAL_ISL|TERRESTRIAL,
  from, to, state: DOWN|ACQUIRING|UP|DEGRADED,
  capacity_bps, propagation_delay_ns, ber?, loss_probability?,
  queue_limit_bytes, epoch_revision, valid_until_ns
}
RouteEntry {
  owner_node_id, destination_prefix_or_endpoint, next_hop,
  link_id, installed_revision, valid_from_ns, expiry_ns
}
ServiceSession {
  session_id, subscriber_id, terminal_id, assigned_addresses[],
  state, qos_policy_id, anchor_pop_id?, revision
}
PacketTrace {
  packet_id, flow_id, size_bytes, source, destination, direction,
  created_ns, hops[], final_state: DELIVERED|DROPPED|EXPIRED
}
```

The fields above are design vocabulary, **not already implemented schema files**. First implementation needs JSON Schema definitions for scenario and exported events, plus round-trip fixtures in all three languages.

## 5. Event domains and source-of-truth

- Orbit owns `EphemerisUpdated`, `VisibilityChanged`; cannot directly install routes.
- Spacecraft owns `PayloadStateChanged`; a change may invalidate link feasibility.
- Access owns `BeamResourceUpdated`, `TerminalAttached`, `HandoverCompleted`; terminal independently confirms modem transitions.
- ISL and ground domains own `LinkAcquired`, `LinkLost`, `FeederCapacityChanged`.
- Control owns `RoutePlanPublished`, `ForwardingRuleCommitted` and `GatewaySelected`.
- Service core owns `SubscriberAuthorized`, `SessionCreated`, `AddressAssigned`.
- Data plane owns `PacketEnqueued`, `PacketForwarded`, `PacketDropped`; observations are not route decisions.
- Operations owns `OperatorActionApproved`; security owns `AuthorizationDenied` and audit events.

## 6. API surfaces and errors

**Internal synchronous queries** may retrieve a consistent snapshot by revision for same-process modules. **External APIs** should expose: `GET /v1/runs/{id}`, `POST /v1/runs`, `POST /v1/runs/{id}/actions` (pause, fault injection, resume), `GET /v1/runs/{id}/artifacts`, `GET /v1/runs/{id}/traces` and a batched/streamed timeline endpoint. Endpoint names are provisional; operation-specific decisions remain in their domain owner.

Errors: `INVALID_ARGUMENT`, `OUT_OF_RANGE`, `NOT_VISIBLE`, `LINK_UNAVAILABLE`, `RESOURCE_EXHAUSTED`, `STALE_REVISION`, `POLICY_DENIED`, `TIME_SYNC_UNSUPPORTED` and `DEPENDENCY_FAILED`. No silent fallback of physics or service eligibility assumptions.

## 7. Contract tests and compatibility

- Encode/decode and validate golden fixtures across Rust serde, Python and TypeScript schema validators.
- Reject unknown major versions and unsupported required fields; tolerate well-defined optional additions under minor versions.
- Validate referential integrity, positive link capacity, nonoverlapping time windows, stable event order and conservation of packet accounting.
- Generate contract documentation and fixtures from schemas once interfaces stabilize; avoid independent hand-written structures diverging.
- External engine results carry adapter version, source revision and fidelity declaration.