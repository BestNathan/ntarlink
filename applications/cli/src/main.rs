use ntarlink_compute_runtime::batch::{run_batch, BatchExperiment};
use ntarlink_compute_runtime::{run_experiment, ComputeExperiment};
use ntarlink_contracts::Scenario;
use ntarlink_packet_runtime::run;
use std::{env, fs, process};

fn main() {
    let args: Vec<String> = env::args().collect();
    if !(args.len() == 3 || (args.len() == 4 && args[3] == "--json"))
        || !matches!(
            args[1].as_str(),
            "validate" | "simulate" | "validate-compute" | "simulate-compute" | "validate-batch" | "simulate-batch"
        )
    {
        eprintln!("usage: ntarlink-cli <validate|simulate|validate-compute|simulate-compute|validate-batch|simulate-batch> <scenario.json> [--json]");
        process::exit(2);
    }
    let raw = fs::read_to_string(&args[2]).unwrap_or_else(|e| fail(&e.to_string()));
    if matches!(args[1].as_str(), "validate-batch" | "simulate-batch") {
        let experiment = BatchExperiment::from_json(&raw).unwrap_or_else(|e| fail(&e));
        if args[1] == "validate-batch" {
            println!("validated compute batch {}", experiment.network.name);
            return;
        }
        let report = run_batch(&experiment).unwrap_or_else(|e| fail(&e));
        if args.len() == 4 {
            println!(
                "{}",
                serde_json::to_string_pretty(&report).unwrap_or_else(|e| fail(&e.to_string()))
            );
        } else {
            for task in &report.tasks {
                println!(
                    "task={} node={} status={:?} delivered_bytes={}",
                    task.task_id, task.node_id, task.status, task.delivered_bytes
                );
            }
        }
        return;
    }
    if matches!(args[1].as_str(), "validate-compute" | "simulate-compute") {
        let scenario = ComputeExperiment::from_json(&raw).unwrap_or_else(|e| fail(&e));
        if args[1] == "validate-compute" {
            println!(
                "validated modeled compute experiment {}",
                scenario.network.name
            );
            return;
        }
        let report = run_experiment(&scenario).unwrap_or_else(|e| fail(&e));
        if args.len() == 4 {
            println!(
                "{}",
                serde_json::to_string_pretty(&report).unwrap_or_else(|e| fail(&e.to_string()))
            );
        } else {
            for policy in [&report.ground_only, &report.onboard_first] {
                println!(
                    "scenario={} policy={:?} status={:?} completion_ns={:?} transferred_bytes={}",
                    report.scenario,
                    policy.policy,
                    policy.status,
                    policy.completed_ns,
                    policy.transfer.as_ref().map_or(0, |t| t.delivered_bytes)
                );
            }
        }
        return;
    }
    let scenario = Scenario::from_json(&raw).unwrap_or_else(|e| fail(&e));
    if args[1] == "validate" {
        println!(
            "validated {}: {} nodes, {} links, {} flows",
            scenario.name,
            scenario.nodes.len(),
            scenario.links.len(),
            scenario.flows.len()
        );
        return;
    }
    let report = run(&scenario).unwrap_or_else(|e| fail(&e));
    if args.len() == 4 {
        println!(
            "{}",
            serde_json::to_string_pretty(&report).unwrap_or_else(|e| fail(&e.to_string()))
        );
    } else {
        println!(
            "scenario={} fidelity={} injected={} delivered={} dropped={} events={} route_revision={}",
            report.scenario,
            report.fidelity,
            report.injected,
            report.delivered,
            report.dropped,
            report.processed_events,
            report.route_revision
        );
        for packet in &report.packets {
            let path: Vec<_> = packet.hops.iter().map(|h| h.link_id.as_str()).collect();
            println!(
                "packet={} status={:?} duration_ns={} path={} reason={}",
                packet.packet_id,
                packet.status,
                packet
                    .completed_ns
                    .unwrap_or(report.duration_ns)
                    .saturating_sub(packet.injected_ns),
                path.join(" -> "),
                packet.drop_reason.as_deref().unwrap_or("-")
            );
        }
    }
}

fn fail(message: &str) -> ! {
    eprintln!("error: {message}");
    process::exit(1)
}
