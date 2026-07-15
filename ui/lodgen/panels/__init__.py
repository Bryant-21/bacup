"""Shared helpers for lodgen panels."""
from __future__ import annotations

from imgui_bundle import imgui

_FORMATS = ["Bc1", "Bc2", "Bc3", "Bc5", "Bc7", "Rgba8", "Bgr565"]


def _format_combo(label: str, current: str) -> str:
    """Draw an imgui combo for texture format. Returns the selected format string."""
    idx = _FORMATS.index(current) if current in _FORMATS else 0
    changed, new_idx = imgui.combo(label, idx, _FORMATS)
    return _FORMATS[new_idx] if changed else current


def _slider_int_setting(label: str, d: dict, key: str, lo: int, hi: int) -> None:
    """Read/write an int from dict d[key] via a slider."""
    val = int(d.get(key, lo))
    changed, new_val = imgui.slider_int(label, val, lo, hi)
    if changed:
        d[key] = new_val


def _slider_float_setting(label: str, d: dict, key: str, lo: float, hi: float) -> None:
    """Read/write a float from dict d[key] via a slider."""
    val = float(d.get(key, lo))
    changed, new_val = imgui.slider_float(label, val, lo, hi)
    if changed:
        d[key] = new_val
