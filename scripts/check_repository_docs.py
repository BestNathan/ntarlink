"""Reject Han-language repository prose and broken local Markdown links.

This checks tracked text files only and intentionally allows non-ASCII symbols
used for mathematics and legitimate identifiers. External URLs are not fetched.
"""
from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path
from urllib.parse import unquote, urlsplit

ROOT = Path(__file__).resolve().parents[1]
TEXT_SUFFIXES = {".md", ".rs", ".py", ".ts", ".tsx", ".js", ".json", ".toml", ".yaml", ".yml", ".sh", ".html", ".css", ".txt"}
HAN = re.compile(r"[\u3400-\u9fff\uf900-\ufaff]")
MARKDOWN_LINK = re.compile(r"(?<!!)\[[^\]]+\]\(([^)]+)\)")


def check() -> list[str]:
    problems = []
    paths = subprocess.check_output(["git", "ls-files", "-z"], cwd=ROOT).split(b"\0")
    for raw_path in paths:
        if not raw_path:
            continue
        relative = raw_path.decode("utf-8")
        path = ROOT / relative
        if path.suffix.lower() not in TEXT_SUFFIXES or not path.is_file():
            continue
        content = path.read_text(encoding="utf-8")
        for match in HAN.finditer(content):
            line = content.count("\n", 0, match.start()) + 1
            problems.append(f"{relative}:{line}: non-English Han text in tracked source")
        if path.suffix.lower() != ".md":
            continue
        for match in MARKDOWN_LINK.finditer(content):
            target = match.group(1).strip().split(" ", 1)[0].strip("<>")
            parts = urlsplit(target)
            if parts.scheme or parts.netloc or target.startswith(("#", "mailto:", "data:")):
                continue
            local = unquote(parts.path)
            if not local or (path.parent / local).exists():
                continue
            line = content.count("\n", 0, match.start()) + 1
            problems.append(f"{relative}:{line}: broken relative Markdown link: {target}")
    return problems


if __name__ == "__main__":
    failures = check()
    for failure in failures:
        print(failure, file=sys.stderr)
    if failures:
        print(f"{len(failures)} documentation issues", file=sys.stderr)
        raise SystemExit(1)
    print("Repository English-only source and local Markdown link checks passed.")
