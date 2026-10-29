"""Reference analytical Doppler and contact sample regression tests."""

import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from ntarlink_orbital.kinematics import (
    estimate_range_rate_mps,
    first_order_doppler_shift_hz,
)


class KinematicsTests(unittest.TestCase):
    def test_linear_range_rate_and_doppler_sign(self):
        def range_at_ns(t_ns):
            return 550_000.0 + 1_000.0 * t_ns / 1_000_000_000

        rate = estimate_range_rate_mps(range_at_ns, 5_000_000_000, 500_000_000)
        self.assertAlmostEqual(rate, 1_000.0, places=6)
        self.assertAlmostEqual(
            first_order_doppler_shift_hz(rate, 12_000_000_000),
            -40_027.6914237785,
            places=4,
        )
        self.assertGreater(first_order_doppler_shift_hz(-rate, 12_000_000_000), 0)
        self.assertEqual(first_order_doppler_shift_hz(0, 12_000_000_000), 0)

    def test_invalid_inputs_fail_closed(self):
        for sample, span in ((0, 1), (1, 0), (-1, 1), (True, 1), (2, 1.5)):
            with self.subTest(sample=sample, span=span), self.assertRaises(ValueError):
                estimate_range_rate_mps(lambda _: 1000, sample, span)
        for range_fn in (lambda _: float("nan"), lambda _: 0.0):
            with self.assertRaises(ValueError):
                estimate_range_rate_mps(range_fn, 10, 1)
        for frequency in (0, -1, float("nan"), float("inf"), True):
            with self.subTest(frequency=frequency), self.assertRaises(ValueError):
                first_order_doppler_shift_hz(1000, frequency)
        with self.assertRaises(ValueError):
            first_order_doppler_shift_hz(float("inf"), 12e9)
        with self.assertRaises(ValueError):
            first_order_doppler_shift_hz(299_792_458.0, 12e9)


class ContactKinematicsIntegrationTests(unittest.TestCase):
    def setUp(self):
        self.scenario = {
            "nodes": [
                {"id": "ue", "kind": "terminal", "latitude_deg": 0, "longitude_deg": 0},
                {"id": "sat", "kind": "satellite"},
            ],
            "links": [{"id": "access", "kind": "user_rf", "from": "ue", "to": "sat"}],
        }
        self.windows = [{"link_id": "access", "start_ns": 0, "end_ns": 10_000_000_000}]
        self.config = {"doppler_reference_frequencies_hz": {"access": 12_000_000_000}}
        self.position = lambda _, t: (6_378_137 + 550_000 + t / 1_000_000, 0, 0)

    def test_generated_contact_observation(self):
        from ntarlink_orbital.contacts import generate_contact_kinematics

        samples = generate_contact_kinematics(
            self.scenario, self.config, self.position, self.windows
        )
        self.assertEqual(len(samples), 1)
        self.assertEqual(samples[0]["link_id"], "access")
        self.assertEqual(samples[0]["sample_ns"], 5_000_000_000)
        self.assertEqual(samples[0]["sample_half_span_ns"], 500_000_000)
        self.assertAlmostEqual(samples[0]["range_rate_mps"], 1000.0, places=4)
        self.assertAlmostEqual(samples[0]["doppler_shift_hz"], -40_027.691424, places=4)

    def test_invalid_reference_frequency_and_short_contact(self):
        from ntarlink_orbital.contacts import generate_contact_kinematics

        for bad in (0, -1, True, "12000000000", float("nan")):
            with self.subTest(bad=bad), self.assertRaises(ValueError):
                generate_contact_kinematics(
                    self.scenario,
                    {"doppler_reference_frequencies_hz": {"access": bad}},
                    self.position,
                    self.windows,
                )
        with self.assertRaises(ValueError):
            generate_contact_kinematics(
                self.scenario,
                {"doppler_reference_frequencies_hz": {"missing": 12e9}},
                self.position,
                self.windows,
            )
        with self.assertRaises(ValueError):
            generate_contact_kinematics(
                self.scenario,
                self.config,
                self.position,
                [{"link_id": "access", "start_ns": 0, "end_ns": 2}],
            )
        with self.assertRaises(ValueError):
            generate_contact_kinematics(
                self.scenario,
                {**self.config, "doppler_half_span_ns": 0},
                self.position,
                self.windows,
            )


if __name__ == "__main__":
    unittest.main()
