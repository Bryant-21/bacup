from __future__ import annotations

from collections import Counter
from pathlib import Path

import pytest

from bacup_lib.tests.test_script_patch_conventions import (
    has_unregistered_inventory_handler,
)
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
CB04 = "Fragments:Quests:QF_CB04_Mayor_002A93F5"
MOM = "MoMHolotapeScript"
SFL02 = "SFL02_Track_QuestScript"
SCRIPTS = (CB04, MOM, SFL02)


def _members(source: str) -> list[tuple[str, int, int]]:
    return [
        (name, start, end)
        for kind, name, start, end in _iter_top_level_papyrus_members(
            source.splitlines()
        )
        if kind in {"function", "event"}
    ]


def _member_body(source: str, member_name: str) -> str:
    lines = source.splitlines()
    start, end = next(
        (start, end)
        for name, start, end in _members(source)
        if name == member_name.lower()
    )
    return "\n".join(lines[start : end + 1])


def _patch(script_name: str) -> str:
    patch = _script_patch_source(script_name)
    assert patch is not None
    return patch


def _merged(script_name: str) -> str:
    source_path = SOURCE_ROOT / _script_relative_path(script_name, ".psc")
    assert source_path.is_file(), source_path
    return _merge_script_method_patches(
        source_path.read_text(encoding="utf-8"), _patch(script_name)
    )


@pytest.mark.parametrize("script_name", SCRIPTS)
def test_p1b_inventory_filter_repairs_merge_once_and_are_idempotent(
    script_name: str,
):
    patch = _patch(script_name)
    patch_names = [name for name, _start, _end in _members(patch)]
    assert Counter(patch_names) == Counter(set(patch_names))
    assert not has_unregistered_inventory_handler(patch)

    merged = _merged(script_name)
    merged_names = [name for name, _start, _end in _members(merged)]
    for member_name in patch_names:
        assert merged_names.count(member_name) == 1
        assert _member_body(merged, member_name) == _member_body(patch, member_name)
    assert _merge_script_method_patches(merged, patch) == merged


def test_cb04_filters_only_four_clues_and_reconciles_duplicate_registration():
    patch = _patch(CB04)
    reconcile = _member_body(patch, "reconcileclueinventorylistener")
    clear = _member_body(patch, "clearclueinventorylistener")
    stage10 = _member_body(patch, "fragment_stage_0010_item_00")
    stage300 = _member_body(patch, "fragment_stage_0300_item_00")
    stage500 = _member_body(patch, "fragment_stage_0500_item_00")
    filters = (
        "CB04_HolotapeClue1",
        "CB04_HolotapeClue2",
        "CB04_Password_Note",
        "CB04_Keycard",
    )

    assert reconcile.count("AddInventoryEventFilter(") == len(filters)
    assert "AddInventoryEventFilter(None)" not in reconcile
    assert ".AddInventoryEventFilter(" not in reconcile
    for inventory_filter in filters:
        assert reconcile.count(f"If {inventory_filter} != None") == 1
        assert reconcile.count(f"AddInventoryEventFilter({inventory_filter})") == 1
    assert reconcile.count('RegisterForRemoteEvent(playerRef, "OnItemAdded")') == 1
    assert reconcile.count('RegisterForRemoteEvent(playerRef, "OnPlayerLoadGame")') == 1
    assert clear.count('UnregisterForRemoteEvent(playerRef, "OnItemAdded")') == 1
    assert clear.count('UnregisterForRemoteEvent(playerRef, "OnPlayerLoadGame")') == 1
    assert clear.count("RemoveAllInventoryEventFilters()") == 1
    for stage in (stage10, stage300):
        assert stage.count("ReconcileClueInventoryListener()") == 1
        assert 'RegisterForRemoteEvent(playerRef, "OnItemAdded")' not in stage
        assert 'RegisterForRemoteEvent(Game.GetPlayer(), "OnItemAdded")' not in stage
    assert "ClearClueInventoryListener()" in stage500


def test_cb04_receiver_keeps_exact_stage_sender_and_item_guards():
    handler = _member_body(_patch(CB04), "objectreference.onitemadded")
    assert "akSender != Game.GetPlayer()" in handler
    assert "currentStage < 300 || currentStage >= 500" in handler
    assert handler.count("akBaseItem ==") == 4
    for inventory_filter in (
        "CB04_HolotapeClue1",
        "CB04_HolotapeClue2",
        "CB04_Password_Note",
        "CB04_Keycard",
    ):
        assert f"akBaseItem == {inventory_filter}" in handler


def test_mom_rebuilds_each_non_none_holotape_filter_on_init_and_load():
    patch = _patch(MOM)
    reconcile = _member_body(patch, "reconcileruntimeregistrations")
    clear = _member_body(patch, "clearruntimeregistrations")
    initialized = _member_body(patch, "onquestinit")
    loaded = _member_body(patch, "actor.onplayerloadgame")
    shutdown = _member_body(patch, "onquestshutdown")

    assert reconcile.count("AddInventoryEventFilter(") == 1
    assert "Holotape holotapeFilter = MoMHolotapeData[index].MoMHolotape" in reconcile
    assert "While firstIndex < index" in reconcile
    assert "holotapeFilter != None && firstIndex == index" in reconcile
    assert "AddInventoryEventFilter(holotapeFilter)" in reconcile
    assert "AddInventoryEventFilter(None)" not in reconcile
    assert ".AddInventoryEventFilter(" not in reconcile
    assert initialized.count("ReconcileRuntimeRegistrations()") == 1
    assert loaded.count("ReconcileRuntimeRegistrations()") == 1
    assert "akSender == Game.GetPlayer()" in loaded
    assert shutdown.count("ClearRuntimeRegistrations()") == 1
    assert clear.count('UnregisterForRemoteEvent(playerRef, "OnItemAdded")') == 1
    assert clear.count('UnregisterForRemoteEvent(playerRef, "OnPlayerLoadGame")') == 1
    assert clear.count("RemoveAllInventoryEventFilters()") == 1
    assert reconcile.count('RegisterForRemoteEvent(player, "OnItemAdded")') == 1
    assert reconcile.count('RegisterForRemoteEvent(player, "OnPlayerLoadGame")') == 1


