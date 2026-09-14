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

BASIC_FRAGMENT_SCRIPT = "Fragments:Quests:QF_EN05_Basic_0008C87F"
BASIC_MISC_FRAGMENT_SCRIPT = "Fragments:Quests:QF_EN05_Basic_Misc_002C5EB1"
BASIC_QUEST_SCRIPT = "EN05_QuestScript"
OFFICER_FRAGMENT_SCRIPT = "Fragments:Quests:QF_EN05_MQ_Officer_0010DBEA"
BASIC_ALIAS_SCRIPT = "EN05_Basic_PlayerAliasScript"
OFFICER_QUEST_SCRIPT = "EN05_MQ_QuestScript"
OFFICER_ALIAS_SCRIPT = "EN05_MQ_PlayerAliasScript"

# Exactly the stages declared by each QUST VMAD fragment table. Adding a
# fragment for an undeclared stage is dead code; omitting a declared one makes
# the engine log a missing-fragment error every time that stage is set.
BASIC_FRAGMENT_STAGES = (
    4,
    5,
    10,
    11,
    12,
    14,
    15,
    20,
    30,
    40,
    50,
    100,
    135,
    140,
    150,
    200,
    201,
)
OFFICER_FRAGMENT_STAGES = (5, 7, 10, 25, 100, 105, 107, 110)
BASIC_MISC_FRAGMENT_STAGES = (10, 100)

PATCH_MEMBERS = {
    BASIC_FRAGMENT_SCRIPT: {
        f"fragment_stage_{stage:04d}_item_00" for stage in BASIC_FRAGMENT_STAGES
    }
    | {
        "en05basic_getplayer",
        "en05basic_recordstage",
        "en05basic_tallycourses",
    },
    BASIC_MISC_FRAGMENT_SCRIPT: {
        f"fragment_stage_{stage:04d}_item_00"
        for stage in BASIC_MISC_FRAGMENT_STAGES
    },
    BASIC_QUEST_SCRIPT: {"en05basic_coursecompleted"},
    OFFICER_FRAGMENT_SCRIPT: {
        f"fragment_stage_{stage:04d}_item_00" for stage in OFFICER_FRAGMENT_STAGES
    }
    | {"en05mq_getplayer"},
    BASIC_ALIAS_SCRIPT: {
        "en05basic_istrackingoutfit",
        "en05basic_updateoutfitobjective",
        "onitemequipped",
        "onitemunequipped",
    },
    OFFICER_QUEST_SCRIPT: {
        "en05mq_getplayer",
        "en05mq_historiccommendations",
        "en05mq_reconcilecommendations",
        "en05mq_addcommendations",
        "onquestinit",
    },
    OFFICER_ALIAS_SCRIPT: {
        "en05mq_alreadycounted",
        "en05mq_rememberkill",
        "en05mq_commendationvaluefor",
        "onkill",
        "ontimer",
    },
}


def _member_names(source: str) -> list[str]:
    return [
        name
        for _kind, name, _start, _end in _iter_top_level_papyrus_members(
            source.splitlines()
        )
    ]


def _member_body(source: str, member_name: str) -> str:
    lines = source.splitlines()
    start, end = next(
        (start, end)
        for _kind, name, start, end in _iter_top_level_papyrus_members(lines)
        if name == member_name.casefold()
    )
    return "\n".join(lines[start : end + 1])


def _merged_source(script_name: str) -> str:
    source_path = SOURCE_ROOT / _script_relative_path(script_name, ".psc")
    patch = _script_patch_source(script_name)
    assert source_path.is_file(), source_path
    assert patch is not None
    return _merge_script_method_patches(source_path.read_text(encoding="utf-8"), patch)


@pytest.mark.parametrize(("script_name", "members"), PATCH_MEMBERS.items())
def test_en05_patches_are_member_only_and_exact(
    script_name: str, members: set[str]
) -> None:
    patch = _script_patch_source(script_name)

    assert patch is not None
    member_names = _member_names(patch)
    assert len(member_names) == len(members)
    assert set(member_names) == members
    assert not any(
        line.strip().casefold().startswith(("scriptname ", "state ", "extends "))
        for line in patch.splitlines()
    )
    assert not any(
        " property " in f" {line.strip().casefold()} " for line in patch.splitlines()
    )


@pytest.mark.parametrize(("script_name", "members"), PATCH_MEMBERS.items())
def test_en05_production_merges_are_unique_and_idempotent(
    script_name: str, members: set[str]
) -> None:
    patch = _script_patch_source(script_name)
    assert patch is not None
    merged = _merged_source(script_name)
    merged_members = _member_names(merged)

    for member in members:
        assert merged_members.count(member) == 1
        assert _member_body(merged, member) == _member_body(patch, member)
    assert _merge_script_method_patches(merged, patch) == merged


