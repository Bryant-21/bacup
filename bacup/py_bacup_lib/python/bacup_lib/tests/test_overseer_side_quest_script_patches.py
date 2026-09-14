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

SCRIPT_MEMBERS = {
    "MQ_OverseerQuestScript": {"onquestinit"},
    "Fragments:Quests:QF_MQ_Overseer_004E49D9": {"fragment_stage_0005_item_00"},
    "OverseerPersonal_PlayerScript": {
        "onaliasinit",
        "onplayerloadgame",
        "onitemadded",
        "onitemequipped",
        "objectreference.onholotapeplay",
        "onaliasshutdown",
        "onlocationchange",
        "initializeplayeralias",
        "reconcileplayerstate",
        "prepareexistingquestobject",
        "fillquestobjectalias",
        "reconcileholotapeplayregistrations",
        "clearholotapeplayregistrations",
        "registerholotapeplay",
        "unregisterholotapeplay",
        "setstageforownedform",
        "setstageonce",
    },
    "OverseerPersonalEvanCollScript": {"ondeath"},
    "Fragments:Quests:QF_OverseerPersonal_0026AA34": {
        "fragment_stage_0010_item_00",
        "fragment_stage_0015_item_00",
        "fragment_stage_0020_item_00",
        "fragment_stage_0025_item_00",
        "fragment_stage_0030_item_00",
        "fragment_stage_0035_item_00",
        "fragment_stage_0040_item_00",
        "fragment_stage_0045_item_00",
        "fragment_stage_0050_item_00",
        "fragment_stage_0055_item_00",
        "fragment_stage_0060_item_00",
        "fragment_stage_0065_item_00",
        "fragment_stage_0067_item_00",
        "fragment_stage_0070_item_00",
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


def _merged_source(script_name: str) -> str:
    source_path = SOURCE_ROOT / _script_relative_path(script_name, ".psc")
    patch = _script_patch_source(script_name)
    assert source_path.is_file(), source_path
    assert patch is not None
    return _merge_script_method_patches(source_path.read_text(encoding="utf-8"), patch)


@pytest.mark.parametrize("script_name", SCRIPT_MEMBERS)
def test_overseer_side_quest_patches_are_member_only(script_name: str):
    patch = _script_patch_source(script_name)

    assert patch is not None
    assert SCRIPT_MEMBERS[script_name] <= set(_member_names(patch))
    assert not any(
        line.strip().lower().startswith(("scriptname ", "state "))
        for line in patch.splitlines()
    )
    assert not any(
        " property " in f" {line.strip().lower()} " for line in patch.splitlines()
    )


@pytest.mark.parametrize("script_name", SCRIPT_MEMBERS)
def test_overseer_side_quest_merges_are_unique_and_idempotent(script_name: str):
    patch = _script_patch_source(script_name)
    assert patch is not None

    merged = _merged_source(script_name)
    names = _member_names(merged)
    for member_name in SCRIPT_MEMBERS[script_name]:
        assert names.count(member_name) == 1
        assert _member_body(merged, member_name) == _member_body(patch, member_name)
    assert _merge_script_method_patches(merged, patch) == merged


def test_personal_journal_alias_tracks_the_six_bound_journals():
    patch = _script_patch_source("OverseerPersonal_PlayerScript")
    assert patch is not None

    journal_aliases = (
        ("OverseerPersonal_01_VTecAgHolotape", "QuestObject01_VTecAg"),
        ("OverseerPersonal_02_FamilyHolotape", "QuestObject02_Family"),
        ("OverseerPersonal_03_HighSchoolHolotape", "QuestObject03_HighSchool"),
        ("OverseerPersonal_04_UniversityHolotape", "QuestObject04_University"),
        ("OverseerPersonal_05_HouseHolotape", "QuestObject05_House"),
        ("OverseerPersonal_06_MineHolotape", "QuestObject06_Mine"),
    )
    assert "AddInventoryEventFilter(OverseerPersonal_HolotapesList)" in patch
    assert "AddInventoryEventFilter(OverseerPersonalMineKeycard)" in patch
    for journal, alias in journal_aliases:
        assert f"PrepareExistingQuestObject({journal}, {alias})" in patch
        assert f"FillQuestObjectAlias({alias}, akItemReference)" in patch

    create_index = patch.index("myPlayer.PlaceAtMe(holotapeBase")
    force_index = patch.index("targetAlias.ForceRefTo(itemReference)")
    enable_index = patch.index("itemReference.Enable()")
    add_index = patch.index("myPlayer.AddItem(itemReference")
    assert create_index < enable_index < force_index < add_index


def test_personal_player_alias_reconciles_existing_saves_without_reassigning_aliases():
    patch = _script_patch_source("OverseerPersonal_PlayerScript")
    assert patch is not None

    load = _member_body(patch, "onplayerloadgame")
    assert load.index("InitializePlayerAlias()") < load.index("ReconcilePlayerState()")

    reconcile = _member_body(patch, "reconcileplayerstate")
    assert reconcile.count("PrepareExistingQuestObject(") == 6
    assert reconcile.count("SetStageForOwnedForm(") == 7
    assert "myPlayer.GetCurrentLocation()" in reconcile
    assert "SetStage(70)" in reconcile

    fill = _member_body(patch, "fillquestobjectalias")
    assert "targetAlias.GetReference() == None" in fill
    assert "targetAlias.GetReference() != itemReference" not in fill


def test_personal_player_alias_stage_callbacks_follow_source_order():
    patch = _script_patch_source("OverseerPersonal_PlayerScript")
    assert patch is not None

    added = _member_body(patch, "onitemadded")
    equipped = _member_body(patch, "onitemequipped")
    location = _member_body(patch, "onlocationchange")
    for stage in (10, 20, 30, 40, 50, 60, 67):
        assert f"SetStageOnce({stage})" in added
    for stage in (15, 25, 35, 45, 55, 65):
        assert f"SetStageOnce({stage})" in equipped
    assert "SubMTRMountBlairWarehouseBasementLocation.IsChild(akNewLoc)" in location
    assert "SetStageOnce(70)" in location


def test_personal_journal_stages_advance_the_bound_pickup_and_played_values():
    patch = _script_patch_source("Fragments:Quests:QF_OverseerPersonal_0026AA34")
    assert patch is not None

    stages = (
        (10, "OverseerPersonal_Holotape01PickedUp", 15),
        (15, "OverseerPersonal_Holotape01Played", 20),
        (20, "OverseerPersonal_Holotape02PickedUp", 25),
        (25, "OverseerPersonal_Holotape02Played", 30),
        (30, "OverseerPersonal_Holotape03PickedUp", 35),
        (35, "OverseerPersonal_Holotape03Played", 40),
        (40, "OverseerPersonal_Holotape04PickedUp", 45),
        (45, "OverseerPersonal_Holotape04Played", 50),
        (50, "OverseerPersonal_Holotape05PickedUp", 55),
        (55, "OverseerPersonal_Holotape05Played", 60),
        (60, "OverseerPersonal_Holotape06PickedUp", 65),
        (65, "OverseerPersonal_Holotape06Played", 67),
    )
    for stage, actor_value, next_objective in stages:
        body = _member_body(patch, f"fragment_stage_{stage:04}_item_00")
        assert f"playerRef.SetValue({actor_value}, 1.0)" in body
        assert f"SetObjectiveDisplayed({next_objective})" in body

    assert "SetObjectiveDisplayed(70)" in _member_body(
        patch, "fragment_stage_0067_item_00"
    )


def test_personal_finale_spawns_evan_and_completes_only_after_his_death():
    fragment = _script_patch_source("Fragments:Quests:QF_OverseerPersonal_0026AA34")
    death_patch = _script_patch_source("OverseerPersonalEvanCollScript")
    assert fragment is not None
    assert death_patch is not None

    finale = _member_body(fragment, "fragment_stage_0070_item_00")
    assert "Alias_EvanSpawnMarker.GetReference()" in finale
    assert "spawnMarker.PlaceAtMe(OverseerPersonal_Evan) as Actor" in finale
    assert "Alias_Evan.ForceRefTo(evanRef)" in finale

    on_death = _member_body(death_patch, "ondeath")
    assert "owningQuest.IsStageDone(70)" in on_death
    assert "!owningQuest.IsStageDone(100)" in on_death
    assert "owningQuest.SetStage(100)" in on_death


@pytest.mark.parametrize("script_name", SCRIPT_MEMBERS)
def test_full_overseer_side_quest_merges_compile_for_fo4(script_name: str):
    base_source = _fo4_base_source()
    assert base_source is not None, "FO4 base Papyrus sources unavailable"

    result = compile_psc(
        _merged_source(script_name),
        imports=[str(SOURCE_ROOT), str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=str(_script_relative_path(script_name, ".psc")),
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None
