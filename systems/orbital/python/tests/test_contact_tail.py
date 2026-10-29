"""Regression for the trailing partial coarse-sampling interval.

Without a final visibility sample, default (non-refined) contact generation
silently missed a transition in (last step, scenario end).
"""
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from ntarlink_orbital.contacts import generate_contact_windows


class TrailingIntervalTests(unittest.TestCase):
    def setUp(self):
        self.scenario = {
            "schema_version": 1,
            "duration_ns": 250,
            "nodes": [
                {"id": "ue", "kind": "terminal", "latitude_deg": 0, "longitude_deg": 0},
                {"id": "sat", "kind": "satellite"},
            ],
            "links": [{"id": "access", "kind": "user_rf", "from": "ue", "to": "sat"}],
        }
        self.config = {"step_ns": 100, "satellites": [{"id": "sat"}]}

    def test_default_coarse_sampling_detects_late_rise(self):
        def position(_satellite_id, t_ns):
            return (7e6, 0, 0) if t_ns >= 225 else (-7e6, 0, 0)

        windows = generate_contact_windows(self.scenario, self.config, position)
        self.assertEqual(
            [(w["start_ns"], w["end_ns"]) for w in windows],
            [(249, 250)],
        )

    def test_default_coarse_sampling_detects_late_set(self):
        def position(_satellite_id, t_ns):
            return (-7e6, 0, 0) if t_ns >= 225 else (7e6, 0, 0)

        self.config["propagation_delay_sample_step_ns"] = 25
        windows = generate_contact_windows(self.scenario, self.config, position)
        self.assertEqual(
            [(w["start_ns"], w["end_ns"]) for w in windows],
            [(0, 200)],
        )
        self.assertEqual(
            [s["at_ns"] for s in windows[0]["propagation_delay_samples"]],
            [0, 25, 50, 75, 100, 125, 150, 175, 199],
        )

    def test_coarse_set_within_full_step_does_not_admit_invisible_time(self):
        self.scenario["duration_ns"] = 350
        self.config["propagation_delay_sample_step_ns"] = 25

        def position(_satellite_id, t_ns):
            return (-7e6, 0, 0) if t_ns >= 125 else (7e6, 0, 0)

        windows = generate_contact_windows(self.scenario, self.config, position)
        self.assertEqual(
            [(w["start_ns"], w["end_ns"]) for w in windows],
            [(0, 100)],
        )
        self.assertEqual(
            windows[0]["propagation_delay_samples"][-1]["at_ns"], 99
        )

    def test_coarse_set_exactly_on_step_boundary_stays_conservative(self):
        def position(_satellite_id, t_ns):
            return (-7e6, 0, 0) if t_ns >= 200 else (7e6, 0, 0)

        windows = generate_contact_windows(self.scenario, self.config, position)
        self.assertEqual(
            [(w["start_ns"], w["end_ns"]) for w in windows],
            [(0, 100)],
        )

    def test_refined_sampling_keeps_original_precision(self):
        self.config["refine_tolerance_ns"] = 1

        def position(_satellite_id, t_ns):
            return (7e6, 0, 0) if t_ns >= 225 else (-7e6, 0, 0)

        windows = generate_contact_windows(self.scenario, self.config, position)
        self.assertEqual(
            [(w["start_ns"], w["end_ns"]) for w in windows],
            [(225, 250)],
        )


if __name__ == "__main__":
    unittest.main()
