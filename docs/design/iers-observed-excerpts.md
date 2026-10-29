# SYS-01 official IERS excerpts: multi-epoch regression

## Source and reproducibility

The three small, **unmodified fixed-width row excerpts** under
`systems/orbital/python/tests/data/finals2000a-*.txt` were transcribed
on 2026-10-29 from the official [IERS Rapid Service finals2000A.all](https://datacenter.iers.org/products/eop/rapid/standard/finals2000A.all)
publication. Each excerpt preserves three consecutive daily rows, including
the I/P quality columns, for the following UTC dates:

| Excerpt | UTC days | SHA-256 of committed excerpt | Quality |
| --- | --- | --- | --- |
| 2004 | 2004-04-05 through 2004-04-07 | `bf9ddc7259666e6315c0ff313e6569b3f08a695a3be62d24f7cb7fd4ec5192cf` | observed I/I |
| 2014 | 2014-01-22 through 2014-01-24 | `79af897ec4e9b1d72b3d4d3e95919ed4d824452ed862a4ca5cfa4b78e964e89e` | observed I/I |
| 2026 | 2026-10-22 through 2026-10-24 | `81cee54306de1f767e03982f0e3aadf72ea5103a5bdbaec2fd6d2ff504d65073` | observed then predicted P/P |

The upstream file is a **rolling publication**: its URL does not identify
an immutable version. These committed excerpts are reproducible and test
the production parser against real IERS row layouts, but do not authenticate
the full upstream archive or supply a production-grade IERS snapshot.
The repository's SHA-256 values pin only the committed excerpts.

## Verification and limits

`test_official_iers_excerpt.py` passes the observed 2004 and 2014 excerpts
through the **production** pinned-EOP timescale builder, verifies daily
UT1/polar-motion interpolation and compares Skyfield's TEME-to-ITRF output
against the separate test-only Vallado GMST82/IAU-1980 calculation at both
epochs. The shared synthetic TEME test vector is **not** an independently
published 2014 orbit position or ITRF reference vector; the numerical
comparison tests implementation agreement, not physical orbit accuracy.

The 2026 excerpt tests the observed-to-forecast boundary and verifies
that the default policy rejects predicted polar-motion/UT1 records, while
explicit opt-in reports the two forecast interpolation days. No prediction
is represented as a measured value.

Run:

```sh
python -m unittest discover -s systems/orbital/python/tests -p test_official_iers_excerpt.py
```

Remaining acceptance: immutable, independently authenticated **complete**
IERS and leap-second archives, independently published multi-epoch ITRF
reference vectors, and quantified constellation-scale performance.
