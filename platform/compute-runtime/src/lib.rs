//! Opt-in S1 modeled orbital workload and artifact transport, using the real packet engine.
//! CPU service units and power budgets are scenario assumptions, not hardware measurements.

use ntarlink_contracts::{Flow, NodeKind, Scenario};
use ntarlink_packet_runtime::{run, PacketStatus, SimulationReport};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

const NS_PER_SECOND: u128 = 1_000_000_000;

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ComputeNode {
    pub id: String,
    pub site_id: String,
    pub network_endpoint_id: String,
    pub cpu_units_per_second: u64,
    pub memory_bytes: u64,
    pub storage_bytes: u64,
    pub power_budget_mw: u64,
    #[serde(default = "default_available")]
    pub available: bool,
}

fn default_available() -> bool {
    true
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DataArtifact {
    pub id: String,
    pub origin_site_id: String,
    pub size_bytes: u64,
    /// External identity supplied by the experiment; the simulator does not invent content.
    pub content_digest: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ComputeWorkload {
    pub id: String,
    pub input_artifact_id: String,
    pub release_ns: u64,
    pub deadline_ns: u64,
    pub cpu_service_units: u64,
    pub memory_bytes: u64,
    pub power_demand_mw: u64,
    pub output_size_bytes: u64,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionProfile {
    S1Deterministic,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ComputeExperiment {
    pub schema_version: u32,
    pub execution_profile: ExecutionProfile,
    pub network: Scenario,
    pub orbital_node: ComputeNode,
    pub ground_node: ComputeNode,
    pub input_artifact: DataArtifact,
    pub workload: ComputeWorkload,
    pub packet_payload_bytes: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Placement {
    GroundOnly,
    OnboardFirst,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkloadStatus {
    Completed,
    Rejected,
    TransferFailed,
    DeadlineMissed,
}

#[derive(Debug, Serialize)]
pub struct ResourceReservation {
    pub node_id: String,
    pub cpu_service_units: u64,
    pub memory_bytes: u64,
    pub peak_storage_bytes: u64,
    pub power_demand_mw: u64,
    pub modeled_cpu_duration_ns: u64,
}

#[derive(Debug, Serialize)]
pub struct ProducedArtifact {
    pub id: String,
    pub derived_from: String,
    pub size_bytes: u64,
    pub site_id: String,
    pub produced_at_ns: u64,
}

#[derive(Debug, Serialize)]
pub struct ArtifactTransfer {
    pub artifact_id: String,
    pub from_site: String,
    pub to_site: String,
    pub injected_bytes: u64,
    pub delivered_bytes: u64,
    pub dropped_bytes: u64,
    pub injected_packets: usize,
    pub delivered_packets: usize,
    pub dropped_packets: usize,
    pub last_arrival_ns: Option<u64>,
}

#[derive(Debug, Serialize)]
pub struct PolicyOutcome {
    pub policy: Placement,
    pub status: WorkloadStatus,
    pub reason: Option<String>,
    pub node_id: String,
    pub started_ns: Option<u64>,
    pub compute_completed_ns: Option<u64>,
    pub completed_ns: Option<u64>,
    pub resource_reservation: Option<ResourceReservation>,
    pub produced_artifact: Option<ProducedArtifact>,
    pub transfer: Option<ArtifactTransfer>,
    /// Authoritative L1 packet report, including actual per-link queue/route/fault decisions.
    pub packet_report: Option<SimulationReport>,
}

impl PolicyOutcome {
    fn new(policy: Placement, node_id: &str) -> Self {
        Self {
            policy,
            status: WorkloadStatus::Rejected,
            reason: None,
            node_id: node_id.into(),
            started_ns: None,
            compute_completed_ns: None,
            completed_ns: None,
            resource_reservation: None,
            produced_artifact: None,
            transfer: None,
            packet_report: None,
        }
    }

    fn fail(&mut self, status: WorkloadStatus, reason: &str) {
        self.status = status;
        self.reason = Some(reason.into());
    }
}

#[derive(Debug, Serialize)]
pub struct ComputeReport {
    pub scenario: String,
    pub fidelity: &'static str,
    pub input_artifact_id: String,
    pub input_bytes: u64,
    pub output_bytes: u64,
    pub ground_only: PolicyOutcome,
    pub onboard_first: PolicyOutcome,
}

impl ComputeExperiment {
    pub fn from_json(raw: &str) -> Result<Self, String> {
        let scenario: Self = serde_json::from_str(raw).map_err(|err| err.to_string())?;
        scenario.validate()?;
        Ok(scenario)
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != 1 {
            return Err("only compute experiment schema_version 1 is supported".into());
        }
        self.network.validate()?;
        let sat = &self.orbital_node;
        let ground = &self.ground_node;
        for node in [sat, ground] {
            if node.id.is_empty()
                || node.site_id != node.network_endpoint_id
                || node.cpu_units_per_second == 0
                || node.memory_bytes == 0
                || node.storage_bytes == 0
                || node.power_budget_mw == 0
            {
                return Err(format!("invalid modeled compute node: {}", node.id));
            }
        }
        if sat.id == ground.id || sat.site_id == ground.site_id {
            return Err("compute nodes must be distinct".into());
        }
        let kind = |id: &str| {
            self.network
                .nodes
                .iter()
                .find(|node| node.id == id)
                .map(|node| &node.kind)
        };
        if kind(&sat.site_id) != Some(&NodeKind::Satellite)
            || kind(&ground.site_id) != Some(&NodeKind::GroundStation)
        {
            return Err("C0 requires satellite and ground-station network endpoints".into());
        }
        let input = &self.input_artifact;
        let task = &self.workload;
        if input.id.is_empty()
            || input.content_digest.is_empty()
            || input.origin_site_id != sat.site_id
            || input.size_bytes == 0
            || task.id.is_empty()
            || task.input_artifact_id != input.id
            || task.cpu_service_units == 0
            || task.memory_bytes == 0
            || task.power_demand_mw == 0
            || task.output_size_bytes == 0
            || task.output_size_bytes >= input.size_bytes
        {
            return Err("invalid input artifact or modeled workload".into());
        }
        if task.release_ns >= task.deadline_ns || task.deadline_ns > self.network.duration_ns {
            return Err("workload release/deadline outside virtual-time horizon".into());
        }
        if !(1..=65_535).contains(&self.packet_payload_bytes) {
            return Err("packet_payload_bytes must be between 1 and 65535".into());
        }
        // Bound each policy's generated packet budget before scheduling any network events.
        let total_packets = self
            .network
            .flows
            .iter()
            .map(|flow| u64::from(flow.packet_count))
            .sum::<u64>();
        for bytes in [input.size_bytes, task.output_size_bytes] {
            let chunks = bytes.div_ceil(self.packet_payload_bytes);
            if chunks > 1_000_000 || total_packets.saturating_add(chunks) > 1_000_000 {
                return Err("compute artifact packet budget exceeded".into());
            }
        }
        for suffix in ["full", "tail"] {
            let id = format!("__c0:{}:{suffix}", task.id);
            if self.network.flows.iter().any(|flow| flow.id == id) {
                return Err("compute transfer flow ID collides with background traffic".into());
            }
        }
        Ok(())
    }
}

fn reserve(
    node: &ComputeNode,
    task: &ComputeWorkload,
    input_bytes: u64,
) -> Result<ResourceReservation, String> {
    let peak_storage_bytes = input_bytes
        .checked_add(task.output_size_bytes)
        .ok_or("modeled storage size overflow")?;
    if !node.available {
        return Err("compute node unavailable".into());
    }
    if task.memory_bytes > node.memory_bytes {
        return Err("insufficient modeled memory".into());
    }
    if peak_storage_bytes > node.storage_bytes {
        return Err("insufficient modeled storage".into());
    }
    if task.power_demand_mw > node.power_budget_mw {
        return Err("insufficient modeled power budget".into());
    }
    let units_ns = u128::from(task.cpu_service_units)
        .checked_mul(NS_PER_SECOND)
        .ok_or("modeled service time overflow")?;
    let duration = units_ns.div_ceil(u128::from(node.cpu_units_per_second));
    let modeled_cpu_duration_ns =
        u64::try_from(duration).map_err(|_| "modeled service duration exceeds u64 nanoseconds")?;
    Ok(ResourceReservation {
        node_id: node.id.clone(),
        cpu_service_units: task.cpu_service_units,
        memory_bytes: task.memory_bytes,
        peak_storage_bytes,
        power_demand_mw: task.power_demand_mw,
        modeled_cpu_duration_ns,
    })
}

/// Adds actual satellite-to-ground unicast flows to the existing L1 packet simulation.
/// The last packet carries exactly the remainder, not padded or instantaneously teleported bytes.
fn transfer(
    scenario: &ComputeExperiment,
    artifact_id: &str,
    from_site: &str,
    to_site: &str,
    bytes: u64,
    at_ns: u64,
) -> Result<(SimulationReport, ArtifactTransfer), String> {
    let mut network = scenario.network.clone();
    let full = bytes / scenario.packet_payload_bytes;
    let tail = bytes % scenario.packet_payload_bytes;
    let total = full + u64::from(tail > 0);
    let last_at = at_ns
        .checked_add(total.saturating_sub(1))
        .ok_or("transfer injection time overflow")?;
    if last_at >= network.duration_ns {
        return Err("artifact cannot be injected within simulation horizon".into());
    }
    let mut added = HashMap::new();
    if full > 0 {
        let id = format!("__c0:{}:full", scenario.workload.id);
        network.flows.push(Flow {
            id: id.clone(),
            source: from_site.into(),
            destination: to_site.into(),
            packet_size_bytes: scenario.packet_payload_bytes,
            packet_count: u32::try_from(full).map_err(|_| "too many full-size packets")?,
            start_ns: at_ns,
            interval_ns: u64::from(full > 1),
        });
        added.insert(id, scenario.packet_payload_bytes);
    }
    if tail > 0 {
        let id = format!("__c0:{}:tail", scenario.workload.id);
        network.flows.push(Flow {
            id: id.clone(),
            source: from_site.into(),
            destination: to_site.into(),
            packet_size_bytes: tail,
            packet_count: 1,
            start_ns: at_ns + full,
            interval_ns: 0,
        });
        added.insert(id, tail);
    }
    let report = run(&network)?;
    let mut transfer = ArtifactTransfer {
        artifact_id: artifact_id.into(),
        from_site: from_site.into(),
        to_site: to_site.into(),
        injected_bytes: bytes,
        delivered_bytes: 0,
        dropped_bytes: 0,
        injected_packets: 0,
        delivered_packets: 0,
        dropped_packets: 0,
        last_arrival_ns: None,
    };
    for packet in &report.packets {
        let Some(&size) = added.get(&packet.flow_id) else {
            continue;
        };
        transfer.injected_packets += 1;
        match packet.status {
            PacketStatus::Delivered => {
                transfer.delivered_packets += 1;
                transfer.delivered_bytes += size;
                let at_ns = packet
                    .completed_ns
                    .ok_or("delivered packet has no completion time")?;
                transfer.last_arrival_ns = Some(
                    transfer
                        .last_arrival_ns
                        .map_or(at_ns, |last| last.max(at_ns)),
                );
            }
            PacketStatus::Dropped => {
                transfer.dropped_packets += 1;
                transfer.dropped_bytes += size;
            }
            PacketStatus::InFlight => {
                return Err("in-flight packet escaped runtime accounting".into())
            }
        }
    }
    if transfer.injected_packets as u64 != total
        || transfer.delivered_bytes + transfer.dropped_bytes != bytes
        || transfer.delivered_packets + transfer.dropped_packets != transfer.injected_packets
    {
        return Err("artifact transfer packet/byte accounting invariant violated".into());
    }
    Ok((report, transfer))
}

fn run_policy(scenario: &ComputeExperiment, policy: Placement) -> Result<PolicyOutcome, String> {
    let task = &scenario.workload;
    let input = &scenario.input_artifact;
    let is_edge = policy == Placement::OnboardFirst;
    let node = if is_edge {
        &scenario.orbital_node
    } else {
        &scenario.ground_node
    };
    let mut outcome = PolicyOutcome::new(policy, &node.id);
    let reservation = match reserve(node, task, input.size_bytes) {
        Ok(reservation) => reservation,
        Err(reason) => {
            outcome.fail(WorkloadStatus::Rejected, &reason);
            return Ok(outcome);
        }
    };
    let cpu_duration = reservation.modeled_cpu_duration_ns;
    outcome.resource_reservation = Some(reservation);
    // A complete transfer must fit its injection schedule inside the virtual horizon.
    // This is a policy-level deadline failure, not an invalid experiment.
    let transfer_fits_horizon = |bytes: u64, at_ns: u64| -> bool {
        let packets = bytes.div_ceil(scenario.packet_payload_bytes);
        at_ns
            .checked_add(packets.saturating_sub(1))
            .is_some_and(|last| last < scenario.network.duration_ns)
    };
    let start_ns = if is_edge {
        task.release_ns
    } else {
        if !transfer_fits_horizon(input.size_bytes, task.release_ns) {
            outcome.fail(
                WorkloadStatus::DeadlineMissed,
                "raw input injection exceeds simulation horizon",
            );
            return Ok(outcome);
        }
        let (packet_report, transfer) = transfer(
            scenario,
            &input.id,
            &scenario.orbital_node.site_id,
            &scenario.ground_node.site_id,
            input.size_bytes,
            task.release_ns,
        )?;
        let transferred = transfer.delivered_bytes == input.size_bytes;
        let arrived = transfer.last_arrival_ns;
        outcome.transfer = Some(transfer);
        outcome.packet_report = Some(packet_report);
        if !transferred {
            outcome.fail(
                WorkloadStatus::TransferFailed,
                "raw input did not reach the ground compute node",
            );
            return Ok(outcome);
        }
        arrived.ok_or("complete input transfer without arrival timestamp")?
    };
    outcome.started_ns = Some(start_ns);
    let completed_compute = start_ns
        .checked_add(cpu_duration)
        .ok_or("modeled compute clock overflow")?;
    // Preserve the modeled execution finish even if it falls beyond the task
    // deadline or the simulation horizon; no output artifact is produced then.
    outcome.compute_completed_ns = Some(completed_compute);
    if completed_compute > task.deadline_ns || completed_compute >= scenario.network.duration_ns {
        outcome.fail(
            WorkloadStatus::DeadlineMissed,
            "modeled compute misses task deadline or simulation horizon",
        );
        return Ok(outcome);
    }
    let output_id = format!("{}:output", task.id);
    outcome.produced_artifact = Some(ProducedArtifact {
        id: output_id.clone(),
        derived_from: input.id.clone(),
        size_bytes: task.output_size_bytes,
        site_id: node.site_id.clone(),
        produced_at_ns: completed_compute,
    });
    let completed_ns = if is_edge {
        if !transfer_fits_horizon(task.output_size_bytes, completed_compute) {
            outcome.fail(
                WorkloadStatus::DeadlineMissed,
                "output injection exceeds simulation horizon",
            );
            return Ok(outcome);
        }
        let (packet_report, transfer) = transfer(
            scenario,
            &output_id,
            &node.site_id,
            &scenario.ground_node.site_id,
            task.output_size_bytes,
            completed_compute,
        )?;
        let transferred = transfer.delivered_bytes == task.output_size_bytes;
        let arrived = transfer.last_arrival_ns;
        outcome.transfer = Some(transfer);
        outcome.packet_report = Some(packet_report);
        if !transferred {
            outcome.fail(
                WorkloadStatus::TransferFailed,
                "computed output did not reach ground station",
            );
            return Ok(outcome);
        }
        arrived.ok_or("complete output transfer without arrival timestamp")?
    } else {
        completed_compute
    };
    // Completion is an observed virtual-time fact even when the workload
    // misses its deadline. Keep it for reproducible latency comparisons rather
    // than erasing evidence of a fully delivered late artifact.
    outcome.completed_ns = Some(completed_ns);
    if completed_ns > task.deadline_ns {
        outcome.fail(
            WorkloadStatus::DeadlineMissed,
            "delivered artifact missed workload deadline",
        );
    } else {
        outcome.status = WorkloadStatus::Completed;
    }
    Ok(outcome)
}

pub fn run_experiment(scenario: &ComputeExperiment) -> Result<ComputeReport, String> {
    scenario.validate()?;
    let ground_only = run_policy(scenario, Placement::GroundOnly)?;
    let onboard_first = run_policy(scenario, Placement::OnboardFirst)?;
    Ok(ComputeReport {
        scenario: scenario.network.name.clone(),
        fidelity: "s1_deterministic_modeled_compute_l1_packet_transport",
        input_artifact_id: scenario.input_artifact.id.clone(),
        input_bytes: scenario.input_artifact.size_bytes,
        output_bytes: scenario.workload.output_size_bytes,
        ground_only,
        onboard_first,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> ComputeExperiment {
        ComputeExperiment::from_json(include_str!("../../../scenarios/c0-orbital-edge.json"))
            .unwrap()
    }

    #[test]
    fn both_policies_use_actual_isl_and_feeder_packets_with_exact_artifact_bytes() {
        let report = run_experiment(&fixture()).unwrap();
        assert_eq!(report.ground_only.status, WorkloadStatus::Completed);
        assert_eq!(report.onboard_first.status, WorkloadStatus::Completed);
        assert!(
            report.onboard_first.completed_ns.unwrap() < report.ground_only.completed_ns.unwrap()
        );
        for (outcome, size) in [
            (&report.ground_only, report.input_bytes),
            (&report.onboard_first, report.output_bytes),
        ] {
            let transfer = outcome.transfer.as_ref().unwrap();
            assert_eq!(transfer.injected_bytes, size);
            assert_eq!(transfer.delivered_bytes, size);
            assert_eq!(transfer.dropped_bytes, 0);
            assert_eq!(transfer.injected_packets, transfer.delivered_packets);
            let network = outcome.packet_report.as_ref().unwrap();
            assert_eq!(network.injected, network.delivered + network.dropped);
            let artifact_packets: Vec<_> = network
                .packets
                .iter()
                .filter(|p| p.flow_id.starts_with("__c0:"))
                .collect();
            assert_eq!(artifact_packets.len(), transfer.injected_packets);
            assert!(artifact_packets.iter().all(|packet| {
                let hops: Vec<_> = packet.hops.iter().map(|hop| hop.link_id.as_str()).collect();
                hops == ["isl-a-b", "feeder-b-gw"]
            }));
        }
        assert_eq!(
            report
                .onboard_first
                .produced_artifact
                .as_ref()
                .unwrap()
                .site_id,
            "sat-a"
        );
        assert_eq!(
            report
                .ground_only
                .produced_artifact
                .as_ref()
                .unwrap()
                .site_id,
            "gw-a"
        );
    }

    #[test]
    fn repeated_runs_produce_byte_identical_virtual_time_and_network_traces() {
        let scenario = fixture();
        let first = serde_json::to_vec(&run_experiment(&scenario).unwrap()).unwrap();
        let second = serde_json::to_vec(&run_experiment(&scenario).unwrap()).unwrap();
        assert_eq!(first, second);
    }

    #[test]
    fn downed_feeder_blocks_both_policies_without_teleporting_artifacts() {
        let mut scenario = fixture();
        scenario
            .network
            .links
            .iter_mut()
            .find(|link| link.id == "feeder-b-gw")
            .unwrap()
            .up = false;
        let report = run_experiment(&scenario).unwrap();
        for outcome in [&report.ground_only, &report.onboard_first] {
            assert_eq!(outcome.status, WorkloadStatus::TransferFailed);
            let transfer = outcome.transfer.as_ref().unwrap();
            assert!(transfer.dropped_bytes > 0);
            assert!(transfer.delivered_bytes < transfer.injected_bytes);
            assert_eq!(
                transfer.injected_bytes,
                transfer.delivered_bytes + transfer.dropped_bytes
            );
        }
        assert!(report.ground_only.produced_artifact.is_none());
        assert!(report.onboard_first.produced_artifact.is_some());
    }

    #[test]
    fn feeder_fault_recovery_controls_both_compute_policy_artifact_paths() {
        use ntarlink_contracts::{Fault, FaultCause, LinkAction};

        let mut scenario = fixture();
        scenario.network.faults = vec![
            Fault {
                at_ns: 0,
                link_id: "feeder-b-gw".into(),
                action: LinkAction::Down,
                cause: FaultCause::Administrative,
            },
            Fault {
                at_ns: 100_000_000,
                link_id: "feeder-b-gw".into(),
                action: LinkAction::Up,
                cause: FaultCause::Administrative,
            },
        ];

        // No implicit retry: an artifact sent during the outage cannot be
        // counted as delivered just because the feeder later recovers.
        let outage = run_experiment(&scenario).unwrap();
        assert_eq!(outage.ground_only.status, WorkloadStatus::TransferFailed);
        assert_eq!(outage.onboard_first.status, WorkloadStatus::TransferFailed);
        for policy in [&outage.ground_only, &outage.onboard_first] {
            let transfer = policy.transfer.as_ref().unwrap();
            assert_eq!(
                transfer.injected_bytes,
                transfer.delivered_bytes + transfer.dropped_bytes
            );
            assert!(transfer.dropped_bytes > 0);
        }

        // A later workload release uses the same committed recovery timeline,
        // including route reconvergence, rather than bypassing packet routing.
        scenario.workload.release_ns = 200_000_000;
        let recovered = run_experiment(&scenario).unwrap();
        for policy in [&recovered.ground_only, &recovered.onboard_first] {
            assert_eq!(policy.status, WorkloadStatus::Completed);
            let transfer = policy.transfer.as_ref().unwrap();
            assert_eq!(transfer.delivered_bytes, transfer.injected_bytes);
            assert_eq!(transfer.dropped_packets, 0);
            assert!(transfer.last_arrival_ns.unwrap() > scenario.workload.release_ns);
        }
        let first = serde_json::to_vec(&run_experiment(&scenario).unwrap()).unwrap();
        let second = serde_json::to_vec(&run_experiment(&scenario).unwrap()).unwrap();
        assert_eq!(first, second);
    }

    #[test]
    fn finite_memory_storage_and_power_reject_without_running_packets() {
        let mut scenario = fixture();
        scenario.orbital_node.memory_bytes = 1;
        let report = run_experiment(&scenario).unwrap();
        assert_eq!(report.onboard_first.status, WorkloadStatus::Rejected);
        assert_eq!(
            report.onboard_first.reason.as_deref(),
            Some("insufficient modeled memory")
        );
        assert!(report.onboard_first.packet_report.is_none());
        assert_eq!(report.ground_only.status, WorkloadStatus::Completed);
        scenario.orbital_node.memory_bytes = 1_000_000;
        scenario.orbital_node.storage_bytes = scenario.input_artifact.size_bytes;
        assert_eq!(
            run_experiment(&scenario)
                .unwrap()
                .onboard_first
                .reason
                .as_deref(),
            Some("insufficient modeled storage")
        );
        scenario.orbital_node.storage_bytes = 1_000_000;
        scenario.orbital_node.power_budget_mw = 1;
        assert_eq!(
            run_experiment(&scenario)
                .unwrap()
                .onboard_first
                .reason
                .as_deref(),
            Some("insufficient modeled power budget")
        );
    }

    #[test]
    fn queue_overflow_is_reported_as_real_packet_loss() {
        let mut scenario = fixture();
        scenario
            .network
            .links
            .iter_mut()
            .find(|link| link.id == "feeder-b-gw")
            .unwrap()
            .queue_limit_bytes = 1;
        let report = run_experiment(&scenario).unwrap();
        assert_eq!(report.ground_only.status, WorkloadStatus::TransferFailed);
        assert_eq!(report.onboard_first.status, WorkloadStatus::TransferFailed);
        for outcome in [&report.ground_only, &report.onboard_first] {
            let trace = outcome.packet_report.as_ref().unwrap();
            assert!(trace
                .packets
                .iter()
                .any(|packet| packet.drop_reason.as_deref() == Some("queue_overflow")));
            let transfer = outcome.transfer.as_ref().unwrap();
            assert_eq!(
                transfer.injected_bytes,
                transfer.delivered_bytes + transfer.dropped_bytes
            );
        }
    }

    #[test]
    fn transfer_injection_past_horizon_is_a_policy_failure_not_an_experiment_error() {
        let mut scenario = fixture();
        scenario.workload.release_ns = scenario.network.duration_ns - 10;
        scenario.workload.deadline_ns = scenario.network.duration_ns;
        let report = run_experiment(&scenario).unwrap();
        assert_eq!(report.ground_only.status, WorkloadStatus::DeadlineMissed);
        assert_eq!(
            report.ground_only.reason.as_deref(),
            Some("raw input injection exceeds simulation horizon")
        );
        assert!(report.ground_only.packet_report.is_none());
        assert_eq!(report.onboard_first.status, WorkloadStatus::DeadlineMissed);

        // The output path is checked independently after modeled execution.
        // At 1,000,000 service units/s, 2,010,000 units take 2,010,000,000 ns.
        // Finish one nanosecond before the horizon so only output injection fails.
        scenario.workload.release_ns = scenario.network.duration_ns - 2_010_000_001;
        scenario.workload.cpu_service_units = 2_010_000;
        scenario.workload.deadline_ns = scenario.network.duration_ns;
        let report = run_experiment(&scenario).unwrap();
        assert_eq!(report.onboard_first.status, WorkloadStatus::DeadlineMissed);
        assert_eq!(
            report.onboard_first.reason.as_deref(),
            Some("output injection exceeds simulation horizon")
        );
        assert!(report.onboard_first.produced_artifact.is_some());
    }

    #[test]
    fn late_delivered_output_retains_completion_time_and_packet_evidence() {
        let mut scenario = fixture();
        // Leave enough time to execute and deliver the output, but set the
        // deadline before its network arrival. The deadline is a task policy,
        // not a reason to discard an observed packet completion timestamp.
        scenario.workload.deadline_ns = 25_000_000;
        let report = run_experiment(&scenario).unwrap();
        let edge = &report.onboard_first;
        assert_eq!(edge.status, WorkloadStatus::DeadlineMissed);
        assert_eq!(
            edge.reason.as_deref(),
            Some("delivered artifact missed workload deadline")
        );
        let delivered_at = edge.completed_ns.expect("late arrival must be retained");
        assert!(delivered_at > scenario.workload.deadline_ns);
        assert_eq!(edge.compute_completed_ns, Some(20_000_000));
        assert_eq!(
            edge.transfer.as_ref().unwrap().last_arrival_ns,
            Some(delivered_at)
        );
        assert_eq!(
            edge.transfer.as_ref().unwrap().delivered_bytes,
            scenario.workload.output_size_bytes
        );
        assert!(edge.packet_report.is_some());
        let repeated = run_experiment(&scenario).unwrap();
        assert_eq!(repeated.onboard_first.completed_ns, Some(delivered_at));
    }

    #[test]
    fn modeled_compute_deadline_miss_retains_execution_finish_without_artifact() {
        let mut scenario = fixture();
        scenario.workload.deadline_ns = 1_000_000;
        let first = run_experiment(&scenario).unwrap();
        let edge = &first.onboard_first;
        assert_eq!(edge.status, WorkloadStatus::DeadlineMissed);
        assert_eq!(edge.started_ns, Some(scenario.workload.release_ns));
        let reservation = edge.resource_reservation.as_ref().unwrap();
        let finished = scenario.workload.release_ns + reservation.modeled_cpu_duration_ns;
        assert_eq!(edge.compute_completed_ns, Some(finished));
        assert!(finished > scenario.workload.deadline_ns);
        assert!(edge.produced_artifact.is_none());
        assert!(edge.transfer.is_none());
        assert!(edge.packet_report.is_none());
        let repeated = run_experiment(&scenario).unwrap();
        assert_eq!(repeated.onboard_first.compute_completed_ns, Some(finished));
    }

    #[test]
    fn deadlines_and_invalid_topology_are_enforced() {
        let mut scenario = fixture();
        scenario.workload.deadline_ns = 1_000_000;
        let report = run_experiment(&scenario).unwrap();
        assert_eq!(report.onboard_first.status, WorkloadStatus::DeadlineMissed);
        assert!(report.onboard_first.produced_artifact.is_none());
        scenario.orbital_node.site_id = "internet".into();
        assert!(scenario
            .validate()
            .unwrap_err()
            .contains("invalid modeled compute node"));
        scenario.orbital_node.network_endpoint_id = "internet".into();
        assert!(scenario
            .validate()
            .unwrap_err()
            .contains("network endpoints"));
        scenario.orbital_node.site_id = "sat-a".into();
        scenario.orbital_node.network_endpoint_id = "sat-a".into();
        scenario.input_artifact.size_bytes = scenario.workload.output_size_bytes;
        assert!(scenario.validate().is_err());
    }
}
