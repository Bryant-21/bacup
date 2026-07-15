"""Custom audio effects implemented with numpy."""
from __future__ import annotations

import numpy as np


def comb_filter(
    audio: np.ndarray,
    sample_rate: int,
    delay_seconds: float = 0.015,
    decay: float = 0.6,
) -> np.ndarray:
    """Comb filter: mix audio with a delayed copy to create metallic resonance.

    Replicates Audacity's Echo effect with very short delay times (10-30ms).
    The interference pattern creates the signature robotic/metallic texture.

    Args:
        audio: Mono float32 audio array.
        sample_rate: Sample rate in Hz.
        delay_seconds: Delay in seconds (0.001-0.1). 0.015 is ideal for robot voice.
        decay: Mix level of the delayed copy (0.0-1.0). 0.6 matches Audacity default.

    Returns:
        Processed float32 audio array (same length as input).
    """
    delay_samples = int(delay_seconds * sample_rate)
    if delay_samples <= 0 or decay == 0.0:
        return audio.copy()

    result = audio.copy()
    # Add delayed copy scaled by decay factor
    if delay_samples < len(audio):
        result[delay_samples:] += decay * audio[:-delay_samples]
    return result.astype(np.float32)


def tremolo(
    audio: np.ndarray,
    sample_rate: int,
    frequency_hz: float = 50.0,
    wet_level: float = 0.45,
) -> np.ndarray:
    """Tremolo: rapid amplitude modulation for digital/computerized texture.

    At 40-50 Hz, this creates a synthetic buzz rather than a rhythmic pulse.
    Replicates Audacity's Tremolo effect with high frequency settings.

    Args:
        audio: Mono float32 audio array.
        sample_rate: Sample rate in Hz.
        frequency_hz: Modulation frequency (1-100). 50 Hz for computerized AI sound.
        wet_level: Mix of modulated signal (0.0-1.0). Keep at 0.4-0.5 for clarity.

    Returns:
        Processed float32 audio array (same length as input).
    """
    if wet_level == 0.0:
        return audio.copy()

    t = np.arange(len(audio), dtype=np.float32) / sample_rate
    # Modulation oscillates between (1 - wet_level) and 1.0
    modulator = 1.0 - wet_level + wet_level * (0.5 + 0.5 * np.sin(2 * np.pi * frequency_hz * t))
    return (audio * modulator).astype(np.float32)


def white_noise_mix(
    audio: np.ndarray,
    amplitude: float = 0.05,
) -> np.ndarray:
    """Mix white noise into the audio signal (for walkie-talkie static).

    Args:
        audio: Mono float32 audio array.
        amplitude: Noise amplitude (0.0-1.0). 0.05 is subtle radio static.

    Returns:
        Processed float32 audio array (same length as input).
    """
    if amplitude == 0.0:
        return audio.copy()

    noise = np.random.default_rng().normal(0, amplitude, len(audio)).astype(np.float32)
    return (audio + noise).astype(np.float32)
