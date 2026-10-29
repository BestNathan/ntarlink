(function ntarlinkDemo() {
  "use strict";
  const data = window.NTARLINK_DEMO;
  if (!data || data.systems.length !== 16) throw new Error("The demo catalog is missing.");
  const $ = (id) => document.getElementById(id);
  const esc = (value) => String(value === null || value === undefined ? "" : value).replace(/[&<>"']/g, (char) => ({"&":"&amp;","<":"&lt;",">":"&gt;",'"':"&quot;","'":"&#39;"}[char]));
  const groups = Object.fromEntries(data.groups.map((g) => [g.id, g]));
  const systems = Object.fromEntries(data.systems.map((s) => [s.id, s]));
  const state = {
    scenario: "nominal",
    time: 42,
    playing: false,
    timer: null,
    filter: "all",
    query: "",
    statuses: {},
    activity: {},
    imported: null,
    importedFile: "",
    notice: "",
    menu: false
  };
  const sourceNote = "All shown telemetry, actions and entity details are synthetic unless explicitly marked as an imported local Rust simulation report.";
  const page = () => {
    const parts = window.location.hash.replace(/^#\/?/, "").split("/").filter(Boolean);
    if (parts[0] === "system" && systems[parts[1]]) {
      return {kind:"system", id:parts[1], tab:["overview","assets","operations","telemetry"].includes(parts[2]) ? parts[2] : "overview"};
    }
    return {kind:"overview"};
  };
  const href = (id, tab = "overview") => "#/system/" + encodeURIComponent(id) + "/" + tab;
  const scenario = () => data.scenarios.find((s) => s.id === state.scenario) || data.scenarios[0];
  const idOf = (s) => String(data.systems.indexOf(s) + 1).padStart(2, "0");
  const htmlStatus = (status) => '<span class="status-dot ' + esc(status) + '"></span>' + esc(status === "nominal" ? "Nominal" : status === "warning" ? "Degraded" : "Offline");
  const modelStatus = (model) => model === "partial" ? '<span class="pill green">WORKING COMPONENTS</span>' : '<span class="pill">DESIGN / MOCK</span>';
  const nowLabel = () => {
    const m = state.time;
    return "T+" + String(Math.floor(m / 60)).padStart(2, "0") + ":" + String(m % 60).padStart(2, "0") + ":00";
  };
  const addActivity = (id, title, detail) => {
    const list = state.activity[id] || [];
    list.unshift({title, detail, minute:state.time});
    state.activity[id] = list.slice(0, 15);
  };
  const activityFor = (s) => {
    const fresh = state.activity[s.id] || [];
    const baseline = s.events.map((title, i) => ({title,detail:"Illustrative domain event • synthetic local demo",minute:Math.max(0,state.time - 8 * (i + 1))}));
    return [...fresh, ...baseline];
  };
  const nav = (current) => {
    let out = '<div class="nav-label">WORKSPACE</div>';
    out += '<a class="nav-item ' + (current.kind === "overview" ? "active" : "") + '" href="#/overview"><span class="nav-code">◎</span><span>Mission Overview</span><span class="arr">↗</span></a>';
    for (const g of data.groups) {
      out += '<div class="nav-group">' + esc(g.label) + '</div>';
      for (const s of data.systems.filter((x) => x.group === g.id)) {
        out += '<a class="nav-item ' + (current.kind === "system" && current.id === s.id ? "active" : "") + '" href="' + href(s.id) + '"><span class="nav-code">' + esc(idOf(s)) + '</span><span>' + esc(s.name) + '</span></a>';
      }
    }
    return out;
  };
  const layout = (current, content) => {
    const active = current.kind === "system" ? systems[current.id] : null;
    return '<div class="layout ' + (state.menu ? "menu-open" : "") + '" id="layout">' +
      '<aside class="sidebar" id="sidebar" aria-label="Subsystem navigation">' +
        '<a class="brand" href="#/overview"><span class="brand-icon">N</span><span><strong>ntarlink</strong><small>DEMO CONSOLE / V1</small></span></a>' +
        '<nav class="nav-sections" aria-label="System sections">' + nav(current) + '</nav>' +
        '<div class="sidebar-footer"><div class="local-indicator"><b>● LOCAL SIMULATION ONLY</b><p>No network control commands or external telemetry are executed by this prototype.</p></div>' +
          '<a href="../index.html">← Platform showcase</a><a href="https://github.com/BestNathan/ntarlink/tree/main/docs/subsystems" target="_blank" rel="noopener">Engineering documentation ↗</a></div>' +
      '</aside><div class="workspace">' +
        '<header class="topbar"><div class="topbar-left"><button class="menu-button" type="button" aria-label="Toggle menu" data-menu>☰</button>' +
          '<div class="breadcrumb"><b>NTARLINK</b><span>/</span>DEMO' + (active ? '<span>/</span>' + esc(active.name) : '<span>/</span>MISSION CONTROL') + '</div></div>' +
          '<div class="topbar-right"><span class="live-pill">SYNTHETIC DATA</span><a class="top-link" href="../index.html">Platform ↗</a><a class="top-link" target="_blank" rel="noopener" href="https://github.com/BestNathan/ntarlink">GitHub ↗</a></div></header>' +
        '<div class="content" id="content">' + content +
          '<footer class="footer">ntarlink / Interactive architectural demo • No real spacecraft, radio equipment or subscriber service is controlled. <a href="https://github.com/BestNathan/ntarlink" target="_blank" rel="noopener">Source & documentation ↗</a></footer>' +
        '</div></div></div><div id="dialog-root"></div><div id="toast-root" aria-live="polite"></div>';
  };
  const kpi = (label, value, unit, note) => '<div class="metric"><div class="label"><i></i>' + esc(label) + '</div><strong>' + esc(value) + (unit ? '<small> ' + esc(unit) + '</small>' : '') + '</strong><div class="foot">' + esc(note) + ' · synthetic</div></div>';
  const svgMap = () => {
    const failIsl = state.scenario === "isl";
    const failGateway = state.scenario === "gateway";
    const node = (x,y,r,id,name,info,warning) =>
      '<g class="node ' + (warning ? "warn" : "") + '" role="button" tabindex="0" data-go="' + esc(id) + '" aria-label="Explore ' + esc(name) + '">' +
        '<circle cx="' + x + '" cy="' + y + '" r="' + r + '"></circle>' +
        '<circle cx="' + x + '" cy="' + y + '" r="' + (r + 8) + '" fill="none" stroke="#79c6b2" stroke-width="1" opacity=".22"></circle>' +
        '<text x="' + (x + 16) + '" y="' + (y - 4) + '">' + esc(name) + '</text><text class="small" x="' + (x + 16) + '" y="' + (y + 9) + '">' + esc(info) + '</text></g>';
    return '<div class="diagram"><svg role="img" aria-label="Conceptual satellite-to-internet topology with clickable subsystem nodes" viewBox="0 0 830 300" preserveAspectRatio="xMidYMid meet">' +
      '<defs><radialGradient id="earthfill"><stop offset="0" stop-color="#266278" stop-opacity=".85"/><stop offset=".62" stop-color="#123951" stop-opacity=".8"/><stop offset="1" stop-color="#071a2b" stop-opacity=".35"/></radialGradient></defs>' +
      '<ellipse cx="427" cy="308" rx="323" ry="132" fill="url(#earthfill)" stroke="#447c8b" stroke-opacity=".4"/>' +
      '<path d="M117 229 L206 111 L361 89 L511 143 L648 219 L746 235" fill="none" stroke="#387183" stroke-width="8" stroke-opacity=".22"/>' +
      '<line class="link animated" x1="117" y1="229" x2="206" y2="111"/>' +
      '<line class="link animated ' + (failIsl ? "alert" : "") + '" x1="206" y1="111" x2="361" y2="89"/>' +
      '<line class="link animated" x1="361" y1="89" x2="511" y2="143"/>' +
      '<line class="link animated ' + (failGateway ? "alert" : "") + '" x1="511" y1="143" x2="648" y2="219"/>' +
      '<line class="link animated" x1="648" y1="219" x2="746" y2="235"/>' +
      node(117,229,9,"terminal","User terminal","Customer edge",false) +
      node(206,111,10,"spacecraft","Satellite A","Serving payload",false) +
      node(361,89,11,"space-network","Optical ISL",failIsl ? "TRACKING LOST" : "Laser mesh",failIsl) +
      node(511,143,10,"spacecraft","Satellite B","Onboard forwarding",false) +
      node(648,219,9,"ground-segment","Feeder gateway",failGateway ? "SITE DOWN" : "Ground antenna",failGateway) +
      node(746,235,9,"terrestrial-network","Internet PoP","Terrestrial exit",false) +
      '</svg></div>';
  };
  const scenarioCards = () => data.scenarios.map((s,i) =>
    '<button type="button" class="scenario-card ' + (state.scenario === s.id ? "selected " : "") + esc(s.tone) + '" data-scenario="' + esc(s.id) + '" aria-pressed="' + (state.scenario === s.id ? "true" : "false") + '">' +
    '<span class="circle">' + String(i+1).padStart(2,"0") + '</span><span><strong>' + esc(s.name) + '</strong><p>' + esc(s.detail) + '</p></span><em></em></button>').join("");
  const eventFeed = () => {
    const s = scenario();
    const events = s.id === "nominal" ?
      ["Contact windows updated","Optical mesh routing stable","Beam allocations refreshed","Ground feeder tracking nominal"] :
      s.id === "isl" ? ["Laser crosslink tracking lost","Topology graph revision +1","Alternate ISL selected","Customer RTT increased"] :
      s.id === "gateway" ? ["Feeder gateway out of service","Ground exit path withdrawn","Regional PoP alternative requested","Service availability reduced"] :
      ["Traffic demand rises by 38%","Beam allocation becomes saturated","Queue processing delay increases","Additional routing capacity evaluated"];
    return events.map((e,i) => '<li class="' + (i===0 && s.id!=="nominal" ? "alert" : "") + '"><strong>' + esc(e) + '</strong><small>' + esc(nowLabel()) + ' · ' + ["Space network","Network control","Access network","Service assurance"][i] + ' · demo</small></li>').join("");
  };
  const flowStrip = () => data.networkPath.map((step,i) =>
    '<a class="flow-node" href="' + href(step.id) + '"><b>' + esc(step.label) + '</b><small>Open subsystem ↗</small></a>' +
    (i < data.networkPath.length-1 ? '<span class="flow-arrow">›</span>' : "")).join("");
  const visibleSystems = () => data.systems.filter((s) =>
    (state.filter === "all" || s.group === state.filter) &&
    (s.name + " " + s.summary + " " + s.capabilities.join(" ") + " " + s.group).toLowerCase().includes(state.query.toLowerCase()));
  const catalogHTML = () => {
    const list = visibleSystems();
    if (!list.length) return '<div class="note-box">No systems match this filter. Change the search or category.</div>';
    return list.map((s) =>
      '<a class="system-card" href="' + href(s.id) + '">' +
      '<div class="top"><span class="code">' + esc(idOf(s)) + '</span><span class="num">SYS-' + esc(idOf(s)) + '</span></div>' +
      '<h3>' + esc(s.name) + '</h3><p>' + esc(s.summary) + '</p>' +
      '<div class="enter"><span>' + esc(groups[s.group].label) + '</span><b>Open workspace ↗</b></div></a>').join("");
  };
  const reportPanel = () => {
    if (!state.imported) return '<div class="import-panel"><div><strong>Bring a real simulation report into this demo</strong><p>Import a JSON report from <code>ntarlink-cli simulate … --json</code>. The file stays in your browser and is not uploaded.</p></div>' +
      '<button class="btn secondary small" type="button" data-choose-file>Import report JSON</button></div>';
    const report = state.imported;
    const packets = report.packets.slice(0,3).map((p) => {
      const hops = (p.hops || []).map((h) => h.link_id).join(" → ");
      return '<div class="component-row"><span>↗</span><div><b>' + esc(p.packet_id) + ' · ' + esc(p.status) + '</b><div style="color:#88abbd;margin-top:5px">' + esc(hops || "No hops") + '</div></div></div>';
    }).join("");
    return '<div class="panel panel-pad" style="margin-top:17px"><div class="panel-head"><h2>Imported local Rust report</h2><span class="pill green">IMPORTED • NOT MOCK</span></div>' +
      '<p style="font-size:11px">File: ' + esc(state.importedFile) + ' · Scenario: ' + esc(report.scenario) + ' · Fidelity: ' + esc(report.fidelity) + '</p>' +
      '<div class="telemetry-strip"><div><small>Packets injected</small><strong>' + esc(report.injected) + '</strong></div><div><small>Delivered</small><strong>' + esc(report.delivered) + '</strong></div><div><small>Dropped</small><strong>' + esc(report.dropped) + '</strong></div></div>' +
      '<div class="component-map" style="margin-top:15px">' + packets + '</div><div class="detail-actions" style="margin-top:15px"><button class="btn tertiary small" data-clear-report>Clear import</button><button class="btn secondary small" data-choose-file>Import another</button></div></div>';
  };
  const home = () => {
    const s = scenario();
    const metrics = [
      ["Active satellites","4,128","","Orbit catalog"],
      ["Optical crosslinks",s.links.toLocaleString("en-US"),"","Space mesh"],
      ["Feeder stations",s.stations,"","Ground segment"],
      ["Modeled network RTT",s.latency.toFixed(1),"ms","End-to-end path"]
    ];
    const metricGrid = metrics.map((m)=>kpi(...m)).join("");
    const filters = [{id:"all",label:"All 16"},...data.groups.map((g)=>({id:g.id,label:g.label}))].map((g)=>
      '<button type="button" class="' + (state.filter===g.id?"active":"") + '" data-filter="' + esc(g.id) + '" aria-pressed="' + (state.filter===g.id?"true":"false") + '">' + esc(g.label) + '</button>').join("");
    return '<div class="hero-intro"><div><div class="eyebrow">MISSION CONTROL / NETWORK OPERATIONS</div><h1>Satellite Network Overview</h1>' +
      '<p>One operating picture for the complete LEO broadband system. Explore network paths, inspect simulated events, and drill into all 16 subsystem workspaces.</p></div>' +
      '<div class="hero-buttons"><button class="btn secondary" type="button" data-scroll="systems">Explore subsystems ↘</button><a class="btn" href="' + href("simulation") + '">Open simulation ↗</a><button class="btn tertiary" type="button" data-export>Export mock snapshot</button></div></div>' +
      '<div class="system-alert"><span><strong>DEMO ENVIRONMENT.</strong> ' + esc(sourceNote) + '</span><span class="source">● MOCK / OFFLINE</span></div>' +
      '<div class="metric-grid">' + metricGrid + '</div>' +
      '<div class="home-panels">' +
        '<section class="panel panel-pad"><div class="panel-head"><h2>Constellation Network / Conceptual Topology</h2><span class="pill">INTERACTIVE</span></div>' +
          svgMap() + '<div class="map-key"><span>Network node</span><span>Space route</span><span>Ground exit</span><small>Click a node to open its subsystem.</small></div></section>' +
        '<section class="panel panel-pad"><div class="panel-head"><h2>Experiment Scenarios</h2><span class="hint">LOCAL STATE</span></div>' +
          '<div class="scenario-stack">' + scenarioCards() + '</div>' +
          '<div class="timeline"><button type="button" data-play aria-label="Play or pause simulated clock">' + (state.playing ? "Ⅱ" : "▶") + '</button><input aria-label="Simulated time" type="range" min="0" max="120" step="1" value="' + state.time + '" id="clock-range"><label id="clock-value">' + nowLabel() + '</label></div>' +
        '</section></div>' +
      '<div class="lower-panels">' +
        '<section class="panel panel-pad"><div class="panel-head"><h2>Broadband Packet Journey</h2><a class="micro-link" href="' + href("network-control") + '">Routing controls ↗</a></div><div class="flow-list">' + flowStrip() + '</div></section>' +
        '<section class="panel panel-pad"><div class="panel-head"><h2>Illustrative Event Stream</h2><span class="pill demo">MOCK EVENTS</span></div><ol class="event-feed">' + eventFeed() + '</ol></section></div>' +
      '<section id="systems"><div class="section-heading"><div><div class="eyebrow">SYSTEM ARCHITECTURE</div><h2>Explore all 16 subsystems</h2>' +
        '<p>Each workspace has its own assets, example operations, telemetry and engineering context.</p></div><a class="micro-link" href="https://github.com/BestNathan/ntarlink/tree/main/docs/subsystems" target="_blank" rel="noopener">Engineering specifications ↗</a></div>' +
        '<div class="catalog-toolbar"><input class="search" id="system-search" type="search" autocomplete="off" placeholder="Search systems and capabilities…" aria-label="Filter subsystems" value="' + esc(state.query) + '">' +
        '<div class="filter-list">' + filters + '</div></div><div class="system-grid" id="system-grid">' + catalogHTML() + '</div></section>' +
      reportPanel() + '<input type="file" accept=".json,application/json" id="report-file" class="import-report" aria-label="Import local simulation report">';
  };
  const detailHeader = (s) =>
    '<div class="detail-hero"><div><div class="system-code">SYS-' + idOf(s) + ' / ' + esc(groups[s.group].label.toUpperCase()) + '</div>' +
    '<h1>' + esc(s.name) + '</h1><p>' + esc(s.summary) + '</p><div class="detail-actions">' + modelStatus(s.model) +
    '<span class="pill demo">ALL TELEMETRY MOCK</span><a class="btn tertiary small" href="https://github.com/BestNathan/ntarlink/blob/main/docs/subsystems/' + idOf(s) + '-' + (s.id === "orbital" ? "orbital-dynamics" : s.id === "spacecraft" ? "spacecraft" : s.id === "space-network" ? "space-network" : s.id === "terminal" ? "terminal" : s.id === "access-network" ? "access-network" : s.id === "ground-segment" ? "ground-segment" : s.id === "subscriber-core" ? "subscriber-core" : s.id === "terrestrial-network" ? "terrestrial-network" : s.id === "network-control" ? "network-control" : s.id === "operations" ? "operations" : s.id === "security" ? "security" : s.id === "environment" ? "environment" : s.id === "service-assurance" ? "service-assurance" : s.id === "simulation" ? "simulation" : s.id === "emulation" ? "emulation" : "data-observability") + '.md" target="_blank" rel="noopener">Open engineering design ↗</a></div></div>' +
    '<div class="detail-emblem" aria-hidden="true">' + idOf(s) + '</div></div>';
  const detailTabs = (s,active) => '<nav class="tabs" aria-label="Subsystem views">' + [
    ["overview","Overview"],["assets","Assets & Inventory"],["operations","Operations"],["telemetry","Telemetry & Analytics"]
  ].map(([tab,label])=>'<a class="tab ' + (active===tab?"active":"") + '" href="' + href(s.id,tab) + '" aria-current="' + (active===tab?"page":"false") + '">' + label + '</a>').join("") + '</nav>';
  const detailKpis = (s) => '<div class="metric-grid">' + s.metrics.map((m) => kpi(m.label,m.value,m.unit,m.trend||"Example value")).join("") + kpi("Data source","MOCK","","Synthetic per subsystem") + '</div>';
  const overviewTab = (s) =>
    '<div class="detail-grid">' +
      '<section class="panel panel-pad"><div class="panel-head"><h2>Primary capabilities</h2><span class="pill">SYS-' + idOf(s) + '</span></div><div class="capabilities">' +
      s.capabilities.map((c,i)=>'<div class="capability"><small>CAPABILITY / 0' + (i+1) + '</small><b>' + esc(c) + '</b><p>Owner: ' + esc(s.owner) + '. Explore its state, inputs and validation in the engineering specification.</p></div>').join("") +
      '</div></section><section class="panel panel-pad"><div class="panel-head"><h2>Internal components</h2><span class="hint">LOGICAL DESIGN</span></div><div class="component-map">' +
      s.components.map((c,i)=>'<div class="component-row"><span>0' + (i+1) + '</span>' + esc(c) + '</div>').join("") +
      '</div><div class="note-box" style="margin-top:15px"><b>System ownership</b><br>' + esc(s.owner) + '. Components describe architectural boundaries, not deployed production microservices.</div></section></div>' +
      '<div class="section-heading"><div><h2>Working surfaces</h2><p>Each tab is navigable and has independent demo interactions.</p></div></div>' +
      '<div class="system-grid" style="grid-template-columns:repeat(auto-fit,minmax(190px,1fr))">' +
      [["assets","Inventory Explorer","Inspect individual entities and their sample operating state."],
      ["operations","Domain Operations","Run illustrative local actions and review an event history."],
      ["telemetry","Telemetry Analyzer","Inspect deterministic synthetic graphs and status metrics."]].map((x)=>'<a class="system-card" style="min-height:145px" href="' + href(s.id,x[0]) + '"><div class="top"><span class="code">↗</span></div><h3>' + x[1] + '</h3><p>' + x[2] + '</p><div class="enter">Open view ↗</div></a>').join("") + '</div>';
  const assetsTab = (s) =>
    '<section class="panel panel-pad"><div class="asset-toolbar"><div><h2>' + esc(s.name) + ' / Inventory</h2><p style="margin:5px 0 0;font-size:11px">Illustrative entities and status. Inspect any row for detail.</p></div><span class="pill demo">4 SYNTHETIC RECORDS</span></div>' +
    '<div class="table-wrap"><table><thead><tr><th>Entity ID</th><th>Region / Domain</th><th>State</th><th>Operating reading</th><th>Inspect</th></tr></thead><tbody>' +
    s.assets.map((a)=>'<tr><td>' + esc(a.id) + '</td><td>' + esc(a.location) + '</td><td>' + htmlStatus(state.statuses[s.id+"|"+a.id] || a.status) + '</td><td>' + esc(a.metric) + '</td><td><button class="table-action" type="button" data-inspect="' + esc(s.id) + '" data-asset="' + esc(a.id) + '">Open ↗</button></td></tr>').join("") +
    '</tbody></table></div></section><div class="note-box" style="margin-top:13px">Records are hand-crafted synthetic examples. No satellite catalog, actual subscriber account or ground infrastructure service is queried.</div>';
  const operationsTab = (s) =>
    '<div class="operations-layout"><section class="panel panel-pad"><div class="panel-head"><h2>Subsystem controls</h2><span class="pill demo">SIMULATED ACTIONS ONLY</span></div>' +
    '<div class="note-box" style="margin-bottom:12px">These buttons only change client-side mock state and append a local event. They do not contact the Rust simulation runtime, external systems or hardware.</div>' +
    '<div class="action-list">' +
    s.actions.map((a,i)=>'<div class="action-card"><div><strong>' + esc(a.label) + '</strong><p>' + esc(a.detail) + '</p><small>Target: ' + esc(a.target || "Domain-wide") + '</small></div>' +
      '<button type="button" class="btn small" data-action="' + esc(s.id) + '" data-operation="' + i + '">Run mock ↗</button></div>').join("") +
    '</div></section><section class="panel panel-pad"><div class="panel-head"><h2>Event journal</h2><span class="hint">LOCAL HISTORY</span></div><div class="activity">' +
    activityFor(s).map((e,i)=>'<div class="activity-item"><span class="number">' + String(i+1).padStart(2,"0") + '</span><div><b>' + esc(e.title) + '</b><p>' + esc(e.detail) + '<br>' + esc("T+"+String(Math.floor(e.minute/60)).padStart(2,"0")+":"+String(e.minute%60).padStart(2,"0")) + ' · synthetic</p></div></div>').join("") +
    '</div></section></div>';
  const graphData = (s) => {
    const index = data.systems.indexOf(s);
    const scenarioOffset = data.scenarios.findIndex((x)=>x.id===state.scenario)*4;
    const ypoints = Array.from({length:42},(_,i)=>Math.max(15,Math.min(89, 51+12*Math.sin(i*.34+index*.5)+8*Math.cos(i*.73+scenarioOffset)+3*Math.sin(i*.23+index))));
    const points = ypoints.map((val,i)=>[25+i*(570/(ypoints.length-1)),210-val*1.8]);
    const d = points.map(([x,y],i)=>(i?"L":"M")+x.toFixed(1)+" "+y.toFixed(1)).join(" ");
    return '<div class="chart-wrap"><div class="panel-head"><h2>Example utilization trend</h2><span class="chart-legend"><i></i> DEMO SIGNAL</span></div>' +
      '<svg viewBox="0 0 620 240" role="img" aria-label="Deterministic illustrative subsystem utilization trend">' +
      '<g class="chart-grid"><line x1="20" y1="45" x2="610" y2="45"/><line x1="20" y1="100" x2="610" y2="100"/><line x1="20" y1="155" x2="610" y2="155"/><line x1="20" y1="210" x2="610" y2="210"/></g>' +
      '<path d="' + d + ' L595 222 L25 222 Z" fill="#64e7c9" fill-opacity=".10"/><path d="' + d + '" fill="none" stroke="#6ae8c9" stroke-width="2.3" stroke-linejoin="round" stroke-linecap="round"/>' +
      points.filter((_,i)=>i%7===0).map(([x,y])=>'<circle cx="'+x.toFixed(1)+'" cy="'+y.toFixed(1)+'" r="3.5" fill="#83ffdb"/>').join("") +
      '</svg><div class="chart-labels"><span>T−40 min</span><span>T−20 min</span><span>NOW / '+esc(nowLabel())+'</span></div></div>';
  };
  const telemetryTab = (s) => '<div class="detail-grid"><section class="panel panel-pad"><div class="panel-head"><h2>Domain telemetry</h2><span class="pill demo">MOCK TIMESERIES</span></div>' + graphData(s) + '</section>' +
    '<section class="panel panel-pad"><div class="panel-head"><h2>Engineering signals</h2><span class="hint">MODEL BOUNDARIES</span></div>' +
    '<div class="component-map">' + s.metrics.map((m,i)=>'<div class="component-row"><span>0'+(i+1)+'</span><div style="flex:1"><strong>'+esc(m.label)+'</strong><div style="color:#80a9ba;margin-top:3px">'+esc(m.value)+' '+esc(m.unit)+'</div></div><span style="width:auto;padding:0 7px">mock</span></div>').join("") + '</div>' +
    '<div class="note-box" style="margin-top:14px"><b>Implementation fidelity</b><br>' + (s.model==="partial"?"There are working repository components in this domain, but this chart is generated from mock values and is not collected from those components.":"This system is currently a design-stage domain. No operational service is queried.") + '</div>' +
    '<div style="margin-top:15px"><strong style="font-size:11px">External components under consideration</strong><p style="font-size:11px;line-height:1.8">'+esc(s.dependencies.join(" · "))+'</p></div></section></div>';
  const detail = (s, tab) =>
    '<div class="hero-intro" style="margin-bottom:18px"><a class="micro-link" href="#/overview">← Return to mission overview</a><span class="pill demo">ARCHITECTURAL PROTOTYPE</span></div>' +
    detailHeader(s) + detailKpis(s) + detailTabs(s,tab) +
    (tab==="overview" ? overviewTab(s) : tab==="assets" ? assetsTab(s) : tab==="operations" ? operationsTab(s) : telemetryTab(s));
  function render() {
    const current = page();
    const scroller = document.querySelector(".sidebar");
    const oldScroll = scroller ? scroller.scrollTop : 0;
    $("app").innerHTML = layout(current, current.kind==="overview" ? home() : detail(systems[current.id],current.tab));
    if ($("sidebar")) $("sidebar").scrollTop = oldScroll;
    if (state.imported && $("report-file")) $("report-file").value = "";
    document.title = (current.kind==="overview" ? "Mission Control" : systems[current.id].name + " / " + current.tab) + " | ntarlink Demo";
  }
  function announce(title,message) {
    const root = $("toast-root");
    if (!root) return;
    root.innerHTML = '<div class="toast" role="status"><strong>' + esc(title) + '</strong><span>' + esc(message) + '</span></div>';
    clearTimeout(announce.timer);
    announce.timer = setTimeout(() => {if (root) root.textContent="";},3800);
  }
  function inspect(systemId,assetId) {
    const s = systems[systemId];if (!s)return;
    const a = s.assets.find((r)=>r.id===assetId);if (!a)return;
    const status=state.statuses[s.id+"|"+a.id]||a.status;
    const html = '<div class="inspector-overlay" data-overlay><div class="inspector" role="dialog" aria-modal="true" aria-labelledby="inspector-title">' +
      '<button type="button" class="close" data-close aria-label="Close inspector">×</button><div class="eyebrow">ENTITY INSPECTOR / MOCK</div>' +
      '<h2 id="inspector-title">' + esc(a.id) + '</h2><p>' + esc(s.name) + ' · Example record. No real asset has been queried.</p>' +
      '<dl><dt>Entity</dt><dd>' + esc(a.id) + '</dd><dt>Domain</dt><dd>' + esc(s.name) + '</dd><dt>Region</dt><dd>' + esc(a.location) + '</dd>' +
      '<dt>Current state</dt><dd>' + htmlStatus(status) + '</dd><dt>Example reading</dt><dd>' + esc(a.metric) + '</dd><dt>Data source</dt><dd>Local synthetic dataset</dd></dl>' +
      '<h4>Available capability</h4><p>' + esc(s.capabilities[0]) + '</p>' +
      '<div class="detail-actions"><a class="btn secondary small" href="' + href(s.id,"operations") + '" data-close>Open subsystem operations ↗</a><button class="btn tertiary small" data-close>Close</button></div></div></div>';
    $("dialog-root").innerHTML=html;
    const close=$("dialog-root").querySelector(".close");if (close) close.focus();
  }
  function mutate(systemId,operationIndex) {
    const s=systems[systemId];if(!s)return;
    const a=s.actions[operationIndex];if(!a)return;
    const target=a.target;
    if(target){
      const match=s.assets.find((x)=>x.id===target);
      if(match){
        const key=systemId+"|"+target;
        const current=state.statuses[key]||match.status;
        state.statuses[key]=current==="nominal"?"warning":"nominal";
      }
    }
    if(a.label.toLowerCase().includes("reset virtual clock"))state.time=0;
    addActivity(systemId,a.label, "Executed locally in the demo UI for "+(target||"the subsystem")+". No backend command was sent.");
    render();
    announce("Simulated action recorded",a.label+" · local state updated");
  }
  function switchScenario(id) {
    if(!data.scenarios.some((x)=>x.id===id))return;
    state.scenario=id;
    addActivity("network-control","Scenario selected: "+scenario().name, "Scenario metrics and topology are simulated examples.");
    render();
    announce("Demo scenario changed",scenario().detail);
  }
  function updateClock() {
    const label=$("clock-value");
    if(label)label.textContent=nowLabel();
  }
  function stopClock() {
    if(state.timer){clearInterval(state.timer);state.timer=null;}
    state.playing=false;
  }
  function startClock() {
    if(state.playing){stopClock();render();return;}
    state.playing=true;
    if(state.time>=120)state.time=0;
    render();
    state.timer=setInterval(()=>{
      state.time+=2;
      if(state.time>=120){state.time=120;stopClock();render();return;}
      const range=$("clock-range");
      if(range)range.value=String(state.time);
      updateClock();
    },380);
  }
  function exportDemo() {
    const json={kind:"ntarlink-demo-local-state",source:"SYNTHETIC",generated_at:new Date().toISOString(),scenario:state.scenario,time_minutes:state.time,system_count:16,actions:state.activity,status_overrides:state.statuses};
    const blob=new Blob([JSON.stringify(json,null,2)],{type:"application/json"});
    const url=URL.createObjectURL(blob);
    const a=document.createElement("a");a.href=url;a.download="ntarlink-demo-snapshot.json";document.body.append(a);a.click();a.remove();setTimeout(()=>URL.revokeObjectURL(url),1000);
    announce("Synthetic snapshot exported","The JSON contains only local mock state and demo actions.");
  }
  function importReport(input) {
    const file=input.files && input.files[0];if(!file)return;
    if(file.size>5_000_000){announce("Report too large","The local JSON import limit is 5 MB.");return;}
    const reader=new FileReader();
    reader.onload=()=>{
      try{
        const v=JSON.parse(String(reader.result));
        if(!v||typeof v.scenario!=="string"||typeof v.fidelity!=="string"||!Array.isArray(v.packets)||!Number.isInteger(v.injected)||!Number.isInteger(v.delivered)||!Number.isInteger(v.dropped)||v.injected<0||v.delivered<0||v.dropped<0||v.delivered+v.dropped>v.injected)throw new Error("Unrecognized ntarlink SimulationReport schema");
        state.imported={scenario:v.scenario,fidelity:v.fidelity,packets:v.packets.slice(0,1000),injected:v.injected,delivered:v.delivered,dropped:v.dropped};
        state.importedFile=file.name;
        render();announce("Local simulation report loaded","Imported packet results are labeled separately from synthetic dashboard values.");
      }catch(error){announce("Unable to import report",String(error.message||error));}
    };
    reader.onerror=()=>announce("Unable to read file","The file could not be read locally.");
    reader.readAsText(file);
  }
  document.addEventListener("click",(e)=>{
    const target=e.target;
    if(!(target instanceof Element))return;
    const menu=target.closest("[data-menu]");
    if(menu){state.menu=!state.menu;render();return;}
    const scroll=target.closest("[data-scroll]");
    if(scroll){const section=$(scroll.getAttribute("data-scroll"));if(section)section.scrollIntoView({behavior:"smooth",block:"start"});return;}
    const n=target.closest("[data-scenario]");
    if(n){switchScenario(n.getAttribute("data-scenario"));return;}
    const go=target.closest("[data-go]");
    if(go){window.location.hash=href(go.getAttribute("data-go"));return;}
    const f=target.closest("[data-filter]");
    if(f){state.filter=f.getAttribute("data-filter");render();return;}
    if(target.closest("[data-play]")){startClock();return;}
    const a=target.closest("[data-action]");
    if(a){mutate(a.getAttribute("data-action"),Number(a.getAttribute("data-operation")));return;}
    const ins=target.closest("[data-inspect]");
    if(ins){inspect(ins.getAttribute("data-inspect"),ins.getAttribute("data-asset"));return;}
    if(target.closest("[data-export]")){exportDemo();return;}
    if(target.closest("[data-choose-file]")){const input=$("report-file");if(input)input.click();return;}
    if(target.closest("[data-clear-report]")){state.imported=null;state.importedFile="";render();return;}
    if(target.closest("[data-close]")){$("dialog-root").textContent="";return;}
    if(target.hasAttribute("data-overlay")){$("dialog-root").textContent="";return;}
    if(target.closest("a[href^='#/']")){state.menu=false;}
  });
  document.addEventListener("input",(e)=>{
    if(e.target.id==="system-search"){
      state.query=e.target.value;
      if($("system-grid"))$("system-grid").innerHTML=catalogHTML();
    }
    if(e.target.id==="clock-range"){state.time=Number(e.target.value);updateClock();}
  });
  document.addEventListener("change",(e)=>{if(e.target.id==="report-file")importReport(e.target);});
  document.addEventListener("keydown",(e)=>{
    if(e.key==="Escape"){
      const root=$("dialog-root");if(root)root.textContent="";
      if(state.menu){state.menu=false;render();}
    }
    if((e.key==="Enter"||e.key===" ") && e.target instanceof Element && e.target.hasAttribute("data-go")){
      e.preventDefault();window.location.hash=href(e.target.getAttribute("data-go"));
    }
  });
  window.addEventListener("hashchange",()=>{state.menu=false;render();window.scrollTo(0,0);});
  render();
})();
