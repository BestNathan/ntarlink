"""Opt-in S2 Linux IP emulation of a bounded path exported from an S1 scenario.

No virtual-time engine executes here: links are applied to a privileged Linux
network namespace worker on a wall clock. Dry-run is the default.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import shutil
import signal
import subprocess
import sys
import time

TOKEN = re.compile(r"[a-z][a-z0-9_-]{0,30}\Z")
MAX_HOPS = 8
MAX_FAULTS = 32


class PlanError(ValueError):
    """Invalid or unsafe S1-to-S2 emulation plan."""


def identifier(value: object) -> str:
    if not isinstance(value, str) or not TOKEN.fullmatch(value):
        raise PlanError("node/link ids must be lowercase ASCII identifiers")
    return value


def unsigned(value: object, label: str, *, minimum: int = 0, maximum: int = 10**13) -> int:
    if type(value) is not int or not minimum <= value <= maximum:
        raise PlanError(f"{label} must be an integer in [{minimum}, {maximum}]")
    return value


def build_plan(raw: bytes, source: str, target: str, tag: str) -> dict:
    source, target = identifier(source), identifier(target)
    if not re.fullmatch(r"[a-z][a-z0-9]{0,3}", tag):
        raise PlanError("tag must be 1-4 lowercase alphanumeric characters")
    try:
        scenario = json.loads(raw)
    except (ValueError, UnicodeError) as exc:
        raise PlanError("scenario must be valid UTF-8 JSON") from exc
    if not isinstance(scenario, dict) or scenario.get("schema_version") != 1:
        raise PlanError("only the S1 scenario-v1 contract is supported")
    duration = unsigned(scenario.get("duration_ns"), "duration_ns", minimum=1, maximum=30_000_000_000)
    node_rows, link_rows = scenario.get("nodes"), scenario.get("links")
    if not isinstance(node_rows, list) or not isinstance(link_rows, list):
        raise PlanError("scenario requires nodes and links")
    nodes = [identifier(n.get("id")) for n in node_rows if isinstance(n, dict)]
    if len(nodes) != len(node_rows) or len(nodes) != len(set(nodes)):
        raise PlanError("node ids must be present and unique")
    if source == target or source not in nodes or target not in nodes:
        raise PlanError("source and target must be distinct scenario nodes")
    links = {}
    adjacency = {node: [] for node in nodes}
    for link in link_rows:
        if not isinstance(link, dict):
            raise PlanError("each link must be an object")
        link_id = identifier(link.get("id"))
        start, end = identifier(link.get("from")), identifier(link.get("to"))
        if link_id in links or start not in adjacency or end not in adjacency or start == end:
            raise PlanError("duplicate link, unknown endpoint or self-loop")
        rate = unsigned(link.get("capacity_bps"), "capacity_bps", minimum=10_000, maximum=10**10)
        delay = unsigned(link.get("propagation_delay_ns"), "propagation_delay_ns", maximum=10**10)
        processing = unsigned(link.get("processing_delay_ns", 0), "processing_delay_ns", maximum=10**10)
        if link.get("up", True) is not True:
            raise PlanError("S2 v1 only supports links initially up")
        links[link_id] = {
            "id": link_id, "from": start, "to": end,
            "capacity_bps": rate, "delay_ns": delay + processing,
        }
        adjacency[start].append(links[link_id])
    # Deterministic shortest directed path. This is *not* the S1 control-plane
    # route; recording the selected path avoids implying routing parity.
    queue = [(source, [], [source])]
    path = None
    while queue:
        current, edges, seen = queue.pop(0)
        if current == target:
            path = edges
            break
        if len(edges) >= MAX_HOPS:
            continue
        for link in sorted(adjacency[current], key=lambda item: item["id"]):
            if link["to"] not in seen:
                queue.append((link["to"], edges + [link], seen + [link["to"]]))
    if not path:
        raise PlanError("no directed path within the eight-hop S2 safety limit")
    node_path = [source] + [link["to"] for link in path]
    if len(set(node_path)) != len(node_path):
        raise PlanError("path contains a cycle")
    events = []
    raw_faults = scenario.get("faults", [])
    if not isinstance(raw_faults, list):
        raise PlanError("faults must be a list")
    chosen = {link["id"] for link in path}
    for fault in raw_faults:
        if not isinstance(fault, dict):
            raise PlanError("fault must be an object")
        link_id = identifier(fault.get("link_id"))
        if link_id not in links:
            raise PlanError("fault references unknown link")
        if link_id not in chosen:
            continue
        at_ns = unsigned(fault.get("at_ns"), "fault.at_ns", maximum=duration - 1)
        action = fault.get("action")
        if action not in ("down", "up"):
            raise PlanError("fault action must be down or up")
        events.append({"at_ns": at_ns, "link_id": link_id, "action": action})
    events.sort(key=lambda value: (value["at_ns"], value["link_id"]))
    if len(events) > MAX_FAULTS or len({(x["at_ns"], x["link_id"]) for x in events}) != len(events):
        raise PlanError("too many faults or ambiguous same-time link transitions")
    # Map each path hop to an isolated /30, avoiding the host routing table.
    ip_links = []
    for index, link in enumerate(path):
        n = index + 1
        ip_links.append({
            **link,
            "left_ns": f"nt{tag}n{index}",
            "right_ns": f"nt{tag}n{index + 1}",
            "left_if": f"nt{tag}l{index}",
            "right_if": f"nt{tag}r{index}",
            "left_ip": f"10.248.{n}.1",
            "right_ip": f"10.248.{n}.2",
            "cidr_left": f"10.248.{n}.1/30",
            "cidr_right": f"10.248.{n}.2/30",
        })
    return {
        "profile": "S2-Linux-IP-wall-clock-v1",
        "source_fidelity": "S1-analytical-to-S2-approximation",
        "source_sha256": hashlib.sha256(raw).hexdigest(),
        "selected_path_policy": "lexical-shortest-directed-not-S1-routing",
        "nodes": [{"id": item, "namespace": f"nt{tag}n{index}"} for index, item in enumerate(node_path)],
        "links": ip_links,
        "faults": events,
        "duration_ns": duration,
        "source_ip": ip_links[0]["left_ip"],
        "target_ip": ip_links[-1]["right_ip"],
        "limits": {"max_hops": MAX_HOPS, "max_faults": MAX_FAULTS},
    }


def cmd_run(args: list[str], *, check: bool = True, timeout: int = 15) -> subprocess.CompletedProcess:
    return subprocess.run(args, text=True, capture_output=True, check=check, timeout=timeout)


class LinuxPath:
    def __init__(self, plan: dict):
        self.plan = plan
        self.namespaces: list[str] = []
        self.host_veth: list[str] = []
        self.commands: list[dict] = []
        self.capture: subprocess.Popen | None = None

    def run(self, args: list[str], *, check: bool = True, timeout: int = 15) -> subprocess.CompletedProcess:
        started = time.monotonic_ns()
        completed = cmd_run(args, check=False, timeout=timeout)
        self.commands.append({
            "argv": args, "returncode": completed.returncode,
            "wall_elapsed_ns": time.monotonic_ns() - started,
            "stderr": completed.stderr[-1200:],
        })
        if check and completed.returncode:
            raise RuntimeError(f"command failed ({completed.returncode}): {args!r}; {completed.stderr[-400:]}")
        return completed

    def setup(self) -> None:
        if os.geteuid() != 0:
            raise PlanError("--apply requires root and Linux CAP_NET_ADMIN")
        if sys.platform != "linux":
            raise PlanError("--apply requires Linux")
        for command in ("ip", "tc", "ping"):
            if not shutil.which(command):
                raise PlanError(f"--apply requires {command} from the host package manager")
        existing = self.run(["ip", "netns", "list"]).stdout
        current = {line.split()[0] for line in existing.splitlines() if line.split()}
        planned = {node["namespace"] for node in self.plan["nodes"]}
        if planned & current:
            raise PlanError("namespace prefix collision; select another --tag")
        for node in self.plan["nodes"]:
            ns = node["namespace"]
            self.run(["ip", "netns", "add", ns])
            self.namespaces.append(ns)
            self.run(["ip", "-n", ns, "link", "set", "lo", "up"])
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
                # Kernel qdisc is a wall-clock approximation, not S1 service time.
                self.run([
                    "ip", "netns", "exec", ns, "tc", "qdisc", "replace",
                    "dev", iface, "root", "netem",
                    "delay", f'{max(1, link["delay_ns"] // 1000)}us',
                    "rate", f'{link["capacity_bps"]}bit',
                ])
        # Static path routes are the baseline; routing convergence is not
        # claimed. Forwarding stays inside only these network namespaces.
        for index, node in enumerate(self.plan["nodes"]):
            ns = node["namespace"]
            if index not in (0, len(self.plan["nodes"]) - 1):
                self.run(["ip", "netns", "exec", ns, "sysctl", "-q", "-w", "net.ipv4.ip_forward=1"])
            if index < len(self.plan["links"]) - 1:
                link = self.plan["links"][index]
                self.run([
                    "ip", "-n", ns, "route", "add", self.plan["target_ip"] + "/32",
                    "via", link["right_ip"], "dev", link["left_if"],
                ])
            elif index == 0:
                link = self.plan["links"][0]
                self.run([
                    "ip", "-n", ns, "route", "add", self.plan["target_ip"] + "/32",
                    "via", link["right_ip"], "dev", link["left_if"],
                ])
            if index > 0:
                prev = self.plan["links"][index - 1]
                if index != 1:
                    self.run([
                        "ip", "-n", ns, "route", "add", self.plan["source_ip"] + "/32",
                        "via", prev["left_ip"], "dev", prev["right_if"],
                    ])
                else:
                    self.run([
                        "ip", "-n", ns, "route", "add", self.plan["source_ip"] + "/32",
                        "via", prev["left_ip"], "dev", prev["right_if"],
                    ])

    def set_fault(self, event: dict) -> None:
        index = next(
            i for i, link in enumerate(self.plan["links"])
            if link["id"] == event["link_id"]
        )
        link = self.plan["links"][index]
        state = "down" if event["action"] == "down" else "up"
        for ns, interface in ((link["left_ns"], link["left_if"]), (link["right_ns"], link["right_if"])):
            self.run(["ip", "-n", ns, "link", "set", interface, state])
        if state == "up":
            # Linux removes routes whose output interface goes down. Restoring
            # static routes here is an explicit fixed-path baseline, not a
            # claim about FRRouting convergence or network controller delay.
            if index < len(self.plan["links"]) - 1:
                self.run([
                    "ip", "-n", link["left_ns"], "route", "replace",
                    self.plan["target_ip"] + "/32", "via", link["right_ip"],
                    "dev", link["left_if"],
                ])
            if index > 0:
                self.run([
                    "ip", "-n", link["right_ns"], "route", "replace",
                    self.plan["source_ip"] + "/32", "via", link["left_ip"],
                    "dev", link["right_if"],
                ])

    def ping(self, label: str) -> dict:
        started = time.monotonic_ns()
        ns = self.plan["nodes"][0]["namespace"]
        completed = self.run([
            "ip", "netns", "exec", ns, "ping", "-n", "-c", "1", "-W", "1",
            "-I", self.plan["source_ip"], self.plan["target_ip"],
        ], check=False, timeout=5)
        matches = re.search(r"time=([0-9.]+) ms", completed.stdout)
        return {
            "label": label, "elapsed_wall_ns": time.monotonic_ns() - started,
            "success": completed.returncode == 0,
            "rtt_ms": float(matches.group(1)) if matches else None,
            "stdout": completed.stdout[-1200:],
        }

    def iperf(self) -> dict:
        if not shutil.which("iperf3"):
            raise PlanError("--iperf requires iperf3")
        sink = self.plan["nodes"][-1]["namespace"]
        source = self.plan["nodes"][0]["namespace"]
        server = subprocess.Popen([
            "ip", "netns", "exec", sink, "iperf3", "-s", "-1",
            "-B", self.plan["target_ip"],
        ], stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        try:
            time.sleep(0.2)
            client = self.run([
                "ip", "netns", "exec", source, "iperf3", "-c",
                self.plan["target_ip"], "-B", self.plan["source_ip"],
                "-t", "1", "-J",
            ], check=False, timeout=12)
            if client.returncode:
                raise RuntimeError("iperf3 failed: " + client.stderr[-500:])
            parsed = json.loads(client.stdout)
            result = parsed.get("end", {}).get("sum_received", {})
            return {
                "received_bytes": result.get("bytes"),
                "received_bps": result.get("bits_per_second"),
                "raw_fidelity": "measured-wall-clock-IP-throughput",
            }
        finally:
            if server.poll() is None:
                server.terminate()
            try:
                server.communicate(timeout=3)
            except subprocess.TimeoutExpired:
                server.kill()
                server.communicate(timeout=3)

    def start_capture(self, pcap: Path) -> None:
        if not shutil.which("tcpdump"):
            raise PlanError("--pcap requires tcpdump")
        ns = self.plan["nodes"][0]["namespace"]
        iface = self.plan["links"][0]["left_if"]
        self.capture = subprocess.Popen([
            "ip", "netns", "exec", ns, "tcpdump", "-n", "-U", "-i", iface,
            "-w", str(pcap), "ip",
        ], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        time.sleep(0.2)
        if self.capture.poll() is not None:
            raise RuntimeError("tcpdump terminated during capture startup")

    def snapshot(self) -> dict:
        return {
            "routes": {
                node["id"]: self.run(["ip", "-n", node["namespace"], "route", "show"]).stdout
                for node in self.plan["nodes"]
            },
            "qdisc": {
                link["id"]: self.run([
                    "ip", "netns", "exec", link["left_ns"], "tc", "qdisc",
                    "show", "dev", link["left_if"],
                ]).stdout
                for link in self.plan["links"]
            },
        }

    def close(self) -> list[str]:
        errors = []
        if self.capture is not None:
            if self.capture.poll() is None:
                self.capture.send_signal(signal.SIGINT)
            try:
                self.capture.wait(timeout=3)
            except subprocess.TimeoutExpired:
                self.capture.kill()
                self.capture.wait(timeout=3)
            self.capture = None
        for ns in reversed(self.namespaces):
            try:
                cmd_run(["ip", "netns", "delete", ns])
            except (OSError, subprocess.SubprocessError) as exc:
                errors.append(f"failed to remove {ns}: {exc}")
        for iface in reversed(self.host_veth):
            try:
                cmd_run(["ip", "link", "delete", iface], check=False)
            except (OSError, subprocess.SubprocessError) as exc:
                errors.append(f"failed to remove {iface}: {exc}")
        self.namespaces.clear()
        self.host_veth.clear()
        return errors


def execute(plan: dict, *, scale: float, use_iperf: bool, pcap: Path | None) -> dict:
    if not 1.0 <= scale <= 100.0 or plan["duration_ns"] * scale / 1e9 > 120.0:
        raise PlanError("time scale must be 1..100 and planned duration no more than 120 wall seconds")
    worker = LinuxPath(plan)
    report = {
        "profile": plan["profile"], "source_sha256": plan["source_sha256"],
        "fidelity": "S2-real-IP-measured-wall-time-not-S1-virtual-time",
        "environment": {
            "kernel": platform.release(), "python": platform.python_version(),
            "ip": shutil.which("ip"), "tc": shutil.which("tc"),
        },
        "time_scale": scale, "events": [], "probes": [],
    }
    try:
        worker.setup()
        report["initial_kernel_state"] = worker.snapshot()
        if pcap:
            worker.start_capture(pcap.resolve())
            report["pcap_path"] = str(pcap.resolve())
        report["probes"].append(worker.ping("initial"))
        if use_iperf:
            report["iperf"] = worker.iperf()
        start = time.monotonic_ns()
        for event in plan["faults"]:
            expected_ns = int(event["at_ns"] * scale)
            remaining = (start + expected_ns - time.monotonic_ns()) / 1e9
            if remaining > 0:
                time.sleep(remaining)
            before = time.monotonic_ns()
            worker.set_fault(event)
            report["events"].append({
                **event, "expected_wall_offset_ns": expected_ns,
                "applied_wall_offset_ns": time.monotonic_ns() - start,
                "scheduler_lateness_ns": max(0, before - start - expected_ns),
            })
            report["probes"].append(worker.ping(f'{event["link_id"]}:{event["action"]}'))
        report["final_kernel_state"] = worker.snapshot()
    finally:
        report["cleanup_errors"] = worker.close()
        report["operation_count"] = len(worker.commands)
        report["operations"] = worker.commands
    return report


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("scenario", type=Path)
    parser.add_argument("--source", required=True)
    parser.add_argument("--target", required=True)
    parser.add_argument("--tag", default="s2")
    parser.add_argument("--apply", action="store_true", help="Opt in to privileged Linux changes")
    parser.add_argument("--time-scale", type=float, default=10.0)
    parser.add_argument("--iperf", action="store_true")
    parser.add_argument("--pcap", type=Path)
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    try:
        plan = build_plan(args.scenario.read_bytes(), args.source, args.target, args.tag)
        if not args.apply:
            if args.iperf or args.pcap:
                raise PlanError("--iperf/--pcap are only available with --apply")
            output = {"mode": "dry-run", "plan": plan, "changes_applied": False}
        else:
            output = {"mode": "applied", "plan": plan, "report": execute(
                plan, scale=args.time_scale, use_iperf=args.iperf, pcap=args.pcap,
            )}
        serialized = json.dumps(output, indent=2, sort_keys=True) + "\n"
        if args.output:
            args.output.write_text(serialized, encoding="utf-8")
        else:
            sys.stdout.write(serialized)
        return 0
    except (PlanError, RuntimeError, OSError, subprocess.SubprocessError) as exc:
        print(f"S2 worker error: {exc}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
