"""S2 Linux IP dynamic *controller-driven* route failover experiment.

This explicitly tests observed Linux route replacement and IP forwarding;
it does not impersonate FRRouting, a distributed protocol, or the S1 scheduler.
"""
from __future__ import annotations

import argparse
from decimal import Decimal
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import signal
import shutil
import subprocess
import sys
import time

from s2_worker import (
    LinuxPath, MAX_FAULTS, MAX_HOPS, PlanError, identifier, unsigned,
)


def find_path(plan: dict, disabled: set[str]) -> list[dict]:
    edges = plan["links"]
    adjacency: dict[str, list[dict]] = {node["id"]: [] for node in plan["nodes"]}
    for edge in edges:
        if edge["id"] not in disabled:
            adjacency[edge["from"]].append(edge)
    queue = [(plan["source"], [], {plan["source"]})]
    while queue:
        node, path, visited = queue.pop(0)
        if node == plan["target"]:
            return path
        if len(path) >= MAX_HOPS:
            continue
        for edge in sorted(adjacency[node], key=lambda item: item["id"]):
            if edge["to"] not in visited:
                queue.append((edge["to"], path + [edge], visited | {edge["to"]}))
    return []


def build_graph(raw: bytes, source: str, target: str, tag: str) -> dict:
    source, target = identifier(source), identifier(target)
    if not re.fullmatch(r"[a-z][a-z0-9]{0,3}", tag):
        raise PlanError("tag must be 1-4 lowercase alphanumeric characters")
    try:
        scenario = json.loads(raw)
    except (ValueError, UnicodeError) as exc:
        raise PlanError("scenario must be valid JSON") from exc
    if not isinstance(scenario, dict) or scenario.get("schema_version") != 1:
        raise PlanError("requires S1 scenario-v1")
    duration = unsigned(scenario.get("duration_ns"), "duration_ns", minimum=1, maximum=30_000_000_000)
    nodes = scenario.get("nodes")
    links = scenario.get("links")
    faults = scenario.get("faults", [])
    if not isinstance(nodes, list) or not isinstance(links, list) or not isinstance(faults, list):
        raise PlanError("nodes, links and faults must be arrays")
    if len(nodes) > MAX_HOPS + 1 or len(links) > MAX_HOPS or not links:
        raise PlanError("S2 graph exceeds node/link budget")
    names = [identifier(item.get("id")) for item in nodes if isinstance(item, dict)]
    if len(names) != len(nodes) or len(set(names)) != len(names):
        raise PlanError("invalid or duplicate node")
    if source == target or source not in names or target not in names:
        raise PlanError("invalid source/target")
    ordered_nodes = [source] + sorted(set(names) - {source, target}) + [target]
    ns_by_id = {name: f"nt{tag}n{i}" for i, name in enumerate(ordered_nodes)}
    all_ids = set()
    link_rows = []
    for raw_link in links:
        if not isinstance(raw_link, dict):
            raise PlanError("each link must be an object")
        link_id = identifier(raw_link.get("id"))
        left, right = identifier(raw_link.get("from")), identifier(raw_link.get("to"))
        if link_id in all_ids or left not in ns_by_id or right not in ns_by_id or left == right:
            raise PlanError("invalid or duplicate directed link")
        all_ids.add(link_id)
        if raw_link.get("up", True) is not True:
            raise PlanError("initial-down graph links are not supported by S2 v1")
        rate = unsigned(raw_link.get("capacity_bps"), "capacity_bps", minimum=10_000, maximum=10**10)
        delay = unsigned(raw_link.get("propagation_delay_ns"), "propagation_delay_ns", maximum=10**10)
        processing = unsigned(raw_link.get("processing_delay_ns", 0), "processing_delay_ns", maximum=10**10)
        link_rows.append({
            "id": link_id, "from": left, "to": right,
            "capacity_bps": rate, "delay_ns": delay + processing,
        })
    # Link indexes are fixed by ID, independent of JSON order.
    link_rows.sort(key=lambda edge: edge["id"])
    canonical_links = []
    for i, edge in enumerate(link_rows):
        canonical_links.append({
            **edge,
            "left_ns": ns_by_id[edge["from"]],
            "right_ns": ns_by_id[edge["to"]],
            "left_if": f"nt{tag}l{i}",
            "right_if": f"nt{tag}r{i}",
            "left_ip": f"10.248.{i + 1}.1",
            "right_ip": f"10.248.{i + 1}.2",
            "cidr_left": f"10.248.{i + 1}.1/30",
            "cidr_right": f"10.248.{i + 1}.2/30",
        })
    event_rows = []
    for fault in faults:
        if not isinstance(fault, dict):
            raise PlanError("fault must be an object")
        lid = identifier(fault.get("link_id"))
        if lid not in all_ids:
            raise PlanError("fault references unknown link")
        at_ns = unsigned(fault.get("at_ns"), "fault.at_ns", maximum=duration - 1)
        action = fault.get("action")
        if action not in ("up", "down"):
            raise PlanError("fault requires up/down")
        event_rows.append({"link_id": lid, "at_ns": at_ns, "action": action})
    event_rows.sort(key=lambda ev: (ev["at_ns"], ev["link_id"]))
    if len(event_rows) > MAX_FAULTS or len(set((e["at_ns"], e["link_id"]) for e in event_rows)) != len(event_rows):
        raise PlanError("too many or ambiguous fault transitions")
    plan = {
        "profile": "S2-Linux-IP-route-controller-v1",
        "source_fidelity": "S1-topology-to-S2-measured-IP-not-protocol-convergence",
        "source_sha256": hashlib.sha256(raw).hexdigest(),
        "source": source, "target": target,
        "nodes": [{"id": name, "namespace": ns_by_id[name]} for name in ordered_nodes],
        "links": canonical_links, "faults": event_rows, "duration_ns": duration,
    }
    initial = find_path(plan, set())
    if len(initial) < 2:
        raise PlanError("must have a valid multihop initial S2 route")
    plan["source_ip"] = initial[0]["left_ip"]
    plan["target_ip"] = initial[-1]["right_ip"]
    plan["initial_path"] = [edge["id"] for edge in initial]
    # S2 v1 binds each endpoint to a stable interface. The initial source and
    # sink link must stay part of every failover path, otherwise addresses can
    # change and measurement semantics require a future multi-homed contract.
    # Validate the entire fault history, not each Down event in isolation:
    # simultaneous or successive failures can select a different endpoint
    # attachment even when every individual failure is safe.
    disabled: set[str] = set()
    for event in event_rows:
        if event["action"] == "down":
            disabled.add(event["link_id"])
        else:
            disabled.discard(event["link_id"])
        candidate = find_path(plan, disabled)
        if candidate and (
            candidate[0]["id"] != initial[0]["id"]
            or candidate[-1]["id"] != initial[-1]["id"]
        ):
            raise PlanError("failover must preserve source and sink attachment")
    return plan