def test_mom_receiver_preserves_sender_count_and_exact_datum_matching():
    patch = _patch(MOM)
    added = _member_body(patch, "objectreference.onitemadded")
    process = _member_body(patch, "processholotape")

    assert "akSender == Game.GetPlayer() && aiItemCount > 0" in added
    assert "ProcessHolotape(akBaseItem as Holotape)" in added
    assert "MoMHolotapeData[index].MoMHolotape == playedTape" in process
    assert "MoMMaster.SetStage(MoMHolotapeData[index].MoMQuestStageToSet)" in process


def test_sfl02_filters_only_eight_filled_alias_base_objects():
    patch = _patch(SFL02)
    register = _member_body(patch, "registertrackeditemfilters")
    get_alias = _member_body(patch, "getaliasbaseobject")
    reconcile = _member_body(patch, "reconcileruntimeregistrations")
    aliases = (
        "SignalBooster01",
        "SignalBooster03",
        "RandyBeacon",
        "HolotapeNari",
        "HolotapeRandy",
        "NariHazmatSuit",
        "NariIDCard",
        "HolotapeLucy",
    )

    assert register.count("GetAliasBaseObject(") == len(aliases)
    for item_alias in aliases:
        assert register.count(f"GetAliasBaseObject({item_alias})") == 1
    assert "If akItemAlias != None" in get_alias
    assert "If itemRef != None" in get_alias
    assert "Return itemRef.GetBaseObject()" in get_alias
    assert "Return None" in get_alias
    assert "inventoryFilters.Find(inventoryFilter) == index" in register
    assert register.count("AddInventoryEventFilter(inventoryFilter)") == 1
    assert "AddInventoryEventFilter(None)" not in patch
    assert ".AddInventoryEventFilter(" not in patch
    for event_name in ("OnItemAdded", "OnItemEquipped", "OnPlayerLoadGame"):
        assert reconcile.index(
            f'UnregisterForRemoteEvent(playerRef, "{event_name}")'
        ) < reconcile.index("RemoveAllInventoryEventFilters()")
    assert reconcile.index("RemoveAllInventoryEventFilters()") < reconcile.index(
        "RegisterTrackedItemFilters()"
    )
    for event_name in ("OnItemAdded", "OnItemEquipped", "OnPlayerLoadGame"):
        assert reconcile.index("RegisterTrackedItemFilters()") < reconcile.index(
            f'RegisterForRemoteEvent(playerRef, "{event_name}")'
        )


def test_sfl02_load_and_shutdown_lifecycle_remains_restart_safe():
    patch = _patch(SFL02)
    initialized = _member_body(patch, "onquestinit")
    loaded = _member_body(patch, "actor.onplayerloadgame")
    reconcile = _member_body(patch, "reconcileruntimeregistrations")
    shutdown = _member_body(patch, "onquestshutdown")

    assert initialized.count("ReconcileRuntimeRegistrations()") == 1
    assert loaded.count("ReconcileRuntimeRegistrations()") == 1
    assert "akSender == Game.GetPlayer()" in loaded
    for event_name in ("OnItemAdded", "OnItemEquipped", "OnPlayerLoadGame"):
        unregister = f'UnregisterForRemoteEvent(playerRef, "{event_name}")'
        register = f'RegisterForRemoteEvent(playerRef, "{event_name}")'
        assert reconcile.count(unregister) == 1
        assert reconcile.count(register) == 1
        assert reconcile.index(unregister) < reconcile.index(register)
        assert shutdown.count(unregister) == 1
    assert shutdown.count("RemoveAllInventoryEventFilters()") == 1


def test_sfl02_receiver_compares_only_the_eight_bound_item_aliases():
    handler = _member_body(_patch(SFL02), "objectreference.onitemadded")
    assert "akSender != Game.GetPlayer() || aiItemCount <= 0" in handler
    assert handler.count("SetStageForAliasForm(akBaseItem,") == 8
    for item_alias in (
        "SignalBooster01",
        "SignalBooster03",
        "RandyBeacon",
        "HolotapeNari",
        "HolotapeRandy",
        "NariHazmatSuit",
        "NariIDCard",
        "HolotapeLucy",
    ):
        assert f"SetStageForAliasForm(akBaseItem, {item_alias}," in handler


@pytest.mark.parametrize("script_name", SCRIPTS)
def test_p1b_inventory_filter_repairs_native_compile_for_fo4(script_name: str):
    base_source = _fo4_base_source()
    assert base_source is not None, "FO4 base Papyrus sources unavailable"

    result = compile_psc(
        _merged(script_name),
        imports=[str(SOURCE_ROOT), str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=str(_script_relative_path(script_name, ".psc")),
    )
    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None
