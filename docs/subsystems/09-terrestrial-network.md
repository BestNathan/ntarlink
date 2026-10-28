# SYS-09 — Terrestrial Backhaul, PoPs, IP Routing and Internet Edge

**Mission:** model what happens **after feeder radio termination**: ground transport, point of presence, core-facing traffic anchors and internet connectivity. A feeder station and a public-facing PoP are separate entities, even if physically colocated.

## Component design

| Component | Detailed scope |
|---|---|
| Backhaul topology | directed fiber/microwave/leased-line links, geographical lengths, capacity, failure domains and costs |
| PoP inventory | regional PoPs, router interfaces, service anchors, ingress/egress capacities and operational policies |
| Packet routing | IPv4/IPv6 FIB, prefixes, BGP/IGP convergence model, default route, routing policy and asymmetric return paths |
| Internet edge | peer/transit/sink selection, synthetic RTT endpoints and congestion from external traffic |
| Address translation | optional IPv4 CGNAT, state-table caps, mapping lifecycle/timeouts, errors; IPv6 routing without NAT by default |
| DNS/control services | resolvers and cached queries as modeled service dependencies, not magical global reachability |
| Backhaul scheduler | serialization/propagation/queue delay, limited bandwidth, burst/drop and traffic classes |
| Real-IP adapter | netns/veth, Linux routing, FRRouting, packet capture and end-host applications |

## Engineering implementation

Represent ground site `GroundSiteId` and network `PopId` separately with a `BackhaulLinkId` and explicit directed edge. Compute terrestrial propagation from length and effective velocity (e.g. fiber refractive propagation with configurable speed, **not vacuum c**); add serialization, processing, congestion and queuing. A colocated ground station still traverses an explicitly modeled zero-distance or local-backhaul resource, not an implicit network shortcut.

A forwarding stage performs route lookup and decrements hop budget; egress policy and service session must authorize the flow. Simulate default IPv6 path as an address/prefix allocation from SYS-08 plus PoP routing; optional IPv4 CGNAT requires capacity-limited mapping tuples and return state. Traffic destination may be an idealized remote server, a modeled WAN transit graph, or a real IP endpoint in emulation mode. Mark which model is used.

Routing control in SYS-07 may choose PoP/path intentions; SYS-09 owns actual terrestrial link/router state. If using FRRouting in emulation, observe BGP/OSPF/BFD control reconvergence and do not additionally apply an instant synthetic FIB update. Treat paths for uplink and downlink independently, with session anchoring and path symmetry as explicit policies.

## Interfaces

Own `BackhaulLink`, `Pop`, `EdgeRouter`, `InternetEndpoint`, `NATBinding`, `DnsRecord` and transport queue. Commands: `ConfigurePopRoute`, `SetBackhaulState`, `SetEgressPolicy`, `InjectInternetTraffic`. Events: `BackhaulCongested`, `BackhaulLost`, `PopDegraded`, `EgressRouteChanged`, `NATResourceExhausted`, `InternetPacketDelivered`. Inputs: SYS-06 feeder handoff, SYS-07 policy/route plan, SYS-08 service session/address and SYS-11 security policy.

Packet paths must record **Ground Station → terrestrial hop(s) → PoP → destination**, allowing independent failure diagnosis and delay decomposition.

## Dependencies and runtime

P1 Rust typed link graph and packet forwarder for deterministic runs. P3 Linux netns/veth/tc/netem and [FRRouting](https://frrouting.org/) as a dedicated privileged/emulation adapter to test real BGP/OSPF/IPv6 behavior. DNS or web endpoints can be small synthetic application workers. Do not simulate enormous global internet BGP tables unless the scenario explicitly requires them.

## Failures, measurement and acceptance

Backhaul cut, congestion, PoP outage, routing loop/blackhole, IP address conflict, NAT table exhaustion, DNS timeout, internet sink failure and one-way reachability. Each has a distinct drop/event reason; SYS-07 must be informed for gateway/PoP reselection.

**Tests:** feeder remains UP while backhaul outage causes user service failure; alternate PoP selected after measured delay; IPv6 packet egress and reverse path; IPv4 NAT state life cycle and timeout; route change does not teleport packets; queue occupancy and loss agree with finite capacity. Metrics include PoP load, goodput, backhaul latency, packet drops, NAT bindings, egress route convergence and end-to-end RTT.

**Build order:** terrestrial link and PoP graph → synthetic IP forwarding → IPv6/IPv4 edge policy → failure/congestion → optional FRR and real IP.