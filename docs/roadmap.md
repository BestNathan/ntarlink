# Delivery roadmap

## P0 — executable foundation
- [x] Repository bootstrap, CLI and scenario validation.
- [ ] Deterministic event clock, topology graph and unit-tested packet event engine.
- [ ] Minimal end-to-end demo: UE → satellite A → satellite B → gateway → internet sink, with latency/queue metrics.
- [ ] CI quality gates and reproducible scenario snapshots.

## P1 — physically meaningful LEO constellation
- [ ] TLE/SGP4 propagation, ECEF/ECI frames, elevation and Earth occultation.
- [ ] RF path loss, Doppler, beam footprint, terminal attach and handover.
- [ ] Optical ISL link establishment, PAT and topology changes.
- [ ] Routing / gateway selection / failure injection.

## P2 — network engineering
- [ ] IP packet emulation using namespaces/veth/tc/netem, Linux routing and traffic generation.
- [ ] Scheduling, QoS, spectrum reuse, contention, congestion, capacity and jitter.
- [ ] Ground gateway / PoP / terrestrial backhaul and mobility session continuity.
- [ ] Web UI with globe, coverage, beams, packet traces, metrics and replay.

## P3 — interoperability and scale
- [ ] OpenSAND/SNS3 models and validation comparisons.
- [ ] OAI / srsRAN 5G NTN experiment adapter where supported.
- [ ] GNU Radio / SDR adapter and CCSDS experimentation as separate profiles.
- [ ] Distributed simulation, reproducible benchmarking and documented accuracy bounds.

## Acceptance principle
A checkbox requires runnable tests and evidence; naming a module or referencing a library is not implementation.
