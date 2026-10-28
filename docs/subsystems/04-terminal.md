# SYS-04 — User Terminal, Antenna, Modem and Customer Premises Equipment

**Mission:** represent subscriber-side physical hardware and the local-network edge. SYS-04 owns device state and measurements; SYS-05 owns network-wide radio admission and beam scheduling; SYS-08 owns subscriber authorization and service sessions.

## Components

| Component | Technical responsibilities |
|---|---|
| Device registry | stable `TerminalId`, model capabilities, location/trajectory, firmware-profile metadata |
| Antenna controller | steering field of view, orientation, max slew rate, mechanical/electronic constraints, obstruction mask |
| Modem/radio state | scanning, candidate measurements, acquisition, synchronization, beam tracking and PHY budget |
| Link monitoring | received power, SNR/SINR estimate, Doppler correction estimate, candidate ranking and hysteresis inputs |
| Customer LAN/CPE | Ethernet/Wi-Fi abstraction, local interface/MAC/IP, DHCP/RA client behavior, NAT/DNS optional |
| Mobility engine | ground fixed/vehicle/marine/aviation position changes and service eligibility checks |
| Device telemetry | uptime, temperature/power proxy, link transitions, user-visible status and local fault codes |

## Device lifecycle

```text
OFF -> BOOTING -> SELF_TEST -> SCANNING -> ACQUIRING
                                   |          |
                                   |          v
                                   |      ATTACHED <-> DEGRADED
                                   |          |
                                   +<-- HANDOVER / REACQUIRE
                                   |
                               FAULTED
```

Terminal stores its selected beam/candidate, physical antenna pointing, modem state, active bearer reference and service/IP lease reference. Keep **radio attachment** distinct from **subscriber service activation**: an acquired beam is not proof of valid subscription. Terminal attempts attach, SYS-05 grants radio resources, SYS-08 authorizes the service; terminal acknowledges and then can transmit user traffic.

Model scan periodicity, acquisition timeout/retry, link measurement interval and motion speed. Obstruction and steering limits may prevent tracking even if geometric horizon checks succeed. For high-speed terminals include Doppler and beam dwell-time modeling. Do not import proprietary dish firmware, beam steering algorithms or cryptographic protocols as facts.

## Interfaces and packet boundary

Inputs: SYS-01 visible satellite candidates; SYS-05 access commands/measurements; SYS-08 session/IP policy; SYS-12 location/weather/regional eligibility. Commands: `PowerOn`, `SetTrajectory`, `StartScan`, `RequestAttach`, `ReleaseBearer`, `InjectDeviceFault`. Outputs: `TerminalBooted`, `CandidateMeasured`, `AcquisitionAttempted`, `TerminalModemAttached`, `TerminalModemDetached`, `TerminalDegraded`; snapshots carry observation timestamp and measurement units.

LAN→satellite: CPE packet (or simulated packet) enters terminal upstream queue, passes service/attachment state check, then SYS-05 schedules transmission. Downlink is delivered to LAN interface only if appropriate bearer/session state exists. Packet ownership remains in the data-plane model, not a synchronous UI callback.

## Dependencies and implementation plan

Rust device state machine and pure geometry/pointing policy; Python trajectories and obstruction maps as optional model inputs; Linux `tap`/`netns` adapter for real IP endpoint in SYS-15. Optional 3GPP NTN terminal is a **separate mode** integrated with OAI, never conflated with broadband modem.

Start with fixed terminal and simple phased-array field-of-view, then moving terminal, scan/handover measurements, device outages, LAN/IP emulator.

## Failure and acceptance tests

No satellite visible, obstructed beam, lost synchronization, overheating/reboot, invalid credentials, exhausted service allocation, terminal movement beyond antenna tracking. Fault reason must identify physical, radio, subscriber or policy layer, not a generic “offline”.

Tests: first attach latency; boot/reacquire timers; steering limit rejection; independent terminal and access state agreement; fixed-vs-moving handover; LAN packet blocked before session enable; service resumption without invented immediate recovery. Observe `time_to_first_service_ms`, `detach_count`, measured candidate margin and `user_outage_ms`.