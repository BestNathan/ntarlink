# SYS-12 — Spectrum, Radio Environment, Weather and Regulatory Constraints

**Mission:** publish exogenous environmental and regulatory constraints on potential links and service eligibility. SYS-12 models **what is permitted/physically degraded**; it does not own beam MAC scheduling, route installation or real governmental authorization.

## Components

1. **Spectrum-plan registry:** service profile, carrier center frequency, bandwidth, polarization, reuse group, emission masks and region.
2. **Regulatory scenario engine:** allowed service geography, altitude/terminal class constraints, gateway operating conditions, effective dates.
3. **Atmosphere and weather:** spatial/time-varying rain, gases, cloud/fog, tropospheric conditions, attenuation maps and temporal interpolation.
4. **Interference engine:** co-channel noise/interferer catalog, spectral overlap, geometry, antenna coupling and impact on SINR.
5. **Obstruction/terrain:** terminal local obstruction profile, building/terrain masks and minimum elevation.
6. **Environmental event generator:** rain-cell movement, weather outage, interference onset/cessation and policy updates.

## Technical propagation policy

Represent frequency in Hz, bandwidth in Hz, power in W or dBW, gain in dBi and losses in dB; linear and logarithmic values must not mix. Space/ground geometry comes from SYS-01 and antenna patterns from SYS-04/05/06; this subsystem outputs attenuation or admissibility constraints with source/time resolution.

Weather attenuation depends on frequency, elevation, path length, rain rate and applicable model. An initial model can use a configurable attenuation curve with labeled assumptions; eventual validation should use [ITU-R P.618](https://www.itu.int/rec/R-REC-P.618) for Earth-space attenuation and appropriate additional recommendations for gaseous/cloud effects. ITU-R texts/data licensing and applicability need independent review. For an optical ISL above the atmosphere, do **not** apply terrestrial rain fade; optical feeder/ground paths require cloud and turbulence models.

Interference model takes all simultaneously allocated overlapping carriers and computes a declared interference contribution per receiver. SYS-05/06 use the result for MCS/capacity decisions; SYS-12 should not schedule those carriers itself. Regional service constraints are simulated configuration rather than claims about actual government licenses or Starlink authorization.

## Data model and APIs

Own `RegionPolicy`, `SpectrumAllocation`, `WeatherCell`, `AttenuationField`, `Interferer` and `ObstructionMask`. APIs: `GetAllowedSpectrum(region, time, service_type)`, `GetAttenuation(endpoint_pair, frequency, time)`, `GetInterference(receiver, channels, time)`, `IsServicePermitted(location,time,terminal_class)`. Events: `WeatherFieldChanged`, `RadioInterferenceRaised/Cleared`, `SpectrumRuleChanged`, `RegionServiceEligibilityChanged`.

Use WGS84 polygon bounds with explicit longitude wrap and antimeridian handling; spatial interpolation and temporal sampling algorithms must include resolution and error estimates.

## Implementation and dependencies

Python preprocessing of weather/gridded data to deterministic versioned snapshots; Rust evaluation in fast packet/run loops. External data sources are optional and must be **captured/pinned** rather than fetched live during deterministic experiments. ITU-R propagation models, openly licensed weather data and antenna pattern sources are candidates after provenance review; avoid redistributing restricted data without permission.

## Faults and tests

Missing/interpolated weather, unknown regulatory region, interference model saturation, invalid spectrum overlap and stale policy change must produce explicit warnings or deny-by-policy as configured. Service must not be silently allowed when a critical eligibility field is unknown.

**Tests:** frequency/unit conversions; controlled rain-rate increase causes reproducible feeder/user RF degradation; no rain attenuation on vacuum ISL; overlapping same-frequency beams raise SINR penalty; terminal crossing prohibited region loses service for correct policy reason; weather time-grid interpolation within tolerance. Metrics include fade margin, blocked coverage fraction, interference rise events and service eligibility changes.