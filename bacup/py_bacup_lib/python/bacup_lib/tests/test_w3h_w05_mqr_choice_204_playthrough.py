from __future__ import annotations

from collections import Counter
from pathlib import Path

import pytest

from bacup_lib.tests.test_terminal_fragment_script_patches import _fo4_base_source
from bacup_lib.workflows.unified import (
    _iter_papyrus_states,
    _iter_top_level_papyrus_members,
    _merge_script_method_patches,
    _script_patch_source,
)
from creation_lib.pex.native_runtime import compile_psc

CHOICE = "Fragments:Quests:QF_W05_MQR_Choice_005930B2"
LEV = "Fragments:Quests:QF_W05_MQR_204P_00535E55"
FREE_LOU = "W05_MQR_204P_FreeLouPerkScript"
KEYPAD_ALIAS = "W05_MQR_Vault79KeypadAliasScript"
KEYPAD_OBJECTIVE = "W05_MQR_PlayerVault79KeypadObjective"
KEYPAD_CONTROLLER = "W05_Vaut79EntranceKeypadScript"
CODE_NOTE = "W05_MQR_Vault79CodeNoteScript"
MQR204_CONTROLLER = "W05_MQR_204P_QuestScript"
SETTLER_CHOICE = "Fragments:Quests:qf_w05_mqs_choice_00592500"
SETTLER_204 = "Fragments:Quests:QF_W05_MQS_204P_0040C458"
QF_SCRIPTS = (CHOICE, LEV)
HELPER_SCRIPTS = (FREE_LOU, KEYPAD_OBJECTIVE)
SCRIPTS = QF_SCRIPTS + HELPER_SCRIPTS

# Generated PSC/PEX/ESM output is stale after the persistent 204 repair and is
# deliberately not a test input. These tracked zero-member skeletons preserve
# the live declarations consumed by the two patches without retaining methods
# from an older generated PEX.
ZERO_MEMBER_SKELETONS = {
    CHOICE: """Scriptname Fragments:Quests:QF_W05_MQR_Choice_005930B2 Extends Quest hidden

quest Property W05_MQ_102P_B Auto mandatory
quest Property W05_MQS_203P Auto mandatory
quest Property W05_MQS_Choice Auto mandatory
message Property W05_MQR_204P_WarningMSG Auto mandatory
quest Property W05_MQS_201P Auto mandatory
quest Property W05_MQS_202P Auto mandatory
keyword Property W05_MQR_204P_QuestStart_Keyword Auto mandatory
actorvalue Property W05_MQR_Choice_QuestComplete Auto mandatory
""",
    LEV: """Scriptname Fragments:Quests:QF_W05_MQR_204P_00535E55 Extends Quest hidden

actorvalue Property W05_MQR_204P_Started Auto mandatory
referencealias Property Alias_Lou Auto mandatory
scene Property GetUpScene Auto
faction Property PlayerEnemyFaction Auto mandatory
referencealias Property Alias_InitEnableMarker Auto mandatory
referencealias Property Alias_Creature Auto mandatory
referencealias Property Alias_LouPole Auto mandatory
key Property WL020_Key02 Auto mandatory
referencealias Property Alias_Fisher Auto mandatory
faction Property W05_DieHardFaction Auto mandatory
faction Property W05_CraterRaiderFaction Auto mandatory
globalvariable Property Rep_Mod_Add_Small Auto mandatory
actorvalue Property W05_MQR_204P_ToldMegAboutRoccoValue Auto mandatory
actorvalue Property W05_MQR_204P_ToldMegAboutBarbValue Auto mandatory
actorvalue Property Health Auto mandatory
referencealias Property Alias_Rocco Auto mandatory
topic Property W05_MQR_204P_Lou_Freed Auto mandatory
referencealias Property Alias_Barb Auto mandatory
scene Property TiedUpScene Auto
referencealias Property TiedUpFurniture Auto
globalvariable Property Rep_Mod_Add_MQ Auto mandatory
actorvalue Property W05_MQR_204P_LevHideoutActiveValue Auto mandatory
globalvariable Property Rep_Mod_Subtract_Small Auto mandatory
globalvariable Property Rep_Mod_Add_Large Auto mandatory
globalvariable Property Rep_Mod_Add_Medium Auto mandatory
globalvariable Property Rep_Mod_Add_Huge Auto mandatory
actorvalue Property Reputation_AV_Crater Auto mandatory
faction Property BoundCaptiveFaction Auto mandatory
faction Property CaptiveFaction Auto mandatory
referencealias Property Alias_Detonator Auto mandatory
referencealias Property Alias_HideoutKey Auto mandatory
referencealias Property Alias_Surge Auto mandatory
referencealias Property Alias_LouTiedUp Auto mandatory
referencealias Property Alias_Lev Auto mandatory
actorvalue Property W05_MQ_204P_FactionChosen Auto mandatory
keyword Property W05_MQR_205P_QuestStart_Keyword Auto mandatory
referencealias Property Alias_currentPlayer Auto mandatory
actorvalue Property W05_MQR_204P_KillRoccoValue Auto mandatory
actorvalue Property W05_MQR_204P_SaboteurKnownValue Auto mandatory
actorvalue Property W05_MQR_204P_LevManipulatedLouValue Auto mandatory
actorvalue Property W05_MQR_204P_KillBarbValue Auto mandatory
""",
    FREE_LOU: """Scriptname W05_MQR_204P_FreeLouPerkScript Extends Perk

quest Property W05_MQR_204P Auto mandatory
""",
    KEYPAD_OBJECTIVE: """Scriptname W05_MQR_PlayerVault79KeypadObjective Extends ReferenceAlias

ActorValue Property W05_MQ00_CodeAV Auto
LocationAlias Property InstancedLocationAlias Auto
Int Property EndOnStage = -1 Auto
Int Property PreReqStage = -1 Auto
Int Property KeypadObjective = -1 Auto
""",
}

