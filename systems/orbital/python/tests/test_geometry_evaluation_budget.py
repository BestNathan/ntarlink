"""Bounded SYS-01 geometry evaluations across every contact prediction phase."""

import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from ntarlink_orbital.contacts import generate_contact_windows


class GeometryEvaluationBudgetTests(unittest.TestCase):
    def setUp(self):
        self.scenario = {
            "schema_version": 1,
            "duration_ns": 10,
            "nodes": [
                {"id": "ue", "kind": "terminal", "latitude_deg": 0, "longitude_deg": 0},
                {"id": "sat", "kind": "satellite"},
            ],
            "links": [
                {"id": "access", "kind": "user_rf", "from": "ue", "to": "sat"}
            ],
        }
        self.config = {"step_ns": 5, "satellites": [{"id": "sat"}]}
        self.probes = []

    def position(self, _satellite, time_ns):
        self.probes.append(time_ns)
        return (7_000_000, 0, 0)

    def test_final_partial_probe_and_midpoint_are_counted(self):
        self.config["max_geometry_evaluations"] = 3
        with self.assertRaisesRegex(ValueError, "geometry evaluation budget exceeded"):
            generate_contact_windows(self.scenario, self.config, self.position)
        self.assertEqual(self.probes, [0, 5, 9])

        self.probes.clear()
        self.config["max_geometry_evaluations"] = 4
        windows = generate_contact_windows(self.scenario, self.config, self.position)
        self.assertEqual(self.probes, [0, 5, 9, 5])
        self.assertEqual(
            [(window["start_ns"], window["end_ns"]) for window in windows],
            [(0, 10)],
        )

    def test_budget_is_shared_across_geometric_links(self):
        self.scenario["links"].append(
            {"id": "feeder", "kind": "feeder", "from": "sat", "to": "ue"}
        )
        self.config["max_geometry_evaluations"] = 7
        with self.assertRaisesRegex(ValueError, "7 probes"):
            generate_contact_windows(self.scenario, self.config, self.position)
        self.assertEqual(len(self.probes), 7)

        self.probes.clear()
        self.config["max_geometry_evaluations"] = 8
        windows = generate_contact_windows(self.scenario, self.config, self.position)
        self.assertEqual(len(self.probes), 8)
        self.assertEqual({window["link_id"] for window in windows}, {"access", "feeder"})

    def test_refinement_and_delay_samples_share_the_same_budget(self):
        self.scenario["duration_ns"] = 30
        self.config.update({
            "step_ns": 10,
            "refine_tolerance_ns": 1,
            "propagation_delay_sample_step_ns": 10,
        })

        def rising_position(_satellite, time_ns):
            self.probes.append(time_ns)
            return (7_000_000, 0, 0) if time_ns >= 5 else (-7_000_000, 0, 0)

        self.config["max_geometry_evaluations"] = 8
        with self.assertRaisesRegex(ValueError, "8 probes"):
            generate_contact_windows(self.scenario, self.config, rising_position)
        self.assertEqual(len(self.probes), 8)

        self.probes.clear()
        self.config["max_geometry_evaluations"] = 9
        with self.assertRaisesRegex(ValueError, "9 probes"):
            generate_contact_windows(self.scenario, self.config, rising_position)
        self.assertEqual(len(self.probes), 9)

        self.probes.clear()
        self.config["max_geometry_evaluations"] = 13
        windows = generate_contact_windows(self.scenario, self.config, rising_position)
        self.assertEqual(len(self.probes), 13)
        self.assertEqual(
            [(window["start_ns"], window["end_ns"]) for window in windows],
            [(5, 30)],
        )
        self.assertEqual(
            [sample["at_ns"] for sample in windows[0]["propagation_delay_samples"]],
            [5, 15, 25, 29],
        )

    def test_invalid_budget_fails_before_propagation(self):
        for invalid in (0, -1, True, 2.5, "10", None, 1_000_001):
            with self.subTest(value=invalid), self.assertRaisesRegex(
                ValueError, "max_geometry_evaluations"
            ):
                generate_contact_windows(
                    self.scenario,
                    {**self.config, "max_geometry_evaluations": invalid},
                    lambda *_args: self.fail("invalid budget reached position provider"),
                )


if __name__ == "__main__":
    unittest.main()
