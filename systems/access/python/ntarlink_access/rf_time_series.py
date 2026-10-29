"""Offline time-resolved analytical RF budgets from SYS-01 delay samples.

This is an observational SYS-05 sidecar. It does not alter Rust packet
forwarding, establish RF links, or perform per-packet PHY calculations.
"""
from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path

from .rf_budget import (
    SPEED_OF_LIGHT_MPS,
    calculate_rf_budget,
    generate_rf_budget_samples,
)


def generate_time_resolved_rf_budget_samples(scenario: dict, config: dict) -> list[dict]:
    """Evaluate Friis/CN/SINR at each validated geometric delay sample.

    Every configured RF contact must carry a complete, ordered delay series.
    Missing samples are rejected rather than silently replacing a time series
    with a constant midpoint. Times are virtual nanoseconds, not Unix epochs.
    This is O(number of samples) and has the same 500,000-sample safety bound
    as the Rust scenario contract.
    """
    # Reuse the midpoint adapter's config, RF-link, interval and delay checks.
    generate_rf_budget_samples(scenario, config)
    links = {link["id"]: link for link in scenario["links"]}
    budgets = config["rf_link_budgets"]
    results: list[dict] = []
    total = 0
    for contact in scenario["contacts"]:
        link_id = contact["link_id"]
        if link_id not in budgets:
            continue
        start_ns, end_ns = contact["start_ns"], contact["end_ns"]
        series = contact.get("propagation_delay_samples")
        if not isinstance(series, list) or not series:
            raise ValueError(f"missing geometric delay samples for RF contact {link_id}")
        total += len(series)
        if total > 500_000:
            raise ValueError("RF time-series sample budget exceeded (500,000)")
        previous_ns = start_ns - 1
        for index, point in enumerate(series):
            if not isinstance(point, dict) or set(point) != {"at_ns", "delay_ns"}:
                raise ValueError(f"invalid geometric delay sample for {link_id}")
            at_ns, delay_ns = point["at_ns"], point["delay_ns"]
            if (isinstance(at_ns, bool) or not isinstance(at_ns, int)
                    or isinstance(delay_ns, bool) or not isinstance(delay_ns, int)
                    or not previous_ns < at_ns < end_ns or delay_ns <= 0):
                raise ValueError(f"invalid geometric delay sample for {link_id}")
            if index == 0 and at_ns != start_ns:
                raise ValueError(f"RF delay samples must span contact {link_id}")
            previous_ns = at_ns
            budget = calculate_rf_budget(
                range_m=delay_ns * SPEED_OF_LIGHT_MPS / 1_000_000_000,
                **budgets[link_id],
            )
            link = links[link_id]
            results.append({
                "link_id": link_id,
                "direction": f"{link['from']} -> {link['to']}",
                "contact_start_ns": start_ns,
                "contact_end_ns": end_ns,
                "sample_ns": at_ns,
                "source_propagation_delay_ns": delay_ns,
                **budget,
            })
        if previous_ns != end_ns - 1:
            raise ValueError(f"RF delay samples must span contact {link_id}")
    return results


def main(argv=None):
    parser = argparse.ArgumentParser(
        description="Offline sampled RF budget sidecar from SYS-01 geometric delays"
    )
    parser.add_argument("compiled_scenario", type=Path)
    parser.add_argument("budget_config", type=Path)
    parser.add_argument("--out", required=True, type=Path)
    args = parser.parse_args(argv)

    raw_scenario = args.compiled_scenario.read_bytes()
    raw_config = args.budget_config.read_bytes()
    samples = generate_time_resolved_rf_budget_samples(
        json.loads(raw_scenario), json.loads(raw_config)
    )
    document = {
        "schema_version": 1,
        "model": "time-sampled one-way Friis/290 K thermal C/N and optional C/(N+I)",
        "fidelity": (
            "analytical; input ECEF geometric delay samples, no continuous "
            "interpolation, fading, predicted interference, PHY, MAC or link admission"
        ),
        "source": "SYS-01 per-contact propagation_delay_samples; scenario-relative ns",
        "compiled_scenario_sha256": hashlib.sha256(raw_scenario).hexdigest(),
        "rf_budget_config_sha256": hashlib.sha256(raw_config).hexdigest(),
        "sample_count": len(samples),
        "samples": samples,
    }
    args.out.write_text(
        json.dumps(document, indent=2, allow_nan=False) + "\n", encoding="utf-8"
    )
    print(f"generated {len(samples)} time-resolved analytical RF samples in {args.out}")


if __name__ == "__main__":
    main()