LIVE_BOUND_STAGES = {
    CHOICE: {100, 200, 9000, 9999},
    LEV: {
        2,
        3,
        4,
        100,
        150,
        151,
        160,
        200,
        300,
        310,
        400,
        500,
        510,
        511,
        512,
        513,
        520,
        521,
        522,
        530,
        531,
        532,
        533,
        540,
        541,
        550,
        560,
        600,
        700,
        800,
        810,
        820,
        830,
        840,
        850,
        900,
        950,
        960,
        970,
        1000,
        1100,
        1110,
        1120,
        5000,
        5100,
        5200,
        5210,
        5300,
        5310,
        9000,
        10000,
    },
}

AUXILIARY_SOURCES = {
    LEV: {
        "W05_MQR_204P_QuestScript.psc": """Scriptname W05_MQR_204P_QuestScript Extends Quest

referencealias Property LevHideoutEnableMarker Auto mandatory
referencealias Property LevHideoutEncounterEnableMarker Auto mandatory
referencealias Property LevHideoutLayoutEnableMarker Auto mandatory
        """,
    },
}

EXPECTED_HELPER_MEMBERS = {
    FREE_LOU: {"fragment_entry_00"},
    KEYPAD_OBJECTIVE: {"onlocationchange"},
}

LOU_ALIAS_LIVE_FILL = {
    "alias": 9,
    "editor_id": "Lou",
    "faction": "00058610",
    "faction_editor_id": "BoundCaptiveFaction",
}


def _fragment_member(stage: int) -> str:
    return f"fragment_stage_{stage:04d}_item_00"


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


def _merged_tracked_source(script_name: str) -> str:
    patch = _script_patch_source(script_name)
    assert patch is not None
    return _merge_script_method_patches(ZERO_MEMBER_SKELETONS[script_name], patch)


