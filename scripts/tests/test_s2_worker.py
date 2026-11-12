"""Contract and safety tests for the optional Linux IP emulation worker."""
from __future__ import annotations

import copy
import json
from pathlib import Path
import subprocess
import sys
import unittest

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "systems" / "network-emulation"))
from s2_worker import PlanError, build_plan  # noqa: E402

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

    def test_reject_ambiguous_fault_and_invalid_time(self):
        data = copy.deepcopy(self.data)
        data["faults"].append(copy.deepcopy(data["faults"][0]))
        with self.assertRaisesRegex(PlanError, "ambiguous"):
            self.plan(data)
        data = copy.deepcopy(self.data)
        data["faults"][0]["at_ns"] = data["duration_ns"]
        with self.assertRaisesRegex(PlanError, "fault.at_ns"):
            self.plan(data)

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
        rejected = subprocess.run(cmd + ["--iperf"], capture_output=True, text=True)
        self.assertEqual(rejected.returncode, 2)
        self.assertIn("--apply", rejected.stderr)


if __name__ == "__main__":
    unittest.main()
