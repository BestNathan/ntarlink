//! C1 opt-in batch execution: modeled local compute with one shared, authoritative
//! L1 packet event run for all output artifacts and concurrent background traffic.
//! CPU service and power are synthetic S1 assumptions, never hardware measurements.

use crate::admission::Decision;
use crate::planning::{plan_shared, Candidate};
use crate::{ComputeNode, ComputeWorkload, DataArtifact, ResourceReservation};
use ntarlink_contracts::{Flow, NodeKind, Scenario};
use ntarlink_packet_runtime::{run, PacketStatus, SimulationReport};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BatchJob {
    pub node_id: String,
    pub workload: ComputeWorkload,
    pub input_artifact: DataArtifact,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BatchExperiment {
    pub schema_version: u32,
    pub network: Scenario,
    pub compute_nodes: Vec<ComputeNode>,
    pub ground_site_id: String,
    pub packet_payload_bytes: u64,
    pub jobs: Vec<BatchJob>,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BatchStatus {
    Rejected,
    Completed,
    TransferFailed,
    DeadlineMissed,
}

#[derive(Debug, Serialize)]
pub struct BatchTaskResult {
    pub task_id: String,
    pub node_id: String,
    pub status: BatchStatus,
    pub reason: Option<String>,
    pub compute_completed_ns: Option<u64>,
    pub artifact_bytes: u64,
    pub injected_packets: usize,
    pub delivered_packets: usize,
    pub dropped_packets: usize,
    pub delivered_bytes: u64,
    pub dropped_bytes: u64,
    pub ground_arrival_ns: Option<u64>,
    pub reservation: Option<ResourceReservation>,
}

#[derive(Debug, Serialize)]
pub struct BatchReport {
    pub scenario: String,
    pub fidelity: &'static str,
    pub tasks: Vec<BatchTaskResult>,
    /// One L1 event run; all accepted workloads compete for actual shared link queues.
    pub network: SimulationReport,
}

impl BatchExperiment {
    pub fn from_json(raw: &str) -> Result<Self, String> {
        let experiment: Self = serde_json::from_str(raw).map_err(|e| e.to_string())?;
        experiment.validate()?;
        Ok(experiment)
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != 1 {
            return Err("only batch schema_version 1 is supported".into());
        }
        self.network.validate()?;
        if !(1..=65_535).contains(&self.packet_payload_bytes)
            || self.compute_nodes.is_empty()
            || self.jobs.is_empty()
            || self.jobs.len() > 10_000
        {
            return Err("invalid batch resource, job count or packet payload".into());
        }
        if !self
            .network
            .nodes
            .iter()
            .any(|n| n.id == self.ground_site_id && n.kind == NodeKind::GroundStation)
        {
            return Err("batch destination must be a network ground station".into());
        }
        let mut node_ids = BTreeSet::new();
        for node in &self.compute_nodes {
            if node.id.is_empty()
                || !node_ids.insert(node.id.as_str())
                || node.network_endpoint_id != node.site_id
                || !self
                    .network
                    .nodes
                    .iter()
                    .any(|n| n.id == node.site_id && n.kind == NodeKind::Satellite)
                || node.cpu_units_per_second == 0
                || node.memory_bytes == 0
                || node.storage_bytes == 0
                || node.power_budget_mw == 0
            {
                return Err("invalid orbital compute-node inventory".into());
            }
        }
        let mut job_ids = BTreeSet::new();
        let mut flow_budget: u64 = self
            .network
            .flows
            .iter()
            .map(|f| u64::from(f.packet_count))
            .sum();
        for job in &self.jobs {
            let task = &job.workload;
            let input = &job.input_artifact;
            let node = self.compute_nodes.iter().find(|n| n.id == job.node_id);
            if task.id.is_empty()
                || !job_ids.insert(task.id.as_str())
                || task.input_artifact_id != input.id
                || input.id.is_empty()
                || input.content_digest.is_empty()
                || input.size_bytes == 0
                || task.cpu_service_units == 0
                || task.memory_bytes == 0
                || task.power_demand_mw == 0
                || task.output_size_bytes == 0
                || task.release_ns >= task.deadline_ns
                || task.deadline_ns > self.network.duration_ns
                || node.is_none_or(|n| n.site_id != input.origin_site_id)
            {
                return Err("invalid batch workload or nonlocal input artifact".into());
            }
            for suffix in ["full", "tail"] {
                let id = format!("__c1:{}:{suffix}", task.id);
                if self.network.flows.iter().any(|f| f.id == id) {
                    return Err("batch flow ID collides with existing traffic".into());
                }
            }
            let packets = task.output_size_bytes.div_ceil(self.packet_payload_bytes);
            flow_budget = flow_budget.saturating_add(packets);
            if flow_budget > 1_000_000 || packets > u64::from(u32::MAX) {
                return Err("batch packet event budget exceeded".into());
            }
        }
        Ok(())
    }
}

pub fn run_batch(experiment: &BatchExperiment) -> Result<BatchReport, String> {
    experiment.validate()?;
    let candidates = experiment
        .jobs
        .iter()
        .map(|job| Candidate {
            node_id: job.node_id.clone(),
            workload: job.workload.clone(),
            input_bytes: job.input_artifact.size_bytes,
        })
        .collect();
    let planned = plan_shared(
        &experiment.compute_nodes,
        candidates,
        experiment.network.duration_ns,
    )?;
    let jobs: BTreeMap<_, _> = experiment
        .jobs
        .iter()
        .map(|job| (job.workload.id.as_str(), job))
        .collect();
    let mut network = experiment.network.clone();
    let mut tasks = Vec::with_capacity(planned.len());
    let mut transfer_flows = BTreeMap::new();
    for plan in planned {
        let job = jobs[plan.admission.task_id.as_str()];
        let mut task = BatchTaskResult {
            task_id: job.workload.id.clone(),
            node_id: job.node_id.clone(),
            status: BatchStatus::Rejected,
            reason: plan.reason,
            compute_completed_ns: plan.end_ns,
            artifact_bytes: job.workload.output_size_bytes,
            injected_packets: 0,
            delivered_packets: 0,
            dropped_packets: 0,
            delivered_bytes: 0,
            dropped_bytes: 0,
            ground_arrival_ns: None,
            reservation: plan.reservation,
        };
        if plan.admission.decision == Decision::Admitted {
            let end_ns = plan.end_ns.ok_or("admitted workload missing compute completion")?;
            let count = task.artifact_bytes.div_ceil(experiment.packet_payload_bytes);
            let last_at = end_ns.checked_add(count - 1);
            if last_at.is_none_or(|at| at >= network.duration_ns) {
                task.status = BatchStatus::DeadlineMissed;
                task.reason = Some("output injection extends past simulation horizon".into());
            } else {
                let full = task.artifact_bytes / experiment.packet_payload_bytes;
                let tail = task.artifact_bytes % experiment.packet_payload_bytes;
                let node = experiment
                    .compute_nodes
                    .iter()
                    .find(|n| n.id == job.node_id)
                    .ok_or("admitted node disappeared")?;
                for (suffix, bytes, packets, start) in [
                    ("full", experiment.packet_payload_bytes, full, end_ns),
                    ("tail", tail, u64::from(tail > 0), end_ns + full),
                ] {
                    if packets == 0 {
                        continue;
                    }
                    let id = format!("__c1:{}:{suffix}", task.task_id);
                    network.flows.push(Flow {
                        id: id.clone(),
                        source: node.site_id.clone(),
                        destination: experiment.ground_site_id.clone(),
                        packet_size_bytes: bytes,
                        packet_count: u32::try_from(packets).map_err(|_| "batch packet overflow")?,
                        start_ns: start,
                        interval_ns: u64::from(packets > 1),
                    });
                    transfer_flows.insert(id, (tasks.len(), bytes));
                }
                task.status = BatchStatus::Completed;
            }
        }
        tasks.push(task);
    }
    // Crucial: one packet runtime and one set of queues/routes/faults for the whole batch.
    let packet_report = run(&network)?;
    for packet in &packet_report.packets {
        let Some(&(index, bytes)) = transfer_flows.get(&packet.flow_id) else {
            continue;
        };
        let task = &mut tasks[index];
        task.injected_packets += 1;
        match packet.status {
            PacketStatus::Delivered => {
                task.delivered_packets += 1;
                task.delivered_bytes += bytes;
                let at = packet.completed_ns.ok_or("delivered packet has no arrival")?;
                task.ground_arrival_ns = Some(task.ground_arrival_ns.map_or(at, |old| old.max(at)));
            }
            PacketStatus::Dropped => {
                task.dropped_packets += 1;
                task.dropped_bytes += bytes;
            }
            PacketStatus::InFlight => return Err("unaccounted batch packet".into()),
        }
    }
    for task in &mut tasks {
        if task.status != BatchStatus::Completed {
            continue;
        }
        if task.delivered_bytes + task.dropped_bytes != task.artifact_bytes
            || task.delivered_packets + task.dropped_packets != task.injected_packets
        {
            return Err("shared batch artifact accounting invariant failed".into());
        }
        if task.delivered_bytes != task.artifact_bytes {
            task.status = BatchStatus::TransferFailed;
            task.reason = Some("modeled artifact lost in shared packet network".into());
        } else if task.ground_arrival_ns.is_none_or(|at| {
            at > jobs[task.task_id.as_str()].workload.deadline_ns
        }) {
            task.status = BatchStatus::DeadlineMissed;
            task.reason = Some("network artifact arrived after deadline".into());
        }
    }
    Ok(BatchReport {
        scenario: network.name.clone(),
        fidelity: "s1_modeled_local_compute_shared_l1_packet_runtime",
        tasks,
        network: packet_report,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> BatchExperiment {
        BatchExperiment::from_json(include_str!("../../../scenarios/c1-shared-network.json"))
            .unwrap()
    }

    #[test]
    fn competing_satellites_share_real_packet_queues_and_exact_bytes() {
        let report = run_batch(&fixture()).unwrap();
        assert_eq!(report.tasks.len(), 4);
        assert_eq!(report.tasks[0].status, BatchStatus::Completed);
        assert_eq!(report.tasks[1].status, BatchStatus::Rejected);
        assert_eq!(report.tasks[2].status, BatchStatus::Completed);
        assert_eq!(report.tasks[3].status, BatchStatus::Completed);
        assert_eq!(report.network.injected, report.network.delivered + report.network.dropped);
        for task in report.tasks.iter().filter(|t| t.status == BatchStatus::Completed) {
            assert_eq!(task.delivered_bytes, task.artifact_bytes);
            assert_eq!(task.dropped_bytes, 0);
            assert_eq!(task.delivered_packets, task.injected_packets);
        }
        let accepted = report.tasks.iter().filter(|t| t.reservation.is_some()).count();
        let artifact_packets = report
            .network
            .packets
            .iter()
            .filter(|p| p.flow_id.starts_with("__c1:"))
            .count();
        assert_eq!(accepted, 3);
        assert_eq!(
            artifact_packets,
            report.tasks.iter().map(|t| t.injected_packets).sum::<usize>()
        );
        let a_hops: Vec<_> = report.network.packets.iter()
            .filter(|p| p.flow_id.starts_with("__c1:a:"))
            .flat_map(|p| p.hops.iter().map(|h| h.link_id.as_str()))
            .collect();
        assert!(a_hops.contains(&"isl-a-b"));
        assert!(a_hops.contains(&"feeder-b-gw"));
    }

    #[test]
    fn all_artifacts_use_one_network_run_and_replay_is_order_independent() {
        let mut experiment = fixture();
        let baseline = serde_json::to_vec(&run_batch(&experiment).unwrap()).unwrap();
        experiment.jobs.reverse();
        let reversed = serde_json::to_vec(&run_batch(&experiment).unwrap()).unwrap();
        assert_eq!(baseline, reversed);
    }

    #[test]
    fn common_feeder_failure_affects_both_satellites_without_fake_delivery() {
        let mut experiment = fixture();
        experiment.network.links.iter_mut().find(|l| l.id == "feeder-b-gw").unwrap().up = false;
        let report = run_batch(&experiment).unwrap();
        for task in report.tasks.iter().filter(|t| t.reservation.is_some()) {
            assert_eq!(task.status, BatchStatus::TransferFailed);
            assert_eq!(task.delivered_bytes, 0);
            assert_eq!(task.dropped_bytes, task.artifact_bytes);
        }
        assert_eq!(report.network.injected, report.network.delivered + report.network.dropped);
    }

    #[test]
    fn workload_validation_rejects_nonlocal_inputs_and_duplicate_task_ids() {
        let mut experiment = fixture();
        experiment.jobs[0].input_artifact.origin_site_id = "sat-b".into();
        assert!(experiment.validate().is_err());
        experiment.jobs[0].input_artifact.origin_site_id = "sat-a".into();
        experiment.jobs[1].workload.id = experiment.jobs[0].workload.id.clone();
        assert!(experiment.validate().is_err());
    }
}
