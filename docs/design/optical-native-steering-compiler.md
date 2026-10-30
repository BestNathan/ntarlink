# SYS-01/SYS-02 to native optical steering observations

The optional `--native-steering-probes-per-contact` compiler mode bridges
the production SYS-01 SGP4/Skyfield ITRS position provider and operator-supplied
SYS-02 sampled spacecraft attitude into the Rust SYS-03 PAT planner. It emits
`optical_steering_samples[]` rather than static `optical_geometry[]` snapshots
or synthetic `sampled_pointing` link faults. Rust evaluates the observations
at virtual-time sample boundaries, combines them with independent
administrative link faults, and applies PAT reacquisition dwell.

## Coordinate and time contracts

- Orbit positions are ITRS Earth-fixed Cartesian **meters** from the pinned
  Skyfield/SGP4 provider, never raw TEME vectors.
- Head axes are dimensionless body-frame unit vectors transformed by
  operator-supplied, time-indexed body-to-ITRS unit quaternions (shortest-arc
  SLERP). The attitude model is not an attitude propagator.
- Output off-axis and per-head steering limits are **degrees**.
- `at_ns` is scenario-relative **virtual nanoseconds** from the UTC orbit
  epoch. The compiler evaluates on a microsecond-aligned grid; the Rust
  runtime holds each observation until the next sample.
- Each PAT link has a real t=0 geometry/attitude observation, even when its
  first geometric contact begins later. For every contact the compiler also
  samples its start, 1/3/7 interior probes and final representable
  microsecond before the exclusive end.
- Observations are sorted by link ID and timestamp, with duplicate sample
  instants removed. All PAT links are covered and the global budget is
  10,000 observations.

## Usage

```sh
python -m ntarlink_orbital.optical_geometry \
  scenarios/optical-sgp4-attitude.json \
  scenarios/optical-sgp4-orbits.synthetic.json \
  scenarios/optical-sgp4-attitude-tracking.synthetic.json \
  --native-steering-probes-per-contact 3 \
  --out /tmp/native-steering.json

cargo run -p ntarlink-cli -- validate /tmp/native-steering.json
cargo run -p ntarlink-cli -- simulate /tmp/native-steering.json --json
```

The output manifest records the exact input and artifact SHA-256 digests,
attitude model, frame/time scale, steering sample count, contact probe plan
and provider Earth-orientation metadata. The artifact is replaced only after
successful compilation; failed validation does not overwrite existing files.
The CLI mode is exclusive with offline pointing audits and legacy fault-plan
compilation. Existing administrative `faults[]` are preserved, while
precompiled `sampled_pointing` faults and existing steering inputs are
rejected to avoid competing owners of the physical pointing state.

## Verification, failure modes and fidelity

Python contract tests cover angle changes, sample ordering, delayed contact
initialization, administrative fault preservation, invalid source positions,
sample-budget enforcement, provenance and failure atomicity. CI runs a
synthetic two-satellite ITRS attitude-drift case through the actual Rust
scenario validator, PAT planner and packet engine, asserting tracking loss,
administrative overlap, reacquisition, control-plane transitions and packet
accounting. The separate production SGP4-to-PAT smoke remains enabled.

The algorithm uses O(S) bounded output memory and O(S) position/attitude
evaluations plus sorting for S observations; it has no runtime performance
SLA. This is **finite sampled, piecewise-constant steering**, not continuous
geometry, a validated orbital truth model, a laser PHY, autonomous pointing
control, or a guarantee that excursions between samples are detected.
Already-admitted packets retain the packet engine's completion-after-departure
convention. Do not interpret a passing contact as proof of optical power
margin or physical acquisition feasibility.
