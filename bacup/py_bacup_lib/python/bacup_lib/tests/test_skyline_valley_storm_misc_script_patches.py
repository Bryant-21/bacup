from __future__ import annotations

import os
from pathlib import Path

import pytest

from bacup_lib.workflows.unified import (
    _iter_papyrus_states,
    _iter_top_level_papyrus_members,
    _merge_script_method_patches,
    _script_patch_source,
    _script_relative_path,
)
from creation_lib.pex.native_runtime import compile_psc


REPO_ROOT = Path(__file__).resolve().parents[5]
SOURCE_ROOT = REPO_ROOT / "mods" / "SeventySix" / "Scripts" / "Source" / "User"

# Every script patched by shard w2-skyline-valley-storm-misc. The seventh row in
# this shard's runbook, Storm_PuzzleLaserGridScript, resolved to non-defect (zero
# source records, zero live VMAD bindings even under the decompress-aware probe
# sweep, no consuming script, and no trace of its own puzzle-room content anywhere
# in FO76 source data -- see contracts/w2-skyline-valley-storm-misc.md) and is
# intentionally not patched or covered here.
PATCH_CASES = (
    "Storm_DefenseUpgradeRepairScript",
    "Storm_DKWD_Interior_SecretDoor_Script",
    "Storm_LaserGridTimerScript",
    "Storm_ManorMapMarkerUnlockScript",
    "Storm_MQ10_KeypadOnLoadScript",
    "StormMetalDetectorScript",
)


def _fo4_base_source() -> Path | None:
    candidates: list[Path] = []
    configured = os.environ.get("FO4_DIR", "").strip().strip('"')
    if configured:
        candidates.append(Path(configured))
    env_path = REPO_ROOT / ".env"
    if env_path.is_file():
        for line in env_path.read_text(encoding="utf-8").splitlines():
            if line.startswith("FO4_DIR="):
                value = line.split("=", 1)[1].strip().strip('"')
                if value:
                    candidates.append(Path(value))
                break
    for game_root in candidates:
        source_root = game_root / "Data" / "Scripts" / "Source" / "Base"
        if source_root.is_dir():
            return source_root
    return None


def _merged_source(script_name: str) -> str:
    source_path = SOURCE_ROOT / _script_relative_path(script_name, ".psc")
    patch = _script_patch_source(script_name)
    assert source_path.is_file(), source_path
    assert patch is not None
    return _merge_script_method_patches(
        source_path.read_text(encoding="utf-8"), patch
    )


def _member_body(source: str, header: str, end_keyword: str) -> str:
    start = source.find(header)
    assert start != -1, f"{header!r} not found"
    end = source.find(end_keyword, start)
    assert end != -1, f"{end_keyword!r} not found after {header!r}"
    return source[start:end]


@pytest.mark.parametrize("script_name", PATCH_CASES)
def test_patch_exists_with_no_scriptname_line(script_name: str):
    patch = _script_patch_source(script_name)
    assert patch is not None
    assert not any(
        line.strip().lower().startswith("scriptname ") for line in patch.splitlines()
    )


@pytest.mark.parametrize("script_name", PATCH_CASES)
def test_patch_declares_no_states_beyond_the_hollow_skeleton(script_name: str):
    # The merger cannot safely introduce new states -- every state named in the
    # patch must already exist (empty) in the generated skeleton. None of this
    # shard's rows use named states, so this should be an empty set on both sides.
    source_path = SOURCE_ROOT / _script_relative_path(script_name, ".psc")
    assert source_path.is_file(), source_path
    skeleton_states = {
        name
        for name, _start, _end in _iter_papyrus_states(
            source_path.read_text(encoding="utf-8").splitlines()
        )
    }
    patch_states = {
        name
        for name, _start, _end in _iter_papyrus_states(
            _script_patch_source(script_name).splitlines()
        )
    }
    assert patch_states <= skeleton_states


