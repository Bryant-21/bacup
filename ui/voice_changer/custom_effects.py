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


def _fft_lowpass(audio: np.ndarray, sample_rate: int, cutoff_hz: float) -> np.ndarray:
    """Zero-phase brick wall with a raised-cosine skirt to keep ringing down."""
    spec = np.fft.rfft(audio)
    freqs = np.fft.rfftfreq(len(audio), 1.0 / sample_rate)
    skirt = max(cutoff_hz * 0.25, 20.0)
    gain = np.clip((cutoff_hz + skirt - freqs) / (2 * skirt), 0.0, 1.0)
    return np.fft.irfft(spec * (0.5 - 0.5 * np.cos(np.pi * gain)), len(audio))


def _stft(audio: np.ndarray, n: int, hop: int):
    win = np.hanning(n)
    padded = np.concatenate([np.zeros(n), audio, np.zeros(2 * n)])
    count = 1 + (len(padded) - n) // hop
    idx = np.arange(n)[None, :] + hop * np.arange(count)[:, None]
    return np.fft.rfft(padded[idx] * win, axis=1)


def _istft(spec: np.ndarray, length: int, n: int, hop: int) -> np.ndarray:
    win = np.hanning(n)
    frames = np.fft.irfft(spec, n, axis=1) * win
    out = np.zeros((len(frames) - 1) * hop + n)
    norm = np.zeros_like(out)
    for i, frame in enumerate(frames):
        out[i * hop:i * hop + n] += frame
        norm[i * hop:i * hop + n] += win * win
    return (out / np.maximum(norm, 1e-8))[n:n + length]


def formant_shift(
    audio: np.ndarray,
    sample_rate: int,
    formant_ratio: float = 0.85,
) -> np.ndarray:
    """Move vocal-tract resonances without touching pitch.

    Cepstral liftering separates the formant envelope from the harmonics, so
    resampling only the envelope changes apparent head/throat size. This is what
    makes a normal voice read as a physically bigger creature; pitch shifting
    alone just sounds like a slowed-down human.

    Args:
        audio: Mono float32 audio array.
        sample_rate: Sample rate in Hz.
        formant_ratio: Envelope scale (0.5-1.5). Below 1.0 = bigger, 0.85 is a large
            humanoid, 0.70 is a giant. Above 1.0 shrinks the speaker.

    Returns:
        Processed float32 audio array (same length as input).
    """
    if formant_ratio == 1.0 or len(audio) < 2048:
        return audio.astype(np.float32)

    n, hop, lifter = 1024, 256, 30
    spec = _stft(audio, n, hop)
    cep = np.fft.irfft(np.log(np.abs(spec) + 1e-10), axis=1)
    cep[:, lifter:-lifter] = 0.0
    env = np.fft.rfft(cep, axis=1).real

    bins = np.arange(env.shape[1])
    src = np.clip(bins / formant_ratio, 0, env.shape[1] - 1)
    warped = np.array([np.interp(src, bins, e) for e in env])
    gain = np.clip(np.exp(warped - env), 0.1, 10.0)
    return _istft(spec * gain, len(audio), n, hop).astype(np.float32)


def subharmonic(
    audio: np.ndarray,
    sample_rate: int,
    mix: float = 0.4,
    cutoff_hz: float = 300.0,
) -> np.ndarray:
    """Octave-down layer for weight and growl (dbx-style divide-by-two).

    A flip-flop toggled on every other rising zero crossing of the low band
    produces a square at half the fundamental; shaping it with the input's own
    envelope keeps it glued to the performance instead of droning.

    Args:
        audio: Mono float32 audio array.
        sample_rate: Sample rate in Hz.
        mix: Level of the sub layer relative to the low band (0.0-1.0).
        cutoff_hz: Top of the band the sub is generated from and filtered to.

    Returns:
        Processed float32 audio array (same length as input).
    """
    if mix <= 0.0 or len(audio) < 1024:
        return audio.astype(np.float32)

    low = _fft_lowpass(audio, sample_rate, cutoff_hz)
    negative = np.signbit(low)
    rising = np.flatnonzero((~negative[1:]) & negative[:-1]) + 1
    if len(rising) < 4:
        return audio.astype(np.float32)

    segment = np.searchsorted(rising, np.arange(len(audio)), side="right")
    square = np.where(segment % 2 == 0, 1.0, -1.0)
    envelope = _fft_lowpass(np.abs(audio), sample_rate, 30.0)
    sub = _fft_lowpass(square * np.maximum(envelope, 0.0), sample_rate, cutoff_hz)

    sub_rms = np.sqrt((sub ** 2).mean())
    low_rms = np.sqrt((low ** 2).mean())
    if sub_rms < 1e-9:
        return audio.astype(np.float32)
    return (audio + mix * sub * (low_rms / sub_rms)).astype(np.float32)


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
