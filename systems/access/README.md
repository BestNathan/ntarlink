# SYS-05: analytical RF link budget

This bounded subsystem consumes the **compiled SYS-01 contact artifact** rather
than calling an orbital propagator. Each directed user/feeder RF contact has a
positive `propagation_delay_ns` sampled at its geometric midpoint. The access
adapter converts that integer-nanosecond vacuum delay back into an approximate
range in meters, with at most ~0.15 m quantization error, then computes an
interference-free Friis/thermal-noise budget.

```sh
python -m pip install -e systems/orbital/python -e systems/access/python
python -m ntarlink_orbital.contacts \
  scenarios/orbital-pass.json scenarios/orbit-sources.sample.json \
  --out /tmp/ntarlink-orbital-pass.json
python -m ntarlink_access.rf_budget \
  /tmp/ntarlink-orbital-pass.json scenarios/rf-budgets.sample.json \
  --out /tmp/ntarlink-rf-budget.json
python -m unittest discover -s systems/access/python/tests
```

The JSON sidecar includes SHA-256 hashes of the exact compiled scenario and RF
configuration, direction, contact interval, midpoint sample, original delay,
derived range, FSPL, received power, thermal noise, C/N0, C/N, configured
reference C/N and reference margin. Inputs are explicit: carrier frequency
(Hz), occupied bandwidth (Hz), transmit power (dBm), directional antenna gains
(dBi), receiver noise figure (dB), other losses (dB), required C/N (dB).
The noise reference is 290 K with exact SI Boltzmann constant.

**Fidelity:** analytical only. Reference margin is not modem admission.
There is no interference, fading, rain attenuation unless manually included
as a static loss, beam steering, spectrum allocation, MODCOD/BLER curve,
carrier recovery, RF waveform, or MAC scheduler. Existing scenario link
capacity is **not changed** by these numbers. A contact is geometric
potential, not an acquired link. Results are midpoint snapshots, not
continuous time-series or calibrated Starlink performance.

See [SYS-05 design](../../../docs/subsystems/05-access-network.md) and
[SYS-01 orbital contacts](../../../scenarios/ORBITAL-README.md).
