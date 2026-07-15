"""Tests for custom numpy audio effects."""
import numpy as np
import pytest


def _sine_wave(freq: float = 440.0, duration: float = 0.1, sr: int = 44100) -> np.ndarray:
    """Generate a mono sine wave for testing."""
    t = np.linspace(0, duration, int(sr * duration), endpoint=False, dtype=np.float32)
    return np.sin(2 * np.pi * freq * t)


class TestCombFilter:
    def test_output_same_length(self):
        from ui.voice_changer.custom_effects import comb_filter
        audio = _sine_wave()
        result = comb_filter(audio, sample_rate=44100, delay_seconds=0.015, decay=0.6)
        assert result.shape == audio.shape

    def test_delay_creates_interference(self):
        from ui.voice_changer.custom_effects import comb_filter
        audio = _sine_wave()
        result = comb_filter(audio, sample_rate=44100, delay_seconds=0.015, decay=0.6)
        # Result should differ from input (interference pattern)
        assert not np.allclose(result, audio)

    def test_zero_decay_returns_original(self):
        from ui.voice_changer.custom_effects import comb_filter
        audio = _sine_wave()
        result = comb_filter(audio, sample_rate=44100, delay_seconds=0.015, decay=0.0)
        np.testing.assert_allclose(result, audio, atol=1e-7)

    def test_output_dtype_float32(self):
        from ui.voice_changer.custom_effects import comb_filter
        audio = _sine_wave()
        result = comb_filter(audio, sample_rate=44100, delay_seconds=0.015, decay=0.6)
        assert result.dtype == np.float32


class TestTremolo:
    def test_output_same_length(self):
        from ui.voice_changer.custom_effects import tremolo
        audio = _sine_wave()
        result = tremolo(audio, sample_rate=44100, frequency_hz=50.0, wet_level=0.5)
        assert result.shape == audio.shape

    def test_zero_wet_returns_original(self):
        from ui.voice_changer.custom_effects import tremolo
        audio = _sine_wave()
        result = tremolo(audio, sample_rate=44100, frequency_hz=50.0, wet_level=0.0)
        np.testing.assert_allclose(result, audio, atol=1e-7)

    def test_modulation_changes_amplitude(self):
        from ui.voice_changer.custom_effects import tremolo
        audio = np.ones(44100, dtype=np.float32)  # constant signal
        result = tremolo(audio, sample_rate=44100, frequency_hz=10.0, wet_level=1.0)
        # With full wet on constant signal, output should vary
        assert result.max() > result.min()

    def test_output_dtype_float32(self):
        from ui.voice_changer.custom_effects import tremolo
        audio = _sine_wave()
        result = tremolo(audio, sample_rate=44100, frequency_hz=50.0, wet_level=0.5)
        assert result.dtype == np.float32


class TestWhiteNoiseMix:
    def test_output_same_length(self):
        from ui.voice_changer.custom_effects import white_noise_mix
        audio = _sine_wave()
        result = white_noise_mix(audio, amplitude=0.05)
        assert result.shape == audio.shape

    def test_zero_amplitude_returns_original(self):
        from ui.voice_changer.custom_effects import white_noise_mix
        audio = _sine_wave()
        result = white_noise_mix(audio, amplitude=0.0)
        np.testing.assert_allclose(result, audio, atol=1e-7)

    def test_noise_added(self):
        from ui.voice_changer.custom_effects import white_noise_mix
        audio = np.zeros(44100, dtype=np.float32)
        result = white_noise_mix(audio, amplitude=0.05)
        # Should have some noise energy
        assert np.abs(result).max() > 0.0

    def test_output_dtype_float32(self):
        from ui.voice_changer.custom_effects import white_noise_mix
        audio = _sine_wave()
        result = white_noise_mix(audio, amplitude=0.05)
        assert result.dtype == np.float32
