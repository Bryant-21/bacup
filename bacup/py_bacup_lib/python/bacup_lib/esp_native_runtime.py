"""Conversion-local ESP registry (bacup_lib._native.esp_authoring_core).

Handles from creation_lib._native must NEVER be passed here, nor vice versa —
development extensions have independent handle registries. Packaged BACUP shares
the registry, but callers still own and close only their own handles.
"""
from __future__ import annotations

from importlib import import_module
from typing import Any

_NATIVE: Any | None = None


def load_esp_native() -> Any:
    global _NATIVE
    if _NATIVE is None:
        _NATIVE = import_module("bacup_lib._native.esp_authoring_core")
    return _NATIVE
