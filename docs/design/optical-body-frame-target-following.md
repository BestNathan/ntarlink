# SYS-02 sampled BODY-frame optical-head target follower

This opt-in deterministic approximation predicts optical-head boresight from
previous actuator state and target commands instead of trusting externally
supplied boresight. It is a sampled rest-to-rest target follower, not a
closed-loop servo, calibrated torque model or continuous pointing proof.

## Contract and reference frame

`optical_body_following_samples[]` supplies ordered target commands for each
PAT link, beginning at scenario-relative virtual time zero and covering every
geometric contact start. Each command has `link_id`, `at_ns`,
`frame: "BODY"`, independent unitless from/to target directions and per-head
maximum angular tracking errors in degrees. The endpoint BODY frames are
distinct and must never be compared directly. Head initial pointing, maximum
angular speed (deg/s), optional angular acceleration (deg/s²) and PAT settling
dwell come from `optical_head_slew_actuators[]`.

Commands are strictly increasing per link, bounded to 10,000 observations,
and contact-start commands must agree with `optical_pat_slew_targets[]`.
This mode cannot mix with observed `optical_body_tracking_samples[]`, other
pointing modes or manually injected sampled-pointing faults. This first
implementation rejects a head shared by multiple PAT links because command
arbitration is not modeled.

## Deterministic sampled state update

Each head retains its normalized predicted boresight, previous held target
and last sample timestamp. At t, the head first advances toward the target
held during the preceding interval dt, then latches the new target. Its
shortest-arc travel is limited by the following rest-to-rest angular budget:

```text
No acceleration cap: D = v * dt
Acceleration cap a and maximum rate v:
  dt <= 2*v/a: D = a*dt*dt/4
  otherwise:   D = v*(dt - v/a)
```

Here D is in degrees and dt in seconds. Each sample interval starts and ends
at rest; neither velocity nor torque propagates across boundaries. If the
held target is reachable, predicted boresight equals that target. Otherwise
the head advances by D on the shortest arc. Partial travel toward an exactly
antiparallel target fails closed because the rotation plane is ambiguous.

The predicted boresight is compared with the **new** target at the sample
instant via atan2(cross-product magnitude, dot product). Both endpoints must
meet their own error limits. SYS-03 combines these pointing edges with
independent administrative faults. Recovery requires fresh PAT acquisition.

## Acceptance and fidelity

`scenarios/optical-body-frame-target-following.json` commands a 90-degree
change at 30 ms with 3,000 deg/s speed and 200,000 deg/s² acceleration caps.
The first 30 ms update retains the old boresight, the next advances only
45 degrees, and the third reaches the target. A second contact retargets
the other head at 120 ms. An administrative outage runs from 40 to 70 ms.
Rust tests cover lag, recovery, invalid directions, antiparallel ambiguity,
PAT loss/reacquisition, packet accounting and deterministic replay.

Complexity is O(S log S + H) for S samples and H heads, with O(H) state.
This is not a continuous motion guarantee. The existing PAT planner still
uses contact-start slew targets and independent head reservations; follower
state is not synchronized into the planner after a fault. Settling dwell is
applied by PAT acquisition scheduling, not the follower's between-sample
tracking integration. Shared-head arbitration, persistent velocity, a
closed-loop controller, physical calibration and constellation-scale
benchmarks remain future work.
