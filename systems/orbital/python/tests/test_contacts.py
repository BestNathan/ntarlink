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

    def test_optical_occlusion_rejects_nonfinite_geometry_and_clearance(self):
        a, b = (7e6, 0, 0), (-7e6, 0, 0)
        self.assertTrue(earth_occludes_segment(a, b, 0.0))
        for invalid in (float("nan"), float("inf"), -1.0):
            with self.subTest(clearance=invalid), self.assertRaisesRegex(
                ValueError, "clearance_m"
            ):
                earth_occludes_segment(a, b, invalid)
        for invalid in (float("nan"), float("inf")):
            with self.subTest(coordinate=invalid), self.assertRaisesRegex(
                ValueError, "finite ECEF coordinates"
            ):
                earth_occludes_segment((invalid, 0, 0), b)

        nodes = {
            "a": {"id": "a", "kind": "satellite"},
            "b": {"id": "b", "kind": "satellite"},
        }
        link = {
            "id": "isl",
            "kind": "optical_isl",
            "from": "a",
            "to": "b",
            "max_range_m": 1.0,
        }
        position = lambda satellite, _time: a if satellite == "a" else b
        for invalid in (float("nan"), float("inf"), -1.0):
            with self.subTest(config_clearance=invalid), self.assertRaisesRegex(
                ValueError, "isl_clearance_m"
            ):
                link_geometry_at(
                    link, nodes, 0, position, {"isl_clearance_m": invalid}
                )
        # Valid configuration still uses the ordinary geometric range gate.
        self.assertEqual(
            link_geometry_at(link, nodes, 0, position, {"isl_clearance_m": 0.0}),
            (False, None),
        )

    def test_geometric_link_gate_limits_fail_closed(self):
        nodes = {
            "ue": {"id": "ue", "kind": "terminal", "latitude_deg": 0, "longitude_deg": 0},
            "a": {"id": "a", "kind": "satellite"},
            "b": {"id": "b", "kind": "satellite"},
        }
        position = lambda satellite, _time: (
            (7e6, 0, 0) if satellite == "a" else (7e6, 7e6, 0)
        )
        rf = {"id": "rf", "kind": "user_rf", "from": "ue", "to": "a"}
        optical = {"id": "isl", "kind": "optical_isl", "from": "a", "to": "b"}

        for invalid in (float("nan"), float("inf"), -1.0, 0.0):
            for link in (rf, optical):
                with self.subTest(kind=link["kind"], max_range=invalid):
                    with self.assertRaisesRegex(ValueError, "max_range_m"):
                        link_geometry_at(
                            {**link, "max_range_m": invalid},
                            nodes, 0, position, {},
                        )

        # Invalid elevation thresholds must not be masked by a range miss.
        for invalid in (float("nan"), float("inf"), -91.0, 91.0):
            with self.subTest(elevation=invalid):
                with self.assertRaisesRegex(ValueError, "min_elevation_deg"):
                    link_geometry_at(
                        {**rf, "max_range_m": 1.0},
                        nodes, 0, position,
                        {"min_elevation_deg": invalid},
                    )

        self.assertTrue(link_geometry_at(rf, nodes, 0, position, {})[0])
        self.assertFalse(link_geometry_at(
            {**rf, "max_range_m": 1.0}, nodes, 0, position, {}
        )[0])
        self.assertTrue(link_geometry_at(optical, nodes, 0, position, {})[0])
        self.assertFalse(link_geometry_at(
            {**optical, "max_range_m": 1.0}, nodes, 0, position, {}
        )[0])

    def test_invalid_geometric_topology_is_not_masked_by_range(self):
        nodes = {
            "ue": {"id": "ue", "kind": "terminal", "latitude_deg": 0, "longitude_deg": 0},
            "ground": {"id": "ground", "kind": "ground_station",
                       "latitude_deg": 0, "longitude_deg": 0},
            "a": {"id": "a", "kind": "satellite"},
            "b": {"id": "b", "kind": "satellite"},
        }
        invalid_links = [
            {"id": "bad-optical", "kind": "optical_isl", "from": "a", "to": "ue"},
            {"id": "bad-rf-space", "kind": "user_rf", "from": "a", "to": "b"},
            {"id": "bad-rf-ground", "kind": "feeder", "from": "ue", "to": "ground"},
            {"id": "missing", "kind": "user_rf", "from": "ue", "to": "unknown"},
        ]
        for link in invalid_links:
            with self.subTest(link=link["id"]), self.assertRaisesRegex(
                ValueError, "optical ISL|user/feeder RF|unknown geometric"
            ):
                link_geometry_at(
                    {**link, "max_range_m": 1.0}, nodes, 0,
                    lambda *_args: self.fail("invalid topology reached position provider"), {},
                )

        # The complete scenario must be preflighted, not just each link when
        # its sampling loop begins: a later invalid link must prevent all work.
        scenario = {
            "schema_version": 1,
            "duration_ns": 100,
            "nodes": list(nodes.values()),
            "links": [
                {"id": "valid", "kind": "user_rf", "from": "ue", "to": "a"},
                {**invalid_links[0], "max_range_m": 1.0},
            ],
        }
        with self.assertRaisesRegex(ValueError, "optical ISL"):
            generate_contact_windows(
                scenario, {"step_ns": 10, "satellites": [{"id": "a"}, {"id": "b"}]},
                lambda *_args: self.fail("preflight must precede every geometry probe"),
            )

        # Reversed RF endpoints remain valid; only topology, not direction,
        # is constrained by this guard.
        self.assertTrue(link_geometry_at(
            {"id": "feeder", "kind": "feeder", "from": "a", "to": "ground"},
            nodes, 0, lambda *_args: (7e6, 0, 0), {},
        )[0])

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

    def test_sampling_budget_counts_all_geometric_links(self):
        scenario = {
            "schema_version": 1,
            "duration_ns": 500,
            "nodes": [
                {"id": "ue", "kind": "terminal", "latitude_deg": 0, "longitude_deg": 0},
                {"id": "sat", "kind": "satellite"},
            ],
            "links": [
                {"id": f"access-{index}", "kind": "user_rf", "from": "ue", "to": "sat"}
                for index in range(1001)
            ],
        }
        config = {"step_ns": 1, "satellites": [{"id": "sat"}]}
        def unexpected_position(*_args):
            self.fail("the global budget must be checked before propagation")

        with self.assertRaisesRegex(ValueError, "500,000 link-steps"):
            generate_contact_windows(scenario, config, unexpected_position)

        # Non-geometric links do not consume orbital geometry probes.
        scenario["links"] = [
            {"id": "access", "kind": "user_rf", "from": "ue", "to": "sat"},
            {"id": "terrestrial", "kind": "terrestrial", "from": "ue", "to": "ue"},
        ]
        windows = generate_contact_windows(
            scenario, {"step_ns": 500, "satellites": [{"id": "sat"}]},
            lambda *_args: (7e6, 0, 0),
        )
        self.assertEqual(
            [(window["link_id"], window["start_ns"], window["end_ns"])
             for window in windows],
            [("access", 0, 500)],
        )

    def test_contact_duration_requires_integer_nanoseconds(self):
        scenario = {
            "schema_version": 1,
            "duration_ns": 100,
            "nodes": [],
            "links": [],
        }
        config = {"step_ns": 10, "satellites": []}
        for invalid in (True, 100.5, "100"):
            with self.subTest(duration=invalid), self.assertRaisesRegex(
                ValueError, "duration_ns must be an integer"
            ):
                generate_contact_windows(
                    {**scenario, "duration_ns": invalid},
                    config, lambda *_args: self.fail("unexpected propagation"),
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
