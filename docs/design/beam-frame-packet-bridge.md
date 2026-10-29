# SYS-05 shared beam-frame grants in packet transport

**Fidelity:** optional deterministic L1 packet-discrete-event MAC model, not an RF
waveform, actual terminal authentication, a 3GPP/DVB modem, or proprietary Starlink
scheduling. This milestone connects the existing SYS-05 weighted deficit
round-robin allocator to the SYS-14 packet event loop for **terminal-to-satellite
user RF uplinks only**.

## Scenario contract and state ownership

A scenario may contain `access_beams[]`, each with a unique beam ID, aggregate
`capacity_bps`, `frame_duration_ns`, `quantum_bytes`,
`packet_overhead_bytes`, shared `queue_limit_bytes` and
`bearers[]` of `{terminal_id, link_id, weight}`. Each bearer identifies a
unique, directed `user_rf` link from a terminal to the **same satellite**.
A link belongs to at most one beam. Invalid directions, duplicate bearers,
nonpositive weights, impossible byte budgets and cross-satellite resources fail
scenario validation. Unlisted links continue using the existing per-link
packet-serialization model; old scenarios remain backward compatible.

SYS-05 owns queued beam packets and grant decisions. The packet runtime owns
route selection, link/contact availability, packet state and hop traces. A
packet arriving at a configured bearer is enqueued into the shared beam, not
immediately serialized at the link's independent `capacity_bps`.
The beam scheduler grants frames in deterministic terminal-ID order with a
rotating start position and weighted deficit credit. Packet overhead consumes
shared frame capacity but does not alter the IP payload size.

## Virtual-time event semantics

- Packet arrival/injection occurs at event priority 20. Beam frame grants
  occur at priority 30, so a packet arriving exactly at a frame boundary may
  enter that frame.
- Frames are aligned to integer multiples of `frame_duration_ns` relative to
  scenario time zero; idle beams do not generate periodic events.
- Grants serialize sequentially within the frame. Cumulative **wire bytes**
  determine start/completion offsets by integer ceiling division, so all
  granted transmissions fit within the shared byte budget without
  per-packet rounding drift.
- A packet's `enqueued_ns` is its actual arrival at the bearer; queue delay
  includes waiting for a frame and other grants. The existing sampled
  geometric propagation delay is evaluated at transmission completion.
- Contact closure or administrative link-down cancels **ungranted** packets
  on the affected bearer with `beam_link_lost`. Other bearers sharing the
  beam remain eligible. Already-issued transmissions finish under the
  documented P0 completion-after-departure policy.
- Queue exhaustion drops new packets with `beam_queue_overflow`. Packets
  that cannot fit into one frame use `beam_packet_unserviceable`. Pending
  packets at simulation end are accounted as `simulation_horizon` drops.
  This model does not retransmit or retain ungranted packets across outages.

The configured link `capacity_bps` is still used for legacy routing cost;
shared beam congestion is **not** incorporated into route planning.
This model does not derive frame capacity from measured SINR/MODCOD, enforce
service-core authorization, allocate downlink beams, or simulate PHY errors.

## Reproducible acceptance

```sh
cargo test --workspace
cargo run -p ntarlink-cli -- simulate scenarios/beam-uplink-contention.json --json > /tmp/beam-report.json
```

The JSON report exposes `beam_grants[]` with beam ID, frame start, bearer,
packet ID, wire-byte allocation and transmit start/completion timestamps;
legacy scenarios omit this empty field. Grant evidence is distinct from
PHY acquisition or authenticated service.

The fixture has two terminals sharing one 1,220-wire-byte frame every 10 ms
at 976 kbit/s. Eight 1,200-byte packets with 20-byte overhead compete for
the beam. Expected: 8 deliveries, 0 drops, one uplink grant per frame, with
start times 0, 10, ..., 70 ms and reproducible JSON. Unit tests also cover
contact loss of one bearer, independent service of the other bearer, shared
queue overflow and validation errors. Work scales with the number of
active terminals and granted packets per scheduled frame; queue memory is
bounded by the configured aggregate wire-byte limit.

## Follow-up

Add explicit radio bearer admission and link acquisition, contact-aware
beam selection, separate uplink/downlink resources, grant event observability,
and calibrated SINR/MODCOD-driven capacity. Do not infer physical throughput
or Starlink compatibility from this L1 fixture.
