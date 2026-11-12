"""Safety and deterministic control-plan tests for S2 route failover."""
import copy
import json
from pathlib import Path
import subprocess
import sys
import unittest

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "systems" / "network-emulation"))
from s2_routing import PlanError, build_graph, find_path, parse_continuous_ping  # noqa: E402

FIXTURE = ROOT / "scenarios" / "s2-route-failover.json"


class S2RouteTests(unittest.TestCase):
    def setUp(self):
        self.raw = json.loads(FIXTURE.read_text())

    def plan(self, obj=None):
        return build_graph(
            json.dumps(obj if obj is not None else self.raw).encode(),
            "ue-a", "internet", "r2",
        )

    def test_continuous_probe_detects_missing_replies_and_bounds_gap(self):
        output = (
            "[1000.000000] 64 bytes from 10.248.7.2: icmp_seq=1 ttl=64 time=1 ms\n"
            "[1000.200000] 64 bytes from 10.248.7.2: icmp_seq=2 ttl=64 time=1 ms\n"
            "[1000.400000] no answer yet for icmp_seq=3\n"
            "[1000.600000] no answer yet for icmp_seq=4\n"
            "[1000.800000] 64 bytes from 10.248.7.2: icmp_seq=5 ttl=64 time=1 ms\n"
        )
        result = parse_continuous_ping(output)
        self.assertEqual(result["received_reply_count"], 3)
        self.assertEqual(result["observed_missing_sequences"], [3, 4])
        self.assertEqual(result["reply_gaps"][0]["missing_sequence_count"], 2)
        self.assertEqual(result["largest_observed_reply_gap_wall_ns"], 600_000_000)
        self.assertIsNone(parse_continuous_ping("no replies")["largest_observed_reply_gap_wall_ns"])

    def test_initial_backup_and_recovery_topology(self):
        plan = self.plan()
        self.assertEqual(len(plan["links"]), 7)
        initial = plan["initial_path"]
        self.assertIn("isl-a-b", initial)
        self.assertEqual(initial[0], "user-ue-a")
        self.assertEqual(initial[-1], "pop-internet")
        failed = [x["id"] for x in find_path(plan, {"isl-a-b"})]
        self.assertNotIn("isl-a-b", failed)
        self.assertIn("isl-a-c", failed)
        self.assertIn("isl-c-b", failed)
        self.assertEqual(failed[0], initial[0])
        self.assertEqual(failed[-1], initial[-1])
        self.assertEqual([x["id"] for x in find_path(plan, set())], initial)

    def test_link_and_node_reordering_preserves_route_and_fault_order(self):
        scrambled = copy.deepcopy(self.raw)
        scrambled["nodes"].reverse()
        scrambled["links"].reverse()
        scrambled["faults"].reverse()
        initial, reordered = self.plan(), self.plan(scrambled)
        for field in ("nodes", "links", "faults", "initial_path", "source_ip", "target_ip"):
            self.assertEqual(initial[field], reordered[field])
        self.assertNotEqual(initial["source_sha256"], reordered["source_sha256"])

    def test_reject_duplicate_or_unsafe_fault_input(self):
        duplicate = copy.deepcopy(self.raw)
        duplicate["faults"].append(duplicate["faults"][0])
        with self.assertRaisesRegex(PlanError, "ambiguous"):
            self.plan(duplicate)
        duplicate = copy.deepcopy(self.raw)
        duplicate["faults"][0]["action"] = "reconfigure; rm -rf /"
        with self.assertRaisesRegex(PlanError, "up/down"):
            self.plan(duplicate)

    def test_safety_budget_and_bad_topology_rejected(self):
        altered = copy.deepcopy(self.raw)
        altered["links"].extend([copy.deepcopy(altered["links"][0]) for _ in range(2)])
        with self.assertRaisesRegex(PlanError, "graph exceeds"):
            self.plan(altered)
        altered = copy.deepcopy(self.raw)
        altered["links"][0]["id"] = "bad|pipe"
        with self.assertRaises(PlanError):
            self.plan(altered)
        with self.assertRaises(PlanError):
            build_graph(FIXTURE.read_bytes(), "ue-a", "internet", "../bad")

    def test_default_dry_run_never_applies_privileged_changes(self):
        args = [
            sys.executable,
            str(ROOT / "systems" / "network-emulation" / "s2_routing.py"),
            str(FIXTURE), "--source", "ue-a", "--target", "internet",
        ]
        first = subprocess.run(args, capture_output=True, text=True, check=True)
        second = subprocess.run(args, capture_output=True, text=True, check=True)
        self.assertEqual(first.stdout, second.stdout)
        value = json.loads(first.stdout)
        self.assertEqual(value["mode"], "dry-run")
        self.assertFalse(value["changes_applied"])
        self.assertIn("isl-a-b", value["plan"]["initial_path"])
        rejected = subprocess.run(args + ["--pcap", "/tmp/unsafe"], capture_output=True, text=True)
        self.assertEqual(rejected.returncode, 2)
        self.assertIn("--apply", rejected.stderr)


if __name__ == "__main__":
    unittest.main()
