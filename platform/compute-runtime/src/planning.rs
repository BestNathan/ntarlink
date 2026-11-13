//! C1 modeled workload admission across shared compute nodes.
//! Planning only: packet transport and virtual-time execution remain authoritative elsewhere.

use crate::admission::{Admission, Capacity, Decision, Ledger, Request};
use crate::{reserve, ComputeNode, ComputeWorkload, ResourceReservation};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug)]
pub struct Candidate {
    pub node_id: String,
    pub workload: ComputeWorkload,
    pub input_bytes: u64,
}

#[derive(Debug, Serialize)]
pub struct PlannedWorkload {
    pub admission: Admission,
    pub reservation: Option<ResourceReservation>,
    pub start_ns: u64,
    pub end_ns: Option<u64>,
    pub reason: Option<String>,
}

/// Admit independent task attempts against shared memory, staging storage and power.
/// The returned decisions are ordered by virtual release, task ID and node ID,
/// regardless of input enumeration. This does not schedule packets, CPU contention,
/// transfer arrival or contact-aware placement; callers must supply input-ready times.
pub fn plan_shared(
    nodes: &[ComputeNode],
    candidates: Vec<Candidate>,
    horizon_ns: u64,
) -> Result<Vec<PlannedWorkload>, String> {
    plan_shared_with_arrivals(nodes, candidates, &BTreeMap::new(), horizon_ns)
}

