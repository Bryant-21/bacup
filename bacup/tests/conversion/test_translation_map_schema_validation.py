from __future__ import annotations

from pathlib import Path
from typing import Any

import yaml

MAP_PATH = Path("bacup/py_bacup_lib/native/conversion/src/embedded/translation_maps/fo76_to_fo4.yaml")


def _load_map(path: Path) -> dict[str, Any]:
    data = yaml.safe_load(path.read_text(encoding="utf-8"))
    assert isinstance(data, dict)
    return data


def test_fo76_to_fo4_native_map_parses_as_mapping() -> None:
    data = _load_map(MAP_PATH)

    assert data


def test_fo76_to_fo4_native_map_has_expected_record_blocks() -> None:
    data = _load_map(MAP_PATH)

    for record_sig in ("AMMO", "NPC_", "RACE", "WEAP"):
        assert isinstance(data.get(record_sig), dict), record_sig


def test_fo76_to_fo4_native_map_uses_native_signatures_not_legacy_names() -> None:
    data = _load_map(MAP_PATH)

    for legacy_name in ("Weapons", "Npcs", "Races", "Ammunitions"):
        assert legacy_name not in data


def test_fo76_misc_keeps_full_and_component_values() -> None:
    data = _load_map(MAP_PATH)
    misc = data["MISC"]

    assert misc["fields"]["FULL"] == "FULL"
    assert misc["fields"]["MCQP"] == "CVPA"
    assert "MCQP" not in misc.get("drop", [])

    full_transform = misc["transforms"]["FULL"]
    assert full_transform["type"] == "trim_languages"
    assert "target" not in full_transform
    assert misc["transforms"]["CVPA"] == {
        "type": "fo76_misc_components",
        "source_esm": "SeventySix.esm",
    }
