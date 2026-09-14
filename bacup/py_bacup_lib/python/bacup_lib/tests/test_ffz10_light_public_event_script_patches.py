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
QUEST_FRAGMENT = "Fragments:Quests:QF_FFZ10_Light_00187531"
DEPOSIT_HELPER = "Quests:_Default:AliasOnActivateRemoveMultipleItems"

PATCH_MEMBERS = {
    "FFZ10_Light_QuestScript": {
        "startshutdowntimer",
        "cancelshutdowntimer",
        "ontimer",
    },
    "FFZ10_Light_ContainerScript": {
        "resetdepositedfluid",
        "onaliasinit",
        "onaliasreset",
        "onaliasshutdown",
        "onitemadded",
    },
    "FFZ10_Light_MothmanAliasScript": {
        "clearcommunionstate",
        "onaliasinit",
        "onaliasreset",
        "onaliasshutdown",
        "onactivate",
    },
    "DefaultSimpleRespawnScript": {
        "canspawnactors",
        "removedeadactors",
        "spawnmissingactors",
        "armrespawntimer",
        "beginspawning",
        "stopspawning",
        "onquestinit",
        "onstageset",
        "ontimer",
        "onquestshutdown",
    },
    "DefaultQuestSetStageOnTimerScript": {
        "armstagetimer",
        "onquestinit",
        "onstageset",
        "ontimer",
        "onquestshutdown",
    },
    DEPOSIT_HELPER: {
        "resetrequireditemcounts",
        "onaliasinit",
        "onaliasreset",
        "onaliasshutdown",
        "onactivate",
    },
    QUEST_FRAGMENT: {
        "seteventreferenceenabled",
        "reseteventobjectives",
        "addplayertocommunecollection",
        "removeplayerfromcommunecollection",
        "seteventworldenabled",
        "shutdownevent",
        "fragment_stage_0005_item_00",
        "fragment_stage_0010_item_00",
        "fragment_stage_0020_item_00",
        "fragment_stage_0030_item_00",
        "fragment_stage_0100_item_00",
        "fragment_stage_0150_item_00",
        "fragment_stage_0200_item_00",
        "fragment_stage_0300_item_00",
        "fragment_stage_1500_item_00",
        "fragment_stage_1600_item_00",
        "fragment_stage_9991_item_00",
    },
}

