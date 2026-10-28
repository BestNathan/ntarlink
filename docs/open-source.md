# Open-source-first integration candidates

| Area | Candidate | Integration approach | Caveat |
|---|---|---|---|
| Orbit | python-sgp4 / Orekit | subprocess/service adapter or independently validated Go implementation | license, accuracy, frames and time systems |
| Satellite networking | OpenSAND | compare gateway / satellite network behavior; optional emulator adapter | not a Starlink implementation |
| Satellite network simulation | SNS3 / ns-3 | reference PHY/MAC models, independent experiments | simulator coupling complexity |
| LEO dynamic network | StarryNet / OpenSN | emulation backend and benchmark comparison | verify maintenance and reproducibility |
| 5G NTN | OpenAirInterface / srsRAN | optional access-network lab | feature support differs by version |
| SDR | GNU Radio / gr-satellites | physical signal processing and telemetry experiments | not broadband Starlink air interface |
| CCSDS | NASA cFS / Yamcs | mission control and telemetry profile | CCSDS != Starlink user broadband |
| Visualization | CesiumJS | orbit and ground track visualization | check current license / asset terms |
| Networking | Linux netns/veth/tc/netem, FRRouting | packet emulation and routing | Linux-only, privileges required |

Do not automatically vendor these projects. Pin versions, audit licenses, verify interfaces, and preserve source attribution before integration.
