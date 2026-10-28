use std::process::Command;

#[test]
fn simulator_outputs_parseable_json_report() {
    let scenario = concat!(env!("CARGO_MANIFEST_DIR"), "/../../scenarios/minimal.json");
    let output = Command::new(env!("CARGO_BIN_EXE_ntarlink-cli"))
        .args(["simulate", scenario, "--json"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["delivered"], 3);
    assert_eq!(report["dropped"], 0);
    assert_eq!(report["packets"][0]["hops"].as_array().unwrap().len(), 5);
}

#[test]
fn rejects_invalid_scenario_file() {
    let output = Command::new(env!("CARGO_BIN_EXE_ntarlink-cli"))
        .args(["validate", "/missing-scenario-file.json"])
        .output()
        .unwrap();
    assert!(!output.status.success());
}
