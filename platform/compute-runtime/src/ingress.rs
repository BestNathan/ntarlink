//! Packet-authoritative S1 input artifact transfers between orbital nodes.
//! A preflight is only an observation: the final shared run must replay the
//! identical input packet traces or execution fails closed.

use crate::batch::BatchExperiment;
use ntarlink_contracts::{Flow, Scenario};
use ntarlink_packet_runtime::{PacketStatus, PacketTrace, SimulationReport};
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Clone, Debug, Default, Serialize)]
pub struct InputReceipt {
    pub injected_packets: usize,
    pub delivered_packets: usize,
    pub dropped_packets: usize,
    pub delivered_bytes: u64,
    pub dropped_bytes: u64,
    /// The last packet arrival only when every byte arrived successfully.
    pub ready_at_ns: Option<u64>,
}

/// Build flows in stable task-ID order, independent of the caller's job order.
pub(crate) fn append_input_flows(
    experiment: &BatchExperiment,
    network: &mut Scenario,
) -> Result<BTreeMap<String, (String, u64)>, String> {
    let mut jobs: Vec<_> = experiment.jobs.iter().collect();
    jobs.sort_by(|a, b| a.workload.id.cmp(&b.workload.id));
    let mut flows = BTreeMap::new();
    for job in jobs {
        let node = experiment
            .compute_nodes
            .iter()
            .find(|n| n.id == job.node_id)
            .ok_or("missing validated input compute node")?;
        if job.input_artifact.origin_site_id == node.site_id {
            continue;
        }
        let bytes = job.input_artifact.size_bytes;
        let full = bytes / experiment.packet_payload_bytes;
        let tail = bytes % experiment.packet_payload_bytes;
        let at = job.workload.release_ns;
        for (suffix, size, count, start) in [
            ("full", experiment.packet_payload_bytes, full, at),
            ("tail", tail, u64::from(tail > 0), at + full),
        ] {
            if count == 0 {
                continue;
            }
            let id = format!("__c1_input:{}:{suffix}", job.workload.id);
            network.flows.push(Flow {
                id: id.clone(),
                source: job.input_artifact.origin_site_id.clone(),
                destination: node.site_id.clone(),
                packet_size_bytes: size,
                packet_count: u32::try_from(count).map_err(|_| "input packet count overflow")?,
                start_ns: start,
                interval_ns: u64::from(count > 1),
            });
            flows.insert(id, (job.workload.id.clone(), size));
        }
    }
    Ok(flows)
}

/// Accumulate measured L1 observations, never infer delivery from topology.
pub(crate) fn observe_input(
    experiment: &BatchExperiment,
    network: &SimulationReport,
    flows: &BTreeMap<String, (String, u64)>,
) -> Result<BTreeMap<String, InputReceipt>, String> {
    let mut receipts = BTreeMap::new();
    for (_, (task_id, _)) in flows {
        receipts
            .entry(task_id.clone())
            .or_insert_with(InputReceipt::default);
    }
    for packet in &network.packets {
        let Some((task_id, bytes)) = flows.get(&packet.flow_id) else {
            continue;
        };
        let receipt = receipts.get_mut(task_id).ok_or("input receipt missing")?;
        receipt.injected_packets += 1;
        match packet.status {
            PacketStatus::Delivered => {
                receipt.delivered_packets += 1;
                receipt.delivered_bytes += bytes;
                let at = packet
                    .completed_ns
                    .ok_or("delivered input has no packet arrival")?;
                receipt.ready_at_ns = Some(receipt.ready_at_ns.map_or(at, |prev| prev.max(at)));
            }
            PacketStatus::Dropped => {
                receipt.dropped_packets += 1;
                receipt.dropped_bytes += bytes;
            }
            PacketStatus::InFlight => return Err("unaccounted input artifact packet".into()),
        }
    }
    for job in &experiment.jobs {
        let Some(receipt) = receipts.get_mut(&job.workload.id) else {
            continue;
        };
        if receipt.delivered_bytes + receipt.dropped_bytes != job.input_artifact.size_bytes
            || receipt.injected_packets != receipt.delivered_packets + receipt.dropped_packets
        {
            return Err("input artifact packet accounting mismatch".into());
        }
        if receipt.dropped_bytes != 0 {
            receipt.ready_at_ns = None;
        }
    }
    Ok(receipts)
}

/// The final mixed packet run is the authority, not the observational pass.
/// If output injections changed an input route, queue or delivery, abort rather
/// than execute work based on a speculative input-ready timestamp.
pub(crate) fn assert_replayed_input(
    observed: &SimulationReport,
    final_network: &SimulationReport,
    flows: &BTreeMap<String, (String, u64)>,
) -> Result<(), String> {
    fn input_packets(
        report: &SimulationReport,
        flows: &BTreeMap<String, (String, u64)>,
    ) -> Vec<PacketTrace> {
        let mut packets: Vec<_> = report
            .packets
            .iter()
            .filter(|packet| flows.contains_key(&packet.flow_id))
            .cloned()
            .collect();
        packets.sort_by(|a, b| a.packet_id.cmp(&b.packet_id));
        packets
    }
    if input_packets(observed, flows) != input_packets(final_network, flows) {
        return Err(
            "input packet observations changed in final shared network replay; refusing causal execution"
                .into(),
        );
    }
    Ok(())
}
