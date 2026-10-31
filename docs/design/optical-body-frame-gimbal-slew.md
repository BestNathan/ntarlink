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

## Optional acceleration-limited retargeting

An actuator may specify `max_slew_acceleration_deg_per_s2` as a finite,
strictly positive angular acceleration cap. With the field omitted, the
original constant-rate zero-ramp calculation is unchanged. When supplied,
each admitted retarget uses a symmetric **rest-to-rest** angular motion
profile with instantaneous switching between acceleration, constant speed,
and deceleration. Given shortest-arc distance `theta` in degrees, maximum
rate `v` in degrees/second and acceleration `a` in degrees/second squared:

- If `theta <= v*v/a`, the triangular profile takes `2*sqrt(theta/a)` seconds.
- Otherwise, the trapezoidal profile takes `theta/v + v/a` seconds.

Travel time is rounded **up** to whole virtual nanoseconds, then combined
with the existing fixed slew dwell and acquisition delay. Both heads slew
in parallel; the slower head determines the start of acquisition. The
head remains exclusively reserved until the end of the accepted contact.
Rejected candidates do not change the stored target direction. This
assumes the head starts and ends each retarget at rest, even after a
fault-split contact. It does not integrate a motor state, momentum,
torque, tracking error or continuous target motion.

The synthetic acceleration fixture uses `v=3000 deg/s`, `a=200000
deg/s^2`, and a 90-degree retarget, requiring 45 ms rather than 30 ms.
With 10 ms acquisition, second-contact tracking begins at 175 ms.
Contract, planner and CI packet tests cover validation, travel timing,
contact rejection and packet conservation. Complexity remains O(C log C)
planning and O(C + H) memory; no scale benchmark or physical calibration
is claimed.

## Optional per-head post-slew settling

Each `optical_head_slew_actuators[]` entry may specify a nonnegative
integer `settling_ns` in scenario-relative virtual nanoseconds. Omission
preserves the existing zero-settling schedule. The dwell is applied **only
when the head's body-frame target differs from its last admitted target**
(nonzero computed slew time); a zero-angle retarget does not incur a new
settling delay. This is an operator-supplied deterministic dwell, not an
estimate from a feedback controller or measured pointing error.

For each head, the required time is `travel_ns + settling_ns` (with
checked integer overflow) for a nonzero slew, otherwise zero. The PAT
planner waits for the maximum of the two independent head requirements
and the existing `slew_ns` floor, then begins acquisition. Shared-head
reservation, fault-split recovery and rejection semantics remain unchanged:
a rejected contact does not commit a new pointing target. The `Acquiring`
and `Tracking` events, acquired windows and packet forwarding all move
accordingly; no new physical state is inferred between discrete events.

The synthetic `optical-body-frame-gimbal-settling.json` scenario applies
15 ms per head after a 45 ms acceleration-limited 90-degree slew. Its
second contact begins at 120 ms, acquires for 10 ms and tracks at **190 ms**.
The initial zero-angle contact still tracks at 20 ms. Rust planner tests
cover zero-angle behavior, asymmetric head dwell, contact rejection,
overflow and input-order determinism. CI verifies a later successful
packet forwarding window, no forwarding before acquisition, packet
conservation and repeatable JSON traces.

**Fidelity:** fixed post-motion stabilization delay only, not a measured
tracking error, continuously moving target, closed-loop gimbal controller,
optical power budget or a physical pointing guarantee. Complexity remains
O(C log C) planning and O(C + H) memory; no scale benchmark is claimed.