@pytest.mark.parametrize("script_name", QF_SCRIPTS)
def test_all_live_bound_fragments_are_authored_and_merge_once(script_name: str):
    patch = _script_patch_source(script_name)
    assert patch is not None
    assert "; TODO" not in patch
    assert _iter_papyrus_states(patch.splitlines()) == []
    assert not any(
        line.strip().lower().startswith("scriptname ") for line in patch.splitlines()
    )

    members = _member_names(patch)
    stages = {
        stage
        for stage in LIVE_BOUND_STAGES[script_name]
        if _fragment_member(stage) in members
    }
    assert stages == LIVE_BOUND_STAGES[script_name]
    assert len(members) == len(stages)
    for member_name in members:
        executable_lines = [
            line.strip()
            for line in _member_body(patch, member_name).splitlines()[1:-1]
            if line.strip() and not line.strip().startswith(";")
        ]
        assert executable_lines, member_name

    skeleton = ZERO_MEMBER_SKELETONS[script_name]
    assert _member_names(skeleton) == []
    merged = _merge_script_method_patches(skeleton, patch)
    assert Counter(_member_names(merged)) == Counter(members)
    for member_name in members:
        assert _member_body(merged, member_name) == _member_body(patch, member_name)
    assert _merge_script_method_patches(merged, patch) == merged


@pytest.mark.parametrize("script_name", HELPER_SCRIPTS)
def test_route_helpers_have_exact_live_callback_surfaces(script_name: str):
    patch = _script_patch_source(script_name)
    assert patch is not None
    assert set(_member_names(patch)) == EXPECTED_HELPER_MEMBERS[script_name]

    merged = _merge_script_method_patches(ZERO_MEMBER_SKELETONS[script_name], patch)
    assert Counter(_member_names(merged)) == Counter(
        EXPECTED_HELPER_MEMBERS[script_name]
    )
    assert _merge_script_method_patches(merged, patch) == merged


def test_choice_commit_stops_settlers_then_launches_from_russia_with_lev():
    patch = _script_patch_source(CHOICE)
    assert patch is not None
    body = _member_body(patch, _fragment_member(9000))

    for quest_name in (
        "W05_MQ_102P_B",
        "W05_MQS_201P",
        "W05_MQS_202P",
        "W05_MQS_203P",
        "W05_MQS_Choice",
    ):
        assert f"If {quest_name} != None" in body
        assert f"{quest_name}.Stop()" in body
    set_choice = "playerRef.SetValue(W05_MQR_Choice_QuestComplete, 1.0)"
    handoff = "W05_MQR_204P_QuestStart_Keyword.SendStoryEvent"
    assert body.index("W05_MQ_102P_B.Stop()") < body.index("W05_MQS_201P.Stop()")
    assert body.index("W05_MQS_Choice.Stop()") < body.index(set_choice)
    assert body.index(set_choice) < body.index(handoff)
    assert ".Start()" not in body

    settler = _script_patch_source(SETTLER_CHOICE)
    assert settler is not None
    settler_terminal = _member_body(settler, _fragment_member(9000))
    for quest_name in (
        "W05_MQ_102P_A",
        "W05_MQR_201P",
        "W05_MQR_202P",
        "W05_MQR_203P",
        "W05_MQR_Choice",
    ):
        assert f"If {quest_name} != None" in settler_terminal
        assert f"{quest_name}.Stop()" in settler_terminal
    assert ".Start()" not in settler_terminal


def test_choice_cancel_is_an_authored_failure_exit():
    patch = _script_patch_source(CHOICE)
    assert patch is not None
    cancel = _member_body(patch, _fragment_member(9999))

    assert "SetObjectiveFailed(100)" in cancel
    assert "SendStoryEvent" not in cancel


def test_instance_bound_producers_are_not_bypassed_by_route_fragments():
    patch = _script_patch_source(LEV)
    assert patch is not None

    start = _member_body(patch, _fragment_member(100))
    vault_init = _member_body(patch, _fragment_member(2))
    hideout_init = _member_body(patch, _fragment_member(3))
    rocco_init = _member_body(patch, _fragment_member(4))
    keypad = _member_body(patch, _fragment_member(151))
    hideout = _member_body(patch, _fragment_member(700))

    assert "Alias_InitEnableMarker.GetReference()" in vault_init
    assert "SetStage(2)" not in start
    assert "SetStage(150)" not in start
    for marker in (
        "LevHideoutEnableMarker",
        "LevHideoutEncounterEnableMarker",
        "LevHideoutLayoutEnableMarker",
    ):
        assert f"questScript.{marker}.GetReference()" in hideout_init
    assert "Alias_Rocco.GetReference()" in rocco_init
    assert "SetObjectiveCompleted(150)" in keypad
    assert "SetStage(160)" not in keypad
    assert "SetStage(3)" not in hideout
    assert "SetStage(800)" not in hideout


