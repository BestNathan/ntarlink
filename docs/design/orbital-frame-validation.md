# SYS-01 historical frame regression and contact-tail coverage

## Reference source and fidelity

The regression in `systems/orbital/python/tests/test_reference_frames.py`
uses the **published 2014 ISS example** from the
[Skyfield Earth Satellites guide](https://rhodesmill.org/skyfield/earth-satellites.html).
The repository pins the same historical TLE in
`scenarios/orbit-sources.sample.json` and the implementation uses
`skyfield==1.55`, `sgp4==2.27` with a built-in, offline timescale.

At **2014-01-23 11:18:07 UTC**, the guide publishes:

| Observable | Published value | Test tolerance |
| --- | --- | --- |
| GCRS x/y/z (km) | -3918.87650458, -1887.64838745, 5209.08801512 | 0.1 km per component |
| WGS84 geodetic latitude | 50° 14′ 37.4″ north | 0.005° |
| WGS84 geodetic longitude | 86° 23′ 23.3″ west | 0.005° |
| Bluffton, Ohio geometric altitude | 16° 16′ 32.6″ | 0.03° |
| Bluffton, Ohio slant range | 1168.7 km | 1 km |

The GCRS vector is compared against the pinned TLE/SGP4 Skyfield output.
The production `build_sgp4_provider()` returns **ITRS/ECEF meters**, not
TEME or GCRS. A separate test-only Bowring WGS84 inverse maps that ITRS
position to the published geodetic subpoint, while the production topocentric
geometry is compared to the published altitude and slant range. A raw
`sgp4.api.Satrec.sgp4()` TEME position is also explicitly checked to **fail**
the Earth-fixed longitude reference, guarding against accidental frame reuse.

These are independently **published expected observables**, but the source
and production adapter both rely on Skyfield/SGP4. They are **not** independent
TEME-to-ITRF software verification, a high-accuracy IERS EOP reference,
or a guarantee of real ISS positional accuracy. TLE prediction errors,
built-in Earth orientation assumptions and rounding of guide outputs limit
the physical claims. The independent reference-vector regression below closes only the
single-epoch transform-vector gap. An opt-in EOP adapter now exists; independent multi-epoch comparison,
authentic IERS archive validation and future leap-second provenance remain
acceptance work.

## Independent 2004 TEME/PEF/ITRF reference vectors

The additional regression in `systems/orbital/python/tests/test_vallado_frame_reference.py`
uses an **external** 2004-04-06 07:51:28.386009 UTC low-Earth-orbit
position example from Vallado, Seago and Seidelmann (AAS-06-134), as
reproduced by [Orekit 13.0's ITRF/GTOD reference test](https://github.com/CS-SI/Orekit/blob/13.0/src/test/java/org/orekit/frames/ITRFEquinoxProviderTest.java)
and [Astropy 7.1.1's TEME/ITRS reference test](https://github.com/astropy/astropy/blob/v7.1.1/astropy/coordinates/tests/test_intermediate_transformations.py).

The pinned reference EOP values are **UT1−UTC = −0.4399619 seconds**,
**x polar motion = −0.140682 arcseconds**, and
**y polar motion = +0.333309 arcseconds**. The example uses the historical
Vallado GMST82 and IAU-1980 polar-motion convention, not an IAU-2006
reduction or an assumed zero-EOP UTC approximation. All vectors are in
kilometers at the same epoch:

| Frame | X | Y | Z |
| --- | ---: | ---: | ---: |
| TEME | 5094.18016210 | 6127.64465050 | 6380.34453270 |
| PEF (GTOD) | -1033.4750313 | 7901.3055856 | 6380.3445328 |
| ITRF | -1033.4793830 | 7901.2952754 | 6380.3565958 |

A **test-only, independent** GMST82 rotation and polar-motion matrix
validate the published PEF and ITRF positions. The pinned
`skyfield==1.55` TEME-to-ITRF helper is independently compared with the
external ITRF vector, using the *same explicit EOP inputs*. Each
three-dimensional position error must be less than **0.03 meters**.
This tolerance includes the rounding of published reference vectors and
the few-microsecond loss in a double-precision Julian-date calculation;
it is a regression tolerance, **not** a claim of centimeter-level
TLE orbit accuracy. Negative controls verify that replacing UT1 with UTC
produces more than 100 meters of error and omitting polar motion more
than 10 meters for this vector.

This validates a reference transformation with **supplied** EOP and
independent published vectors. The production `build_sgp4_provider()`
still defaults to Skyfield's offline built-in timescale, but now supports
an **opt-in checksum-pinned local IERS finals2000A table** for UT1 and
polar motion. See [Earth-orientation input](earth-orientation-data.md).
The opt-in implementation has explicit interpolation, coverage, expiry and
provenance, but is **not independently calibrated across multiple epochs**
and does not bundle an authenticated IERS archive. Further work must compare
the production provider with independent vectors at several epochs and
validate future leap-second updates. Velocity/LOD and full
precession-nutation validation remain out of scope.

## Trailing partial sample interval

The contact predictor now samples the last representable time
`duration_ns - 1` even when `refine_tolerance_ns == step_ns`.
Previously the default coarse mode could omit a rise or set in the final
partial sampling interval. A 250 ns synthetic scenario with 100 ns steps
and a transition at 225 ns verifies both late rise and late set; a 1 ns
refinement test verifies the original bisection result.

Default coarse sampling remains conservative for **detected** transitions:
a rise opens at the first visible sample, while a set closes at the previous
known-visible sample, including when the actual set occurs exactly on a
coarse sample boundary. This can omit up to one
coarse sampling interval of valid contact, but does not extend a detected
setting window into a known-invisible interval. The trailing partial
interval uses the same rule. A late rise may produce only a short terminal
window. Optional propagation-delay samples are therefore never generated
past a detected coarse set.

Use bisection refinement for tighter rise/set bounds. No finite sampling
step proves continuous visibility: multiple transitions within a single
sample interval may still be missed, including in the final interval.
These guarantees apply to bracketed transitions, not unobserved ones.

## Verification

```sh
python -m pip install -e systems/orbital/python
python -m unittest discover -s systems/orbital/python/tests
```

CI also exercises the historical ISS contact artifact through the Rust
packet engine. No RF/optical acquisition, modem, or continuous propagation
delay capability is implied by these regression tests.
