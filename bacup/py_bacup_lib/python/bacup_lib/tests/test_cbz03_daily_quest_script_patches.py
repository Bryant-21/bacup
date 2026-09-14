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
SQUATTER_ALIAS = "Quests:CBZ03_Deputy:SquatterScript"
QUEST_FRAGMENT = "Fragments:Quests:QF_CBZ03_Deputy_00116532"
PRE_MISC_FRAGMENT = (
    "Fragments:Quests:QF_CBZ03_Pre_MiscQuestObject_0050E620"
)

PATCH_MEMBERS = {
    QUEST_FRAGMENT: {
        "fragment_Stage_0050_Item_00",
        "fragment_Stage_0100_Item_00",
        "fragment_Stage_0300_Item_00",
    },
    PRE_MISC_FRAGMENT: {
        "fragment_Stage_0010_Item_00",
        "fragment_Stage_0100_Item_00",
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
def test_cbz03_patch_is_member_only_and_complete(
    script_name: str, members: set[str]
):
    patch = _script_patch_source(script_name)

    assert patch is not None
    assert set(_member_names(patch)) == {member.lower() for member in members}
    assert not any(
        line.strip().lower().startswith(("scriptname ", "state "))
        for line in patch.splitlines()
    )
    assert not any(
        " property " in f" {line.strip().lower()} "
        for line in patch.splitlines()
    )


@pytest.mark.parametrize(("script_name", "members"), PATCH_MEMBERS.items())
def test_cbz03_patch_merges_once_and_native_compiles(
    script_name: str, members: set[str]
):
    patch = _script_patch_source(script_name)
    assert patch is not None
    merged = _merged_production_source(script_name)
    names = _member_names(merged)

    for member in members:
        assert names.count(member.lower()) == 1
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


def test_cbz03_does_not_override_the_base_collection_death_handler():
    assert _script_patch_source(SQUATTER_ALIAS) is None


def test_cbz03_chief_objectives_marker_and_shutdown_form_a_complete_chain():
    quest = _script_patch_source(QUEST_FRAGMENT)
    pre_misc = _script_patch_source(PRE_MISC_FRAGMENT)
    assert quest is not None
    assert pre_misc is not None

    start = _member_body(quest, "fragment_stage_0050_item_00")
    assignment = _member_body(quest, "fragment_stage_0100_item_00")
    shutdown = _member_body(quest, "fragment_stage_0300_item_00")
    pre_start = _member_body(pre_misc, "fragment_stage_0010_item_00")
    pre_shutdown = _member_body(pre_misc, "fragment_stage_0100_item_00")

    assert "SetObjectiveDisplayed(50)" in start
    assert "SetObjectiveCompleted(50)" in assignment
    assert "SetObjectiveDisplayed(100)" in assignment
    assert "CBZ03_Pre_MiscQuestObjective.SetStage(100)" in assignment
    assert "Alias_MapMarker.GetReference().AddToMap()" in assignment
    assert "Stop()" in shutdown
    assert "SetObjectiveDisplayed(10)" in pre_start
    assert "SetObjectiveCompleted(10)" in pre_shutdown
    assert "Stop()" in pre_shutdown
