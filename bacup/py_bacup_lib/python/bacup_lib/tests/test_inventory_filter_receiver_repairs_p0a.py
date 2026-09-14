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
TW007 = "TW007_PlayerScript"
FS02 = "FS02_MQ_Reassembly_PlayerAliasScript"
BOSZ01 = "BoSZ01PlayerAliasScript"
FREE_PRISONERS = "Expeditions:ObjectiveModule_FreePrisoners"
SCRIPTS = (TW007, FS02, BOSZ01, FREE_PRISONERS)


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
def test_inventory_filter_repairs_merge_once_and_are_idempotent(script_name: str):
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


def test_tw007_rebuilds_non_none_clue_filters_on_init_and_load():
    patch = _patch(TW007)
    reset = _member_body(patch, "resetfreddycluefilters")
    initialized = _member_body(patch, "onaliasinit")
    loaded = _member_body(patch, "onplayerloadgame")
    shutdown = _member_body(patch, "onaliasshutdown")

    assert reset.count("RemoveAllInventoryEventFilters()") == 1
    assert reset.count("AddInventoryEventFilter(FreddyClues[i].ClueForm)") == 1
    assert "If FreddyClues[i].ClueForm != None" in reset
    assert reset.index("RemoveAllInventoryEventFilters()") < reset.index(
        "AddInventoryEventFilter(FreddyClues[i].ClueForm)"
    )
    assert ".AddInventoryEventFilter(" not in reset
    for lifecycle in (initialized, loaded):
        assert lifecycle.count("ResetFreddyClueFilters()") == 1
        assert lifecycle.count("ReconcileFreddyClues()") == 1
        assert lifecycle.index("ResetFreddyClueFilters()") < lifecycle.index(
            "ReconcileFreddyClues()"
        )
    assert shutdown.count("RemoveAllInventoryEventFilters()") == 1


def test_fs02_filters_belong_to_the_alias_and_have_exact_teardown():
    patch = _patch(FS02)
    initialized = _member_body(patch, "onaliasinit")
    loaded = _member_body(patch, "onplayerloadgame")
    reset = _member_body(patch, "resettrackeditemfilters")
    shutdown = _member_body(patch, "onaliasshutdown")
    filters = ("FS01_MQ_Warn_UplinkMiscItem", "FS01_MQ_Warn_UpgradedTransmitter")

    assert initialized.count("ResetTrackedItemFilters()") == 1
    assert initialized.index("ResetTrackedItemFilters()") < initialized.index(
        "EvaluateAbbiesBunkerEntry()"
    )
    assert loaded.count("ResetTrackedItemFilters()") == 1
    assert reset.count("RemoveAllInventoryEventFilters()") == 1
    assert reset.count("AddInventoryEventFilter(") == 2
    assert ".AddInventoryEventFilter(" not in reset
    assert "AddInventoryEventFilter(None)" not in reset
    for inventory_filter in filters:
        guard = f"If {inventory_filter} != None"
        add = f"AddInventoryEventFilter({inventory_filter})"
        assert reset.count(guard) == 1
        assert reset.count(add) == 1
        assert reset.index(guard) < reset.index(add)
    assert shutdown.count("RemoveAllInventoryEventFilters()") == 1


