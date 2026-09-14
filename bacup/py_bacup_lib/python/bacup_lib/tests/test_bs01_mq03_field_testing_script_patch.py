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
from creation_lib.pex.native_runtime import compile_psc


REPO_ROOT = Path(__file__).resolve().parents[5]
SOURCE_ROOT = REPO_ROOT / "mods" / "SeventySix" / "Scripts" / "Source" / "User"
SCRIPT_NAME = "Fragments:Quests:QF_BS01_FieldTesting_005C70CD"
ENCOUNTER_SCRIPT_NAME = "defaultquestencounterwavescript"

EXPECTED_MEMBERS = frozenset(
    {
        "fragment_stage_0100_item_00",
        "fragment_stage_0150_item_00",
        "fragment_stage_0151_item_00",
        "fragment_stage_0200_item_00",
        "fragment_stage_0300_item_00",
        "fragment_stage_0302_item_00",
        "fragment_stage_0400_item_00",
        "fragment_stage_0500_item_00",
        "fragment_stage_0600_item_00",
        "fragment_stage_0700_item_00",
        "fragment_stage_0700_item_01",
        "fragment_stage_0750_item_00",
        "fragment_stage_0800_item_00",
        "fragment_stage_0800_item_01",
        "fragment_stage_0800_item_02",
        "fragment_stage_0810_item_00",
        "fragment_stage_0900_item_00",
        "fragment_stage_0980_item_00",
        "fragment_stage_0990_item_00",
        "fragment_stage_1000_item_00",
        "fragment_stage_1000_item_01",
        "fragment_stage_1000_item_02",
        "fragment_stage_1100_item_00",
        "fragment_stage_9000_item_00",
    }
)
EXPECTED_QUEST_MEMBERS = EXPECTED_MEMBERS | {"ontimer"}
EXPECTED_ENCOUNTER_MEMBERS = {
    "onquestinit",
    "onquestshutdown",
    "actor.onplayerloadgame",
    "startlocalencounterwave",
    "handlelocalencounteractordeath",
    "reconcilelocalencounterwaveregistrations",
    "registerlocalwavecollection",
    "unregisterlocalwavecollection",
    "localwavecontainsreference",
    "localwavehasanyreference",
    "countlivinglocalwaveactors",
    "countlivingcollectionactors",
    "evaluatelocalencounterwave",
    "setlocalencounterstageifpending",
    "startbs01fieldtestinglocalencounter",
    "completebs01fieldtestinglocalencounterifalldead",
    "actor.ondeath",
}


def _patch() -> str:
    patch = _script_patch_source(SCRIPT_NAME)
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
    lines = source.splitlines()
    start, end = next(
        (start, end)
        for _kind, name, start, end in _iter_top_level_papyrus_members(lines)
        if name == member_name.lower()
    )
    return "\n".join(lines[start : end + 1])


def _stage_body(stage: int, item: int = 0) -> str:
    return _member_body(_patch(), f"fragment_stage_{stage:04d}_item_{item:02d}")


def _merged_source() -> str:
    source_path = SOURCE_ROOT / _script_relative_path(SCRIPT_NAME, ".psc")
    assert source_path.is_file(), source_path
    return _merge_script_method_patches(
        source_path.read_text(encoding="utf-8"), _patch()
    )


def _encounter_patch() -> str:
    patch = _script_patch_source(ENCOUNTER_SCRIPT_NAME)
    assert patch is not None
    return patch


def _merged_encounter_source() -> str:
    source_path = SOURCE_ROOT / _script_relative_path(ENCOUNTER_SCRIPT_NAME, ".psc")
    assert source_path.is_file(), source_path
    return _merge_script_method_patches(
        source_path.read_text(encoding="utf-8"), _encounter_patch()
    )


def test_all_vmad_bound_members_merge_exactly_once_without_duplicates():
    patch = _patch()
    patch_members = _member_names(patch)

    assert Counter(patch_members) == Counter(
        {member: 1 for member in EXPECTED_QUEST_MEMBERS}
    )
    assert not any(
        line.strip().lower().startswith("scriptname ") for line in patch.splitlines()
    )

    merged = _merged_source()
    merged_members = _member_names(merged)
    assert Counter(merged_members) == Counter(
        {member: 1 for member in EXPECTED_QUEST_MEMBERS}
    )
    for member in EXPECTED_QUEST_MEMBERS:
        assert _member_body(merged, member) == _member_body(patch, member)
    assert _merge_script_method_patches(merged, patch) == merged

    encounter_patch = _encounter_patch()
    assert "TotalNumLegendaryCreaturesToSpawn = -1" in _member_body(
        encounter_patch, "startbs01fieldtestinglocalencounter"
    )
    expected_encounter_members = Counter(
        {member: 1 for member in EXPECTED_ENCOUNTER_MEMBERS}
    )
    assert Counter(_member_names(encounter_patch)) == expected_encounter_members
    merged_encounter = _merged_encounter_source()
    assert Counter(_member_names(merged_encounter)) == expected_encounter_members
    assert (
        _merge_script_method_patches(merged_encounter, encounter_patch)
        == merged_encounter
    )


