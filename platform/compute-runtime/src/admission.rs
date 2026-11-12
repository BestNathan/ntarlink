//! Deterministic interval-based admission for modeled shared compute resources.
//! This is an S1 planning primitive, not a hardware scheduler or network authority.

use serde::Serialize;
use std::collections::BTreeMap;

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
}

impl Ledger {
    pub fn new(capacities: BTreeMap<String, Capacity>) -> Self {
        Self { capacities, admitted: Vec::new() }
    }

    pub fn admitted(&self) -> &[Request] {
        &self.admitted
    }

    pub fn admit(&mut self, request: Request) -> Admission {
        let decision = if request.task_id.is_empty()
            || request.start_ns >= request.end_ns
        {
            Decision::RejectedInvalidInterval
        } else if !self.capacities.contains_key(&request.node_id) {
            Decision::RejectedUnknownNode
        } else if self.admitted.iter().any(|r| r.task_id == request.task_id) {
            Decision::RejectedDuplicateTask
        } else if self.fits(&request) {
            self.admitted.push(request.clone());
            Decision::Admitted
        } else {
            Decision::RejectedCapacity
        };
        Admission { task_id: request.task_id, node_id: request.node_id, decision }
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
        boundaries.into_iter().filter(|&t| t < request.end_ns).all(|t| {
            let mut memory = u128::from(request.memory_bytes);
            let mut storage = u128::from(request.storage_bytes);
            let mut power = u128::from(request.power_mw);
            for r in self.admitted.iter().filter(|r| {
                r.node_id == request.node_id && r.start_ns <= t && t < r.end_ns
            }) {
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
    requests.sort_by(|a, b| {
        (a.start_ns, &a.task_id, &a.node_id)
            .cmp(&(b.start_ns, &b.task_id, &b.node_id))
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
            ("sat-a".into(), Capacity { memory_bytes: 10, storage_bytes: 10, power_mw: 10 }),
            ("sat-b".into(), Capacity { memory_bytes: 10, storage_bytes: 10, power_mw: 10 }),
        ])
    }
    fn req(id: &str, node: &str, start: u64, end: u64, usage: u64) -> Request {
        Request {
            task_id: id.into(), node_id: node.into(), start_ns: start,
            end_ns: end, memory_bytes: usage, storage_bytes: usage, power_mw: usage,
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
        assert_eq!(first.iter().map(|a| &a.decision).collect::<Vec<_>>(),
            vec![&Decision::Admitted, &Decision::RejectedCapacity, &Decision::Admitted]);
        assert_eq!(admitted.len(), 2);
    }

    #[test]
    fn half_open_intervals_release_capacity_at_exact_end() {
        let mut ledger = Ledger::new(capacities());
        assert_eq!(ledger.admit(req("a", "sat-a", 0, 10, 10)).decision, Decision::Admitted);
        assert_eq!(ledger.admit(req("b", "sat-a", 10, 20, 10)).decision, Decision::Admitted);
        assert_eq!(ledger.admit(req("c", "sat-a", 9, 11, 1)).decision, Decision::RejectedCapacity);
        assert_eq!(ledger.admitted().len(), 2);
    }

    #[test]
    fn overlapping_nonuniform_usage_checks_every_boundary() {
        let mut ledger = Ledger::new(capacities());
        assert_eq!(ledger.admit(req("a", "sat-a", 0, 5, 6)).decision, Decision::Admitted);
        assert_eq!(ledger.admit(req("b", "sat-a", 5, 10, 6)).decision, Decision::Admitted);
        assert_eq!(ledger.admit(req("c", "sat-a", 2, 8, 5)).decision, Decision::RejectedCapacity);
        assert_eq!(ledger.admit(req("d", "sat-a", 2, 8, 4)).decision, Decision::Admitted);
    }

    #[test]
    fn invalid_and_duplicate_requests_do_not_mutate_ledger() {
        let mut ledger = Ledger::new(capacities());
        assert_eq!(ledger.admit(req("a", "sat-a", 0, 10, 1)).decision, Decision::Admitted);
        assert_eq!(ledger.admit(req("a", "sat-b", 10, 20, 1)).decision, Decision::RejectedDuplicateTask);
        assert_eq!(ledger.admit(req("b", "sat-a", 10, 10, 1)).decision, Decision::RejectedInvalidInterval);
        assert_eq!(ledger.admit(req("c", "missing", 10, 20, 1)).decision, Decision::RejectedUnknownNode);
        assert_eq!(ledger.admitted().len(), 1);
    }

    #[test]
    fn large_resource_values_cannot_overflow_admission() {
        let mut ledger = Ledger::new(BTreeMap::from([
            ("sat".into(), Capacity {
                memory_bytes: u64::MAX, storage_bytes: u64::MAX, power_mw: u64::MAX
            })
        ]));
        assert_eq!(ledger.admit(req("a", "sat", 0, 10, u64::MAX)).decision, Decision::Admitted);
        assert_eq!(ledger.admit(req("b", "sat", 0, 10, 1)).decision, Decision::RejectedCapacity);
    }
}
