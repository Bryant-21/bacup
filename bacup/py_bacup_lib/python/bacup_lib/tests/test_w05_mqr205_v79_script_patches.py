from __future__ import annotations

from collections import Counter
from pathlib import Path

import pytest

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
GENERATED_SOURCE_ROOT = (
    REPO_ROOT / "mods" / "SeventySix" / "Scripts" / "Source" / "User"
)

RARA_COWER = "W05_MQR_205P_RaRaCowerTriggerScript"
SECURITY = "W05_MQR_205P_SecurityTriggerScript"
KEYPAD_OBJECTIVE = "W05_MQR_PlayerVault79KeypadObjective"
KEYPAD_ALIAS = "W05_MQR_Vault79KeypadAliasScript"

SCRIPTS = (RARA_COWER, SECURITY, KEYPAD_OBJECTIVE, KEYPAD_ALIAS)
EXPECTED_MEMBERS = {
    RARA_COWER: {"ontriggerenter"},
    SECURITY: {"ontriggerenter"},
    KEYPAD_OBJECTIVE: {"onlocationchange"},
    KEYPAD_ALIAS: {"onactivate"},
}
TODO_MARKERS = {RARA_COWER: 1, SECURITY: 0, KEYPAD_OBJECTIVE: 0, KEYPAD_ALIAS: 0}

# --- Folded from the former mqr201_202 shard (Intercom trigger) ---------------
INTERCOM = "W05_MQR_201P_IntercomTriggerScript"
MQR201_202_ZERO_MEMBER_SCRIPTS = (
    "W05_MQR_201P_LouRoomTriggerScript",
    "W05_MQR_202P_DummyActivateMarker",
)

# --- Folded from the former mqr203 shard (DoorPortal teleport) ----------------
DOOR_PORTAL = "W05_MQR_203P_DoorPortalScript"

# --- Folded from the former mqs_overseer_qt shard (OverseerCAMP tutorial) -----
OVERSEER_CAMP_TUT_TRIGGER = "W05_OverseerCAMP_TutTriggerScript"


def _patch(script_name: str) -> str:
    patch = _script_patch_source(script_name)
    assert patch is not None
    return patch


def _member_names(source: str) -> list[str]:
    return [
        name
        for kind, name, _start, _end in _iter_top_level_papyrus_members(
            source.splitlines()
        )
        if kind in {"function", "event"}
    ]


def _production_skeleton(script_name: str) -> str:
    pex_path = DEPLOYED_SCRIPTS_ROOT / _script_relative_path(script_name, ".pex")
    assert pex_path.is_file(), f"deployed production PEX unavailable: {pex_path}"
    return decompile_pex(pex_path, fo4_api_compat=True)


def _merged_production_source(script_name: str) -> str:
    return _merge_script_method_patches(_production_skeleton(script_name), _patch(script_name))


def _merged_generated_source(script_name: str) -> str:
    # Folded scripts (DoorPortal, OverseerCAMP_TutTrigger) already have a
    # generated .psc skeleton under Scripts/Source/User -- unlike the deployed
    # production PEXes the other scripts in this file merge against.
    source_path = GENERATED_SOURCE_ROOT / _script_relative_path(script_name, ".psc")
    assert source_path.is_file(), source_path
    return _merge_script_method_patches(
        source_path.read_text(encoding="utf-8"), _patch(script_name)
    )


@pytest.mark.parametrize("script_name", SCRIPTS)
def test_patch_surface_matches_the_live_alias_contract(script_name: str):
    patch = _patch(script_name)
    assert not any(
        line.strip().lower().startswith("scriptname ") for line in patch.splitlines()
    )
    assert Counter(_member_names(patch)) == Counter(EXPECTED_MEMBERS[script_name])
    assert patch.splitlines().count("; TODO") == TODO_MARKERS[script_name]


def test_vault79_helpers_keep_the_proven_player_and_stage_guards():
    cower = _patch(RARA_COWER)
    security = _patch(SECURITY)
    objective = _patch(KEYPAD_OBJECTIVE)
    keypad = _patch(KEYPAD_ALIAS)

    assert cower.index("akActionRef != Game.GetPlayer()") < cower.index(
        "RaRaCowerIdleMarker.GetReference()"
    )
    assert "raRaRef.EvaluatePackage()" in cower

    assert security.index("akActionRef != Game.GetPlayer()") < security.index(
        "collisionRef.Disable()"
    )

    assert "targetLocation == None || akNewLoc != targetLocation" in objective
    assert "currentStage >= PreReqStage && currentStage < EndOnStage" in objective
    assert "owningQuest.SetObjectiveDisplayed(KeypadObjective)" in objective

    assert keypad.index("akActionRef != Game.GetPlayer()") < keypad.index(
        "owningQuest.SetStage(StageToSet)"
    )
    assert "!owningQuest.IsStageDone(PreReqStage)" in keypad