def test_objectives_scene_and_optional_aid_follow_field_testing_chronology():
    start = _stage_body(100)
    assert "Alias_Player.ForceRefTo(playerRef)" in start
    assert "pBS01_MQ02_Invention_ValdezRep_UpsetThreshold.GetValue()" in start
    assert "playerRef.AddItem(recommendationLetter, 1, False)" in start
    assert "SetObjectiveDisplayed(100)" in start

    rahmani = _stage_body(150)
    assert "Alias_Item_ValdezLetter.Clear()" in rahmani
    assert "SetObjectiveCompleted(100)" in rahmani
    assert "SetObjectiveDisplayed(200)" in rahmani
    assert rahmani.index("SetObjectiveCompleted(100)") < rahmani.index(
        "SetObjectiveDisplayed(200)"
    )
    assert "playerRef.AddItem(Item_Stimpak, 1, False)" in _stage_body(151)

    lewis_and_sons = _stage_body(200)
    assert "SetObjectiveCompleted(200)" in lewis_and_sons
    assert "SetObjectiveDisplayed(300)" in lewis_and_sons
    assert "Scene_Putnam_Intro.Start()" in lewis_and_sons

    putnams = _stage_body(300)
    for objective in (300, 400, 500, 600):
        call = (
            f"SetObjectiveCompleted({objective})"
            if objective == 300
            else f"SetObjectiveDisplayed({objective})"
        )
        assert call in putnams
    assert "playerRef.AddItem(Item_RadX, 1, False)" in _stage_body(302)

    orwell = _stage_body(600)
    assert "playerRef.SetValue(BS01_ReachedOrwell, 1.0)" in orwell
    assert "SetObjectiveCompleted(600)" in orwell
    assert "SetObjectiveDisplayed(700)" in orwell
    assert "SetObjectiveCompleted(700)" in _stage_body(700)
    assert "SetObjectiveDisplayed(800)" in _stage_body(700)
    assert "Alias_EnableMarker_BunkerTriggers.GetReference()" in _stage_body(700, 1)


def test_marty_and_colin_choices_remain_mutually_exclusive_and_reward_visible():
    marty = _stage_body(400)
    assert "playerRef.SetValue(BS01_RecruitedMarty, 1.0)" in marty
    assert "playerRef.SetValue(BS01_RecruitedColin, 0.0)" in marty
    assert "SetObjectiveCompleted(400)" in marty
    assert "SetObjectiveFailed(500)" in marty
    assert "Alias_EnableMarker_MartyBunker.GetReference()" in marty

    colin = _stage_body(500)
    assert "playerRef.SetValue(BS01_RecruitedColin, 1.0)" in colin
    assert "playerRef.SetValue(BS01_RecruitedMarty, 0.0)" in colin
    assert "SetObjectiveCompleted(500)" in colin
    assert "SetObjectiveFailed(400)" in colin
    assert "Alias_EnableMarker_ColinBunker.GetReference()" in colin

    assert "SetObjectiveDisplayed(900)" in _stage_body(800, 1)
    assert "SetObjectiveDisplayed(910)" in _stage_body(800, 2)
    assert "playerRef.SetValue(BS01_MartyPostBunker, 1.0)" in _stage_body(1000, 1)
    assert "SetObjectiveDisplayed(1100)" in _stage_body(1000, 1)
    assert "playerRef.SetValue(BS01_ColinPostBunker, 1.0)" in _stage_body(1000, 2)
    assert "Alias_EnableMarker_ColinBunker.GetReference()" in _stage_body(1000, 2)
    assert "colinMarker.Enable()" in _stage_body(1000, 2)
    assert "SetObjectiveDisplayed(1110)" in _stage_body(1000, 2)

    debrief = _stage_body(1100)
    assert "SetObjectiveCompleted(1100)" in debrief
    assert "SetObjectiveCompleted(1110)" in debrief
    assert "SetObjectiveDisplayed(1200)" in debrief


