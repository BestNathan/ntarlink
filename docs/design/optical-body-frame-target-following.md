# SYS-02 sampled BODY-frame optical-head target follower

This opt-in deterministic approximation predicts optical-head boresight from
previous actuator state and target commands instead of trusting externally
supplied boresight. It is a sampled rest-to-rest target follower, not a
closed-loop servo, calibrated torque model or continuous pointing proof.

## Shared virtual-time command execution

Both the provisional follower pass and the later lease-checked replay now use
one packet-adapter executor over SYS-03's canonical Release, Reserve and Sample
event sequence. The provisional pass has no reservations and yields sampled
success/error observations plus immutable per-head boundary projections for
PAT planning. After planning, the same executor replays the accepted half-open
reservations, compares full observation results, and rejects endpoint or lease
mismatches. All configured reservation and sampled-link identities are
preflighted before SYS-02 state mutation. Equal-time ordering remains
Release, Reserve, Sample, including fault-gap target updates and exact handoff.

This unifies command application, **not** actuator state across the two passes.
PAT still requires provisional future observations before it can plan its
reservations. No live feedback, shared-head predicted arbitration or continuous
servo model is implied.

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

## Link-scoped sampled command ownership

The per-head runtime now supports an atomic link-scoped pair lease. The
packet adapter supplies the PAT link ID when committing each predicted sample.
A different link cannot command a leased head, a link cannot silently swap
its leased endpoint pair, and unscoped single/pair commands cannot bypass an
active lease. Invalid commands, partial releases
and wrong-owner releases leave both actuator histories and leases intact.
An explicit paired release preserves boresight and target state for later
retargeting by another link. `release_pair_at(link, from, to, at_ns)` also
records a virtual-time release barrier on both heads: a later owner cannot
retroactively issue a command before that boundary. A release preceding either
head's last successful sample or prior release barrier is rejected atomically.
The legacy `release_pair()` releases at the latest committed sample timestamp
for compatibility; a PAT lifecycle bridge must use the time-aware API with
the actual reservation end time. Standalone unleased APIs remain supported.

**Boundary:** This is command ownership only, not PAT resource arbitration.
The current scenario contract still rejects shared-head predicted command
streams. The packet adapter now releases leases at accepted PAT reservation ends
in an offline second pass, including fault-split contact boundaries. SYS-03 still owns the
separate geometric contact reservations. The offline replay aligns lease acquisition/release with PAT admission and
fault/contact boundaries; shared-head predicted scenarios remain forbidden. This step adds constant
expected-time ownership checks per pair and O(H) owner storage.

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

## SYS-03 exclusive reservation timeline contract

`PatPlan.reservations[]` now reports the accepted pair of exclusive heads,
their link identity and a half-open virtual-time interval
`[start_ns, end_ns)`. The start is the time both heads become available
after any competing reservation, not necessarily the candidate's geometric
start. The interval includes slew, settling, acquisition and tracking.
`PatPlan.windows[]` continues to describe only the later packet-forwarding
tracking interval. Rejected candidates do not reserve either head or change
its pointing. The timeline is sorted by actual reservation start and link
identity, and a shared head can be handed off at the exact exclusive end.

The contract is tested in SYS-03 for simultaneous independent links,
shared-head contention, short rejected contacts and adjacent contact
boundaries. A SYS-03/packet-runtime regression compares the reservation
intervals with the admitted tracking contacts and packet conservation.

**Integration boundary:** These typed spans now drive a second, offline
`BodyHeadRuntime` replay with virtual-time lease transitions and atomic pair
ownership across fault splits. The provisional follower pass still precedes
PAT scheduling, so this is not single-pass event-driven lease release,
shared-head predicted following or a single actuator owner.
The extra timeline uses O(A) storage and O(A log A) ordering for A accepted
candidates; it adds no physical fidelity or packet execution events.

## Offline PAT reservation and command-lease replay

The packet adapter now consumes `PatPlan.reservations[]` after SYS-03
planning. It first evaluates all sampled follower commands without leases
to obtain immutable PAT boundary projections. A second deterministic pass
replays those commands through a fresh `BodyHeadRuntime` in virtual-time
order. Each accepted reservation atomically claims both heads with
`reserve_pair_for_link`, and its exclusive end calls
`release_pair_at(link, from, to, end_ns)`. At equal timestamps the order is
release, reserve, sample; this permits same-time handoff without retroactive
commands. No synthetic actuator observation is inserted at a lease start.

A sample outside any reservation (for example during administrative or
sampled-pointing loss) uses the unleased pair API to retain target-following
state for later PAT recovery. This is safe only because the current scenario
contract prohibits predicted streams sharing heads across links. A command
inside a reservation must use its link-scoped lease. The replay compares
the exact predicted boresight results and success/failure outcomes against
the provisional pass and fails closed if they diverge. A conflicting
reservation or invalid historical handoff aborts the run.

