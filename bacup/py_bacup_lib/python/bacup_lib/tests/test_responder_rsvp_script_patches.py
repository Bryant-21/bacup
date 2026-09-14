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
RSVP01_FRAGMENT = "Fragments:Quests:QF_RSVP01_Water_003B32DC"
RSVP02_FRAGMENT = "Fragments:Quests:QF_RSVP02_Hunger_003B32DA"

PATCH_MEMBERS = {
    RSVP01_FRAGMENT: {
        "fragment_stage_0010_item_00",
        "fragment_stage_0300_item_00",
        "fragment_stage_0310_item_00",
        "fragment_stage_0320_item_00",
        "fragment_stage_0400_item_00",
        "fragment_stage_0500_item_00",
        "fragment_stage_0600_item_00",
        "fragment_stage_0605_item_00",
        "fragment_stage_0610_item_00",
        "fragment_stage_0700_item_00",
        "fragment_stage_0800_item_00",
        "fragment_stage_0900_item_00",
        "fragment_stage_1000_item_00",
        "fragment_stage_8999_item_00",
        "fragment_stage_9000_item_00",
        "fragment_stage_9500_item_00",
        "fragment_stage_9999_item_00",
    },
    RSVP02_FRAGMENT: {
        "fragment_stage_0000_item_00",
        "fragment_stage_1000_item_00",
        "fragment_stage_1200_item_00",
        "fragment_stage_2000_item_00",
        "fragment_stage_2100_item_00",
        "fragment_stage_2200_item_00",
        "fragment_stage_3000_item_00",
        "fragment_stage_6000_item_00",
        "fragment_stage_7000_item_00",
        "fragment_stage_9000_item_00",
        "fragment_stage_9999_item_00",
        "registerribeyesubstitute",
        "tryprepareribeyesubstitute",
        "objectreference.onitemadded",
    },
    "RSVP00_OnContainerChangedSetAV": {"oncontainerchanged"},
    "RSVP00_OnItemCraftedSetStage": {
        "onaliasinit",
        "onplayerloadgame",
        "onitemadded",
        "reconcileunsupportedrecipegate",
        "onaliasshutdown",
        "registerrecipegatefilters",
    },
    "RSVP01_PlayerCollectWater": {
        "onaliasinit",
        "onplayerloadgame",
        "reconcileboiledwatergate",
        "objectreference.onactivate",
        "onitemadded",
        "onaliasshutdown",
    },
    "AliasOnPlayerHolotapeSetAV": {"onholotapeplay"},
    "RSVP02_OnTerminalEnterSetAV": {"onmenuitemrun"},
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
    return _merge_script_method_patches(
        source_path.read_text(encoding="utf-8"), patch
    )


@pytest.mark.parametrize(("script_name", "members"), PATCH_MEMBERS.items())
def test_rsvp_patch_member_contract_is_exact(
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
        " property " in f" {line.strip().casefold()} "
        for line in patch.splitlines()
    )


@pytest.mark.parametrize(("script_name", "members"), PATCH_MEMBERS.items())
def test_rsvp_production_merge_is_unique_idempotent_and_compiles(
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


def test_rsvp01_restores_sample_analysis_donation_and_handoff() -> None:
    patch = _script_patch_source(RSVP01_FRAGMENT)
    assert patch is not None

    start = _member_body(patch, "fragment_stage_0010_item_00")
    river = _member_body(patch, "fragment_stage_0605_item_00")
    well = _member_body(patch, "fragment_stage_0610_item_00")
    analyze = _member_body(patch, "fragment_stage_0800_item_00")
    complete = _member_body(patch, "fragment_stage_9000_item_00")

    assert "SetStage(300)" in start
    assert "SetValue(pRSVP00_AV_StartedRSVP01, 1.0)" in start
    assert "SetValue(pRSVP01_AV_RiverSampleCollected, 1.0)" in river
    assert "GetValue(PRSVP01_AV_WellSampleCollected) > 0.0" in river
    assert "SetValue(PRSVP01_AV_WellSampleCollected, 1.0)" in well
    assert "GetValue(pRSVP01_AV_RiverSampleCollected) > 0.0" in well
    assert "SetValue(pRSVP01_AV_AnalyzedSample, 1.0)" in analyze
    assert "GetItemCount(pWaterBoiled) == 0" in analyze
    assert "AddItem(pWaterBoiled, 1, False)" in analyze
    assert "GetItemCount(pWaterBoiled) > 0" in analyze
    assert "SetStage(900)" in analyze
    assert "SetValue(AV_QuestDone, 1.0)" in complete
    assert "SetValue(RSVP02_AV_QuestStarted, 1.0)" in complete
    assert "AddItem(" not in complete
    assert "CompleteQuest(" not in complete


def test_rsvp02_restores_delbert_cooking_and_responder_handoff() -> None:
    patch = _script_patch_source(RSVP02_FRAGMENT)
    assert patch is not None

    start = _member_body(patch, "fragment_stage_0000_item_00")
    terminal = _member_body(patch, "fragment_stage_2100_item_00")
    prepare = _member_body(patch, "fragment_stage_2200_item_00")
    cooked = _member_body(patch, "fragment_stage_3000_item_00")
    complete = _member_body(patch, "fragment_stage_9000_item_00")
    shutdown = _member_body(patch, "fragment_stage_9999_item_00")
    register = _member_body(patch, "registerribeyesubstitute")
    substitute = _member_body(patch, "tryprepareribeyesubstitute")
    item_added = _member_body(patch, "objectreference.onitemadded")

    assert "GetValue(pRSVP00_AV_foundDelbert) > 0.0" in start
    assert "SetStage(1200)" in start
    assert "SetStage(1000)" in start
    assert "SetStage(2200)" in terminal
    assert "RegisterRibeyeSubstitute()" in prepare
    assert "TryPrepareRibeyeSubstitute()" in prepare
    assert "RemoveItem(pBrahminMeat, 1, True)" in substitute
    assert "RemoveItem(pC_Wood, 1, True)" in substitute
    assert "AddItem(pBrahminMeatCooked, 1, False)" in substitute
    assert "SetStage(3000)" in substitute
    assert "akSender == playerRef" in item_added
    assert "akBaseItem == pBrahminMeat" in item_added
    assert "akBaseItem == pC_Wood" in item_added
    assert "akBaseItem == pBrahminMeatCooked" in item_added
    assert 'UnregisterForRemoteEvent(playerRef, "OnItemAdded")' in cooked
    assert "RemoveAllInventoryEventFilters()" in cooked
    assert "RemoveAllInventoryEventFilters()" in shutdown
    assert "RemoveAllInventoryEventFilters()" in register
    for exact_item in ("pBrahminMeat", "pC_Wood", "pBrahminMeatCooked"):
        assert f"AddInventoryEventFilter({exact_item})" in register
        assert register.index("RemoveAllInventoryEventFilters()") < register.index(
            f"AddInventoryEventFilter({exact_item})"
        )
    assert register.index(
        'UnregisterForRemoteEvent(playerRef, "OnItemAdded")'
    ) < register.index('RegisterForRemoteEvent(playerRef, "OnItemAdded")')
    assert "SetStage(6000)" in cooked
    assert "SetValue(pRSVP02_AV_Quest_Done, 1.0)" in complete
    assert "SetValue(pRSVP00_AV_isVolunteer, 1.0)" in complete
    assert "SetValue(pRS01B_Contact_Started, 1.0)" in complete
    assert "AddItem(" not in complete
    assert "CompleteQuest(" not in complete


def test_rsvp_event_adapters_are_repeat_safe_and_bound() -> None:
    container = _script_patch_source("RSVP00_OnContainerChangedSetAV")
    crafted = _script_patch_source("RSVP00_OnItemCraftedSetStage")
    water = _script_patch_source("RSVP01_PlayerCollectWater")
    holotape = _script_patch_source("AliasOnPlayerHolotapeSetAV")
    terminal = _script_patch_source("RSVP02_OnTerminalEnterSetAV")
    assert container is not None
    assert crafted is not None
    assert water is not None
    assert holotape is not None
    assert terminal is not None

    assert "akNewContainer == playerRef" in container
    assert "playerRef.GetValue(AVToSet) != SetToValue" in container
    assert "OnEquipped" not in container
    assert "AddInventoryEventFilter(ItemToCraft)" in crafted
    assert "RemoveAllInventoryEventFilters()" in crafted
    assert "akBaseItem == ItemToCraft" in crafted
    assert "owningQuest.GetStage() < StageToSet" in crafted
    assert "!owningQuest.IsStageDone(StageToSet)" in crafted
    crafted_load = _member_body(crafted, "onplayerloadgame")
    crafted_reconcile = _member_body(crafted, "reconcileunsupportedrecipegate")
    assert "RemoveAllInventoryEventFilters()" in crafted_load
    assert "RegisterRecipeGateFilters()" in crafted_load
    assert "ReconcileUnsupportedRecipeGate()" in crafted_load
    filters = _member_body(crafted, "registerrecipegatefilters")
    assert "AddInventoryEventFilter(ItemToCraft)" in filters
    assert 'Game.GetFormFromFile(0x04695A, "SeventySix.esm")' in filters
    assert "AddInventoryEventFilter(rawBrahminMeat)" in filters
    assert "AddInventoryEventFilter(wood)" in filters
    assert 'Game.GetFormFromFile(0x04695A, "SeventySix.esm")' in crafted_reconcile
    assert 'Game.GetFormFromFile(0x04A13F, "SeventySix.esm")' in crafted_reconcile
    assert 'Game.GetFormFromFile(0x01FAC2, "Fallout4.esm")' in crafted_reconcile
    assert "RemoveItem(rawBrahminMeat, 1, True)" in crafted_reconcile
    assert "RemoveItem(wood, 1, True)" in crafted_reconcile
    assert "AddItem(ItemToCraft, 1, False)" in crafted_reconcile
    water_load = _member_body(water, "onplayerloadgame")
    water_reconcile = _member_body(water, "reconcileboiledwatergate")
    assert "ReconcileBoiledWaterGate()" in water_load
    assert "RemoveAllInventoryEventFilters()" in water_load
    assert "AddInventoryEventFilter(WaterDirty)" in water_load
    assert "AddInventoryEventFilter(WaterBoiled)" in water_load
    assert "GetItemCount(WaterBoiled) == 0" in water_reconcile
    assert "AddItem(WaterBoiled, 1, False)" in water_reconcile
    assert "SetStage(StageToSet_GotBoiledWater)" in water_reconcile
    assert "akSourceContainer == RSVP01_REF_Furniture_Well_Pump01" in water
    assert "akSourceContainer == RSVP01_REF_Furniture_Well_Pump02" in water
    assert "AddInventoryEventFilter(WaterDirty)" in water
    assert "AddInventoryEventFilter(WaterBoiled)" in water
    assert 'RegisterForRemoteEvent(RSVP01_REF_Furniture_Well_Pump01, "OnActivate")' in water
    assert 'RegisterForRemoteEvent(RSVP01_REF_Furniture_Well_Pump02, "OnActivate")' in water
    assert "RemoveAllInventoryEventFilters()" in water
    assert 'UnregisterForRemoteEvent(RSVP01_REF_Furniture_Well_Pump01, "OnActivate")' in water
    assert 'UnregisterForRemoteEvent(RSVP01_REF_Furniture_Well_Pump02, "OnActivate")' in water
    assert "!RSVP01_Quest_Thirst.IsStageDone(StageToSet_GotWellWater)" in water
    assert "!RSVP01_Quest_Thirst.IsStageDone(StageToSet_GotRiverWater)" in water
    pump = _member_body(water, "objectreference.onactivate")
    added = _member_body(water, "onitemadded")
    assert "removedItem = WaterDirty" in pump
    assert "akActionRef.AddItem(WaterDirty, 1, True)" in pump
    assert "RSVP01_Quest_Thirst.SetStage(StageToSet_GotWellWater)" in pump
    assert "akBaseItem == WaterDirty && removedItem == WaterDirty" in added
    assert "removedItem = None" in added
    sentinel_branch = added.split(
        "If akBaseItem == WaterDirty && removedItem == WaterDirty", 1
    )[1].split("If akBaseItem == WaterBoiled", 1)[0]
    assert "StageToSet_GotWellWater" in sentinel_branch
    assert "StageToSet_GotRiverWater" not in sentinel_branch
    assert "Return" in sentinel_branch
    assert "If setAVonEnd" in holotape
    assert "playerRef.GetItemCount(HolotapeToWatchFor) > 0" in holotape
    assert "playerRef.GetValue(HolotapeAV) != Value" in holotape
    assert "playerRef.GetValue(AVToSet) != SetToValue" in terminal
