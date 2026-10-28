import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from ntarlink_orbital.scenario import load_scenario


class ScenarioTests(unittest.TestCase):
    def test_minimal(self):
        root = Path(__file__).resolve().parents[4]
        data = load_scenario(root / "scenarios/minimal.json")
        self.assertEqual(data["schema_version"], 1)
        self.assertEqual(data["name"], "minimal-broadband-path")
        self.assertEqual(len(data["links"]), 5)
