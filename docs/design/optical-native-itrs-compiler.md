# SYS-01/SYS-02 to Rust-native ITRS optical observations

The optional `--native-itrs-probes-per-contact` mode compiles production
SYS-01 Skyfield/SGP4 satellite positions and operator-supplied SYS-02 optical
head attitude into the Rust `optical_geometry_samples[]` contract. Unlike
the earlier `--native-steering-probes-per-contact` mode, it does **not**
precompute off-axis angles in Python. SYS-03 evaluates reciprocal ITRS
line-of-sight geometry and pointing envelopes in the Rust PAT planner.

## Contract and model ownership

- Satellite positions are **ITRS Earth-fixed Cartesian meters**, never raw
  TEME vectors. The pinned Skyfield/SGP4 provider supplies positions.
- Boresights are dimensionless ITRS unit vectors rotated from body-frame
  optical head axes using explicit fixed or time-indexed body-to-ITRS unit
  quaternions. Sampled attitudes use shortest-arc SLERP. Optional attitude
  angular-rate caps are validated by the existing input parser.
- Head steering envelopes are in degrees; `at_ns` is scenario-relative
  integer virtual nanoseconds from the UTC orbit epoch.
- Every PAT link receives a real t=0 observation, including links whose first
  contact starts later. Every geometric contact contributes its start,
  1/3/7 interior probes and last representable microsecond before its
  exclusive end. Duplicate timestamps are removed per link.
- The shared deterministic sampling scheduler limits output to 10,000
  observations and 10,000 optical contacts. Existing administrative faults
  remain unchanged. Precompiled `sampled_pointing` faults and existing
  pointing/geometry observations are rejected rather than mixed.
- Each observation contains both endpoint ITRS positions, both rotated
  boresights and both head limits. The Rust runtime holds each observation
  until the next sample; it is not a live ephemeris or attitude propagator.

## Usage

```sh
python -m ntarlink_orbital.optical_geometry \
  scenarios/optical-sgp4-attitude.json \
  scenarios/optical-sgp4-orbits.synthetic.json \
  scenarios/optical-sgp4-attitude-tracking.synthetic.json \
  --native-itrs-probes-per-contact 3 \
  --out /tmp/native-itrs.json

cargo run -p ntarlink-cli -- validate /tmp/native-itrs.json
cargo run -p ntarlink-cli -- simulate /tmp/native-itrs.json --json
```

The companion `.optical-manifest.json` includes exact SHA-256 digests of all
three input files and the output artifact, Earth-orientation provenance,
attitude-rate audit, `geometry_sample_count`, contact sample counts and the
explicit fidelity limitation. Failed validation leaves an existing artifact
and manifest unchanged. This is a batch compiler; the Rust packet runtime
does not invoke Python or SGP4 during a simulation.

## Acceptance and limitations

Python contract tests cover deterministic sample ordering, real t=0
initialization, sampled attitude rotation, fixed-attitude compatibility,
administrative fault preservation, invalid/degenerate ITRS positions,
exclusive modes, global sample budget, short half-open contacts, digest
reproducibility and failed-compile output safety.

CI exercises a synthetic ITRS attitude excursion through the Rust validator,
PAT planner and packet runtime, checking tracking loss, overlapping
administrative failure, reacquisition, routing events and packet accounting.
A separate production SGP4 smoke compiles the same scenario with the real
provider and checks its provenance and Rust schema acceptance. Synthetic
orbital data is not independently calibrated orbit truth.

The compiler performs O(S) position and attitude evaluations and O(S)
bounded output memory for S observations, with per-link sorting. There is
no large-constellation benchmark or performance SLA. The result is **finite
sample-and-hold geometry**, not continuous pointing certification, a
physical optical head actuator, a laser link budget, or a validated
light-time model. Excursions between samples can be missed, and the
existing packet engine's in-flight completion convention is unchanged.

## Optional spacecraft-body LOS slew-rate audit

The `--native-itrs-max-body-los-rate-deg-per-s` option is available only
with `--native-itrs-probes-per-contact`. At every adjacent native ITRS
observation, the compiler computes each endpoint's reciprocal LOS vector,
rotates it into that head's **spacecraft body frame** using the inverse
body-to-ITRS quaternion, and divides the shortest angular displacement by
elapsed scenario virtual time (seconds). Both heads must satisfy the
nonnegative finite limit in degrees per second. The first observation is
a baseline; observations are compared across contact gaps as well.

The opt-in fail-closed audit runs before output replacement. Its per-head
peak sampled rate, threshold, reference frame and limitations are recorded
under `native_itrs_plan.body_los_rate_audit` in the SHA-256-bound manifest.
This catches attitude-driven relative target motion missed by an ITRS-only
LOS rate check. The cap is shared by both endpoints in this first bounded
version; per-head actuator parameters are future work.

This is a **finite-sample average angular velocity gate**, not a gimbal
controller, acceleration limit, tracking-error integration or continuous
feasibility certificate. It does not alter the Rust runtime's native
geometry or packet forwarding behavior; an excessive input is rejected
offline rather than converted to a runtime Tracking Loss event.

## Optional sampled body-frame LOS angular-acceleration audit

The opt-in `--native-itrs-max-body-los-accel-deg-per-s2` option is valid only
with `--native-itrs-probes-per-contact`. For each optical head, the compiler
rotates consecutive ITRS LOS observations into the spacecraft body frame,
computes a shortest-arc **angular velocity vector** (degrees per virtual
second), and differences consecutive vectors over the elapsed time between
their interval midpoints. The magnitude is a finite-difference angular
acceleration proxy in degrees per second squared. A direction reversal is
therefore detected even if adjacent scalar angular speeds are equal.

The first observation establishes the LOS baseline; two intervals (three
observations) are required before acceleration is defined. Exact antiparallel
LOS vectors have no uniquely determined rotation axis and are rejected when
the acceleration audit is enabled. A nonnegative finite threshold is required.
The optional acceleration cap is independent of the existing rate cap; both
may be enabled. Per-head peak sampled acceleration, the configured threshold,
reference frame and fidelity warning are recorded in the SHA-256-bound
`native_itrs_plan.body_los_acceleration_audit` manifest field. Invalid
inputs and exceeded limits fail before artifact replacement.

This is **not** an optical gimbal actuator simulation: it does not integrate
torque, motor motion, settling, tracking error or continuous feasibility.
Unequal sample spacing is handled by interval-midpoint separation, but
unobserved motion between samples remains invisible. Complexity is O(S)
time and O(L) previous-vector state for at most 10,000 observations across L
PAT links. Unit tests cover rate steps, direction reversal, invalid/ambiguous
inputs, reproducible provenance and failed-output preservation.
