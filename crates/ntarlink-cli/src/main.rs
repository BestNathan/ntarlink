use ntarlink_core::Scenario;
use std::{env, fs, process};

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() != 3 || !matches!(args[1].as_str(), "validate" | "simulate") {
        eprintln!("usage: ntarlink-cli <validate|simulate> <scenario.json>");
        process::exit(2);
    }
    let raw = fs::read_to_string(&args[2]).unwrap_or_else(|e| fail(&e.to_string()));
    let scenario: Scenario = serde_json::from_str(&raw).unwrap_or_else(|e| fail(&e.to_string()));
    scenario.validate().unwrap_or_else(|e| fail(&e));
    println!("scenario={} satellites={} terminals={} gateways={}", scenario.name, scenario.satellites.len(), scenario.terminals.len(), scenario.gateways.len());
    if args[1] == "simulate" { println!("simulation engine not yet implemented (bootstrap only)"); }
}
fn fail(message: &str) -> ! { eprintln!("error: {message}"); process::exit(1) }