@pytest.mark.parametrize("script_name", PATCH_CASES)
def test_merged_source_has_single_scriptname_line(script_name: str):
    merged = _merged_source(script_name)
    assert merged.lower().count("scriptname ") == 1


# --- Storm_DefenseUpgradeRepairScript -----------------------------------------


def test_defense_upgrade_repair_patch_supplies_onactivate():
    patch = _script_patch_source("Storm_DefenseUpgradeRepairScript")
    assert patch is not None
    members = {
        (kind, name)
        for kind, name, _start, _end in _iter_top_level_papyrus_members(
            patch.splitlines()
        )
    }
    assert ("event", "onactivate") in members


def test_defense_upgrade_repair_guards_both_bound_properties():
    patch = _script_patch_source("Storm_DefenseUpgradeRepairScript")
    assert patch is not None
    assert "SpawnedObject != None" in patch
    assert "Storm_DefenseEventMessageUpgrade != None" in patch


def test_defense_upgrade_repair_order_is_spawn_then_message_then_disable_then_delete():
    merged = _merged_source("Storm_DefenseUpgradeRepairScript")
    body = _member_body(merged, "Event OnActivate(", "EndEvent")
    spawn_index = body.find("PlaceAtMe(SpawnedObject)")
    show_index = body.find("Storm_DefenseEventMessageUpgrade.Show()")
    disable_index = body.find("Disable()")
    delete_index = body.find("Delete()")
    assert spawn_index != -1
    assert show_index != -1
    assert disable_index != -1
    assert delete_index != -1
    assert spawn_index < show_index < disable_index < delete_index


# --- Storm_DKWD_Interior_SecretDoor_Script -------------------------------------


def test_secret_door_patch_supplies_onload():
    patch = _script_patch_source("Storm_DKWD_Interior_SecretDoor_Script")
    assert patch is not None
    members = {
        (kind, name)
        for kind, name, _start, _end in _iter_top_level_papyrus_members(
            patch.splitlines()
        )
    }
    assert ("event", "onload") in members


def test_secret_door_unlocks_on_quest_completion_one_directionally():
    # Matches the shipped UnlockOnLoadScript.psc precedent exactly: unlock-only,
    # no re-lock else-branch (see contract disagreement note).
    patch = _script_patch_source("Storm_DKWD_Interior_SecretDoor_Script")
    assert patch is not None
    assert "TargetQuest != None" in patch
    assert "TargetQuest.IsCompleted()" in patch
    assert "Lock(False)" in patch
    assert "Lock(True)" not in patch


# --- Storm_LaserGridTimerScript -------------------------------------------------


def test_laser_grid_timer_patch_supplies_three_members():
    patch = _script_patch_source("Storm_LaserGridTimerScript")
    assert patch is not None
    members = {
        (kind, name)
        for kind, name, _start, _end in _iter_top_level_papyrus_members(
            patch.splitlines()
        )
    }
    assert ("event", "oninit") in members
    assert ("function", "applygridstate") in members
    assert ("event", "ontimer") in members


def test_laser_grid_timer_guards_none_chain_and_none_grids():
    patch = _script_patch_source("Storm_LaserGridTimerScript")
    assert patch is not None
    assert "chain != None" in patch
    assert "laserGrids == None" in patch


def test_laser_grid_timer_uses_explicit_nonzero_timer_id():
    patch = _script_patch_source("Storm_LaserGridTimerScript")
    assert patch is not None
    assert "StartTimer(fTimerInterval, 1)" in patch
    assert "aiTimerID == 1" in patch


def test_laser_grid_timer_broadcasts_via_verified_parent_function():
    merged = _merged_source("Storm_LaserGridTimerScript")
    apply_body = _member_body(merged, "Function ApplyGridState(", "EndFunction")
    assert "UpdateClientAnimationStateValue(bAllowAccess)" in apply_body