def test_stage_160_starts_tied_scene_and_stage_300_releases_lou():
    patch = _script_patch_source(LEV)
    assert patch is not None
    creature_encounter = _member_body(patch, _fragment_member(160))
    body = _member_body(patch, _fragment_member(200))

    assert "SetObjectiveDisplayed(160)" in creature_encounter
    assert "TiedUpScene.Start()" in creature_encounter
    assert "SetStage(200)" not in creature_encounter
    assert "SetStage(300)" not in creature_encounter
    assert "SetObjectiveDisplayed(200)" in body
    assert "GetUpScene.Start()" not in body
    assert "TiedUpScene.Stop()" not in body
    assert "SetStage(300)" not in body

    freed = _member_body(patch, _fragment_member(300))
    assert freed.index("TiedUpScene.Stop()") < freed.index("GetUpScene.Start()")
    assert freed.index("GetUpScene.Start()") < freed.index("SetObjectiveCompleted(200)")
    assert freed.index("SetObjectiveCompleted(200)") < freed.index(
        "SetObjectiveDisplayed(300)"
    )


def test_free_lou_is_player_only_stage_bounded_idempotent_and_preserves_alfc():
    patch = _script_patch_source(FREE_LOU)
    assert patch is not None

    assert (
        "Function Fragment_Entry_00(ObjectReference akTargetRef, Actor akActor)"
        in patch
    )
    assert "akActor != Game.GetPlayer()" in patch
    assert "akTargetRef == None" in patch
    assert "W05_MQR_204P.GetStage() == 200" in patch
    assert "!W05_MQR_204P.IsStageDone(300)" in patch
    assert patch.count("W05_MQR_204P.SetStage(300)") == 1
    for forbidden in ("AddToFaction", "RemoveFromFaction", "SetFactionRank"):
        assert forbidden not in patch

    assert LOU_ALIAS_LIVE_FILL == {
        "alias": 9,
        "editor_id": "Lou",
        "faction": "00058610",
        "faction_editor_id": "BoundCaptiveFaction",
    }


def test_keypad_objective_remains_bounded_and_activation_fallback_is_retired():
    objective = _script_patch_source(KEYPAD_OBJECTIVE)
    alias = _script_patch_source(KEYPAD_ALIAS)
    keypad = _script_patch_source(KEYPAD_CONTROLLER)
    note = _script_patch_source(CODE_NOTE)
    controller = _script_patch_source(MQR204_CONTROLLER)
    assert objective is not None
    assert alias is None
    assert keypad is None
    assert note is not None
    assert controller is not None

    assert "targetLocation == None || akNewLoc != targetLocation" in objective
    assert "currentStage >= PreReqStage && currentStage < EndOnStage" in objective
    assert "owningQuest.SetObjectiveDisplayed(KeypadObjective)" in objective

    assert "Utility.RandomInt(100000, 999999)" in controller
    assert "playerRef.AddItem(W05_MQR_Vault79CodeNote, 1, False)" in controller
    assert "Debug.Notification(\"Vault 79 keypad code: \" + vault79Code)" in note


