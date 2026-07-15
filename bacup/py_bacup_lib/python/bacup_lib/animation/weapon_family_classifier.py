"""Classify FNV/FO3 weapons into FO4 animation families."""
from __future__ import annotations

from pathlib import Path

import yaml

_TABLE_PATH = Path(__file__).with_name("weapon_family_table.yaml")
_TABLE: dict | None = None


def _load_table() -> dict:
    global _TABLE
    if _TABLE is None:
        with _TABLE_PATH.open(encoding="utf-8") as stream:
            _TABLE = yaml.safe_load(stream) or {}
    return _TABLE


def classify_weapon(
    weap_eid: str,
    animation_type: str,
) -> tuple[str, list[str], dict[str, str]]:
    """Return the family id, weapon-bone filter list, and bone remap."""
    table = _load_table()
    family = table.get("weapons", {}).get(weap_eid)
    if family is None:
        family = table.get("unclassified_fallbacks", {}).get(animation_type)
    if family is None:
        return "B21_FNVUnclassified", [], {}

    family_data = table.get("families", {}).get(family, {})
    return (
        family,
        list(family_data.get("weapon_bones", [])),
        dict(family_data.get("bone_remap", {})),
    )


def family_subgraph(family: str) -> str:
    """Return the FO4 subgraph name for a family id."""
    if family == "B21_FNVUnclassified":
        return family
    table = _load_table()
    family_data = table.get("families", {}).get(family, {})
    return str(family_data.get("fo4_subgraph", family))
