# Packet Adapter shared SYS-02 execution integration

Status: implementation plan for [P1 issue #3](https://github.com/BestNathan/ntarlink/issues/3). This document does **not** claim that the shared execution path exists.

## Current boundary

`execute_pat_head_timeline` in `platform/packet-runtime/src/lib.rs` constructs a `PatExecutionRuntime` with optical-head actuators but no registered fault links. It commits the canonical SYS-03 Release/Reserve/Command timeline transactionally and returns provisional BODY-frame command outcomes. `project_body_following_faults` then converts these outcomes into offline `SampledPointing` edges. `prepare_pat` replays the administrative and sampled fault batches in a second `PatExecutionRuntime` with registered links but no actuators, reduces effective availability to contact windows, plans reservations, and finally checks leased command parity.

The current execution primitive already supports Fault, Release, Reserve, Command, and post-Command PointingObservation phases, including transactional rollback. Merely concatenating existing streams is incorrect: predicted fault windows influence reservation planning, while the observations that produce those windows depend on command execution. A same-time command-derived loss must not suppress its own successful command.

## Target invariants

1. SYS-02 owns committed fault domains, optical-head state, leases, command outcomes, and post-command observations. The packet adapter consumes immutable effective-state edges and PAT traces; it must not maintain a competing authoritative fault reducer.
2. Provisional planning is explicitly speculative and does not mutate the committed execution owner. Planning may produce candidate contacts and reservations, but not authoritative observed faults.
3. The committed event stream has deterministic virtual-time ordering: external FaultBatch, Release, Reserve, Command, then command-derived PointingObservation. Independent links remain order-independent. Rejected events do not partially commit a batch.
4. Only a successful same-link, same-instant command may authorize a command-derived **Down** observation. A failed command is not evidence of recovery. An **Up** recovery must be an explicitly sourced external observation or fault transition, not inferred from a command that could not execute while down.
5. Administrative and sampled-pointing faults are independent domains. A recovery of one domain cannot reopen a link held down by the other. Preserve the original down cause for PAT trace attribution, while converting only the final effective state to the packet engine's metadata-free administrative edge.
6. Resource contention, failed-command provenance, same-instant handoff, link identity, and head-pair identity remain fail-closed. The existing scenario schema and packet accounting must not silently change.

## Incremental implementation sequence

### A. Separate observation evidence from fault projections

Introduce an internal typed projection for each BODY-frame command containing link identity, timestamp, exact command outcome, and the computed pointing predicate. Do not encode failed commands as successful sensor observations. Keep existing `project_body_following_faults` behavior as a compatibility baseline until the committed executor consumes the new projection.

Acceptance: tests distinguish successful pointing loss, rejected command, independent external recovery, and an unrelated link's command. Input permutation cannot alter the projection.

### B. Build a committed timeline from validated planning output

Keep provisional SYS-03 candidate and reservation planning as a read-only phase. Register the actual PAT links and optical-head actuators in one SYS-02 execution owner for the **committed** timeline. Include administrative fault edges, leases, and command events. Feed only causally valid command-derived observations after their matching commands; represent recovery through separately sourced events. Ensure that planned reservations are not retroactively invalidated by their own resulting observation.

Acceptance: shuffled cross-link events produce identical committed head, lease, fault, contact and trace results; a late invalid event rolls back all earlier changes in the transaction. A fault at the command timestamp follows the documented phase ordering.

### C. Retire the duplicate authoritative fault replay

Derive effective link availability and packet edges from the committed SYS-02 state transitions, preserving link-major packet-edge serialization and the original down-cause trace. Compare outputs against the existing two-pass adapter for non-conflicting legacy fixtures. If planning needs a projected fault window, label it speculative and reconcile it against committed outcomes rather than treating it as a second authority.

Acceptance: deterministic packet accounting, route convergence, PAT acquisition/reacquisition, shared-head contention, and recovery regressions remain unchanged unless an intentional behavior change is documented with a counterexample.

## Required deterministic acceptance matrix

| Scenario | Required observation |
| --- | --- |
| Successful Command then same-time pointing Down | Command succeeds; loss becomes effective afterward |
| Failed Command then inferred Up | No recovery; failure provenance preserved |
| External Up while administrative Down | Effective link stays down |
| Administrative Down with same-time Command | Command rejected according to phase order |
| Two links share a head at one instant | Deterministic fail-closed contention or validated Release/Reserve handoff |
| Invalid late event in mixed batch | Entire committed state unchanged |
| Reversed input order across independent links | Identical contacts, PAT traces, packet accounting, and digests |
| Provisional forecast differs from committed outcome | Explicit mismatch or replan, never silent state divergence |

## CI and review gate

Implement in a dedicated code PR, not by marking this plan complete. Run the GitHub-hosted Rust formatting, Clippy, workspace tests, Python, TypeScript, docs checks and packet baseline as applicable. Inspect actual Actions jobs against the code PR head SHA; a successful main run or empty commit status is not evidence for the new changes. Keep the PR draft if any required check is unavailable. Do not close issue #3 until the shared production path and end-to-end tests are verified.