def test_bosz01_remote_listener_filters_and_cleans_up_on_the_alias():
    patch = _patch(BOSZ01)
    initialized = _member_body(patch, "onaliasinit")
    loaded = _member_body(patch, "onplayerloadgame")
    reset = _member_body(patch, "resettechnicaldocumentlistener")
    removed = _member_body(patch, "objectreference.onitemremoved")
    shutdown = _member_body(patch, "onaliasshutdown")

    assert initialized.count("ResetTechnicalDocumentListener()") == 1
    assert loaded.count("ResetTechnicalDocumentListener()") == 1
    assert reset.count("RemoveAllInventoryEventFilters()") == 1
    assert reset.count("UnregisterForAllRemoteEvents()") == 1
    assert reset.count("AddInventoryEventFilter(BoSTechnicalDocument)") == 1
    assert reset.count('RegisterForRemoteEvent(playerRef, "OnItemRemoved")') == 1
    assert "playerRef != None && BoSTechnicalDocument != None" in reset
    assert "AddInventoryEventFilter(None)" not in reset
    assert ".AddInventoryEventFilter(" not in reset
    assert reset.index("RemoveAllInventoryEventFilters()") < reset.index(
        "UnregisterForAllRemoteEvents()"
    )
    assert reset.index("UnregisterForAllRemoteEvents()") < reset.index(
        "AddInventoryEventFilter(BoSTechnicalDocument)"
    )
    assert reset.index("AddInventoryEventFilter(BoSTechnicalDocument)") < reset.index(
        'RegisterForRemoteEvent(playerRef, "OnItemRemoved")'
    )
    assert "akSender == playerRef && akBaseItem == BoSTechnicalDocument" in removed
    assert "owningQuest.SetStage(350)" in removed
    assert shutdown.count("UnregisterForAllRemoteEvents()") == 1
    assert shutdown.count("RemoveAllInventoryEventFilters()") == 1
    assert "GetActorReference()" not in shutdown
    assert shutdown.index("UnregisterForAllRemoteEvents()") < shutdown.index(
        "RemoveAllInventoryEventFilters()"
    )


def test_free_prisoners_remote_listener_filters_and_cleans_up_on_the_quest():
    patch = _patch(FREE_PRISONERS)
    initialized = _member_body(patch, "onquestinit")
    reset = _member_body(patch, "resetdoorkeyinventorylistener")
    loaded = _member_body(patch, "actor.onplayerloadgame")
    added = _member_body(patch, "objectreference.onitemadded")
    shutdown = _member_body(patch, "onquestshutdown")

    assert initialized.count("ResetDoorKeyInventoryListener(playerRef)") == 1
    assert initialized.count(
        'RegisterForRemoteEvent(playerRef, "OnPlayerLoadGame")'
    ) == 1
    assert reset.count("RemoveAllInventoryEventFilters()") == 1
    assert reset.count('UnregisterForRemoteEvent(playerRef, "OnItemAdded")') == 1
    assert reset.count("AddInventoryEventFilter(DoorKey)") == 1
    assert reset.count('RegisterForRemoteEvent(playerRef, "OnItemAdded")') == 1
    assert "If DoorKey != None" in reset
    assert "AddInventoryEventFilter(None)" not in reset
    assert ".AddInventoryEventFilter(" not in reset
    assert reset.index("RemoveAllInventoryEventFilters()") < reset.index(
        'UnregisterForRemoteEvent(playerRef, "OnItemAdded")'
    )
    assert reset.index('UnregisterForRemoteEvent(playerRef, "OnItemAdded")') < reset.index(
        "AddInventoryEventFilter(DoorKey)"
    )
    assert reset.index("AddInventoryEventFilter(DoorKey)") < reset.index(
        'RegisterForRemoteEvent(playerRef, "OnItemAdded")'
    )
    assert loaded.index("ResetDoorKeyInventoryListener(akSender)") < loaded.index(
        "ReconcileLocalPrisonState()"
    )
    assert "akSender == Game.GetPlayer() && akBaseItem == DoorKey" in added
    assert "ReconcileLocalPrisonState()" in added
    assert shutdown.count("UnregisterForAllEvents()") == 1
    assert shutdown.count("RemoveAllInventoryEventFilters()") == 1
    assert shutdown.index("UnregisterForAllEvents()") < shutdown.index(
        "RemoveAllInventoryEventFilters()"
    )


@pytest.mark.parametrize("script_name", SCRIPTS)
def test_inventory_filter_repairs_native_compile_for_fo4(script_name: str):
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
