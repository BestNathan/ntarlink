"""Time-sampled SYS-01 to SYS-05 contract, numerical and CLI regression tests."""
import copy
import hashlib
import json
import math
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from ntarlink_access.rf_budget import generate_rf_budget_samples
from ntarlink_access.rf_time_series import (
    generate_time_resolved_rf_budget_samples,
    main,
)

PARAMS = {
    "carrier_hz": 12e9,
    "bandwidth_hz": 20e6,
    "tx_power_dbm": 30,
    "tx_gain_dbi": 35,
    "rx_gain_dbi": 35,
    "noise_figure_db": 3,
    "other_losses_db": 2,
    "required_cn_db": 8,
    "interference_power_dbm": -100,
    "required_sinr_db": 8,
}


class SampledRfTests(unittest.TestCase):
    def setUp(self):
        self.scenario = {
            "schema_version": 1,
            "contact_mode": "scheduled",
            "duration_ns": 10_000_000_000,
            "links": [
                {"id": "user", "kind": "user_rf", "from": "ue", "to": "sat"},
                {"id": "land", "kind": "backhaul", "from": "gw", "to": "pop"},
            ],
            "contacts": [{
                "link_id": "user",
                "start_ns": 0,
                "end_ns": 10_000_000_000,
                "propagation_delay_ns": 2_000_000,
                "propagation_delay_samples": [
                    {"at_ns": 0, "delay_ns": 2_000_000},
                    {"at_ns": 5_000_000_000, "delay_ns": 1_500_000},
                    {"at_ns": 9_999_999_999, "delay_ns": 2_500_000},
                ],
            }],
        }
        self.config = {"rf_link_budgets": {"user": PARAMS}}

    def test_range_variation_and_midpoint_compatibility(self):
        original = copy.deepcopy(self.scenario)
        samples = generate_time_resolved_rf_budget_samples(self.scenario, self.config)
        midpoint = generate_rf_budget_samples(self.scenario, self.config)
        self.assertEqual(len(samples), 3)
        self.assertEqual(midpoint[0]["source_propagation_delay_ns"], 2_000_000)
        self.assertEqual([s["sample_ns"] for s in samples],
                         [0, 5_000_000_000, 9_999_999_999])
        self.assertEqual(samples[0]["direction"], "ue -> sat")
        self.assertAlmostEqual(
            samples[2]["free_space_path_loss_db"] - samples[1]["free_space_path_loss_db"],
            20 * math.log10(2.5 / 1.5), places=5,
        )
        self.assertGreater(samples[1]["cn_db"], samples[2]["cn_db"])
        self.assertTrue(all(s["sinr_db"] <= s["cn_db"] for s in samples))
        self.assertEqual(self.scenario, original)

    def test_missing_series_fails_closed(self):
        del self.scenario["contacts"][0]["propagation_delay_samples"]
        with self.assertRaisesRegex(ValueError, "missing geometric delay samples"):
            generate_time_resolved_rf_budget_samples(self.scenario, self.config)

    def test_invalid_series_rejected(self):
        original = self.scenario["contacts"][0]["propagation_delay_samples"]
        for bad in (
            [],
            [{"at_ns": 1, "delay_ns": 1}, original[-1]],
            [original[0], original[0], original[-1]],
            [original[0], {"at_ns": 10_000_000_000, "delay_ns": 1}],
            [original[0], {"at_ns": 9_999_999_999, "delay_ns": 0}],
            [original[0], {"at_ns": True, "delay_ns": 1}, original[-1]],
            [original[0], {"at_ns": 1, "delay_ns": False}, original[-1]],
            [original[0], {"at_ns": 1, "delay_ns": 1, "unknown": 2}, original[-1]],
            original[:-1],
        ):
            with self.subTest(bad=bad), self.assertRaises(ValueError):
                self.scenario["contacts"][0]["propagation_delay_samples"] = bad
                generate_time_resolved_rf_budget_samples(self.scenario, self.config)

    def test_one_nanosecond_contact(self):
        self.scenario["contacts"][0].update({
            "start_ns": 7, "end_ns": 8,
            "propagation_delay_samples": [{"at_ns": 7, "delay_ns": 2_000_000}],
        })
        self.assertEqual(
            generate_time_resolved_rf_budget_samples(self.scenario, self.config)[0]["sample_ns"],
            7,
        )

    def test_empty_contact_and_unconfigured_non_rf(self):
        self.scenario["contacts"] = []
        self.assertEqual(
            generate_time_resolved_rf_budget_samples(self.scenario, self.config), []
        )
        self.scenario["contacts"].append({
            "link_id": "land", "start_ns": 0, "end_ns": 10,
        })
        self.assertEqual(
            generate_time_resolved_rf_budget_samples(self.scenario, self.config), []
        )

    def test_cli_deterministic_provenance(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            scenario = root / "scenario.json"
            config = root / "config.json"
            out = root / "sampled.json"
            scenario.write_text(json.dumps(self.scenario), encoding="utf-8")
            config.write_text(json.dumps(self.config), encoding="utf-8")
            main([str(scenario), str(config), "--out", str(out)])
            first = out.read_bytes()
            main([str(scenario), str(config), "--out", str(out)])
            self.assertEqual(out.read_bytes(), first)
            document = json.loads(first)
            self.assertEqual(document["sample_count"], 3)
            self.assertEqual(document["compiled_scenario_sha256"],
                             hashlib.sha256(scenario.read_bytes()).hexdigest())
            self.assertEqual(document["rf_budget_config_sha256"],
                             hashlib.sha256(config.read_bytes()).hexdigest())


if __name__ == "__main__":
    unittest.main()
