/* Local, synthetic planning examples only. No compute runtime or accelerator is accessed. */
(function ntarlinkComputeLab() {
  "use strict";

  const workloads = [
    {id:"earth-image", label:"Earth observation image", inputMB:960, outputMB:72, operationsGop:650, memoryGB:3.8, deadlineS:210, origin:"SAT-A"},
    {id:"telemetry", label:"Satellite telemetry inference", inputMB:160, outputMB:8, operationsGop:140, memoryGB:1.0, deadlineS:70, origin:"SAT-A"},
    {id:"high-res", label:"High-resolution imaging batch", inputMB:4096, outputMB:260, operationsGop:2400, memoryGB:6.0, deadlineS:260, origin:"SAT-A"}
  ];

  const nodes = [
    {id:"SAT-A", label:"Satellite A · Edge CPU", site:"ORBIT / SOURCE", computeGopsPerS:7.5, memoryGB:8, activePowerW:92, energyBudgetWh:24},
    {id:"SAT-B", label:"Satellite B · Compute payload", site:"ORBIT / PEER", computeGopsPerS:20, memoryGB:16, activePowerW:180, energyBudgetWh:35},
    {id:"GROUND", label:"Ground compute cluster", site:"TERRESTRIAL / SITE", computeGopsPerS:75, memoryGB:128, activePowerW:880, energyBudgetWh:1000}
  ];

  const policies = [
    {id:"ground", label:"Ground only", nodeId:"GROUND", explanation:"Downlink full input, then execute on ground."},
    {id:"onboard", label:"Onboard first", nodeId:"SAT-A", explanation:"Execute on the source spacecraft, downlink compact output."},
    {id:"peer", label:"Peer via optical ISL", nodeId:"SAT-B", explanation:"Transfer input over an ISL, execute on peer, downlink output."}
  ];

  /* Synthetic capacity/wait assumptions per existing network demo preset. */
  const networks = {
    nominal: {label:"Nominal orbit", downlinkWaitS:42, downlinkMBps:42, islWaitS:0, islMBps:95, islAvailable:true},
    isl: {label:"ISL failure", downlinkWaitS:42, downlinkMBps:42, islWaitS:0, islMBps:95, islAvailable:false},
    gateway: {label:"Gateway outage", downlinkWaitS:175, downlinkMBps:20, islWaitS:12, islMBps:95, islAvailable:true},
    traffic: {label:"Traffic surge", downlinkWaitS:55, downlinkMBps:16, islWaitS:10, islMBps:60, islAvailable:true}
  };

  const round = (value) => Math.round(value * 100) / 100;
  function evaluate(workloadId, scenarioId) {
    const workload = workloads.find((w) => w.id === workloadId) || workloads[0];
    const network = networks[scenarioId] || networks.nominal;
    const results = policies.map((policy) => {
      const node = nodes.find((n) => n.id === policy.nodeId);
      const computeSeconds = workload.operationsGop / node.computeGopsPerS;
      const computeEnergyWh = computeSeconds * node.activePowerW / 3600;
      const inputTransferSeconds = policy.id === "ground" ? network.downlinkWaitS + workload.inputMB / network.downlinkMBps :
        policy.id === "peer" ? network.islWaitS + workload.inputMB / network.islMBps : 0;
      const outputTransferSeconds = policy.id === "ground" ? 0 : network.downlinkWaitS + workload.outputMB / network.downlinkMBps;
      const transferMB = policy.id === "ground" ? workload.inputMB :
        policy.id === "peer" ? workload.inputMB + workload.outputMB : workload.outputMB;
      const totalSeconds = inputTransferSeconds + computeSeconds + outputTransferSeconds;
      let blocked = "";
      if (node.memoryGB < workload.memoryGB) blocked = "Insufficient working memory";
      else if (node.energyBudgetWh < computeEnergyWh) blocked = "Compute energy budget exceeded";
      else if (policy.id === "peer" && !network.islAvailable) blocked = "Optical ISL unavailable";
      return {
        id:policy.id, label:policy.label, nodeId:node.id, explanation:policy.explanation,
        seconds:round(totalSeconds), computeSeconds:round(computeSeconds),
        inputTransferSeconds:round(inputTransferSeconds), outputTransferSeconds:round(outputTransferSeconds),
        transferredMB:round(transferMB), energyWh:round(computeEnergyWh),
        feasible:!blocked, deadlineMet:!blocked && totalSeconds <= workload.deadlineS,
        reason:blocked || (totalSeconds > workload.deadlineS ? "Synthetic deadline missed" : "Synthetic deadline met")
      };
    });
    return {workload, network, results};
  }

  window.NTARLINK_COMPUTE_LAB = Object.freeze({
    fidelity:"SYNTHETIC ANALYTICAL ESTIMATE / NOT AN EXECUTED WORKLOAD",
    workloads, nodes, policies, networks, evaluate
  });
})();
