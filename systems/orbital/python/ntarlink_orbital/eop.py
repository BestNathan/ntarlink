"""Offline, checksum-pinned IERS finals2000A Earth-orientation timescale.

This is an opt-in analytical frame input. No network fetches, silent
extrapolation, or replacement of Skyfield's pinned leap-second schedule.
"""

from __future__ import annotations

import hashlib
import io
import math
import re
from datetime import datetime, timedelta, timezone
from pathlib import Path

MAX_EOP_BYTES = 8_000_000
MAX_EOP_DAYS = 50_000
MJD_EPOCH = datetime(1858, 11, 17, tzinfo=timezone.utc)


def _utc(value: str) -> datetime:
    if not isinstance(value, str) or not value.endswith("Z"):
        raise ValueError("EOP valid_until_utc must be an ISO-8601 UTC Z timestamp")
    try:
        parsed = datetime.fromisoformat(value.replace("Z", "+00:00"))
    except ValueError as exc:
        raise ValueError("invalid EOP valid_until_utc") from exc
    return parsed.astimezone(timezone.utc)


def _mjd(value: datetime) -> float:
    return (value - MJD_EPOCH).total_seconds() / 86_400.0


def _classify_eop_rows(raw: bytes, mjd, start_mjd: float, end_mjd: float,
                       allow_predictions: bool) -> dict:
    """Check both IERS quality flags for every parsed daily record.

    The Skyfield parser intentionally ignores the I/P quality columns.
    Finals2000A uses byte offsets 16 and 57 for polar motion and UT1;
    either P flag makes that day's frame input predicted. Interpolation
    may use the next daily record, so include both bracketing UTC days.
    """
    days = {int(day) for day in mjd}
    flags = {}
    for row in raw.splitlines():
        try:
            day = float(row[6:15])
        except (ValueError, IndexError):
            continue
        if day not in days:
            continue
        if day in flags:
            raise ValueError("duplicate EOP quality-flag day")
        if (len(row) <= 57 or row[16:17] not in (b"I", b"P")
                or row[57:58] not in (b"I", b"P")):
            raise ValueError("EOP records require I/P polar-motion and UT1 quality flags")
        flags[day] = (row[16:17], row[57:58])
    if len(flags) != len(mjd):
        raise ValueError("EOP quality flags missing for parsed daily records")

    predicted = {day for day, (polar, ut1) in flags.items()
                 if polar == b"P" or ut1 == b"P"}
    first_needed, last_needed = math.floor(start_mjd), math.ceil(end_mjd)
    predicted_in_window = sorted(day for day in predicted
                                 if first_needed <= day <= last_needed)
    if predicted_in_window and not allow_predictions:
        raise ValueError("predicted EOP within simulation interpolation window; "
                         "set allow_predictions=true to opt in")
    return {
        "prediction_policy": "allow" if allow_predictions else "reject",
        "predicted_source_days": len(predicted),
        "predicted_interpolation_days": len(predicted_in_window),
    }


