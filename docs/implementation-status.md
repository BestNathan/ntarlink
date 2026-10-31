## Explicit SYS-03 optical-head reservation timeline

SYS-03 now returns a typed `PatReservation` for every successfully
acquired candidate, recording the link, both exclusive head IDs, and the
half-open interval from actual head availability through geometric contact
end. Reservations include slew, settling and acquisition dwell; they are
distinct from the later packet-forwarding `AcquiredWindow`. A rejected
candidate produces no reservation. Deterministic planner regressions cover
shared-head waits, rejection without resource leakage, simultaneous
independent heads and same-time release/reacquisition. A packet-runtime
regression compares reservations and admitted forwarding windows for a
two-contact PAT fixture, including packet conservation.

**Boundary and cost:** The reservation vector adds O(A) storage and
O(A log A) ordering for A accepted candidates. This exposes the exact
SYS-03 ownership intervals needed for a later SYS-02 lease bridge; the
packet adapter does not yet consume these intervals to release
`BodyHeadRuntime` command leases. Predicted shared-head streams remain
disallowed. No continuous servo, calibrated PHY or new packet admission
behavior is claimed. See [design](design/optical-body-frame-target-following.md).

## Virtual-time optical-head lease handoff barrier

SYS-02 now provides an atomic `release_pair_at(..., at_ns)` operation that
preserves the two followers and records the earliest allowed timestamp for
subsequent commands on each released head. It rejects releases before either
head's latest successful sample or prior release boundary. A later owner,
including an unscoped caller, cannot submit a command retroactively before
the release. A Rust regression covers wrong-owner release, rollback, early
handoff rejection, same-time handoff, repeated handoff and history retention.

**Scope:** This is a virtual-time-safe lease primitive, not automatic PAT
reservation release or shared-head predicted scenario admission. The existing
`release_pair()` remains compatible and uses the latest committed sample
time; future PAT integration must pass its actual reservation end timestamp.
The extra barrier map requires O(H) memory and O(1) expected checks.

## Link-scoped sampled optical-head command ownership

SYS-02 now supports atomic per-link command leases for both predicted optical
heads. The packet adapter uses the link-scoped paired observation API;
conflicting owners and unscoped writes cannot mutate a leased head. Explicit
paired release preserves actuator history and rejects wrong-owner or partial
release without side effects. Rust tests cover conflicts, failed-command
rollback, retry, release and retargeting by a different link; the existing
predicted follower packet/PAT scenario remains the cross-subsystem regression.

**Scope and cost:** O(1) expected owner checks and O(H) owner state. The
scenario contract still forbids shared-head predicted streams, and the packet
adapter does not yet release leases on PAT events. These command leases do
not replace SYS-03's exclusive contact reservations, model continuous servo
velocity or establish shared-head PAT arbitration. See
[design](design/optical-body-frame-target-following.md).

## Atomic sampled optical-head pair transitions

SYS-02 now stages both heads of each predicted BODY-frame link observation
before committing either follower or its successful-command history. If one
endpoint rejects a command, both remain unchanged and a valid same-timestamp
retry is possible. The packet adapter uses this atomic API. Rust regressions
cover invalid second-head targets, ambiguous antiparallel motion, unknown or
identical heads and retries; the existing synthetic PAT/packet scenario remains
the cross-subsystem compatibility check.

**Fidelity and performance:** constant-time staging of two follower clones
and two successful snapshots per sample; no new scenario fields. This prevents
partial state commits but does not model shared-head arbitration, continuous
velocity, servo control or optical PHY. See
[design](design/optical-body-frame-target-following.md).

## Per-head predicted optical actuator runtime ownership

SYS-02 now owns each predicted BODY-frame head's mutable follower and
ordered successful-command snapshots in `BodyHeadRuntime`. SYS-03 PAT
requests read-only projections at admitted contact and recovery boundaries
rather than maintaining its own follower-history map. Unit regressions
cover independent heads, duplicate/unknown IDs, invalid and out-of-order
samples, non-mutating projection and retry after a rejected command. The
existing end-to-end follower scenario remains the packet/PAT acceptance
case; the runtime has no new scenario fields.

**Fidelity and performance:** deterministic sampled rest-to-rest motion,
O(S + H) snapshot storage and O(log S) per-head historical lookup. PAT
reservations and servo state are still distinct; this is not shared-head
arbitration, continuous velocity, physical calibration or a closed-loop
controller. See [design](design/optical-body-frame-target-following.md).

## Opt-in sampled BODY-frame predicted target following

SYS-02 accepts `optical_body_following_samples[]` as an alternative to
externally measured boresight samples. Each exclusive head maintains
predicted BODY-frame pointing and a held target. At each sample Rust
advances the head toward the previous target using a rate- and optional
acceleration-limited rest-to-rest travel budget, latches the new target,
and evaluates tracking error. Failed samples interrupt PAT; recovery
requires new slew, settling and acquisition and cannot override an
administrative outage. The synthetic two-contact fixture checks lag,
recovery, deterministic traces and packet conservation.

**Fidelity:** sampled zero-velocity-at-boundary approximation, not a
continuous servo or calibrated motor model. Predicted mode currently
disallows shared-head command streams. PAT now consumes immutable projected
head-state snapshots and held targets at admitted contact and fault-recovery
boundaries, avoiding stale contact-start slew assumptions. PAT reservations
and follower integration remain separate; no single mutable servo owner is
claimed. See
[design](design/optical-body-frame-target-following.md).

## Sampled BODY-frame tracking target angular-rate feasibility

SYS-02 now checks consecutive commanded target directions from
`optical_body_tracking_samples[]` against each exclusive head's existing
maximum slew rate. Even perfectly aligned externally observed boresight
cannot mask an unreachable sampled target change. A failed target-rate
sample interrupts PAT and packet forwarding; a later feasible sample
requires reacquisition and does not override an administrative outage.
Synthetic Rust tests and packet CI cover a 90-degree / 25 ms target step,
recovery, independent faults, deterministic traces and conservation.

