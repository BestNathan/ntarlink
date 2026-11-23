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
    candidates: Vec<Candidate>,
    input_arrivals_ns: &BTreeMap<String, u64>,
    horizon_ns: u64,
) -> Result<Vec<PlannedWorkload>, String> {
    plan_shared_internal(
        nodes,
        candidates,
        input_arrivals_ns,
        &BTreeMap::new(),
        horizon_ns,
        false,
    )
}

/// Conservatively retain modeled output storage until the simulation horizon.
/// The final packet run remains authoritative for delivery; this planner never
/// predicts an earlier release from a speculative route or packet receipt.
pub fn plan_shared_with_output_staging(
    nodes: &[ComputeNode],
    candidates: Vec<Candidate>,
    input_arrivals_ns: &BTreeMap<String, u64>,
    horizon_ns: u64,
) -> Result<Vec<PlannedWorkload>, String> {
    plan_shared_internal(
        nodes,
        candidates,
        input_arrivals_ns,
        &BTreeMap::new(),
        horizon_ns,
        true,
    )
}

/// Plan output staging using *observed complete packet delivery* times supplied by
/// the authoritative packet runtime. Missing observations (including loss or
/// incomplete transfer) retain storage until the horizon; no route prediction
/// releases capacity. Callers must supply observations from the same scenario
/// and virtual clock, after packet execution, not from a speculative plan.
pub fn plan_shared_with_observed_deliveries(
    nodes: &[ComputeNode],
    candidates: Vec<Candidate>,
    input_arrivals_ns: &BTreeMap<String, u64>,
    output_deliveries_ns: &BTreeMap<String, u64>,
    horizon_ns: u64,
) -> Result<Vec<PlannedWorkload>, String> {
    plan_shared_internal(
        nodes,
        candidates,
        input_arrivals_ns,
        output_deliveries_ns,
        horizon_ns,
        true,
    )
}

