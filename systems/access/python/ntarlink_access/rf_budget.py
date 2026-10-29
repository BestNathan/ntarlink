"""Deterministic, interference-free analytical RF budgets for geometric contacts.

SYS-05 consumes SYS-01 midpoint geometric delays; no radio bearer is established,
predict MAC capacity, or reproduce a proprietary satellite air interface.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import math
from pathlib import Path

SPEED_OF_LIGHT_MPS = 299_792_458.0

BOLTZMANN_J_PER_K = 1.380649e-23
REFERENCE_TEMPERATURE_K = 290.0
RF_KINDS = {"user_rf", "feeder"}
PARAMETERS = {
    "carrier_hz", "bandwidth_hz", "tx_power_dbm", "tx_gain_dbi",
    "rx_gain_dbi", "noise_figure_db", "other_losses_db", "required_cn_db",
}


def _number(name: str, value: object, *, positive=False, nonnegative=False) -> float:
    if isinstance(value, bool) or not isinstance(value, (int, float)):
        raise ValueError(f"{name} must be a finite number")
    value = float(value)
    if not math.isfinite(value):
        raise ValueError(f"{name} must be a finite number")
    if positive and value <= 0 or nonnegative and value < 0:
        raise ValueError(f"{name} is outside its physical domain")
    return value


def calculate_rf_budget(
    *,
    range_m: float,
    carrier_hz: float,
    bandwidth_hz: float,
    tx_power_dbm: float,
    tx_gain_dbi: float,
    rx_gain_dbi: float,
    noise_figure_db: float,
    other_losses_db: float,
    required_cn_db: float,
) -> dict:
    """One-way Friis + 290 K thermal-noise budget; all power is in dBm.

    C/N0 is in dB-Hz, C/N and margin in dB. The configured required C/N
    is a *reference threshold*, not a measured MODCOD/BLER or admission rule.
    Interference, fading, weather, pointing and resource allocation are absent.
    """
    distance = _number("range_m", range_m, positive=True)
    frequency = _number("carrier_hz", carrier_hz, positive=True)
    bandwidth = _number("bandwidth_hz", bandwidth_hz, positive=True)
    tx_power = _number("tx_power_dbm", tx_power_dbm)
    tx_gain = _number("tx_gain_dbi", tx_gain_dbi)
    rx_gain = _number("rx_gain_dbi", rx_gain_dbi)
    noise_figure = _number("noise_figure_db", noise_figure_db, nonnegative=True)
    other_losses = _number("other_losses_db", other_losses_db, nonnegative=True)
    required_cn = _number("required_cn_db", required_cn_db)

    # Logarithmic form avoids overflow for large, otherwise finite inputs.
    fspl_db = 20 * (
        math.log10(4 * math.pi / SPEED_OF_LIGHT_MPS)
        + math.log10(distance) + math.log10(frequency)
    )
    received_dbm = tx_power + tx_gain + rx_gain - fspl_db - other_losses
    noise_density_dbm_hz = (
        10 * math.log10(BOLTZMANN_J_PER_K * REFERENCE_TEMPERATURE_K / 1e-3)
        + noise_figure
    )
    noise_dbm = noise_density_dbm_hz + 10 * math.log10(bandwidth)
    cn0_db_hz = received_dbm - noise_density_dbm_hz
    cn_db = received_dbm - noise_dbm
    margin_db = cn_db - required_cn
    outputs = {
        "range_m": distance,
        "carrier_hz": frequency,
        "bandwidth_hz": bandwidth,
        "free_space_path_loss_db": fspl_db,
        "received_power_dbm": received_dbm,
        "noise_density_dbm_hz": noise_density_dbm_hz,
        "thermal_noise_dbm": noise_dbm,
        "cn0_db_hz": cn0_db_hz,
        "cn_db": cn_db,
        "required_cn_db": required_cn,
        "link_margin_db": margin_db,
        "meets_reference_cn": margin_db >= 0,
    }
    if not all(math.isfinite(v) for v in outputs.values() if isinstance(v, float)):
        raise ValueError("RF budget result is nonfinite")
    return {
        key: round(value, 6) if isinstance(value, float) else value
        for key, value in outputs.items()
    }


def generate_rf_budget_samples(scenario: dict, config: dict) -> list[dict]:
    """Sample directed RF budgets from SYS-01 contact midpoint delays.

    The published contact propagation_delay_ns is the vacuum geometric
    distance divided by c and rounded to integer nanoseconds. Converting it
    back introduces at most about 0.15 m of distance quantization error.
    The value is constant per contact, not time-resolved propagation.
    """
    if scenario.get("schema_version") != 1 or scenario.get("contact_mode") != "scheduled":
        raise ValueError("RF budget requires a compiled scheduled-contact v1 scenario")
    budgets = config.get("rf_link_budgets")
    if not isinstance(budgets, dict) or not budgets:
        raise ValueError("rf_link_budgets must be a nonempty link-to-parameters object")
    links = {link["id"]: link for link in scenario["links"]}
    for link_id, params in budgets.items():
        if link_id not in links or links[link_id]["kind"] not in RF_KINDS:
            raise ValueError(f"unknown or non-RF budget link: {link_id}")
        if not isinstance(params, dict) or set(params) != PARAMETERS:
            raise ValueError(f"RF budget {link_id} must provide exactly {sorted(PARAMETERS)}")
        calculate_rf_budget(range_m=1.0, **params)

    samples = []
    for contact in scenario["contacts"]:
        link_id = contact["link_id"]
        if link_id not in budgets:
            continue
        start_ns, end_ns = contact["start_ns"], contact["end_ns"]
        if not (isinstance(start_ns, int) and not isinstance(start_ns, bool)
                and isinstance(end_ns, int) and not isinstance(end_ns, bool)
                and 0 <= start_ns < end_ns <= scenario["duration_ns"]):
            raise ValueError(f"invalid contact interval for {link_id}")
        delay_ns = contact.get("propagation_delay_ns")
        if (isinstance(delay_ns, bool) or not isinstance(delay_ns, int)
                or delay_ns <= 0):
            raise ValueError(f"missing positive geometric midpoint delay for {link_id}")
        midpoint_ns = start_ns + (end_ns - start_ns) // 2
        link = links[link_id]
        budget = calculate_rf_budget(
            range_m=delay_ns * SPEED_OF_LIGHT_MPS / 1_000_000_000,
            **budgets[link_id],
        )
        samples.append({
            "link_id": link_id,
            "direction": f"{link['from']} -> {link['to']}",
            "contact_start_ns": start_ns,
            "contact_end_ns": end_ns,
            "sample_ns": midpoint_ns,
            "source_propagation_delay_ns": delay_ns,
            **budget,
        })
    return samples


def main(argv=None):
    parser = argparse.ArgumentParser(
        description="Offline analytical RF budgets for compiled orbital contacts"
    )
    parser.add_argument("compiled_scenario", type=Path)
    parser.add_argument("budget_config", type=Path)
    parser.add_argument("--out", required=True, type=Path)
    args = parser.parse_args(argv)

    raw_scenario = args.compiled_scenario.read_bytes()
    raw_config = args.budget_config.read_bytes()
    scenario = json.loads(raw_scenario)
    config = json.loads(raw_config)
    samples = generate_rf_budget_samples(scenario, config)
    document = {
        "schema_version": 1,
        "model": "one-way Friis free-space loss and 290 K thermal-noise C/N",
        "fidelity": "analytical; no interference, fading, pointing, MAC or PHY",
        "source": "SYS-01 midpoint geometric propagation delay (integer nanoseconds)",
        "reference_temperature_k": REFERENCE_TEMPERATURE_K,
        "compiled_scenario_sha256": hashlib.sha256(raw_scenario).hexdigest(),
        "rf_budget_config_sha256": hashlib.sha256(raw_config).hexdigest(),
        "samples": samples,
    }
    payload = json.dumps(document, indent=2, allow_nan=False) + "\n"
    args.out.write_text(payload, encoding="utf-8")
    print(f"generated {len(samples)} analytical RF samples in {args.out}")


if __name__ == "__main__":
    main()