**Fidelity:** finite-sample mean target-rate gate only; the observed
boresight remains external. No continuous servo, acceleration-constrained
target follower, optical PHY or physical calibration is claimed. See
[design](design/optical-body-frame-tracking-error.md).

## Optional sampled BODY-frame optical tracking error

SYS-02/03 now supports `optical_body_tracking_samples[]` for external
per-head BODY-frame target and actual boresight observations. Rust computes
reciprocal angular tracking errors and gates PAT at sample instants.
The contract requires gimbal actuators, consistent per-head limits and
sampled targets matching each geometric contact's slew target. A pointing
loss interrupts Tracking and releases heads; an administrative fault cannot
independently reopen the link. The synthetic fixture verifies loss at 30 ms,
recovery at 90 ms, reacquisition at 110 ms, second-contact tracking at
190 ms, deterministic packet traces and conservation. See
[design](design/optical-body-frame-tracking-error.md).

**Fidelity:** externally supplied finite samples held between observations,
not a closed-loop servo, inferred gimbal boresight or continuous pointing
bound. No physical calibration or constellation-scale benchmark is claimed.

## Optional post-slew optical-head settling dwell

SYS-02/03 now supports an opt-in `settling_ns` per exclusive body-frame
optical-head actuator. After a nonzero retarget, Rust waits for the
acceleration-limited or constant-rate travel plus that head's settling
dwell, before beginning PAT acquisition. The slower of two independent
heads and the existing fixed slew floor determine the delay. Zero-angle
targets and omitted dwell retain legacy timing; rejected contacts do not
change head pointing. A synthetic packet-runtime CI scenario verifies
20 ms initial tracking, 190 ms reacquisition after a 90-degree retarget,
suppressed forwarding before acquisition, deterministic traces and packet
conservation. Rust contract/planner regressions cover input validation,
overflow, asymmetric heads and rejected retargets.

**Fidelity:** operator-supplied fixed stabilization dwell, not continuous
tracking error, measured servo response, calibrated laser PHY or a physical
settling guarantee. See [design](design/optical-body-frame-gimbal-slew.md).

## Optional acceleration-limited body-frame optical-head slew

SYS-02/03 now accepts an optional positive per-head
`max_slew_acceleration_deg_per_s2` alongside existing constant-rate
gimbal configuration. The deterministic PAT planner computes a
rest-to-rest triangular or trapezoidal slew profile, rounds travel up to
virtual nanoseconds and delays acquisition until both exclusive heads
finish. Omitted acceleration caps retain legacy constant-rate behavior.
Contract and planner regressions plus a synthetic packet-runtime CI
scenario verify 90-degree retargeting (45 ms travel, 175 ms second
tracking), contact rejection and packet conservation.

**Fidelity:** idealized acceleration-bounded kinematics, not motor torque,
feedback, continuous target tracking, physical calibration or optical PHY.
See [design](design/optical-body-frame-gimbal-slew.md).

## Optional body-frame optical-head slew actuator scheduling

SYS-02/03 now supports opt-in `optical_head_slew_actuators[]` and
`optical_pat_slew_targets[]` for every exclusive PAT head and geometric
contact. The Rust planner computes per-head shortest-arc body-frame target
travel at a configured constant angular rate, waits for shared-head
availability, and uses the greater of dynamic travel and existing fixed
slew dwell before acquisition. Fault-split segments inherit the contact
target; rejected contacts do not move actuator state. The synthetic
packet scenario demonstrates a 90-degree retarget delaying the second
contact's tracking to 160 ms, with suppressed pre-acquisition forwarding
and deterministic packet accounting.

**Fidelity:** supplied constant body-frame targets and a zero-ramp
kinematic travel-time assumption, not continuous attitude dynamics,
acceleration-limited motor control, tracking-error feedback or optical PHY.
See [design](design/optical-body-frame-gimbal-slew.md).

## Optional Rust-native sampled ITRS LOS angular-acceleration gate

SYS-02 now accepts independent `optical_tracking_accelerations[]` limits
for native `optical_geometry_samples[]`. Rust compares consecutive signed
ITRS LOS angular-velocity vectors over the midpoint-to-midpoint interval,
detecting direction reversals as well as velocity steps. The first two
observations establish a baseline; ambiguous antiparallel axes fail closed.
The independent sampled-pointing state controls SYS-03 tracking loss,
exclusive-head release, reacquisition and packet forwarding while
administrative faults remain independent. Contract, numerical and packet
regressions and a CI scenario cover threshold failure, overlapping outages,
recovery, deterministic replay and packet conservation.

**Fidelity:** this is a finite-sample ITRS motion proxy, not body-frame
actuator dynamics, continuous pointing feasibility or optical PHY. See
[design](design/optical-itrs-los-acceleration-gate.md).

## Optional sampled spacecraft-body LOS angular-acceleration audit

The SYS-01/02 native ITRS compiler now accepts an opt-in, finite nonnegative
`--native-itrs-max-body-los-accel-deg-per-s2` threshold. It derives sampled
angular-velocity vectors from successive body-frame LOS directions and
finite-differences them over the time between adjacent interval midpoints.
The signed vector formulation detects direction reversals even when scalar
angular speeds match. The first two observations establish the velocity
baseline; antiparallel rotation axes are ambiguous and fail closed.
The SHA-256-bound manifest records per-head peak sampled acceleration.
Python regression tests cover bounds, reversals, provenance and atomic
failure behavior. The rate cap remains independently optional.

**Fidelity:** a finite-sample angular-acceleration proxy only, not a physical
gimbal servo, motor torque model, tracking error, continuous motion guarantee
or runtime optical loss mechanism. See
[design](design/optical-native-itrs-compiler.md).

## Optional spacecraft-body LOS slew-rate audit

