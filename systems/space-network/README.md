# SYS-03: deterministic optical PAT acquisition planning

This standalone Rust crate models a bounded **virtual-time acquisition gate**
for directed optical inter-satellite geometric contacts supplied by SYS-01.
A geometric contact is a candidate, never an already acquired optical link.

`plan_contacts` accepts half-open candidate windows in scenario-relative
integer nanoseconds, distinct exclusive optical-head identifiers, and explicit
slew/acquisition dwell assumptions. It sorts candidates by geometric start and
link ID, waits for both heads to be free, and reserves them through the end of
an acquired contact. A link becomes usable **only after** both dwell stages.
Contacts that expire before tracking begins are rejected, not silently admitted.
A contact ending at time `t` releases both heads at `t`, permitting a new
contact to start acquisition at the same instant.

The output contains half-open `AcquiredWindow` intervals and ordered
`PatEvent` transitions: candidate, optional slewing, acquiring, tracking,
and lost; rejected candidates have an explicit reason. At equal timestamps,
loss precedes new candidate processing. Inputs are capped at 100,000 windows,
and invalid intervals, overlapping windows for one link, duplicate head
identity, zero acquisition dwell and timestamp overflow fail closed.

Run:

```sh
cargo test -p ntarlink-optical-pat
cargo clippy -p ntarlink-optical-pat --all-targets -- -D warnings
```

**Fidelity and limitations:** deterministic assumed delays and one exclusive
resource per optical head, with nonpreemptive earliest-start/link-ID priority.
This is not physical pointing-angle validation, optical power or detector
budget, spacecraft attitude/thermal constraints, probabilistic acquisition,
tracking degradation, recovery from mid-contact faults, or measured Starlink
laser behavior. The planner is not yet connected to Rust packet forwarding,
routing or SYS-02 state; `AcquiredWindow` is a prospective integration
contract, not an active data-plane link. Sorting costs O(N log N), with
O(N) output and head-reservation state bounded by input size.

See [SYS-03 design](../../docs/subsystems/03-space-network.md) and
[engineering roadmap](../../docs/roadmap.md).
