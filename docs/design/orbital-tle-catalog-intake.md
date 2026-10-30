# SYS-01 offline TLE catalog intake and provenance

## Purpose and trust boundary

`ntarlink_orbital.tle_catalog` converts a local ASCII two-line element catalog
into the existing SYS-01 `epoch_utc` / `satellites` orbit configuration. It
accepts strict 2-line or `0 NAME` / 1 / 2 records. The input is never fetched
automatically, and a caller must provide an HTTPS source URL, a license or
permission descriptor, and a UTC retrieval timestamp.

The importer checks both TLE mod-10 checksums, matching five-digit catalog
numbers, unique NORAD numbers, calendar-valid TLE epochs and the maximum age
relative to the selected simulation epoch. It rejects malformed or truncated
pairs, non-ASCII input, catalogs larger than 1 MB or 1,000 satellites, and
nonfinite/invalid age limits. These checks validate input structure and
recency, **not** orbital truth or independently verified source ownership.

## CLI

First save a permitted and independently sourced catalog to a local file.
The example uses placeholders, not a bundled external catalog:

```sh
python -m ntarlink_orbital.tle_catalog /path/to/catalog.tle \
  --epoch-utc 2026-10-29T20:00:00Z \
  --retrieved-utc 2026-10-29T19:59:00Z \
  --source-url https://example.org/your-licensed-snapshot.tle \
  --source-license 'Operator-verified permission: reference' \
  --out /tmp/orbit-catalog.json
python -m ntarlink_orbital.sgp4_scale_benchmark /tmp/orbit-catalog.json \
  --satellites 2 --samples-per-satellite 2 --catalog-mode unique
```

Use `--satellites` equal to the actual catalog size, not necessarily 2.
The importer does not change the propagation code or the existing Skyfield
TLE freshness/ITRS transformation policies. The benchmark's requested
duration must also remain inside its own provider age limit.

The default companion manifest is
`<output>.tle-manifest.json`; `--manifest` can override its path. The
manifest includes SHA-256 of the exact input bytes and generated JSON bytes,
satellite count, minimum and maximum source TLE epochs, and the caller's
source URL, license descriptor and retrieval timestamp. The source metadata
is **operator asserted** and not independently checked. The SHA-256 digest
detects changes relative to a trusted manifest; it is not a digital signature
or source authentication.

## Verified consumption by the isolated SGP4 benchmark

The opt-in `--catalog-manifest` argument on
`ntarlink_orbital.sgp4_scale_benchmark_suite` requires
`--catalog-mode unique`. The suite validates the exact configuration
bytes against the importer-generated `orbit_config_sha256`, checks the
manifest schema and source digest syntax, satellite count, unique NORAD
identities, TLE checksums, and epoch age before starting any worker.
Each worker reopens and verifies the manifest and configuration before
constructing the production Skyfield provider; the suite rejects any
worker reporting a different manifest digest.

```sh
python -m ntarlink_orbital.sgp4_scale_benchmark_suite /tmp/orbit-catalog.json \
  --catalog-manifest /tmp/orbit-catalog.json.tle-manifest.json \
  --catalog-mode unique --satellites 2 --samples-per-satellite 2 \
  --repetitions 2 --report /tmp/verified-catalog-benchmark.json
```

This is **local integrity checking against a caller-trusted manifest**,
not a cryptographic signature or proof of publisher identity, usage rights,
orbit accuracy, or an independently sourced multi-orbit fixture. Verification
reads bounded inputs (2 MB configuration and 64 KiB manifest), rejects
duplicate JSON keys and nonfinite values, and returns the exact verified
configuration object to the worker. The existing unpinned benchmark mode
remains available for synthetic and historical smoke tests.

## Verification and fidelity

Unit tests cover a historical public ISS TLE, checksums, catalog IDs,
named records, truncation, duplicate entries, stale epochs, invalid metadata,
resource bounds, output digest and the existing distinct-TLE benchmark
contract. CI also runs an actual two-position Skyfield/SGP4/ITRS computation
from an imported historical ISS TLE.

A test-generated second TLE identifier is only a **synthetic parser and
cardinality fixture**. No independent multi-orbit licensed catalog is bundled
or asserted by this change. A real 100k/500k benchmark still requires an
externally sourced and legally reviewed snapshot, pinned environment,
repeatable hardware, and separate performance measurement. The importer
does not archive leap-second/EOP inputs or independently calibrate ITRF.
