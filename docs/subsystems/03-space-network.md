# SYS-03 — Optical Inter-Satellite Links and Onboard Packet Network

**Mission:** model usable satellite-to-satellite optical links, onboard forwarding, buffers and transmission. **Not responsible** for orbital propagation (SYS-01), globally optimal route selection (SYS-07) or terminal access (SYS-05).

## Internal components

1. **ISL contact candidate filter:** consume geometry/spacecraft capability; select line-of-sight endpoints and enforce optical-head count, orientation, max range and Earth occlusion.
2. **Pointing, acquisition and tracking (PAT):** explicit `UNAVAILABLE → CANDIDATE → SLEWING → ACQUIRING → TRACKING → DEGRADED → LOST` state machine. Acquisition delay and reacquisition are not zero.
3. **Optical link model:** range, aperture, beam divergence, transmitter optical power, pointing loss, detector sensitivity/background, error and effective bitrate. Do not substitute Ka-band RF MCS.
4. **Link capacity scheduler:** directional capacity and reservations, half-/full-duplex assumption, time-window expiry and resource conflicts.
5. **Satellite forwarding engine:** per-node FIB, next-hop validity, TTL/hop budget, packet processing delay, route revision and failure handling.
6. **Finite packet queues:** link-specific FIFO baseline, byte caps, optional priority/weighted fair queue, drop-tail and queue accounting.
7. **ISL telemetry:** acquired state, goodput, bit/packet loss, queue occupancy, reconfiguration and route lookup errors.

## Technical and algorithm details

Geometric feasibility requires an unoccluded satellite–satellite segment and suitable slew/pointing capability. Then PAT must complete and transceiver resources be reserved before `ISL_UP`. The controller may request a link, but SYS-03 owns the final acquired state and reports `ISL_DOWN` on loss of tracking. A contact expires at its planned boundary even if packets remain queued; define transmit-in-flight behavior.

For packet hop `A→B`, compute `t_tx = ceil(8·size_bytes / bitrate_bps × 1e9)`, geometric `t_prop = round(distance_m / c × 1e9)`, node processing and queue delay; channel loss follows a labeled deterministic/error model with a seeded RNG. Link state and packet arrival events must be ordered with a reproducible precedence rule. If a link changes mid-transmission, specify whether frames finish, are aborted, or are retransmitted by another layer.

A valid route entry is owned and installed through SYS-07, but the actual packet forwarding lookup, checksum/TTL handling and queue drop belong here. Intermediate satellite forwarding needs an explicit regenerative/packet-capable payload profile. No global shared routing table updated implicitly by geometry.

## Data model and contracts

Entities: `OpticalTerminalId`, `IslLinkId` (directed), `PatSession`, `TxReservation`, `PacketQueue`, `ForwardingTableRevision`. Input events: `ContactCandidateAvailable` (SYS-01), `PayloadResourceChanged` (SYS-02), `InstallForwardingRules` (SYS-07). Output events: `IslAcquisitionStarted`, `IslLinkEstablished`, `IslLinkLost`, `QueueOverflow`, `PacketForwarded`, `PacketDropped`.

Queries: `GetUsableLinks(at_time)`, `GetQueueMetrics(link_id)`, `GetForwardingState(node_id,revision)`. Commands: `AcquireIsl`, `ReleaseIsl`, `InstallRoutes`; reject requests beyond contact expiry, terminal availability or resource constraints. Use [canonical messages](../design/contracts.md).

## Dependencies and accuracy

**P1:** pure Rust modeled PAT/queue/forwarding under SYS-14 virtual clock; Python/SYS-01 provides batched geometry. **Later:** [ns-3](https://www.nsnam.org/) or [SNS3](https://github.com/sns3/sns3-satellite) packet/protocol comparison and research optical model evaluation. SNS3 ISL functionality is not equivalent to a verified commercial laser modem; validate scope before use. External stacks are separate adapters with versioned trace contracts and GPL review.

## Verification and adverse cases

- Two satellites clear LOS but PAT unacquired: **no data-plane route is usable**.
- Same link with greater range yields larger propagation delay; atmospheric RF equations never used for vacuum optical hop.
- Optical terminal conflict correctly rejects second simultaneous exclusive link.
- Failed link invalidates FIB next hop, triggers control update and accounts for packets queued/in flight.
- Full queue produces deterministic drop reason; byte-level accounting reconciles.
- Validate PHY/PAT approximations against public link-budget references, with assumption/tolerance table.

**Build order:** directed link abstraction → finite queues and trace → PAT state machine → control-plane FIB install → fault and contact-window expiration → optional protocol validation.