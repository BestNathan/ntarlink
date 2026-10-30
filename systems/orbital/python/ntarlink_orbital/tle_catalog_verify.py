"""Fail-closed verification of a pinned offline TLE intake artifact.

A digest binds local bytes to an operator-supplied manifest. It does not
authenticate the publisher, license, orbital truth or the manifest itself.
"""

from __future__ import annotations

import hashlib
import json
import re
from pathlib import Path

from .tle_catalog import MAX_SATELLITES, _tle_epoch, _utc, _validate_tle

MAX_CONFIG_BYTES = 2_000_000
MAX_MANIFEST_BYTES = 65_536
SHA256_PATTERN = re.compile(r"^[0-9a-f]{64}$")


def _unique_pairs(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError(f"duplicate JSON key: {key}")
        result[key] = value
    return result


def _read_json(path: Path, limit: int, name: str) -> tuple[dict, bytes]:
    with path.open("rb") as handle:
        raw = handle.read(limit + 1)
    if not raw or len(raw) > limit:
        raise ValueError(f"{name} exceeds its byte limit or is empty")
    try:
        parsed = json.loads(raw.decode("utf-8"), object_pairs_hook=_unique_pairs,
                            parse_constant=lambda value: (_ for _ in ()).throw(
                                ValueError(f"nonfinite JSON value: {value}")))
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        raise ValueError(f"invalid {name} JSON") from error
    if not isinstance(parsed, dict):
        raise ValueError(f"{name} must be a JSON object")
    return parsed, raw


def load_verified_catalog(config_path: Path, manifest_path: Path) -> tuple[dict, str]:
    """Return an already verified configuration and manifest-byte SHA-256.

    The exact configuration bytes used for verification are parsed and
    returned, rather than reopening the configuration after checking it.
    """
    manifest, manifest_bytes = _read_json(Path(manifest_path), MAX_MANIFEST_BYTES,
                                          "TLE manifest")
    config, config_bytes = _read_json(Path(config_path), MAX_CONFIG_BYTES,
                                      "orbit configuration")
    if (type(manifest.get("schema_version")) is not int
            or manifest["schema_version"] != 1
            or manifest.get("generator") != "ntarlink_orbital.tle_catalog"):
        raise ValueError("unsupported TLE intake manifest")
    for field in ("raw_tle_sha256", "orbit_config_sha256"):
        if not isinstance(manifest.get(field), str) or not SHA256_PATTERN.fullmatch(
            manifest[field]
        ):
            raise ValueError(f"invalid manifest {field}")
    if hashlib.sha256(config_bytes).hexdigest() != manifest["orbit_config_sha256"]:
        raise ValueError("orbit configuration digest mismatch")
    satellites = config.get("satellites")
    if (not isinstance(satellites, list)
            or not 1 <= len(satellites) <= MAX_SATELLITES
            or type(manifest.get("satellite_count")) is not int
            or manifest["satellite_count"] != len(satellites)):
        raise ValueError("TLE manifest satellite count mismatch")
    epoch = _utc(config.get("epoch_utc"), "epoch_utc")
    age = config.get("max_tle_age_days")
    if type(age) not in (int, float) or not 0 < age <= 365:
        raise ValueError("invalid catalog maximum TLE age")
    seen = set()
    for satellite in satellites:
        if not isinstance(satellite, dict):
            raise ValueError("invalid catalog satellite record")
        line1, line2 = satellite.get("tle_line1"), satellite.get("tle_line2")
        if not isinstance(line1, str) or not isinstance(line2, str):
            raise ValueError("missing catalog TLE pair")
        number = _validate_tle(line1, "1")
        if _validate_tle(line2, "2") != number or satellite.get("id") != f"norad-{number}":
            raise ValueError("catalog satellite identity mismatch")
        if number in seen:
            raise ValueError("duplicate catalog satellite number")
        seen.add(number)
        if abs((epoch - _tle_epoch(line1)).total_seconds()) > age * 86400:
            raise ValueError("catalog contains stale TLE")
    return config, hashlib.sha256(manifest_bytes).hexdigest()
