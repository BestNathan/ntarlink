import math
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from ntarlink_orbital.contacts import ContactWindow, predict_ground_contacts
from ntarlink_orbital.geometry import A


class ContactPredictionTests(unittest.TestCase):
    def test_overhead_pass_has_refined_half_open_boundaries(self):
        # A synthetic ECEF trajectory, not an orbit propagation reference.
        # Elevation is above 30 degrees while |y| < 550km / tan(30deg).
        trajectory = lambda t_ns: (A + 550_000, (t_ns - 1000) * 2000.0, 0.0)
        windows = predict_ground_contacts(
            trajectory,
            start_ns=0,
            end_ns=2000,
            latitude_deg=0.0,
            longitude_deg=0.0,
            min_elevation_deg=30.0,
            sample_step_ns=100,
            refine_tolerance_ns=1,
        )
        self.assertEqual(len(windows), 1)
        exact_enter = 1000 - 550_000 / math.tan(math.radians(30)) / 2000
        exact_leave = 1000 + 550_000 / math.tan(math.radians(30)) / 2000
        self.assertGreaterEqual(windows[0].start_ns, exact_enter)
        self.assertLessEqual(windows[0].end_ns, exact_leave)
        self.assertLessEqual(windows[0].start_ns - exact_enter, 1.0)
        self.assertLessEqual(exact_leave - windows[0].end_ns, 1.0)

    def test_whole_range_visible(self):
        windows = predict_ground_contacts(
            lambda _: (A + 550_000, 0.0, 0.0),
            start_ns=5,
            end_ns=100,
            latitude_deg=0,
            longitude_deg=0,
        )
        self.assertEqual(windows, [ContactWindow(5, 100)])

    def test_whole_range_blocked(self):
        windows = predict_ground_contacts(
            lambda _: (A + 550_000, 5_000_000.0, 0.0),
            start_ns=0,
            end_ns=100,
            latitude_deg=0,
            longitude_deg=0,
            min_elevation_deg=30,
        )
        self.assertEqual(windows, [])

    def test_invalid_inputs_fail_closed(self):
        base = dict(start_ns=0, end_ns=100, latitude_deg=0, longitude_deg=0)
        for additional in (
            {"sample_step_ns": 0},
            {"refine_tolerance_ns": 0},
            {"end_ns": 0},
            {"latitude_deg": float("nan")},
            {"min_elevation_deg": 91},
            {"start_ns": -1},
        ):
            with self.subTest(additional=additional), self.assertRaises(ValueError):
                predict_ground_contacts(
                    lambda _: (A + 550_000, 0, 0),
                    **(base | additional),
                )
        with self.assertRaises(ValueError):
            predict_ground_contacts(lambda _: (float("nan"), 0, 0), **base)
        with self.assertRaises(ValueError):
            predict_ground_contacts(lambda _: (A, 0, 0), **base)


if __name__ == "__main__":
    unittest.main()
