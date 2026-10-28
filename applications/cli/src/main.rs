use ntarlink_contracts::Scenario;
use ntarlink_packet_runtime::run;
use std::{env, fs, process};

fn main() {
    let args: Vec<String> = env::args().collect();
    if !(args.len() == 3 || (args.len() == 4 && args[3] == "--json"))
        || !matches!(args[1].as_str(), "validate" | "simulate")
    {
        eprintln!("usage: ntarlink-cli <validate|simulate> <scenario.json> [--json]");
        process::exit(2);
    }
    let raw = fs::read_to_string(&args[2]).unwrap_or_else(|e| fail(&e.to_string()));
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
