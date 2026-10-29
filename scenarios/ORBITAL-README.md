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

The generator writes a companion `/tmp/ntarlink-orbital-pass.json.orbit-manifest.json` with TLE input hash, Skyfield/SGP4 versions, epoch, coordinate frame, sampling error and generated contact count.

The sample orbit config uses a 5-second coarse scan and 100-millisecond bisection tolerance for bracketed rise/set events. A contact can still be missed if two visibility transitions occur between consecutive coarse samples; reducing the scan step is required to detect such short contacts. The manifest records the scan step, refinement tolerance and algorithm. Refined boundaries round inward to the visible side, so generated contact intervals never intentionally extend beyond a bracketed geometric visibility crossing.

The Rust runner uses half-open geometric contacts `[start_ns, end_ns)`, sets affected links DOWN outside visibility and applies a configured control-plane convergence delay before route installation. Delivered packets are expected only during the pass; out-of-contact packets drop rather than teleporting.

**Limitations:** with refinement enabled, bracketed contact boundaries have at most the chosen bisection tolerance, while unbracketed short contacts may still be missed by the coarse scan; link propagation is fixed at contact midpoint; antenna pointing, acquisition delays, RF SINR/MCS, rain fade and Doppler correction remain future models. Orbit propagation is SGP4, Earth frame uses Skyfield ITRS, Earth shape is WGS84. Contact is geometric feasibility, not PHY acquisition.
