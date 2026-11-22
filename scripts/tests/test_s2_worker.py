"""Contract and safety tests for the optional Linux IP emulation worker."""
from __future__ import annotations

import copy
import json
from pathlib import Path
import subprocess
import sys
import unittest
from unittest.mock import MagicMock, patch

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "systems" / "network-emulation"))
from s2_worker import (  # noqa: E402
    LinuxPath, PlanError, build_plan, clock_pair, correlate_probe_events,
    summarize_ping_stream, execute,
)

SCENARIO = ROOT / "scenarios" / "s2-linux-ip-path.json"


class S2WorkerTests(unittest.TestCase):
    def setUp(self):
        self.data = json.loads(SCENARIO.read_text())

    def plan(self, data=None):
        return build_plan(
            json.dumps(self.data if data is None else data).encode(),
            "ue-a", "gw-a", "s2",
        )

    def test_canonical_multihop_plan_and_recovery(self):
        plan = self.plan()
        self.assertEqual(plan["profile"], "S2-Linux-IP-wall-clock-v1")
        self.assertEqual([x["id"] for x in plan["nodes"]], ["ue-a", "sat-a", "sat-b", "gw-a"])
        self.assertEqual(
            [x["id"] for x in plan["links"]],
            ["user-uplink", "space-isl", "feeder-downlink"],
        )
        self.assertEqual(plan["target_ip"], "10.248.3.2")
        self.assertEqual([f["action"] for f in plan["faults"]], ["down", "up"])
        self.assertEqual(
            [(f["left_ns"], f["right_ns"]) for f in plan["links"]],
            [("nts2n0", "nts2n1"), ("nts2n1", "nts2n2"), ("nts2n2", "nts2n3")],
        )

    def test_path_selection_is_deterministic_on_unsorted_links(self):
        data = copy.deepcopy(self.data)
        data["links"].reverse()
        data["nodes"].reverse()
        first = self.plan()
        second = self.plan(data)
        self.assertEqual(first["links"], second["links"])
        self.assertEqual(first["faults"], second["faults"])
        self.assertNotEqual(first["source_sha256"], second["source_sha256"])

    def test_bounded_dense_topology_uses_stable_shortest_path(self):
        data = copy.deepcopy(self.data)
        data["nodes"] = [{"id": f"n{i}"} for i in range(60)]
        data["links"] = [
            {
                "id": f"e{i:02d}-{j:02d}", "from": f"n{i}", "to": f"n{j}",
                "capacity_bps": 1_000_000, "propagation_delay_ns": 1000,
            }
            for i in range(59) for j in range(i + 1, min(60, i + 9))
        ]
        data["faults"] = []
        raw = json.dumps(data).encode()
        first = build_plan(raw, "n0", "n59", "s2")
        data["links"].reverse()
        second = build_plan(json.dumps(data).encode(), "n0", "n59", "s2")
        self.assertEqual(
            [link["id"] for link in first["links"]],
            [link["id"] for link in second["links"]],
        )
        self.assertLessEqual(len(first["links"]), 8)

    def test_reject_oversized_topology_before_path_expansion(self):
        data = copy.deepcopy(self.data)
        data["nodes"].extend(
            {"id": f"extra{i}"} for i in range(129)
        )
        with self.assertRaisesRegex(PlanError, "bounded node/link"):
            self.plan(data)
        data = copy.deepcopy(self.data)
        data["links"].extend(
            {
                "id": f"extra{i}", "from": "ue-a", "to": "sat-a",
                "capacity_bps": 1_000_000, "propagation_delay_ns": 1000,
            }
            for i in range(513)
        )
        with self.assertRaisesRegex(PlanError, "bounded node/link"):
            self.plan(data)

    def test_reject_ambiguous_fault_and_invalid_time(self):
        data = copy.deepcopy(self.data)
        data["faults"].append(copy.deepcopy(data["faults"][0]))
        with self.assertRaisesRegex(PlanError, "ambiguous"):
            self.plan(data)
        data = copy.deepcopy(self.data)
        data["faults"][0]["at_ns"] = data["duration_ns"]
        with self.assertRaisesRegex(PlanError, "fault.at_ns"):
            self.plan(data)

    def test_off_path_faults_still_validate_source_contract(self):
        data = copy.deepcopy(self.data)
        data["links"].append({
            "id": "reverse-only", "from": "gw-a", "to": "ue-a",
            "capacity_bps": 1_000_000,
            "propagation_delay_ns": 1_000_000,
        })
        data["faults"].append({
            "link_id": "reverse-only", "at_ns": 1, "action": "down",
        })
        plan = self.plan(data)
        self.assertNotIn("reverse-only", [x["id"] for x in plan["links"]])
        self.assertNotIn("reverse-only", [x["link_id"] for x in plan["faults"]])

        data["faults"][-1]["action"] = "invalid"
        with self.assertRaisesRegex(PlanError, "fault action"):
            self.plan(data)
        data["faults"][-1]["action"] = "down"
        data["faults"][-1]["at_ns"] = data["duration_ns"]
        with self.assertRaisesRegex(PlanError, "fault.at_ns"):
            self.plan(data)

    def test_off_path_duplicate_faults_reject_before_projection(self):
        data = copy.deepcopy(self.data)
        data["links"].append({
            "id": "reverse-only", "from": "gw-a", "to": "ue-a",
            "capacity_bps": 1_000_000, "propagation_delay_ns": 1_000_000,
        })
        data["faults"].extend([
            {"link_id": "reverse-only", "at_ns": 1, "action": "down"},
            {"link_id": "reverse-only", "at_ns": 1, "action": "up"},
        ])
        with self.assertRaisesRegex(PlanError, "ambiguous"):
            self.plan(data)
        data["faults"][-1]["action"] = "down"
        with self.assertRaisesRegex(PlanError, "ambiguous"):
            self.plan(data)

    def test_off_path_fault_count_obeys_source_safety_limit(self):
        data = copy.deepcopy(self.data)
        data["links"].append({
            "id": "reverse-only", "from": "gw-a", "to": "ue-a",
            "capacity_bps": 1_000_000, "propagation_delay_ns": 1_000_000,
        })
        # All source events are valid and distinct, but none are executed on
        # the chosen path. They must still consume the scenario safety budget.
        from s2_worker import MAX_FAULTS
        data["faults"] = [
            {"link_id": "reverse-only", "at_ns": at_ns, "action": "down"}
            for at_ns in range(MAX_FAULTS + 1)
        ]
        with self.assertRaisesRegex(PlanError, "too many faults"):
            self.plan(data)
        data["faults"].pop()
        self.assertEqual(self.plan(data)["faults"], [])

    def test_reject_unsafe_ids_rates_and_unsupported_initial_state(self):
        data = copy.deepcopy(self.data)
        data["nodes"][0]["id"] = "ue-a;rm -rf /"
        with self.assertRaises(PlanError):
            self.plan(data)
        data = copy.deepcopy(self.data)
        data["links"][0]["capacity_bps"] = True
        with self.assertRaises(PlanError):
            self.plan(data)
        data = copy.deepcopy(self.data)
        data["links"][0]["up"] = False
        with self.assertRaisesRegex(PlanError, "initially up"):
            self.plan(data)
        with self.assertRaises(PlanError):
            build_plan(SCENARIO.read_bytes(), "ue-a", "gw-a", "../unsafe")

    def test_reject_unreachable_or_oversized_path(self):
        data = copy.deepcopy(self.data)
        data["links"] = data["links"][:1]
        with self.assertRaisesRegex(PlanError, "no directed path"):
            self.plan(data)
        data = copy.deepcopy(self.data)
        data["duration_ns"] = 31_000_000_000
        with self.assertRaisesRegex(PlanError, "duration_ns"):
            self.plan(data)

    def test_second_endpoint_failure_rolls_back_first_endpoint(self):
        worker = LinuxPath(self.plan())
        calls = []
        failed = False

        def apply(command):
            nonlocal failed
            calls.append(command)
            if command[-1] == "down" and len(calls) == 2 and not failed:
                failed = True
                raise RuntimeError("second endpoint failed")

        with patch.object(worker, "run", side_effect=apply):
            with self.assertRaisesRegex(RuntimeError, "second endpoint failed"):
                worker.set_fault({"link_id": "space-isl", "action": "down"})
        self.assertEqual([c[-1] for c in calls], ["down", "down", "up"])
        self.assertEqual(calls[0][1:6], calls[-1][1:6])

    def test_failed_rollback_blocks_further_faults(self):
        worker = LinuxPath(self.plan())
        calls = []

        def apply(command):
            calls.append(command)
            if len(calls) >= 2:
                raise RuntimeError("kernel failure")

        with patch.object(worker, "run", side_effect=apply):
            with self.assertRaisesRegex(RuntimeError, "rollback was incomplete"):
                worker.set_fault({"link_id": "space-isl", "action": "down"})
        with patch.object(worker, "run") as run:
            with self.assertRaisesRegex(RuntimeError, "state uncertain"):
                worker.set_fault({"link_id": "space-isl", "action": "up"})
            run.assert_not_called()

    def test_route_recovery_failure_returns_both_endpoints_down(self):
        worker = LinuxPath(self.plan())
        calls = []

        def apply(command):
            calls.append(command)
            if command[3:5] == ["route", "replace"]:
                raise RuntimeError("route install failed")

        with patch.object(worker, "run", side_effect=apply):
            with self.assertRaisesRegex(RuntimeError, "route install failed"):
                worker.set_fault({"link_id": "space-isl", "action": "up"})
        self.assertEqual([c[-1] for c in calls[:2]], ["up", "up"])
        self.assertEqual([c[-1] for c in calls[-2:]], ["down", "down"])
        self.assertEqual(calls[0][1:6], calls[-1][1:6])
        self.assertEqual(calls[1][1:6], calls[-2][1:6])
        self.assertFalse(getattr(worker, "_fault_state_uncertain", False))

    def test_route_recovery_rollback_failure_blocks_future_faults(self):
        worker = LinuxPath(self.plan())
        calls = []

        def apply(command):
            calls.append(command)
            if command[3:5] == ["route", "replace"] or command[-1] == "down":
                raise RuntimeError("kernel failure")

        with patch.object(worker, "run", side_effect=apply):
            with self.assertRaisesRegex(RuntimeError, "route recovery rollback was incomplete"):
                worker.set_fault({"link_id": "space-isl", "action": "up"})
        with patch.object(worker, "run") as run:
            with self.assertRaisesRegex(RuntimeError, "state uncertain"):
                worker.set_fault({"link_id": "space-isl", "action": "down"})
            run.assert_not_called()

    def test_second_route_failure_rolls_back_after_first_route_replaced(self):
        worker = LinuxPath(self.plan())
        calls = []
        route_count = 0

        def apply(command):
            nonlocal route_count
            calls.append(command)
            if command[3:5] == ["route", "replace"]:
                route_count += 1
                if route_count == 2:
                    raise RuntimeError("return route failed")

        with patch.object(worker, "run", side_effect=apply):
            with self.assertRaisesRegex(RuntimeError, "return route failed"):
                worker.set_fault({"link_id": "space-isl", "action": "up"})
        self.assertEqual(route_count, 2)
        self.assertEqual([c[-1] for c in calls[-2:]], ["down", "down"])

    def test_continuous_ping_parser_distinguishes_missing_and_late_replies(self):
        output = (
            "[1795080000.100] 64 bytes from 10.248.3.2: icmp_seq=1 ttl=61 time=12.3 ms\n"
            "[1795080000.300] no answer yet for icmp_seq=2\n"
            "[1795080000.500] no answer yet for icmp_seq=3\n"
            "[1795080000.600] 64 bytes from 10.248.3.2: icmp_seq=3 ttl=61 time=300 ms\n"
            "[1795080000.700] 64 bytes from 10.248.3.2: icmp_seq=4 ttl=61 time=13.1 ms\n"
        )
        result = summarize_ping_stream(output)
        self.assertEqual(result["received"], 3)
        self.assertEqual(result["reported_missing"], 1)
        self.assertEqual(result["missing_sequences"], [2])
        self.assertEqual([x["sequence"] for x in result["received_samples"]], [1, 3, 4])
        self.assertEqual(result["received_samples"][0]["rtt_ms"], 12.3)
        self.assertIn("not S1 virtual time", result["timestamp_scale"])

        self.assertEqual(result["observed_missing_windows"], [{
            "first_sequence": 2, "last_sequence": 2, "missing_count": 1,
            "first_subsequent_reply_sequence": 3,
            "first_subsequent_reply_wall_epoch_ns": result["received_samples"][1]["wall_epoch_ns"],
        }])

    def test_probe_evidence_preserves_nanoseconds_and_deduplicates_late_replies(self):
        output = (
            "[1795080000.123456789] no answer yet for icmp_seq=7\n"
            "[1795080000.123456790] 64 bytes from 10.248.3.2: icmp_seq=7 ttl=61 time=2 ms\n"
            "[1795080000.123456791] 64 bytes from 10.248.3.2: icmp_seq=7 ttl=61 time=3 ms\n"
            "[1795080000.123456792] no answer yet for icmp_seq=8\n"
            "[1795080000.123456793] 64 bytes from 10.248.3.2: icmp_seq=9 ttl=61 time=4 ms\n"
        )
        result = summarize_ping_stream(output)
        self.assertEqual(result["received"], 2)
        self.assertEqual(result["duplicate_reply_count"], 1)
        self.assertEqual(result["late_reply_sequences"], [7])
        self.assertEqual(result["reported_missing"], 1)
        self.assertEqual(result["missing_sequences"], [8])
        self.assertEqual(
            result["received_samples"][0]["wall_epoch_ns"], 1795080000123456790
        )
        self.assertEqual(result["observed_missing_windows"][0][
            "first_subsequent_reply_wall_epoch_ns"], 1795080000123456793
        )
        # Reordering duplicate reply lines must not change first-arrival evidence.
        lines = output.splitlines()
        lines[1], lines[2] = lines[2], lines[1]
        self.assertEqual(
            summarize_ping_stream("\n".join(lines))["received_samples"],
            result["received_samples"],
        )

    def test_continuous_probe_loss_windows_are_observations_not_blackout_durations(self):
        output = (
            "[1795080000.100] no answer yet for icmp_seq=2\n"
            "[1795080000.200] no answer yet for icmp_seq=3\n"
            "[1795080000.300] 64 bytes from 10.248.3.2: icmp_seq=4 ttl=61 time=10 ms\n"
            "[1795080000.400] no answer yet for icmp_seq=6\n"
        )
        result = summarize_ping_stream(output)
        self.assertEqual(result["reported_missing"], 3)
        self.assertEqual([(w["first_sequence"], w["last_sequence"], w["missing_count"])
                          for w in result["observed_missing_windows"]], [(2, 3, 2), (6, 6, 1)])
        self.assertEqual(result["observed_missing_windows"][0]["first_subsequent_reply_sequence"], 4)
        self.assertIsNone(result["observed_missing_windows"][1]["first_subsequent_reply_sequence"])
        self.assertIn("not measured continuous blackout durations", result["limitations"])

    def test_clock_pair_bounds_monotonic_sampling(self):
        with patch("s2_worker.time.monotonic_ns", side_effect=[100, 110]):
            with patch("s2_worker.time.time_ns", return_value=5000):
                pair = clock_pair()
        self.assertEqual(pair["wall_epoch_ns"], 5000)
        self.assertEqual(pair["monotonic_midpoint_ns"], 105)
        self.assertEqual(pair["sampling_uncertainty_ns"], 5)

    def test_probe_correlation_stops_at_next_transition(self):
        events = [
            {"link_id": "space-isl", "action": "down",
             "clock_after_apply": {"wall_epoch_ns": 100},
             "clock_before_apply": {"wall_epoch_ns": 90}},
            {"link_id": "space-isl", "action": "up",
             "clock_after_apply": {"wall_epoch_ns": 300},
             "clock_before_apply": {"wall_epoch_ns": 290}},
        ]
        traffic = {"received_samples": [
            {"sequence": 1, "wall_epoch_ns": 99},
            {"sequence": 2, "wall_epoch_ns": 200},
            {"sequence": 3, "wall_epoch_ns": 290},
            {"sequence": 4, "wall_epoch_ns": 301},
        ]}
        correlated = correlate_probe_events(events, traffic)
        self.assertEqual(correlated[0]["first_observed_reply_sequence"], 2)
        self.assertEqual(correlated[0]["observed_reply_count"], 1)
        self.assertEqual(correlated[1]["first_observed_reply_sequence"], 4)
        self.assertEqual(correlated[1]["observed_reply_count"], 1)
        self.assertIn("not protocol convergence", correlated[0]["interpretation"])

    def test_continuous_probe_uses_disk_backed_output_and_closes_handles(self):
        worker = LinuxPath(self.plan())

        def launch(command, **kwargs):
            self.assertNotEqual(kwargs["stdout"], subprocess.PIPE)
            self.assertNotEqual(kwargs["stderr"], subprocess.PIPE)
            self.assertIn("-O", command)
            kwargs["stdout"].write(
                "[1795080000.100] 64 bytes from 10.248.3.2: icmp_seq=1 ttl=61 time=12.3 ms\\n"
            )
            kwargs["stderr"].write("probe warning\\n")
            return object()

        with patch("s2_worker.subprocess.Popen", side_effect=launch):
            worker.start_continuous_ping()
        stdout, stderr = worker.read_probe_output()
        self.assertEqual(summarize_ping_stream(stdout)["received"], 1)
        self.assertIn("probe warning", stderr)
        handles = (worker.probe_output, worker.probe_errors)
        worker.close_probe_output()
        self.assertTrue(all(handle.closed for handle in handles))
        worker.close_probe_output()

    def test_continuous_probe_spawn_failure_closes_temporary_files(self):
        worker = LinuxPath(self.plan())
        handles = []

        def fail(command, **kwargs):
            handles.extend((kwargs["stdout"], kwargs["stderr"]))
            raise OSError("ping unavailable")

        with patch("s2_worker.subprocess.Popen", side_effect=fail):
            with self.assertRaisesRegex(OSError, "ping unavailable"):
                worker.start_continuous_ping()
        self.assertEqual(len(handles), 2)
        self.assertTrue(all(handle.closed for handle in handles))
        self.assertIsNone(worker.probe_output)
        self.assertIsNone(worker.probe_errors)

    def test_continuous_probe_does_not_block_fault_timeline(self):
        # The controller must not run synchronous ping probes between scheduled
        # transitions: a missing reply can otherwise consume a full second.
        plan = self.plan()
        worker = MagicMock()
        worker.read_probe_output.return_value = ("", "")
        worker.start_continuous_ping.return_value.returncode = -15
        with patch("s2_worker.LinuxPath", return_value=worker), \
             patch("s2_worker.time.monotonic_ns", return_value=0), \
             patch("s2_worker.time.sleep"), \
             patch("s2_worker.clock_pair", return_value={
                 "wall_epoch_ns": 0, "monotonic_midpoint_ns": 0,
                 "sampling_uncertainty_ns": 0,
             }):
            report = execute(
                plan, scale=1.0, use_iperf=False, pcap=None,
                continuous_ping=True,
            )
        self.assertEqual(worker.set_fault.call_count, len(plan["faults"]))
        self.assertEqual(worker.ping.call_count, 1)
        self.assertEqual(len(report["probes"]), 1)
        self.assertEqual(len(report["events"]), len(plan["faults"]))
        self.assertEqual(len(report["probe_event_correlation"]), len(plan["faults"]))
        self.assertEqual(report["continuous_traffic"]["received"], 0)
        worker.close_probe_output.assert_called_once()

    def test_noncontinuous_mode_retains_transition_probes(self):
        plan = self.plan()
        worker = MagicMock()
        with patch("s2_worker.LinuxPath", return_value=worker), \
             patch("s2_worker.time.monotonic_ns", return_value=0), \
             patch("s2_worker.time.sleep"), \
             patch("s2_worker.clock_pair", return_value={
                 "wall_epoch_ns": 0, "monotonic_midpoint_ns": 0,
                 "sampling_uncertainty_ns": 0,
             }):
            report = execute(
                plan, scale=1.0, use_iperf=False, pcap=None,
                continuous_ping=False,
            )
        self.assertEqual(worker.ping.call_count, len(plan["faults"]) + 1)
        self.assertEqual(len(report["probes"]), len(plan["faults"]) + 1)
        worker.start_continuous_ping.assert_not_called()

    def test_dry_run_is_safe_and_reproducible(self):
        cmd = [
            sys.executable, str(ROOT / "systems" / "network-emulation" / "s2_worker.py"),
            str(SCENARIO), "--source", "ue-a", "--target", "gw-a",
        ]
        first = subprocess.run(cmd, capture_output=True, text=True, check=True)
        second = subprocess.run(cmd, capture_output=True, text=True, check=True)
        self.assertEqual(first.stdout, second.stdout)
        result = json.loads(first.stdout)
        self.assertEqual(result["mode"], "dry-run")
        self.assertFalse(result["changes_applied"])
        self.assertEqual(len(result["plan"]["links"]), 3)
        rejected = subprocess.run(cmd + ["--continuous-ping"], capture_output=True, text=True)
        self.assertEqual(rejected.returncode, 2)
        self.assertIn("--apply", rejected.stderr)


if __name__ == "__main__":
    unittest.main()
