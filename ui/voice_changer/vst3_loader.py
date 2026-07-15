"""VST3 plugin scanner and loader for the voice changer."""
from __future__ import annotations

import logging
import math
import os
from dataclasses import dataclass, field
from typing import Any

_log = logging.getLogger("toolkit.voice_changer.vst3")


_FLOAT_CLAMP = 1e18  # safe range for ImGui sliders


def _to_float(value: Any) -> float:
    """Coerce a parameter value to float, handling pedalboard's WeakTypeWrapper.

    Also clamps inf/-inf to finite bounds so ImGui sliders don't assert.
    """
    try:
        f = float(value)
    except (TypeError, ValueError):
        if hasattr(value, "raw_value"):
            f = float(value.raw_value)
        else:
            return 0.0
    if math.isinf(f) or math.isnan(f):
        return _FLOAT_CLAMP if f > 0 else -_FLOAT_CLAMP if f < 0 else 0.0
    return f



@dataclass
class VST3PluginInfo:
    """Metadata about a discovered VST3 plugin."""
    name: str
    path: str
    parameters: dict[str, dict[str, Any]] = field(default_factory=dict)
    # parameters: {param_name: {"min": float, "max": float, "default": float}}
    backend: str = "pedalboard"  # "pedalboard" or "dawdreamer"
    _pedalboard_instance: Any = field(default=None, repr=False)
    """Cached pedalboard plugin instance (loaded on main thread during scan).
    Pedalboard refuses to reload plugins from non-main threads, so the engine
    reuses this instance for processing."""


def scan_vst3_directory(vst3_dir: str) -> list[VST3PluginInfo]:
    """Scan a directory for .vst3 files and enumerate their parameters.

    Args:
        vst3_dir: Path to directory containing .vst3 plugin files/folders.

    Returns:
        List of VST3PluginInfo with discovered parameters.
    """
    if not os.path.isdir(vst3_dir):
        return []

    plugins: list[VST3PluginInfo] = []
    for dirpath, dirnames, filenames in os.walk(vst3_dir):
        # .vst3 can be a file or a directory (bundle); check both
        for name in filenames + dirnames:
            if name.endswith(".vst3"):
                info = _load_plugin_info(os.path.join(dirpath, name))
                if info:
                    plugins.append(info)
        # Don't recurse into .vst3 bundle directories
        dirnames[:] = [d for d in dirnames if not d.endswith(".vst3")]

    _log.info("Scanned VST3 directory %s: found %d plugins", vst3_dir, len(plugins))
    return plugins


def _load_plugin_info(plugin_path: str) -> VST3PluginInfo | None:
    """Load a VST3 plugin via pedalboard and extract its parameter metadata.

    The instance is cached so _apply_vst3 can reuse it with reset=False from
    any thread (creating a new instance on a non-scan thread triggers pedalboard's
    "must reload on original thread" error).
    """
    try:
        import pedalboard
    except ImportError:
        return _load_plugin_info_dawdreamer(plugin_path)

    try:
        plugin = pedalboard.load_plugin(plugin_path)
        name = os.path.splitext(os.path.basename(plugin_path))[0]

        params: dict[str, dict[str, Any]] = {}
        for param_name in plugin.parameters.keys():
            param = plugin.parameters[param_name]
            valid_values = getattr(param, "valid_values", None)
            raw_default = getattr(plugin, param_name, 0.0)

            meta: dict[str, Any] = {
                "min": _to_float(getattr(param, "min_value", 0.0)),
                "max": _to_float(getattr(param, "max_value", 1.0)),
            }

            # Detect string enums (e.g. year: ['1930', '1940', ...])
            if valid_values and any(isinstance(v, str) for v in valid_values):
                str_values = [str(v) for v in valid_values]
                meta["valid_values"] = str_values
                meta["default"] = str(raw_default)
            else:
                meta["default"] = _to_float(raw_default)

            params[param_name] = meta

        return VST3PluginInfo(
            name=name, path=plugin_path, parameters=params,
            _pedalboard_instance=plugin,
        )
    except Exception:
        _log.debug("pedalboard failed on %s, trying DawDreamer", os.path.basename(plugin_path))
        return _load_plugin_info_dawdreamer(plugin_path)


def _load_plugin_info_dawdreamer(plugin_path: str) -> VST3PluginInfo | None:
    """Try loading a VST3 plugin via DawDreamer (fallback for pedalboard rejects).

    DawDreamer must be imported lazily — importing it before JAX/PyTorch/Numba
    causes LLVM state conflicts and segfaults.
    """
    try:
        import dawdreamer  # lazy import — see docstring
    except ImportError:
        _log.debug(
            "Skipping incompatible VST3 (dawdreamer not installed): %s",
            os.path.basename(plugin_path),
        )
        return None

    try:
        engine = dawdreamer.RenderEngine(sample_rate=44100, block_size=512)
        name = os.path.splitext(os.path.basename(plugin_path))[0]
        processor = engine.make_plugin_processor(name, plugin_path)

        params: dict[str, dict[str, Any]] = {}
        for desc in processor.get_plugin_parameters_description():
            try:
                p_min = _to_float(desc["min"])
                p_max = _to_float(desc["max"])
            except (ValueError, TypeError):
                p_min, p_max = 0.0, 1.0
            params[desc["name"]] = {
                "min": p_min,
                "max": p_max,
                "default": _to_float(desc.get("defaultValue", 0.0)),
            }

        _log.info("Loaded via DawDreamer fallback: %s (%d params)", name, len(params))
        return VST3PluginInfo(
            name=name, path=plugin_path, parameters=params, backend="dawdreamer",
        )
    except Exception:
        _log.warning(
            "DawDreamer also failed to load VST3: %s", os.path.basename(plugin_path),
        )
        return None


def load_vst3_for_processing(
    plugin_path: str,
    params: dict[str, Any],
    backend: str = "pedalboard",
    sample_rate: int = 44100,
    block_size: int = 512,
) -> Any:
    """Load a VST3 plugin instance configured with the given parameters.

    Args:
        plugin_path: Path to the .vst3 file.
        params: Dict of parameter_name -> value to apply.
        backend: Which loader to use ("pedalboard" or "dawdreamer").
        sample_rate: Sample rate for DawDreamer engine (ignored for pedalboard).
        block_size: Block size for DawDreamer engine (ignored for pedalboard).

    Returns:
        Loaded plugin instance ready for processing. For pedalboard this is a
        pedalboard plugin; for dawdreamer this is a (RenderEngine, processor) tuple.
    """
    if backend == "dawdreamer":
        return _load_vst3_dawdreamer(plugin_path, params, sample_rate, block_size)

    import pedalboard
    plugin = pedalboard.load_plugin(plugin_path, parameter_values=params)
    return plugin


def _load_vst3_dawdreamer(
    plugin_path: str,
    params: dict[str, Any],
    sample_rate: int,
    block_size: int,
) -> tuple[Any, Any]:
    """Load a VST3 via DawDreamer and apply parameters.

    Returns:
        (engine, processor) tuple — caller drives rendering via engine.render().
    """
    import dawdreamer

    engine = dawdreamer.RenderEngine(sample_rate=sample_rate, block_size=block_size)
    name = os.path.splitext(os.path.basename(plugin_path))[0]
    processor = engine.make_plugin_processor(name, plugin_path)

    name_to_index = {
        desc["name"]: desc["index"]
        for desc in processor.get_plugin_parameters_description()
    }
    for param_name, value in params.items():
        idx = name_to_index.get(param_name)
        if idx is not None:
            processor.set_parameter(idx, float(value))

    return engine, processor
