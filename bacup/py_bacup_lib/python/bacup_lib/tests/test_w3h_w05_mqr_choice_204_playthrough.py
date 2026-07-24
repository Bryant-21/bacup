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
    _script_relative_path,
)
from creation_lib.esp import Plugin
from creation_lib.pex import decompile_pex
from creation_lib.pex.native_runtime import compile_psc


REPO_ROOT = Path(__file__).resolve().parents[5]
DEPLOYED_SCRIPTS_ROOT = REPO_ROOT / "mods" / "SeventySix" / "data" / "Scripts"
GENERATED_SOURCE_ROOT = (
    REPO_ROOT / "mods" / "SeventySix" / "Scripts" / "Source" / "User"
)

CHOICE = "Fragments:Quests:QF_W05_MQR_Choice_005930B2"
LEV = "Fragments:Quests:QF_W05_MQR_204P_00535E55"
SCRIPTS = (CHOICE, LEV)

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

ROUTE_STAGES = {
    CHOICE: {100, 200, 9000},
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
        520,
        530,
        540,
        600,
        700,
        800,
        810,
        820,
        830,
        840,
        850,
        900,
        970,
        1000,
        1100,
        5000,
        5100,
        5200,
        5300,
        9000,
    },
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


def _production_skeleton(script_name: str) -> str:
    pex_path = DEPLOYED_SCRIPTS_ROOT / _script_relative_path(script_name, ".pex")
    assert pex_path.is_file(), f"deployed production PEX unavailable: {pex_path}"
    return decompile_pex(pex_path, fo4_api_compat=True)


def _merged_production_source(script_name: str) -> str:
    patch = _script_patch_source(script_name)
    assert patch is not None
    return _merge_script_method_patches(_production_skeleton(script_name), patch)


@pytest.mark.parametrize("script_name", SCRIPTS)
def test_route_patch_uses_only_live_bound_fragments_and_merges_once(script_name: str):
    patch = _script_patch_source(script_name)
    assert patch is not None
    assert patch.splitlines()[0] == "; TODO"
    assert patch.count("; TODO") == 1
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
    assert stages == ROUTE_STAGES[script_name]
    assert len(members) == len(stages)

    skeleton = _production_skeleton(script_name)
    merged = _merge_script_method_patches(skeleton, patch)
    assert Counter(_member_names(merged)) == Counter(members)
    for member_name in members:
        assert _member_body(merged, member_name) == _member_body(patch, member_name)
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


def test_hollow_instance_producers_have_guarded_bound_stage_substitutes():
    patch = _script_patch_source(LEV)
    assert patch is not None

    start = _member_body(patch, _fragment_member(100))
    keypad_objective = _member_body(patch, _fragment_member(150))
    vault_init = _member_body(patch, _fragment_member(2))
    hideout_init = _member_body(patch, _fragment_member(3))
    rocco_init = _member_body(patch, _fragment_member(4))
    keypad = _member_body(patch, _fragment_member(151))
    hideout = _member_body(patch, _fragment_member(700))

    assert "Alias_InitEnableMarker.GetReference()" in vault_init
    assert start.index("Alias_InitEnableMarker.GetReference()") < start.index(
        "SetStage(2)"
    )
    assert start.index("SetStage(2)") < start.index("SetStage(150)")
    assert keypad_objective.index("SetObjectiveCompleted(100)") < keypad_objective.index(
        "SetObjectiveDisplayed(150)"
    )
    for marker in (
        "LevHideoutEnableMarker",
        "LevHideoutEncounterEnableMarker",
        "LevHideoutLayoutEnableMarker",
    ):
        assert f"questScript.{marker}.GetReference()" in hideout_init
    assert "Alias_Rocco.GetReference()" in rocco_init
    assert "SetObjectiveCompleted(150)" in keypad
    assert "Alias_Lou.GetActorReference()" in keypad
    assert "Alias_Creature.GetActorReference()" in keypad
    assert keypad.index("louRef != None && creatureRef != None") < keypad.index(
        "SetStage(160)"
    )

    for marker in (
        "LevHideoutEnableMarker",
        "LevHideoutEncounterEnableMarker",
        "LevHideoutLayoutEnableMarker",
    ):
        assert f"questScript.{marker} == None" in hideout
        assert f"questScript.{marker}.GetReference()" in hideout
    for actor_alias in ("Alias_Lev", "Alias_Fisher", "Alias_Surge"):
        assert f"{actor_alias}.GetActorReference()" in hideout
    assert hideout.index("SetStage(3)") < hideout.index("SetStage(800)")


