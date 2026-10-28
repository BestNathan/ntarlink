# SYS-05 — User Access Radio Network: Beams, PHY/MAC and Mobility

**Mission:** convert viable terminal–satellite RF opportunities into allocated, time-bounded, capacity-limited bidirectional access service. This is the network side of radio access; the terminal owns device state (SYS-04), the subscriber core owns authorization/session (SYS-08), and network control owns multi-link routing (SYS-07).

## Internal functional blocks

| Block | Engineering responsibilities |
|---|---|
| Spot-beam manager | beam geometry, beam-pointing footprint, satellite/beam mapping, activation and footprint movement |
| Spectrum manager | carrier bandwidth, polarization, frequency reuse, co-/adjacent-channel interference and regional spectrum constraints |
| PHY abstraction | EIRP/G/T, path loss, atmospheric loss, Doppler, thermal noise, SINR, MODCOD and BLER/PER mapping |
| Timing/synchronization | propagation-delay compensation, frame timing, terminal oscillator/timing-error profile |
| MAC/RRM | per-beam uplink/downlink scheduling, resource allocation, QoS, control overhead, fairness |
| Network attachment | candidate discovery, resource admission, authentication coordination, radio bearer lifecycle |
| Mobility controller | handover eligibility, hysteresis/TTT, make-before-break versus break-before-make, beam/satellite switch |
| Measurement and accounting | per-beam utilization, scheduling grants, radio packet error, attach failures, handover interruption |

## PHY and propagation mechanics

For each user link derive range, elevation and relative radial velocity from SYS-01, available payload/beam capability from SYS-02, terminal gain/steering from SYS-04 and weather/spectrum constraints from SYS-12.

A first-order received power budget:

```text
Pr[dBW] = Pt[dBW] + Gt[dBi] + Gr[dBi]
        - FSPL[dB] - Latmos[dB] - Lpoint[dB] - Lother[dB]
FSPL[dB] = 20 log10(4*pi*d/lambda)
Doppler[Hz] ~= -(v_radial/c) * carrier_hz
```

Convert received power to `C/N0` and then `Eb/N0` or SINR using **occupied bandwidth, thermal noise, noise figure, interference and actual allocated resources**. Map SINR to an explicitly selected MCS/MODCOD and BLER reference curve; the Shannon bound may be reported only as a theoretical upper bound. Distinguish aggregate beam downlink/uplink capacities and per-terminal grant rates, subtracting control/framing overhead. A user packet cannot use the full beam rate unless the scheduler actually allocates it.

Interference model starts with simplified beam-overlap and spectral-reuse groups, advances to antenna patterns, co-channel and inter-beam coupling. A feasible user-satellite geometry alone does not imply RF coverage, regulatory eligibility or a free scheduling slot.

## Admission, MAC and handover algorithms

Attachment: `DISCOVERED → RADIO_CANDIDATE → ACQUIRING → ADMISSION_PENDING → ALLOCATED → ACTIVE → HANDOVER_PENDING → RELEASED`. Reject or defer attach on unsupported terminal capability, insufficient resource blocks, invalid regional policy, authentication denial or insufficient link margin.

P1 MAC: deterministic weighted round-robin with a per-beam frame budget; account packet overhead and terminal queue; fairness and priority are explicit parameters. P2 candidates: proportional fair, max-C/I, deficit round robin and service-class reservations, each compared on throughput, latency and starvation tests.

Handover score may combine measured `SINR_target - SINR_serving`, time-to-contact-end, beam load, steering time, network path latency and eligibility. Apply hysteresis + time-to-trigger to avoid ping-pong. A handover event includes prepare, target reservation, forwarding update, terminal retune/acquisition, final release and service outage timestamps. SYS-08 maintains logical service session independently of radio bearer.

## Contracts and ownership

Own `SpotBeam`, `SpectrumChannel`, `ResourcePool`, `RadioBearer`, `Attachment`, `HandoverTransaction`. Consume ephemeris/contacts, spacecraft capability, terminal telemetry, service policy and environment rules. Publish `BeamActivated`, `AdmissionGranted/Denied`, `RadioBearerEstablished`, `HandoverStarted/Completed/Failed`, `BeamCapacityChanged`, `RadioPacketDropped`.

Commands: `RequestAccess`, `ReserveBeamResources`, `ScheduleTraffic`, `PrepareHandover`, `CommitHandover`, `ReleaseBearer`. Queries: beam geometry/capacity and terminal bearer state at revision/time. All rates are directional and packets reference allocated bearer IDs.

## Implementation and external components

**Baseline:** Rust simplified RF budget + directional beam queue/scheduler + attachment/handover state machine, with independent unit tests. Python/SYS-01 supplies position/elevation and Doppler inputs. **Protocol fidelity:** evaluate [SNS3](https://github.com/sns3/sns3-satellite) for multi-spot beam, DVB-S2/RCS2 scheduling, error and interference models; [OpenSAND](https://www.opensand.org/overview.html) for interactive satellite broadband packet emulation. These model other documented standards, **not** a verified Starlink waveform. [OpenAirInterface](https://openairinterface.org/) only for an explicitly separate 3GPP NTN profile (current CSSL terms).

## Required verification

Radio power and SINR calculation against independent reference values; Doppler sign/units; beam footprint and steering limits; two terminals exceeding shared beam capacity produce constrained grants; weighted fairness and no starvation; interference reduces effective rate; radio blackout/handover causes measurable packet/service interruption; handover does not accidentally re-create an authorized service session. Report beam goodput, scheduled resources, BLER, packet queue delay, attach completion, handover outages and radio coverage.

**Milestones:** analytic RF/link budget → deterministic beam scheduler → admission/session coordination → handover → external protocol comparison.