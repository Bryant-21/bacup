from __future__ import annotations

from collections import Counter

import pytest

from bacup_lib.tests.test_terminal_fragment_script_patches import _fo4_base_source
from bacup_lib.workflows.unified import (
    _iter_top_level_papyrus_members,
    _merge_script_method_patches,
    _script_patch_source,
)
from creation_lib.pex.native_runtime import compile_psc


QF_205P = "Fragments:Quests:QF_W05_MQR_205P_00548B7A"
QF_205P_A = "Fragments:Quests:QF_W05_MQR_205P_A_005588EF"
RARA_COWER = "W05_MQR_205P_RaRaCowerTriggerScript"
SECURITY = "W05_MQR_205P_SecurityTriggerScript"
TURRETS = "W05_MQR_205P_TurretsOffScript"
KEYPAD_OBJECTIVE = "W05_MQR_PlayerVault79KeypadObjective"
KEYPAD_ALIAS = "W05_MQR_Vault79KeypadAliasScript"

MQR205_HELPERS = (
    "W05_MQR_205P_RaRaCombatScript",
    RARA_COWER,
    "W05_MQR_205P_ScannerFurnitureScript",
    SECURITY,
    TURRETS,
    "W05_MQR_205P_VentSequenceScript",
)
MQR205_HELPER_MEMBERS = {
    "W05_MQR_205P_RaRaCombatScript": {"oncombatstatechanged"},
    RARA_COWER: {"ontriggerenter"},
    "W05_MQR_205P_ScannerFurnitureScript": {"onactivate"},
    SECURITY: {"ontriggerenter"},
    TURRETS: {"ontriggerenter"},
    "W05_MQR_205P_VentSequenceScript": {"onactivate"},
}
MQR205_MINIMAL_SKELETONS = {
    TURRETS: "Scriptname W05_MQR_205P_TurretsOffScript Extends ReferenceAlias\n",
    KEYPAD_OBJECTIVE: """Scriptname W05_MQR_PlayerVault79KeypadObjective Extends ReferenceAlias

ActorValue Property W05_MQ00_CodeAV Auto
LocationAlias Property InstancedLocationAlias Auto
Int Property EndOnStage = -1 Auto
Int Property PreReqStage = -1 Auto
Int Property KeypadObjective = -1 Auto
""",
}
SCRIPTS = (KEYPAD_OBJECTIVE,)
EXPECTED_MEMBERS = {
    KEYPAD_OBJECTIVE: {"onlocationchange"},
}
TODO_MARKERS = {KEYPAD_OBJECTIVE: 0}

INTERCOM = "W05_MQR_201P_IntercomTriggerScript"
MQR201_202_ZERO_MEMBER_SCRIPTS = (
    "W05_MQR_201P_LouRoomTriggerScript",
    "W05_MQR_202P_DummyActivateMarker",
)

EXPECTED_QF_MEMBERS = {
    QF_205P: {
        f"fragment_stage_{stage:04d}_item_00"
        for stage in (
            100,
            105,
            106,
            110,
            1,
            200,
            210,
            250,
            260,
            300,
            305,
            310,
            315,
            320,
            325,
            330,
            400,
            410,
            500,
            550,
            560,
            600,
            610,
            615,
            620,
            700,
            710,
            800,
            810,
            900,
            905,
            906,
            910,
            915,
            920,
            921,
            930,
            940,
            1100,
            1105,
            1106,
            1107,
            1110,
            1111,
            1120,
            1200,
            1210,
            1220,
            1230,
            1240,
            1250,
            9000,
        )
    }
    | {"fragment_stage_0930_item_01", "fragment_stage_0940_item_01"},
    QF_205P_A: {
        f"fragment_stage_{stage:04d}_item_00"
        for stage in (100, 200, 300, 400, 500, 600, 9000)
    },
}


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


def _merged_tracked_source(script_name: str) -> str:
    return _merge_script_method_patches(
        MQR205_MINIMAL_SKELETONS[script_name], _patch(script_name)
    )


