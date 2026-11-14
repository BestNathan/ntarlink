"""Safety and deterministic control-plan tests for S2 route failover."""
import copy
import json
from pathlib import Path
import subprocess
import sys
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "systems" / "network-emulation"))
from s2_routing import (  # noqa: E402
    LinuxRouteGraph, PlanError, build_graph, correlate_route_transition_observations,
    find_path, parse_continuous_ping, remaining_probe_window_ns,
)

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
        self.assertEqual(result["reply_samples"][1],
                         {"seq": 2, "wall_utc_ns": 1_000_200_000_000})
        self.assertEqual(result["unanswered_samples"][0],
                         {"seq": 3, "wall_utc_ns": 1_000_400_000_000})
        self.assertEqual(result["reply_gaps"][0]["missing_sequence_count"], 2)
        self.assertEqual(result["largest_observed_reply_gap_wall_ns"], 600_000_000)
        self.assertIsNone(parse_continuous_ping("no replies")["largest_observed_reply_gap_wall_ns"])


    def test_late_reply_is_not_counted_as_confirmed_missing_probe(self):
        # iputils ping -O may emit "no answer yet" before a delayed reply.
        output = (
            "[1000.000000] 64 bytes from host: icmp_seq=1 ttl=64 time=1 ms\n"
            "[1000.200000] no answer yet for icmp_seq=2\n"
            "[1000.300000] 64 bytes from host: icmp_seq=2 ttl=64 time=300 ms\n"
            "[1000.400000] no answer yet for icmp_seq=3\n"
            "[1000.600000] 64 bytes from host: icmp_seq=4 ttl=64 time=1 ms\n"
        )
        ping = parse_continuous_ping(output)
        self.assertEqual(ping["late_reply_sequences"], [2])
        self.assertEqual(ping["observed_missing_sequences"], [3])
        self.assertEqual([p["seq"] for p in ping["unanswered_samples"]], [3])
        self.assertEqual(
            [p["seq"] for p in ping["provisional_unanswered_samples"]], [2, 3]
        )
        event = {
            "link_id": "isl-a-b", "action": "down",
            "fault_start_wall_utc_ns": 1_000_100_000_000,
            "route_applied_wall_utc_ns": 1_000_150_000_000,
        }
        observation = correlate_route_transition_observations([event], ping)[0]
        self.assertEqual(observation["observed_unanswered_at_probe_interval"], 1)
        self.assertEqual(observation["first_unanswered_wall_utc_ns"], 1_000_400_000_000)

    def test_route_transition_attribution_uses_real_utc_samples_not_virtual_time(self):
        output = (
            "[1000.000000] 64 bytes from 10.248.7.2: icmp_seq=1 ttl=64 time=1 ms\n"
            "[1000.200000] 64 bytes from 10.248.7.2: icmp_seq=2 ttl=64 time=1 ms\n"
            "[1000.400000] no answer yet for icmp_seq=3\n"
            "[1000.600000] no answer yet for icmp_seq=4\n"
            "[1000.800000] 64 bytes from 10.248.7.2: icmp_seq=5 ttl=64 time=1 ms\n"
            "[1001.000000] 64 bytes from 10.248.7.2: icmp_seq=6 ttl=64 time=1 ms\n"
            "[1001.200000] no answer yet for icmp_seq=7\n"
        )
        events = [
            {
                "link_id": "isl-a-b", "action": "down",
                "fault_start_wall_utc_ns": 1_000_300_000_000,
                "route_applied_wall_utc_ns": 1_000_350_000_000,
            },
            {
                "link_id": "isl-a-b", "action": "up",
                "fault_start_wall_utc_ns": 1_001_100_000_000,
                "route_applied_wall_utc_ns": 1_001_150_000_000,
            },
        ]
        rows = correlate_route_transition_observations(
            events, parse_continuous_ping(output)
        )
        self.assertEqual(len(rows), 2)
        first, second = rows
        self.assertEqual(first["assessment"], "sampled_unanswered_then_replied")
        self.assertEqual(first["observed_unanswered_at_probe_interval"], 2)
        self.assertEqual(first["first_reply_after_apply_elapsed_wall_ns"], 450_000_000)
        self.assertEqual(first["sampled_reply_gap_bracketing_loss_wall_ns"], 600_000_000)
        self.assertEqual(first["next_fault_start_wall_utc_ns"], 1_001_100_000_000)
        self.assertEqual(second["assessment"],
                         "sampled_unanswered_without_recovery_in_window")
        self.assertIsNone(second["first_reply_after_unanswered_wall_utc_ns"])
        self.assertIsNone(second["sampled_reply_gap_bracketing_loss_wall_ns"])
        self.assertEqual(second["observed_unanswered_at_probe_interval"], 1)

    def test_route_transition_without_sampled_loss_does_not_claim_no_blackout(self):
        output = (
            "[1000.000000] 64 bytes from host: icmp_seq=1 ttl=64 time=1 ms\n"
            "[1000.200000] 64 bytes from host: icmp_seq=2 ttl=64 time=1 ms\n"
            "[1000.400000] 64 bytes from host: icmp_seq=3 ttl=64 time=1 ms\n"
        )
        event = {
            "link_id": "isl-a-b", "action": "down",
            "fault_start_wall_utc_ns": 1_000_250_000_000,
            "route_applied_wall_utc_ns": 1_000_300_000_000,
        }
        result = correlate_route_transition_observations(
            [event], parse_continuous_ping(output)
        )[0]
        self.assertEqual(result["assessment"], "no_unanswered_observed_at_probe_cadence")
        self.assertEqual(result["first_reply_after_apply_elapsed_wall_ns"], 100_000_000)
        self.assertIsNone(result["sampled_reply_gap_bracketing_loss_wall_ns"])
        event["route_applied_wall_utc_ns"] = 1_000_200_000_000
        with self.assertRaisesRegex(RuntimeError, "moved backwards"):
            correlate_route_transition_observations(
                [event], parse_continuous_ping(output)
            )

    def test_disruption_attribution_never_uses_replies_after_next_fault(self):
        output = (
            "[1000.000000] 64 bytes from host: icmp_seq=1 ttl=64 time=1 ms\n"
            "[1000.400000] no answer yet for icmp_seq=2\n"
            "[1001.200000] 64 bytes from host: icmp_seq=3 ttl=64 time=1 ms\n"
        )
        events = [
            {
                "link_id": "isl-a-b", "action": "down",
                "fault_start_wall_utc_ns": 1_000_300_000_000,
                "route_applied_wall_utc_ns": 1_000_350_000_000,
            },
            {
                "link_id": "isl-a-b", "action": "up",
                "fault_start_wall_utc_ns": 1_001_100_000_000,
                "route_applied_wall_utc_ns": 1_001_150_000_000,
            },
        ]
        first, second = correlate_route_transition_observations(
            events, parse_continuous_ping(output)
        )
        self.assertEqual(first["assessment"],
                         "sampled_unanswered_without_recovery_in_window")
        self.assertIsNone(first["first_reply_after_apply_wall_utc_ns"])
        self.assertEqual(second["assessment"], "no_unanswered_observed_at_probe_cadence")
        self.assertEqual(second["first_reply_after_apply_elapsed_wall_ns"], 50_000_000)

    def test_continuous_probe_waits_for_full_scaled_horizon(self):
        origin = 1_000_000_000
        duration = 400_000_000
        scale = 20.0
        self.assertEqual(
            remaining_probe_window_ns(origin, duration, scale, origin + 4_400_000_000),
            3_600_000_000,
        )
        # A slow controller cannot cause negative sleeps or rewind the clock.
        self.assertEqual(
            remaining_probe_window_ns(origin, duration, scale, origin + 9_000_000_000),
            0,
        )
        self.assertEqual(remaining_probe_window_ns(origin, duration, 1.0, origin), duration)

    def test_unrelated_fault_preserves_active_linux_routes(self):
        plan = self.plan()
        worker = LinuxRouteGraph(plan)
        worker.active_path = list(plan["initial_path"])
        unused = next(link["id"] for link in plan["links"]
                      if link["id"] not in worker.active_path)
        with patch.object(worker, "run") as run:
            worker.disabled.add(unused)
            unchanged = worker.program_routes()
            self.assertFalse(unchanged["routes_changed"])
            self.assertEqual(unchanged["selected_path"], plan["initial_path"])
            run.assert_not_called()

            worker.disabled.add("isl-a-b")
            rerouted = worker.program_routes()
            self.assertTrue(rerouted["routes_changed"])
            self.assertNotEqual(rerouted["selected_path"], plan["initial_path"])
            self.assertTrue(run.called)

            run.reset_mock()
            worker.disabled.discard("isl-a-b")
            recovered = worker.program_routes()
            self.assertTrue(recovered["routes_changed"])
            self.assertEqual(recovered["selected_path"], plan["initial_path"])
            self.assertTrue(run.called)

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

    def test_cumulative_failures_cannot_switch_source_attachment(self):
        # The primary and same-attachment backup both disappear only after
        # two faults. Each Down in isolation preserves the original source
        # interface, but the cumulative fallback would change its IP address.
        nodes = ["src", "a", "b", "c", "d", "dst"]
        edges = [
            ("aa", "src", "a"), ("ab", "a", "b"),
            ("ac", "a", "c"), ("cb", "c", "b"),
            ("bt", "b", "dst"), ("zd", "src", "d"),
            ("dc", "d", "c"),
        ]
        graph = {
            "schema_version": 1, "duration_ns": 1000,
            "nodes": [{"id": node} for node in nodes],
            "links": [
                {"id": lid, "from": left, "to": right,
                 "capacity_bps": 100000, "propagation_delay_ns": 1000}
                for lid, left, right in edges
            ],
            "faults": [
                {"at_ns": 100, "link_id": "ab", "action": "down"},
                {"at_ns": 200, "link_id": "ac", "action": "down"},
            ],
        }
        def make_plan(value):
            return build_graph(json.dumps(value).encode(), "src", "dst", "r2")

        for event in graph["faults"]:
            single = copy.deepcopy(graph)
            single["faults"] = [event]
            self.assertEqual(make_plan(single)["initial_path"][0], "aa")
        with self.assertRaisesRegex(PlanError, "preserve source and sink attachment"):
            make_plan(graph)

        # Recovery before the second failure makes the sequence safe again.
        graph["faults"].insert(
            1, {"at_ns": 150, "link_id": "ab", "action": "up"}
        )
        self.assertEqual(make_plan(graph)["initial_path"][0], "aa")

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
