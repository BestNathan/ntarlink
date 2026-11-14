//! Deterministic interval-based admission for modeled shared compute resources.
//! This is an S1 planning primitive, not a hardware scheduler or network authority.

use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Capacity {
    pub memory_bytes: u64,
    pub storage_bytes: u64,
    pub power_mw: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Request {
    pub task_id: String,
    pub node_id: String,
    /// Half-open modeled execution interval [start_ns, end_ns).
    pub start_ns: u64,
    pub end_ns: u64,
    pub memory_bytes: u64,
    pub storage_bytes: u64,
    pub power_mw: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Decision {
    Admitted,
    RejectedInvalidInterval,
    RejectedUnknownNode,
    RejectedDuplicateTask,
    RejectedCapacity,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Admission {
    pub task_id: String,
    pub node_id: String,
    pub decision: Decision,
}

/// A deterministic S1 capacity ledger. The caller owns placement, task execution,
/// transfer completion, and the virtual clock; this ledger only owns reservations.
pub struct Ledger {
    capacities: BTreeMap<String, Capacity>,
    admitted: Vec<Request>,
    retired_tasks: BTreeSet<String>,
}

impl Ledger {
    pub fn new(capacities: BTreeMap<String, Capacity>) -> Self {
        Self {
            capacities,
            admitted: Vec::new(),
            retired_tasks: BTreeSet::new(),
        }
    }

    pub fn admitted(&self) -> &[Request] {
        &self.admitted
    }

    pub fn admit(&mut self, request: Request) -> Admission {
        let decision = if request.task_id.is_empty() || request.start_ns >= request.end_ns {
            Decision::RejectedInvalidInterval
        } else if !self.capacities.contains_key(&request.node_id) {
            Decision::RejectedUnknownNode
        } else if self.retired_tasks.contains(&request.task_id)
            || self.admitted.iter().any(|r| r.task_id == request.task_id)
        {
            Decision::RejectedDuplicateTask
        } else if self.fits(&request) {
            self.admitted.push(request.clone());
            Decision::Admitted
        } else {
            Decision::RejectedCapacity
        };
        Admission {
            task_id: request.task_id,
            node_id: request.node_id,
            decision,
        }
    }

    /// Atomically admit a set of resource phases. Each phase has its own
    /// interval and resource profile; all phases are committed or none are.
    /// Distinct phase identities keep existing cancellation semantics explicit.
    /// This is a modeled resource transaction, not a packet delivery receipt.
    pub fn admit_bundle(&mut self, mut phases: Vec<Request>) -> Vec<Admission> {
        phases.sort_by(|a, b| {
            (&a.task_id, &a.node_id, a.start_ns, a.end_ns,
             a.memory_bytes, a.storage_bytes, a.power_mw)
                .cmp(&(&b.task_id, &b.node_id, b.start_ns, b.end_ns,
                       b.memory_bytes, b.storage_bytes, b.power_mw))
        });
        let mut staged = Vec::with_capacity(phases.len());
        let mut seen = BTreeSet::new();
        for phase in &phases {
            let decision = if phase.task_id.is_empty() || phase.start_ns >= phase.end_ns {
                Decision::RejectedInvalidInterval
            } else if !self.capacities.contains_key(&phase.node_id) {
                Decision::RejectedUnknownNode
            } else if !seen.insert(phase.task_id.clone())
                || self.retired_tasks.contains(&phase.task_id)
                || self.admitted.iter().any(|r| r.task_id == phase.task_id)
            {
                Decision::RejectedDuplicateTask
            } else if self.fits(phase) {
                self.admitted.push(phase.clone());
                Decision::Admitted
            } else {
                Decision::RejectedCapacity
            };
            staged.push(Admission {
                task_id: phase.task_id.clone(),
                node_id: phase.node_id.clone(),
                decision: decision.clone(),
            });
            if decision != Decision::Admitted {
                // Roll back only this bundle, preserving all prior reservations.
                for accepted in &staged {
                    if accepted.decision == Decision::Admitted {
                        self.admitted.retain(|r| r.task_id != accepted.task_id);
                    }
                }
                return staged;
            }
        }
        staged
    }

    /// Truncate a modeled reservation at a failure/cancellation timestamp.
    /// The caller supplies virtual time; this ledger does not advance the clock.
    /// Historical occupancy before `at_ns` remains committed. A task identity
    /// cannot be reused after cancellation; a retry must use a new attempt ID.
    /// Unknown, already-ended and repeated cancellations do not mutate state.
    pub fn terminate_at(&mut self, task_id: &str, at_ns: u64) -> bool {
        if self.retired_tasks.contains(task_id) {
            return false;
        }
        let Some(index) = self.admitted.iter().position(|r| r.task_id == task_id) else {
            return false;
        };
        if at_ns >= self.admitted[index].end_ns {
            return false;
        }
        if at_ns <= self.admitted[index].start_ns {
            self.admitted.remove(index);
        } else {
            self.admitted[index].end_ns = at_ns;
        }
        self.retired_tasks.insert(task_id.to_owned());
        true
    }

    fn fits(&self, request: &Request) -> bool {
        let capacity = &self.capacities[&request.node_id];
        // Resource usage changes only at interval boundaries. For each start
        // boundary, check all live reservations, including the candidate.
        let mut boundaries = vec![request.start_ns];
        for r in self.admitted.iter().filter(|r| {
            r.node_id == request.node_id
                && r.start_ns < request.end_ns
                && request.start_ns < r.end_ns
        }) {
            boundaries.push(r.start_ns.max(request.start_ns));
            boundaries.push(r.end_ns.min(request.end_ns));
        }
        boundaries.sort_unstable();
        boundaries.dedup();
        boundaries
            .into_iter()
            .filter(|&t| t < request.end_ns)
            .all(|t| {
                let mut memory = u128::from(request.memory_bytes);
                let mut storage = u128::from(request.storage_bytes);
                let mut power = u128::from(request.power_mw);
                for r in self
                    .admitted
                    .iter()
                    .filter(|r| r.node_id == request.node_id && r.start_ns <= t && t < r.end_ns)
                {
                    memory += u128::from(r.memory_bytes);
                    storage += u128::from(r.storage_bytes);
                    power += u128::from(r.power_mw);
                }
                memory <= u128::from(capacity.memory_bytes)
                    && storage <= u128::from(capacity.storage_bytes)
                    && power <= u128::from(capacity.power_mw)
            })
    }
}

/// Stable admission order independent of input enumeration. Equal release
/// timestamps are resolved by task ID, then node ID.
pub fn admit_deterministic(
    capacities: BTreeMap<String, Capacity>,
    mut requests: Vec<Request>,
) -> (Vec<Admission>, Vec<Request>) {
    // A task ID may appear more than once with conflicting payloads. Include
    // every admission-relevant field in the tie break so a reversed input
    // cannot change which attempt wins the unique task identity.
    requests.sort_by(|a, b| {
        (
            a.start_ns,
            &a.task_id,
            &a.node_id,
            a.end_ns,
            a.memory_bytes,
            a.storage_bytes,
            a.power_mw,
        )
            .cmp(&(
                b.start_ns,
                &b.task_id,
                &b.node_id,
                b.end_ns,
                b.memory_bytes,
                b.storage_bytes,
                b.power_mw,
            ))
    });
    let mut ledger = Ledger::new(capacities);
    let decisions = requests.into_iter().map(|r| ledger.admit(r)).collect();
    (decisions, ledger.admitted)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn capacities() -> BTreeMap<String, Capacity> {
        BTreeMap::from([
            (
                "sat-a".into(),
                Capacity {
                    memory_bytes: 10,
                    storage_bytes: 10,
                    power_mw: 10,
                },
            ),
            (
                "sat-b".into(),
                Capacity {
                    memory_bytes: 10,
                    storage_bytes: 10,
                    power_mw: 10,
                },
            ),
        ])
    }
    fn req(id: &str, node: &str, start: u64, end: u64, usage: u64) -> Request {
        Request {
            task_id: id.into(),
            node_id: node.into(),
            start_ns: start,
            end_ns: end,
            memory_bytes: usage,
            storage_bytes: usage,
            power_mw: usage,
        }
    }

    #[test]
    fn competing_tasks_are_admitted_in_stable_order() {
        let requests = vec![
            req("b", "sat-a", 0, 10, 7),
            req("a", "sat-a", 0, 10, 7),
            req("c", "sat-b", 0, 10, 7),
        ];
        let (first, admitted) = admit_deterministic(capacities(), requests.clone());
        let (second, _) = admit_deterministic(capacities(), requests.into_iter().rev().collect());
        assert_eq!(first, second);
        assert_eq!(
            first.iter().map(|a| &a.decision).collect::<Vec<_>>(),
            vec![
                &Decision::Admitted,
                &Decision::RejectedCapacity,
                &Decision::Admitted
            ]
        );
        assert_eq!(admitted.len(), 2);
    }

    #[test]
    fn conflicting_same_identity_requests_are_permutation_invariant() {
        // Same release, task ID and node but different resource demand.
        // Previously the stable sort preserved input order for this tie,
        // changing both the admitted request and later capacity decisions.
        let requests = vec![
            req("same", "sat-a", 0, 20, 9),
            req("same", "sat-a", 0, 10, 4),
            req("other", "sat-a", 1, 9, 6),
        ];
        let first = admit_deterministic(capacities(), requests.clone());
        let reversed = admit_deterministic(capacities(), requests.into_iter().rev().collect());
        assert_eq!(first, reversed);
        assert_eq!(
            first.0.iter().map(|a| &a.decision).collect::<Vec<_>>(),
            vec![
                &Decision::Admitted,
                &Decision::RejectedDuplicateTask,
                &Decision::Admitted
            ]
        );
        assert_eq!(first.1[0].memory_bytes, 4);
        assert_eq!(first.1[0].end_ns, 10);
    }

    #[test]
    fn half_open_intervals_release_capacity_at_exact_end() {
        let mut ledger = Ledger::new(capacities());
        assert_eq!(
            ledger.admit(req("a", "sat-a", 0, 10, 10)).decision,
            Decision::Admitted
        );
        assert_eq!(
            ledger.admit(req("b", "sat-a", 10, 20, 10)).decision,
            Decision::Admitted
        );
        assert_eq!(
            ledger.admit(req("c", "sat-a", 9, 11, 1)).decision,
            Decision::RejectedCapacity
        );
        assert_eq!(ledger.admitted().len(), 2);
    }

    #[test]
    fn overlapping_nonuniform_usage_checks_every_boundary() {
        let mut ledger = Ledger::new(capacities());
        assert_eq!(
            ledger.admit(req("a", "sat-a", 0, 5, 6)).decision,
            Decision::Admitted
        );
        assert_eq!(
            ledger.admit(req("b", "sat-a", 5, 10, 6)).decision,
            Decision::Admitted
        );
        assert_eq!(
            ledger.admit(req("c", "sat-a", 2, 8, 5)).decision,
            Decision::RejectedCapacity
        );
        assert_eq!(
            ledger.admit(req("d", "sat-a", 2, 8, 4)).decision,
            Decision::Admitted
        );
    }

    #[test]
    fn invalid_and_duplicate_requests_do_not_mutate_ledger() {
        let mut ledger = Ledger::new(capacities());
        assert_eq!(
            ledger.admit(req("a", "sat-a", 0, 10, 1)).decision,
            Decision::Admitted
        );
        assert_eq!(
            ledger.admit(req("a", "sat-b", 10, 20, 1)).decision,
            Decision::RejectedDuplicateTask
        );
        assert_eq!(
            ledger.admit(req("b", "sat-a", 10, 10, 1)).decision,
            Decision::RejectedInvalidInterval
        );
        assert_eq!(
            ledger.admit(req("c", "missing", 10, 20, 1)).decision,
            Decision::RejectedUnknownNode
        );
        assert_eq!(ledger.admitted().len(), 1);
    }

    #[test]
    fn failure_truncates_future_capacity_but_preserves_past_occupancy() {
        let mut ledger = Ledger::new(capacities());
        assert_eq!(
            ledger.admit(req("task-1", "sat-a", 0, 100, 10)).decision,
            Decision::Admitted
        );
        assert!(ledger.terminate_at("task-1", 40));
        assert_eq!(ledger.admitted()[0].end_ns, 40);
        assert_eq!(
            ledger.admit(req("past", "sat-a", 20, 30, 1)).decision,
            Decision::RejectedCapacity
        );
        assert_eq!(
            ledger.admit(req("retry-2", "sat-a", 40, 100, 10)).decision,
            Decision::Admitted
        );
        assert_eq!(
            ledger.admit(req("task-1", "sat-b", 40, 100, 1)).decision,
            Decision::RejectedDuplicateTask
        );
        assert!(!ledger.terminate_at("task-1", 50));
        assert!(!ledger.terminate_at("missing", 50));
    }

    #[test]
    fn prestart_cancellation_releases_all_capacity_without_reusing_identity() {
        let mut ledger = Ledger::new(capacities());
        assert_eq!(
            ledger.admit(req("queued", "sat-a", 50, 100, 10)).decision,
            Decision::Admitted
        );
        assert!(ledger.terminate_at("queued", 10));
        assert!(ledger.admitted().is_empty());
        assert_eq!(
            ledger
                .admit(req("replacement", "sat-a", 50, 100, 10))
                .decision,
            Decision::Admitted
        );
        assert_eq!(
            ledger.admit(req("queued", "sat-b", 50, 100, 1)).decision,
            Decision::RejectedDuplicateTask
        );
        assert!(!ledger.terminate_at("replacement", 100));
        assert_eq!(ledger.admitted()[0].end_ns, 100);
    }

    #[test]
    fn phased_storage_holds_capacity_after_cpu_releases() {
        let mut ledger = Ledger::new(capacities());
        let mut cpu = req("job:cpu", "sat-a", 0, 10, 0);
        cpu.memory_bytes = 10;
        cpu.power_mw = 10;
        let mut output = req("job:output", "sat-a", 10, 40, 0);
        output.storage_bytes = 10;
        assert!(ledger.admit_bundle(vec![output, cpu]).iter().all(|a| a.decision == Decision::Admitted));
        let mut next = req("next", "sat-a", 10, 20, 0);
        next.memory_bytes = 10;
        next.power_mw = 10;
        assert_eq!(ledger.admit(next).decision, Decision::Admitted);
        let mut competing = req("competing", "sat-a", 20, 30, 0);
        competing.storage_bytes = 1;
        assert_eq!(ledger.admit(competing).decision, Decision::RejectedCapacity);
        let mut after = req("after", "sat-a", 40, 50, 0);
        after.storage_bytes = 10;
        assert_eq!(ledger.admit(after).decision, Decision::Admitted);
    }

    #[test]
    fn failed_bundle_rolls_back_all_phases_and_is_permutation_invariant() {
        let phases = vec![
            req("a:cpu", "sat-a", 0, 10, 10),
            req("a:output", "sat-a", 10, 30, 11),
        ];
        for candidate in [phases.clone(), phases.into_iter().rev().collect()] {
            let mut ledger = Ledger::new(capacities());
            let result = ledger.admit_bundle(candidate);
            assert!(result.iter().any(|a| a.decision == Decision::RejectedCapacity));
            assert!(ledger.admitted().is_empty());
            assert_eq!(ledger.admit(req("replacement", "sat-a", 0, 30, 10)).decision, Decision::Admitted);
        }
    }

    #[test]
    fn large_resource_values_cannot_overflow_admission() {
        let mut ledger = Ledger::new(BTreeMap::from([(
            "sat".into(),
            Capacity {
                memory_bytes: u64::MAX,
                storage_bytes: u64::MAX,
                power_mw: u64::MAX,
            },
        )]));
        assert_eq!(
            ledger.admit(req("a", "sat", 0, 10, u64::MAX)).decision,
            Decision::Admitted
        );
        assert_eq!(
            ledger.admit(req("b", "sat", 0, 10, 1)).decision,
            Decision::RejectedCapacity
        );
    }
}