/// Model transfer readiness as an external packet-runtime observation. Planning
/// never infers arrival from link geometry or advances the network clock.
/// Missing observations preserve the original release-time-only planning API.
pub fn plan_shared_with_arrivals(
    nodes: &[ComputeNode],
    mut candidates: Vec<Candidate>,
    input_arrivals_ns: &BTreeMap<String, u64>,
    horizon_ns: u64,
) -> Result<Vec<PlannedWorkload>, String> {
    let mut capacities = BTreeMap::new();
    for node in nodes {
        if node.id.is_empty() || capacities.contains_key(&node.id) {
            return Err("duplicate or empty compute node ID".into());
        }
        capacities.insert(
            node.id.clone(),
            Capacity {
                memory_bytes: node.memory_bytes,
                storage_bytes: node.storage_bytes,
                power_mw: node.power_budget_mw,
            },
        );
    }
    candidates.sort_by(|a, b| {
        (
            a.workload.release_ns,
            &a.workload.id,
            &a.node_id,
            a.workload.deadline_ns,
            a.workload.cpu_service_units,
            a.workload.memory_bytes,
            a.workload.power_demand_mw,
            a.workload.output_size_bytes,
            a.input_bytes,
        )
            .cmp(&(
                b.workload.release_ns,
                &b.workload.id,
                &b.node_id,
                b.workload.deadline_ns,
                b.workload.cpu_service_units,
                b.workload.memory_bytes,
                b.workload.power_demand_mw,
                b.workload.output_size_bytes,
                b.input_bytes,
            ))
    });
    let mut ledger = Ledger::new(capacities);
    let mut seen = BTreeSet::new();
    let mut results = Vec::with_capacity(candidates.len());
    for candidate in candidates {
        let task = &candidate.workload;
        let start = task.release_ns.max(
            input_arrivals_ns.get(&task.id).copied().unwrap_or(task.release_ns),
        );
        let mut outcome = PlannedWorkload {
            admission: Admission {
                task_id: task.id.clone(),
                node_id: candidate.node_id.clone(),
                decision: Decision::RejectedInvalidInterval,
            },
            reservation: None,
            start_ns: start,
            end_ns: None,
            reason: None,
        };
        // Reject duplicate attempts even when the earlier request was infeasible.
        if !seen.insert(task.id.clone()) {
            outcome.admission.decision = Decision::RejectedDuplicateTask;
            outcome.reason = Some("duplicate workload attempt ID".into());
        } else if task.id.is_empty() || start >= task.deadline_ns || task.deadline_ns > horizon_ns {
            outcome.reason = Some("invalid workload interval or horizon".into());
        } else if let Some(node) = nodes.iter().find(|n| n.id == candidate.node_id) {
            match reserve(node, task, candidate.input_bytes) {
                Ok(reservation) => {
                    if let Some(end) = start.checked_add(reservation.modeled_cpu_duration_ns) {
                        if end <= task.deadline_ns && end < horizon_ns {
                            outcome.admission = ledger.admit(Request {
                                task_id: task.id.clone(),
                                node_id: node.id.clone(),
                                start_ns: start,
                                end_ns: end,
                                memory_bytes: task.memory_bytes,
                                storage_bytes: reservation.peak_storage_bytes,
                                power_mw: task.power_demand_mw,
                            });
                            if outcome.admission.decision == Decision::Admitted {
                                outcome.reservation = Some(reservation);
                                outcome.end_ns = Some(end);
                            } else {
                                outcome.reason = Some("shared compute capacity exceeded".into());
                            }
                        } else {
                            outcome.reason =
                                Some("modeled execution misses deadline or horizon".into());
                        }
                    } else {
                        outcome.reason = Some("modeled execution time overflow".into());
                    }
                }
                Err(reason) => outcome.reason = Some(reason),
            }
        } else {
            outcome.admission.decision = Decision::RejectedUnknownNode;
            outcome.reason = Some("unknown compute node".into());
        }
        results.push(outcome);
    }
    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{run_experiment, ComputeExperiment, WorkloadStatus};

    fn fixture() -> ComputeExperiment {
        ComputeExperiment::from_json(include_str!("../../../scenarios/c0-orbital-edge.json"))
            .unwrap()
    }

    #[test]
    fn shared_capacity_is_deterministic_and_separate_across_nodes() {
        let scenario = fixture();
        let mut sat = scenario.orbital_node.clone();
        sat.memory_bytes = 150_000;
        let mut peer = sat.clone();
        peer.id = "peer-cpu".into();
        peer.site_id = "sat-b".into();
        peer.network_endpoint_id = "sat-b".into();
        let candidate = |id: &str, node_id: &str, release_ns: u64| {
            let mut workload = scenario.workload.clone();
            workload.id = id.into();
            workload.release_ns = release_ns;
            Candidate {
                node_id: node_id.into(),
                workload,
                input_bytes: scenario.input_artifact.size_bytes,
            }
        };
        let jobs = vec![
            candidate("b", &sat.id, 0),
            candidate("a", &sat.id, 0),
            candidate("c", &peer.id, 0),
            candidate("d", &sat.id, 30_000_000),
        ];
        let first = plan_shared(
            &[sat.clone(), peer.clone()],
            jobs.clone(),
            scenario.network.duration_ns,
        )
        .unwrap();
        let second = plan_shared(
            &[sat, peer],
            jobs.into_iter().rev().collect(),
            scenario.network.duration_ns,
        )
        .unwrap();
        assert_eq!(
            serde_json::to_vec(&first).unwrap(),
            serde_json::to_vec(&second).unwrap()
        );
        assert_eq!(
            first
                .iter()
                .map(|r| &r.admission.decision)
                .collect::<Vec<_>>(),
            vec![
                &Decision::Admitted,
                &Decision::RejectedCapacity,
                &Decision::Admitted,
                &Decision::Admitted
            ]
        );
        assert_eq!(first[0].end_ns, Some(20_000_000));
        assert_eq!(first[3].start_ns, 30_000_000);
    }

    #[test]
    fn observed_packet_arrival_delays_admission_and_can_miss_deadline() {
        let scenario = fixture();
        let make = |id: &str| {
            let mut workload = scenario.workload.clone();
            workload.id = id.into();
            Candidate {
                node_id: scenario.orbital_node.id.clone(),
                workload,
                input_bytes: scenario.input_artifact.size_bytes,
            }
        };
        let duration = 20_000_000;
        let arrivals = BTreeMap::from([
            ("late".to_owned(), scenario.workload.deadline_ns),
            ("ready".to_owned(), 5_000_000),
        ]);
        let results = plan_shared_with_arrivals(
            &[scenario.orbital_node.clone()],
            vec![make("ready"), make("late")],
            &arrivals,
            scenario.network.duration_ns,
        )
        .unwrap();
        let ready = results.iter().find(|r| r.admission.task_id == "ready").unwrap();
        let late = results.iter().find(|r| r.admission.task_id == "late").unwrap();
        assert_eq!(ready.start_ns, 5_000_000);
        assert_eq!(ready.end_ns, Some(5_000_000 + duration));
        assert_eq!(ready.admission.decision, Decision::Admitted);
        assert_eq!(late.admission.decision, Decision::RejectedInvalidInterval);
        assert!(late.reservation.is_none());
        let reversed = plan_shared_with_arrivals(
            &[scenario.orbital_node.clone()],
            vec![make("late"), make("ready")],
            &arrivals,
            scenario.network.duration_ns,
        )
        .unwrap();
        assert_eq!(serde_json::to_vec(&results).unwrap(), serde_json::to_vec(&reversed).unwrap());
    }

    #[test]
    fn admission_does_not_replace_packet_delivery_or_fault_evidence() {
        let mut scenario = fixture();
        let job = Candidate {
            node_id: scenario.orbital_node.id.clone(),
            workload: scenario.workload.clone(),
            input_bytes: scenario.input_artifact.size_bytes,
        };
        let plan = plan_shared(
            &[scenario.orbital_node.clone()],
            vec![job],
            scenario.network.duration_ns,
        )
        .unwrap();
        assert_eq!(plan[0].admission.decision, Decision::Admitted);
        scenario
            .network
            .links
            .iter_mut()
            .find(|l| l.id == "feeder-b-gw")
            .unwrap()
            .up = false;
        let report = run_experiment(&scenario).unwrap();
        assert_eq!(report.onboard_first.status, WorkloadStatus::TransferFailed);
        assert!(report.onboard_first.transfer.unwrap().dropped_bytes > 0);
    }
}
