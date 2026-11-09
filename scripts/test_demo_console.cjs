#!/usr/bin/env node
"use strict";

/* Dependency-free smoke test for the static GitHub Pages demo router. */
const fs = require("node:fs");
const path = require("node:path");
const vm = require("node:vm");
const assert = require("node:assert/strict");

const root = path.resolve(__dirname, "..", "applications", "console", "pages", "demo");
const html = fs.readFileSync(path.join(root, "index.html"), "utf8");
const dataJs = fs.readFileSync(path.join(root, "data.js"), "utf8");
const appJs = fs.readFileSync(path.join(root, "app.js"), "utf8");
const computeJs = fs.readFileSync(path.join(root, "compute.js"), "utf8");
assert(html.includes('./data.js') && html.includes('./compute.js') && html.includes('./app.js') && html.includes('./styles.css'));

const callbacks = {};
const app = {innerHTML: ""};
const dialog = {innerHTML: "", textContent: "", querySelector: () => null};
const toast = {innerHTML: "", textContent: ""};
const sidebar = {scrollTop: 0};
const ids = {"app": app, "dialog-root": dialog, "toast-root": toast, "sidebar": sidebar};
const document = {
  title: "",
  getElementById: (id) => ids[id] || null,
  querySelector: (selector) => selector === ".sidebar" ? sidebar : null,
  addEventListener: (event, callback) => {callbacks["doc:" + event] = callback;},
};
const window = {
  location: {hash: "#/overview"},
  addEventListener: (event, callback) => {callbacks["win:" + event] = callback;},
  scrollTo: () => {},
};
const context = {
  window, document,
  setTimeout: () => 1, clearTimeout: () => {},
  setInterval: () => 1, clearInterval: () => {},
  Blob, URL,
  Element: class Element {},
  console
};
vm.runInNewContext(dataJs, context, {filename:"data.js"});
vm.runInNewContext(computeJs, context, {filename:"compute.js"});
vm.runInNewContext(appJs, context, {filename:"app.js"});

assert.equal(window.NTARLINK_DEMO.systems.length, 16);
assert.equal(new Set(window.NTARLINK_DEMO.systems.map(s => s.id)).size, 16);
assert.equal(window.NTARLINK_DEMO.scenarios.length, 4);
assert.match(app.innerHTML, /Satellite Network Overview/);
assert.match(app.innerHTML, /Broadband Packet Journey/);
assert.match(app.innerHTML, /All 16/);
assert.match(app.innerHTML, /SYNTHETIC DATA/);
assert.match(app.innerHTML, /Export mock snapshot/);
assert.match(app.innerHTML, /Compute Fabric Lab/);
const lab = window.NTARLINK_COMPUTE_LAB;
assert.equal(lab.workloads.length, 3);
assert.equal(lab.nodes.length, 3);
const baseline = lab.evaluate("earth-image", "nominal");
assert.equal(baseline.results.length, 3);
assert(baseline.results.every((row) => row.feasible), "nominal policies must all be feasible");
assert(baseline.results.every((row) => row.seconds > 0 && row.transferredMB > 0));
assert(baseline.results.find((row) => row.id === "ground").transferredMB >
  baseline.results.find((row) => row.id === "onboard").transferredMB, "local compute must reduce downlink traffic");
assert.equal(lab.evaluate("earth-image", "isl").results.find((row) => row.id === "peer").reason, "Optical ISL unavailable");
assert(lab.evaluate("earth-image", "gateway").results.find((row) => row.id === "ground").seconds >
  baseline.results.find((row) => row.id === "ground").seconds, "gateway outage should delay completion");
assert.equal(lab.evaluate("earth-image", "nominal").results[0].seconds, baseline.results[0].seconds, "synthetic results must be deterministic");

function route(hash) {
  window.location.hash = hash;
  callbacks["win:hashchange"]();
  assert(app.innerHTML.length > 200, hash);
}
for (const system of window.NTARLINK_DEMO.systems) {
  route("#/system/" + system.id + "/overview");
  assert(app.innerHTML.includes(system.name.replaceAll("&", "&amp;")), system.id + " overview");
  assert(app.innerHTML.includes("Primary capabilities"), system.id + " capabilities");
  route("#/system/" + system.id + "/assets");
  assert(app.innerHTML.includes("<table>"), system.id + " inventory");
  assert(app.innerHTML.includes(system.assets[0].id), system.id + " first asset");
  route("#/system/" + system.id + "/operations");
  assert(app.innerHTML.includes("Run mock"), system.id + " operations");
  route("#/system/" + system.id + "/telemetry");
  assert(app.innerHTML.includes("Mock") || app.innerHTML.includes("MOCK"), system.id + " telemetry notice");
  assert(app.innerHTML.includes("<svg"), system.id + " telemetry graph");
}
route("#/compute");
assert(app.innerHTML.includes("Compute Fabric Lab"), "compute route should render");
assert(app.innerHTML.includes("Network-attached compute topology"), "compute topology should render");
assert(app.innerHTML.includes("SAT-A"), "the source compute node should be visible");
assert(app.innerHTML.includes("Ground only"), "ground placement should be visible");
assert(app.innerHTML.includes("Peer via optical ISL"), "peer compute placement should be visible");
assert(app.innerHTML.includes("C3"), "hardware worker roadmap should be visible");
assert(app.innerHTML.includes("NO GPU / NO LIVE EXECUTION"), "physical execution must not be implied");
route("#/overview");
assert(app.innerHTML.includes("Experiment Scenarios"));
assert(app.innerHTML.includes("Nominal orbit"));


/* Exercise delegated controls, not only static markup. */
function clickAs(selector, attributes) {
  const target = new context.Element();
  target.closest = (candidate) => candidate === selector ? {
    getAttribute: (key) => attributes[key],
  } : null;
  callbacks["doc:click"]({target});
}
route("#/overview");
clickAs("[data-scenario]", {"data-scenario": "isl"});
assert(app.innerHTML.includes("44.7"), "ISL failure should change the displayed RTT");
assert(app.innerHTML.includes("TRACKING LOST"), "ISL failure should affect network topology");

route("#/compute");
clickAs("[data-placement]", {"data-placement": "peer"});
assert(app.innerHTML.includes("Optical ISL unavailable"), "the ISL failure must block peer placement");
clickAs("[data-workload]", {"data-workload": "telemetry"});
assert(app.innerHTML.includes("Satellite telemetry inference"), "workload selection must update task details");
clickAs("[data-scenario]", {"data-scenario": "nominal"});
assert(!app.innerHTML.includes("Optical ISL unavailable"), "nominal links should restore peer placement");
assert(app.innerHTML.includes("Compute Fabric Lab"), "state changes should remain on compute route");

route("#/system/orbital/operations");
clickAs("[data-action]", {"data-action": "orbital", "data-operation": "0"});
assert(app.innerHTML.includes("Executed locally in the demo UI"), "mock operation should append an event");
route("#/system/orbital/assets");
assert(app.innerHTML.includes("Degraded"), "mock operation should change local inventory status");

clickAs("[data-inspect]", {"data-inspect": "orbital", "data-asset": "LEO-SAT-0412"});
assert(dialog.innerHTML.includes('role="dialog"'), "entity inspector should open");
assert(dialog.innerHTML.includes("LEO-SAT-0412"), "inspector should show selected asset");
clickAs("[data-close]", {});
assert.equal(dialog.textContent, "", "entity inspector should close");

console.log("Demo console smoke tests passed: 16 systems × 4 pages, compute fabric scenario/policy/workload comparisons, mock operations and entity inspection.");