The SGP4-to-native-ITRS compiler can now reject supplied orbit/attitude
observations whose sampled line-of-sight direction moves too rapidly in
either endpoint's spacecraft body frame. The opt-in global degrees-per-second
threshold uses inverse body-to-ITRS quaternion rotation, a stable atan2
angular displacement and elapsed virtual seconds. Its SHA-256-bound manifest
records each head's peak sampled mean rate. Tests cover static-body motion,
co-rotating attitude, invalid limits, deterministic provenance and failed
artifact replacement. The existing Rust PAT/packet path is unchanged.

**Fidelity:** finite sampled body-frame LOS rate only, not actuator dynamics,
acceleration, continuous tracking or an online PAT loss mechanism. See
[design](design/optical-native-itrs-compiler.md).

## Sampled ITRS LOS angular-rate gate

The optional `optical_tracking_rates[]` scenario contract applies per-head
finite, nonnegative degrees-per-second bounds to consecutive native
`optical_geometry_samples[]`. Rust evaluates the sampled ITRS LOS displacement
and holds the resulting availability state. Exceeding a limit interrupts PAT
tracking, releases both heads and requires reacquisition after effective
administrative/pointing recovery. Existing scenarios remain compatible when
the optional configuration is absent. Synthetic Rust contract, geometry and
packet-runtime tests and CI verify a 90-degree LOS change, overlapping
administrative outage, control events and packet accounting.

**Fidelity:** average sampled ITRS LOS angular rate only, not spacecraft
body-frame gimbal slew, acceleration, tracking servo dynamics or optical PHY.
Finite samples cannot guarantee continuous pointing feasibility; no
constellation-scale performance measurement is claimed. See
[design](design/optical-itrs-los-rate-gate.md).

## Production SGP4 and attitude to Rust-native ITRS geometry

The optional `--native-itrs-probes-per-contact` mode now compiles real
Skyfield/SGP4 ITRS satellite positions and fixed or sampled body-to-ITRS
optical head attitudes into `optical_geometry_samples[]`. The Rust SYS-02
geometry evaluator, rather than Python, computes reciprocal off-axis angles
and feeds SYS-03 PAT tracking loss/reacquisition. The deterministic
microsecond probe grid is shared with the earlier native-angle compiler.
It includes a t=0 observation per PAT link, contact start, interior and
end probes, and a global 10,000-observation budget. Administrative faults
remain independent. SHA-256 source/output digests, attitude-rate checks and
per-contact counts are recorded in the manifest.

Python contract tests cover fixed/sampled attitude, deterministic sampling,
degenerate geometry, invalid mode composition, provenance and failed-write
safety. CI exercises a synthetic geometry-driven PAT/packet case and a
separate real SGP4 compiler smoke. See [design](design/optical-native-itrs-compiler.md).

**Fidelity:** this is offline discrete ITRS observation compilation, not
Rust-native orbital propagation, continuous attitude dynamics, calibrated
optical PHY, or a proof of feasibility between sample instants. No
large-scale throughput or memory benchmark is claimed.

## Rust-native time-indexed ITRS optical pointing observations

SYS-02 now accepts opt-in `optical_geometry_samples[]` with frame-tagged
ITRS Cartesian satellite positions (meters) and optical boresight vectors
(dimensionless), sampled in scenario-relative nanoseconds. Rust evaluates
reciprocal line-of-sight steering feasibility at each supplied timestamp and
holds that boolean state until the next sample on the same link. SYS-03
consumes the resulting typed sampled-pointing state transitions, independently
of administrative faults, releasing exclusive optical heads on loss and
requiring fresh PAT acquisition after effective recovery. The packet runtime
and delayed control-plane routing consume the resulting acquired windows.

Validation requires one initial t=0 observation per PAT link, strictly
increasing in-horizon timestamps, finite and nondegenerate ITRS geometry,
consistent per-head limits, and at most 10,000 observations. It rejects
mixing other pointing modes or preplanned sampled-pointing faults. The
`scenarios/optical-native-itrs-samples.json` fixture and CI acceptance
cover pointing loss, overlapping administrative outages, reacquisition,
deterministic packet traces and packet conservation.

**Fidelity:** these are operator-supplied discrete ITRS observations, not
Rust-native SGP4 propagation, body-to-ITRS attitude dynamics, continuous
line-of-sight certification or calibrated optical PHY. Existing angle-sample
and fixed-contact geometry modes remain backward compatible. The sample
budget is bounded; no constellation-scale benchmark is claimed.

# Implementation status and current fidelity

## Optional sampled body-attitude angular-rate guard

SYS-02 sampled body-to-ITRS head attitude inputs now accept an opt-in,
nonnegative `max_body_attitude_rate_deg_per_s` per head. The offline adapter
rejects adjacent shortest-arc quaternion SLERP segments whose implied
constant angular rate exceeds the configured limit, including when compiling
SGP4 observations for Rust-native optical steering. The SHA-256-bound
manifest reports per-head peak modeled angular rate and the optional cap.
Python contract tests cover boundaries, quaternion hemisphere equivalence,
tiny rotations, invalid limits, deterministic output and failed-compile
atomicity. Existing Rust SGP4-to-PAT acceptance remains enabled.

This is a **modeled body-attitude input guard**, not an actuator or
continuous dynamics model. It does not limit optical head gimbal motion,
change PAT acquisition time or guarantee physical tracking between samples.

## SGP4/ITRS and sampled attitude to Rust-native steering

The optional `--native-steering-probes-per-contact` mode compiles real
Skyfield/SGP4 ITRS positions and time-indexed body-to-ITRS head attitudes
into bounded `optical_steering_samples[]` for the Rust PAT planner, rather
than emitting `optical_geometry[]` start snapshots or preplanned pointing
fault edges. It preserves independent administrative link faults, evaluates
every PAT link at t=0 and at each contact's finite interior/end probes,
and records exact SHA-256 provenance. Python contract tests and a synthetic
ITRS-to-Rust PAT/packet CI acceptance cover pointing loss, overlapping
faults, reacquisition and packet accounting. This is **discrete sample-and-hold
steering**; neither continuous attitude propagation nor laser PHY is
implemented. See [adapter design](design/optical-native-steering-compiler.md).

