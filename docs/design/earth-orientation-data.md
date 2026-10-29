# SYS-01 pinned Earth-orientation data (opt-in)

## Scope and fidelity

The offline SGP4 provider can now consume a **locally supplied, SHA-256-pinned**
IERS `finals2000A.all` table. The same Earth-orientation timescale is used by
both contact prediction and ITRS ephemeris export. Skyfield 1.55 interpolates
daily UT1 and polar motion; the orbital model remains TLE/SGP4, not precision
orbit determination. The default remains Skyfield's offline built-in timescale,
without an installed polar-motion table, for backward compatibility.

Reference: [Skyfield polar motion documentation](https://rhodesmill.org/skyfield/accuracy-efficiency.html)
and [IERS data and timescale documentation](https://rhodesmill.org/skyfield/time.html).
No network download is performed at simulation time.

## Input contract

In the orbit-source JSON, add:

```json
{
  "earth_orientation": {
    "schema_version": 1,
    "format": "iers-finals2000a",
    "path": "data/iers/finals2000A.all",
    "sha256": "<64 lowercase hexadecimal characters>",
    "valid_until_utc": "2014-01-24T00:00:00Z",
    "allow_predictions": false
  }
}
```

The path is resolved relative to the orbit-source JSON file's directory by
the contact and ephemeris CLIs. Programmatic callers of
`build_sgp4_provider(config, duration_ns, config_dir=...)` may supply a
different base directory. The operator must obtain and archive an authentic
IERS file separately, review its observation/prediction horizon and choose a
conservative `valid_until_utc`. No IERS file is bundled with the repository.

The loader checks the byte-for-byte digest, 8 MB file limit, 2–50,000 parsed
daily records, consecutive UTC MJD, finite/bounded DUT1 and polar motion,
simulation coverage and explicit validity deadline. It **fails closed**
outside that coverage; no silent extrapolation or auto-refresh is allowed.
The deadline cannot exceed the last tabulated UTC epoch. Simulation
intervals are half-open in integer nanoseconds. Because Python datetime and
the Skyfield interface represent only microseconds, the loader conservatively
rounds the **last included nanosecond up** to a microsecond when checking
coverage, expiry and prediction flags. A submicrosecond overrun at a source
boundary therefore fails closed rather than being silently truncated. This
may reject a submicrosecond interval that would otherwise fit a strict
nanosecond boundary; it does not imply nanosecond frame accuracy.

### Observation/prediction quality policy

The IERS finals2000A polar-motion and UT1 quality columns (zero-based byte
offsets 16 and 57) independently indicate `I` (observed/rapid-service)
or `P` (predicted). Skyfield's pinned numeric parser does not return these
flags, so ntarlink validates them against every parsed daily record.
Missing, unrecognized or duplicate quality rows fail closed.

The default `allow_predictions: false` rejects either `P` flag in the
UTC daily records bracketing the simulation interval (including the following
day used for linear interpolation). Predictions elsewhere in a larger source
archive are permitted but counted. An operator can explicitly set
`allow_predictions: true` for a forecast experiment; this is a fidelity
choice, not evidence that predicted EOP is equivalent to observations.

The manifest records `prediction_policy`, `predicted_source_days` and
`predicted_interpolation_days`. Both the EOP source SHA-256 and the
orbit-source JSON SHA-256 still bind the run to its exact inputs.
The flags are **not** cryptographic evidence of IERS authenticity; the
operator must still archive and authenticate the upstream release.

The source contains DUT1 (UT1−UTC, seconds) and polar motion xp/yp
(arcseconds). The pinned Skyfield 1.55 **built-in leap-second table** supplies
TAI−UTC. For each daily UTC record, the model computes
`TT−UT1 = (TAI−UTC) + 32.184 s − DUT1`, builds a Skyfield timescale and
installs the polar-motion table. Skyfield interpolates at simulation times.
This implementation does not independently ingest or certify future leap
seconds or guarantee EOP accuracy from a user-supplied file. The validity deadline is an operator
assertion, not an IERS authenticity certificate.

## Provenance and verification

Both `*.orbit-manifest.json` and `*.ephemeris-manifest.json` record the
mode, EOP SHA-256, tabulated MJD coverage, record count, expiry, interpolation
and leap-second source. The orbit-source JSON digest is retained separately.

```sh
python -m unittest discover -s systems/orbital/python/tests -p test_eop.py
python -m ntarlink_orbital.contacts scenarios/orbital-pass.json \
  path/to/orbit-config-with-pinned-eop.json --out /tmp/contacts.json
python -m ntarlink_orbital.ephemeris scenarios/orbital-pass.json \
  path/to/orbit-config-with-pinned-eop.json --step-ns 5000000000 \
  --out /tmp/ephemeris.json
```

Tests use **synthetic** finals2000A-formatted daily tables, with the central
2004 EOP values taken from the published Vallado reference. They verify
DUT1/polar interpolation, a reference TEME→ITRF vector, production-provider
opt-in behavior, IERS quality-flag prediction gating, provenance and
invalid-data rejection. These fixtures are
not official IERS snapshots. The independent single-epoch Vallado test
remains in `test_vallado_frame_reference.py`.

## Remaining work

Pin an actual externally obtained IERS archive with a documented retrieval
date and prediction flags; independently compare multiple production
Skyfield epochs with an external frame implementation; validate leap-second
updates and future-EOP policies; and measure constellation-scale frame
conversion performance. No centimetric physical orbit accuracy is claimed.