STAGE_FRAGMENT_MEMBERS = {
    "fragment_stage_0005_item_00",
    "fragment_stage_0010_item_00",
    "fragment_stage_0020_item_00",
    "fragment_stage_0030_item_00",
    "fragment_stage_0100_item_00",
    "fragment_stage_0150_item_00",
    "fragment_stage_0200_item_00",
    "fragment_stage_0300_item_00",
    "fragment_stage_1500_item_00",
    "fragment_stage_1600_item_00",
    "fragment_stage_9991_item_00",
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
def test_ffz10_light_patch_is_member_only_and_complete(
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
def test_ffz10_light_patch_merges_once_and_native_compiles(
    script_name: str, members: set[str], tmp_path: Path
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
    overlay_source = tmp_path / "Source"
    for patched_script_name in PATCH_MEMBERS:
        overlay_path = overlay_source / _script_relative_path(
            patched_script_name, ".psc"
        )
        overlay_path.parent.mkdir(parents=True, exist_ok=True)
        overlay_path.write_text(
            _merged_production_source(patched_script_name), encoding="utf-8"
        )

    result = compile_psc(
        merged,
        imports=[str(overlay_source), str(SOURCE_ROOT), str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=str(_script_relative_path(script_name, ".psc")),
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, f"{script_name}:\n{diagnostics}"
    assert result.pex_bytes is not None


def test_ffz10_light_namespaced_patches_use_source_path_casing():
    fragment_path = str(_script_relative_path(QUEST_FRAGMENT, ".psc")).replace(
        "\\", "/"
    )
    helper_path = str(_script_relative_path(DEPOSIT_HELPER, ".psc")).replace(
        "\\", "/"
    )

    assert fragment_path == "Fragments/Quests/QF_FFZ10_Light_00187531.psc"
    assert helper_path == "Quests/_Default/AliasOnActivateRemoveMultipleItems.psc"
    assert _script_patch_source(QUEST_FRAGMENT) is not None
    assert _script_patch_source(DEPOSIT_HELPER) is not None


def test_ffz10_light_fragment_restores_the_single_player_event_chain():
    patch = _script_patch_source(QUEST_FRAGMENT)
    assert patch is not None
    members = set(_member_names(patch))

    assert members & STAGE_FRAGMENT_MEMBERS == STAGE_FRAGMENT_MEMBERS
    assert "fragment_stage_10000_item_00" not in members

    start = _member_body(patch, "fragment_stage_0005_item_00")
    arrival = _member_body(patch, "fragment_stage_0010_item_00")
    collection = _member_body(patch, "fragment_stage_0030_item_00")
    deposited = _member_body(patch, "fragment_stage_0200_item_00")
    communed = _member_body(patch, "fragment_stage_0300_item_00")
    shutdown = _member_body(patch, "shutdownevent")

    assert "ResetEventObjectives()" in start
    assert "SetObjectiveDisplayed(1, True, True)" in start
    assert "SetObjectiveCompleted(1, True)" in arrival
    assert "SetStage(20)" in arrival
    assert "SetObjectiveDisplayed(5, True, True)" in collection
    assert "SetObjectiveDisplayed(10, True, True)" in collection
    assert "SetObjectiveCompleted(5, True)" in deposited
    assert "SetObjectiveCompleted(10, True)" in deposited
    assert "SetObjectiveDisplayed(20, True, True)" in deposited
    assert "FFZ10_Light_Global.SetValue(30.0)" in deposited
    assert "AddPlayerToCommuneCollection()" in deposited
    assert "eventScript.StartShutdownTimer()" in communed
    assert "Stop()" in shutdown
    assert "FFZ10_Light_LighthouseActivatorRef" not in patch


def test_ffz10_light_deposit_helper_is_cumulative_and_caps_each_requirement():
    patch = _script_patch_source(DEPOSIT_HELPER)
    assert patch is not None
    activate = _member_body(patch, "onactivate")

    assert "akActionRef != Game.GetPlayer()" in activate
    assert "amountNeeded = requiredCount - requiredItem.CurrentCount" in activate
    assert "amountToRemove > amountNeeded" in activate
    assert (
        "akActionRef.RemoveItem(requiredItem.ItemToRemove, amountToRemove, True, depositContainer)"
        in activate
    )
    assert "amountRemoved = countBefore - akActionRef.GetItemCount" in activate
    assert "requiredItem.CurrentCount += amountRemoved" in activate
    assert "requiredItem.CurrentCount = requiredCount" in activate
    assert "OwningQuest.SetStage(requiredItem.StageToSet)" in activate


def test_ffz10_light_container_uses_the_exact_fluid_filter_lifecycle():
    patch = _script_patch_source("FFZ10_Light_ContainerScript")
    assert patch is not None
    initialized = _member_body(patch, "onaliasinit")
    reset = _member_body(patch, "onaliasreset")
    shutdown = _member_body(patch, "onaliasshutdown")
    added = _member_body(patch, "onitemadded")

    assert initialized.count("AddInventoryEventFilter(BiolumFluid01)") == 1
    assert initialized.count("RemoveAllInventoryEventFilters()") == 1
    assert initialized.index("RemoveAllInventoryEventFilters()") < initialized.index(
        "ResetDepositedFluid()"
    )
    assert initialized.index("ResetDepositedFluid()") < initialized.index(
        "AddInventoryEventFilter(BiolumFluid01)"
    )

    assert reset.count("RemoveAllInventoryEventFilters()") == 1
    assert reset.count("AddInventoryEventFilter(BiolumFluid01)") == 1
    assert reset.index("RemoveAllInventoryEventFilters()") < reset.index(
        "ResetDepositedFluid()"
    )
    assert reset.index("ResetDepositedFluid()") < reset.index(
        "AddInventoryEventFilter(BiolumFluid01)"
    )

    assert "AddInventoryEventFilter(None)" not in patch
    assert ".AddInventoryEventFilter(" not in patch
    assert "RemoveAllInventoryEventFilters()" in shutdown
    assert "BiolumFluid01 == None" in added
    assert added.index("owningQuest.SetStage(200)") < added.index(
        "RemoveAllInventoryEventFilters()"
    )


def test_ffz10_light_respawn_loop_is_bounded_and_stops_at_deposit_stage():
    patch = _script_patch_source("DefaultSimpleRespawnScript")
    assert patch is not None
    spawn = _member_body(patch, "spawnmissingactors")
    timer = _member_body(patch, "ontimer")
    stages = _member_body(patch, "onstageset")

    assert "attemptCount < desiredCount" in spawn
    assert "RespawnCollection.AddRef(spawnedActor)" in spawn
    assert "While True" not in patch
    assert "RespawnTimerResetCount += 1" in timer
    assert "RespawnTimerResetCount < RespawnTimerMaxResetCount" in timer
    assert "auiStageID == SpawnActorsStage" in stages
    assert "auiStageID == StopSpawningStage" in stages
    assert "StopSpawning()" in stages


def test_ffz10_light_stage_timer_fails_an_abandoned_event_locally():
    patch = _script_patch_source("DefaultQuestSetStageOnTimerScript")
    fragment_patch = _script_patch_source(QUEST_FRAGMENT)
    assert patch is not None
    assert fragment_patch is not None
    init = _member_body(patch, "onquestinit")
    stages = _member_body(patch, "onstageset")
    timer = _member_body(patch, "ontimer")
    shutdown = _member_body(patch, "onquestshutdown")
    timeout = _member_body(fragment_patch, "fragment_stage_0150_item_00")

    assert "If StartTimerOnInit" in init
    assert "auiStageID == StageToStartTimer" in stages
    assert "CancelTimer(87531)" in patch
    assert "StartTimer(delay, 87531)" in patch
    assert "IsRunning()" in timer
    assert "!IsStageDone(StageToSetOnTimerEnd)" in timer
    assert "SetStage(StageToSetOnTimerEnd)" in timer
    assert "CancelTimer(87531)" in shutdown
    assert "SetStage(1500)" in timeout


def test_ffz10_light_mothman_commune_grants_once_then_advances():
    patch = _script_patch_source("FFZ10_Light_MothmanAliasScript")
    assert patch is not None
    activate = _member_body(patch, "onactivate")

    assert "akActionRef != playerRef" in activate
    assert "owningQuest.IsStageDone(200)" in activate
    assert "owningQuest.IsStageDone(300)" in activate
    assert "PlayersCanCommune.RemoveRef(playerRef)" in activate
    assert "PlayersAlreadyCommuned.AddRef(playerRef)" in activate
    assert "playerRef.AddKeyword(FFZ10_Light_AlreadyCommunedKeyword)" in activate
    assert "playerRef.AddSpell(crXPBonusSpell, False)" in activate
    assert "playerRef.AddToFaction(FFZ10_Light_WiseMothmanFaction)" in activate
    assert "playerRef.SetValue(FFZ10_Light_MothmanFriend, 1.0)" in activate
    assert "owningQuest.SetStage(300)" in activate


def test_ffz10_light_patches_do_not_reintroduce_online_runtime_calls():
    combined = "\n".join(
        _script_patch_source(script_name) or "" for script_name in PATCH_MEMBERS
    )

    for unsupported in (
        "SQ_Master.",
        "RegisterForPlayer",
        "GetCurrentPlayer",
        "InstanceOwner",
        "AddEventScore",
        "IncludeTeammates",
        "SendStoryEvent",
    ):
        assert unsupported not in combined
