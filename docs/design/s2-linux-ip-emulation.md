# S2 Linux real-IP path experiment (Issue #4)

**Status:** experimental, opt-in Linux proof of concept. This is **not** satellite RF, a faithful Starlink protocol stack, an S1 route emulator, or an always-running service. Its topology uses real host-kernel IP forwarding and wall-clock Linux traffic-control impairments.

## Separation of execution authorities

- **S1** owns virtual time, analytical geometry/link budgets, packet queues, faults and deterministic event replay. This worker **never** advances or modifies S1 state.
- **S2** selects the lexical shortest directed path through the scenario-v1 link inventory. This is a fixed-route benchmark, **not** the SYS-07 S1 routing decision.
- S1 propagation and processing delay map to per-egress Linux netem delay. S1 capacity maps to netem rate. Both direction qdiscs are configured; this approximates the wall-clock data plane, not S1 finite queues, head acquisition, RF fading or routing convergence.
- Each S1 fault time maps to a wall-clock target using the requested time scale. Command lateness is measured, not silently relabeled as virtual simulation time.
- Every completed run records input SHA-256, selected path, execution clock, host kernel, commands/exit codes/elapsed durations, initial/final routes and qdiscs, ping evidence, optional iperf3 throughput and optional real packet capture.

## Safety and reproducibility

Use only on an isolated, disposable Linux test host. Real execution requires Linux, iproute2 (ip and tc), iputils-ping and root/CAP_NET_ADMIN; iperf3 and tcpdump are opt-in. Namespace and veth names derive from a restricted 1-4-character tag. If a requested namespace already exists, the worker aborts without adopting it. Never expose this privileged interface to untrusted scenarios.

The S2-v1 adapter accepts one directed path of at most eight links, initially up, at most 32 distinct fault transitions, at most 30 seconds S1 duration, and a maximum 120-second scaled wall-clock horizon. No shell interpolation from scenario JSON occurs. The namespace-local IPv4 address pool is 10.248.1.0/30 through 10.248.8.0/30; it is not a general addressing solution. Use a unique tag for concurrent invocations. Execution cleans up only namespaces created by this invocation on normal exit or command failure. Uncatchable termination or a host crash may leave resources behind; manual recovery must check ownership before deletion.

Dry-run, no root required:

    python3 systems/network-emulation/s2_worker.py scenarios/s2-linux-ip-path.json --source ue-a --target gw-a

Explicitly privileged real multihop experiment:

    sudo python3 systems/network-emulation/s2_worker.py scenarios/s2-linux-ip-path.json --source ue-a --target gw-a --tag s2 --apply --time-scale 20 --iperf --pcap /tmp/ntarlink-s2.pcap --output /tmp/ntarlink-s2-report.json

Acceptance: the initial ping succeeds, down-fault ping fails, recovery ping succeeds, iperf3 reports received bytes, and a nonempty pcap is collected. The worker must produce route/qdisc snapshots and zero cleanup errors. **These are acceptance targets, not measured claims until the exact-head hosted privileged integration job passes.**

## Boundaries and next gates

1. Static selected paths are implemented, but actual FRRouting/BGP/OSPF convergence and alternate path failover are still open.
2. Linux netem is a wall-clock approximation; finite shared queues, PHY effects and convergence are not modeled.
3. The host packages require version/license pinning and a reproducible image for release use. Ubuntu-24.04 hosted runners and apt package versions are not immutable; #5 owns this gate.
4. The Pages console remains synthetic and must not be called a live S2 telemetry dashboard.
5. The P2 acceptance must eventually compare real PCAP/delay/throughput to independently measured time and resources, record skew and routing changes, and include failures/retries.

**CI:** the dedicated S2 workflow tests default dry-run without privilege and runs a privileged Linux netns/ping/iperf3/tcpdump smoke experiment. The S1 Rust CI remains the deterministic packet authority.


## Centralized backup-ISL routing experiment

A second opt-in command uses the same namespace/qdisc primitives, but exports
the **whole bounded directed scenario graph** instead of a single path. It
selects a stable lexical shortest path, keeps a backup ISL physically present,
and after a link failure recomputes the route using only active links. The
controller then updates actual Linux /32 IP routes in the affected namespaces,
measures kernel route-configuration wall time and probes real IP connectivity.
Recovery of the original ISL switches traffic back to the shorter primary path.

    python3 systems/network-emulation/s2_routing.py scenarios/s2-route-failover.json --source ue-a --target internet
    sudo python3 systems/network-emulation/s2_routing.py scenarios/s2-route-failover.json --source ue-a --target internet --tag r2 --apply --time-scale 20 --iperf --pcap /tmp/s2-routing.pcap --output /tmp/s2-routing-report.json

The committed backup-path fixture represents terminal → satellite A → satellite
B → gateway → PoP → internet, with a secondary satellite A → satellite C →
satellite B path. The fixture supplies ISL Down and Up. The acceptance checks
that *live Linux* ping succeeds before the fault, over the secondary path, and
after returning to the direct path; it requires route snapshots, controller
apply timing, iperf3 and a nonempty PCAP, and zero leftover namespaces.

**Critical fidelity distinction:** this is a centralized, deterministic
IP route-programming baseline, *not* FRRouting/BGP/OSPF, a measured routing
protocol convergence result, an S1 route replay or a distributed path optimizer.
The controller's measured route-apply duration and post-update ping do not
include protocol discovery, neighbor state sync, or route advertisement.
Failure during programming may temporarily disrupt packets; future tests must
measure that blackout independently. FRR (or a chosen comparable protocol
stack), measured convergence under recurring flaps, capacity contention and
a production worker/image contract remain in Issue #4.

## Sustained real-IP failover sampling

The route-controller experiment now starts a Linux `ping -D -O -i 0.2` process in the source namespace before fault replay and stops it after the final kernel snapshot. The measured report contains received replies, missing ICMP sequences and gaps between successful replies in host wall-clock nanoseconds. These are sampled observations, **not** exact blackout start/end times or distributed routing convergence. Host scheduling, ICMP sampling, subprocess startup and kernel timestamp precision limit inference. The existing controller-apply durations remain separately labeled. The GitHub-hosted S2 acceptance requires at least one sustained reply; outage gaps are recorded rather than artificially required because short faults can fall between probes.