## Runtime-evaluated optical steering observations

The opt-in `optical_steering_samples[]` contract accepts strictly ordered
scenario-relative steering angles for every PAT link, beginning at zero.
Rust evaluates the two endpoint steering limits at each sample timestamp,
holds the resulting feasibility state until the next sample, and combines
it with independent administrative link faults before SYS-03 acquisition.
A link cannot reacquire while either domain remains down. The new
`optical-native-steering-samples.json` acceptance scenario checks tracking
loss, overlapping failures, recovery, control events and packet accounting.
Legacy `faults[]` inputs remain supported but cannot mix
`sampled_pointing` fault edges with native steering samples.

This is deterministic virtual-time **sample-and-hold** evaluation of supplied
angles, not a continuous attitude/SGP4 solver, real optical PHY, or a proof
of feasibility between observations. Already-admitted packets still complete
under the existing L1 departure convention.

## Overlapping optical PAT fault domains

The Rust PAT preparation stage now combines administrative and sampled
pointing faults independently. A recovery edge cannot restore a link
while the other fault domain is still down. Only combined-state
transitions reach the packet runtime; PAT loss events preserve the
initiating cause, and simultaneous failures use administrative precedence.
Rust contract tests and the `optical-pat-overlapping-faults.json`
CI scenario verify reacquisition, control-event causality, and packet
accounting. This is deterministic virtual-time fault scheduling, not a
native continuous attitude or physical optical tracking model.

## Operational P0 and P1 orbital-contact bridge

The existing P0 Rust packet simulator executes a deterministic packet journey across user RF, satellite optical ISL, feeder station, terrestrial backhaul and internet endpoint. The new **P1 orbital bridge** provides offline TLE/SGP4 propagation, Skyfield TEME-to-ITRS transformation, WGS84 elevation and Earth-occlusion geometry, sampled contact windows with optional bisection-refined boundaries and geometric propagation delay, plus optional per-contact analytical range-rate/Doppler observations in a separate sidecar.

**Implemented in code:** contact windows with half-open `[start_ns,end_ns)` bounds and optional midpoint geometric delay; `contact_mode: "scheduled"` prevents space links without visibility from being treated as always available; Rust contact transitions drive link state and delayed route recomputation. The same packet engine reports delivered and dropped packets as contacts open/close. Python runs offline and writes a versioned JSON scenario plus a provenance sidecar.

**Existing P0:** typed nodes/links/flows/faults, deterministic integer-nanosecond event scheduler, shortest-route FIB with controller delay, packet queues, propagation/serialization/processing delay, packet traces, JSON report and CLI.

### Provenance and accuracy

- Python dependency pins: `skyfield==1.55`, `sgp4==2.27`; zero network downloads during propagation (`timescale(builtin=True)`).
- Orbital input TLE and absolute historical `epoch_utc` are explicitly recorded and checked for freshness (configurable maximum 14 days from TLE epoch).
- Skyfield converts SGP4 TEME output to ITRS Earth-fixed coordinates before WGS84 elevation/occlusion calculations. Published 2014 ISS GCRS, WGS84 subpoint, topocentric altitude and range observables are now regression-tested with declared tolerances; this is not an independent EOP/ITRF transform reference (see [frame validation](design/orbital-frame-validation.md)).
- Contacts use a coarse visibility scan; detected rise/set transitions now conservatively round into visibility even without bisection (coarse sets use the previous visible sample), and optional bisection refines those bounds to a configured tolerance. Coarse sets may discard up to one sampling interval of valid visibility. Two transitions within one sample interval can still be missed. The midpoint delay remains a **fixed route-planning weight**; optionally the Python generator emits bounded per-contact ECEF geometric delay samples and Rust interpolates delay **at each packet's transmission completion** using integer nanosecond arithmetic. With no samples, legacy midpoint behavior remains unchanged. This is a geometric approximation, not retarded-time/light-time iteration or a PHY group-delay model. See [delay design](design/time-varying-propagation.md).
- Optional per-link carrier reference frequencies produce midpoint line-of-sight range-rate and first-order one-way Doppler estimates in a separate JSON sidecar. Positive range rate means recession and negative received frequency shift. This does not affect Rust packet forwarding or model RF carrier synchronization.
- A geometric contact is a **candidate**, not proof of beam scheduling, modem synchronization, optical PAT acquisition or resource admission.
- Packet transmissions already admitted before a contact closes finish under the P0 `complete_after_departure` convention. Physical interruption and retransmission models remain future work.
- Linux IP, real PHY/MAC, BER, interference, handover signaling and satellite control remain unimplemented.

### Reproducible scenarios

| Scenario | Input | Verified behavior |
|---|---|---|
| `scenarios/minimal.json` | configured static links | 3 packets delivered over five hops |
| `scenarios/isl-failure.json` | configured link failure | reroute through backup ISL after control delay |
| `scenarios/visibility-handover.json` | synthetic, time-varying RF/ISL contact windows | deterministic contact opens/closes, temporary outage and alternative access route |
| `scenarios/orbital-pass.json` | historic public ISS TLE from Skyfield guide, 2014 epoch | Python generates real geometric contacts, Rust simulates internet delivery only within visibility intervals |

Read [historical pass instructions](../scenarios/ORBITAL-README.md) for the end-to-end command. This demo uses the ISS and a co-located laboratory ground site for reproducibility; it **does not** represent an operational Starlink constellation.

## Bounded offline SYS-01 ephemeris snapshots

