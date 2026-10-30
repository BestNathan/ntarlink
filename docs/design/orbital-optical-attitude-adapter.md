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
provider positions and manifest reproducibility. CI also propagates a
**synthetic two-orbit** fixture through the real SGP4/Skyfield provider,
compiles the geometry, validates the Rust scenario, and exercises PAT
packet forwarding. The fixture deliberately perturbs the historical
ISS TLE mean anomaly to construct a distinct second orbit; it is **not**
an independently sourced constellation catalog or validated orbit truth.
The smoke fixture uses a permissive 180-degree steering envelope to
verify the integration path, not calibrated pointing accuracy.

Next: introduce time-indexed body-to-ITRS attitude samples and optical
geometry checks during each contact, then independent attitude and
frame reference validation. Until then, a contact that starts in
envelope may drift outside it without the runtime noticing.
