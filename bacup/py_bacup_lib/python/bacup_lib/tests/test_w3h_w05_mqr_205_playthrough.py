from __future__ import annotations

from collections import Counter
from pathlib import Path

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
RARA_COMBAT = "W05_MQR_205P_RaRaCombatScript"
RARA_COWER = "W05_MQR_205P_RaRaCowerTriggerScript"
SCANNER = "W05_MQR_205P_ScannerFurnitureScript"
SECURITY = "W05_MQR_205P_SecurityTriggerScript"
TURRETS = "W05_MQR_205P_TurretsOffScript"
VENT = "W05_MQR_205P_VentSequenceScript"

QUEST_FRAGMENTS = (QF_205P, QF_205P_A)
HELPER_SCRIPTS = (
    RARA_COMBAT,
    RARA_COWER,
    SCANNER,
    SECURITY,
    TURRETS,
    VENT,
)

MINIMAL_SKELETONS = {
    QF_205P: """Scriptname Fragments:Quests:QF_W05_MQR_205P_00548B7A Extends Quest hidden

Keyword Property W05_MQA_206P_QuestStart_Keyword Auto
ActorValue Property W05_MQR_205P_RaRaOpenDoorsValue Auto
ActorValue Property W05_MQR_205P_PlasmaGunAcquiredValue Auto
Scene Property W05_MQR_205P_001_IntroScene Auto
Scene Property W05_MQR_205P_002_Lou_Door02 Auto
Scene Property W05_MQR_205P_003_DoorBlownUp Auto
Scene Property W05_MQR_205P_004A_Meg_ComeBack Auto
Scene Property W05_MQR_205P_004A_JohnnyDoor Auto
Scene Property W05_MQR_205P_005_SecurityRoom Auto
Scene Property W05_MQR_205P_015_RaRa_OverseerRoom Auto
Scene Property W05_MQR_205P_016B_RaRa_OptionalVent Auto
Scene Property W05_MQR_205P_017_RaRa_LastVent02 Auto
ReferenceAlias Property Alias_InitEnableMarker Auto
ReferenceAlias Property Alias_SecurityRoomDoor Auto
ReferenceAlias Property Alias_SecurityRoomCollision Auto
ReferenceAlias Property Alias_Gail Auto
ReferenceAlias Property Alias_Johnny Auto
ReferenceAlias Property Alias_RaRa Auto
ReferenceAlias Property Alias_currentPlayer Auto
RefCollectionAlias Property Alias_AtriumRobotsWave01 Auto
ReferenceAlias Property Alias_OverseerRoomExitDoor Auto
ReferenceAlias Property Alias_RaRaEndDoorVentExitMarker Auto
ReferenceAlias Property Alias_endDoor Auto
""",
    QF_205P_A: "Scriptname Fragments:Quests:QF_W05_MQR_205P_A_005588EF Extends Quest hidden\n",
    RARA_COMBAT: """Scriptname W05_MQR_205P_RaRaCombatScript Extends ReferenceAlias

Keyword Property AnimArchetypeFriendly Auto
Package Property W05_MQR_205P_RaRaFleeCombatOverride Auto
Keyword Property AnimArchetypeScared Auto
""",
    RARA_COWER: "Scriptname W05_MQR_205P_RaRaCowerTriggerScript Extends ReferenceAlias\n",
    SCANNER: """Scriptname W05_MQR_205P_ScannerFurnitureScript Extends ReferenceAlias

ReferenceAlias Property Gail Auto
""",
    SECURITY: """Scriptname W05_MQR_205P_SecurityTriggerScript Extends ReferenceAlias

ReferenceAlias Property SecurityRoomCollision Auto
""",
    TURRETS: "Scriptname W05_MQR_205P_TurretsOffScript Extends ReferenceAlias\n",
    VENT: """Scriptname W05_MQR_205P_VentSequenceScript Extends ReferenceAlias

ReferenceAlias Property VentButton Auto
""",
}

CONTROLLER_STUB = """Scriptname W05_MQR_205P_QuestScript Extends Quest

ReferenceAlias Property RaRa Auto
ReferenceAlias Property RaRaCowerIdleMarker Auto
RefCollectionAlias Property SecurityRoomTurrets Auto
ReferenceAlias Property OptionalDoor Auto
Scene Property W05_MQR_205P_014_RaRa_OverseerVent03 Auto
Scene Property W05_MQR_205P_016_RaRa_OptionalVent03 Auto
Scene Property W05_MQR_205P_017_RaRa_LastVent02 Auto

Function KillJohnnyInSecurityRoom()
EndFunction
"""