def test_live_creature_alias_produces_stage_200_after_stage_160():
    plugin = Plugin.load(
        REPO_ROOT / "mods" / "SeventySix" / "SeventySix.esm",
        game="fo4",
        lazy_index=True,
    )
    quest = plugin.read_authoring_record(0x00535E55)
    assert quest is not None
    assert quest["eid"] == "W05_MQR_204P"

    vmad = quest["fields"][0]["VirtualMachineAdapter"]
    creature_alias = next(
        alias
        for alias in vmad["Script Fragments"]["Aliases"]
        if alias["Object"]["Alias"] == 25
    )
    death_script = next(
        script
        for script in creature_alias["Alias Scripts"]
        if script["ScriptName"] == "DefaultAliasOnDeath"
    )
    properties = {
        prop["propertyName"]: prop["Value"] for prop in death_script["Properties"]
    }
    assert properties == {
        "StageToSet": 200,
        "UseOnDyingInstead": True,
        "preReqStage": 160,
    }

    alias_start = next(
        index
        for index, field in enumerate(quest["fields"])
        if field.get("ALST") == 25
    )
    alias_fields = quest["fields"][alias_start:]
    alias_end = next(
        index for index, field in enumerate(alias_fields) if "ALED" in field
    )
    alias_fields = alias_fields[: alias_end + 1]
    assert {"ALID": "Creature"} in alias_fields
    assert {"ALFA": 4} in alias_fields
    assert {
        "ALRT": {
            "reference": {
                "plugin": "SeventySix.esm",
                "object_id": "56FA79",
            }
        }
    } in alias_fields


def test_creature_death_precedes_guarded_free_lou_single_player_substitute():
    patch = _script_patch_source(LEV)
    assert patch is not None
    creature_encounter = _member_body(patch, _fragment_member(160))
    body = _member_body(patch, _fragment_member(200))

    assert "SetObjectiveDisplayed(160)" in creature_encounter
    assert "SetStage(200)" not in creature_encounter
    assert "SetStage(300)" not in creature_encounter
    assert "Alias_Lou == None || GetUpScene == None" in body
    assert "SetStage(200)" not in body
    assert body.index("TiedUpScene.Stop()") < body.index("GetUpScene.Start()")
    assert body.index("GetUpScene.Start()") < body.index("SetStage(300)")
    assert body.count("SetStage(300)") == 1


def test_dialogue_and_combat_convergences_keep_native_scene_handoffs():
    patch = _script_patch_source(LEV)
    assert patch is not None

    for completed, others in ((510, (520, 530)), (520, (510, 530)), (530, (510, 520))):
        body = _member_body(patch, _fragment_member(completed))
        assert f"SetObjectiveCompleted({completed})" in body
        for other in others:
            assert f"IsStageDone({other})" in body
        assert "SetStage(600)" in body

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
        "levRef.ResetHealthAndLimbs()"
    )
    assert lev_bleedout.index("levRef.EvaluatePackage()") < lev_bleedout.index(
        "SetStage(900)"
    )


def test_detonator_pickup_and_terminal_story_manager_handoff_are_ordered():
    patch = _script_patch_source(LEV)
    assert patch is not None

    detonator = _member_body(patch, _fragment_member(1000))
    assert "Alias_Detonator.GetReference()" in detonator
    assert "playerRef.AddItem(detonatorRef, 1, False)" in detonator
    assert "SetStage(1100)" not in detonator

    terminal = _member_body(patch, _fragment_member(9000))
    chosen = "playerRef.SetValue(W05_MQ_204P_FactionChosen, 1.0)"
    handoff = "W05_MQR_205P_QuestStart_Keyword.SendStoryEvent"
    assert terminal.index(chosen) < terminal.index(handoff)
    assert ".Start()" not in terminal


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
def test_full_production_merge_native_compiles_for_fo4(script_name: str):
    base_source = _fo4_base_source()
    assert base_source is not None, "FO4 base Papyrus sources unavailable"

    result = compile_psc(
        _merged_production_source(script_name),
        imports=[str(base_source), str(GENERATED_SOURCE_ROOT)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=f"{script_name.replace(':', '/')}.psc",
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None