class LinuxRouteGraph(LinuxPath):
    def __init__(self, plan: dict):
        super().__init__(plan)
        self.disabled: set[str] = set()
        self.active_path: list[str] = []

    def start_capture(self, pcap: Path) -> None:
        if not shutil.which("tcpdump"):
            raise PlanError("--pcap requires tcpdump")
        # Graph links are ID-sorted, not necessarily source-attached.
        # Capture on the active path's source egress, not links[0].
        edge = next(link for link in self.plan["links"] if link["id"] == self.active_path[0])
        self.capture = subprocess.Popen([
            "ip", "netns", "exec", edge["left_ns"], "tcpdump",
            "-n", "-U", "-i", edge["left_if"], "-w", str(pcap), "ip",
        ], stdout=subprocess.DEVNULL, stderr=subprocess.PIPE, text=True)
        time.sleep(0.2)
        if self.capture.poll() is not None:
            _, err = self.capture.communicate()
            raise RuntimeError("S2 graph capture failed: " + err[-500:])

    def setup(self) -> None:
        if sys.platform != "linux" or os.geteuid() != 0:
            raise PlanError("Linux root/CAP_NET_ADMIN is required for --apply")
        for binary in ("ip", "tc", "ping"):
            if not shutil.which(binary):
                raise PlanError(f"missing required host tool: {binary}")
        current = {
            line.split()[0]
            for line in self.run(["ip", "netns", "list"]).stdout.splitlines()
            if line.split()
        }
        if current & {n["namespace"] for n in self.plan["nodes"]}:
            raise PlanError("namespace tag collision")
        for node in self.plan["nodes"]:
            ns = node["namespace"]
            self.run(["ip", "netns", "add", ns])
            self.namespaces.append(ns)
            self.run(["ip", "-n", ns, "link", "set", "lo", "up"])
            if node["id"] not in (self.plan["source"], self.plan["target"]):
                self.run(["ip", "netns", "exec", ns, "sysctl", "-q", "-w", "net.ipv4.ip_forward=1"])
        for link in self.plan["links"]:
            left, right = link["left_if"], link["right_if"]
            self.run(["ip", "link", "add", left, "type", "veth", "peer", "name", right])
            self.host_veth.append(left)
            self.run(["ip", "link", "set", left, "netns", link["left_ns"]])
            self.run(["ip", "link", "set", right, "netns", link["right_ns"]])
            self.host_veth.remove(left)
            for ns, iface, cidr in (
                (link["left_ns"], left, link["cidr_left"]),
                (link["right_ns"], right, link["cidr_right"]),
            ):
                self.run(["ip", "-n", ns, "addr", "add", cidr, "dev", iface])
                self.run(["ip", "-n", ns, "link", "set", iface, "up"])
                self.run([
                    "ip", "netns", "exec", ns, "tc", "qdisc", "replace",
                    "dev", iface, "root", "netem",
                    "delay", f'{max(1, link["delay_ns"] // 1000)}us',
                    "rate", f'{link["capacity_bps"]}bit',
                ])
        self.program_routes()

    def program_routes(self) -> dict:
        path = find_path(self.plan, self.disabled)
        selected = [link["id"] for link in path]
        # An unrelated link fault must not tear down healthy /32 routes.
        # Linux route deletion/replacement is observable packet disruption,
        # so only reprogram when the selected forwarding path changes.
        if selected == self.active_path:
            return {"selected_path": selected, "route_available": bool(path),
                    "routes_changed": False}
        # Routes are scoped to owned namespaces and only to experiment /32
        # endpoints. This is an explicit centralized controller, *not* FRR.
        for node in self.plan["nodes"]:
            ns = node["namespace"]
            if node["id"] != self.plan["target"]:
                self.run(["ip", "-n", ns, "route", "del", self.plan["target_ip"] + "/32"], check=False)
            if node["id"] != self.plan["source"]:
                self.run(["ip", "-n", ns, "route", "del", self.plan["source_ip"] + "/32"], check=False)
        for i, link in enumerate(path):
            if i < len(path) - 1:
                self.run([
                    "ip", "-n", link["left_ns"], "route", "replace",
                    self.plan["target_ip"] + "/32", "via", link["right_ip"],
                    "dev", link["left_if"],
                ])
            if i > 0:
                self.run([
                    "ip", "-n", link["right_ns"], "route", "replace",
                    self.plan["source_ip"] + "/32", "via", link["left_ip"],
                    "dev", link["right_if"],
                ])
        self.active_path = selected
        return {"selected_path": selected, "route_available": bool(path),
                "routes_changed": True}

    def fault_and_reconfigure(self, event: dict) -> dict:
        link = next(edge for edge in self.plan["links"] if edge["id"] == event["link_id"])
        action = event["action"]
        for ns, iface in ((link["left_ns"], link["left_if"]), (link["right_ns"], link["right_if"])):
            self.run(["ip", "-n", ns, "link", "set", iface, action])
        if action == "down":
            self.disabled.add(link["id"])
        else:
            self.disabled.discard(link["id"])
        before = time.monotonic_ns()
        route = self.program_routes()
        route["controller_apply_elapsed_ns"] = time.monotonic_ns() - before
        return route


