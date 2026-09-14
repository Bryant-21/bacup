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

PATCH_MEMBERS = {
    "DefaultAliasMakeAliasRefForInventory": {
        "onaliasinit",
        "onitemadded",
        "onaliasshutdown",
    },
    "DefaultAliasOnActivateRemoveItems": {"onactivate"},
    "DefaultQuestEmergencyBroadcastScript": {
        "sendemergencybroadcast",
        "onquestinit",
        "onstageset",
    },
    "DefaultQuestOnKillManager": {
        "setvictimkeyword",
        "setvictimfaction",
        "setweaponkeyword",
        "setvictimsrequired",
        "starttrackingkills",
        "onquestinit",
        "actor.onkill",
        "onquestshutdown",
    },
    "DefaultOnItemAddedScript": {
        "onquestinit",
        "objectreference.onitemadded",
        "onquestshutdown",
    },
    "DefaultQuestShowMapMarkerScript": {
        "enablemapmarkersforstage",
        "onquestinit",
        "onstageset",
    },
    "DefaultQuestCleanupItemsOnShutdown": {
        "removequestitemfromplayer",
        "onquestshutdown",
    },
    "DefaultQuestOnAddPlayersAddToAlias": {"onquestinit"},
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
def test_daily_shared_helper_patch_is_member_only(
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
def test_daily_shared_helper_patch_merges_once_and_native_compiles(
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


def test_inventory_alias_helpers_keep_reference_identity_and_variants_inherit():
    patch = _script_patch_source("DefaultAliasMakeAliasRefForInventory")
    assert patch is not None
    item_added = _member_body(patch, "onitemadded")

    assert "akItemReference == None" in item_added
    assert "TargetAlias.ForceRefTo(akItemReference)" in item_added
    assert "TargetCollection.AddRef(akItemReference)" in item_added
    assert "AddInventoryEventFilter(TargetObject)" in patch
    assert "RemoveAllInventoryEventFilters()" in patch
    assert "DropObject" not in patch

    for variant in (
        "DefaultAliasMakeAliasRefForInventoryA",
        "DefaultAliasMakeAliasRefForInventoryB",
    ):
        source_path = SOURCE_ROOT / _script_relative_path(variant, ".psc")
        source = source_path.read_text(encoding="utf-8")
        assert "Extends DefaultAliasMakeAliasRefForInventory" in source
        assert _script_patch_source(variant) is None


def test_remove_items_activation_charges_before_parent_stage_dispatch():
    patch = _script_patch_source("DefaultAliasOnActivateRemoveItems")
    assert patch is not None
    on_activate = _member_body(patch, "onactivate")

    remove_index = on_activate.index(
        "akActionRef.RemoveItem(ItemToRemove, requiredCount, True)"
    )
    dispatch_index = on_activate.index("parent.OnActivate(akActionRef)")
    assert remove_index < dispatch_index
    assert "akActionRef != Game.GetPlayer()" in on_activate
    assert "MessageInsufficientItems.Show()" in on_activate


def test_kill_manager_restores_the_retained_runtime_filter_contract():
    patch = _script_patch_source("DefaultQuestOnKillManager")
    assert patch is not None
    on_kill = _member_body(patch, "actor.onkill")

    assert "VictimKeywordOverride = NewVictimKeyword" in patch
    assert "VictimFactionOverride = NewVictimFaction" in patch
    assert "WeaponKeywordOverride = NewWeaponKeyword" in patch
    assert "VictimsRequiredOverride = NewVictimsRequiredAmount" in patch
    assert "RegisterForRemoteEvent(Game.GetPlayer(), \"OnKill\")" in patch
    assert "akVictim.HasKeyword(requiredVictimKeyword)" in on_kill
    assert "CountVictims += 1" in on_kill
    assert "SetStage(StageToSet)" in on_kill


def test_item_and_map_helpers_are_event_driven_and_shutdown_safe():
    item_patch = _script_patch_source("DefaultOnItemAddedScript")
    map_patch = _script_patch_source("DefaultQuestShowMapMarkerScript")
    cleanup_patch = _script_patch_source("DefaultQuestCleanupItemsOnShutdown")
    assert item_patch is not None
    assert map_patch is not None
    assert cleanup_patch is not None

    assert "ObjectReference.OnItemAdded" in item_patch
    assert "AddInventoryEventFilter(ItemStages[index].itemFilter)" in item_patch
    assert "GetItemCount(requiredItem)" in item_patch
    assert "UnregisterForRemoteEvent(Game.GetPlayer(), \"OnItemAdded\")" in item_patch
    assert "RemoveAllInventoryEventFilters()" in item_patch
    assert "mapMarker.AddToMap(MapMarkerData[index].MarkDiscovered)" in map_patch
    assert "akItemReference.GetContainer() == playerRef" in cleanup_patch
    assert "playerRef.RemoveItem(akItemReference, 1, True)" in cleanup_patch


def test_emergency_broadcast_is_local_and_does_not_require_sq_master_queue():
    patch = _script_patch_source("DefaultQuestEmergencyBroadcastScript")
    assert patch is not None
    send_broadcast = _member_body(patch, "sendemergencybroadcast")

    bound_actor_index = send_broadcast.index(
        "speaker = akSpeakerAlias.GetActorReference()"
    )
    fallback_index = send_broadcast.index("SQ_Master as SQ_MasterScript")
    say_index = send_broadcast.index("speaker.Say(akTopic)")

    assert bound_actor_index < fallback_index < say_index
    assert "speaker = masterScript.EBSDefaultActor" in send_broadcast
    assert "akTopic == None || akSpeakerAlias == None" not in send_broadcast
    assert "TurnOffActorValue" in send_broadcast
    assert "SQ_Master." not in patch
    assert "IncludeTeammates" not in patch
