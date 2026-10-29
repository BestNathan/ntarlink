"""First-order one-way Doppler estimates from Earth-fixed geometric ranges.

These analytical observables do not model RF carrier recovery, modulation,
oscillator error, atmospheric effects or optical acquisition. Inputs are
scenario-relative integer nanoseconds and geometric ranges in meters.
"""

from math import isfinite
from typing import Callable

from .geometry import SPEED_OF_LIGHT_MPS


def estimate_range_rate_mps(
    range_at_ns: Callable[[int], float], sample_ns: int, half_span_ns: int
) -> float:
    """Central finite-difference d(range)/dt; positive means receding."""
    if (isinstance(sample_ns, bool) or not isinstance(sample_ns, int)
            or isinstance(half_span_ns, bool) or not isinstance(half_span_ns, int)
            or half_span_ns <= 0 or sample_ns < half_span_ns):
        raise ValueError("sample time and finite-difference span must be valid nanoseconds")
    near = float(range_at_ns(sample_ns - half_span_ns))
    far = float(range_at_ns(sample_ns + half_span_ns))
    if not all(isfinite(value) and value > 0 for value in (near, far)):
        raise ValueError("geometric ranges must be finite and positive")
    rate = (far - near) * 1_000_000_000 / (2 * half_span_ns)
    if not isfinite(rate) or abs(rate) >= SPEED_OF_LIGHT_MPS:
        raise ValueError("range rate must be finite and subluminal")
    return rate


def first_order_doppler_shift_hz(range_rate_mps: float, reference_frequency_hz: float) -> float:
    """One-way first-order delta-f = -f0 * d(range)/dt / c.

    The result is a received-minus-transmitted carrier shift. Positive range
    rate means receding endpoints and therefore a negative frequency shift.
    """
    if not isfinite(range_rate_mps) or abs(range_rate_mps) >= SPEED_OF_LIGHT_MPS:
        raise ValueError("range rate must be finite and subluminal")
    if (isinstance(reference_frequency_hz, bool)
            or not isinstance(reference_frequency_hz, (int, float))
            or not isfinite(reference_frequency_hz) or reference_frequency_hz <= 0):
        raise ValueError("reference frequency must be finite and positive Hz")
    result = -reference_frequency_hz * range_rate_mps / SPEED_OF_LIGHT_MPS
    if not isfinite(result):
        raise ValueError("Doppler shift is not finite")
    return result
