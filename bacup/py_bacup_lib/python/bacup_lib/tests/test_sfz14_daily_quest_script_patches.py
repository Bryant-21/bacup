from __future__ import annotations

from pathlib import Path

import pytest

from bacup_lib.tests.test_terminal_fragment_script_patches import _fo4_base_source
from bacup_lib.workflows.unified import (
    _iter_top_level_papyrus_members,
    _merge_script_method_patches,
    _script_patch_source,
    _script_relative_path,
)
from creation_lib.pex.native_runtime import compile_psc


REPO_ROOT = Path(__file__).resolve().parents[5]
SOURCE_ROOT = REPO_ROOT / "mods" / "SeventySix" / "Scripts" / "Source" / "User"
QUEST_FRAGMENT = "Fragments:Quests:QF_SFZ14_Bomb_000490B6"

PATCH_MEMBERS = {
    "SFZ14_Bomb_QuestScript": {
        "preparebombs",
        "getauthoredbombmarker",
        "clearbombtracking",
    },
    QUEST_FRAGMENT: {
        "fragment_stage_0010_item_00",
        "fragment_stage_0050_item_00",
        "fragment_stage_0060_item_00",
        "fragment_stage_0075_item_00",
        "fragment_stage_0100_item_00",
        "fragment_stage_0300_item_00",
        "fragment_stage_1000_item_00",
    },
}


def _member_names(source: str) -> list[str]:
    return [
        name
        for _kind, name, _start, _end in _iter_top_level_papyrus_members(
            source.splitlines()
        )
    ]


def _member_body(source: str, member_name: str) -> str:
    lines = source.splitlines()
    start, end = next(
        (start, end)
        for _kind, name, start, end in _iter_top_level_papyrus_members(lines)
        if name == member_name.lower()
    )
    return "\n".join(lines[start : end + 1])


def _merged_production_source(script_name: str) -> str:
    source_path = SOURCE_ROOT / _script_relative_path(script_name, ".psc")
    patch = _script_patch_source(script_name)
    assert source_path.is_file(), source_path
    assert patch is not None
    return _merge_script_method_patches(
        source_path.read_text(encoding="utf-8"), patch
    )


@pytest.mark.parametrize(("script_name", "members"), PATCH_MEMBERS.items())
def test_sfz14_patch_is_member_only_and_complete(
    script_name: str, members: set[str]
):
    patch = _script_patch_source(script_name)

    assert patch is not None
    assert set(_member_names(patch)) == members
    assert not any(
        line.strip().lower().startswith(("scriptname ", "state "))
        for line in patch.splitlines()
    )
    assert not any(
        " property " in f" {line.strip().lower()} " for line in patch.splitlines()
    )


@pytest.mark.parametrize(("script_name", "members"), PATCH_MEMBERS.items())
def test_sfz14_patch_merges_once_and_native_compiles(
    script_name: str, members: set[str]
):
    patch = _script_patch_source(script_name)
    assert patch is not None
    merged = _merged_production_source(script_name)
    names = _member_names(merged)

    for member in members:
        assert names.count(member) == 1
    assert _merge_script_method_patches(merged, patch) == merged

    base_source = _fo4_base_source()
    assert base_source is not None, "FO4 base Papyrus sources unavailable"
    result = compile_psc(
        merged,
        imports=[str(SOURCE_ROOT), str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=str(_script_relative_path(script_name, ".psc")),
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None


def test_sfz14_cross_script_contract_native_compiles(tmp_path: Path):
    patched_source_root = tmp_path / "User"
    patched_source_root.mkdir()
    main_script = _merged_production_source("SFZ14_Bomb_QuestScript")
    (patched_source_root / "SFZ14_Bomb_QuestScript.psc").write_text(
        main_script, encoding="utf-8"
    )

    base_source = _fo4_base_source()
    assert base_source is not None, "FO4 base Papyrus sources unavailable"
    quest = _merged_production_source(QUEST_FRAGMENT)
    result = compile_psc(
        quest,
        imports=[str(patched_source_root), str(SOURCE_ROOT), str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=str(_script_relative_path(QUEST_FRAGMENT, ".psc")),
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None


def test_sfz14_first_completion_and_repeat_entry_stay_distinct():
    quest = _script_patch_source(QUEST_FRAGMENT)
    assert quest is not None

    startup = _member_body(quest, "fragment_stage_0010_item_00")
    completion = _member_body(quest, "fragment_stage_1000_item_00")

    assert "GetValue(SFZ14_Bomb_QuestCompletedValue) > 0.0" in startup
    assert "SetStage(50)" in startup
    assert "SetStage(60)" in startup
    assert "GetValue(SFZ14_Bomb_QuestCompletedValue) <= 0.0" in completion
    assert "SetValue(SFZ14_Bomb_QuestCompletedValue, 1.0)" in completion


def test_sfz14_places_only_the_selected_bombs_and_sets_the_pickup_threshold():
    quest_script = _script_patch_source("SFZ14_Bomb_QuestScript")
    quest = _script_patch_source(QUEST_FRAGMENT)
    assert quest_script is not None
    assert quest is not None

    prepare = _member_body(quest_script, "preparebombs")
    recover = _member_body(quest, "fragment_stage_0100_item_00")

    assert "BombsWanted = Utility.RandomInt(3, 5)" in prepare
    assert "BombsToUse[index].IntAssigned <= BombsWanted" in prepare
    assert "bombRef.MoveTo(markerRef)" in prepare
    assert "bombRef.AddKeyword(SFZ14_Bomb_ChosenBombKeyword)" in prepare
    assert "BombsChosen.AddRef(bombRef)" in prepare
    assert "BombsWanted >= 3" in prepare
    assert "inventoryManager.SetRequiredAmount(BombsWanted)" in prepare
    assert "questScript.PrepareBombs()" in recover


def test_sfz14_completion_preserves_bombs_without_leaking_daily_tracking():
    quest_script = _script_patch_source("SFZ14_Bomb_QuestScript")
    quest = _script_patch_source(QUEST_FRAGMENT)
    assert quest_script is not None
    assert quest is not None

    cleanup = _member_body(quest_script, "clearbombtracking")
    recovered = _member_body(quest, "fragment_stage_0300_item_00")

    assert "BombsChosen.RemoveRef(bombRef)" in cleanup
    assert "bombRef.ResetKeyword(SFZ14_Bomb_ChosenBombKeyword)" in cleanup
    assert "RemoveItem" not in cleanup
    assert "questScript.ClearBombTracking()" in recovered
    assert "SetStage(1000)" in recovered
