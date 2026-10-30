# SYS-01 to SYS-02 SGP4 optical attitude snapshot adapter

## Scope and fidelity

`ntarlink_orbital.optical_geometry` compiles **offline** SGP4/Skyfield
satellite positions in the ITRS Earth-fixed frame at the start of each
scheduled optical PAT contact. Each optical head supplies an explicit fixed
body-frame boresight and a fixed **body-to-ITRS** unit quaternion, ordered
`[w, x, y, z]`. The adapter rotates the body boresight into ITRS and
writes the existing `optical_geometry[]` contract. SYS-02 computes
reciprocal line-of-sight angles; SYS-03 applies its acquisition dwell and
the packet runtime gates forwarding.

This is an **analytical, discrete contact-start snapshot**, not a spacecraft
attitude propagator, time-varying pointing controller, continuously valid
optical link, or laser PHY. A body-to-ITRS quaternion fixed in an Earth-fixed
frame is an operator-supplied attitude assumption, not a physical attitude
solution. The SGP4 source is an orbital propagation estimate, not orbit truth.
The adapter does not derive optical contact windows or verify Earth clearance;
the scenario must supply candidate windows from a separate geometric source.

## Inputs and command

```sh
python -m ntarlink_orbital.optical_geometry \
  scenarios/optical-sgp4-attitude.json \
  scenarios/optical-sgp4-orbits.synthetic.json \
  scenarios/optical-sgp4-attitude.synthetic.json \
  --pointing-probes-per-contact 3 \
  --out /tmp/optical-sgp4-compiled.json

cargo run -p ntarlink-cli -- simulate /tmp/optical-sgp4-compiled.json --json
```

The scenario must use schema v1, `contact_mode: "scheduled"`, optical
ISL links, `optical_pat[]` and half-open `contacts[]`. Every PAT optical
contact receives exactly one snapshot at its `start_ns`. The adapter
rejects an existing `optical_pointing[]` or `optical_geometry[]`
configuration rather than silently replacing it.

The attitude file is a JSON object:

```json
{
  "frame": "ITRS",
  "attitude_model": "fixed_body_to_itrs_quaternion",
  "heads": [
    {
      "head_id": "sat-a/head-1",
      "boresight_body": [1, 0, 0],
      "body_to_itrs_quaternion_wxyz": [1, 0, 0, 0],
      "max_off_axis_deg": 30
    }
  ]
}
```

Each referenced PAT head must belong to the corresponding satellite
(`<satellite-id>/...`) and appear exactly once. The boresight is a
nonzero body-frame vector and is normalized. The quaternion must be
finite, nonzero and within `1e-6` of unit norm; the adapter normalizes
it before rotation. Steering limits are inclusive and expressed in
degrees. Positions are in meters, time is integer nanoseconds relative
to the orbital `epoch_utc` (UTC), and the Skyfield provider resolves
positions only at microsecond-aligned timestamps. TEME is never treated
as ITRS.

## Optional time-indexed body-to-ITRS attitude

The adapter also accepts
`"attitude_model": "sampled_body_to_itrs_quaternion"` instead of
`fixed_body_to_itrs_quaternion`. Each head then supplies
`attitude_samples` instead of the single fixed quaternion:

```json
{
  "frame": "ITRS",
  "attitude_model": "sampled_body_to_itrs_quaternion",
  "heads": [{
    "head_id": "sat-a/head-1",
    "boresight_body": [1, 0, 0],
    "max_off_axis_deg": 30,
    "attitude_samples": [
      {"at_ns": 0, "body_to_itrs_quaternion_wxyz": [1, 0, 0, 0]},
      {"at_ns": 100000000, "body_to_itrs_quaternion_wxyz": [0.9961946980917455, 0, 0, 0.08715574274765817]},
      {"at_ns": 200000000, "body_to_itrs_quaternion_wxyz": [1, 0, 0, 0]}
    ]
  }]
}
```

Each sample is a unit quaternion in scalar-first `[w,x,y,z]` order,
representing body-to-ITRS orientation at integer virtual `at_ns`.
Timestamps are strictly increasing and microsecond-aligned, begin at
zero, and cover the final representable microsecond of the scenario.
The final keyframe may equal the exclusive scenario duration. At least
two samples are required per head and no more than 100,000 across all
heads. Duplicate timestamps, malformed sample keys, nonfinite or
non-unit quaternions, missing coverage and mixing fixed/sampled fields
fail closed.