def test_dialogue_fragments_do_not_invent_convergence_but_combat_join_is_explicit():
    patch = _script_patch_source(LEV)
    assert patch is not None

    for completed in (510, 520, 530):
        body = _member_body(patch, _fragment_member(completed))
        assert f"SetObjectiveCompleted({completed})" in body
        assert "SetStage(600)" not in body

    for defeated, other in ((810, 820), (820, 810)):
        body = _member_body(patch, _fragment_member(defeated))
        assert f"IsStageDone({other})" in body
        assert "SetStage(830)" in body

    lou_dialogue = _member_body(patch, _fragment_member(310))
    meg_betrayal = _member_body(patch, _fragment_member(400))
    accusation = _member_body(patch, _fragment_member(600))
    assert "SetStage(400)" not in lou_dialogue
    assert "SetStage(500)" not in meg_betrayal
    assert "SetStage(700)" not in accusation

    lev_bleedout = _member_body(patch, _fragment_member(850))
    assert lev_bleedout.index("levRef.StopCombat()") < lev_bleedout.index(
        "SetStage(900)"
    )
    assert "ResetHealthAndLimbs" not in lev_bleedout
    assert lev_bleedout.index("levRef.EvaluatePackage()") < lev_bleedout.index(
        "SetStage(900)"
    )


def test_detonator_pickup_and_terminal_story_manager_handoff_are_ordered():
    patch = _script_patch_source(LEV)
    assert patch is not None

    detonator = _member_body(patch, _fragment_member(1000))
    assert "SetObjectiveDisplayed(1000)" in detonator
    assert "AddItem" not in detonator
    assert "SetStage(1100)" not in detonator

    terminal = _member_body(patch, _fragment_member(9000))
    chosen = "playerRef.SetValue(W05_MQ_204P_FactionChosen, 1.0)"
    handoff = "W05_MQR_205P_QuestStart_Keyword.SendStoryEvent"
    assert terminal.index(chosen) < terminal.index(handoff)
    assert ".Start()" not in terminal

    settler = _script_patch_source(SETTLER_204)
    assert settler is not None
    settler_terminal = _member_body(settler, _fragment_member(9000))
    assert "playerRef.SetValue(W05_MQ_204P_FactionChosen, 2.0)" in settler_terminal


def test_optional_rocco_route_rejoins_main_handoff_and_shutdown_is_safe():
    patch = _script_patch_source(LEV)
    assert patch is not None

    spared = _member_body(patch, _fragment_member(5210))
    killed = _member_body(patch, _fragment_member(5310))
    assert "SetObjectiveCompleted(5200)" in spared
    assert "playerRef.SetValue(W05_MQR_204P_KillRoccoValue, 1.0)" in killed
    assert "SetStage(5210)" in killed

    shutdown = _member_body(patch, _fragment_member(10000))
    assert "playerRef.SetValue(W05_MQR_204P_LevHideoutActiveValue, 0.0)" in shutdown
    assert "TiedUpScene.Stop()" in shutdown
    assert "GetUpScene.Stop()" in shutdown


def test_excluded_online_reward_and_reputation_systems_are_not_recreated():
    patches = "\n".join(_script_patch_source(name) or "" for name in SCRIPTS)
    for forbidden in (
        "EWS",
        "GoldBullion",
        "Rep_Mod_",
        "Reputation_AV_",
        "QuestReward_",
        "CompleteAllObjectives",
    ):
        assert forbidden not in patches


@pytest.mark.parametrize("script_name", SCRIPTS)
def test_tracked_zero_member_merge_is_idempotent(script_name: str):
    merged = _merged_tracked_source(script_name)
    patch = _script_patch_source(script_name)
    assert patch is not None
    assert Counter(_member_names(merged)) == Counter(_member_names(patch))
    assert _merge_script_method_patches(merged, patch) == merged


@pytest.mark.parametrize("script_name", SCRIPTS)
def test_tracked_zero_member_merge_native_compiles_for_fo4(
    script_name: str, tmp_path: Path
):
    base_source = _fo4_base_source()
    if base_source is None:
        pytest.skip("FO4 base Papyrus sources unavailable")

    for file_name, source in AUXILIARY_SOURCES.get(script_name, {}).items():
        (tmp_path / file_name).write_text(source, encoding="utf-8")

    result = compile_psc(
        _merged_tracked_source(script_name),
        imports=[str(base_source), str(tmp_path)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=f"{script_name.replace(':', '/')}.psc",
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None
