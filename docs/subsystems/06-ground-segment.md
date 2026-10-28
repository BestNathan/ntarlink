# SYS-06 — Feeder Ground Stations, Antennas and Radio Gateways

**Mission:** terminate and manage **space-to-ground feeder** links, station antennas and ground-side radio infrastructure. A ground feeder site is **not** the subscriber core or public internet PoP (SYS-08/09).

## Components

1. **Ground site registry:** geodetic position, altitude, regional location, climate model, availability, site IDs and coverage constraints.
2. **Antenna farm and tracking:** exclusive beam/antenna resources, azimuth/elevation limits, slew/acquisition delays, tracking and pointing error.
3. **Feeder link manager:** bidirectional Ka/other selected frequency links, contact candidates, acquisition and channel occupancy.
4. **Radio/physical model:** EIRP/G/T, feeder rain fade, atmospherics, noise/interference, feeder coding and effective capacity.
5. **Gateway radio termination:** bridge feeder bearer/frames to a ground network ingress; preserve modeled encapsulation/processing costs.
6. **Site control:** radio health, power and environmental alerts, maintenance, antenna reassignment and feeder failover.
7. **Ground ingress interface:** hand-off into an explicit terrestrial backhaul link to PoP, with distinct capacity and queue metrics.

## Technical link and resource design

SYS-01 computes satellite/site line-of-sight and elevation windows; SYS-02 constrains satellite-side feeder radios; SYS-12 supplies weather/region. SYS-06 additionally checks antenna occupancy, frequency/polarization, acquisition time, uplink/downlink power and the site capacity ledger. Its feeder state machine is `INVISIBLE → VISIBLE → TRACKING → ACQUIRING → UP → DEGRADED → LOST`.

For each feeder direction compute `FSPL`, transmitter/receiver gain, pointing loss, atmospheric/gaseous/rain attenuation, receiver noise and coding/framing overhead. High-frequency feeder link rain fade should be configurable with externally justified reference-model parameters. Separate user RF and feeder carriers even if the same satellite is involved. Site capacity can be constrained by a limited number of simultaneous antennas and ground transport bandwidth.

A feeder modem’s output is not an immediate public internet route: `GroundSite → BackhaulLink → PoP` is modeled by SYS-09. A radio site may have excellent satellite connectivity and still lose user traffic due to terrestrial backhaul outage.

## Data model and interfaces

Own `GroundSiteId`, `AntennaId`, `FeederBearerId`, `StationResourceLease`, `SiteHealthState`, `RadioGatewayId`. External inputs: position/elevation, spacecraft resource state, regional spectrum/weather, SYS-07 gateway/route plan and SYS-09 backhaul availability.

Commands: `RequestFeederAcquisition`, `AllocateGroundAntenna`, `ReleaseAntenna`, `SetSiteOperationalMode`, `ApplyFeederPolicy`. Events: `FeederContactCandidate`, `AntennaTracking`, `FeederLinkEstablished`, `FeederLinkDegraded`, `FeederLinkLost`, `GroundSiteUnavailable`. Outputs include resource availability, direction-specific capacity/delay and termination-point mapping to SYS-09.

## External dependencies and staged implementation

P1 simplified feeder RF + station tracking/antenna allocation in Rust; Python orbit/weather snapshots as inputs. [OpenSAND](https://www.opensand.org/overview.html) is a promising ground/user satellite broadband emulation reference but uses DVB-S2/RCS2 rather than an asserted Starlink feeder waveform; GPL/LGPL and operating environment review required. [GNU Radio](https://www.gnuradio.org/) is relevant for experimental waveform/SDR integration, not for constellation-scale event simulation. Optional SNS3 feeder link studies must pin exact matching ns-3 modules and data.

## Failure modes and acceptance

Rain fade, site power loss, antenna jam/overbooking, unavailable satellite radio, ground station maintenance, tracking timeout, regulatory exclusion and terrestrial backhaul outage. Fault ownership must distinguish **feeder failed** from **backhaul failed**; SYS-07 handles selection of an alternate site and SYS-13 reports customer impact.

**Tests:** two satellites contending for one ground antenna are not both admitted; geometry expiry forces link loss; weather attenuation reduces capacity predictably; gateway failover includes acquisition and route install delay; a healthy feeder with failed backhaul is observable and still cannot reach internet sink; no double resource reservations. Capture site utilization, active feeder count, C/N, rain fade, link uptime, time-to-acquire and egress handoff packets.

**Milestones:** site/antenna catalog → feeder link budget and acquisition → terrestrial handoff → failure and gateway-selection integration → external protocol emulation.