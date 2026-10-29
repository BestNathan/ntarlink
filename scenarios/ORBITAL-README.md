# Historical ISS pass: sample orbital integration

This test fixture uses a **public 2014 ISS TLE from the Skyfield documentation**, not an actual Starlink satellite, network or modem. It demonstrates physically derived visibility windows and geometric propagation delay feeding the same Rust packet simulation used by the LEO network. The historical epoch is deliberately close to the historical element epoch; no live ephemeris download occurs.

Both the test terminal and the feeder gateway intentionally share a ground coordinate in Bluffton, Ohio to make a reproducible laboratory fixture. This is not a representative production topology.

The reference source is [Skyfield's Earth Satellites guide](https://rhodesmill.org/skyfield/earth-satellites.html); it describes an ISS pass above 30 degrees around 2014-01-23 06:25–06:28 UTC.

## Generate refined geometric contacts

```sh
python -m pip install -e systems/orbital/python
python -m ntarlink_orbital.contacts \
  scenarios/orbital-pass.json \
  scenarios/orbit-sources.sample.json \
  --out /tmp/ntarlink-orbital-pass.json
cargo run -p ntarlink-cli -- simulate /tmp/ntarlink-orbital-pass.json --json > /tmp/pass-report.json
```

The generator writes a companion `/tmp/ntarlink-orbital-pass.json.orbit-manifest.json` with TLE input hash, Skyfield/SGP4 versions, epoch, coordinate frame, sampling error, generated contact count and dynamic delay sample count.

The example orbit configuration also specifies **illustrative, non-Starlink** reference carriers of 12 GHz for `user-iss` and 18 GHz for `feeder-iss`. It writes `/tmp/ntarlink-orbital-pass.json.orbit-kinematics.json` with one observation per configured link/contact: midpoint sample time, range rate in m/s (positive when receding), and first-order one-way Doppler shift in Hz (negative when receding). The default central difference spans 0.5 seconds on each side of the midpoint and is shortened for short contacts. The manifest includes a SHA-256 digest of this separate artifact. This analytical output does **not** change the packet simulator or implement carrier recovery.

The published 2014 ISS frame and topocentric observables are covered by [frame reference regression tests](../docs/design/orbital-frame-validation.md), including a guard against treating raw TEME as Earth-fixed coordinates. The default coarse predictor also checks the final partial sample interval, although its edge timing remains coarse.

The sample orbit config uses a 5-second coarse scan and 100-millisecond bisection tolerance for bracketed rise/set events. A contact can still be missed if two visibility transitions occur between consecutive coarse samples; reducing the scan step is required to detect such short contacts. The manifest records the scan step, refinement tolerance and algorithm. Refined boundaries round inward to the visible side, so generated contact intervals never intentionally extend beyond a bracketed geometric visibility crossing.

The Rust runner uses half-open geometric contacts `[start_ns, end_ns)`, sets affected links DOWN outside visibility and applies a configured control-plane convergence delay before route installation. The optional `propagation_delay_sample_step_ns: 5000000000` setting generates ECEF range/c samples every five seconds plus both window endpoints (the last representable nanosecond before closure). Rust linearly interpolates these values at **transmission completion**, recording each hop's effective `propagation_ns`. The original midpoint `propagation_delay_ns` remains the route weight; no route reinstallation is triggered by a delay sample. Delivered packets are expected only during the pass; out-of-contact packets drop rather than teleporting.

**Limitations:** with refinement enabled, bracketed contact boundaries have at most the chosen bisection tolerance, while unbracketed short contacts may still be missed by the coarse scan. Dynamic delay uses sampled vacuum geometric range and piecewise-linear interpolation, not light-time iteration, atmospheric group delay or changing route weights; without the optional samples, delay is fixed at contact midpoint. Antenna pointing, acquisition delays, RF SINR/MCS, rain fade and time-resolved Doppler correction remain future models. Orbit propagation is SGP4, Earth frame uses Skyfield ITRS, Earth shape is WGS84. Contact is geometric feasibility, not PHY acquisition.
