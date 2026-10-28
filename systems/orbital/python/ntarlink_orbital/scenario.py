"""Read versioned deterministic satellite network scenarios."""
import json
from pathlib import Path


def load_scenario(path: str | Path) -> dict:
    data = json.loads(Path(path).read_text(encoding="utf-8"))
    if not isinstance(data, dict) or data.get("schema_version") != 1:
        raise ValueError("scenario must be a version 1 object")
    if not data.get("name") or not data.get("nodes") or not data.get("links"):
        raise ValueError("scenario requires name, nodes and links")
    return data
