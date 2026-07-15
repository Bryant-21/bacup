"""Tests for the voice changer effect engine."""
import numpy as np
import pytest


def _sine_wave(freq: float = 440.0, duration: float = 0.5, sr: int = 44100) -> np.ndarray:
    t = np.linspace(0, duration, int(sr * duration), endpoint=False, dtype=np.float32)
    return np.sin(2 * np.pi * freq * t)


class TestEffectEngine:
    def test_empty_chain_returns_copy(self):
        from ui.voice_changer.engine import process_chain
        audio = _sine_wave()
        result = process_chain(audio, sample_rate=44100, chain=[])
        np.testing.assert_allclose(result, audio, atol=1e-6)

    def test_single_native_effect(self):
        from ui.voice_changer.engine import process_chain
        audio = _sine_wave()
        chain = [{"type": "Gain", "enabled": True, "params": {"gain_db": -6.0}}]
        result = process_chain(audio, sample_rate=44100, chain=chain)
        # Gain of -6dB should reduce amplitude by ~half
        assert result.shape == audio.shape
        assert np.abs(result).max() < np.abs(audio).max()

    def test_disabled_effect_skipped(self):
        from ui.voice_changer.engine import process_chain
        audio = _sine_wave()
        chain = [{"type": "Gain", "enabled": False, "params": {"gain_db": -60.0}}]
        result = process_chain(audio, sample_rate=44100, chain=chain)
        np.testing.assert_allclose(result, audio, atol=1e-6)

    def test_custom_comb_filter(self):
        from ui.voice_changer.engine import process_chain
        audio = _sine_wave()
        chain = [{"type": "CombFilter", "enabled": True, "params": {"delay_seconds": 0.015, "decay": 0.6}}]
        result = process_chain(audio, sample_rate=44100, chain=chain)
        assert not np.allclose(result, audio)

    def test_custom_tremolo(self):
        from ui.voice_changer.engine import process_chain
        audio = _sine_wave()
        chain = [{"type": "Tremolo", "enabled": True, "params": {"frequency_hz": 50.0, "wet_level": 0.5}}]
        result = process_chain(audio, sample_rate=44100, chain=chain)
        assert not np.allclose(result, audio)

    def test_custom_white_noise_mix(self):
        from ui.voice_changer.engine import process_chain
        audio = np.zeros(44100, dtype=np.float32)
        chain = [{"type": "WhiteNoiseMix", "enabled": True, "params": {"amplitude": 0.05}}]
        result = process_chain(audio, sample_rate=44100, chain=chain)
        assert np.abs(result).max() > 0.0

    def test_chained_effects(self):
        from ui.voice_changer.engine import process_chain
        audio = _sine_wave()
        chain = [
            {"type": "HighpassFilter", "enabled": True, "params": {"cutoff_frequency_hz": 300}},
            {"type": "LowpassFilter", "enabled": True, "params": {"cutoff_frequency_hz": 4000}},
            {"type": "Gain", "enabled": True, "params": {"gain_db": -3.0}},
        ]
        result = process_chain(audio, sample_rate=44100, chain=chain)
        assert result.shape == audio.shape

    def test_mixed_native_and_custom(self):
        from ui.voice_changer.engine import process_chain
        audio = _sine_wave()
        chain = [
            {"type": "CombFilter", "enabled": True, "params": {"delay_seconds": 0.015, "decay": 0.6}},
            {"type": "HighpassFilter", "enabled": True, "params": {"cutoff_frequency_hz": 100}},
            {"type": "Compressor", "enabled": True, "params": {"threshold_db": -20, "ratio": 4.0}},
        ]
        result = process_chain(audio, sample_rate=44100, chain=chain)
        assert result.shape == audio.shape

    def test_normalize_flag(self):
        from ui.voice_changer.engine import process_chain
        audio = _sine_wave() * 0.1  # quiet signal
        chain = [{"type": "Gain", "enabled": True, "params": {"gain_db": 0.0}}]
        result = process_chain(audio, sample_rate=44100, chain=chain, normalize=True)
        # After normalization, peak should be near -1.0 dB (~0.89)
        assert np.abs(result).max() > np.abs(audio).max()


class TestBuildEffect:
    def test_unknown_type_raises(self):
        from ui.voice_changer.engine import build_native_effect
        with pytest.raises(ValueError, match="Unknown effect type"):
            build_native_effect("FakeEffect", {})

    def test_build_highpass(self):
        from ui.voice_changer.engine import build_native_effect
        import pedalboard
        effect = build_native_effect("HighpassFilter", {"cutoff_frequency_hz": 300})
        assert isinstance(effect, pedalboard.HighpassFilter)

    def test_build_compressor(self):
        from ui.voice_changer.engine import build_native_effect
        import pedalboard
        effect = build_native_effect("Compressor", {"threshold_db": -15, "ratio": 4.0})
        assert isinstance(effect, pedalboard.Compressor)