@pytest.mark.parametrize("script_name", SCRIPTS)
def test_production_merge_is_unique_idempotent_and_native_compiles(script_name: str):
    merged = _merged_production_source(script_name)
    assert Counter(_member_names(merged)) == Counter(EXPECTED_MEMBERS[script_name])
    assert _merge_script_method_patches(merged, _patch(script_name)) == merged

    base_source = _fo4_base_source()
    assert base_source is not None, "FO4 base Papyrus sources unavailable"
    assert GENERATED_SOURCE_ROOT.is_dir(), "generated source root unavailable"
    result = compile_psc(
        merged,
        imports=[str(base_source), str(GENERATED_SOURCE_ROOT)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=f"{script_name}.psc",
    )
    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None


# --- Folded from the former mqr201_202 shard (Intercom trigger) ---------------


def test_intercom_recovery_is_the_only_evidence_backed_member_patch():
    patch = _patch(INTERCOM)
    assert _member_names(patch) == ["ontriggerenter"]
    assert Counter(_member_names(patch)) == Counter({"ontriggerenter": 1})
    assert patch.splitlines().count("; TODO") == 1

    merged = _merged_production_source(INTERCOM)
    assert _member_names(merged).count("ontriggerenter") == 1
    assert "owningQuest.SetStage(1200)" in merged
    assert "intercomRef.Say(W05_MQR_201P_LouSaysTopic_IntercomGreeting" in merged
    assert "owningQuest.SetStage(1300)" in merged


def test_zero_member_candidates_have_no_persistent_marker_only_patch():
    for script_name in MQR201_202_ZERO_MEMBER_SCRIPTS:
        assert _script_patch_source(script_name) is None
        assert _member_names(_production_skeleton(script_name)) == []


def test_full_merged_intercom_psc_compiles_for_fo4():
    base_source = _fo4_base_source()
    assert base_source is not None, "FO4 base Papyrus sources unavailable"

    result = compile_psc(
        _merged_production_source(INTERCOM),
        imports=[str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=f"{INTERCOM}.psc",
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None


# --- Folded from the former mqr203 shard (DoorPortal teleport) ----------------


def test_door_portal_moves_the_activator_to_its_bound_destination_once():
    patch = _patch(DOOR_PORTAL)
    assert patch.lower().count("event onactivate(") == 1
    assert "Alias_Destination == None" in patch
    assert "destinationRef != None" in patch
    assert "akActionRef.MoveTo(destinationRef)" in patch

    merged = _merged_generated_source(DOOR_PORTAL)
    assert merged.lower().count("event onactivate(") == 1


def test_zero_member_door_scripts_remain_unpatched():
    assert _script_patch_source("W05_MQR_203P_ArenaDoorCloseLock") is None
    assert _script_patch_source("W05_MQR_203P_DoorPortalRefScript") is None

    arena = (GENERATED_SOURCE_ROOT / "W05_MQR_203P_ArenaDoorCloseLock.psc").read_text(
        encoding="utf-8"
    )
    portal_ref = (
        GENERATED_SOURCE_ROOT / "W05_MQR_203P_DoorPortalRefScript.psc"
    ).read_text(encoding="utf-8")
    assert arena.strip() == "Scriptname W05_MQR_203P_ArenaDoorCloseLock Extends ObjectReference"
    assert "State questactive" in portal_ref
    assert "State questcompleted" in portal_ref
    assert "Event " not in portal_ref
    assert "Function " not in portal_ref


def test_door_portal_merged_script_compiles_for_fo4():
    base_source = _fo4_base_source()
    assert base_source is not None, "FO4 base Papyrus sources unavailable"

    result = compile_psc(
        _merged_generated_source(DOOR_PORTAL),
        imports=[str(base_source), str(GENERATED_SOURCE_ROOT)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=f"{DOOR_PORTAL}.psc",
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None


# --- Folded from the former mqs_overseer_qt shard (OverseerCAMP tutorial) -----


def test_w05_trigger_patches_supply_one_handler():
    patch = _patch(OVERSEER_CAMP_TUT_TRIGGER)
    assert "Scriptname " not in patch
    members = {
        name.lower()
        for _kind, name, _start, _end in _iter_top_level_papyrus_members(
            patch.splitlines()
        )
    }
    assert "ontriggerenter" in members
    merged = _merged_generated_source(OVERSEER_CAMP_TUT_TRIGGER)
    assert merged.lower().count("event ontriggerenter(") == 1


def test_overseer_camp_trigger_uses_bound_story_keywords_only_for_unstarted_quests():
    merged = _merged_generated_source(OVERSEER_CAMP_TUT_TRIGGER)
    player_guard = merged.find("akActionRef != Game.GetPlayer()")
    binding_guard = merged.find(
        "currentQuest.BlockingValue && currentQuest.StartingKeyword && currentQuest.TargetQuest"
    )
    state_guard = merged.find(
        "!currentQuest.TargetQuest.IsRunning() && !currentQuest.TargetQuest.IsCompleted()"
    )
    send_event = merged.find("currentQuest.StartingKeyword.SendStoryEvent(akRef1 = akActionRef)")
    assert -1 not in (player_guard, binding_guard, state_guard, send_event)
    assert player_guard < binding_guard < state_guard < send_event


def test_qt_trigger_remains_evidence_blocked_without_a_persistent_patch():
    assert _script_patch_source("W05_QT_TriggerScript") is None


def test_w05_trigger_patch_set_native_compiles_for_fo4():
    base_source = _fo4_base_source()
    assert base_source is not None, "FO4 base Papyrus sources unavailable"

    result = compile_psc(
        _merged_generated_source(OVERSEER_CAMP_TUT_TRIGGER),
        imports=[str(GENERATED_SOURCE_ROOT), str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=f"{OVERSEER_CAMP_TUT_TRIGGER}.psc",
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None