def parse_continuous_ping(output: str) -> dict:
    """Summarize observed kernel ping replies without claiming exact outage onset."""
    replies = []
    missed = []
    unanswered_samples = []
    for line in output.splitlines():
        match = re.search(r"^\[(\d+(?:\.\d+)?)\].*icmp_seq=(\d+)", line)
        if not match:
            continue
        timestamp_ns = int(Decimal(match.group(1)) * 1_000_000_000)
        seq = int(match.group(2))
        if "bytes from" in line:
            replies.append((seq, timestamp_ns))
        elif "no answer yet" in line:
            missed.append(seq)
            unanswered_samples.append({"seq": seq, "wall_utc_ns": timestamp_ns})
    # ping -O reports an unanswered probe at the next send, not a final
    # packet loss. A delayed reply may still arrive later in the capture.
    replied_sequences = {seq for seq, _ in replies}
    confirmed_unanswered = [
        sample for sample in unanswered_samples
        if sample["seq"] not in replied_sequences
    ]
    gaps = [
        {"before_seq": previous[0], "after_seq": current[0],
         "elapsed_wall_ns": current[1] - previous[1],
         "missing_sequence_count": max(0, current[0] - previous[0] - 1)}
        for previous, current in zip(replies, replies[1:])
        if current[0] > previous[0] + 1
    ]
    return {
        "received_reply_count": len(replies),
        "observed_missing_sequences": sorted(set(missed) - replied_sequences),
        "late_reply_sequences": sorted(set(missed) & replied_sequences),
        "reply_samples": [
            {"seq": seq, "wall_utc_ns": stamp} for seq, stamp in replies
        ],
        "unanswered_samples": confirmed_unanswered,
        "provisional_unanswered_samples": unanswered_samples,
        "reply_gaps": gaps,
        "largest_observed_reply_gap_wall_ns": max(
            (gap["elapsed_wall_ns"] for gap in gaps), default=None
        ),
        "interpretation": "sampled wall-clock reply gap, not exact outage or protocol convergence",
    }