Between keyframes the adapter uses normalized shortest-arc quaternion
SLERP, treating `q` and `-q` as equivalent rotations and using
normalized linear interpolation for nearly identical orientations.
The interpolated body-to-ITRS rotation is applied to the fixed
body-frame head boresight. The output retains one ITRS boresight
snapshot per contact start. With `--pointing-probes-per-contact`, the
audit also evaluates the interpolated head attitude **at each probe
time**, alongside freshly propagated SGP4 positions. A sampled attitude
excursion can therefore fail the audit even if the contact-start
snapshot passes. The manifest records the chosen `attitude_model`.

This is a deterministic interpolation of **operator-supplied**
orientation samples, not an attitude dynamics solution, gyro/actuator
model or certified spacecraft frame. Finite probes cannot establish
continuous steering feasibility, and the Rust runtime does not consume
the time-indexed samples or generate mid-contact tracking-loss events.
For a reproducible integration smoke, use
`scenarios/optical-sgp4-attitude-sampled.synthetic.json` with the
existing synthetic two-orbit scenario. No optical-head actuator slew-rate or angular-acceleration constraint is
enforced. An optional modeled spacecraft body-attitude rate limit is described
below; it is not a head steering-rate model.

## Optional sampled body-attitude angular-rate guard

A sampled head may optionally specify
`"max_body_attitude_rate_deg_per_s": 900.0` (finite, nonnegative degrees
per second). For each pair of adjacent normalized body-to-ITRS quaternion
keyframes, the adapter computes the shortest relative rotation (treating
`q` and `-q` as identical) divided by the elapsed scenario-relative
time. It rejects the entire compilation if the implied constant-angular-rate
SLERP segment exceeds the configured cap. An exact boundary is allowed.
A zero cap permits only unchanged modeled orientation.

The check is deterministic and linear in the number of supplied keyframes;
it uses a chord/asin formulation for stable small rotations and rejects
invalid limits. The output manifest includes an `attitude_rate_audit` for
every head: configured cap (null if absent) and maximum implied keyframe
rate. An omitted cap preserves the previous behavior. The cap is valid only
for `sampled_body_to_itrs_quaternion`, not the fixed model.

This is a constraint on the **operator-supplied body-to-ITRS interpolant**,
not a spacecraft torque/dynamics simulation, head gimbal slew-rate model,
optical tracking error bound or proof of physical feasibility between
observations. It does not change Rust PAT acquisition timing or packet
completion semantics.

## Optional finite steering-probe gate

With `--pointing-probes-per-contact 1`, `3` or `7`, the compiler
evaluates each candidate PAT contact at its start, at the requested
evenly spaced interior instants, and at the final representable
microsecond before its exclusive end. All times are quantized to the
provider's microsecond resolution; duplicates in short windows are
evaluated once. Both endpoint line-of-sight angles are computed from
fresh SGP4 ITRS positions against the **same fixed body-to-ITRS head
boresights**. If any probe violates either head's inclusive steering
limit (with the same 1e-9 degree comparison tolerance as SYS-02), the
compiler fails **before** replacing the output or manifest. It does
not silently admit a failed contact or convert it to a passing snapshot.

A successful manifest includes `pointing_probe_audit`: per-contact
sample counts, maximum observed endpoint angles, worst sampled
steering margin and its timestamp, plus aggregate counts. Without
the option this field is null and legacy start-only compilation
remains unchanged. For 10,000 contacts and seven interior probes,
the audit adds at most 160,000 position-provider evaluations
(8 additional timestamps times two satellites per contact), excluding
the original 20,000 contact-start evaluations. This is a bounded
offline compute cost, **not** a measured runtime benchmark.

A finite set of samples cannot prove continuous feasibility or detect
brief between-sample excursions. The output still contains only
contact-start `optical_geometry[]` snapshots, so the Rust runtime
does **not** re-evaluate pointing between samples. No time-varying
attitude, optical link budget, tracking servo, or physical reacquisition
is implied. This option is an explicit conservative admission gate
for the sampled instants, not a replacement for a time-varying
contact-state model.

## Opt-in sampled pointing loss and PAT reacquisition

The optional `--runtime-pointing-probes-per-contact 1|3|7` mode compiles
**operator-supplied time-indexed** attitude and propagated ITRS geometry
into deterministic link `faults[]` edges for the existing Rust PAT runtime.
It is mutually exclusive with the fail-closed
`--pointing-probes-per-contact` admission audit. Both reuse the same
contact-start optical geometry snapshot and microsecond-aligned probe grid.

For each geometric PAT contact, the compiler evaluates both optical heads
at contact start, evenly spaced interior instants and the final representable
microsecond. A passing-to-failing sample emits a link `down` fault at
that sample; a failing-to-passing sample emits `up`. If the final sample
fails, a recovery edge at the exclusive contact end resets the link for
the next independently evaluated contact. This uses the existing SYS-03
administrative-fault acquisition logic: tracking is lost, heads are released,
the controller reconverges and a recovered link must slew and acquire again.
The manifest records every derived fault edge and the probe count.

