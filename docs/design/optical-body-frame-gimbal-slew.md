# SYS-02 / SYS-03 body-frame optical-head slew actuator

The optional `optical_head_slew_actuators[]` and
`optical_pat_slew_targets[]` scenario fields add deterministic, per-head
retargeting travel time to the existing exclusive-head PAT planner. This is
the first **actuator scheduling** slice: a constant angular-speed, zero-ramp
kinematic assumption with explicitly supplied spacecraft-body-frame targets.
It does not infer body attitude, optical geometry, motor torque or laser PHY.

## Contract and reference frame

Each exclusive PAT head has exactly one actuator record with a unique
`head_id`, an initial direction vector in the spacecraft's **BODY** frame,
and a finite positive maximum angular slew rate in degrees/second. The
unitless vectors are normalized by the planner. A target record is required
for every geometric optical PAT contact, keyed by `(link_id, at_ns)`, where
`at_ns` is the original contact start. The target provides independent
`from_target_body` and `to_target_body` directions with explicit
`frame: "BODY"`. The source scenario is validated for head ownership,
complete coverage, nonzero finite vectors and unique records. Omitted fields
retain the previous fixed-slew PAT behavior.

Body frames are **local to each spacecraft**; the two endpoint vectors
must not be compared to one another. Operators must supply consistent
targets with their own attitude and orbit model. The runtime does not
silently convert TEME, ITRS or another frame into BODY.

## Deterministic planning

At each admitted candidate start, the planner first waits until both
heads' exclusive reservations end. Each head's angular distance between
its last accepted target (or explicit initial direction) and the new target
is `atan2(|u × v|, u · v)` in degrees. The required travel time is
`ceil(angle_deg / max_rate_deg_per_s * 1e9)` virtual nanoseconds. Both
heads move in parallel. The candidate's effective slew dwell is the
maximum of its existing fixed `slew_ns` and both calculated travel times,
followed by the existing `acquisition_ns`. A contact whose acquisition
cannot finish before its exclusive end is rejected. A rejected candidate
does **not** update actuator pointing state.

Administrative link faults and optical-head outages split the contact
into admitted segments; each segment inherits the original geometric
contact's body-frame target and must reacquire. Shared heads remain
exclusive. Packet forwarding starts only at the acquired PAT window and
route convergence continues to use the existing control delay.

## Evidence and limitations

The synthetic `scenarios/optical-body-frame-gimbal-slew.json` fixture
retargets both heads by 90 degrees at 120 ms. At 3,000 degrees/second
the second contact requires 30 ms of actuator travel and 10 ms of
acquisition, delaying tracking from the legacy 140 ms to **160 ms**.
The first contact still tracks at 20 ms. Unit tests cover deterministic
input order, rejected too-short contacts, malformed/missing target
vectors and zero/invalid actuator rates. Rust contract and packet-runtime
tests, plus CI, verify schema acceptance, PAT event timing, packet
accounting and suppressed forwarding before reacquisition.

Complexity is O(C log C) for sorted candidates and O(C) head-state updates
for C admitted contact segments; memory is O(C + H), with H exclusive heads.
No constellation-scale benchmark has been performed.

This is not a gimbal servo, acceleration-limited trajectory, motor
saturation, continuous tracking-error calculation or optical link budget.
Target directions are held constant within each contact, even when a
contact is split. The next fidelity step is sampled target evolution,
finite acceleration/settling and tracking-error-aware loss and recovery.
