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
QUEST_FRAGMENT = "Fragments:Quests:QF_SFZ04_Waste_00124487"

PATCH_MEMBERS = {
    "SFZ04_Waste_QuestScript": {"assigncoretoprotectron"},
    "SFZ04_Waste_ProtectronScript": {"ondeath"},
    "SFZ04_Waste_BinScript": {"onactivate"},
    QUEST_FRAGMENT: {
        "fragment_stage_0000_item_00",
        "fragment_stage_0050_item_00",
        "fragment_stage_0100_item_00",
        "fragment_stage_0110_item_00",
        "fragment_stage_0115_item_00",
        "fragment_stage_0120_item_00",
        "fragment_stage_0125_item_00",
        "fragment_stage_0130_item_00",
        "fragment_stage_0135_item_00",
        "fragment_stage_0200_item_00",
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
def test_sfz04_patch_is_member_only_and_complete(
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
def test_sfz04_patch_merges_once_and_native_compiles(
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


def test_sfz04_first_run_uses_alerts_and_repeat_run_skips_it():
    quest = _script_patch_source(QUEST_FRAGMENT)
    assert quest is not None

    startup = _member_body(quest, "fragment_stage_0000_item_00")
    collect = _member_body(quest, "fragment_stage_0100_item_00")

    assert "playerRef.GetValue(SFZ04_Waste_QuestCompletedValue) > 0.0" in startup
    assert "SetStage(100)" in startup
    assert "SFZ04_Recycle_MaintenanceMessage.Start()" in startup
    assert "SetStage(50)" in startup
    assert "SetObjectiveCompleted(50, True)" in collect
    assert "SetObjectiveDisplayed(100, True)" in collect


def test_sfz04_assigns_one_persistent_core_to_each_spawn_group():
    quest_script = _script_patch_source("SFZ04_Waste_QuestScript")
    quest = _script_patch_source(QUEST_FRAGMENT)
    assert quest_script is not None
    assert quest is not None

    assert "CoresInHolding.RemoveRef(coreRef)" in quest_script
    assert "CoresInWorld.AddRef(coreRef)" in quest_script
    assert "protectronRef.AddItem(coreRef, 1, True)" in quest_script

    for stage, collection in (
        ("0115", "Alias_SpawnedProtectron01"),
        ("0125", "Alias_SpawnedProtectron02"),
        ("0135", "Alias_SpawnedProtectron03"),
    ):
        body = _member_body(quest, f"fragment_stage_{stage}_item_00")
        assert f"questScript.AssignCoreToProtectron({collection})" in body


def test_sfz04_protectron_death_and_recycler_are_idempotent():
    protectron = _script_patch_source("SFZ04_Waste_ProtectronScript")
    recycler = _script_patch_source("SFZ04_Waste_BinScript")
    assert protectron is not None
    assert recycler is not None

    assert "akSenderRef.GetItemCount(SFZ04_Waste_Core) == 0" in protectron
    assert "akSenderRef.AddItem(SFZ04_Waste_Core, 1, True)" in protectron
    assert "Alias_SpawnedProtectrons.RemoveRef(akSenderRef)" in protectron

    assert "akActionRef != playerRef" in recycler
    assert "!owningQuest.IsStageDone(200)" in recycler
    assert "owningQuest.IsStageDone(1000)" in recycler
    assert "playerRef.GetItemCount(SFZ04_Waste_Core) < 3" in recycler
    assert "playerRef.RemoveItem(SFZ04_Waste_Core, 3, True)" in recycler
    assert "owningQuest.SetStage(1000)" in recycler


def test_sfz04_completion_marks_future_runs_and_stops_cleanly():
    quest = _script_patch_source(QUEST_FRAGMENT)
    assert quest is not None

    complete = _member_body(quest, "fragment_stage_1000_item_00")

    assert "SetObjectiveCompleted(200, True)" in complete
    assert "playerRef.SetValue(SFZ04_Waste_QuestCompletedValue, 1.0)" in complete
    assert "Stop()" in complete
