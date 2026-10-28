import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from ntarlink_orbital.geometry import elevation_deg, ground_ecef_m


class GeometryTests(unittest.TestCase):
    def test_overhead(self):
        x, y, z = ground_ecef_m(0.0, 0.0)
        self.assertAlmostEqual(x, 6378137.0)
        self.assertAlmostEqual(elevation_deg((x + 550000.0, y, z), 0.0, 0.0), 90.0)

    def test_opposite_side(self):
        self.assertLess(elevation_deg((-7000000.0, 0.0, 0.0), 0.0, 0.0), 0.0)
