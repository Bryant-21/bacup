from __future__ import annotations

from collections import Counter
from pathlib import Path

from bacup_lib.tests.test_terminal_fragment_script_patches import _fo4_base_source
from bacup_lib.workflows.unified import (
    _iter_top_level_papyrus_members,
    _merge_script_method_patches,
    _script_patch_source,
    _script_relative_path,
)
from creation_lib.pex import decompile_pex
from creation_lib.pex.native_runtime import compile_psc


REPO_ROOT = Path(__file__).resolve().parents[5]
DEPLOYED_SCRIPTS_ROOT = REPO_ROOT / "mods" / "SeventySix" / "data" / "Scripts"
QF_202 = "Fragments:Quests:QF_W05_MQS_202P_Acrobat_003F28C7"
INSTANCE_HELPER = "DefaultSetStageOnInstanceLoadQuest"


def _member_names(source: str) -> list[str]:
    return [
        name
        for kind, name, _start, _end in _iter_top_level_papyrus_members(
            source.splitlines()
        )
        if kind in {"function", "event"}
    ]


def _member_body(source: str, member_name: str) -> str:
    lines = source.splitlines()
    start, end = next(
        (start, end)
        for _kind, name, start, end in _iter_top_level_papyrus_members(lines)
        if name == member_name.lower()
    )
    return "\n".join(lines[start : end + 1])


def _production_skeleton(script_name: str) -> str:
    pex_path = DEPLOYED_SCRIPTS_ROOT / _script_relative_path(script_name, ".pex")
    assert pex_path.is_file(), f"deployed production PEX unavailable: {pex_path}"
    return decompile_pex(pex_path, fo4_api_compat=True)


def _merged_production_source(script_name: str) -> str:
    patch = _script_patch_source(script_name)
    assert patch is not None
    return _merge_script_method_patches(_production_skeleton(script_name), patch)


def test_stage_700_waits_for_the_bound_instance_location_producer():
    quest_patch = _script_patch_source(QF_202)
    helper_patch = _script_patch_source(INSTANCE_HELPER)
    assert quest_patch is not None
    assert helper_patch is not None

    stage_700 = _member_body(quest_patch, "fragment_stage_0700_item_00")
    assert "SetObjectiveCompleted(600)" in stage_700
    assert "SetObjectiveDisplayed(700)" in stage_700
    assert "SetStage(720)" not in stage_700

    check_location = _member_body(
        helper_patch, "checkplayerinstancelocation"
    )
    assert "locationData.TargetLocation == playerLocation" in check_location
    assert "locationData.StageToSet >= 0" in check_location
    assert "locationData.PreReqStage < 0 || IsStageDone(locationData.PreReqStage)" in check_location
    assert "locationData.ShutdownStage < 0 || GetStage() < locationData.ShutdownStage" in check_location
    assert check_location.index("locationData.TargetLocation == playerLocation") < check_location.index(
        "SetStage(locationData.StageToSet)"
    )


def test_instance_entry_reconciles_location_on_init_change_and_save_load():
    helper_patch = _script_patch_source(INSTANCE_HELPER)
    assert helper_patch is not None

    init = _member_body(helper_patch, "onquestinit")
    location_change = _member_body(helper_patch, "actor.onlocationchange")
    player_load = _member_body(helper_patch, "actor.onplayerloadgame")
    stage_set = _member_body(helper_patch, "onstageset")
    shutdown = _member_body(helper_patch, "onquestshutdown")

    assert 'RegisterForRemoteEvent(playerRef, "OnLocationChange")' in init
    assert 'RegisterForRemoteEvent(playerRef, "OnPlayerLoadGame")' in init
    assert "CheckPlayerInstanceLocation(playerRef.GetCurrentLocation())" in init
    assert "CheckPlayerInstanceLocation(akNewLoc)" in location_change
    assert "CheckPlayerInstanceLocation(akSender.GetCurrentLocation())" in player_load
    assert "CheckPlayerInstanceLocation(playerRef.GetCurrentLocation())" in stage_set
    assert 'UnregisterForRemoteEvent(playerRef, "OnLocationChange")' in shutdown
    assert 'UnregisterForRemoteEvent(playerRef, "OnPlayerLoadGame")' in shutdown
    assert "StartTimer" not in helper_patch


def test_deep_entry_members_merge_once_and_full_sources_compile_for_fo4():
    base_source = _fo4_base_source()
    assert base_source is not None, "FO4 base Papyrus sources unavailable"

    for script_name in (QF_202, INSTANCE_HELPER):
        patch = _script_patch_source(script_name)
        assert patch is not None
        merged = _merged_production_source(script_name)
        patch_names = _member_names(patch)
        merged_names = _member_names(merged)

        for member_name in patch_names:
            assert Counter(merged_names)[member_name] == 1
            assert _member_body(merged, member_name) == _member_body(
                patch, member_name
            )
        assert _merge_script_method_patches(merged, patch) == merged

        result = compile_psc(
            merged,
            imports=[str(base_source)],
            game="fo4",
            flags=str(base_source / "Institute_Papyrus_Flags.flg"),
            source_path=f"{script_name.replace(':', '/')}.psc",
        )
        diagnostics = "\n".join(str(item) for item in result.diagnostics)
        assert result.ok, diagnostics
        assert result.pex_bytes is not None
