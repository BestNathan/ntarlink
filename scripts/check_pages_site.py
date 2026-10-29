"""Offline checks for the self-contained ntarlink Pages showcase."""

from __future__ import annotations

import re
import subprocess
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
PAGE = ROOT / "applications" / "console" / "pages" / "index.html"


def main() -> None:
    html = PAGE.read_text(encoding="utf-8")
    required = [
        "MOCK DATA", "DEMO DATA", "SYNTHETIC",
        "CONSTELLATION NETWORK", "PACKET", "SYSTEM CATALOG",
        "ISL failure", "Gateway outage", "Traffic surge",
        "const S=", "const MODES=", "data-system",
        "Read full subsystem design",
    ]
    for marker in required:
        if marker not in html:
            raise AssertionError(f"Missing showcase feature: {marker}")

    slugs = re.findall(r'"(\d{2}-[a-z0-9-]+)"\]', html)
    slugs = list(dict.fromkeys(slugs))
    if len(slugs) != 16:
        raise AssertionError(f"Expected 16 subsystem specs, got {len(slugs)}: {slugs}")
    for slug in slugs:
        spec = ROOT / "docs" / "subsystems" / f"{slug}.md"
        if not spec.is_file():
            raise AssertionError(f"Broken subsystem spec link: {spec}")

    scripts = re.findall(r"<script[^>]*>(.*?)</script>", html, re.DOTALL)
    if len(scripts) != 1:
        raise AssertionError("Expected exactly one inline client script")
    with tempfile.TemporaryDirectory() as tmp:
        path = Path(tmp) / "showcase.js"
        path.write_text(scripts[0], encoding="utf-8")
        subprocess.run(["node", "--check", str(path)], check=True)

    print("Pages site smoke checks passed: 16 systems, working links and valid JavaScript.")


if __name__ == "__main__":
    main()
