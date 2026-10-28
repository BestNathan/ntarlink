import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from ntarlink_experiments.scenario import load_scenario


class ScenarioTests(unittest.TestCase):
    def test_minimal_scenario(self):
        root = Path(__file__).resolve().parents[2]
        scenario = load_scenario(root / "scenarios/minimal.json")
        self.assertEqual(scenario["name"], "minimal-leo-lab")
