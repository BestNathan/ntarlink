"""SYS-01 sampled geometric delay contract tests (synthetic ECEF trajectories)."""

import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from ntarlink_orbital.contacts import generate_contact_windows
from ntarlink_orbital.geometry import geometric_delay_ns


class PropagationDelaySamplesTests(unittest.TestCase):
    def setUp(self):
        self.scenario = {
            "schema_version": 1,
            "duration_ns": 200,
            "nodes": [
                {"id": "ue", "kind": "terminal", "latitude_deg": 0, "longitude_deg": 0},
                {"id": "sat", "kind": "satellite"},
            ],
            "links": [
                {"id": "user", "kind": "user_rf", "from": "ue", "to": "sat"}
            ],
        }
        self.config = {
            "step_ns": 100,
            "satellites": [{"id": "sat"}],
            "propagation_delay_sample_step_ns": 60,
        }

    @staticmethod
    def radial_position(_satellite_id, t_ns):
        # Artificial radial trajectory for exact geometric checks, not an orbit.
        return (6_378_137 + 550_000 + 1000 * t_ns, 0.0, 0.0)

    def test_samples_span_half_open_window_and_track_range(self):
        windows = generate_contact_windows(
            self.scenario, self.config, self.radial_position
        )
        self.assertEqual(len(windows), 1)
        window = windows[0]
        samples = window["propagation_delay_samples"]
        self.assertEqual([s["at_ns"] for s in samples], [0, 60, 120, 180, 199])
        self.assertEqual(
            [s["delay_ns"] for s in samples],
            [geometric_delay_ns(550_000 + 1000 * t) for t in (0, 60, 120, 180, 199)],
        )
        self.assertTrue(all(
            a["delay_ns"] < b["delay_ns"] for a, b in zip(samples, samples[1:])
        ))
        self.assertEqual(
            window["propagation_delay_ns"], geometric_delay_ns(550_000 + 1000 * 100)
        )

    def test_disabled_samples_preserve_legacy_midpoint_contract(self):
        self.config.pop("propagation_delay_sample_step_ns")
        window = generate_contact_windows(
            self.scenario, self.config, self.radial_position
        )[0]
        self.assertNotIn("propagation_delay_samples", window)

    def test_rejects_invalid_sampling_step_and_oversized_artifact(self):
        for value in (0, -1, True, 1.5, "5"):
            self.config["propagation_delay_sample_step_ns"] = value
            with self.subTest(value=value), self.assertRaisesRegex(
                ValueError, "positive integer"
            ):
                generate_contact_windows(
                    self.scenario, self.config, self.radial_position
                )
        self.scenario["duration_ns"] = 1_000_000
        self.config["step_ns"] = 1_000_000
        self.config["propagation_delay_sample_step_ns"] = 1
        with self.assertRaisesRegex(ValueError, "sample budget exceeded"):
            generate_contact_windows(
                self.scenario, self.config, self.radial_position
            )


if __name__ == "__main__":
    unittest.main()