The `ntarlink_orbital.ephemeris` CLI exports sorted, frame-tagged ITRS
satellite position snapshots on a bounded, half-open UTC time grid. It
reuses the existing pinned SGP4/Skyfield position provider and writes an
independent JSON artifact with exact input/output SHA-256 provenance and
dependency versions. A global 500,000 satellite-time-pair cap and explicit
microsecond-resolution time grid prevent unbounded work and false
nanosecond precision. Unit tests cover deterministic ordering, time
boundaries, invalid IDs, malformed positions, resource limits and digests;
CI generates a real historical ISS ephemeris.

This offline analytical export does **not** modify the Rust packet runtime,
contact windows or RF budgets. It has no interpolation accuracy bound,
velocity output, EOP calibration or constellation-scale benchmark.
See [ephemeris artifact design](design/orbital-ephemeris-artifacts.md).

## Offline SYS-01 ephemeris fixed interior-probe interpolation audit

The optional `ntarlink_orbital.ephemeris_audit` command validates a pinned
ITRS snapshot against the same SGP4/Skyfield provider at 1, 3 or 7
evenly spaced interior probes per complete adjacent sample pair. It reports
midpoint and all-probe per-satellite/global maximum and RMS position errors
in meters, and fails its CLI gate when the selected probes exceed the
configured threshold. The audit verifies the exporter manifest's exact
scenario/orbit/artifact SHA-256 provenance and rejects malformed time grids,
coordinates and satellite catalogs before propagation. Historical ISS
three-probe checks run in CI. This is a **same-provider, finite-probe**
self-consistency check; it neither proves continuous interpolation accuracy
nor independently validates orbit truth or EOP. See
[ephemeris audit design](design/orbital-ephemeris-audit.md).

## Bounded SYS-01 JSONL streaming ephemeris

The optional `ntarlink_orbital.ephemeris_stream` CLI emits a versioned,
frame-tagged, satellite-major JSONL artifact without retaining all position
samples in memory. It shares the original exporter's UTC grid validation,
500,000 satellite-time-pair limit and pinned SGP4/EOP provider. Each stream
has a header plus one record per sample, a 128 MB byte limit, atomic
replacement after successful validation, and a companion SHA-256 provenance
manifest. Python CI exercises a real historical ISS stream, deterministic
ordering, failure cleanup and resource limits. This does not replace the
original nested JSON exporter or its interpolation audit, and is neither
columnar output nor a measured constellation-scale benchmark. A separate
`ntarlink_orbital.ephemeris_reader` validates the full JSONL schema, ordering,
UTC grid, finite positions, byte count and SHA-256 manifest before yielding
records. It uses a bounded temporary disk snapshot to prevent source changes
between validation and consumption; this is not authenticated provenance,
a live feed or a measured throughput benchmark. See
[streaming design](design/orbital-ephemeris-stream.md).

## Synthetic SYS-01 ephemeris scale baseline

The optional `ntarlink_orbital.ephemeris_benchmark` command creates a
bounded, deterministic synthetic satellite/time grid, exports JSONL using
the production streaming writer and validates every record through the
production snapshotting reader. It reports artifact bytes/digest, measured
wall-clock throughput and peak traced Python heap for each stage. CI checks
1,000 and 10,000 position pairs without timing gates. This is a synthetic
serialization and verified-read baseline, not a real constellation, SGP4
propagation benchmark, process-RSS measurement or performance guarantee.
See [benchmark protocol](design/orbital-ephemeris-benchmark.md).

## Production-provider SGP4/ITRS compute benchmark

The opt-in `ntarlink_orbital.sgp4_scale_benchmark` invokes the real
Skyfield/SGP4 ITRS position provider on a bounded UTC grid, using replicated
historical TLE records under distinct IDs. It reports provider-build time,
position throughput, sample count and a deterministic position digest.
Python unit tests cover failure handling and CI exercises a small historical
ISS case. This is a computation baseline, not a physically diverse
constellation, multi-epoch orbit validation, or a 100k/500k performance
measurement. See [benchmark scope](design/orbital-sgp4-scale-benchmark.md).

## Isolated production SGP4/ITRS benchmark repetitions

The opt-in `ntarlink_orbital.sgp4_scale_benchmark_suite` runs the existing
replicated historical-TLE SGP4/ITRS workload in fresh Python subprocesses.
It checks position and input digests across runs, reports median build and
propagation timing/throughput, and records whole-process peak RSS in bytes
on Linux/macOS. A small historical ISS subprocess smoke is exercised by
Python CI. Large 100k/500k workloads remain manual and unmeasured here.
The RSS high-water mark is not a per-stage measurement; replicated TLEs
do not represent a physical constellation. See
[SGP4 benchmark protocol](design/orbital-sgp4-scale-benchmark.md).

## Distinct supplied-TLE SGP4 benchmark mode

The production single-process and isolated SGP4 benchmark commands now
optionally accept `--catalog-mode unique`. This requires exactly one distinct
supplied TLE pair per requested satellite and refuses silent replication or
duplicate source pairs. Reports identify the catalog mode and distinct source
count. Contract tests check invalid catalogs, source mapping, and isolated
digest/cardinality consistency; CI exercises the real Skyfield provider
with a single historical ISS TLE in this mode. No independent multi-orbit
catalog has yet been sourced or validated, and the 100k/500k runs remain
unmeasured. See [SGP4 benchmark protocol](design/orbital-sgp4-scale-benchmark.md).

## Offline TLE catalog intake and provenance

The optional `ntarlink_orbital.tle_catalog` CLI imports strict local ASCII
TLE pairs, verifies both mod-10 checksums, matching and unique NORAD IDs,
valid source epochs and configured freshness, and writes an existing-compatible
orbit configuration with a SHA-256 manifest. Caller-supplied HTTPS source,
license/permission descriptor and retrieval timestamp are recorded but are
**not independently authenticated**. Tests cover malformed input, bounded
resource use, output digests, the distinct-TLE benchmark handoff and a real
historical ISS SGP4/ITRS smoke. No multi-orbit licensed catalog is bundled,
and no large-scale measurements are claimed. See
[catalog intake design](design/orbital-tle-catalog-intake.md).