@pytest.mark.parametrize("script_name", SCRIPTS)
def test_patch_surface_matches_the_live_alias_contract(script_name: str):
    patch = _patch(script_name)
    assert not any(
        line.strip().lower().startswith("scriptname ") for line in patch.splitlines()
    )
    assert Counter(_member_names(patch)) == Counter(EXPECTED_MEMBERS[script_name])
    assert patch.splitlines().count("; TODO") == TODO_MARKERS[script_name]


def test_vault79_objective_keeps_the_proven_location_and_stage_guards():
    objective = _patch(KEYPAD_OBJECTIVE)

    assert "targetLocation == None || akNewLoc != targetLocation" in objective
    assert "currentStage >= PreReqStage && currentStage < EndOnStage" in objective
    assert "owningQuest.SetObjectiveDisplayed(KeypadObjective)" in objective


def test_vault79_keypad_alias_activation_fallback_is_retired():
    assert _script_patch_source(KEYPAD_ALIAS) is None


@pytest.mark.parametrize("script_name", MQR205_HELPERS)
def test_mqr205_helpers_match_the_live_callback_surface(script_name: str):
    patch = _patch(script_name)
    assert Counter(_member_names(patch)) == Counter(MQR205_HELPER_MEMBERS[script_name])
    assert "; TODO" not in patch


def test_mqr201_intercom_reconstruction_is_owned_by_the_focused_playthrough():
    assert _script_patch_source(INTERCOM) is not None


@pytest.mark.parametrize("script_name", (QF_205P, QF_205P_A))
def test_mqr205_parent_and_child_manifests_match_current_reconciliation(
    script_name: str,
):
    assert Counter(_member_names(_patch(script_name))) == Counter(
        EXPECTED_QF_MEMBERS[script_name]
    )


def test_mqr205_parent_is_54_of_54_and_child_is_7_of_7():
    parent_members = set(_member_names(_patch(QF_205P)))
    child_members = set(_member_names(_patch(QF_205P_A)))

    assert parent_members == EXPECTED_QF_MEMBERS[QF_205P]
    assert len(parent_members) == 54
    assert child_members == EXPECTED_QF_MEMBERS[QF_205P_A]
    assert len(child_members) == 7


def test_mqr205_turrets_off_patch_has_one_exact_player_receiver():
    patch = _patch(TURRETS)
    assert Counter(_member_names(patch)) == Counter({"ontriggerenter": 1})
    assert patch.splitlines() == [
        "Event OnTriggerEnter(ObjectReference akActionRef)",
        "    If akActionRef != Game.GetPlayer()",
        "        Return",
        "    EndIf",
        "",
        "    W05_MQR_205P_QuestScript owningQuestScript = GetOwningQuest() as W05_MQR_205P_QuestScript",
        "    If owningQuestScript != None && owningQuestScript.SecurityRoomTurrets != None",
        "        owningQuestScript.SecurityRoomTurrets.DisableAll()",
        "    EndIf",
        "EndEvent",
    ]

    merged = _merge_script_method_patches(MQR205_MINIMAL_SKELETONS[TURRETS], patch)
    assert Counter(_member_names(merged)) == Counter({"ontriggerenter": 1})
    assert _merge_script_method_patches(merged, patch) == merged


@pytest.mark.parametrize("script_name", SCRIPTS)
def test_tracked_minimal_merge_is_unique_idempotent_and_native_compiles(
    script_name: str,
):
    merged = _merged_tracked_source(script_name)
    assert Counter(_member_names(merged)) == Counter(EXPECTED_MEMBERS[script_name])
    assert _merge_script_method_patches(merged, _patch(script_name)) == merged

    base_source = _fo4_base_source()
    assert base_source is not None, "FO4 base Papyrus sources unavailable"
    result = compile_psc(
        merged,
        imports=[str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=f"{script_name}.psc",
    )
    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None


def test_zero_member_candidates_have_no_persistent_marker_only_patch():
    for script_name in MQR201_202_ZERO_MEMBER_SCRIPTS:
        assert _script_patch_source(script_name) is None
