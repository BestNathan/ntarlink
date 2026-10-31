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

## Per-head runtime state ownership

The SYS-02 `BodyHeadRuntime` now owns the mutable follower for each
configured head and its ordered successful-command snapshots. The packet
adapter sends sampled commands to this runtime and requests immutable
`predicted_at(head, virtual_ns)` projections for SYS-03 PAT boundaries.
The runtime rejects duplicate/unknown heads and out-of-order per-head
commands. A failed head update is not recorded as a successful snapshot;
the other head remains independently observable. Projection never advances
the live follower or inserts a synthetic sample into the command timeline.

A sampled link command is applied as an atomic two-head transition: both
endpoint followers are staged and validated before either successful snapshot
is committed. If one endpoint rejects a command (including ambiguous
antiparallel motion), neither head advances. A later valid command may retry
the same timestamp. This prevents a failed link observation from silently
advancing only one actuator; it does not add shared-head arbitration or a
continuous servo. Runtime tests cover invalid second-head commands, distinct
head IDs, timestamp retries and motion failures. Existing PAT/packet scenario
checks remain the cross-subsystem compatibility acceptance.

This consolidates state ownership and removes packet-runtime's duplicate
per-head history bookkeeping. It does **not** unify PAT reservations with
the follower, implement shared-head command arbitration, or introduce
velocity-continuous servo dynamics. A historical projection uses binary
search over that head's successful samples: O(log S) lookup plus constant
kinematic work, with O(S + H) stored snapshots for S commands and H heads.
The preexisting packet scenario is a cross-subsystem regression for PAT
reacquisition, forwarding windows and packet conservation.

## Acceptance and fidelity

`scenarios/optical-body-frame-target-following.json` commands a 90-degree
change at 30 ms with 3,000 deg/s speed and 200,000 deg/s² acceleration caps.
The first 30 ms update retains the old boresight, the next advances only
45 degrees, and the third reaches the target. A second contact retargets
the other head at 120 ms. An administrative outage runs from 40 to 70 ms.
Rust tests cover lag, recovery, invalid directions, antiparallel ambiguity,
PAT loss/reacquisition, packet accounting and deterministic replay.

Complexity is O(S log S + H) for S samples and H heads, with O(H) state.
This is not a continuous motion guarantee. The PAT planner now consumes an immutable predicted boresight and held target
for each admitted contact or fault-recovery boundary. The snapshot is projected
from the latest successful follower command without inserting a new sample
into the follower's rest-to-rest integration timeline. Thus a recovery uses
the current commanded target rather than rewinding to the geometric contact's
initial target. A later contact does not double-count a slew already completed
by the follower. Legacy scenarios without predicted commands keep the prior
PAT scheduling semantics. PAT still owns independent exclusive reservations
and applies its fixed slew floor, settling dwell and acquisition time; the
follower does not simulate that settling or the acquisition controller.
Shared-head arbitration, continuous actuator velocity, a single mutable
runtime state owner, a closed-loop controller, physical calibration and
constellation-scale benchmarks remain future work.
