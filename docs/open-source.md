# Open-source-first engineering policy

The authoritative dependency matrix, source URLs, licensing notes, integration APIs, selection gates and adapter topology live in [External dependencies and integration decisions](dependencies.md).

## Reuse principles

- Prefer mature and validated scientific/telecommunications models over an in-house rewrite when the model matches the required fidelity.
- Keep the Rust/Python/TypeScript product boundaries independent from C/C++/Java simulator internals.
- Avoid conflating an external standard (DVB-S2/RCS2 or 3GPP NTN) with undocumented Starlink air-interface behavior.
- Respect explicit GPL/LGPL/AGPL/CSSL conditions for pinned revisions and distributed binaries; independent-process adapters are an architectural design, **not** a claim of license exemption.
- Maintain numerical golden tests, dependency provenance, compatibility matrices and reproducible images for each adopted engine.

## Primary candidates

SGP4, Orekit, ns-3, SNS3, OpenSAND, OpenAirInterface RAN, Open5GS, GNU Radio, Linux netns/tc, FRRouting, StarryNet/OpenSN, CesiumJS, Prometheus and OpenTelemetry.

The repository currently contains **no implementation** of the external integrations above.