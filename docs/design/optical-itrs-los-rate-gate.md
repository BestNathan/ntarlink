# SYS-02 sampled ITRS line-of-sight angular-rate gate

The optional `optical_tracking_rates[]` contract adds a bounded, deterministic
**line-of-sight (LOS) motion proxy** to Rust-native `optical_geometry_samples[]`.
Each PAT link may configure independent nonnegative finite limits in degrees
per second for its two optical heads. Identical head IDs must have identical
limits across configured links. No setting means the existing steering-only
behavior is preserved.

## Discrete evaluation

For two consecutive ITRS observations on the same link, Rust subtracts
endpoint positions (meters) to form a unit LOS vector. The required angular
displacement is computed with `atan2(|u × v|, u · v)` in degrees. It must
not exceed `min(from_rate, to_rate) × elapsed_seconds`, with a 1e-9 degree
numerical tolerance. Both endpoints see the same unsigned LOS displacement
in ITRS. The first t=0 sample has no preceding rate requirement. The standalone Rust
rate predicate also requires exact directed link-ID equality between the two
observations before evaluating their geometry; otherwise it fails closed.
This prevents accidental cross-link comparisons outside scenario validation.

A failed later sample marks the independent `sampled_pointing` fault domain
down. A subsequent sample with a feasible angular change may restore it.
Administrative failures remain independent; the combined state must be up
before SYS-03 may reacquire, reserve heads and admit optical packet forwarding.
This gate uses existing PAT loss provenance and route convergence events.

## Scope and limitations

This is an **ITRS LOS angular-rate proxy**, not a physical head slew controller.
The ITRS frame rotates with Earth; the required body-frame gimbal motion
depends on spacecraft attitude, gimbal axis geometry, angular acceleration,
tracking servo errors and finite pointing settle time, none of which are
modeled. The gate evaluates average displacement between finite observations,
then holds the result until the next observation. It does not prove continuous
feasibility, resolve excursions between probes or model optical PHY.

The bounded input is validated for PAT link identity, exclusive native ITRS
mode, finite nonnegative limits, per-head consistency and unique link entries.
There is one O(S) pass over the at-most-10,000 supplied geometry samples,
with O(L) previous-sample state for L links. No constellation-scale throughput
benchmark is claimed.

The synthetic `scenarios/optical-itrs-los-rate-gate.json` fixture rotates a
perfectly aligned LOS by 90 degrees in 30 ms (3,000 degrees/second), above a
1,000 degrees/second cap. A 50 ms return sample also exceeds the rate cap;
a stationary 70 ms sample recovers. An overlapping administrative outage
ends at 70 ms, and PAT requires a fresh 20 ms acquisition. Rust contract,
unit and packet tests plus CI assert failure provenance, delayed reacquisition,
control events, packet accounting and deterministic replay. This is synthetic
ITRS geometry, not a calibrated orbital or actuator validation.
