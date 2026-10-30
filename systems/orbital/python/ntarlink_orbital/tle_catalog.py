"""Offline checksum-validated TLE catalog intake with operator-supplied provenance.

This parser does not fetch data, certify its publisher/license, or authenticate
an orbit. It emits the existing SYS-01 orbit configuration for SGP4 consumers.
"""

from __future__ import annotations

import argparse
import calendar
import hashlib
import json
import math
import re
from datetime import datetime, timedelta, timezone
from decimal import Decimal
from pathlib import Path
from urllib.parse import urlsplit

MAX_CATALOG_BYTES = 1_000_000
MAX_SATELLITES = 1_000
EPOCH_PATTERN = re.compile(r"^[0-9]{5}\\.[0-9]{8}$")
UTC_PATTERN = re.compile(r"^\\d{4}-\\d{2}-\\d{2}T\\d{2}:\\d{2}:\\d{2}Z$")


def _utc(value: str, name: str) -> datetime:
    if not isinstance(value, str) or not UTC_PATTERN.fullmatch(value):
        raise ValueError(f"{name} must be a second-resolution ISO-8601 UTC timestamp")
    try:
        return datetime.fromisoformat(value.replace("Z", "+00:00"))
    except ValueError as error:
        raise ValueError(f"invalid {name}") from error


def _tle_epoch(line1: str) -> datetime:
    field = line1[18:32]
    if not EPOCH_PATTERN.fullmatch(field):
        raise ValueError("invalid TLE epoch field")
    short_year = int(field[:2])
    year = 2000 + short_year if short_year < 57 else 1900 + short_year
    day = int(field[2:5])
    if day < 1 or day > (366 if calendar.isleap(year) else 365):
        raise ValueError("TLE epoch day is outside its calendar year")
    micros = int(Decimal("0." + field[6:]) * Decimal(86_400_000_000))
    return datetime(year, 1, 1, tzinfo=timezone.utc) + timedelta(
        days=day - 1, microseconds=micros
    )


def _validate_tle(line: str, prefix: str) -> str:
    if (len(line) != 69 or not line.startswith(prefix + " ")
            or any(ord(char) < 32 or ord(char) > 126 for char in line)
            or not line[2:7].isdigit() or not line[68].isdigit()):
        raise ValueError(f"malformed TLE line {prefix}")
    checksum = sum(
        int(char) if char.isdigit() else 1 if char == "-" else 0
        for char in line[:68]
    ) % 10
    if checksum != int(line[68]):
        raise ValueError(f"TLE line {prefix} checksum mismatch")
    return line[2:7]