@pytest.mark.parametrize("script_name", PATCH_MEMBERS)
def test_en05_production_merges_compile(script_name: str, tmp_path: Path) -> None:
    merged = _merged_source(script_name)
    base_source = _fo4_base_source()
    assert base_source is not None, "FO4 base Papyrus sources unavailable"

    for dependency_name in PATCH_MEMBERS:
        dependency_path = tmp_path / _script_relative_path(dependency_name, ".psc")
        dependency_path.parent.mkdir(parents=True, exist_ok=True)
        dependency_path.write_text(
            _merged_source(dependency_name),
            encoding="utf-8",
        )

    result = compile_psc(
        merged,
        imports=[str(tmp_path), str(SOURCE_ROOT), str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=str(_script_relative_path(script_name, ".psc")),
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None


def test_basic_course_tally_is_derived_from_stages_not_incremented() -> None:
    patch = _script_patch_source(BASIC_FRAGMENT_SCRIPT)
    assert patch is not None

    tally = _member_body(patch, "en05basic_tallycourses")
    for course_stage in (30, 40, 50):
        assert f"IsStageDone({course_stage})" in tally
    assert "+= 1" in tally
    assert "basic.iCoursesCompletedCount = completed" in tally
    assert "completed >= 3 && !IsStageDone(90)" in tally
    assert tally.index("SetObjectiveDisplayed(90)") < tally.index("SetStage(90)")

    for course_stage, objective in ((30, 30), (40, 40), (50, 50)):
        body = _member_body(patch, f"fragment_stage_{course_stage:04d}_item_00")
        assert f"SetObjectiveCompleted({objective})" in body
        assert "EN05Basic_TallyCourses()" in body


def test_basic_uniform_grant_is_idempotent_and_optional_safe() -> None:
    patch = _script_patch_source(BASIC_FRAGMENT_SCRIPT)
    assert patch is not None

    body = _member_body(patch, "fragment_stage_0015_item_00")
    assert "ClothesFatiguesPostWar != None" in body
    assert "!player.WornHasKeyword(EN05_MilitaryFatigueKeyword)" in body
    assert "player.GetItemCount(ClothesFatiguesPostWar) < 1" in body
    assert "!player.WornHasKeyword(EN05_MilitaryHelmetKeyword)" in body
    assert "player.GetItemCount(Armor_Army_Helmet_postwar) < 1" in body
    assert body.count("player.AddItem(ClothesFatiguesPostWar, 1, True)") == 1
    assert body.count("player.AddItem(Armor_Army_Helmet_postwar, 1, True)") == 1
    assert "player.RemoveItem(EN05_UniformVoucher" in body


def test_basic_flag_aliases_are_forced_then_cleared() -> None:
    patch = _script_patch_source(BASIC_FRAGMENT_SCRIPT)
    assert patch is not None

    assert (
        "Alias_PlayerCanCollectUniform.ForceRefTo(player)"
        in _member_body(patch, "fragment_stage_0012_item_00")
    )
    assert (
        "Alias_PlayerCanCollectUniform.Clear()"
        in _member_body(patch, "fragment_stage_0020_item_00")
    )
    assert (
        "Alias_PlayerReadyForCombat.ForceRefTo(player)"
        in _member_body(patch, "fragment_stage_0100_item_00")
    )
    assert (
        "Alias_PlayerReadyForCombat.Clear()"
        in _member_body(patch, "fragment_stage_0140_item_00")
    )

    shutdown = _member_body(patch, "fragment_stage_0201_item_00")
    assert "Alias_PlayerCanCollectUniform.Clear()" in shutdown
    assert "Alias_PlayerReadyForCombat.Clear()" in shutdown


def test_basic_graduation_hands_off_to_officer_questline() -> None:
    patch = _script_patch_source(BASIC_FRAGMENT_SCRIPT)
    assert patch is not None

    body = _member_body(patch, "fragment_stage_0150_item_00")
    assert "player.SetValue(EN05_CompletedValue, 1.0)" in body
    assert (
        "EN05_BackToMODUSMiscQuestStartKeyword.SendStoryEvent(None, player, player)"
        in body
    )
    assert body.index("CompleteAllObjectives()") < body.index(
        "EN05_BackToMODUSMiscQuestStartKeyword.SendStoryEvent("
    )


def test_basic_graduation_relies_on_converted_rewards_and_story_manager() -> None:
    patch = _script_patch_source(BASIC_FRAGMENT_SCRIPT)
    assert patch is not None

    body = _member_body(patch, "fragment_stage_0150_item_00")
    assert "AddItem(" not in body
    assert "RewardPlayerXP(" not in body
    assert "FolderPapersArmyRegistration01" not in body
    assert "pBoS02SoldierCertificate" not in body
    assert 'Game.GetFormFromFile(0x0010DBEA, "SeventySix.esm")' not in body
    assert ".Start()" not in body
    assert "pBoS02.SetStage(500)" not in body


def test_basic_controller_reconciles_all_four_course_reports() -> None:
    patch = _script_patch_source(BASIC_QUEST_SCRIPT)
    assert patch is not None

    body = _member_body(patch, "en05basic_coursecompleted")
    for course_id, completion_stage in (
        ("iMarkmanshipCourseID", "iMarksmanshipCompleteStage"),
        ("iObstacleCourseID", "iObstacleCompleteStage"),
        ("iPatriotismCourseID", "iPatriotismCompleteStage"),
    ):
        assert f"aiCourseID == {course_id}" in body
        assert f"stageToSet = {completion_stage}" in body

    assert "aiCourseID == iCombatCourseID" in body
    assert "IsStageDone(iInitialCoursesCompletedStage)" in body
    assert "stageToSet = iCombatCompleteStage" in body
    assert "stageToSet = iCombatCompletedEarlyStage" in body
    assert "stageToSet >= 0 && !IsStageDone(stageToSet)" in body
    assert body.count("SetStage(stageToSet)") == 1


def test_basic_misc_breadcrumb_marks_start_and_cleans_up() -> None:
    patch = _script_patch_source(BASIC_MISC_FRAGMENT_SCRIPT)
    assert patch is not None

    started = _member_body(patch, "fragment_stage_0010_item_00")
    assert "Alias_currentPlayer.GetActorReference()" in started
    assert "player = Game.GetPlayer()" in started
    assert "Alias_currentPlayer.ForceRefTo(player)" in started
    assert "player.SetValue(EN05_Basic_MiscStartedValue, 1.0)" in started
    assert "SetObjectiveDisplayed(10)" in started

    cleanup = _member_body(patch, "fragment_stage_0100_item_00")
    assert cleanup.index("SetObjectiveCompleted(10)") < cleanup.index(
        "Alias_currentPlayer.Clear()"
    )
    assert cleanup.index("Alias_currentPlayer.Clear()") < cleanup.index("Stop()")
    assert "SendStoryEvent" not in patch
    assert ".Start()" not in patch


def test_officer_completion_chains_to_en07_and_marks_player() -> None:
    patch = _script_patch_source(OFFICER_FRAGMENT_SCRIPT)
    assert patch is not None

    body = _member_body(patch, "fragment_stage_0110_item_00")
    assert "player.SetValue(EN05_MQ_CompletedValue, 1.0)" in body
    assert "EN07_IntroMiscQuestStartKeyword.SendStoryEvent(" in body
    assert ".Start()" not in body
    assert "EN07.SetStage(" not in body

    presidential = _member_body(patch, "fragment_stage_0025_item_00")
    assert "player.SetValue(EN05_Officer_StartedPresidentalRaceValue, 1.0)" in presidential
    assert "!IsStageDone(105)" in presidential
    assert "SetStage(105)" in presidential

    shortcut = _member_body(patch, "fragment_stage_0105_item_00")
    assert "!IsStageDone(110)" in shortcut
    assert "SetStage(110)" in shortcut


def test_basic_alias_tracks_outfit_only_inside_the_bound_stage_window() -> None:
    patch = _script_patch_source(BASIC_ALIAS_SCRIPT)
    assert patch is not None

    guard = _member_body(patch, "en05basic_istrackingoutfit")
    assert "owner.GetStageDone(iTrackOutfitStage)" in guard
    assert "!owner.GetStageDone(iStopTrackingOutfitStage)" in guard

    update = _member_body(patch, "en05basic_updateoutfitobjective")
    assert "EN05_MilitaryFatiguesFormlist.HasForm(akBaseObject)" in update
    assert "owner.SetObjectiveCompleted(10, abEquipped)" in update
    assert "EN05_MilitaryHelmetFormlist.HasForm(akBaseObject)" in update
    assert "owner.SetObjectiveCompleted(11, abEquipped)" in update

    assert (
        "EN05Basic_UpdateOutfitObjective(akBaseObject, True)"
        in _member_body(patch, "onitemequipped")
    )
    assert (
        "EN05Basic_UpdateOutfitObjective(akBaseObject, False)"
        in _member_body(patch, "onitemunequipped")
    )


def test_officer_kill_commendations_use_bound_globals_and_dedupe() -> None:
    patch = _script_patch_source(OFFICER_ALIAS_SCRIPT)
    assert patch is not None

    value_for = _member_body(patch, "en05mq_commendationvaluefor")
    assert "akVictim.GetValue(EpicRankAV) >= EN05_Officer_EpicAVThreshold.GetValue()" in value_for
    assert "EN05_Officer_Legendary_KillCommendationValue.GetValue() as Int" in value_for
    assert "akVictim.GetRace() == ScorchBeastRace" in value_for
    assert "akVictim.HasKeyword(CB15_QuestKillTarget)" in value_for
    assert "EN05_MQ_KillCommendenationValue.GetValue() as Int" in value_for
    assert "Return 0" in value_for

    on_kill = _member_body(patch, "onkill")
    assert "!owner.GetStageDone(iCommendationBegunStage)" in on_kill
    assert "owner.GetStageDone(iCommendationCompletionStage)" in on_kill
    assert "EN05MQ_AlreadyCounted(akVictim)" in on_kill
    assert on_kill.index("EN05MQ_RememberKill(akVictim)") < on_kill.index(
        "officer.EN05MQ_AddCommendations(awarded)"
    )
    assert "player.GetValue(EN05_Officer_KillCompletedValue) + 1.0" in on_kill
    assert on_kill.count("officer.EN05MQ_AddCommendations(awarded)") == 1
    assert "officer.iCurrentCommendations" not in on_kill
    assert "owner.SetStage(iCommendationCompletionStage)" not in on_kill
    assert "iCombatCommendationObjIndex" not in on_kill

    remember = _member_body(patch, "en05mq_rememberkill")
    assert "KilledEpics = new Actor[1]" in remember
    assert "KilledEpics.Add(akVictim)" in remember
    assert "StartTimer(iKilledListTimerLength as Float, iClearKilledListTimerID)" in remember

    assert "KilledEpics = None" in _member_body(patch, "ontimer")


def test_officer_controller_reconciles_checkpoint_and_historic_progress() -> None:
    patch = _script_patch_source(OFFICER_QUEST_SCRIPT)
    assert patch is not None

    historic = _member_body(patch, "en05mq_historiccommendations")
    assert "While index < TimesCompletedValues.Length" in historic
    assert "akPlayer.GetValue(completedValue) as Int" in historic
    assert "If value > 0" in historic

    reconcile = _member_body(patch, "en05mq_reconcilecommendations")
    assert "If historic > iCurrentCommendations" in reconcile
    assert "iCurrentCommendations = historic" in reconcile
    assert "iCurrentCommendations > threshold" in reconcile
    assert "SetObjectiveDisplayed(10, True, True)" in reconcile
    assert "SetStage(107)" in reconcile

    on_init = _member_body(patch, "onquestinit")
    assert "player.IsInFaction(EN06_EnclavePresidentFaction)" in on_init
    assert "EN06_Pres.IsCompleted()" in on_init
    assert "SetStage(iCompletedPresidentalRace)" in on_init
    assert "player.GetValue(EN05_MQ_StageValue)" in on_init
    assert "checkpoint <= fFirstTimeThroughValue" in on_init
    assert "SetStage(iStartUpStage)" in on_init
    assert "checkpoint <= fDirectToRegisterValue" in on_init
    assert "SetStage(iPlayerHeardIntro)" in on_init
    assert "EN05MQ_ReconcileCommendations(player)" in on_init
    assert "SetStage(iPlayerRegisteredStage)" in on_init


def test_officer_controller_adds_positive_commendations_and_completes_once() -> None:
    patch = _script_patch_source(OFFICER_QUEST_SCRIPT)
    assert patch is not None

    add = _member_body(patch, "en05mq_addcommendations")
    assert "aiAmount <= 0 || IsStageDone(iCommendationsCompletedStage)" in add
    assert "iCurrentCommendations += aiAmount" in add
    assert "iCurrentCommendations > threshold" in add
    assert "iCurrentCommendations = threshold" in add
    assert "SetObjectiveDisplayed(10, True, True)" in add
    assert "!IsStageDone(iCommendationsCompletedStage)" in add
    assert add.count("SetStage(iCommendationsCompletedStage)") == 1


def test_officer_stage_fragments_persist_checkpoint_and_reconcile() -> None:
    patch = _script_patch_source(OFFICER_FRAGMENT_SCRIPT)
    assert patch is not None

    heard_intro = _member_body(patch, "fragment_stage_0007_item_00")
    assert "(Self as Quest) as EN05_MQ_QuestScript" in heard_intro
    assert (
        "player.SetValue(controller.EN05_MQ_StageValue, "
        "controller.fDirectToRegisterValue)"
    ) in heard_intro

    registered = _member_body(patch, "fragment_stage_0010_item_00")
    assert "(Self as Quest) as EN05_MQ_QuestScript" in registered
    assert (
        "player.SetValue(controller.EN05_MQ_StageValue, "
        "controller.fSystemActiveValue)"
    ) in registered
    assert "controller.EN05MQ_ReconcileCommendations(player)" in registered
