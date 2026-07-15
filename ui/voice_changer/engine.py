"""Effect chain processor — Pedalboard native effects + custom numpy effects."""
from __future__ import annotations

import logging
from typing import Any

import numpy as np
import pedalboard

from ui.voice_changer.custom_effects import comb_filter, tremolo, white_noise_mix

_log = logging.getLogger("toolkit.voice_changer.engine")

# Custom effects processed directly on numpy arrays (not through Pedalboard)
_CUSTOM_EFFECTS = {"CombFilter", "Tremolo", "WhiteNoiseMix"}

# Map of type string -> Pedalboard class
_NATIVE_EFFECT_MAP: dict[str, type] = {
    "HighpassFilter": pedalboard.HighpassFilter,
    "LowpassFilter": pedalboard.LowpassFilter,
    "Compressor": pedalboard.Compressor,
    "Gain": pedalboard.Gain,
    "Distortion": pedalboard.Distortion,
    "Chorus": pedalboard.Chorus,
    "Phaser": pedalboard.Phaser,
    "Delay": pedalboard.Delay,
    "Reverb": pedalboard.Reverb,
    "Limiter": pedalboard.Limiter,
    "NoiseGate": pedalboard.NoiseGate,
    "PeakFilter": pedalboard.PeakFilter,
    "LadderFilter": pedalboard.LadderFilter,
    "PitchShift": pedalboard.PitchShift,
    "HighShelfFilter": pedalboard.HighShelfFilter,
    "LowShelfFilter": pedalboard.LowShelfFilter,
    "Clipping": pedalboard.Clipping,
    "Bitcrush": pedalboard.Bitcrush,
    "MP3Compressor": pedalboard.MP3Compressor,
    "GSMFullRateCompressor": pedalboard.GSMFullRateCompressor,
}


def build_native_effect(effect_type: str, params: dict[str, Any]) -> pedalboard.Plugin:
    """Instantiate a Pedalboard effect from type string and parameter dict.

    Args:
        effect_type: Key from _NATIVE_EFFECT_MAP (e.g. "HighpassFilter").
        params: Constructor kwargs (e.g. {"cutoff_frequency_hz": 300}).

    Returns:
        Pedalboard Plugin instance.

    Raises:
        ValueError: If effect_type is not recognized.
    """
    cls = _NATIVE_EFFECT_MAP.get(effect_type)
    if cls is None:
        raise ValueError(f"Unknown effect type: {effect_type!r}")
    return cls(**params)


def _apply_custom_effect(
    audio: np.ndarray, sample_rate: int, effect_type: str, params: dict[str, Any]
) -> np.ndarray:
    """Apply a custom numpy effect."""
    if effect_type == "CombFilter":
        return comb_filter(audio, sample_rate=sample_rate, **params)
    elif effect_type == "Tremolo":
        return tremolo(audio, sample_rate=sample_rate, **params)
    elif effect_type == "WhiteNoiseMix":
        return white_noise_mix(audio, **params)
    else:
        raise ValueError(f"Unknown custom effect: {effect_type!r}")


def _expand_chain(chain: list[dict[str, Any]]) -> list[dict[str, Any]]:
    """Expand compound nodes (e.g. ParametricEQ -> multiple PeakFilters)."""
    expanded = []
    for node in chain:
        if node["type"] == "ParametricEQ" and node.get("enabled", True):
            for band in node.get("bands", []):
                expanded.append({
                    "type": "PeakFilter",
                    "enabled": True,
                    "params": {
                        "cutoff_frequency_hz": band.get("freq", 1000),
                        "gain_db": band.get("gain", 0),
                        "q": band.get("q", 1.0),
                    },
                })
        elif node["type"] in ("HighpassFilter", "LowpassFilter"):
            params = dict(node.get("params", {}))
            rolloff = params.pop("rolloff_db_per_oct", 6)
            count = max(1, round(rolloff / 6))
            for _ in range(count):
                expanded.append({
                    "type": node["type"],
                    "enabled": node.get("enabled", True),
                    "params": dict(params),
                })
        else:
            expanded.append(node)
    return expanded


