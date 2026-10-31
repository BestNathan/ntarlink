# SYS-02 / SYS-03 sampled body-frame optical tracking error

This opt-in contract evaluates **supplied** target and observed optical-head
boresight directions in each spacecraft's local BODY frame. It complements
the per-head rest-to-rest gimbal slew and settling scheduler. The model is
a deterministic sampled pointing gate, **not** a servo integration, continuous
tracking guarantee, or calibrated laser link budget.

## Inputs and reference frame

`optical_body_tracking_samples[]` supplies an ordered observation for each
configured optical PAT link, beginning at scenario-relative virtual time zero.
Each record contains `link_id`, `at_ns`, `frame: "BODY"`, and independent
`from_` and `to_` target/actual boresight vectors and maximum tracking
errors in degrees. All four vectors are nonzero finite unitless directions;
they are normalized before angular comparison. Endpoint BODY frames are
independent and **must not** be compared directly. The maximum error is a
finite value in [0, 180] degrees, consistent for any shared head.

The feature requires complete `optical_head_slew_actuators[]` and
`optical_pat_slew_targets[]` coverage. A tracking observation is required at
every geometric PAT contact start, and its target direction must agree
with that contact's gimbal slew target (directional tolerance 1e-10 in
normalized dot product). This prevents stale target observations across
retargeting contacts. Observations are strictly increasing per link, bounded
to 10,000 total, and must cover every PAT link. Other native pointing modes
and user-supplied `sampled_pointing` faults are mutually exclusive to avoid
competing writers for the same sampled-pointing state domain.

## Deterministic sampled gate

At each sample the Rust SYS-02 evaluator calculates each endpoint's angular
tracking error as

```text
error_deg = atan2(norm(unit_target x unit_boresight),
                  dot(unit_target, unit_boresight)) * 180 / pi
```

Both errors must be within their respective limits (numerical comparison
tolerance 1e-9 degrees). A failed endpoint emits a sampled-pointing down
edge. A later valid sample emits an up edge. Each observation is held until
the next sample, without interpolation or implied motion between samples.

SYS-03 folds these edges with independent administrative link faults.
Tracking loss releases exclusive heads, prevents optical packet forwarding,
and requires fresh slew, settling and acquisition after recovery. Recovery
of only the administrative fault does not reopen a still-invalid pointing
state. Contact boundaries still use the configured gimbal targets and the
existing nonpreemptive head reservation policy.

## Reproducible acceptance

`scenarios/optical-body-frame-tracking-error.json` extends the synthetic
acceleration-limited and settling-aware two-contact fixture. A 90-degree
from-head boresight error at 30 ms exceeds a 5-degree threshold; pointing
recovers at 90 ms. An independent administrative outage runs from 40 to
70 ms. The expected Tracking instants are 20, 110 and 190 ms, with no
optical forwarding outside acquired windows. The CI fixture checks
contract validation, PAT loss reason, recovery, packet conservation and
byte-identical repeated JSON traces.

Validation is O(S + C + H) expected time using hash lookups for S samples,
C contact targets and H head limits; runtime evaluation is O(S) and uses
bounded sample storage. No constellation-scale benchmark was performed.

## Fidelity limitations and next step

Observed boresight is an **external input**, not predicted from the gimbal
actuator's velocity, acceleration, momentum, control law or previous tracking
state. Sample-and-hold can miss arbitrary motion between observations.
The next slice should introduce a deterministic target-following actuator
state update and an explicit tracking-error model driven by that state,
then validate it against a physical reference or high-fidelity controller.
