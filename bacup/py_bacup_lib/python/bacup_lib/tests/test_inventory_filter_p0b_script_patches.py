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

CASES = {
    "mtr04_gamescomplete": {
        "filter": "Uniform",
        "event": "objectreference.onitemadded",
        "sender": "akSender == playerRef",
        "item": "akBaseItem == Uniform",
    },
    "Quests:MTR04:Dross": {
        "filter": "DrossGrenade",
        "event": "objectreference.onitemremoved",
        "sender": "akSender == playerRef",
        "item": "akBaseItem == DrossGrenade",
    },
    "W05_MortTapeQuestScript": {
        "filter": "TargetTape",
        "event": "objectreference.onitemadded",
        "sender": "akSender == Game.GetPlayer()",
        "item": "akBaseItem == TargetTape",
    },
}


def _member_body(source: str, member_name: str) -> str:
    lines = source.splitlines()
    start, end = next(
        (start, end)
        for _kind, name, start, end in _iter_top_level_papyrus_members(lines)
        if name == member_name.lower()
    )
    return "\n".join(lines[start : end + 1])


def _merged_source(script_name: str) -> tuple[str, str]:
    source_path = SOURCE_ROOT / _script_relative_path(script_name, ".psc")
    patch = _script_patch_source(script_name)
    assert source_path.is_file(), source_path
    assert patch is not None
    return patch, _merge_script_method_patches(
        source_path.read_text(encoding="utf-8"), patch
    )


@pytest.mark.parametrize(("script_name", "case"), CASES.items())
def test_p0b_filter_patch_merges_once_and_is_idempotent(
    script_name: str, case: dict[str, str]
):
    patch, merged = _merged_source(script_name)
    member_names = [
        name
        for _kind, name, _start, _end in _iter_top_level_papyrus_members(
            merged.splitlines()
        )
    ]

    assert member_names.count("onquestinit") == 1
    assert member_names.count("onquestshutdown") == 1
    assert member_names.count(case["event"]) == 1
    assert _merge_script_method_patches(merged, patch) == merged


@pytest.mark.parametrize(("script_name", "case"), CASES.items())
def test_p0b_filter_lifecycle_is_exact_and_restart_safe(
    script_name: str, case: dict[str, str]
):
    patch, _merged = _merged_source(script_name)
    init = _member_body(patch, "onquestinit")
    shutdown = _member_body(patch, "onquestshutdown")
    filter_call = f"AddInventoryEventFilter({case['filter']})"
    remote_registration = "RegisterForRemoteEvent("

    assert "AddInventoryEventFilter(None)" not in patch
    assert ".AddInventoryEventFilter(" not in patch
    assert ".RemoveAllInventoryEventFilters(" not in patch
    assert init.count("RemoveAllInventoryEventFilters()") == 1
    assert init.count("AddInventoryEventFilter(") == 1
    assert init.count(filter_call) == 1
    assert f"{case['filter']} != None" in init
    assert init.index("RemoveAllInventoryEventFilters()") < init.index(filter_call)
    assert init.index(filter_call) < init.index(remote_registration)
    assert shutdown.count("UnregisterForAllEvents()") == 1
    assert shutdown.count("RemoveAllInventoryEventFilters()") == 1
    assert shutdown.index("UnregisterForAllEvents()") < shutdown.index(
        "RemoveAllInventoryEventFilters()"
    )


@pytest.mark.parametrize(("script_name", "case"), CASES.items())
def test_p0b_remote_receiver_rechecks_sender_and_exact_item(
    script_name: str, case: dict[str, str]
):
    patch, _merged = _merged_source(script_name)
    handler = _member_body(patch, case["event"])

    assert case["sender"] in handler
    assert case["item"] in handler


def test_mort_pickup_receiver_rejects_empty_inventory_notifications():
    patch, _merged = _merged_source("W05_MortTapeQuestScript")
    handler = _member_body(patch, "objectreference.onitemadded")

    assert "aiItemCount > 0" in handler


def test_dross_disarms_filter_before_shutdown_inventory_cleanup():
    patch, _merged = _merged_source("Quests:MTR04:Dross")
    shutdown = _member_body(patch, "onquestshutdown")

    assert shutdown.index("UnregisterForAllEvents()") < shutdown.index(
        "RemoveAllInventoryEventFilters()"
    ) < shutdown.index(
        "playerRef.RemoveItem(DrossGrenade, -1, True)"
    )


@pytest.mark.parametrize("script_name", CASES)
def test_p0b_merged_source_native_compiles_for_fo4(script_name: str):
    base_source = _fo4_base_source()
    assert base_source is not None, "FO4 base Papyrus sources unavailable"
    _patch, merged = _merged_source(script_name)

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