def process_chain(
    audio: np.ndarray,
    sample_rate: int,
    chain: list[dict[str, Any]],
    normalize: bool = False,
    vst3_plugins: list | None = None,
) -> np.ndarray:
    """Process audio through an ordered effect chain.

    The chain is a list of dicts, each with:
        - "type": str — effect class name
        - "enabled": bool — skip if False
        - "params": dict — constructor kwargs

    Native Pedalboard effects are batched into contiguous groups and processed
    together for efficiency. Custom numpy effects break the batch.

    Args:
        audio: Mono float32 audio array.
        sample_rate: Sample rate in Hz.
        chain: Ordered list of effect descriptors.
        normalize: If True, normalize output to -1.0 dB peak.

    Returns:
        Processed float32 audio array.
    """
    if not chain:
        return audio.copy()

    chain = _expand_chain(chain)
    result = audio.copy()

    # Group contiguous native effects into Pedalboard batches
    native_batch: list[pedalboard.Plugin] = []

    def _flush_batch():
        nonlocal result
        if native_batch:
            board = pedalboard.Pedalboard(list(native_batch))
            result = board(result, sample_rate)
            native_batch.clear()

    for node in chain:
        if not node.get("enabled", True):
            continue

        effect_type = node["type"]
        params = node.get("params", {})

        if effect_type == "VST3":
            # VST3 plugins handled separately — flush batch first
            _flush_batch()
            plugin_path = node.get("plugin_path", "")
            backend = node.get("backend", "pedalboard")
            if plugin_path:
                try:
                    result = _apply_vst3(
                        result, sample_rate, plugin_path, backend, params,
                        vst3_plugins,
                    )
                except Exception:
                    _log.exception("Failed to process VST3 plugin: %s", plugin_path)
        elif effect_type in _CUSTOM_EFFECTS:
            _flush_batch()
            result = _apply_custom_effect(result, sample_rate, effect_type, params)
        else:
            try:
                effect = build_native_effect(effect_type, params)
                native_batch.append(effect)
            except ValueError:
                _log.warning("Skipping unknown effect: %s", effect_type)

    _flush_batch()

    if normalize:
        peak = np.abs(result).max()
        if peak > 0:
            # -1.0 dB peak = 10^(-1/20) ≈ 0.891
            target = 10 ** (-1.0 / 20.0)
            result = result * (target / peak)

    return result.astype(np.float32)


def _apply_vst3(
    audio: np.ndarray,
    sample_rate: int,
    plugin_path: str,
    backend: str,
    params: dict[str, Any],
    vst3_plugins: list | None = None,
) -> np.ndarray:
    """Process audio through a VST3 plugin using the appropriate backend."""
    if backend == "dawdreamer":
        return _apply_vst3_dawdreamer(audio, sample_rate, plugin_path, params)

    # Reuse the cached pedalboard instance from the scan (loaded on main thread).
    # Pedalboard refuses to reload plugins from non-main threads.
    vst = None
    if vst3_plugins:
        for info in vst3_plugins:
            if info.path == plugin_path and info._pedalboard_instance is not None:
                vst = info._pedalboard_instance
                break

    if vst is None:
        import threading
        if threading.current_thread() is not threading.main_thread():
            _log.warning(
                "VST3 plugin not pre-scanned; cannot load from non-main thread: %s",
                os.path.basename(plugin_path),
            )
            return audio
        vst = pedalboard.VST3Plugin(plugin_path)
        if vst3_plugins:
            for _info in vst3_plugins:
                if _info.path == plugin_path:
                    _info._pedalboard_instance = vst
                    break

    for name, value in params.items():
        if hasattr(vst, name):
            try:
                p = vst.parameters.get(name)
                if p and getattr(p, "type", None) == bool:
                    setattr(vst, name, bool(value))
                elif isinstance(value, str):
                    # String enum (e.g. year: "1940")
                    setattr(vst, name, value)
                else:
                    setattr(vst, name, float(value))
            except (ValueError, TypeError):
                pass

    import threading
    on_main = threading.current_thread() is threading.main_thread()
    board = pedalboard.Pedalboard([vst])
    return board(audio, sample_rate, reset=on_main)


def _apply_vst3_dawdreamer(
    audio: np.ndarray,
    sample_rate: int,
    plugin_path: str,
    params: dict[str, Any],
) -> np.ndarray:
    """Process audio through a VST3 plugin via DawDreamer's render engine."""
    import dawdreamer
    import os

    engine = dawdreamer.RenderEngine(sample_rate=sample_rate, block_size=512)
    name = os.path.splitext(os.path.basename(plugin_path))[0]
    processor = engine.make_plugin_processor(name, plugin_path)

    descs = processor.get_plugin_parameters_description()
    name_to_index = {d["name"]: d["index"] for d in descs}
    for param_name, value in params.items():
        idx = name_to_index.get(param_name)
        if idx is not None:
            processor.set_parameter(idx, float(value))

    # Ensure audio is 2D (channels, samples)
    if audio.ndim == 1:
        audio_2d = audio.reshape(1, -1)
    else:
        audio_2d = audio

    # Connect audio input → plugin → output via the graph
    duration = audio_2d.shape[1] / sample_rate
    processor.set_input("input", audio_2d.astype(np.float32))
    engine.load_graph([(processor, [])])
    engine.render(duration)

    output = engine.get_audio()
    # Match original shape
    if audio.ndim == 1:
        return output[0] if output.shape[0] >= 1 else output.flatten()
    return output