def correlate_route_transition_observations(events: list[dict], ping: dict) -> list[dict]:
    """Join actual S2 route changes with UTC-tagged ICMP observations.

    iputils ping -D uses CLOCK_REALTIME/Unix UTC; controller timestamps use
    time.time_ns() from the same host, never S1 virtual time. ICMP sampling
    brackets *observed* disruption but cannot establish exact blackout onset
    or dynamic-routing protocol convergence.
    """
    replies = sorted(ping["reply_samples"], key=lambda item: item["wall_utc_ns"])
    unanswered = sorted(ping["unanswered_samples"], key=lambda item: item["wall_utc_ns"])
    observations = []
    for index, event in enumerate(events):
        start = event["fault_start_wall_utc_ns"]
        applied = event["route_applied_wall_utc_ns"]
        next_start = (
            events[index + 1]["fault_start_wall_utc_ns"]
            if index + 1 < len(events) else None
        )
        if applied < start:
            raise RuntimeError("host wall clock moved backwards during route update")
        if next_start is not None and next_start <= start:
            raise RuntimeError("fault wall-clock timestamps are not increasing")
        in_window = lambda t: start <= t and (next_start is None or t < next_start)
        missing = [sample for sample in unanswered if in_window(sample["wall_utc_ns"])]
        after_apply = next(
            (sample for sample in replies
             if sample["wall_utc_ns"] >= applied and
             (next_start is None or sample["wall_utc_ns"] < next_start)),
            None,
        )
        first_missing = missing[0] if missing else None
        before_missing = (
            next(
                (sample for sample in reversed(replies)
                 if sample["wall_utc_ns"] < first_missing["wall_utc_ns"]),
                None,
            )
            if first_missing else None
        )
        after_missing = (
            next(
                (sample for sample in replies
                 if sample["wall_utc_ns"] > first_missing["wall_utc_ns"]
                 and (next_start is None or sample["wall_utc_ns"] < next_start)),
                None,
            )
            if first_missing else None
        )
        if first_missing and after_missing:
            assessment = "sampled_unanswered_then_replied"
        elif first_missing:
            assessment = "sampled_unanswered_without_recovery_in_window"
        elif after_apply:
            assessment = "no_unanswered_observed_at_probe_cadence"
        else:
            assessment = "insufficient_probe_evidence"
        observations.append({
            "link_id": event["link_id"],
            "action": event["action"],
            "clock": "S2_host_UTC_wall_clock_not_S1_virtual_time",
            "fault_start_wall_utc_ns": start,
            "route_applied_wall_utc_ns": applied,
            "next_fault_start_wall_utc_ns": next_start,
            "observed_unanswered_at_probe_interval": len(missing),
            "first_unanswered_wall_utc_ns": (
                first_missing["wall_utc_ns"] if first_missing else None
            ),
            "first_reply_after_apply_wall_utc_ns": (
                after_apply["wall_utc_ns"] if after_apply else None
            ),
            "first_reply_after_apply_elapsed_wall_ns": (
                after_apply["wall_utc_ns"] - applied if after_apply else None
            ),
            "first_reply_after_unanswered_wall_utc_ns": (
                after_missing["wall_utc_ns"] if after_missing else None
            ),
            "sampled_reply_gap_bracketing_loss_wall_ns": (
                after_missing["wall_utc_ns"] - before_missing["wall_utc_ns"]
                if after_missing and before_missing else None
            ),
            "assessment": assessment,
        })
    return observations


