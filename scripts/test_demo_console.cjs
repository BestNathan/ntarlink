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
assert(html.includes('./data.js') && html.includes('./app.js') && html.includes('./styles.css'));

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
vm.runInNewContext(appJs, context, {filename:"app.js"});

assert.equal(window.NTARLINK_DEMO.systems.length, 16);
assert.equal(new Set(window.NTARLINK_DEMO.systems.map(s => s.id)).size, 16);
assert.equal(window.NTARLINK_DEMO.scenarios.length, 4);
assert.match(app.innerHTML, /Satellite Network Overview/);
assert.match(app.innerHTML, /Broadband Packet Journey/);
assert.match(app.innerHTML, /All 16/);
assert.match(app.innerHTML, /SYNTHETIC DATA/);
assert.match(app.innerHTML, /Export mock snapshot/);

function route(hash) {
  window.location.hash = hash;
  callbacks["win:hashchange"]();
  assert(app.innerHTML.length > 200, hash);
}
for (const system of window.NTARLINK_DEMO.systems) {
  route("#/system/" + system.id + "/overview");
  assert(app.innerHTML.includes(system.name), system.id + " overview");
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
route("#/overview");
assert(app.innerHTML.includes("Experiment Scenarios"));
assert(app.innerHTML.includes("Nominal orbit"));

console.log("Demo console smoke tests passed: 16 systems × 4 pages, home, scenario and mock-data indicators.");
