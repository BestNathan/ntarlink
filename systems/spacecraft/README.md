# SYS-02: spacecraft optical head availability

The `ntarlink-spacecraft` Rust crate owns a deterministic, indexed
virtual-time availability ledger for optical heads identified by their
spacecraft-owned head IDs. The opt-in `optical_head_outages[]` scenario
contract specifies half-open `[start_ns, end_ns)` maintenance or fault
intervals. All timestamps are scenario-relative nanoseconds.

Scenario validation rejects unknown PAT heads, empty/out-of-horizon intervals,
overlapping outages on the same head, and more than 10,000 outage entries.
Adjacent outages are allowed. The ledger intersects both endpoint heads'
availability with geometric and administrative-up windows before SYS-03 PAT
planning. At a head outage, tracking ends, the head is released, and recovery
requires fresh slew/acquisition. Other links sharing the same head are
constrained by the same ledger. PAT events use the
`optical_head_unavailable` loss reason; route installation and packet
accounting remain owned by the existing runtime.

```sh
cargo test -p ntarlink-spacecraft
cargo run -p ntarlink-cli -- simulate scenarios/optical-head-outage.json --json
```

**Fidelity:** preplanned head availability only. This is not a spacecraft
attitude model, pointing-vector constraint, optical power budget, autonomous
failure detection, live telemetry, or physical laser tracking. An already
admitted packet retains the L1 completion-after-departure convention. The
availability index is built in O(O log O) for O outage records; a candidate
queries only its two heads, with sorting of overlapping intervals. Overall
candidate output remains bounded by SYS-03's 100,000 candidate limit.
