from __future__ import annotations

from pathlib import Path

import yaml


MAP_PATHS = (
    Path("bacup/py_bacup_lib/native/conversion/src/embedded/translation_maps/fo76_to_fo4.yaml"),
)


def test_fo76_to_fo4_map_skip_records_policy() -> None:
    required_skips = {
        "GMST",
        "DFOB",
        "ACHR",
        "DIAL",
        "INFO",
        "NAVI",
        "NAVM",
        "PGRE",
        "PHZD",
        "PLYR",
        "PMIS",
        "REFR",
    }

    for map_path in MAP_PATHS:
        data = yaml.safe_load(map_path.read_text(encoding="utf-8"))
        skipped = set(data.get("skip_records") or [])

        assert required_skips <= skipped, str(map_path)
        assert "SCEN" not in skipped, str(map_path)
        assert "DLBR" not in skipped, str(map_path)


def test_fo76_to_fo4_map_subrecord_drop_policy() -> None:
    for map_path in MAP_PATHS:
        data = yaml.safe_load(map_path.read_text(encoding="utf-8"))

        for record_sig in ("STAT", "FURN"):
            assert "NVNM" not in (data[record_sig].get("drop") or []), f"{map_path}:{record_sig}"

        npc = data["NPC_"]
        for field_name in ("group_object_template", "ObjectTemplates"):
            assert field_name not in (npc.get("fields") or {}), f"{map_path}:{field_name}"
            assert field_name not in (npc.get("transforms") or {}), f"{map_path}:{field_name}"
            assert field_name in (npc.get("drop") or []), f"{map_path}:{field_name}"
