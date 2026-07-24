from __future__ import annotations

from pathlib import Path

from bacup_lib.tests.test_terminal_fragment_script_patches import _fo4_base_source
from bacup_lib.workflows.unified import (
    _iter_top_level_papyrus_members,
    _merge_script_method_patches,
    _script_patch_source,
)
from creation_lib.pex import decompile_pex
from creation_lib.pex.native_runtime import compile_psc


REPO_ROOT = Path(__file__).resolve().parents[5]
SCRIPT_NAME = "W05_MQ_004P_Crane_DoorTriggerScript"
DEPLOYED_PEX = (
    REPO_ROOT / "mods" / "SeventySix" / "data" / "Scripts" / f"{SCRIPT_NAME}.pex"
)


def _member_body(source: str, member_name: str) -> str:
    lines = source.splitlines()
    start, end = next(
        (start, end)
        for _kind, name, start, end in _iter_top_level_papyrus_members(lines)
        if name == member_name.lower()
    )
    return "\n".join(lines[start : end + 1])


def _merged_production_source() -> str:
    assert DEPLOYED_PEX.is_file(), f"deployed production PEX unavailable: {DEPLOYED_PEX}"
    patch = _script_patch_source(SCRIPT_NAME)
    assert patch is not None
    skeleton = decompile_pex(DEPLOYED_PEX, fo4_api_compat=True)
    return _merge_script_method_patches(skeleton, patch)


def test_registered_cache_scan_opens_door_then_sets_stage_1000_once():
    patch = _script_patch_source(SCRIPT_NAME)
    assert patch is not None

    trigger = _member_body(patch, "ontriggerenter")
    registration_guard = (
        "playerRef.GetValue(W05_MQ_004P_Crane_PlayerRegisteredPipBoy) > 0.0"
    )
    open_door = "cacheDoor.SetOpen(True)"
    stage_guard = "!W05_MQ_004P_Crane.IsStageDone(1000)"
    set_stage = "W05_MQ_004P_Crane.SetStage(1000)"

    assert trigger.count(set_stage) == 1
    assert trigger.index(registration_guard) < trigger.index(open_door)
    assert trigger.index(open_door) < trigger.index(stage_guard) < trigger.index(set_stage)
    assert "SetCurrentStageID(1000)" not in trigger


def test_cache_scan_does_not_shortcut_registration_or_grant_rewards():
    patch = _script_patch_source(SCRIPT_NAME)
    assert patch is not None

    trigger = _member_body(patch, "ontriggerenter")
    assert "SetValue(W05_MQ_004P_Crane_PlayerRegisteredPipBoy" not in trigger
    assert "SetStage(820)" not in trigger
    assert "SetCurrentStageID(820)" not in trigger
    assert "AddItem(" not in trigger
    assert "CompleteQuest(" not in trigger


def test_crane_cache_patch_production_merge_is_unique_and_idempotent():
    patch = _script_patch_source(SCRIPT_NAME)
    assert patch is not None
    merged = _merged_production_source()

    member_names = [
        name
        for _kind, name, _start, _end in _iter_top_level_papyrus_members(
            merged.splitlines()
        )
    ]
    assert member_names.count("ontriggerenter") == 1
    assert member_names.count("ontimer") == 1
    assert _member_body(merged, "ontriggerenter") == _member_body(
        patch, "ontriggerenter"
    )
    assert _merge_script_method_patches(merged, patch) == merged


def test_crane_cache_patch_full_production_merge_native_compiles_for_fo4():
    base_source = _fo4_base_source()
    assert base_source is not None, "FO4 base Papyrus sources unavailable"

    result = compile_psc(
        _merged_production_source(),
        imports=[str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=f"{SCRIPT_NAME}.psc",
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None