## SGP4 phase-boundary resident-memory observations

The production SGP4/ITRS benchmark now records three Linux current-RSS
checkpoints (before provider build, after provider build, and after position
propagation). The isolated suite validates the checkpoint contract across
runs and aggregates their maximum observed values. These are snapshots,
not per-stage memory peaks or allocations. Whole-process high-water RSS
remains separately reported. Unsupported platforms return null; the small
real-provider CI smoke checks the Linux values. No 100k/500k workload has
been measured or performance-certified by this change. See
[benchmark protocol](design/orbital-sgp4-scale-benchmark.md).

## Opt-in production SYS-01 Earth-orientation input

The SGP4 provider can now load a local, SHA-256-pinned IERS finals2000A
file into a Skyfield timescale for both contact prediction and ITRS
ephemeris export. Daily UT1−UTC and xp/yp polar motion are interpolated
with strict source coverage and an explicit operator-supplied expiry;
leap seconds still use Skyfield 1.55's bundled schedule. Both output
manifests record EOP source provenance. The default remains the built-in
timescale, and no IERS file is bundled or fetched. Synthetic EOP fixtures
exercise the production adapter and Vallado single-epoch reference.
The opt-in loader now checks both daily IERS observation/prediction flags,
rejects forecast rows in the interpolation window by default and requires
explicit operator opt-in to use predicted EOP. Quality-policy counts are
recorded in the frame-input manifest; this does not authenticate the source.
EOP coverage, expiry and forecast-quality checks conservatively round the
last included simulation nanosecond upward to Skyfield's microsecond
resolution, preventing submicrosecond end-of-coverage leakage.
This is not independent multi-epoch calibration, authenticated IERS data,
or validation of future leap seconds. See [EOP design](design/earth-orientation-data.md).

## First SYS-05 analytical RF budget

A separate `systems/access/python` package now consumes the compiled
SYS-01 scheduled-contact artifact, using each directed RF contact's midpoint
`propagation_delay_ns` to recover an approximate geometric range. It
calculates free-space path loss, received power, 290 K thermal noise, C/N0,
C/N and margin against an explicitly configured reference threshold.
The JSON output includes input SHA-256 provenance and per-contact samples.
An optional per-directed-link received aggregate in-band interference
power (dBm) now yields analytical C/(N+I), noise-plus-interference power
and a separately configured SINR reference margin. This interference is
**supplied**, not calculated from beams, channels, reuse or pointing.
The original Rust link capacity and packet decisions are unchanged.
This is **not** an RF waveform, physically predicted interference, beam
allocation or radio bearer acquisition; unconfigured losses remain absent.
See [access subsystem instructions](../systems/access/README.md).

## Time-resolved SYS-05 RF observations

An optional separate `ntarlink_access.rf_time_series` batch command now
evaluates the existing analytical Friis/C/N/SINR equations at every validated
SYS-01 geometric propagation-delay sample. The midpoint RF sidecar and
the Rust data plane remain unchanged. The output carries contact and
scenario-relative sample timestamps, directed link IDs, source delays and
SHA-256 input provenance. Incomplete or malformed RF contact series fail
closed, and the 500,000-sample bound is enforced. This is **discrete sampled
analysis**, not a live RF waveform, continuously interpolated SINR, physical
interference prediction, admission or MAC scheduling.

## Standalone SYS-05 beam-frame resource scheduler

The Rust `systems/access/rust` crate now models a finite-capacity,
single-direction beam-frame allocator using deterministic weighted deficit
round robin. Per-terminal packet queues consume a shared bounded wire-byte
budget, and frame grants account for fixed per-packet overhead. Unit tests
exercise two saturated terminals at 3:1 weight, per-frame byte conservation,
small-frame alternation, deferred large-packet service, invalid input and
stable terminal ordering. The crate is compiled and tested by the existing Rust workspace CI.
An **opt-in** shared-beam bridge now uses this allocator to issue uplink packet
grants; links without an access beam still use independent per-link capacity.
There is no PHY admission, beam steering, RF-derived capacity, subscriber
authorization or complete handover signaling.

## Optional SYS-05 shared-beam packet grant bridge

The opt-in `access_beams[]` scenario contract now connects SYS-05's
deterministic weighted beam-frame scheduler to Rust packet forwarding for
multiple terminal-to-satellite uplinks sharing one finite directional
resource. Packet grants serialize within a common frame budget and account
for framing overhead. Contact loss or link failure cancels ungranted
bearer packets, with bounded shared queue and deterministic drop reasons.
Unconfigured links retain their legacy independent capacity behavior.
See [beam-frame integration design](design/beam-frame-packet-bridge.md)
and `scenarios/beam-uplink-contention.json` for the cross-subsystem fixture.

This remains a **modeled L1 MAC** without RF-derived capacity, bearer
authorization, modem acquisition, downlink scheduling or handover signaling.

## Official IERS excerpt regression

Small fixed-width excerpts transcribed from the published IERS finals2000A rolling file (2004, 2014 and 2026) are pinned as test fixtures. Production EOP parsing and two-epoch independent GMST82/IAU-1980 transform agreement are exercised, along with forecast rejection/opt-in. See [source and limitations](design/iers-observed-excerpts.md). These are not an authenticated complete IERS archive or independently published multi-epoch ITRF vectors.

## Production frame cross-check at multiple instants

The production SGP4/Skyfield ITRS provider is regression-tested at three
2014-01-23 UTC instants against raw SGP4 TEME positions transformed by a
separate test-only Vallado GMST82/IAU-1980 implementation. It consumes
the pinned official 2014 IERS excerpt and checks SHA-256 provenance,
observed EOP policy, three-dimensional position agreement within 0.5 m
and negative controls for omitted UT1 or polar motion. The comparison
tests frame wiring, not independent orbital truth. It does not supply
externally published ITRF vectors for several years, a complete certified
IERS archive or independently maintained leap-second tables. See
[frame validation](design/orbital-frame-validation.md).

