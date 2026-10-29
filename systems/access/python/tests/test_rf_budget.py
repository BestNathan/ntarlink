"""Independent Friis/thermal-noise references and cross-subsystem contact tests."""
import math
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from ntarlink_access.rf_budget import calculate_rf_budget, generate_rf_budget_samples


PARAMS = {
    "carrier_hz": 1e9,
    "bandwidth_hz": 1e6,
    "tx_power_dbm": 30,
    "tx_gain_dbi": 0,
    "rx_gain_dbi": 0,
    "noise_figure_db": 0,
    "other_losses_db": 0,
    "required_cn_db": 10,
}


class AnalyticalRfBudgetTests(unittest.TestCase):
    def test_friis_and_thermal_noise_reference(self):
        budget = calculate_rf_budget(range_m=1000, **PARAMS)
        # Friis at 1 km/1 GHz: 92.44778 dB; kTB at 290 K/1 MHz:
        # approximately -113.975 dBm, before receiver noise figure.
        self.assertAlmostEqual(budget["free_space_path_loss_db"], 92.447783, places=5)
        self.assertAlmostEqual(budget["thermal_noise_dbm"], -113.975188, places=4)
        self.assertAlmostEqual(
            budget["cn0_db_hz"] - 10 * math.log10(PARAMS["bandwidth_hz"]),
            budget["cn_db"], places=5
        )
        self.assertAlmostEqual(
            budget["link_margin_db"], budget["cn_db"] - PARAMS["required_cn_db"],
            places=5
        )
        self.assertTrue(budget["meets_reference_cn"])

    def test_distance_and_bandwidth_scaling(self):
        near = calculate_rf_budget(range_m=1000, **PARAMS)
        far = calculate_rf_budget(range_m=2000, **PARAMS)
        wide = calculate_rf_budget(range_m=1000, **{**PARAMS, "bandwidth_hz": 2e6})
        self.assertAlmostEqual(
            far["free_space_path_loss_db"] - near["free_space_path_loss_db"],
            20 * math.log10(2), places=5
        )
        self.assertAlmostEqual(
            near["cn_db"] - wide["cn_db"], 10 * math.log10(2), places=5
        )

    def test_reference_threshold_is_not_link_admission(self):
        poor = calculate_rf_budget(
            range_m=1e8, **{**PARAMS, "required_cn_db": 30}
        )
        self.assertFalse(poor["meets_reference_cn"])
        self.assertLess(poor["link_margin_db"], 0)

    def test_equal_thermal_noise_and_interference_degrades_sinr_by_three_db(self):
        clean = calculate_rf_budget(range_m=1000, **PARAMS)
        noisy = calculate_rf_budget(
            range_m=1000, **PARAMS,
            interference_power_dbm=clean["thermal_noise_dbm"],
            required_sinr_db=clean["cn_db"] - 2,
        )
        self.assertAlmostEqual(clean["cn_db"], noisy["cn_db"], places=6)
        self.assertAlmostEqual(
            clean["cn_db"] - noisy["sinr_db"], 10 * math.log10(2), places=5
        )
        self.assertAlmostEqual(
            noisy["noise_plus_interference_dbm"] - noisy["thermal_noise_dbm"],
            10 * math.log10(2), places=5
        )
        self.assertFalse(noisy["meets_reference_sinr"])
        self.assertTrue(noisy["meets_reference_cn"])

    def test_absent_or_negligible_interference_preserves_cn(self):
        clean = calculate_rf_budget(range_m=1000, **PARAMS)
        weak = calculate_rf_budget(
            range_m=1000, **PARAMS, interference_power_dbm=-300,
            required_sinr_db=10,
        )
        self.assertIsNone(clean["interference_power_dbm"])
        self.assertIsNone(clean["meets_reference_sinr"])
        self.assertEqual(clean["sinr_db"], clean["cn_db"])
        self.assertAlmostEqual(weak["sinr_db"], clean["cn_db"], places=5)
        self.assertTrue(weak["meets_reference_sinr"])

    def test_invalid_interference_and_sinr_threshold(self):
        for key, value in (
            ("interference_power_dbm", float("nan")),
            ("interference_power_dbm", True),
            ("interference_power_dbm", float("inf")),
            ("required_sinr_db", False),
            ("required_sinr_db", float("-inf")),
        ):
            with self.subTest(key=key, value=value), self.assertRaises(ValueError):
                calculate_rf_budget(range_m=1000, **PARAMS, **{key: value})

    def test_invalid_physical_inputs(self):
        for field, value in (
            ("range_m", 0), ("carrier_hz", 0), ("bandwidth_hz", -1),
            ("noise_figure_db", -1), ("other_losses_db", -1),
            ("tx_gain_dbi", float("nan")), ("tx_power_dbm", float("inf")),
            ("required_cn_db", True),
        ):
            with self.subTest(field=field), self.assertRaises(ValueError):
                calculate_rf_budget(**({**PARAMS, "range_m": 1000, field: value}))