def import_catalog(
    raw: bytes, *, epoch_utc: str, source_url: str, source_license: str,
    retrieved_utc: str, max_tle_age_days: float = 14.0,
) -> tuple[dict, dict]:
    """Import strict two-line or 0-name/1/2 TLE records, with no network access."""
    if not isinstance(raw, bytes) or not 0 < len(raw) <= MAX_CATALOG_BYTES:
        raise ValueError("TLE catalog must be nonempty and at most 1 MB")
    target_epoch = _utc(epoch_utc, "epoch_utc")
    _utc(retrieved_utc, "retrieved_utc")
    if (not isinstance(source_url, str) or len(source_url) > 2048
            or any(char.isspace() for char in source_url)):
        raise ValueError("source_url must be an HTTPS URL")
    url = urlsplit(source_url)
    if url.scheme != "https" or not url.hostname or url.username or url.password:
        raise ValueError("source_url must be an HTTPS URL without credentials")
    if (not isinstance(source_license, str) or not source_license.strip()
            or len(source_license) > 200 or any(ord(char) < 32 for char in source_license)):
        raise ValueError("source_license must be a nonempty operator-supplied descriptor")
    if (isinstance(max_tle_age_days, bool)
            or not isinstance(max_tle_age_days, (int, float))
            or not math.isfinite(max_tle_age_days) or max_tle_age_days <= 0):
        raise ValueError("max_tle_age_days must be positive and finite")
    try:
        lines = [line for line in raw.decode("ascii").splitlines() if line]
    except UnicodeDecodeError as error:
        raise ValueError("TLE catalog must contain ASCII text") from error
    if not lines:
        raise ValueError("TLE catalog has no records")
    satellites: list[dict] = []
    epochs: list[datetime] = []
    seen: set[str] = set()
    index = 0
    while index < len(lines):
        if lines[index].startswith("0 "):
            if len(lines[index]) <= 2:
                raise ValueError("empty TLE name line")
            index += 1
        if index + 1 >= len(lines):
            raise ValueError("truncated TLE pair")
        line1, line2 = lines[index:index + 2]
        number = _validate_tle(line1, "1")
        if _validate_tle(line2, "2") != number:
            raise ValueError("TLE pair has mismatched satellite numbers")
        if number in seen:
            raise ValueError(f"duplicate TLE satellite number: {number}")
        seen.add(number)
        tle_epoch = _tle_epoch(line1)
        if abs((target_epoch - tle_epoch).total_seconds()) > max_tle_age_days * 86400:
            raise ValueError(f"stale TLE for satellite {number}")
        satellites.append({
            "id": f"norad-{number}", "tle_line1": line1, "tle_line2": line2,
        })
        epochs.append(tle_epoch)
        if len(satellites) > MAX_SATELLITES:
            raise ValueError("TLE catalog exceeds 1,000 satellites")
        index += 2

    config = {
        "epoch_utc": epoch_utc,
        "max_tle_age_days": float(max_tle_age_days),
        "satellites": satellites,
    }
    manifest = {
        "schema_version": 1,
        "generator": "ntarlink_orbital.tle_catalog",
        "source_url": source_url,
        "source_license": source_license,
        "retrieved_utc": retrieved_utc,
        "raw_tle_sha256": hashlib.sha256(raw).hexdigest(),
        "satellite_count": len(satellites),
        "first_tle_epoch_utc": min(epochs).isoformat().replace("+00:00", "Z"),
        "last_tle_epoch_utc": max(epochs).isoformat().replace("+00:00", "Z"),
        "provenance_scope": (
            "operator-supplied metadata and content digest; publisher identity, "
            "license rights and orbital truth are not independently authenticated"
        ),
    }
    return config, manifest


def main(argv=None) -> None:
    parser = argparse.ArgumentParser(description="Import a pinned offline TLE catalog")
    parser.add_argument("catalog", type=Path)
    parser.add_argument("--epoch-utc", required=True)
    parser.add_argument("--source-url", required=True)
    parser.add_argument("--source-license", required=True)
    parser.add_argument("--retrieved-utc", required=True)
    parser.add_argument("--max-tle-age-days", type=float, default=14.0)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--manifest", type=Path)
    args = parser.parse_args(argv)
    raw = args.catalog.read_bytes()
    config, manifest = import_catalog(
        raw, epoch_utc=args.epoch_utc, source_url=args.source_url,
        source_license=args.source_license, retrieved_utc=args.retrieved_utc,
        max_tle_age_days=args.max_tle_age_days,
    )
    rendered = (json.dumps(config, sort_keys=True, indent=2, allow_nan=False) + "\n").encode()
    manifest["orbit_config_sha256"] = hashlib.sha256(rendered).hexdigest()
    manifest_path = args.manifest or Path(str(args.out) + ".tle-manifest.json")
    if args.out.resolve() == manifest_path.resolve() or args.catalog.resolve() in (
        args.out.resolve(), manifest_path.resolve()
    ):
        raise ValueError("input, output and manifest paths must be distinct")
    args.out.write_bytes(rendered)
    manifest_path.write_text(json.dumps(manifest, sort_keys=True, indent=2) + "\n",
                             encoding="utf-8")
    print(f"imported {len(config['satellites'])} TLE records into {args.out}")


if __name__ == "__main__":
    main()
