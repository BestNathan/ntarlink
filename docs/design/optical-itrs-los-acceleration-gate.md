# SYS-02 sampled ITRS line-of-sight angular-acceleration gate

The opt-in `optical_tracking_accelerations[]` contract extends the native
`optical_geometry_samples[]` PAT availability gate. Each PAT link may set
independent finite nonnegative limits in degrees per second squared for both
optical heads. Shared head IDs must use identical limits across configured
links. The stricter endpoint limit applies to the same reciprocal ITRS LOS
motion. The acceleration and existing angular-rate gates are independently
optional; omitted limits preserve legacy behavior.

## Finite-difference model

Three successive observations on the same link form two time intervals.
For each interval, Rust normalizes the ITRS position difference, computes
the signed shortest-arc rotation vector from cross and dot products, and
divides the angle in degrees by the interval duration in virtual seconds.
The magnitude of the difference between consecutive angular-velocity
vectors, divided by the time between interval midpoints, is the sampled
angular-acceleration proxy in degrees per second squared. This detects
reversals of equal-magnitude angular velocity that a scalar speed difference
would miss. Exact 180-degree reversals have an ambiguous rotation axis and
fail closed. Invalid or degenerate observations fail closed as well.

The first two observations establish a baseline without an acceleration
test. The third and subsequent samples may change the independent
`sampled_pointing` availability domain. A failed sample releases SYS-03
PAT head reservations and interrupts optical forwarding. Recovery must
satisfy the latest sample, all administrative faults and the PAT acquisition
dwell before routing and forwarding can resume.

## Acceptance and fidelity

The synthetic `scenarios/optical-itrs-los-acceleration-gate.json` fixture
uses an ITRS LOS 90-degree change in 30 ms followed by zero change in 20 ms:
the 3,000 to 0 degrees-per-second velocity step implies 120,000 degrees per
second squared, above a 100,000 limit. The loss at 50 ms overlaps an
administrative outage from 55 to 75 ms. A stationary 70 ms observation
recovers pointing but not the administrative fault; PAT tracking resumes
only after a fresh acquisition at 95 ms. Rust unit/contract/packet tests
and CI assert typed loss provenance, control edges, packet accounting and
deterministic replay.

This is a **sampled Earth-fixed ITRS angular-acceleration proxy**, not a
spacecraft-body gimbal actuator, motor torque/servo model, tracking-error
bound or continuous feasibility certificate. It does not incorporate body
attitude or optical head mount axes. The existing offline Python body-frame
audit remains a separate check, not equivalent to this runtime gate.
Evaluation is linear in the at-most-10,000 samples, with two preceding
observations per PAT link and no constellation-scale benchmark claim.