def test_local_wave_substitute_uses_bound_aliases_and_converges_once():
    no_companion = _stage_body(800)
    assert no_companion.index(
        "!recruitedMarty && !recruitedColin"
    ) < no_companion.index("encounterController.StartBS01FieldTestingLocalEncounter()")
    assert "SetStage(810)" not in no_companion

    for entry_stage in (800, 900):
        body = _stage_body(entry_stage)
        assert "Alias_SpawnCenter_Bunker_LivingArea.GetReference()" in body
        assert "Alias_SpawnCenter_Bunker_Pool.GetReference()" in body
        assert "(Self as Quest) as DefaultQuestEncounterWaveScript" in body
        assert "encounterController.StartBS01FieldTestingLocalEncounter()" in body
        assert "SetStage(980)" not in body
        assert "SetStage(990)" not in body

    colin_hides = _stage_body(810)
    assert "Alias_EnableMarker_ColinBunker.GetReference()" in colin_hides
    assert "colinMarker.Disable()" in colin_hides
    assert "SetObjectiveDisplayed(1000)" not in colin_hides
    assert "StartBS01FieldTestingLocalEncounter" not in colin_hides
    assert "SetStage(" not in colin_hides

    living_area_done = _stage_body(980)
    pool_done = _stage_body(990)
    assert "Alias_SpawnCenter_Bunker_LivingArea.GetReference()" in living_area_done
    assert "Alias_SpawnCenter_Bunker_Pool.GetReference()" in pool_done
    for body in (living_area_done, pool_done):
        assert "IsStageDone(980) && IsStageDone(990) && !IsStageDone(1000)" in body
        assert "SetStage(1000)" in body

    encounter_patch = _encounter_patch()
    assert "GetAlias(31) as RefCollectionAlias" in encounter_patch
    assert 'RegisterForRemoteEvent(enemyRef, "OnDeath")' in encounter_patch
    assert "Event Actor.OnDeath(Actor akSender, Actor akKiller)" in encounter_patch
    assert (
        "CompleteBS01FieldTestingLocalEncounterIfAllDead(akSender)" in encounter_patch
    )
    death = _member_body(encounter_patch, "actor.ondeath")
    assert "If TotalNumLegendaryCreaturesToSpawn < 0" in death
    assert death.index("CompleteBS01FieldTestingLocalEncounterIfAllDead") < death.index(
        "Else"
    )
    assert death.index("Else") < death.index("HandleLocalEncounterActorDeath")
    assert "If enemyCount <= 0" in encounter_patch
    assert "If akConfirmedDeath == None" in encounter_patch
    assert "If !foundEnemy && akConfirmedDeath == None" in encounter_patch
    assert "If !enemyRef.IsDead()" in encounter_patch
    assert encounter_patch.rindex("If !enemyRef.IsDead()") < encounter_patch.index(
        "SetStage(980)"
    )
    assert encounter_patch.index("SetStage(980)") < encounter_patch.index(
        "SetStage(990)"
    )
    combined = "\n".join(
        [_stage_body(stage) for stage in (810, 900, 980, 990)]
        + [
            _member_body(encounter_patch, "startbs01fieldtestinglocalencounter"),
            _member_body(
                encounter_patch,
                "completebs01fieldtestinglocalencounterifalldead",
            ),
        ]
    )
    for forbidden in (
        "EncounterWaves",
        "SendCustomEvent",
        "Community",
        "Bounty",
    ):
        assert forbidden not in combined


def test_terminal_handoff_retries_rejection_and_stops_after_acceptance():
    terminal = _stage_body(9000)
    assert terminal.index("Alias_Player.GetActorReference()") < terminal.index(
        "playerRef = Game.GetPlayer()"
    )
    assert "playerRef.SetValue(BS01_AV_IsInitiate, 1.0)" in terminal
    assert "SetObjectiveCompleted(1200)" in terminal
    assert (
        "handoffAccepted = pBS01_MQ04_Arms_StartKeyword.SendStoryEventAndWait(None, playerRef, playerRef)"
        in terminal
    )
    assert terminal.index("If !handoffAccepted") < terminal.index(
        "StartTimer(5.0, 9000)"
    )

    retry = _member_body(_patch(), "ontimer")
    assert "If aiTimerID != 9000 || !IsStageDone(9000)" in retry
    assert retry.index("Alias_Player.GetActorReference()") < retry.index(
        "playerRef = Game.GetPlayer()"
    )
    assert (
        "handoffAccepted = pBS01_MQ04_Arms_StartKeyword.SendStoryEventAndWait(None, playerRef, playerRef)"
        in retry
    )
    assert retry.index("If !handoffAccepted") < retry.index("StartTimer(5.0, 9000)")
    assert retry.count("StartTimer(5.0, 9000)") == 1
    assert "Else" not in retry

    combined = terminal + retry
    assert ".Start()" not in terminal
    assert "SendStoryEvent(" not in combined
    assert "Utility.Wait" not in combined
    assert "While" not in combined
    assert "Stop()" not in combined
    assert "CompleteQuest" not in combined


def test_full_production_merge_native_compiles_for_fo4(tmp_path: Path):
    base_source = _fo4_base_source()
    assert base_source is not None, "FO4 base Papyrus sources unavailable"

    encounter_source = _merged_encounter_source()
    encounter_result = compile_psc(
        encounter_source,
        imports=[str(base_source), str(SOURCE_ROOT)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=f"{ENCOUNTER_SCRIPT_NAME}.psc",
    )
    encounter_diagnostics = "\n".join(
        str(item) for item in encounter_result.diagnostics
    )
    assert encounter_result.ok, encounter_diagnostics
    assert encounter_result.pex_bytes is not None

    patched_import = tmp_path / f"{ENCOUNTER_SCRIPT_NAME}.psc"
    patched_import.write_text(encounter_source, encoding="utf-8")

    result = compile_psc(
        _merged_source(),
        imports=[str(tmp_path), str(base_source), str(SOURCE_ROOT)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=f"{SCRIPT_NAME.replace(':', '/')}.psc",
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None