class ContactRfIntegrationTests(unittest.TestCase):
    def setUp(self):
        self.scenario = {
            "schema_version": 1, "contact_mode": "scheduled",
            "duration_ns": 10_000_000_000,
            "nodes": [
                {"id": "ue", "kind": "terminal", "latitude_deg": 0, "longitude_deg": 0},
                {"id": "sat", "kind": "satellite"},
            ],
            "links": [
                {"id": "access", "kind": "user_rf", "from": "ue", "to": "sat"},
                {"id": "backhaul", "kind": "backhaul", "from": "x", "to": "y"},
            ],
            "contacts": [
                {"link_id": "access", "start_ns": 0, "end_ns": 10_000_000_000, "propagation_delay_ns": 2_000_000}
            ],
        }
        self.config = {"rf_link_budgets": {"access": PARAMS}}

    def test_directed_contact_midpoint_and_range(self):
        samples = generate_rf_budget_samples(
            self.scenario, self.config
        )
        self.assertEqual(len(samples), 1)
        self.assertEqual(samples[0]["sample_ns"], 5_000_000_000)
        self.assertEqual(samples[0]["direction"], "ue -> sat")
        self.assertAlmostEqual(samples[0]["range_m"], 599_584.916)
        self.assertGreater(samples[0]["free_space_path_loss_db"], 100)
        self.assertEqual(samples[0]["source_propagation_delay_ns"], 2_000_000)

    def test_configured_interference_is_observational_only(self):
        self.config["rf_link_budgets"]["access"] = {
            **PARAMS, "interference_power_dbm": -100,
            "required_sinr_db": 15,
        }
        before = self.scenario["contacts"][0].copy()
        sample = generate_rf_budget_samples(self.scenario, self.config)[0]
        self.assertEqual(sample["interference_power_dbm"], -100)
        self.assertLess(sample["sinr_db"], sample["cn_db"])
        self.assertEqual(sample["required_sinr_db"], 15)
        self.assertEqual(self.scenario["contacts"][0], before)

    def test_non_rf_link_and_incomplete_budget_rejected(self):
        for bad in (
            {"backhaul": PARAMS},
            {"missing": PARAMS},
            {"access": {key: value for key, value in PARAMS.items()
                        if key != "bandwidth_hz"}},
            {"access": {**PARAMS, "unknown": 1}},
        ):
            with self.subTest(bad=bad), self.assertRaises(ValueError):
                generate_rf_budget_samples(
                    self.scenario, {"rf_link_budgets": bad}
                )

    def test_no_contact_means_no_rf_sample(self):
        self.scenario["contacts"] = []
        self.assertEqual(
            generate_rf_budget_samples(self.scenario, self.config),
            []
        )

    def test_missing_midpoint_delay_fails_closed(self):
        del self.scenario["contacts"][0]["propagation_delay_ns"]
        with self.assertRaisesRegex(ValueError, "midpoint delay"):
            generate_rf_budget_samples(self.scenario, self.config)

    def test_requires_scheduled_geometry(self):
        self.scenario["contact_mode"] = "static"
        with self.assertRaises(ValueError):
            generate_rf_budget_samples(self.scenario, self.config)


if __name__ == "__main__":
    unittest.main()
