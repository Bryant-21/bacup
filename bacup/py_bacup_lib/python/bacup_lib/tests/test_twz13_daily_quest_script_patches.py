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
QUEST_FRAGMENT = "Fragments:Quests:QF_TWZ13_0010C1E3"

PATCH_MEMBERS = {
    "TWZ13DirtTriggerBoxScript": {"onactivate"},
    QUEST_FRAGMENT: {
        "fragment_stage_0100_item_00",
        "fragment_stage_0200_item_00",
        "fragment_stage_0300_item_00",
        "fragment_stage_0400_item_00",
        "fragment_stage_0500_item_00",
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
def test_twz13_patch_is_member_only_and_complete(
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
def test_twz13_patch_merges_once_and_native_compiles(
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


def test_twz13_dirt_trigger_requires_the_player_stage_and_equipped_shovel():
    patch = _script_patch_source("TWZ13DirtTriggerBoxScript")
    assert patch is not None
    activate = _member_body(patch, "onactivate")

    assert "akActionRef != playerRef" in activate
    assert "!owningQuest.IsStageDone(PrereqStage)" in activate
    assert "playerRef.GetEquippedWeapon() != Shovel" in activate
    assert "TWZ13_NoShovelMsg.Show()" in activate
    assert "owningQuest.SetStage(StageToSet)" in activate


def test_twz13_fragments_restore_the_solo_burial_chain():
    patch = _script_patch_source(QUEST_FRAGMENT)
    assert patch is not None

    started = _member_body(patch, "fragment_stage_0100_item_00")
    collected = _member_body(patch, "fragment_stage_0300_item_00")
    placed = _member_body(patch, "fragment_stage_0400_item_00")
    buried = _member_body(patch, "fragment_stage_0500_item_00")
    completed = _member_body(patch, "fragment_stage_1000_item_00")

    assert 'graveDisplay.ClientPlayAnimation("Start")' in started
    assert "Alias_DirtTriggerBox.TryToDisableNoWait()" in started
    assert "TWZ13_GraveTriggerRef.Enable(False)" in collected
    assert "playerRef.RemoveItem(remainsRef, 1, True)" in placed
    assert 'graveDisplay.ClientPlayAnimation("Remains")' in placed
    assert "Alias_DirtTriggerBox.TryToEnableNoWait()" in placed
    assert (
        "shovelMarker.PlaceAtMe(Shovel, 1, False, False, True)" in placed
    )
    assert "Alias_Shovel." not in placed
    assert 'graveDisplay.ClientPlayAnimation("DirtMound")' in buried
    assert "SetStage(1000)" in buried
    assert "Stop()" in completed