fn plan_shared_internal(
    nodes: &[ComputeNode],
    mut candidates: Vec<Candidate>,
    input_arrivals_ns: &BTreeMap<String, u64>,
    output_deliveries_ns: &BTreeMap<String, u64>,
    horizon_ns: u64,
    stage_output: bool,
) -> Result<Vec<PlannedWorkload>, String> {
    let mut capacities = BTreeMap::new();
    // Resolve each candidate against the validated inventory in logarithmic
    // time instead of rescanning all nodes for every workload in large C1 runs.
    let mut node_index = BTreeMap::new();
    for node in nodes {
        if node.id.is_empty() || node_index.insert(node.id.as_str(), node).is_some() {
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
            a.workload.release_ns.max(
                input_arrivals_ns
                    .get(&a.workload.id)
                    .copied()
                    .unwrap_or(a.workload.release_ns),
            ),
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
                b.workload.release_ns.max(
                    input_arrivals_ns
                        .get(&b.workload.id)
                        .copied()
                        .unwrap_or(b.workload.release_ns),
                ),
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
            input_arrivals_ns
                .get(&task.id)
                .copied()
                .unwrap_or(task.release_ns),
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
        } else if let Some(node) = node_index.get(candidate.node_id.as_str()) {
            match reserve(node, task, candidate.input_bytes) {
                Ok(reservation) => {
                    if let Some(end) = start.checked_add(reservation.modeled_cpu_duration_ns) {
                        if end <= task.deadline_ns && end < horizon_ns {
                            let execution = Request {
                                task_id: if stage_output {
                                    format!("phase:{}:{}:execution", task.id.len(), task.id)
                                } else {
                                    task.id.clone()
                                },
                                node_id: node.id.clone(),
                                start_ns: start,
                                end_ns: end,
                                memory_bytes: task.memory_bytes,
                                storage_bytes: reservation.peak_storage_bytes,
                                power_mw: task.power_demand_mw,
                            };
                            outcome.admission = if stage_output {
                                let output = Request {
                                    task_id: format!("phase:{}:{}:output", task.id.len(), task.id),
                                    node_id: node.id.clone(),
                                    start_ns: end,
                                    end_ns: match output_deliveries_ns.get(&task.id) {
                                        Some(&delivered) if delivered >= end && delivered <= horizon_ns => delivered,
                                        Some(_) => return Err(format!(
                                            "output delivery observation outside execution/horizon for {}",
                                            task.id
                                        )),
                                        None => horizon_ns,
                                    },
                                    memory_bytes: 0,
                                    storage_bytes: task.output_size_bytes,
                                    power_mw: 0,
                                };
                                let phases = ledger.admit_bundle(vec![execution, output]);
                                let decision = if phases
                                    .iter()
                                    .all(|p| p.decision == Decision::Admitted)
                                {
                                    Decision::Admitted
                                } else {
                                    phases
                                        .iter()
                                        .find(|p| p.decision != Decision::RejectedBundle)
                                        .map_or(Decision::RejectedCapacity, |p| p.decision.clone())
                                };
                                Admission {
                                    task_id: task.id.clone(),
                                    node_id: node.id.clone(),
                                    decision,
                                }
                            } else {
                                ledger.admit(execution)
                            };
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
    fn large_permuted_inventory_preserves_admission_and_unknown_node_rejection() {
        let scenario = fixture();
        let nodes: Vec<_> = (0..1_000)
            .map(|index| {
                let mut node = scenario.orbital_node.clone();
                node.id = format!("compute-{index:04}");
                node
            })
            .collect();
        let candidates: Vec<_> = (0..1_000)
            .map(|index| {
                let mut workload = scenario.workload.clone();
                workload.id = format!("task-{index:04}");
                Candidate {
                    node_id: format!("compute-{index:04}"),
                    workload,
                    input_bytes: scenario.input_artifact.size_bytes,
                }
            })
            .collect();
        let first = plan_shared(&nodes, candidates.clone(), scenario.network.duration_ns).unwrap();
        let reversed = plan_shared(
            &nodes.into_iter().rev().collect::<Vec<_>>(),
            candidates.into_iter().rev().collect(),
            scenario.network.duration_ns,
        )
        .unwrap();
        assert_eq!(
            serde_json::to_vec(&first).unwrap(),
            serde_json::to_vec(&reversed).unwrap()
        );
        assert!(first
            .iter()
            .all(|entry| entry.admission.decision == Decision::Admitted));
        let mut missing = scenario.workload.clone();
        missing.id = "unknown-at-scale".into();
        let rejected = plan_shared(
            std::slice::from_ref(&scenario.orbital_node),
            vec![Candidate {
                node_id: "missing".into(),
                workload: missing,
                input_bytes: 1,
            }],
            scenario.network.duration_ns,
        )
        .unwrap();
        assert_eq!(
            rejected[0].admission.decision,
            Decision::RejectedUnknownNode
        );
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
            std::slice::from_ref(&scenario.orbital_node),
            vec![make("ready"), make("late")],
            &arrivals,
            scenario.network.duration_ns,
        )
        .unwrap();
        let ready = results
            .iter()
            .find(|r| r.admission.task_id == "ready")
            .unwrap();
        let late = results
            .iter()
            .find(|r| r.admission.task_id == "late")
            .unwrap();
        assert_eq!(ready.start_ns, 5_000_000);
        assert_eq!(ready.end_ns, Some(5_000_000 + duration));
        assert_eq!(ready.admission.decision, Decision::Admitted);
        assert_eq!(late.admission.decision, Decision::RejectedInvalidInterval);
        assert!(late.reservation.is_none());
        let reversed = plan_shared_with_arrivals(
            std::slice::from_ref(&scenario.orbital_node),
            vec![make("late"), make("ready")],
            &arrivals,
            scenario.network.duration_ns,
        )
        .unwrap();
        assert_eq!(
            serde_json::to_vec(&results).unwrap(),
            serde_json::to_vec(&reversed).unwrap()
        );
    }

    #[test]
    fn output_staging_blocks_later_storage_even_after_cpu_finishes() {
        let scenario = fixture();
        let mut node = scenario.orbital_node.clone();
        let mut first = scenario.workload.clone();
        first.id = "first".into();
        first.release_ns = 0;
        let mut second = first.clone();
        second.id = "second".into();
        second.release_ns = 30_000_000;
        node.storage_bytes = first
            .output_size_bytes
            .max(scenario.input_artifact.size_bytes + first.output_size_bytes);
        let make = |workload| Candidate {
            node_id: node.id.clone(),
            workload,
            input_bytes: scenario.input_artifact.size_bytes,
        };
        let results = plan_shared_with_output_staging(
            std::slice::from_ref(&node),
            vec![make(first), make(second)],
            &BTreeMap::new(),
            scenario.network.duration_ns,
        )
        .unwrap();
        assert_eq!(results[0].admission.decision, Decision::Admitted);
        assert_eq!(results[1].admission.decision, Decision::RejectedCapacity);
    }

    #[test]
    fn observed_delivery_releases_storage_but_missing_delivery_does_not() {
        let scenario = fixture();
        let mut node = scenario.orbital_node.clone();
        let mut first = scenario.workload.clone();
        first.id = "first".into();
        first.release_ns = 0;
        let mut second = first.clone();
        second.id = "second".into();
        second.release_ns = 30_000_000;
        node.storage_bytes = scenario.input_artifact.size_bytes + first.output_size_bytes;
        let make = |workload| Candidate {
            node_id: node.id.clone(),
            workload,
            input_bytes: scenario.input_artifact.size_bytes,
        };
        let jobs = vec![make(first), make(second)];
        let deliveries = BTreeMap::from([("first".to_string(), 25_000_000)]);
        let planned = plan_shared_with_observed_deliveries(
            std::slice::from_ref(&node),
            jobs.clone(),
            &BTreeMap::new(),
            &deliveries,
            scenario.network.duration_ns,
        )
        .unwrap();
        assert_eq!(planned[0].admission.decision, Decision::Admitted);
        assert_eq!(planned[1].admission.decision, Decision::Admitted);
        let unknown = plan_shared_with_observed_deliveries(
            std::slice::from_ref(&node),
            jobs.clone(),
            &BTreeMap::new(),
            &BTreeMap::new(),
            scenario.network.duration_ns,
        )
        .unwrap();
        assert_eq!(unknown[1].admission.decision, Decision::RejectedCapacity);
        let invalid = BTreeMap::from([("first".to_string(), 1)]);
        assert!(plan_shared_with_observed_deliveries(
            &[node],
            jobs,
            &BTreeMap::new(),
            &invalid,
            scenario.network.duration_ns,
        )
        .is_err());
    }

    #[test]
    fn phase_ids_cannot_alias_other_workload_ids() {
        let scenario = fixture();
        let node = scenario.orbital_node.clone();
        let make = |id: &str, release_ns: u64| {
            let mut workload = scenario.workload.clone();
            workload.id = id.into();
            workload.release_ns = release_ns;
            Candidate {
                node_id: node.id.clone(),
                workload,
                input_bytes: scenario.input_artifact.size_bytes,
            }
        };
        let results = plan_shared_with_output_staging(
            std::slice::from_ref(&node),
            vec![make("a:execution", 0), make("a", 30_000_000)],
            &BTreeMap::new(),
            scenario.network.duration_ns,
        )
        .unwrap();
        assert_eq!(results.len(), 2);
        assert!(results
            .iter()
            .all(|result| { result.admission.decision != Decision::RejectedDuplicateTask }));
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
            std::slice::from_ref(&scenario.orbital_node),
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
