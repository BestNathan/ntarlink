import json
from pathlib import Path


def load_scenario(path: str | Path) -> dict:
    data = json.loads(Path(path).read_text(encoding="utf-8"))
    if not isinstance(data, dict) or not data.get("name"):
        raise ValueError("scenario must be an object with a name")
    return data