ENCOUNTER_WAVE_STUB = """Scriptname defaultquestencounterwavescript Extends Quest

Function StartLocalEncounterWave(Int aiWaveIndex)
EndFunction
"""

EXPECTED_MEMBERS = {
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

OBJECTIVE_ONLY_MEMBERS = {
    QF_205P: {
        f"fragment_stage_{stage:04d}_item_00"
        for stage in (
            100,
            105,
            200,
            300,
            310,
            320,
            325,
            330,
            500,
            600,
            700,
            800,
            810,
            900,
            1100,
            1200,
        )
    },
    QF_205P_A: {f"fragment_stage_{stage:04d}_item_00" for stage in (100, 500)},
}

EXPECTED_HELPER_MEMBERS = {
    RARA_COMBAT: {"oncombatstatechanged"},
    RARA_COWER: {"ontriggerenter"},
    SCANNER: {"onactivate"},
    SECURITY: {"ontriggerenter"},
    TURRETS: {"ontriggerenter"},
    VENT: {"onactivate"},
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


def _member_body(source: str, member_name: str) -> str:
    start, end = next(
        (start, end)
        for _kind, name, start, end in _iter_top_level_papyrus_members(
            source.splitlines()
        )
        if name == member_name.lower()
    )
    return "\n".join(source.splitlines()[start : end + 1])


def _merged_source(script_name: str) -> str:
    return _merge_script_method_patches(
        MINIMAL_SKELETONS[script_name], _patch(script_name)
    )


@pytest.mark.parametrize("script_name", QUEST_FRAGMENTS)
def test_parent_and_child_patch_manifests_are_exact(script_name: str):
    patch = _patch(script_name)
    assert not any(
        line.strip().lower().startswith("scriptname ") for line in patch.splitlines()
    )
    assert Counter(_member_names(patch)) == Counter(EXPECTED_MEMBERS[script_name])
    assert "; TODO" not in patch


@pytest.mark.parametrize("script_name", QUEST_FRAGMENTS)
def test_every_patched_member_only_displays_its_matching_objective(script_name: str):
    patch = _patch(script_name)
    for member_name in OBJECTIVE_ONLY_MEMBERS[script_name]:
        stage = int(member_name.removeprefix("fragment_stage_").split("_")[0])
        body = _member_body(patch, member_name)
        assert body.splitlines() == [
            f"Function Fragment_Stage_{stage:04d}_Item_00()",
            f"    SetObjectiveDisplayed({stage})",
            "EndFunction",
        ]


def test_all_live_fragment_members_are_repaired():
    assert len(EXPECTED_MEMBERS[QF_205P]) == 54
    assert len(EXPECTED_MEMBERS[QF_205P_A]) == 7
    for script_name in QUEST_FRAGMENTS:
        patched = set(_member_names(_patch(script_name)))
        assert patched == EXPECTED_MEMBERS[script_name]


def test_terminal_handoff_exactly_matches_the_sibling_settler_receiver():
    body = _member_body(_patch(QF_205P), "fragment_stage_9000_item_00")
    assert body.splitlines() == [
        "Function Fragment_Stage_9000_Item_00()",
        "    Actor playerRef = Game.GetPlayer()",
        "    If W05_MQA_206P_QuestStart_Keyword != None && playerRef != None",
        "        W05_MQA_206P_QuestStart_Keyword.SendStoryEvent(None, playerRef, playerRef)",
        "    EndIf",
        "EndFunction",
    ]


def test_parent_scene_receivers_start_without_forcing_completion_stages():
    patch = _patch(QF_205P)
    scene_receivers = {
        110: "W05_MQR_205P_001_IntroScene",
        210: "W05_MQR_205P_002_Lou_Door02",
        260: "W05_MQR_205P_003_DoorBlownUp",
        305: "W05_MQR_205P_004A_Meg_ComeBack",
        315: "W05_MQR_205P_004A_JohnnyDoor",
        400: "W05_MQR_205P_005_SecurityRoom",
        910: "owningQuestScript.W05_MQR_205P_014_RaRa_OverseerVent03",
        920: "W05_MQR_205P_015_RaRa_OverseerRoom",
        1107: "W05_MQR_205P_016B_RaRa_OptionalVent",
        1210: "W05_MQR_205P_017_RaRa_LastVent02",
    }
    for stage, scene_property in scene_receivers.items():
        body = _member_body(patch, f"fragment_stage_{stage:04d}_item_00")
        guard = f"{scene_property} != None && !{scene_property}.IsPlaying()"
        assert guard in body
        assert f"{scene_property}.Start()" in body
        assert "SetStage(" not in body
    assert "SetObjectiveDisplayed(210)" in _member_body(
        patch, "fragment_stage_0210_item_00"
    )
    assert "SetObjectiveDisplayed(400)" in _member_body(
        patch, "fragment_stage_0400_item_00"
    )


def test_native_scene_completion_receivers_restore_local_door_effects():
    patch = _patch(QF_205P)
    assert _member_body(patch, "fragment_stage_0250_item_00").splitlines() == [
        "Function Fragment_Stage_0250_Item_00()",
        "    SetObjectiveCompleted(210)",
        "EndFunction",
    ]
    assert _member_body(patch, "fragment_stage_0410_item_00").splitlines() == [
        "Function Fragment_Stage_0410_Item_00()",
        "    ObjectReference securityDoor = Alias_SecurityRoomDoor.GetReference()",
        "    If securityDoor != None",
        "        securityDoor.SetOpen(False)",
        "    EndIf",
        "    ObjectReference securityCollision = Alias_SecurityRoomCollision.GetReference()",
        "    If securityCollision != None",
        "        securityCollision.Enable()",
        "    EndIf",
        "EndFunction",
    ]
    assert _member_body(patch, "fragment_stage_0921_item_00").splitlines() == [
        "Function Fragment_Stage_0921_Item_00()",
        "    ObjectReference exitDoor = Alias_OverseerRoomExitDoor.GetReference()",
        "    If exitDoor != None",
        "        exitDoor.Lock(False)",
        "        exitDoor.SetOpen(True)",
        "    EndIf",
        "    If !IsStageDone(930)",
        "        SetStage(930)",
        "    EndIf",
        "EndFunction",
    ]
    assert _member_body(patch, "fragment_stage_1230_item_00").splitlines() == [
        "Function Fragment_Stage_1230_Item_00()",
        "    ObjectReference endDoor = Alias_endDoor.GetReference()",
        "    If endDoor != None",
        "        endDoor.Lock(False)",
        "        endDoor.SetOpen(True)",
        "    EndIf",
        "EndFunction",
    ]


def test_atrium_wave_fragments_start_the_exact_live_struct_indices():
    patch = _patch(QF_205P)
    assert "StartLocalEncounterWave(0)" in _member_body(
        patch, "fragment_stage_0915_item_00"
    )
    assert "SetStage(920)" not in _member_body(patch, "fragment_stage_0915_item_00")
    assert "StartLocalEncounterWave(1)" in _member_body(
        patch, "fragment_stage_0930_item_01"
    )
    all_rooms = _member_body(patch, "fragment_stage_0940_item_01")
    assert all_rooms.index("StartLocalEncounterWave(2)") < all_rooms.index(
        "StartLocalEncounterWave(3)"
    )


def test_final_local_receiver_converges_on_the_story_event_handoff():
    body = _member_body(_patch(QF_205P), "fragment_stage_1250_item_00")
    assert body.splitlines() == [
        "Function Fragment_Stage_1250_Item_00()",
        "    SetObjectiveCompleted(1200)",
        "    If !IsStageDone(9000)",
        "        SetStage(9000)",
        "    EndIf",
        "EndFunction",
    ]


def test_child_local_outcomes_converge_without_reputation_mutation():
    patch = _patch(QF_205P_A)
    for stage in (200, 300, 400):
        body = _member_body(patch, f"fragment_stage_{stage:04d}_item_00")
        assert "SetObjectiveCompleted(100)" in body
        assert "If !IsStageDone(9000)" in body
        assert "SetStage(9000)" in body
    stage_600 = _member_body(patch, "fragment_stage_0600_item_00")
    assert "SetObjectiveCompleted(500)" in stage_600
    assert "If !IsStageDone(9000)" in stage_600
    assert "SetStage(9000)" in stage_600
    assert _member_body(patch, "fragment_stage_9000_item_00").splitlines() == [
        "Function Fragment_Stage_9000_Item_00()",
        "    SetObjectiveCompleted(100)",
        "    SetObjectiveCompleted(500)",
        "EndFunction",
    ]


def test_online_effects_remain_absent_from_parent_and_child():
    main_patch = _patch(QF_205P)
    child_patch = _patch(QF_205P_A)
    assert main_patch.count("SendStoryEvent") == 1
    assert "SendStoryEvent" not in child_patch
    combined = main_patch + "\n" + child_patch
    for forbidden in (
        "Rep_Mod_",
        "Reputation_AV_",
        ".AddItem(",
        ".RemoveItem(",
        ".Stop()",
        "CompleteQuest(",
        "GoldBullion",
    ):
        assert forbidden not in combined


@pytest.mark.parametrize("script_name", HELPER_SCRIPTS)
def test_quest_specific_helpers_restore_the_exact_callback_surface(
    script_name: str,
):
    patch = _patch(script_name)
    assert Counter(_member_names(patch)) == Counter(
        EXPECTED_HELPER_MEMBERS[script_name]
    )
    assert "; TODO" not in patch


def test_local_scanner_security_and_vent_receivers_are_guarded():
    scanner = _patch(SCANNER)
    assert "akActionRef != Gail.GetReference()" in scanner
    assert "!owningQuest.IsStageDone(610)" in scanner
    assert "owningQuest.SetStage(610)" in scanner

    security = _patch(SECURITY)
    assert "akActionRef != Game.GetPlayer()" in security
    assert "collisionRef.Disable()" in security

    vent = _patch(VENT)
    assert "akActionRef != owningQuestScript.RaRa.GetReference()" in vent
    assert "ventButtonRef.Activate(akActionRef)" in vent
    for stage_floor, scene in (
        (1200, "W05_MQR_205P_017_RaRa_LastVent02"),
        (1100, "W05_MQR_205P_016_RaRa_OptionalVent03"),
        (900, "W05_MQR_205P_014_RaRa_OverseerVent03"),
    ):
        assert f"currentStage >= {stage_floor}" in vent
        assert scene in vent


def test_ra_ra_combat_and_cower_receivers_keep_package_motion_local():
    combat = _patch(RARA_COMBAT)
    assert "aeCombatState > 0" in combat
    assert "ChangeAnimArchetype(AnimArchetypeScared)" in combat
    assert "ChangeAnimArchetype(AnimArchetypeFriendly)" in combat
    assert combat.count("EvaluatePackage()") == 1

    cower = _patch(RARA_COWER)
    assert "akActionRef != Game.GetPlayer()" in cower
    assert "cowerMarker.MoveTo(triggerRef)" in cower
    assert "raRaRef.EvaluatePackage()" in cower


def test_turrets_off_has_one_exact_player_collection_receiver():
    body = _patch(TURRETS)
    assert _member_names(body) == ["ontriggerenter"]
    assert body.splitlines() == [
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


@pytest.mark.parametrize("script_name", QUEST_FRAGMENTS + HELPER_SCRIPTS)
def test_tracked_minimal_merge_is_exact_unique_and_idempotent(script_name: str):
    patch = _patch(script_name)
    merged = _merged_source(script_name)
    expected = EXPECTED_MEMBERS.get(
        script_name, EXPECTED_HELPER_MEMBERS.get(script_name)
    )
    assert expected is not None
    assert Counter(_member_names(merged)) == Counter(expected)
    for member_name in expected:
        assert _member_body(merged, member_name) == _member_body(patch, member_name)
    assert _merge_script_method_patches(merged, patch) == merged


@pytest.mark.parametrize("script_name", QUEST_FRAGMENTS + HELPER_SCRIPTS)
def test_full_tracked_minimal_merge_native_compiles_for_fo4(
    script_name: str, tmp_path: Path
):
    base_source = _fo4_base_source()
    assert base_source is not None, "FO4 base Papyrus sources unavailable"
    (tmp_path / "W05_MQR_205P_QuestScript.psc").write_text(
        CONTROLLER_STUB, encoding="utf-8"
    )
    (tmp_path / "defaultquestencounterwavescript.psc").write_text(
        ENCOUNTER_WAVE_STUB, encoding="utf-8"
    )

    result = compile_psc(
        _merged_source(script_name),
        imports=[str(tmp_path), str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=f"{script_name.replace(':', '/')}.psc",
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None
