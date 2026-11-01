"""Fail-closed preflight and bounded SYS-01 analytical Doppler sidecars."""

import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from ntarlink_orbital.contacts import generate_contact_kinematics


class DopplerPreflightTests(unittest.TestCase):
    def setUp(self):
        self.scenario = {
            "duration_ns": 10_000_000_000,
            "nodes": [
                {"id": "ue", "kind": "terminal",
                 "latitude_deg": 0, "longitude_deg": 0},
                {"id": "sat", "kind": "satellite"},
            ],
            "links": [
                {"id": "access", "kind": "user_rf", "from": "ue", "to": "sat"}
            ],
        }
        self.config = {
            "doppler_reference_frequencies_hz": {"access": 12_000_000_000},
        }
        self.window = {
            "link_id": "access", "start_ns": 0, "end_ns": 10_000_000_000
        }

    def fail_position(self, *_args):
        self.fail("Doppler preflight must precede all position-provider calls")

    def test_later_invalid_window_fails_before_first_position(self):
        invalid_windows = (
            {"link_id": "access", "start_ns": 5, "end_ns": 5},
            {"link_id": "access", "start_ns": -1, "end_ns": 100},
            {"link_id": "access", "start_ns": True, "end_ns": 100},
            {"link_id": "access", "start_ns": 0, "end_ns": 10_000_000_001},
        )
        for invalid in invalid_windows:
            with self.subTest(window=invalid), self.assertRaisesRegex(
                ValueError, "invalid Doppler contact interval"
            ):
                generate_contact_kinematics(
                    self.scenario, self.config, self.fail_position,
                    [self.window, invalid],
                )

    def test_later_invalid_link_gate_fails_before_first_position(self):
        self.scenario["links"].append({
            "id": "bad", "kind": "user_rf", "from": "ue", "to": "sat",
            "max_range_m": float("nan"),
        })
        self.config["doppler_reference_frequencies_hz"]["bad"] = 12_000_000_000
        with self.assertRaisesRegex(ValueError, "max_range_m"):
            generate_contact_kinematics(
                self.scenario, self.config, self.fail_position,
                [self.window, {"link_id": "bad", "start_ns": 0,
                               "end_ns": 10_000_000_000}],
            )

    def test_budget_rejects_excess_before_propagation(self):
        self.config["max_doppler_samples"] = 1
        with self.assertRaisesRegex(ValueError, "Doppler sample budget exceeded"):
            generate_contact_kinematics(
                self.scenario, self.config, self.fail_position,
                [self.window, self.window],
            )

        calls = []
        def position(_satellite, at_ns):
            calls.append(at_ns)
            return (6_378_137 + 550_000 + at_ns / 1_000_000, 0, 0)

        samples = generate_contact_kinematics(
            self.scenario, self.config, position, [self.window],
        )
        self.assertEqual(len(samples), 1)
        self.assertEqual(calls, [4_500_000_000, 5_500_000_000])
        self.assertAlmostEqual(samples[0]["range_rate_mps"], 1000.0, places=4)

    def test_invalid_budget_is_rejected_without_propagation(self):
        for invalid in (0, -1, True, 1.5, "10", None, 100_001):
            with self.subTest(value=invalid), self.assertRaisesRegex(
                ValueError, "max_doppler_samples"
            ):
                generate_contact_kinematics(
                    self.scenario,
                    {**self.config, "max_doppler_samples": invalid},
                    self.fail_position, [self.window],
                )

    def test_nonfinite_provider_range_fails_closed(self):
        with self.assertRaisesRegex(ValueError, "geometric ranges must be finite"):
            generate_contact_kinematics(
                self.scenario, self.config,
                lambda *_args: (float("nan"), 0, 0), [self.window],
            )


if __name__ == "__main__":
    unittest.main()