## Verified offline TLE benchmark consumption

The isolated SYS-01 SGP4 benchmark accepts an optional intake manifest in
unique-TLE mode. It checks the exact configuration SHA-256, manifest schema,
catalog cardinality, NORAD identity, TLE checksums and epoch freshness before
spawning workers; each worker verifies again before propagation and the suite
compares the manifest digest across repetitions. Unit tests cover tampered
artifacts, malformed metadata, duplicate JSON keys, worker disagreement and
a small historical ISS production-provider path. This is a local integrity
boundary, **not** authenticated publisher provenance, a licensed multi-orbit
catalog or 100k/500k measured performance.

## Standalone SYS-03 optical PAT acquisition planner

The Rust `ntarlink-optical-pat` crate now provides deterministic, bounded
virtual-time acquisition planning for SYS-01 optical contact candidates.
It enforces exclusive optical-head reservations, configured slew and
acquisition dwell, half-open tracking windows and explicit rejection when
contact expires before acquisition. Stable event ordering and input
validation are covered by unit tests, including resource conflicts,
reacquisition at contact boundaries and timestamp overflow.

## Opt-in SYS-03 PAT packet-forwarding gate

A scenario can declare `contact_mode: "scheduled"` and `optical_pat[]` to
bind an optical ISL to two exclusive optical-head resources and explicit
slew/acquisition dwell. The packet runtime invokes the SYS-03 planner over
SYS-01 geometric contacts and substitutes **only acquired tracking windows**
for configured optical links. Unacquired links never enter the forwarding
availability graph. Each acquired opening/closing triggers the existing
control-delay route recomputation, and the JSON report exposes planned
`pat_events` separately from actual packet/control events. The sample
`scenarios/optical-pat-acquisition.json` tests outage, reacquisition and
packet accounting in CI. Sampled geometric delays are reanchored at the
tracking start without changing the original interpolation model.

This is an **opt-in deterministic offline gate**, not a live optical bearer,
pointing-angle/laser budget, physical acquisition measurement, or tracking
failure/recovery. Planned PAT state does not react to runtime administrative
faults; the existing link-fault gate still prevents forwarding. Packets
admitted before contact expiry finish under the existing P0 completion policy.
Unconfigured scenarios retain legacy behavior. See
[SYS-03 crate documentation](../systems/space-network/README.md).

## SYS-03 optical PAT reacquisition after link faults

For PAT-enabled directed optical ISLs, the existing scenario `faults[]`
administrative down/up events now partition each geometric visibility contact
into half-open availability intervals before offline PAT planning. A down event
ends any tracking interval and releases both optical-head reservations; an up
event creates a new acquisition candidate that must complete the configured
slew and acquisition dwell before packets can enter the link. Other optical
links may acquire a released head during the outage. At equal timestamps,
faults retain the packet scheduler's original scenario ordering.

The packet runtime still executes the original administrative fault events,
so the resulting acquired windows and the link administrative gate must both
permit forwarding. The report distinguishes planned PAT loss with reason
`administrative_link_down` from actual `link_down` and `contact_close`
control events. Route updates continue to use the configured controller delay.
Split contacts reanchor both endpoints of any sampled geometric delay series.
The reproducible `scenarios/optical-pat-link-fault.json` fixture and Rust
contract tests cover reacquisition, forwarding suppression, delay samples and
deterministic packet accounting.

This is deterministic **preplanned fault handling**, not detection of
unmodeled tracking degradation, autonomous recovery policy, optical power
or pointing physics. The P0 completion-after-departure policy still applies
to packets already admitted when a fault occurs. PAT-disabled links retain
their existing administrative fault behavior.

## SYS-02 optical-head availability gating

The new `ntarlink-spacecraft` Rust crate indexes per-head half-open outage
intervals, supplied as optional `optical_head_outages[]` in a scheduled PAT
scenario. Validation rejects unknown spacecraft-owned PAT heads, invalid or
overlapping intervals, and excessive input. Before SYS-03 acquisition, the
packet runtime intersects both endpoint heads' availability with geometric
and administrative-up windows. A head outage releases tracking and requires
fresh acquisition after recovery, even while geometric visibility persists.
PAT loss is labeled `optical_head_unavailable`; delayed route convergence,
packet gating, deterministic reports and sampled-delay clipping are preserved.

`scenarios/optical-head-outage.json` provides a cross-subsystem test with
a 50-70 ms head outage, 90 ms reacquisition, and forwarding suppression
during the outage. The model is **preplanned availability**, not a pointing
envelope, spacecraft power/thermal budget, measured head health or a laser
PHY. Existing in-flight packet completion semantics remain unchanged.

## SYS-02 static optical pointing-envelope gate

An optional `optical_pointing[]` contract supplies fixed per-link endpoint
off-boresight angles and inclusive per-head steering limits in degrees.
All PAT links must be covered when enabled; malformed angles, duplicate
links, inconsistent shared-head limits and missing PAT references are
rejected. The SYS-02 spacecraft model gates SYS-03 acquisition before
head reservations, emitting `pointing_outside_envelope` for rejected
geometric contacts. Packet forwarding remains unavailable for rejected
links; existing unconfigured scenarios retain their prior behavior.
Rust contract, spacecraft and packet-runtime tests plus a deterministic
CLI fixture exercise boundary acceptance and rejection. **The angles are
operator-supplied constants**, not calculated from propagated position,
attitude, boresight or terminal motion. No time-varying pointing, calibration
or optical PHY is claimed.

## SYS-02 ITRS optical pointing snapshots

