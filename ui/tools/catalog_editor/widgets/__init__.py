"""Custom schema widgets (``x-widget``): name -> draw(label, node, value, ctx, id_path) -> (changed, value)."""

from __future__ import annotations

from collections.abc import Callable
from typing import Any

from ui.tools.catalog_editor.widgets import conditions, curve

REGISTRY: dict[str, Callable[[str, dict, Any, Any, str], tuple[bool, Any]]] = {
    "conditions": conditions.draw,
    "curve": curve.draw,
}
