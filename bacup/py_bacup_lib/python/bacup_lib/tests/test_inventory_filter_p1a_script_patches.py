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
MAP_MARKER = "DefaultQuestAddMapMarkerOnHolotapeEnd"
START_QUEST = "DefaultStartQuestOnHolotapeEvent"
GATHER = "Expeditions:ObjectiveModule_GatherAndDeposit"
SCRIPTS = (MAP_MARKER, START_QUEST, GATHER)


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
def test_p1a_filter_patches_merge_once_and_are_idempotent(script_name: str):
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


@pytest.mark.parametrize(
    ("script_name", "rebuild_name", "dedupe_name", "array_name", "field_name"),
    (
        (
            MAP_MARKER,
            "rebuildholotapemapmarkerfilters",
            "hasearlierholotapemapmarkerfilter",
            "HolotapeMapMarkerData",
            "Tape",
        ),
        (
            START_QUEST,
            "rebuildholotapeeventfilters",
            "hasearlierholotapeeventfilter",
            "HolotapeData",
            "TriggeringTape",
        ),
        (
            GATHER,
            "rebuildlocalitemfilters",
            "hasearlierlocalitemfilter",
            "ItemsToDepositStructArray",
            "ItemForm",
        ),
    ),
)
def test_p1a_rebuilds_each_distinct_non_none_array_filter(
    script_name: str,
    rebuild_name: str,
    dedupe_name: str,
    array_name: str,
    field_name: str,
):
    patch = _patch(script_name)
    rebuild = _member_body(patch, rebuild_name)
    dedupe = _member_body(patch, dedupe_name)

    assert "AddInventoryEventFilter(None)" not in patch
    assert rebuild.count("RemoveAllInventoryEventFilters()") == 1
    assert rebuild.count("AddInventoryEventFilter(") == 1
    assert ".RemoveAllInventoryEventFilters(" not in rebuild
    assert ".AddInventoryEventFilter(" not in rebuild
    assert f"While {array_name} != None &&" in rebuild
    assert f".{field_name}" in rebuild
    assert "!= None && !HasEarlier" in rebuild
    assert rebuild.index("RemoveAllInventoryEventFilters()") < rebuild.index(
        "AddInventoryEventFilter("
    )
    assert f"While {array_name} != None &&" in dedupe
    assert f".{field_name} ==" in dedupe
    assert "Return True" in dedupe
    assert "Return False" in dedupe


@pytest.mark.parametrize(
    ("script_name", "rebuild_call", "registrations"),
    (
        (
            MAP_MARKER,
            "RebuildHolotapeMapMarkerFilters()",
            ('RegisterForRemoteEvent(player, "OnItemAdded")',),
        ),
        (
            START_QUEST,
            "RebuildHolotapeEventFilters()",
            ('RegisterForRemoteEvent(player, "OnItemAdded")',),
        ),
        (
            GATHER,
            "RebuildLocalItemFilters()",
            (
                'RegisterForRemoteEvent(playerRef, "OnItemAdded")',
                'RegisterForRemoteEvent(playerRef, "OnItemRemoved")',
            ),
        ),
    ),
)
def test_p1a_arms_filters_before_remote_inventory_registration(
    script_name: str, rebuild_call: str, registrations: tuple[str, ...]
):
    initialized = _member_body(_patch(script_name), "onquestinit")
    assert initialized.count(rebuild_call) == 1
    for registration in registrations:
        assert initialized.count(registration) == 1
        assert initialized.index(rebuild_call) < initialized.index(registration)


@pytest.mark.parametrize("script_name", SCRIPTS)
def test_p1a_reset_does_not_override_startup_and_shutdown_disarms_filters(
    script_name: str,
):
    patch = _patch(script_name)
    shutdown = _member_body(patch, "onquestshutdown")

    assert "onreset" not in [name for name, _start, _end in _members(patch)]
    assert shutdown.count("RemoveAllInventoryEventFilters()") == 1
    if script_name == GATHER:
        assert shutdown.count("UnregisterForAllEvents()") == 1
        assert shutdown.index("UnregisterForAllEvents()") < shutdown.index(
            "RemoveAllInventoryEventFilters()"
        )
    else:
        unregister = 'UnregisterForRemoteEvent(player, "OnItemAdded")'
        assert shutdown.count(unregister) == 1
        assert shutdown.index(unregister) < shutdown.index(
            "RemoveAllInventoryEventFilters()"
        )


def test_gather_rearms_filters_before_load_reconciliation():
    loaded = _member_body(_patch(GATHER), "actor.onplayerloadgame")
    assert "akSender == Game.GetPlayer() && !IsStageDone(CompleteStage)" in loaded
    assert loaded.count("RebuildLocalItemFilters()") == 1
    assert loaded.index("RebuildLocalItemFilters()") < loaded.index(
        "PrepareLocalItemSets()"
    )


def test_existing_inventory_handlers_keep_sender_item_and_count_guards():
    marker_added = _member_body(_patch(MAP_MARKER), "objectreference.onitemadded")
    start_added = _member_body(_patch(START_QUEST), "objectreference.onitemadded")
    gather_added = _member_body(_patch(GATHER), "objectreference.onitemadded")
    gather_removed = _member_body(_patch(GATHER), "objectreference.onitemremoved")

    assert "akSender == Game.GetPlayer() && aiItemCount > 0" in marker_added
    assert "ProcessHolotape(akBaseItem as Holotape)" in marker_added
    assert "akSender != Game.GetPlayer() || aiItemCount <= 0" in start_added
    assert "ProcessHolotape(akBaseItem as Holotape, eventRef)" in start_added
    assert "akSender == Game.GetPlayer()" in gather_added
    assert "ReconcileLocalItemCounts(akBaseItem)" in gather_added
    assert "akSender == Game.GetPlayer()" in gather_removed
    assert "ReconcileLocalItemCounts(akBaseItem)" in gather_removed


@pytest.mark.parametrize("script_name", SCRIPTS)
def test_p1a_merged_source_native_compiles_for_fo4(script_name: str):
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
