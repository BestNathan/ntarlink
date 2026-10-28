# SYS-15 — Linux Packet Emulation, Protocol Simulators and Hardware-in-the-Loop

**Mission:** connect modeled links and network state to actual IP packets, external telecom simulators and optional SDR/hardware experiments. Distinguish **emulation** from accelerated discrete-event simulation and waveform-level RF experiments.

## Architecture: multiple backends, one declared capability contract

| Backend | Execution mechanics | Fidelity / important limits |
|---|---|---|
| Linux network namespaces | one/few netns per modeled node, veth/TAP, tc netem, qdisc shaping and routes | real Linux IP traffic; link impairment approximation |
| FRRouting | BGP/OSPF/IS-IS/BFD per selected virtual routers | genuine protocol convergence; not satellite MAC or native contact-graph routing |
| ns-3/SNS3 | externally built C++ scenarios with pinned compatible revisions | network/protocol study, standard-specific PHY/MAC assumptions |
| OpenSAND | satellite modem/channel emulation and real packet interfaces | DVB-S2/RCS2 profiles, not Starlink-proprietary protocol |
| OpenAirInterface + Open5GS | separate 3GPP NTN experiment with RAN/core | explicit NTN profile; licensing and infrastructure obligations |
| GNU Radio / SDR | IQ samples, GNU Radio flow graphs and supported RF hardware | real-time hardware, legal spectrum controls; not global network scale |

## Adapter interface and coupling model

Adapters implement a versioned `EmulationBackend` contract: `DescribeCapabilities`, `PrepareTopology`, `Start`, `ApplyLinkState`, `InjectTraffic`, `CollectCapture`, `Stop`, `Cleanup`. Capability record includes timing ownership, supported link types, accepted units, packet encapsulation, kernel/privilege requirements, external binary digest, license/source revision and observability.

**Mode A — offline comparison:** generate scenario/contact/traffic inputs, run external engine independently, import packet or performance traces. The two engines are not coupled in real time; this is safest for ns-3/SNS3 first integration.

**Mode B — realtime impairment:** Rust controller emits real-wall-clock link schedule; privileged Linux worker updates netns/veth routes, qdisc rates/latencies/loss, then traffic generators send real IP packets. Measure qdisc update granularity, scheduling skew, command-to-kernel latency and capture timestamps.

**Mode C — synchronized co-simulation:** external engine advances only at negotiated conservative boundaries; designate a single simulation time authority, manage lookahead and causality, and prevent applying delay/loss twice. Use only after numerical validation.

## Linux implementation details

Create a per-run namespace prefix, idempotent resource journal, explicit bridge/veth naming and sysctl/IP addresses. Keep netns creation under minimal CAP_NET_ADMIN in a separate worker, never in web application. Configure `tc netem` for variable delay/loss and `tbf`/`htb` (where appropriate) for shaping. Because qdisc behavior and kernel timers differ from a virtual clock, calibrate achieved rate, jitter and loss against trace captures before comparing with SYS-14 output.

Provision traffic endpoints such as iperf3, ping and synthetic TCP/UDP flows. Instrument `tcpdump`/PCAP, interface counters, qdisc statistics and FRR routing/FIB state. Clean up all namespaces, virtual interfaces, processes and qdiscs on completion, cancellation and crash/restart. Do not run arbitrary shell commands from uploaded scenario files.

## Data and interfaces

Own `AdapterInstance`, `NamespaceId`, `VirtualInterface`, `ImpairmentSchedule`, `CaptureArtifact`, `TimeBridge`. Inputs: SYS-07 route/graph revision, SYS-09 IP topology, SYS-05/06/03 link capacity and delay snapshots, SYS-14 run actions. Events: `AdapterStarted`, `KernelLinkUpdated`, `PacketCaptured`, `TimeSkewExceeded`, `AdapterFailed`, `CleanupCompleted`.

## OSS and licensing constraints

ns-3 GPL-2.0, SNS3 GPL-3.0, OpenSAND GPL/LGPL, FRR generally GPL-2.0+, Open5GS AGPL-3.0, OpenAirInterface RAN CSSL v1.0 (current): each requires pin/version/license and distribution review. Process isolation does not automatically eliminate license obligations. Native SDR transmission may be subject to legal restrictions; initial SDR/HIL experiments should use conducted/shielded setups and qualified users.

## Acceptance and failure cases

Worker lacks privileges, kernel/netem feature mismatch, FRR daemon exit, namespace name collisions, stale interface, packet loss calibration mismatch, external engine time skew or unsupported link-rate update. No silent substitution of analytical output for unavailable emulation.

**Tests:** ping RTT approximately matches configured round-trip impairment within measured tolerance; iperf goodput reflects bottleneck; route failure causes a real packet outage and FRR convergence delay; wall-clock update skew bounded; two concurrent isolated runs do not share namespaces; forced crash cleanup leaves no resources. Capture reproducible environment digest and PCAP hash.

**Build order:** simple netns pair → mutable tc impairment → multi-hop routed topology → FRR → offline ns-3/SNS3 comparison → optional HIL bridge.