The opt-in `optical_geometry[]` scenario contract now accepts one
frame-tagged ITRS position/boresight snapshot per directed PAT contact.
SYS-02 calculates both reciprocal line-of-sight steering angles from
Cartesian positions in meters and boresight directions in the same frame.
Malformed, incomplete or inconsistent geometry fails validation, while
out-of-envelope contacts are rejected before SYS-03 acquisition. PAT
rejection events use `geometry_outside_envelope`; the packet runtime
removes unacquired windows and preserves delayed route recomputation.
A synthetic two-window CLI fixture exercises one accepted and one rejected
contact with packet accounting. This is **not** a production ephemeris
adapter or an attitude solution: the supplied vectors are fixed over each
window and are not guaranteed physically valid at later times.

## Offline SYS-01 SGP4 to SYS-02 fixed-attitude optical adapter

The optional Python `ntarlink_orbital.optical_geometry` command samples
the production SGP4/Skyfield ITRS position provider at each optical PAT
contact start, rotates explicitly supplied body-frame optical boresights
using fixed body-to-ITRS unit quaternions, and emits the existing
`optical_geometry[]` contract. A companion SHA-256 manifest records
the scenario, orbital source, attitude input and output digests. Contract
tests cover frame, quaternion, head ownership, timestamps, malformed
geometry and reproducibility. A synthetic perturbed-ISS two-orbit fixture
exercises the actual Python-to-Rust PAT packet path in CI.

An optional 1/3/7-interior-probe admission audit now samples additional
ITRS positions through each half-open PAT contact, including its final
representable microsecond. It rejects any sampled steering-envelope
violation before writing artifacts, and records per-contact sample counts,
maximum angles and worst margin in the SHA-256-linked manifest. The legacy
mode remains start-only. Python tests exercise interior and trailing-probe
drift, short windows, determinism and fail-before-write behavior; CI
exercises the real synthetic two-orbit provider with the audit enabled.
This is an **offline finite-probe fail-closed gate**, not continuous tracking:
between-probe drift may still be missed, and the runtime continues to use
one start snapshot per contact. At seven interior probes, 10,000 contacts
can add up to 160,000 provider evaluations; no throughput claim is made.

This **does not** infer or propagate spacecraft attitude, verify the
synthetic orbit against independent reference data, recalculate geometric
contact visibility, or model continuous pointing drift during the contact. See
[adapter design](design/orbital-optical-attitude-adapter.md).

### Time-indexed optical-head attitude input

The offline adapter now accepts an opt-in
`attitude_model: "sampled_body_to_itrs_quaternion"` with ordered,
microsecond-aligned body-to-ITRS quaternion samples per optical head.
It uses shortest-arc spherical interpolation, including sign-equivalent
quaternions, to calculate contact-start boresights and (when requested)
recalculate both boresights at every finite steering probe. Samples must
cover the virtual-time grid and are limited to 100,000 across all heads.
The legacy fixed-attitude input remains supported. The manifest identifies
the attitude model and retains exact input and output digests.

Unit tests exercise interpolation, sign equivalence, invalid coverage,
interior attitude excursions and unchanged outputs on failed compilation.
A synthetic time-indexed fixture is run through real SGP4, Rust validation,
PAT acquisition and packet forwarding in CI. These are **operator-supplied
attitude samples**, not spacecraft attitude propagation or independent
attitude truth. A passing finite-probe audit cannot guarantee between-probe
pointing; Rust still uses one snapshot per contact and does not close
tracking mid-contact when the attitude drifts.

## Sampled SYS-02 pointing loss into SYS-03 PAT reacquisition

The opt-in `ntarlink_orbital.optical_geometry
--runtime-pointing-probes-per-contact` mode converts finite sampled
ITRS steering-envelope failures and recoveries into existing link
`faults[]` down/up transitions. It requires operator-supplied
time-indexed attitude, a passing contact-start snapshot, and no
preexisting faults on the same PAT link. It preserves other faults,
caps generated events at 10,000, and records event provenance in the
SHA-256-linked manifest. The Rust PAT planner already releases both
optical heads, interrupts tracking and requires fresh acquisition
after a link recovers; the packet engine uses its existing delayed
route convergence and packet accounting.

Python unit tests cover deterministic edges, failure at contact start,
fault conflicts, half-open boundaries, manifest digests and unchanged
output on invalid compilation. CI uses a synthetic aligned ITRS
position provider with an explicit 90-degree attitude excursion,
then runs the generated scenario through Rust validation and packet
forwarding. Separate existing CI checks still exercise the real SGP4
provider. This is an **offline finite-probe fault plan**, not a live
attitude controller or independently validated orbital/attitude
solution. Derived faults now carry optional `cause: "sampled_pointing"`
provenance, and PAT `lost` events report `sampled_pointing_loss`.
Legacy administrative faults remain backward compatible. This is
still an offline sampled fault plan, not native runtime steering evaluation.
Between-probe excursions and already-admitted packet interruption
remain unmodeled. See [optical attitude adapter](design/orbital-optical-attitude-adapter.md).

## Remaining subsystem work

P1 next: externally archived IERS source and independent multi-epoch frame verification (the production provider now accepts pinned EOP, but no authentic IERS archive is bundled or multi-epoch calibrated), large-scale orbit catalog and columnar ephemerides with benchmarks, physically calibrated time-resolved Doppler/PHY correction, beam/interference RF budget, MAC access/scheduler, optical PAT and spacecraft constraints. Per-packet sampled geometric delay is implemented, but signal travel-time iteration and continuous path prediction are not.

P2/P3: user session/core/IPAM, terrestrial routing emulation, realistic subscriber service and gateway handover, RF weather and interference, NOC/telemetry/security, Cesium console, large-scale event benchmarks and external SNS3/OpenSAND/OAI comparisons. The sixteen-system [engineering blueprint](subsystems/README.md) remains the overall target.

## Test commands

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
python -m pip install -e systems/orbital/python
python -m unittest discover -s systems/orbital/python/tests
python -m ntarlink_orbital.contacts scenarios/orbital-pass.json scenarios/orbit-sources.sample.json --out /tmp/orbital-run.json
cargo run -p ntarlink-cli -- simulate /tmp/orbital-run.json --json > /tmp/orbital-report.json
```
