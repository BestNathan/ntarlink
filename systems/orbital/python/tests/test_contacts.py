"""Geometry/contact reference regression tests for Skyfield/SGP4 integration."""

import json
import math
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from ntarlink_orbital.contacts import (
    build_sgp4_provider,
    generate_contact_windows,
    link_geometry_at,
    parse_utc,
)
from ntarlink_orbital.geometry import (
    distance_m,
    earth_occludes_segment,
    geometric_delay_ns,
    ground_ecef_m,
)


class OrbitalContactTests(unittest.TestCase):
    def test_earth_occlusion_and_delay_units(self):
        self.assertTrue(earth_occludes_segment((7e6, 0, 0), (-7e6, 0, 0)))
        self.assertTrue(earth_occludes_segment((7e6, 0, 0), (0, 7e6, 0)))
        self.assertFalse(earth_occludes_segment((7e6, 0, 0), (7e6, 7e6, 0)))
        self.assertAlmostEqual(
            geometric_delay_ns(299_792_458.0), 1_000_000_000, delta=1
        )
        with self.assertRaises(ValueError):
            geometric_delay_ns(0)

    def test_sampled_window_from_synthetic_earth_fixed_orbit(self):
        scenario = {
            "schema_version": 1,
            "duration_ns": 100_000_000_000,
            "nodes": [
                {"id": "ue", "kind": "terminal", "latitude_deg": 0, "longitude_deg": 0},
                {"id": "sat", "kind": "satellite"},
            ],
            "links": [{"id": "access", "kind": "user_rf", "from": "ue", "to": "sat"}],
        }
        config = {
            "step_ns": 10_000_000_000,
            "min_elevation_deg": 10,
            "satellites": [{"id": "sat"}],
        }

        def position(_satellite_id, time_ns):
            return ((6_378_137 + 550_000, 0, 0) if time_ns < 40_000_000_000
                    else (-7_000_000, 0, 0))

        windows = generate_contact_windows(scenario, config, position)
        self.assertEqual(len(windows), 1)
        self.assertEqual(windows[0]["link_id"], "access")
        self.assertEqual(windows[0]["start_ns"], 0)
        self.assertEqual(windows[0]["end_ns"], 30_000_000_000)
        self.assertAlmostEqual(windows[0]["propagation_delay_ns"], 1_834_600, delta=5_000)

    def test_refined_ground_pass_edges_are_conservative(self):
        # Synthetic ECEF pass with analytically known 30-degree crossings.
        # The motion is deliberately not a physical SGP4 trajectory.
        scenario = {
            "schema_version": 1,
            "duration_ns": 2000,
            "nodes": [
                {"id": "ue", "kind": "terminal", "latitude_deg": 0, "longitude_deg": 0},
                {"id": "sat", "kind": "satellite"},
            ],
            "links": [{"id": "access", "kind": "user_rf", "from": "ue", "to": "sat"}],
        }
        config = {
            "step_ns": 100,
            "refine_tolerance_ns": 1,
            "min_elevation_deg": 30,
            "satellites": [{"id": "sat"}],
        }
        position = lambda _sat, t: (6_378_137 + 550_000, (t - 1000) * 2000.0, 0)
        windows = generate_contact_windows(scenario, config, position)
        self.assertEqual(len(windows), 1)
        window = windows[0]
        exact_half_width = 550_000 / math.tan(math.radians(30)) / 2000
        exact_rise = 1000 - exact_half_width
        exact_set = 1000 + exact_half_width
        self.assertGreaterEqual(window["start_ns"], exact_rise)
        self.assertLessEqual(window["end_ns"], exact_set)
        self.assertLessEqual(window["start_ns"] - exact_rise, 1)
        self.assertLessEqual(exact_set - window["end_ns"], 1)
        self.assertLess(window["start_ns"], window["end_ns"])
        self.assertGreater(window["propagation_delay_ns"], 0)

    def test_refined_contact_in_final_partial_interval(self):
        scenario = {
            "schema_version": 1,
            "duration_ns": 250,
            "nodes": [
                {"id": "ue", "kind": "terminal", "latitude_deg": 0, "longitude_deg": 0},
                {"id": "sat", "kind": "satellite"},
            ],
            "links": [{"id": "access", "kind": "user_rf", "from": "ue", "to": "sat"}],
        }
        config = {
            "step_ns": 100,
            "refine_tolerance_ns": 1,
            "satellites": [{"id": "sat"}],
        }
        position = lambda _sat, t: ((7e6, 0, 0) if t >= 225 else (-7e6, 0, 0))
        windows = generate_contact_windows(scenario, config, position)
        self.assertEqual(
            [(w["start_ns"], w["end_ns"]) for w in windows],
            [(225, 250)],
        )

    def test_refinement_configuration_is_validated(self):
        scenario = {
            "schema_version": 1,
            "duration_ns": 100,
            "nodes": [
                {"id": "ue", "kind": "terminal", "latitude_deg": 0, "longitude_deg": 0},
                {"id": "sat", "kind": "satellite"},
            ],
            "links": [{"id": "access", "kind": "user_rf", "from": "ue", "to": "sat"}],
        }
        for invalid in (0, -1, 101, 0.5, True):
            config = {
                "step_ns": 100,
                "refine_tolerance_ns": invalid,
                "satellites": [{"id": "sat"}],
            }
            with self.subTest(invalid=invalid), self.assertRaises(ValueError):
                generate_contact_windows(scenario, config, lambda *_: (7e6, 0, 0))

    def test_iss_historical_reference_pass_from_skyfield_docs(self):
        root = Path(__file__).resolve().parents[4]
        scenario = json.loads((root / "scenarios/orbital-pass.json").read_text())
        config = json.loads((root / "scenarios/orbit-sources.sample.json").read_text())
        position = build_sgp4_provider(config, scenario["duration_ns"])
        state = position("iss-demo", 0)
        self.assertTrue(all(math.isfinite(x) for x in state))
        self.assertTrue(6_500_000 < distance_m(state, (0, 0, 0)) < 7_200_000)
        contacts = generate_contact_windows(scenario, config, position)
        access = [x for x in contacts if x["link_id"] == "user-iss"]
        feeder = [x for x in contacts if x["link_id"] == "feeder-iss"]
        self.assertGreaterEqual(len(access), 1)
        self.assertEqual(
            [(x["start_ns"], x["end_ns"]) for x in access],
            [(x["start_ns"], x["end_ns"]) for x in feeder],
        )
        first = access[0]
        # Reference guide predicts ~06:25:37 to ~06:28:19 UTC.
        self.assertTrue(300_000_000_000 <= first["start_ns"] <= 370_000_000_000)
        self.assertTrue(470_000_000_000 <= first["end_ns"] <= 540_000_000_000)
        self.assertGreater(first["propagation_delay_ns"], 0)

    def test_stale_tle_is_rejected(self):
        root = Path(__file__).resolve().parents[4]
        config = json.loads((root / "scenarios/orbit-sources.sample.json").read_text())
        config["epoch_utc"] = "2026-10-29T00:00:00Z"
        with self.assertRaisesRegex(ValueError, "stale TLE"):
            build_sgp4_provider(config, 1_000_000_000)

    def test_link_geometry_excludes_opposite_side_and_elevation(self):
        nodes = {
            "ue": {"id": "ue", "kind": "terminal", "latitude_deg": 0, "longitude_deg": 0},
            "sat": {"id": "sat", "kind": "satellite"},
        }
        link = {"id": "rf", "kind": "user_rf", "from": "ue", "to": "sat"}
        below = lambda _, __: (-7e6, 0, 0)
        self.assertFalse(link_geometry_at(
            link, nodes, 0, below, {"min_elevation_deg": 10}
        )[0])
        above = lambda _, __: (7e6, 0, 0)
        self.assertTrue(link_geometry_at(
            link, nodes, 0, above, {"min_elevation_deg": 10}
        )[0])

    def test_epoch_must_be_utc(self):
        self.assertEqual(parse_utc("2014-01-23T06:20:00Z").year, 2014)
        with self.assertRaises(ValueError):
            parse_utc("2014-01-23T06:20:00+08:00")


if __name__ == "__main__":
    unittest.main()
