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
ROOT_SCRIPT = "NWOT_Beneath_QuestScript"
FRAGMENT_SCRIPT = "Fragments:Quests:QF_NWOT_Beneath_00664AD8"
ACCEPTANCE_SCRIPT = "Fragments:TopicInfos:TIF_NWOT_Pete_Dialogue_00677FE6"
PATCH_MEMBERS = {
    ROOT_SCRIPT: {"onquestinit", "onstageset"},
    FRAGMENT_SCRIPT: {
        "fragment_stage_0100_item_00",
        "fragment_stage_0200_item_00",
        "fragment_stage_0300_item_00",
        "fragment_stage_0350_item_00",
        "fragment_stage_0400_item_00",
        "fragment_stage_1000_item_00",
        "fragment_stage_1001_item_00",
        "fragment_stage_1002_item_00",
        "fragment_stage_1003_item_00",
        "fragment_stage_1100_item_00",
    },
    ACCEPTANCE_SCRIPT: {"fragment_end"},
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
        if name == member_name.casefold()
    )
    return "\n".join(lines[start : end + 1])


def _merged_source(script_name: str) -> str:
    source_path = SOURCE_ROOT / _script_relative_path(script_name, ".psc")
    patch = _script_patch_source(script_name)
    assert source_path.is_file(), source_path
    assert patch is not None
    return _merge_script_method_patches(source_path.read_text(encoding="utf-8"), patch)


@pytest.mark.parametrize(("script_name", "members"), PATCH_MEMBERS.items())
def test_nwot_beneath_patches_are_member_only(
    script_name: str, members: set[str]
) -> None:
    patch = _script_patch_source(script_name)

    assert patch is not None
    assert set(_member_names(patch)) == members
    assert not any(
        line.strip().casefold().startswith(("scriptname ", "state "))
        for line in patch.splitlines()
    )
    assert not any(
        " property " in f" {line.strip().casefold()} " for line in patch.splitlines()
    )


@pytest.mark.parametrize(("script_name", "members"), PATCH_MEMBERS.items())
def test_nwot_beneath_production_merges_are_unique_idempotent_and_compile(
    script_name: str, members: set[str]
) -> None:
    patch = _script_patch_source(script_name)
    assert patch is not None
    merged = _merged_source(script_name)
    merged_members = _member_names(merged)

    for member in members:
        assert merged_members.count(member) == 1
        assert _member_body(merged, member) == _member_body(patch, member)
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


def test_quest_init_spawns_each_bound_item_once_without_stale_collection() -> None:
    patch = _script_patch_source(ROOT_SCRIPT)
    assert patch is not None
    init = _member_body(patch, "onquestinit")

    assert init.count("PlaceAtMe(Form_Holotape") == 1
    assert init.count("PlaceAtMe(Form_FilterSupplies") == 1
    assert "Alias_Holotape.ForceRefTo(holotapeRef)" in init
    assert "holotapeContainer.AddItem(holotapeRef" in init
    assert "Alias_Supplies.ForceRefTo(suppliesRef)" in init
    assert init.index("holotapeRef == None") < init.index("PlaceAtMe(Form_Holotape")
    assert init.index("suppliesRef == None") < init.index(
        "PlaceAtMe(Form_FilterSupplies"
    )
    assert "Alias_SupplySpawnPoints" not in init


def test_stage_coordinator_restores_conditional_state_and_camera_feedback() -> None:
    patch = _script_patch_source(ROOT_SCRIPT)
    assert patch is not None
    stage = _member_body(patch, "onstageset")

    assert "auiStageID == 200" in stage
    assert "CamShakeSpell.Cast(player, player)" in stage
    assert "auiStageID == 300" in stage
    assert "holotapeFound = 1" in stage
    assert "auiStageID == 350" in stage
    assert "suppliesFound = 1" in stage
    assert "player.SetValue(NWOT_Beneath_HasSupplies, 1.0)" in stage
    assert "player.SetValue(NWOT_Pete_HasBeatenQuest, 1.0)" in stage


def test_objectives_follow_the_bound_quest_stage_contract() -> None:
    patch = _script_patch_source(FRAGMENT_SCRIPT)
    assert patch is not None

    stage_100 = _member_body(patch, "fragment_stage_0100_item_00")
    assert "MineshaftMapMarker.AddToMap()" in stage_100
    assert "SetObjectiveDisplayed(5)" in stage_100

    stage_200 = _member_body(patch, "fragment_stage_0200_item_00")
    assert "SetObjectiveCompleted(5)" in stage_200
    assert "SetObjectiveDisplayed(10)" in stage_200
    assert "SetObjectiveDisplayed(15)" in stage_200

    stage_300 = _member_body(patch, "fragment_stage_0300_item_00")
    assert "SetObjectiveCompleted(10)" in stage_300
    assert "SetObjectiveDisplayed(20)" in stage_300
    assert "SetObjectiveCompleted(15)" in _member_body(
        patch, "fragment_stage_0350_item_00"
    )


def test_hand_in_fragments_record_outcomes_and_remove_exact_aliases() -> None:
    patch = _script_patch_source(FRAGMENT_SCRIPT)
    assert patch is not None

    holotape_only = _member_body(patch, "fragment_stage_1000_item_00")
    assert "SetValue(HandedInOneAV, 1.0)" in holotape_only
    assert "RemoveItem(holotapeRef, 1, True)" in holotape_only
    assert "RemoveItem(suppliesRef" not in holotape_only

    both = _member_body(patch, "fragment_stage_1001_item_00")
    assert "SetValue(HandedInBothAV, 1.0)" in both
    assert "RemoveItem(holotapeRef, 1, True)" in both
    assert "RemoveItem(suppliesRef, 1, True)" in both

    parts_only = _member_body(patch, "fragment_stage_1002_item_00")
    assert "SetValue(HandedInOneAV, 1.0)" in parts_only
    assert "RemoveItem(suppliesRef, 1, True)" in parts_only
    assert "RemoveItem(holotapeRef" not in parts_only

    neither = _member_body(patch, "fragment_stage_1003_item_00")
    assert "SetValue(HandedInNoneAV, 1.0)" in neither


def test_acceptance_fragment_uses_bound_dialogue_contract_once() -> None:
    patch = _script_patch_source(ACCEPTANCE_SCRIPT)
    assert patch is not None
    fragment = _member_body(patch, "fragment_end")

    assert "GetOwningQuest() as NWOT_Pete_DialogueScript" in fragment
    assert fragment.count("dialogueQuest.BeginQuestStage") == 3
    assert "!dialogueQuest.NWOT_Beneath.IsStageDone(" in fragment
    assert "dialogueQuest.NWOT_Beneath.SetStage(" in fragment
    assert "SetStage(100)" not in fragment
