"""Lightweight loader for translation-map YAML data tables.

Loads only the data tables consumed by the Python pipeline
(``material_overrides``). The Rust translator owns its own copy of the
translation-map data and does not consume this module — see
``py_creation_lib/native/esp/src/conversion/translator/`` for that path.
"""
from __future__ import annotations

from functools import lru_cache

import yaml

from bacup_lib.native_maps import native_translation_maps_dir


@lru_cache(maxsize=None)
def load_translation_map_overrides(source_game: str, target_game: str) -> dict:
    """Return the ``material_overrides`` dict from the translation-map YAML.

    Returns an empty dict if the YAML file is absent or has no overrides
    section. Cached per (source, target) pair.
    """
    path = native_translation_maps_dir() / f"{source_game}_to_{target_game}.yaml"
    if not path.is_file():
        return {}
    with open(path, encoding="utf-8") as f:
        data = yaml.safe_load(f) or {}
    if not isinstance(data, dict):
        return {}
    overrides = data.get("material_overrides") or {}
    return overrides if isinstance(overrides, dict) else {}
