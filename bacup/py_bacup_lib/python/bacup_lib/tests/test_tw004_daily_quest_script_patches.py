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
QUEST_SCRIPT = "tw004script"
QUEST_FRAGMENT = "Fragments:Quests:QF_TW004a_0011D76B"

PATCH_MEMBERS = {
    QUEST_SCRIPT: {
        "onquestinit",
        "objectreference.onactivate",
        "actor.onkill",
        "starthunt",
        "selecthunt",
        "completehunttarget",
        "alltargetsbagged",
    },
    QUEST_FRAGMENT: {
        "fragment_stage_0020_item_00",
        "fragment_stage_1000_item_00",
        "fragment_stage_0040_item_00",
        "fragment_stage_0400_item_00",
        "fragment_stage_0100_item_00",
        "fragment_stage_0200_item_00",
        "fragment_stage_0030_item_00",
        "fragment_stage_0300_item_00",
        "fragment_stage_0150_item_00",
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
def test_tw004_patch_is_member_only_and_complete(
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
def test_tw004_patch_merges_once(script_name: str, members: set[str]):
    patch = _script_patch_source(script_name)
    assert patch is not None
    merged = _merged_production_source(script_name)
    names = _member_names(merged)

    for member in members:
        assert names.count(member) == 1
    assert _merge_script_method_patches(merged, patch) == merged


def test_tw004_merged_sources_native_compile(tmp_path: Path):
    merged_quest = _merged_production_source(QUEST_SCRIPT)
    merged_fragment = _merged_production_source(QUEST_FRAGMENT)
    base_source = _fo4_base_source()
    assert base_source is not None, "FO4 base Papyrus sources unavailable"

    patched_imports = tmp_path / "patched_imports"
    patched_imports.mkdir()
    quest_import_path = patched_imports / _script_relative_path(
        QUEST_SCRIPT, ".psc"
    )
    quest_import_path.parent.mkdir(parents=True, exist_ok=True)
    quest_import_path.write_text(merged_quest, encoding="utf-8")

    for script_name, source, imports in (
        (QUEST_SCRIPT, merged_quest, [str(SOURCE_ROOT), str(base_source)]),
        (
            QUEST_FRAGMENT,
            merged_fragment,
            [str(patched_imports), str(SOURCE_ROOT), str(base_source)],
        ),
    ):
        result = compile_psc(
            source,
            imports=imports,
            game="fo4",
            flags=str(base_source / "Institute_Papyrus_Flags.flg"),
            source_path=str(_script_relative_path(script_name, ".psc")),
        )
        diagnostics = "\n".join(str(item) for item in result.diagnostics)
        assert result.ok, diagnostics
        assert result.pex_bytes is not None


def test_tw004_huntmaster_and_player_kill_contract():
    patch = _script_patch_source(QUEST_SCRIPT)
    assert patch is not None

    init = _member_body(patch, "onquestinit")
    activation = _member_body(patch, "objectreference.onactivate")
    player_kill = _member_body(patch, "actor.onkill")
    start_hunt = _member_body(patch, "starthunt")

    assert 'RegisterForRemoteEvent(playerRef, "OnKill")' in init
    assert 'RegisterForRemoteEvent(huntmasterAlias.GetReference(), "OnActivate")' in init
    assert "akActivator != Game.GetPlayer()" in activation
    assert "StartHunt()" in activation
    assert "akVictim.HasKeyword(HuntSelected[huntIndex].raceKeyword)" in player_kill
    assert "SetStage(HuntSelected[huntIndex].StageToSet)" in player_kill
    assert "SelectHunt(0, HuntOptions0)" in start_hunt
    assert "SelectHunt(1, HuntOptions1)" in start_hunt
    assert "SelectHunt(2, HuntOptions2)" in start_hunt
    assert "SetStage(HuntStartStage)" in start_hunt


def test_tw004_fragment_progression_and_completion_contract():
    patch = _script_patch_source(QUEST_FRAGMENT)
    assert patch is not None

    stage_100 = _member_body(patch, "fragment_stage_0100_item_00")
    stage_150 = _member_body(patch, "fragment_stage_0150_item_00")
    stage_200 = _member_body(patch, "fragment_stage_0200_item_00")
    stage_300 = _member_body(patch, "fragment_stage_0300_item_00")
    stage_400 = _member_body(patch, "fragment_stage_0400_item_00")
    stage_1000 = _member_body(patch, "fragment_stage_1000_item_00")

    assert "TW004.SetObjectiveDisplayed(100)" in stage_100
    assert "TW004a_Intro.Start()" in stage_100
    assert "TW004.SetObjectiveCompleted(100)" in stage_150
    assert "TW004.SetObjectiveDisplayed(200)" in stage_150
    assert "TW004.SetObjectiveDisplayed(300)" in stage_150
    assert "TW004.SetObjectiveDisplayed(400)" in stage_150
    assert "hunt.CompleteHuntTarget(0)" in stage_200
    assert "hunt.CompleteHuntTarget(1)" in stage_300
    assert "hunt.CompleteHuntTarget(2)" in stage_400
    assert "playerRef.SetValue(TW004status, 2.0)" in stage_1000