The bridge adds O((A + S) log(A + S)) virtual-time transition sorting and
O(A + S + H) auxiliary memory. It is still an offline two-pass adapter:
SYS-03 does not yet schedule a single live actuator runtime, and shared-head
predicted admission remains unsupported. A future single-pass event engine
must arbitrate target commands during faults and model velocity-continuous
servo dynamics without allowing unreserved commands to bypass ownership.

## Provisional command preflight

The provisional follower pass now uses the same SYS-03
`pat_runtime_transitions` sample ordering as the subsequent lease replay,
with an empty reservation set. This rejects blank link IDs and duplicate
link/timestamp commands before any provisional head mutation, and reports
unknown PAT links as errors rather than indexing an absent configuration.
It prevents divergent command ordering and unvalidated-input panics but does
not eliminate the offline second pass or permit shared-head predicted
command streams.

## Canonical PAT lease boundary validation

SYS-03 exports a typed, validated reservation-transition sequence. Each
accepted half-open reservation emits a Reserve at its start and a Release at
its exclusive end. At a shared virtual timestamp, releases precede reserves,
then the packet adapter processes sampled commands. Ordering is independent
of the input reservation vector and deterministic by link ID. Preflight
rejects malformed intervals, duplicate active links and overlapping head
occupancy before SYS-02 lease replay mutates actuator state.

SYS-02 still owns actuator samples and command leases; SYS-03 owns the
reservation plan and canonical boundary ordering. The packet adapter consumes
typed edges instead of constructing its own reservation start/end events.
Unit tests cover same-instant handoff, independent parallel heads, reversed
input, invalid intervals and conflicting ownership. Packet-runtime tests
also replay reversed reservations and verify identical follower projections,
packet conservation and deterministic output. This adds an offline O(A log A)
sort and O(A) memory for A accepted reservations; it is not a live single-pass
arbiter or a servo model.

## Active lease acquisition-time safety

SYS-02 records the acquisition timestamp for each active link-scoped head pair.
A command issued under that lease, or its explicit release, cannot precede
its acquisition boundary, even when the pair has not received any sampled
actuator command. A failed historical operation preserves both head owners,
command history and release barriers. A same-instant command or release is
valid, preserving half-open PAT reservation semantics. The compatibility
`release_pair()` API releases an unsampled reservation no earlier than its
acquisition timestamp; PAT replay continues to use the explicit
`release_pair_at()` boundary.

This closes a gap in the SYS-02 API when it is used independently of the
sorted SYS-03 replay. It does not make the offline two-pass adapter into a
live shared-head arbiter, continuous servo or physical pointing guarantee.
Per-link acquisition timestamps add O(L) state for L active leases and
constant expected-time checks per paired command and release. A Rust
regression covers pre-acquisition commands, premature release, atomic
failure, exact-boundary handoff and unsampled legacy release. The existing
packet/PAT acceptance scenarios remain the cross-subsystem compatibility
checks.

## Canonical offline PAT command event ordering

SYS-03 exposes `pat_runtime_transitions(reservations, samples)`, merging
validated head-lease boundaries and sampled BODY-frame target commands.
The typed timeline sorts by virtual timestamp, then Release before Reserve
before Sample, then link ID and original index. This allows an exclusive
head to change owners at a half-open boundary before a command at that
same timestamp. Duplicate commands for one link/timestamp, blank sample
links and invalid/overlapping reservations fail before the SYS-02 replay
mutates any follower state. The packet adapter no longer maintains a
separate transition sorting implementation. SYS-03 also projects the
active reservation index (or none) onto each Sample after sorting lease
boundaries. The packet adapter consumes that projection rather than
maintaining a second active-link set, and checks the projected link/head
pair before applying the leased command. At a half-open release boundary,
a command is unleased unless another reservation for its link starts at
that instant. Unit tests cover fault gaps, boundary ownership and reversed
reservation input; packet replay tests reject a swapped endpoint pair.

The provisional SYS-02 pass still supplies immutable head projections
for PAT planning; the canonical event stream only drives the second,
offline lease-verification pass. Fault-gap commands remain unleased
under the existing exclusive-head scenario contract. This change is not
a live single-pass actuator scheduler or shared-head command arbiter.


## Reservation endpoint preflight

The packet adapter verifies every accepted SYS-03 reservation against its
configured PAT link and ordered optical-head endpoints before starting the
SYS-02 lease replay. The check also covers intervals without sampled
commands, which previously bypassed the sample-time endpoint guard.
Unknown links and mismatched head pairs fail closed without modifying
actuator state. The sample-time endpoint check remains as defense in depth.
This is O(A) expected-time validation for A accepted reservations and does
not change the offline two-pass model or admit shared-head predicted streams.