def build_timescale(earth_orientation: dict | None, epoch: datetime,
                    duration_ns: int, config_dir: Path | None = None):
    """Return (Skyfield timescale, frame-input provenance).

    The optional EOP file must be supplied by the experiment and pinned by
    SHA-256. Its daily IERS values drive both UT1 and polar motion; leap
    seconds continue to come from the pinned Skyfield release.
    """
    from skyfield.api import Timescale, load
    from skyfield.data import iers
    import numpy as np

    if isinstance(duration_ns, bool) or not isinstance(duration_ns, int) or duration_ns <= 0:
        raise ValueError("duration_ns must be a positive integer")
    builtin = load.timescale(builtin=True)
    if earth_orientation is None:
        return builtin, {"mode": "skyfield_builtin", "polar_motion": "not installed",
                         "leap_seconds": "skyfield==1.55 builtin"}
    if not isinstance(earth_orientation, dict) or earth_orientation.get("schema_version") != 1:
        raise ValueError("earth_orientation must be a version 1 object")
    if earth_orientation.get("format") != "iers-finals2000a":
        raise ValueError("earth_orientation format must be iers-finals2000a")
    path_value = earth_orientation.get("path")
    digest = earth_orientation.get("sha256")
    if not isinstance(path_value, str) or not path_value.strip():
        raise ValueError("earth_orientation path is required")
    if not isinstance(digest, str) or re.fullmatch(r"[0-9a-f]{64}", digest) is None:
        raise ValueError("earth_orientation sha256 must be lowercase SHA-256 hex")
    allow_predictions = earth_orientation.get("allow_predictions", False)
    if not isinstance(allow_predictions, bool):
        raise ValueError("earth_orientation allow_predictions must be boolean")
    valid_until = _utc(earth_orientation.get("valid_until_utc"))
    end = epoch + timedelta(microseconds=(duration_ns - 1) // 1000)
    if epoch > valid_until or end > valid_until:
        raise ValueError("EOP validity expired before the simulation ends")

    path = Path(path_value)
    if not path.is_absolute():
        path = (config_dir or Path.cwd()) / path
    if not path.is_file() or path.stat().st_size > MAX_EOP_BYTES:
        raise ValueError("EOP file missing or exceeds 8 MB limit")
    raw = path.read_bytes()
    actual_digest = hashlib.sha256(raw).hexdigest()
    if actual_digest != digest:
        raise ValueError("EOP file SHA-256 mismatch")

    values = iers.parse_x_y_dut1_from_finals_all(io.BytesIO(raw))
    if not 2 <= len(values) <= MAX_EOP_DAYS:
        raise ValueError("EOP requires 2 to 50,000 complete daily records")
    mjd = np.asarray(values["utc_mjd"], dtype=float)
    xp = np.asarray(values["x_arcseconds"], dtype=float)
    yp = np.asarray(values["y_arcseconds"], dtype=float)
    dut1 = np.asarray(values["dut1"], dtype=float)
    if (not np.all(np.isfinite(mjd)) or not np.all(np.isfinite(xp))
            or not np.all(np.isfinite(yp)) or not np.all(np.isfinite(dut1))
            or not np.all(np.abs(xp) <= 2.0) or not np.all(np.abs(yp) <= 2.0)
            or not np.all(np.abs(dut1) <= 1.0)
            or not np.all(np.abs(mjd - np.rint(mjd)) < 1e-8)
            or not np.all(np.abs(np.diff(mjd) - 1.0) < 1e-8)):
        raise ValueError("EOP records must be finite, bounded and consecutive UTC days")
    if _mjd(epoch) < mjd[0] or _mjd(end) > mjd[-1]:
        raise ValueError("EOP coverage does not contain the full simulation interval")
    if _mjd(valid_until) > mjd[-1]:
        raise ValueError("EOP valid_until_utc exceeds the last tabulated UTC epoch")

    quality = _classify_eop_rows(raw, mjd, _mjd(epoch), _mjd(end),
                                 allow_predictions)

    # Use the release-pinned leap table, not a guessed TAI-UTC offset.
    # Delta-T = (TAI-UTC) + 32.184 - (UT1-UTC), in SI seconds.
    utc_jd = mjd + 2_400_000.5
    indices = np.searchsorted(builtin.leap_dates, utc_jd, side="right") - 1
    if np.any(indices < 0):
        raise ValueError("EOP precedes the Skyfield leap-second table")
    tai_utc = np.asarray(builtin.leap_offsets)[indices]
    daily_tt = utc_jd + (tai_utc + 32.184) / 86_400.0
    daily_delta_t = tai_utc + 32.184 - dut1
    timescale = Timescale((daily_tt, daily_delta_t),
                          builtin.leap_dates, builtin.leap_offsets)
    iers.install_polar_motion_table(timescale, values)
    return timescale, {
        "mode": "pinned_iers_finals2000a",
        "sha256": actual_digest,
        "first_utc_mjd": float(mjd[0]),
        "last_utc_mjd": float(mjd[-1]),
        "sample_count": len(values),
        **quality,
        "valid_until_utc": earth_orientation["valid_until_utc"],
        "leap_seconds": "skyfield==1.55 builtin",
        "interpolation": "Skyfield linear UT1 and polar-motion interpolation",
    }
