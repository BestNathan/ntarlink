/* All telemetry is synthetic demo data. No remote services are queried. */
window.NTARLINK_DEMO = {
  "version": 1,
  "label": "SYNTHETIC DEMO DATA",
  "groups": [
    {
      "id": "space",
      "label": "Space Segment",
      "subtitle": "Orbit, spacecraft, inter-satellite transport",
      "symbol": "◈"
    },
    {
      "id": "access",
      "label": "User & Access",
      "subtitle": "Devices and broadband radio access",
      "symbol": "⌁"
    },
    {
      "id": "ground",
      "label": "Ground & Core",
      "subtitle": "Feeder facilities, subscriber services, internet",
      "symbol": "▤"
    },
    {
      "id": "control",
      "label": "Network Control",
      "subtitle": "Topology and traffic engineering",
      "symbol": "◉"
    },
    {
      "id": "operations",
      "label": "Operations & Assurance",
      "subtitle": "Security, environment, service operations",
      "symbol": "◎"
    },
    {
      "id": "platform",
      "label": "Simulation Platform",
      "subtitle": "Execution, emulation, observability",
      "symbol": "▦"
    }
  ],
  "systems": [
    {
      "id": "orbital",
      "name": "Orbital Dynamics",
      "group": "space",
      "summary": "Earth-fixed ephemerides, contact windows, and constellation visibility geometry.",
      "owner": "Physical position and contact geometry",
      "model": "partial",
      "metrics": [
        {
          "label": "Tracked objects",
          "value": "4,128",
          "unit": "",
          "trend": ""
        },
        {
          "label": "Predicted contacts",
          "value": "12,486",
          "unit": "",
          "trend": ""
        },
        {
          "label": "Propagation error",
          "value": "0.6",
          "unit": "km",
          "trend": ""
        }
      ],
      "capabilities": [
        "TLE / SGP4 propagation",
        "TEME → ITRS / WGS84 reference frames",
        "Ground visibility and Earth occultation",
        "Contact-window scheduling and Doppler"
      ],
      "components": [
        "Orbit catalog",
        "Propagation worker",
        "Contact geometry",
        "Ephemeris store"
      ],
      "assets": [
        {
          "id": "LEO-SAT-0412",
          "location": "Shell A · 550 km",
          "status": "nominal",
          "metric": "7.6 km/s"
        },
        {
          "id": "LEO-SAT-0671",
          "location": "Shell A · 550 km",
          "status": "nominal",
          "metric": "7.6 km/s"
        },
        {
          "id": "LEO-SAT-1209",
          "location": "Shell B · 530 km",
          "status": "nominal",
          "metric": "7.7 km/s"
        },
        {
          "id": "LEO-SAT-2431",
          "location": "Shell C · 570 km",
          "status": "warning",
          "metric": "7.5 km/s"
        }
      ],
      "actions": [
        {
          "label": "Recompute passes",
          "detail": "Regenerate example visibility windows",
          "target": "LEO-SAT-0412"
        },
        {
          "label": "Inject ephemeris drift",
          "detail": "Illustrate stale orbit propagation inputs",
          "target": "LEO-SAT-2431"
        },
        {
          "label": "Refresh orbit catalog",
          "detail": "Record a simulated orbit catalog refresh",
          "target": "LEO-SAT-0671"
        }
      ],
      "dependencies": [
        "Skyfield",
        "SGP4",
        "Orekit (candidate)"
      ],
      "events": [
        "Ephemeris sample generated",
        "Contact window opened",
        "Predicted pass updated"
      ],
      "accent": "#84c3fc"
    },
    {
      "id": "spacecraft",
      "name": "Spacecraft & Constellation",
      "group": "space",
      "summary": "Spacecraft inventory, payload states, attitude, and onboard resource constraints.",
      "owner": "Spacecraft and payload capability",
      "model": "design",
      "metrics": [
        {
          "label": "Active spacecraft",
          "value": "4,112",
          "unit": "",
          "trend": ""
        },
        {
          "label": "Payload availability",
          "value": "99.4",
          "unit": "%",
          "trend": ""
        },
        {
          "label": "Fleet health",
          "value": "97.8",
          "unit": "%",
          "trend": ""
        }
      ],
      "capabilities": [
        "Fleet lifecycle and inventory",
        "Attitude and pointing limits",
        "Power and thermal resource budget",
        "Payload health and safe mode"
      ],
      "components": [
        "Fleet registry",
        "Payload manager",
        "Attitude controller",
        "Health monitor"
      ],
      "assets": [
        {
          "id": "SAT-0182",
          "location": "Shell A · Plane 12",
          "status": "nominal",
          "metric": "80% power"
        },
        {
          "id": "SAT-0454",
          "location": "Shell B · Plane 04",
          "status": "nominal",
          "metric": "73% power"
        },
        {
          "id": "SAT-1028",
          "location": "Shell C · Plane 19",
          "status": "warning",
          "metric": "61% power"
        },
        {
          "id": "SAT-2041",
          "location": "Shell A · Plane 21",
          "status": "nominal",
          "metric": "88% power"
        }
      ],
      "actions": [
        {
          "label": "Request safe mode",
          "detail": "Simulate a spacecraft entering safe mode",
          "target": "SAT-1028"
        },
        {
          "label": "Release payload",
          "detail": "Simulate a payload resource release",
          "target": "SAT-0454"
        },
        {
          "label": "Inspect power budget",
          "detail": "Show spacecraft resource diagnostics",
          "target": "SAT-0182"
        }
      ],
      "dependencies": [
        "Orekit (candidate)",
        "Rust"
      ],
      "events": [
        "Spacecraft health snapshot",
        "Payload resource derated",
        "Attitude target updated"
      ],
      "accent": "#a8b7ff"
    },
    {
      "id": "space-network",
      "name": "Space Network / Optical ISL",
      "group": "space",
      "summary": "Laser crosslink acquisition and directional packet forwarding in the space mesh.",
      "owner": "Acquired optical links and forwarding queues",
      "model": "partial",
      "metrics": [
        {
          "label": "Active ISLs",
          "value": "9,604",
          "unit": "",
          "trend": ""
        },
        {
          "label": "Mean ISL delay",
          "value": "3.4",
          "unit": "ms",
          "trend": ""
        },
        {
          "label": "Mesh utilization",
          "value": "62.8",
          "unit": "%",
          "trend": ""
        }
      ],
      "capabilities": [
        "Geometric line-of-sight verification",
        "Optical PAT acquisition and tracking",
        "Onboard packet queues and forwarding",
        "ISL link budgets and path resilience"
      ],
      "components": [
        "Optical terminal pool",
        "PAT link state machine",
        "ISL transmission queue",
        "Onboard FIB"
      ],
      "assets": [
        {
          "id": "ISL-018↔072",
          "location": "Orbital plane 12",
          "status": "nominal",
          "metric": "4.8 Gbps"
        },
        {
          "id": "ISL-031↔114",
          "location": "Plane 18 crossing",
          "status": "nominal",
          "metric": "3.9 Gbps"
        },
        {
          "id": "ISL-022↔047",
          "location": "Shell A",
          "status": "warning",
          "metric": "2.8 Gbps"
        },
        {
          "id": "ISL-088↔101",
          "location": "Shell B",
          "status": "nominal",
          "metric": "4.5 Gbps"
        }
      ],
      "actions": [
        {
          "label": "Simulate ISL loss",
          "detail": "Mark crosslink unavailable in demo",
          "target": "ISL-022↔047"
        },
        {
          "label": "Reacquire laser link",
          "detail": "Model a recovered PAT state",
          "target": "ISL-018↔072"
        },
        {
          "label": "Flush forwarding queue",
          "detail": "Simulate packet-queue maintenance",
          "target": "ISL-031↔114"
        }
      ],
      "dependencies": [
        "Rust packet runtime",
        "ns-3 (candidate)",
        "SNS3 (candidate)"
      ],
      "events": [
        "Optical terminal tracking locked",
        "ISL queue utilization changed",
        "Crosslink route switched"
      ],
      "accent": "#7bdccf"
    },
    {
      "id": "terminal",
      "name": "User Terminals & CPE",
      "group": "access",
      "summary": "Customer terminal devices, antenna steering, modem state, and LAN connectivity.",
      "owner": "Terminal hardware and modem state",
      "model": "design",
      "metrics": [
        {
          "label": "Connected terminals",
          "value": "1.21",
          "unit": "M",
          "trend": ""
        },
        {
          "label": "Typical acquisition",
          "value": "18",
          "unit": "s",
          "trend": ""
        },
        {
          "label": "Antenna tracking",
          "value": "98.1",
          "unit": "%",
          "trend": ""
        }
      ],
      "capabilities": [
        "Terminal lifecycle and onboarding",
        "Antenna pointing and obstacle masks",
        "Modem acquisition and tracking",
        "Local CPE and IP interface"
      ],
      "components": [
        "Terminal registry",
        "Antenna controller",
        "Modem state machine",
        "LAN bridge"
      ],
      "assets": [
        {
          "id": "UT-00821",
          "location": "Virginia Beach · US",
          "status": "nominal",
          "metric": "98% signal"
        },
        {
          "id": "UT-04712",
          "location": "Rural Nevada · US",
          "status": "nominal",
          "metric": "96% signal"
        },
        {
          "id": "UT-01804",
          "location": "Atlantic Maritime",
          "status": "warning",
          "metric": "81% signal"
        },
        {
          "id": "UT-09633",
          "location": "Southern Australia",
          "status": "nominal",
          "metric": "94% signal"
        }
      ],
      "actions": [
        {
          "label": "Restart modem",
          "detail": "Simulate local device restart",
          "target": "UT-01804"
        },
        {
          "label": "Request reacquisition",
          "detail": "Model satellite candidate scanning",
          "target": "UT-04712"
        },
        {
          "label": "Check antenna alignment",
          "detail": "Show pointing diagnostic event",
          "target": "UT-00821"
        }
      ],
      "dependencies": [
        "Rust",
        "Linux TAP (candidate)"
      ],
      "events": [
        "Terminal attached to beam",
        "Antenna pointing correction",
        "Terminal modem re-acquired"
      ],
      "accent": "#efc18a"
    },
    {
      "id": "access-network",
      "name": "User Access Network",
      "group": "access",
      "summary": "Spot beams, spectrum allocation, RF link quality, MAC scheduling, and mobility.",
      "owner": "Beam resources and radio admission",
      "model": "design",
      "metrics": [
        {
          "label": "Active user beams",
          "value": "14,380",
          "unit": "",
          "trend": ""
        },
        {
          "label": "Mean access SINR",
          "value": "18.4",
          "unit": "dB",
          "trend": ""
        },
        {
          "label": "Beam load",
          "value": "72.6",
          "unit": "%",
          "trend": ""
        }
      ],
      "capabilities": [
        "Beam footprints and frequency reuse",
        "RF link budget / SINR / MODCOD",
        "MAC resource scheduling and fairness",
        "Terminal admission and handover"
      ],
      "components": [
        "Beam allocator",
        "Spectrum planner",
        "RF link model",
        "Handover controller"
      ],
      "assets": [
        {
          "id": "BEAM-A102",
          "location": "North America East",
          "status": "nominal",
          "metric": "68% load"
        },
        {
          "id": "BEAM-B044",
          "location": "Western Europe",
          "status": "nominal",
          "metric": "74% load"
        },
        {
          "id": "BEAM-C510",
          "location": "North Pacific",
          "status": "warning",
          "metric": "93% load"
        },
        {
          "id": "BEAM-A301",
          "location": "Southeast Asia",
          "status": "nominal",
          "metric": "63% load"
        }
      ],
      "actions": [
        {
          "label": "Rebalance beam capacity",
          "detail": "Simulate moving resource allocation",
          "target": "BEAM-C510"
        },
        {
          "label": "Trigger beam handover",
          "detail": "Model access transition",
          "target": "BEAM-A102"
        },
        {
          "label": "Inject co-channel load",
          "detail": "Model a degraded RF environment",
          "target": "BEAM-B044"
        }
      ],
      "dependencies": [
        "SNS3 (candidate)",
        "OpenSAND (candidate)",
        "OAI NTN (optional)"
      ],
      "events": [
        "Beam allocation adjusted",
        "Terminal handover completed",
        "Beam resource saturation"
      ],
      "accent": "#ebc576"
    },
    {
      "id": "ground-segment",
      "name": "Ground Stations & Feeder",
      "group": "ground",
      "summary": "Radio feeder ground sites, antenna tracking, site availability, and gateway handoff.",
      "owner": "Feeder sites and RF termination",
      "model": "design",
      "metrics": [
        {
          "label": "Feeder sites",
          "value": "186",
          "unit": "",
          "trend": ""
        },
        {
          "label": "Feeder links up",
          "value": "412",
          "unit": "",
          "trend": ""
        },
        {
          "label": "Site availability",
          "value": "99.7",
          "unit": "%",
          "trend": ""
        }
      ],
      "capabilities": [
        "Ground site/antenna inventory",
        "Feeder RF link budgets",
        "Antenna tracking and acquisition",
        "Site capacity and gateway failure"
      ],
      "components": [
        "Station manager",
        "Antenna tracker",
        "Feeder modem",
        "Backhaul handoff"
      ],
      "assets": [
        {
          "id": "GS-001",
          "location": "Washington · US",
          "status": "nominal",
          "metric": "6.4 Gbps"
        },
        {
          "id": "GS-017",
          "location": "Bordeaux · France",
          "status": "nominal",
          "metric": "7.2 Gbps"
        },
        {
          "id": "GS-042",
          "location": "Western Australia",
          "status": "warning",
          "metric": "4.1 Gbps"
        },
        {
          "id": "GS-090",
          "location": "Hokkaido · Japan",
          "status": "nominal",
          "metric": "6.8 Gbps"
        }
      ],
      "actions": [
        {
          "label": "Simulate station outage",
          "detail": "Withdraw a modeled feeder facility",
          "target": "GS-042"
        },
        {
          "label": "Switch feeder antenna",
          "detail": "Simulate site antenna reassignment",
          "target": "GS-001"
        },
        {
          "label": "Apply weather fade",
          "detail": "Show feeder attenuation impact",
          "target": "GS-017"
        }
      ],
      "dependencies": [
        "OpenSAND (candidate)",
        "GNU Radio (candidate)"
      ],
      "events": [
        "Feeder acquisition completed",
        "Ground antenna reassigned",
        "Station availability degraded"
      ],
      "accent": "#8fdde0"
    },
    {
      "id": "subscriber-core",
      "name": "Subscriber Core",
      "group": "ground",
      "summary": "Subscriber authorization, service sessions, IP assignment, policy, and service continuity.",
      "owner": "Subscriber sessions and IP policy",
      "model": "design",
      "metrics": [
        {
          "label": "Service sessions",
          "value": "1.21",
          "unit": "M",
          "trend": ""
        },
        {
          "label": "IP pool occupancy",
          "value": "63",
          "unit": "%",
          "trend": ""
        },
        {
          "label": "Auth success",
          "value": "99.8",
          "unit": "%",
          "trend": ""
        }
      ],
      "capabilities": [
        "Subscriber identity and entitlement",
        "Session lifecycle and IP address leasing",
        "QoS policies and service tiers",
        "Mobility anchoring across handover"
      ],
      "components": [
        "Identity registry",
        "Session controller",
        "IP address manager",
        "Policy engine"
      ],
      "assets": [
        {
          "id": "SESS-40421",
          "location": "Residential · IPv6",
          "status": "nominal",
          "metric": "320 Mbps"
        },
        {
          "id": "SESS-11108",
          "location": "Maritime · IPv6",
          "status": "nominal",
          "metric": "180 Mbps"
        },
        {
          "id": "SESS-80943",
          "location": "Business · Dual Stack",
          "status": "warning",
          "metric": "210 Mbps"
        },
        {
          "id": "SESS-00243",
          "location": "Residential · IPv6",
          "status": "nominal",
          "metric": "260 Mbps"
        }
      ],
      "actions": [
        {
          "label": "Suspend demo session",
          "detail": "Model service-policy suspension",
          "target": "SESS-80943"
        },
        {
          "label": "Renew IP lease",
          "detail": "Simulate address assignment",
          "target": "SESS-11108"
        },
        {
          "label": "Move session anchor",
          "detail": "Illustrate mobility-continuity event",
          "target": "SESS-40421"
        }
      ],
      "dependencies": [
        "Open5GS (NTN candidate)",
        "Rust"
      ],
      "events": [
        "Subscriber authorization granted",
        "Session anchor updated",
        "IP lease extended"
      ],
      "accent": "#9dc2fb"
    },
    {
      "id": "terrestrial-network",
      "name": "Terrestrial Backbone & Edge",
      "group": "ground",
      "summary": "Gateway backhaul, PoPs, IP routing, IPv6, NAT, and external internet connectivity.",
      "owner": "Ground IP transport and egress",
      "model": "design",
      "metrics": [
        {
          "label": "Backbone PoPs",
          "value": "68",
          "unit": "",
          "trend": ""
        },
        {
          "label": "Backhaul capacity",
          "value": "13.6",
          "unit": "Tbps",
          "trend": ""
        },
        {
          "label": "Egress RTT",
          "value": "12.8",
          "unit": "ms",
          "trend": ""
        }
      ],
      "capabilities": [
        "Ground station backhaul paths",
        "PoP and internet egress routing",
        "IPv6 addressing and optional CGNAT",
        "IP routing resilience and congestion"
      ],
      "components": [
        "PoP router",
        "Backhaul topology",
        "BGP/FIB policy",
        "Internet traffic sink"
      ],
      "assets": [
        {
          "id": "POP-US-EAST",
          "location": "Ashburn · US",
          "status": "nominal",
          "metric": "83% links"
        },
        {
          "id": "POP-EU-WEST",
          "location": "Frankfurt · DE",
          "status": "nominal",
          "metric": "71% links"
        },
        {
          "id": "POP-AP-SOUTH",
          "location": "Singapore · SG",
          "status": "warning",
          "metric": "91% links"
        },
        {
          "id": "POP-US-WEST",
          "location": "Los Angeles · US",
          "status": "nominal",
          "metric": "75% links"
        }
      ],
      "actions": [
        {
          "label": "Recompute egress path",
          "detail": "Simulate a PoP route revision",
          "target": "POP-AP-SOUTH"
        },
        {
          "label": "Drain transit link",
          "detail": "Model planned routing maintenance",
          "target": "POP-EU-WEST"
        },
        {
          "label": "Test IPv6 reachability",
          "detail": "Run a mock internet-path probe",
          "target": "POP-US-EAST"
        }
      ],
      "dependencies": [
        "FRRouting (candidate)",
        "Linux netns/tc (candidate)"
      ],
      "events": [
        "BGP edge update applied",
        "Backhaul capacity threshold",
        "PoP path failover"
      ],
      "accent": "#7fa9fc"
    },
    {
      "id": "network-control",
      "name": "Network Control & Routing",
      "group": "control",
      "summary": "Time-varying contact graph, route installation, gateway selection, and traffic engineering.",
      "owner": "Network route and gateway policy",
      "model": "partial",
      "metrics": [
        {
          "label": "Route revisions",
          "value": "2,418",
          "unit": "",
          "trend": ""
        },
        {
          "label": "Reconvergence",
          "value": "21",
          "unit": "ms",
          "trend": ""
        },
        {
          "label": "Reachable nodes",
          "value": "99.3",
          "unit": "%",
          "trend": ""
        }
      ],
      "capabilities": [
        "Contact graph and topology revisions",
        "Shortest and time-dependent route planning",
        "Gateway selection and capacity-aware TE",
        "Route installation and fault reconvergence"
      ],
      "components": [
        "Topology aggregator",
        "Route planner",
        "Rule distributor",
        "Gateway selector"
      ],
      "assets": [
        {
          "id": "ROUTE-1209",
          "location": "UE → SAT-A → SAT-B → GS",
          "status": "nominal",
          "metric": "28.4 ms"
        },
        {
          "id": "ROUTE-2310",
          "location": "EU → SAT-D → GS-EU",
          "status": "nominal",
          "metric": "31.7 ms"
        },
        {
          "id": "ROUTE-4041",
          "location": "APAC → SAT-C → SAT-F → GS",
          "status": "warning",
          "metric": "42.1 ms"
        },
        {
          "id": "ROUTE-8012",
          "location": "US → SAT-Z → GS-US",
          "status": "nominal",
          "metric": "25.0 ms"
        }
      ],
      "actions": [
        {
          "label": "Recompute path",
          "detail": "Simulate a new FIB revision",
          "target": "ROUTE-4041"
        },
        {
          "label": "Inject topology fault",
          "detail": "Trigger illustrative reconvergence",
          "target": "ROUTE-1209"
        },
        {
          "label": "Select alternate gateway",
          "detail": "Model constrained gateway selection",
          "target": "ROUTE-2310"
        }
      ],
      "dependencies": [
        "Rust Dijkstra",
        "FRRouting (candidate)",
        "OpenSN (candidate)"
      ],
      "events": [
        "Topology graph revision +1",
        "Route installation acknowledged",
        "Gateway preference changed"
      ],
      "accent": "#6cd9d0"
    },
    {
      "id": "operations",
      "name": "Network & Mission Operations",
      "group": "operations",
      "summary": "Operations center for health monitoring, incident workflows, and spacecraft/network commands.",
      "owner": "Alarm, incident, operator command",
      "model": "design",
      "metrics": [
        {
          "label": "Active alarms",
          "value": "7",
          "unit": "",
          "trend": ""
        },
        {
          "label": "Open incidents",
          "value": "2",
          "unit": "",
          "trend": ""
        },
        {
          "label": "MTTR",
          "value": "4.2",
          "unit": "min",
          "trend": ""
        }
      ],
      "capabilities": [
        "Fleet and ground site telemetry",
        "Incident and alarm lifecycle",
        "Operator command approvals",
        "Change windows and maintenance"
      ],
      "components": [
        "NOC event collector",
        "Incident manager",
        "Command approver",
        "Operations audit"
      ],
      "assets": [
        {
          "id": "INC-2408",
          "location": "Gateway site warning",
          "status": "warning",
          "metric": "P2"
        },
        {
          "id": "ALM-0921",
          "location": "Beam saturation",
          "status": "warning",
          "metric": "P3"
        },
        {
          "id": "INC-2410",
          "location": "ISL recovery",
          "status": "nominal",
          "metric": "P3"
        },
        {
          "id": "ALM-0920",
          "location": "Terminal reacquisition",
          "status": "nominal",
          "metric": "P4"
        }
      ],
      "actions": [
        {
          "label": "Acknowledge incident",
          "detail": "Change mock incident workflow state",
          "target": "INC-2408"
        },
        {
          "label": "Schedule maintenance",
          "detail": "Add illustrative maintenance action",
          "target": "ALM-0921"
        },
        {
          "label": "Review fleet health",
          "detail": "Record a monitoring audit event",
          "target": "INC-2410"
        }
      ],
      "dependencies": [
        "OpenTelemetry (candidate)",
        "Prometheus (candidate)",
        "Yamcs (candidate)"
      ],
      "events": [
        "New gateway warning reported",
        "Incident impact estimated",
        "Operator action acknowledged"
      ],
      "accent": "#eab797"
    },
    {
      "id": "security",
      "name": "Security & Trust",
      "group": "operations",
      "summary": "Identity, access policy, control-plane trust, auditing, and modeled threat scenarios.",
      "owner": "Trust and authorization rules",
      "model": "design",
      "metrics": [
        {
          "label": "Policies evaluated",
          "value": "18.6",
          "unit": "K/min",
          "trend": ""
        },
        {
          "label": "Policy denials",
          "value": "28",
          "unit": "",
          "trend": ""
        },
        {
          "label": "Audit coverage",
          "value": "100",
          "unit": "%",
          "trend": ""
        }
      ],
      "capabilities": [
        "Service and device identity",
        "Management-plane RBAC/policies",
        "Credential rotation and expiration",
        "Threat scenarios and audit trail"
      ],
      "components": [
        "Identity provider",
        "Authorization engine",
        "Credential store",
        "Audit ledger"
      ],
      "assets": [
        {
          "id": "POL-SPACE-CMD",
          "location": "Spacecraft control",
          "status": "nominal",
          "metric": "Strict"
        },
        {
          "id": "POL-CORE-SESSION",
          "location": "Subscriber session API",
          "status": "nominal",
          "metric": "Strict"
        },
        {
          "id": "POL-GW-ADMIN",
          "location": "Gateway operations",
          "status": "warning",
          "metric": "Review"
        },
        {
          "id": "POL-ROUTE-INSTALL",
          "location": "Network controller",
          "status": "nominal",
          "metric": "Strict"
        }
      ],
      "actions": [
        {
          "label": "Simulate denied command",
          "detail": "Record an illustrative authorization failure",
          "target": "POL-GW-ADMIN"
        },
        {
          "label": "Rotate demo credential",
          "detail": "Simulate a credential lifecycle event",
          "target": "POL-CORE-SESSION"
        },
        {
          "label": "Review audit trail",
          "detail": "Inspect mock security events",
          "target": "POL-SPACE-CMD"
        }
      ],
      "dependencies": [
        "Rust TLS (candidate)",
        "OpenTelemetry (candidate)"
      ],
      "events": [
        "Authorization policy evaluated",
        "Credential rotation scheduled",
        "Management command denied"
      ],
      "accent": "#d8b3ee"
    },
    {
      "id": "environment",
      "name": "Spectrum & Environment",
      "group": "operations",
      "summary": "Spectrum assignments, weather attenuation, interference, and regional eligibility.",
      "owner": "Environmental and regulatory constraints",
      "model": "design",
      "metrics": [
        {
          "label": "Weather cells",
          "value": "1,284",
          "unit": "",
          "trend": ""
        },
        {
          "label": "Rain fade alerts",
          "value": "36",
          "unit": "",
          "trend": ""
        },
        {
          "label": "Restricted regions",
          "value": "19",
          "unit": "",
          "trend": ""
        }
      ],
      "capabilities": [
        "Spectrum assignment and reuse rules",
        "Rain fade and atmospheric attenuation",
        "Interference and beam overlap",
        "Regional service eligibility"
      ],
      "components": [
        "Spectrum registry",
        "Weather sampler",
        "Interference model",
        "Region policy engine"
      ],
      "assets": [
        {
          "id": "WX-APAC-042",
          "location": "Singapore maritime",
          "status": "warning",
          "metric": "8.1 dB fade"
        },
        {
          "id": "WX-EU-118",
          "location": "Western Europe",
          "status": "nominal",
          "metric": "1.2 dB fade"
        },
        {
          "id": "RF-NA-901",
          "location": "East US coverage",
          "status": "nominal",
          "metric": "Low interference"
        },
        {
          "id": "REG-CA-02",
          "location": "Canadian coverage",
          "status": "nominal",
          "metric": "Allowed"
        }
      ],
      "actions": [
        {
          "label": "Apply rain-fade event",
          "detail": "Change illustrative attenuation level",
          "target": "WX-APAC-042"
        },
        {
          "label": "Check spectrum conflict",
          "detail": "Simulate co-channel eligibility check",
          "target": "RF-NA-901"
        },
        {
          "label": "Update region policy",
          "detail": "Record a mock policy revision",
          "target": "REG-CA-02"
        }
      ],
      "dependencies": [
        "ITU-R models (candidate)",
        "Weather inputs (candidate)"
      ],
      "events": [
        "Rain fade event detected",
        "Interference map refreshed",
        "Service-region rule updated"
      ],
      "accent": "#b7cf94"
    },
    {
      "id": "service-assurance",
      "name": "Service Assurance",
      "group": "operations",
      "summary": "Customer service experience, outage detection, probes, and SLA reporting.",
      "owner": "User-level service quality metrics",
      "model": "design",
      "metrics": [
        {
          "label": "Availability",
          "value": "99.21",
          "unit": "%",
          "trend": ""
        },
        {
          "label": "P95 latency",
          "value": "53.2",
          "unit": "ms",
          "trend": ""
        },
        {
          "label": "Customer loss",
          "value": "0.18",
          "unit": "%",
          "trend": ""
        }
      ],
      "capabilities": [
        "RTT, loss, jitter and goodput probes",
        "Availability and outage classification",
        "QoS / SLA reporting",
        "Cross-domain incident correlation"
      ],
      "components": [
        "Probe scheduler",
        "Quality metrics",
        "Outage correlator",
        "Report builder"
      ],
      "assets": [
        {
          "id": "SLA-RES-NA",
          "location": "North America residential",
          "status": "nominal",
          "metric": "99.3%"
        },
        {
          "id": "SLA-BIZ-EU",
          "location": "Europe business",
          "status": "nominal",
          "metric": "99.9%"
        },
        {
          "id": "SLA-MAR-APAC",
          "location": "Asia maritime",
          "status": "warning",
          "metric": "97.8%"
        },
        {
          "id": "SLA-AVI-US",
          "location": "US aviation",
          "status": "nominal",
          "metric": "99.5%"
        }
      ],
      "actions": [
        {
          "label": "Run experience probes",
          "detail": "Generate mock latency/packet-loss samples",
          "target": "SLA-RES-NA"
        },
        {
          "label": "Open SLA comparison",
          "detail": "Simulate baseline comparison report",
          "target": "SLA-MAR-APAC"
        },
        {
          "label": "Classify outage",
          "detail": "Record a mock service event",
          "target": "SLA-BIZ-EU"
        }
      ],
      "dependencies": [
        "OpenTelemetry (candidate)",
        "Prometheus (candidate)"
      ],
      "events": [
        "Probe summary received",
        "Outage classification revised",
        "Availability report prepared"
      ],
      "accent": "#8fddd3"
    },
    {
      "id": "simulation",
      "name": "Simulation Runtime",
      "group": "platform",
      "summary": "Deterministic virtual clock, event scheduling, packet traces, fault injection, and experiment replay.",
      "owner": "Virtual time and experiment execution",
      "model": "partial",
      "metrics": [
        {
          "label": "Scenario runs",
          "value": "128",
          "unit": "",
          "trend": ""
        },
        {
          "label": "Events processed",
          "value": "18.4",
          "unit": "M",
          "trend": ""
        },
        {
          "label": "Replay agreement",
          "value": "100",
          "unit": "%",
          "trend": ""
        }
      ],
      "capabilities": [
        "Deterministic event queue / virtual time",
        "Modeled packets, finite queues, and route events",
        "Fault injection and scenario playback",
        "Run manifests and reproducibility"
      ],
      "components": [
        "Event scheduler",
        "Packet runtime",
        "Scenario loader",
        "Trace exporter"
      ],
      "assets": [
        {
          "id": "RUN-0218",
          "location": "Nominal LEO path",
          "status": "nominal",
          "metric": "1.2M events"
        },
        {
          "id": "RUN-0217",
          "location": "ISL failover",
          "status": "nominal",
          "metric": "890K events"
        },
        {
          "id": "RUN-0215",
          "location": "Gateway failure",
          "status": "warning",
          "metric": "430K events"
        },
        {
          "id": "RUN-0214",
          "location": "Orbital pass",
          "status": "nominal",
          "metric": "240 packets"
        }
      ],
      "actions": [
        {
          "label": "Start demo simulation",
          "detail": "Append a synthetic run to activity",
          "target": "RUN-0218"
        },
        {
          "label": "Inject link fault",
          "detail": "Model an experimental fault",
          "target": "RUN-0217"
        },
        {
          "label": "Reset virtual clock",
          "detail": "Rewind the mock experiment timeline",
          "target": "RUN-0214"
        }
      ],
      "dependencies": [
        "Rust",
        "serde_json",
        "Skyfield / SGP4"
      ],
      "events": [
        "Simulation run completed",
        "Fault event dispatched",
        "Run output archived"
      ],
      "accent": "#8ed1fd"
    },
    {
      "id": "emulation",
      "name": "Network Emulation",
      "group": "platform",
      "summary": "Optional real-IP namespaces, qdisc link impairment, protocol adapters, and hardware bridges.",
      "owner": "Linux workers and external adapters",
      "model": "design",
      "metrics": [
        {
          "label": "Prepared labs",
          "value": "8",
          "unit": "",
          "trend": ""
        },
        {
          "label": "Netns workers",
          "value": "0",
          "unit": "",
          "trend": ""
        },
        {
          "label": "Adapter profiles",
          "value": "6",
          "unit": "",
          "trend": ""
        }
      ],
      "capabilities": [
        "Linux netns/veth/tc impairment",
        "Packet capture and FRR routing",
        "ns-3/SNS3/OpenSAND adapters",
        "SDR/hardware-in-the-loop boundaries"
      ],
      "components": [
        "Emulation manager",
        "Link impairment bridge",
        "Protocol adapters",
        "Capture exporter"
      ],
      "assets": [
        {
          "id": "LAB-IP-001",
          "location": "Linux veth demo",
          "status": "nominal",
          "metric": "Mock"
        },
        {
          "id": "LAB-NS3-002",
          "location": "ns-3 comparison",
          "status": "warning",
          "metric": "Planned"
        },
        {
          "id": "LAB-SDR-003",
          "location": "RF bench",
          "status": "warning",
          "metric": "Planned"
        },
        {
          "id": "LAB-FRR-004",
          "location": "Routing study",
          "status": "nominal",
          "metric": "Mock"
        }
      ],
      "actions": [
        {
          "label": "Provision mock lab",
          "detail": "Illustrate isolated test topology",
          "target": "LAB-IP-001"
        },
        {
          "label": "Apply packet delay",
          "detail": "Simulate qdisc impairment",
          "target": "LAB-FRR-004"
        },
        {
          "label": "Review adapter contract",
          "detail": "Inspect mock adapter profile",
          "target": "LAB-NS3-002"
        }
      ],
      "dependencies": [
        "Linux netns/tc (candidate)",
        "ns-3 (candidate)",
        "GNU Radio (candidate)"
      ],
      "events": [
        "Emulation adapter prepared",
        "Impairment schedule updated",
        "Packet capture indexed"
      ],
      "accent": "#b0bbd9"
    },
    {
      "id": "observability",
      "name": "Data & Visualization",
      "group": "platform",
      "summary": "Structured experiment evidence, trace archives, telemetry rollups, and 3D constellation views.",
      "owner": "Run artifacts and visualization projection",
      "model": "partial",
      "metrics": [
        {
          "label": "Indexed events",
          "value": "24.6",
          "unit": "M",
          "trend": ""
        },
        {
          "label": "Stored experiments",
          "value": "128",
          "unit": "",
          "trend": ""
        },
        {
          "label": "Trace completeness",
          "value": "99.9",
          "unit": "%",
          "trend": ""
        }
      ],
      "capabilities": [
        "Scenario and event schema registry",
        "Packet and domain trace archive",
        "Telemetry aggregation and comparison",
        "Geospatial constellation visualization"
      ],
      "components": [
        "Event/trace store",
        "Experiment report index",
        "Visualization API",
        "Cesium presentation layer"
      ],
      "assets": [
        {
          "id": "ART-0218",
          "location": "Nominal network trace",
          "status": "nominal",
          "metric": "2.8 MB"
        },
        {
          "id": "ART-0217",
          "location": "ISL failure replay",
          "status": "nominal",
          "metric": "1.3 MB"
        },
        {
          "id": "ART-0215",
          "location": "Gateway event stream",
          "status": "warning",
          "metric": "0.9 MB"
        },
        {
          "id": "ART-0214",
          "location": "Historical pass report",
          "status": "nominal",
          "metric": "1.6 MB"
        }
      ],
      "actions": [
        {
          "label": "Replay demo trace",
          "detail": "Inspect illustrative packet journey",
          "target": "ART-0217"
        },
        {
          "label": "Export demo snapshot",
          "detail": "Download current generated JSON",
          "target": "ART-0218"
        },
        {
          "label": "Compare two runs",
          "detail": "Simulate experiment diff",
          "target": "ART-0215"
        }
      ],
      "dependencies": [
        "TypeScript",
        "CesiumJS (candidate)",
        "OpenTelemetry (candidate)"
      ],
      "events": [
        "Experiment artifact indexed",
        "Metrics rollup complete",
        "Replay projection refreshed"
      ],
      "accent": "#a5d5ee"
    }
  ],
  "scenarios": [
    {
      "id": "nominal",
      "name": "Nominal orbit",
      "detail": "Reference connectivity across all system segments.",
      "health": 98.6,
      "links": 9604,
      "stations": 186,
      "latency": 28.6,
      "loss": 0.18,
      "demand": 4.82,
      "tone": "good"
    },
    {
      "id": "isl",
      "name": "ISL failure",
      "detail": "Laser crosslink degradation triggers route convergence.",
      "health": 91.8,
      "links": 9603,
      "stations": 186,
      "latency": 44.7,
      "loss": 1.26,
      "demand": 4.51,
      "tone": "warning"
    },
    {
      "id": "gateway",
      "name": "Gateway outage",
      "detail": "One feeder gateway is withdrawn and traffic shifts.",
      "health": 87.9,
      "links": 9597,
      "stations": 185,
      "latency": 61.2,
      "loss": 2.81,
      "demand": 4.09,
      "tone": "danger"
    },
    {
      "id": "surge",
      "name": "Traffic surge",
      "detail": "Increased demand raises scheduling and queue delays.",
      "health": 93.4,
      "links": 9604,
      "stations": 186,
      "latency": 52.8,
      "loss": 1.03,
      "demand": 5.37,
      "tone": "warning"
    }
  ],
  "networkPath": [
    {
      "label": "User Terminal",
      "id": "terminal"
    },
    {
      "label": "Access Beam",
      "id": "access-network"
    },
    {
      "label": "Satellite A",
      "id": "spacecraft"
    },
    {
      "label": "Optical ISL",
      "id": "space-network"
    },
    {
      "label": "Satellite B",
      "id": "spacecraft"
    },
    {
      "label": "Feeder Station",
      "id": "ground-segment"
    },
    {
      "label": "Terrestrial PoP",
      "id": "terrestrial-network"
    },
    {
      "label": "Internet",
      "id": "terrestrial-network"
    }
  ]
};
