from __future__ import annotations

from pathlib import Path

import yaml


MAP_PATHS = (
    Path("bacup/py_bacup_lib/native/conversion/src/embedded/translation_maps/fo76_to_fo4.yaml"),
)

PRIMARY_MAP_PATHS = MAP_PATHS


def test_fo76_to_fo4_map_skips_game_settings() -> None:
    for map_path in PRIMARY_MAP_PATHS:
        data = yaml.safe_load(map_path.read_text(encoding="utf-8"))

        assert "GMST" in (data.get("skip_records") or []), str(map_path)


def test_fo76_to_fo4_map_skips_records_requiring_structured_or_ck_safe_emitters() -> None:
    structured_emitter_signatures = {
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

    for map_path in PRIMARY_MAP_PATHS:
        data = yaml.safe_load(map_path.read_text(encoding="utf-8"))
        skipped = set(data.get("skip_records") or [])

        assert structured_emitter_signatures <= skipped, str(map_path)


def test_fo76_to_fo4_map_does_not_skip_flat_scenedlbr_records() -> None:
    # The translation map comment says these are flat top-level FO4 records
    # and should be emitted directly by the generic writer.
    for map_path in PRIMARY_MAP_PATHS:
        data = yaml.safe_load(map_path.read_text(encoding="utf-8"))
        skipped = set(data.get("skip_records") or [])

        assert "SCEN" not in skipped, str(map_path)
        assert "DLBR" not in skipped, str(map_path)


def test_fo76_to_fo4_map_skips_fo76_default_object_assignments() -> None:
    for map_path in PRIMARY_MAP_PATHS:
        data = yaml.safe_load(map_path.read_text(encoding="utf-8"))

        assert "DFOB" in (data.get("skip_records") or []), str(map_path)


def test_fo76_to_fo4_map_preserves_nvnm_for_target_records() -> None:
    for map_path in MAP_PATHS:
        data = yaml.safe_load(map_path.read_text(encoding="utf-8"))

        for record_sig in ("STAT", "FURN"):
            dropped = data[record_sig].get("drop") or []
            assert "NVNM" not in dropped, f"{map_path}:{record_sig}"


def test_fo76_to_fo4_race_map_drops_ck_incompatible_fo76_subrecords() -> None:
    expected_drops = {"HEAD", "MPPK", "BSMP", "BSMB", "BSMS", "BMMP"}

    for map_path in MAP_PATHS:
        data = yaml.safe_load(map_path.read_text(encoding="utf-8"))
        dropped = set(data["RACE"].get("drop") or [])

        assert expected_drops <= dropped, str(map_path)


def test_fo76_to_fo4_npc_map_drops_object_templates() -> None:
    for map_path in MAP_PATHS:
        data = yaml.safe_load(map_path.read_text(encoding="utf-8"))
        npc = data["NPC_"]

        for field_name in ("group_object_template", "ObjectTemplates"):
            assert field_name not in (npc.get("fields") or {}), f"{map_path}:{field_name}"
            assert field_name not in (npc.get("transforms") or {}), f"{map_path}:{field_name}"
            assert field_name in (npc.get("drop") or []), f"{map_path}:{field_name}"
