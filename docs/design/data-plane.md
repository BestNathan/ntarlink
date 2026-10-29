# End-to-end data/control flows and link engineering

**Status:** the Rust L1 packet runner implements five link classes and deterministic packet/queue timing. Python can now precompute TLE/SGP4-derived **geometric** visibility windows and midpoint delay for the Rust runtime, triggering contact open/close events, link availability and route changes. This does not yet implement RF/laser acquisition, MAC scheduling, bent-pipe relay or a physical modem. See [implementation status](../implementation-status.md).

## 1. Four classes of links

| Link | Endpoints | Responsible owners | Modeling requirements |
|---|---|---|---|
| User RF | terminal ↔ serving satellite spot beam | SYS-04/05 | antenna gain/steering, link budget, Doppler, SINR, MAC scheduling, uplink/downlink asymmetry |
| Feeder RF/optical | satellite ↔ ground station | SYS-06 + spacecraft | elevation/range, weather, antenna tracking, capacity and station contention |
| Optical ISL | satellite ↔ satellite | SYS-03 | Earth occlusion, laser pointing/acquisition/tracking, relative motion, optical budget, capacity, queued forwarding |
| Terrestrial | ground station ↔ PoP ↔ destination | SYS-09 | fiber/backhaul latency, packet queues, IP routing, congestion, failures, return path |

Feasible geometry is only the **candidate contact**. Radio/optical acquisition, resource reservation, congestion and policy must also succeed before setting a link to UP.

## 2. Cold-start and session establishment

```text
Terminal                Access/Beam          Service Core       Network Control
   | boot, location          |                     |                   |
   |---- discover ---------->|                     |                   |
   |<--- visible candidates -|                     |                   |
   |---- acquire/attach ---->|                     |                   |
   |                         |---- authorize ------>|                   |
   |                         |<--- policy/session --|                   |
   |<--- attach/session -----|                     |                   |
   |                         |---- demand/capacity -------------------->|
   |                         |<--- forwarding/gateway plan ------------|
   |---- uplink data ------->|      link graph + subscriber policy      |
```

Terminal state machine: `OFF → BOOTING → SCANNING → ACQUIRING → ATTACHED → DEGRADED/HANDOVER → RECOVERING/SCANNING`. Network-side Access state is separate: `CANDIDATE → ADMITTED → ALLOCATED → ACTIVE → SUSPENDED/RELEASED`. Authorization never implies link acquisition by itself.

## 3. Packet journey (first vertical slice)

```text
Flow source
  -> LAN/CPE (encapsulation + terminal TX queue)
  -> user RF (serialization + propagation + model loss)
  -> Satellite A (payload mode + packet queue + forwarding lookup)
  -> optical ISL A→B (acquisition required; propagation + queue)
  -> Satellite B (forwarding)
  -> feeder downlink B→Ground Station
  -> ground station backhaul to PoP
  -> subscriber policy / egress routing
  -> destination sink (delivery recorded)
```

For every directed hop, calculate:

- `propagation_ns = round(distance_m / c × 1e9)` for vacuum geometric paths (use configurable effective medium for terrestrial segments).
- `serialization_ns = ceil(packet_bits / effective_capacity_bps × 1e9)`; effective capacity is residual after framing/coding/resource allocation.
- `queue_delay` is determined by bytes ahead, server availability and discipline (FIFO initially; QoS later).
- `processing_ns` comes from declared node processing model; no undocumented zero-cost forwarding.
- `link_loss` results from a tagged stochastic/error model, not a fixed arbitrary number unless explicitly selected by scenario.
- `end_to_end_delay = Σ(propagation + serialization + queueing + processing + acquisition interruption + terrestrial delays)` for the delivered packet's actual path.

Capacity, delay and loss are **directional**. Store queue-drop reason, deadline and packet lifetime; account separately for link outage, no route, policy deny, buffer overflow and channel error.

## 4. Bent-pipe vs regenerative payloads

- **Bent-pipe:** satellite frequency-translates/amplifies and relays the feeder/user signal; the satellite does **not** magically act as an IP router. Packet routing and user-plane termination are anchored in a compatible ground-side model.
- **Regenerative:** satellite terminates some PHY/MAC/protocol processing and can make onboard forwarding decisions; the chosen layer of regeneration is explicit.
- **Space IP routing over ISLs** requires a consistent regenerative/packet-forwarding model. A scenario that selects pure transparent bent-pipe cannot simultaneously claim arbitrary onboard IP hop-by-hop routing.

Scenario validation must reject incompatible payload and ISL forwarding combinations.

## 5. Handover under motion

1. SYS-01 predicts candidate visibility windows, SYS-02 advertises viable payload/pointing state, SYS-12 contributes weather/policy constraints.
2. SYS-05 evaluates SINR, load, steering cost and hysteresis/time-to-trigger; SYS-07 prepares route/gateway changes.
3. Terminal and access perform coordinated break-before-make or modeled make-before-break; resulting attachment and session events have timestamps.
4. SYS-08 preserves the logical subscriber session/IP anchor if policy permits; SYS-09 updates egress if needed.
5. SYS-13 computes interruption time, TCP/application impact and dropped packet counts.

Avoid instantaneous handover or using RF visibility alone as an attach criterion; measured overhead and control latency are parameters.

## 6. ISL or gateway failure

```text
Link monitoring -> LinkLost -> topology revision -> route computation
              -> propagation/installation delay -> new forwarding state
              -> queued packet retries or explicit drops -> service recovery
```

Route planner must consider contact end time, capacity, propagation delay, queuing and convergence; for time-varying topology a static shortest path is only an initial baseline. Failures are driven by virtual-time events and must be replayable.

## 7. Link budgets and validated models

The simplified free-space path loss model for isotropic RF links is `FSPL_dB = 20log10(4πd/λ)`. Received power is `Pr_dBW = Pt_dBW + Gt_dBi + Gr_dBi − FSPL_dB − L_other_dB`. Thermal noise `N_dBW = 10log10(kTB)` with noise figure and system temperature treatment documented; derive `C/N` or `Eb/N0` from an explicit coding and occupied-bandwidth assumption. Do **not** equate Shannon capacity with a real PHY data rate. Doppler is approximated by radial relative velocity `f_d ≈ −(v_r/c)f_c` with sign convention explicit.

For optical ISLs, adopt a separate link budget (aperture, divergence, pointing loss, detector noise, background, weather for relevant paths); RF FSPL/MCS functions are not a substitute. For feeder Ka-band, rain attenuation and fade margin must reference documented environment/scenario inputs.

## 8. Acceptance traces

Deliver a record per hop: timestamp, packet and flow IDs, from/to, link ID/type, geometry revision, queue occupancy, capacity, propagation, serialization, processing, channel outcome, route revision, drop reason and final delivery result. Test asymmetric uplink/downlink, queue saturation, temporary loss, route updates and session continuity. See [system scenarios](../system-scenarios.md).