def remaining_probe_window_ns(origin_ns: int, duration_ns: int, scale: float, now_ns: int) -> int:
    """Keep the live probe running until the full scaled scenario horizon."""
    return max(0, origin_ns + int(duration_ns * scale) - now_ns)


def execute_graph(plan: dict, *, scale: float, iperf: bool, pcap: Path | None) -> dict:
    if not 1.0 <= scale <= 100.0 or plan["duration_ns"] * scale / 1e9 > 120:
        raise PlanError("invalid time-scale or excessive wall duration")
    worker = LinuxRouteGraph(plan)
    report = {
        "fidelity": "S2-real-IP-centralized-route-controller-not-FRR",
        "source_sha256": plan["source_sha256"],
        "kernel": platform.release(), "scale": scale,
        "events": [], "probes": [],
    }
    continuous_ping = None
    try:
        worker.setup()
        report["initial_path"] = worker.active_path
        report["initial_kernel_state"] = worker.snapshot()
        if pcap:
            worker.start_capture(pcap.resolve())
            report["pcap_path"] = str(pcap.resolve())
        report["probes"].append(worker.ping("initial"))
        if iperf:
            report["iperf_initial"] = worker.iperf()
        # A real, sustained host-clock probe spans the entire fault/recovery
        # sequence. The post-route probes alone cannot reveal lost traffic.
        source_ns = next(node["namespace"] for node in plan["nodes"]
                         if node["id"] == plan["source"])
        continuous_ping = subprocess.Popen(
            ["ip", "netns", "exec", source_ns, "ping", "-n", "-D", "-O",
             "-i", "0.2", plan["target_ip"]],
            stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True,
        )
        origin_wall_utc_before_ns = time.time_ns()
        origin = time.monotonic_ns()
        origin_wall_utc_after_ns = time.time_ns()
        report["probe_clock"] = {
            "source": "S2_host_monotonic_schedule_and_host_UTC_ping_D",
            "origin_monotonic_ns": origin,
            "origin_wall_utc_ns": origin_wall_utc_before_ns,
            "origin_clock_pair_uncertainty_ns": (
                origin_wall_utc_after_ns - origin_wall_utc_before_ns
            ),
            "warning": "host UTC can be adjusted; do not equate ping timestamps with S1 virtual time",
        }
        for event in plan["faults"]:
            target_offset = int(event["at_ns"] * scale)
            wait_s = (origin + target_offset - time.monotonic_ns()) / 1e9
            if wait_s > 0:
                time.sleep(wait_s)
            at = time.monotonic_ns()
            fault_start_wall_utc_ns = time.time_ns()
            result = worker.fault_and_reconfigure(event)
            route_applied_wall_utc_ns = time.time_ns()
            controller_total_elapsed_monotonic_ns = time.monotonic_ns() - at
            ping = worker.ping(f'{event["link_id"]}:{event["action"]}:rerouted')
            report["probes"].append(ping)
            report["events"].append({
                **event, **result, "applied_wall_offset_ns": time.monotonic_ns() - origin,
                "scheduler_lateness_ns": max(0, at - origin - target_offset),
                "fault_start_wall_utc_ns": fault_start_wall_utc_ns,
                "route_applied_wall_utc_ns": route_applied_wall_utc_ns,
                "controller_total_elapsed_monotonic_ns": controller_total_elapsed_monotonic_ns,
                "post_route_ping_success": ping["success"],
                "post_route_rtt_ms": ping["rtt_ms"],
            })
        # A final recovery event can occur near the end of the fault list.
        # Without this hold, SIGINT truncates the probe before any recovered
        # replies can be sampled. The horizon is wall-clock, not S1 time.
        remaining_ns = remaining_probe_window_ns(
            origin, plan["duration_ns"], scale, time.monotonic_ns()
        )
        if remaining_ns:
            time.sleep(remaining_ns / 1e9)
        report["probe_window_wall_ns"] = time.monotonic_ns() - origin
        report["expected_probe_window_wall_ns"] = int(plan["duration_ns"] * scale)
        report["final_kernel_state"] = worker.snapshot()
    finally:
        if continuous_ping is not None:
            if continuous_ping.poll() is None:
                continuous_ping.send_signal(signal.SIGINT)
            try:
                stdout, stderr = continuous_ping.communicate(timeout=5)
            except subprocess.TimeoutExpired:
                continuous_ping.kill()
                stdout, stderr = continuous_ping.communicate()
                report["continuous_ping_warning"] = "probe required forced termination"
            report["continuous_ping"] = parse_continuous_ping(stdout)
            report["continuous_ping"]["stderr_tail"] = stderr[-300:]
            report["route_transition_observations"] = (
                correlate_route_transition_observations(
                    report["events"], report["continuous_ping"]
                )
            )
        else:
            report["continuous_ping_warning"] = "probe did not start"
        report["cleanup_errors"] = worker.close()
        report["operation_count"] = len(worker.commands)
        report["operations"] = worker.commands
    return report


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("scenario", type=Path)
    parser.add_argument("--source", required=True)
    parser.add_argument("--target", required=True)
    parser.add_argument("--tag", default="r2")
    parser.add_argument("--apply", action="store_true")
    parser.add_argument("--time-scale", type=float, default=20.0)
    parser.add_argument("--iperf", action="store_true")
    parser.add_argument("--pcap", type=Path)
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    try:
        plan = build_graph(args.scenario.read_bytes(), args.source, args.target, args.tag)
        if not args.apply:
            if args.iperf or args.pcap:
                raise PlanError("--iperf/--pcap require --apply")
            result = {"mode": "dry-run", "plan": plan, "changes_applied": False}
        else:
            result = {
                "mode": "applied", "plan": plan,
                "report": execute_graph(plan, scale=args.time_scale, iperf=args.iperf, pcap=args.pcap),
            }
        serialized = json.dumps(result, indent=2, sort_keys=True) + "\n"
        if args.output:
            args.output.write_text(serialized, encoding="utf-8")
        else:
            sys.stdout.write(serialized)
        return 0
    except (OSError, RuntimeError, PlanError, subprocess.SubprocessError) as exc:
        print(f"S2 routing experiment failed: {exc}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