def test_laser_grid_timer_ontimer_flips_access_and_rearms():
    merged = _merged_source("Storm_LaserGridTimerScript")
    timer_body = _member_body(merged, "Event OnTimer(", "EndEvent")
    assert "bCurrentlyAllowingAccess = !bCurrentlyAllowingAccess" in timer_body
    assert "ApplyGridState(bCurrentlyAllowingAccess)" in timer_body


# --- Storm_ManorMapMarkerUnlockScript -------------------------------------------


def test_manor_map_marker_unlock_patch_supplies_onactivate():
    patch = _script_patch_source("Storm_ManorMapMarkerUnlockScript")
    assert patch is not None
    members = {
        (kind, name)
        for kind, name, _start, _end in _iter_top_level_papyrus_members(
            patch.splitlines()
        )
    }
    assert ("event", "onactivate") in members


def test_manor_map_marker_unlock_is_player_gated_and_idempotent():
    patch = _script_patch_source("Storm_ManorMapMarkerUnlockScript")
    assert patch is not None
    assert "akActionRef != Game.GetPlayer()" in patch
    assert "IsMapMarkerVisible()" in patch


def test_manor_map_marker_unlock_matches_addtomap_false_precedent():
    # All three existing AddToMap precedents in script_patches/ ship False or the
    # equivalent default; none hardcode True (see contract justification).
    patch = _script_patch_source("Storm_ManorMapMarkerUnlockScript")
    assert patch is not None
    assert "AddToMap(False)" in patch
    assert "AddToMap(True)" not in patch


# --- Storm_MQ10_KeypadOnLoadScript ----------------------------------------------


def test_keypad_on_load_patch_supplies_onload():
    patch = _script_patch_source("Storm_MQ10_KeypadOnLoadScript")
    assert patch is not None
    members = {
        (kind, name)
        for kind, name, _start, _end in _iter_top_level_papyrus_members(
            patch.splitlines()
        )
    }
    assert ("event", "onload") in members


def test_keypad_on_load_broadcasts_preset_code_to_player():
    patch = _script_patch_source("Storm_MQ10_KeypadOnLoadScript")
    assert patch is not None
    assert "keypadCode != None" in patch
    assert "Game.GetPlayer().SetValue(keypadCode, presetCode as Float)" in patch


# --- StormMetalDetectorScript ----------------------------------------------------


def test_metal_detector_patch_supplies_three_members():
    patch = _script_patch_source("StormMetalDetectorScript")
    assert patch is not None
    members = {
        (kind, name)
        for kind, name, _start, _end in _iter_top_level_papyrus_members(
            patch.splitlines()
        )
    }
    assert ("event", "oninit") in members
    assert ("event", "ontriggerenter") in members
    assert ("event", "ontimer") in members


def test_metal_detector_reassigns_timer_id_away_from_skeleton_zero_default():
    # The skeleton declares `Int cooldownTimerID = 0`, which would silently no-op
    # every StartTimer call (lesson #12) if left at its default.
    merged = _merged_source("StormMetalDetectorScript")
    init_body = _member_body(merged, "Event OnInit()", "EndEvent")
    assert "cooldownTimerID = 1" in init_body


def test_metal_detector_enable_disable_are_cooldown_gated_and_guarded():
    patch = _script_patch_source("StormMetalDetectorScript")
    assert patch is not None
    assert "!cooldownActive" in patch
    assert "soundMarkerToEnable != None" in patch
    assert "soundMarkerToEnable.Enable()" in patch
    assert "soundMarkerToEnable.Disable()" in patch
    assert "StartTimer(numberOfSecondsCooldown as Float, cooldownTimerID)" in patch


# --- compile verification -------------------------------------------------------


@pytest.mark.parametrize("script_name", PATCH_CASES)
def test_merged_patch_native_compiles_for_fo4(script_name: str):
    base_source = _fo4_base_source()
    if base_source is None:
        pytest.skip("FO4 base Papyrus sources unavailable")

    merged = _merged_source(script_name)
    result = compile_psc(
        merged,
        imports=[str(base_source), str(SOURCE_ROOT)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=str(_script_relative_path(script_name, ".psc")),
    )
    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None