**Safety constraints:** the contact start must pass, because Rust currently
uses the start snapshot to gate the entire geometric contact. Existing
administrative faults on the same PAT link are rejected rather than
unsafely combined with derived edges. Other link faults are preserved.
The compiler caps generated edges at 10,000 and retains input/output
SHA-256 provenance. No new Rust scenario schema is required.

Example with a *synthetic aligned ITRS position provider* (the committed
tracking fixture is not a real orbital/attitude reference):

```sh
python -m unittest discover -s systems/orbital/python/tests
# The GitHub Actions acceptance injects a deterministic synthetic position
# provider and runs the compiled scenario through Rust validate/simulate.
```

Generated `faults[]` edges now include `"cause": "sampled_pointing"`,
while legacy events without `cause` default to `administrative`.
The Rust PAT trace labels a derived loss `sampled_pointing_loss`
instead of `administrative_link_down`. This is **typed provenance for
a precomputed finite-probe transition**, not a physical sensor reading
or a native continuous attitude model. The manifest also preserves
the sampled pointing source. A down/up edge is a **discrete sampled approximation**,
not continuous runtime steering evaluation. Brief excursions between probes
can be missed, and a recovery sample does not establish continuous stability.
Already-admitted packet transmissions follow the existing
`complete_after_departure` convention. No physical optical tracking servo,
angular-rate bound, interference or laser PHY is represented.

## Safety and provenance

The adapter rejects missing TLE IDs, duplicate contact starts, invalid
time bounds, nonfinite positions, coincident endpoints, malformed head
ownership, non-unit quaternions, and invalid limits. Inputs are bounded
to 16 MB each and 10,000 PAT contacts. Output is sorted by
`(start_ns, link_id)` and written through a temporary file replacement.
The companion `.optical-manifest.json` records exact SHA-256 hashes of
the three input files and the compiled scenario, the UTC epoch, ITRS
frame and the provider's Earth-orientation metadata.

SHA-256 establishes local content integrity relative to the manifest;
it does not authenticate a TLE publisher, an attitude sensor or the
input source. The output and manifest are two separate atomic file
replacements, not a transactional pair. Consumers should verify the
artifact hash before using the manifest.

## Test evidence and limitations

Python unit tests cover deterministic ordering, quaternion rotation,
invalid attitude/head ownership, malformed contact windows, degenerate
provider positions, manifest reproducibility, finite-probe drift rejection,
exclusive-end sampling, and no-write behavior on audit failure. CI also propagates a
**synthetic two-orbit** fixture through the real SGP4/Skyfield provider,
compiles the geometry, validates the Rust scenario, and exercises PAT
packet forwarding. The fixture deliberately perturbs the historical
ISS TLE mean anomaly to construct a distinct second orbit; it is **not**
an independently sourced constellation catalog or validated orbit truth.
The smoke fixture uses a permissive 180-degree steering envelope to
verify the integration path, not calibrated pointing accuracy.

Next: introduce time-indexed body-to-ITRS attitude samples and runtime
contact-state transitions on sampled steering failures, followed by
independent attitude and frame reference validation. Without the
opt-in audit, a contact that starts in envelope may drift outside
it unnoticed; even with the audit, between-probe drift is not bounded.

## Independent administrative and sampled-pointing fault domains

The Rust SYS-03 PAT preparation layer now treats `administrative` and
`sampled_pointing` fault edges as independent booleans for each optical PAT
link. The effective link is available only while **both** domains are up,
the geometric contact is active, both optical heads are available, and
acquisition has completed. A recovery in either domain does not reopen a
link while the other remains down. Edges at the same timestamp are applied
as one deterministic batch before the effective state is evaluated.

Only changes in the combined fault state are forwarded to the packet
runtime as link up/down events. This prevents misleading intermediate
`link_up` control events and premature route installation during
overlapping faults. PAT loss reasons retain the domain that first made
the effective state unavailable; simultaneous failures use
`administrative_link_down` precedence. All timestamps remain
scenario-relative nanoseconds and contact windows remain half-open.

The fixture `scenarios/optical-pat-overlapping-faults.json` covers
pointing loss at 30 ms, administrative down at 40 ms, pointing recovery
at 50 ms, and administrative recovery at 70 ms. With 20 ms acquisition,
tracking resumes at 90 ms rather than 70 ms. Rust tests cover both
fault orders and simultaneous same-time edges; CI checks the full
packet/control/PAT trace and packet accounting.

This is still an **offline discrete-event fault plan**, not a native
attitude solver or continuous optical PHY. Already-admitted transmissions
retain the existing complete-after-departure convention.
