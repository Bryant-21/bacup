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
FRAGMENT_SCRIPT = "Fragments:Quests:QF_BS02_MQ03_Blue_005F5E1A"
EXIT_SCRIPT = "Quests:BS02_MQ03_Blue:PostQuestExitInstance"
BARSTOOL_SCRIPT = "Quests:BS02_MQ03_Blue:BarstoolScript"
FRAGMENT_STAGES = (
    10,
    98,
    100,
    120,
    205,
    220,
    225,
    230,
    240,
    250,
    255,
    260,
    265,
    270,
    271,
    280,
    300,
    350,
    375,
    377,
    380,
    390,
    395,
    400,
    410,
    420,
    421,
    422,
    423,
    440,
    445,
    450,
    455,
    460,
    470,
    477,
    479,
    480,
    481,
    482,
    483,
    484,
    485,
    486,
    487,
    488,
    489,
    490,
    500,
    505,
    510,
    520,
    530,
    535,
    540,
    560,
    570,
    600,
    610,
    611,
    612,
    613,
    620,
    625,
    626,
    627,
    640,
    8000,
    9000,
    9010,
    9999,
)
PATCH_MEMBERS = {
    FRAGMENT_SCRIPT: {
        f"fragment_stage_{stage:04d}_item_00" for stage in FRAGMENT_STAGES
    }
    | {"ontimer"},
    EXIT_SCRIPT: {
        "onaliasinit",
        "onlocationchange",
        "reconcilesuccessoracceptance",
    },
    BARSTOOL_SCRIPT: {"onaliasinit", "actor.onsit"},
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
def test_bs02_mq03_blue_patches_are_member_only_and_exact(
    script_name: str, members: set[str]
) -> None:
    patch = _script_patch_source(script_name)

    assert patch is not None
    if script_name == FRAGMENT_SCRIPT:
        assert len(FRAGMENT_STAGES) == 71
    member_names = _member_names(patch)
    assert len(member_names) == len(members)
    assert set(member_names) == members
    assert not any(
        line.strip().casefold().startswith(("scriptname ", "state "))
        for line in patch.splitlines()
    )
    assert not any(
        " property " in f" {line.strip().casefold()} " for line in patch.splitlines()
    )


@pytest.mark.parametrize(("script_name", "members"), PATCH_MEMBERS.items())
def test_bs02_mq03_blue_production_merges_are_unique_and_idempotent(
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
def test_bs02_mq03_blue_production_merges_compile(
    script_name: str, tmp_path: Path
) -> None:
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


def test_four_local_waves_wait_for_death_before_next_stage_and_progress() -> None:
    patch = _script_patch_source(FRAGMENT_SCRIPT)
    assert patch is not None

    wave_contract = {
        480: (481, 486),
        481: (482, 487),
        482: (483, 488),
        483: (484, 489),
    }
    for stage, (next_stage, progress_stage) in wave_contract.items():
        body = _member_body(patch, f"fragment_stage_{stage:04d}_item_00")
        assert 'Game.GetFormFromFile(0x0052C788, "SeventySix.esm")' in body
        assert "GetAlias(133) as ReferenceAlias" in body
        assert "GetAlias(134) as RefCollectionAlias" in body
        assert "Alias_Player.GetActorReference()" in body
        assert "Actor[] spawnedWave = new Actor[1]" in body
        assert body.count("spawnCenter.PlaceAtMe(waveActorBase)") == 1
        assert (
            "playerRef == None || waveActorBase == None || spawnCenter == None || "
            "waveEnemies == None" in body
        )
        assert "waveEnemies.AddRef(spawnedActor)" in body
        assert "spawnedActor.StartCombat(playerRef, True)" in body
        assert "spawnedActor == None" in body
        assert body.index("Return") < body.index(f"SetStage({next_stage})")
        assert "!waveActor.IsDead()" in body
        assert "Utility.Wait(1.0)" in body
        assert "waveEnemies.RemoveRef(spawnedWave[cleanupIndex])" in body
        assert body.index("!waveActor.IsDead()") < body.index(
            f"SetStage({next_stage})"
        )
        assert body.index("Utility.Wait(1.0)") < body.index(
            f"SetStage({next_stage})"
        )
        completion_body = _member_body(
            patch, f"fragment_stage_{next_stage:04d}_item_00"
        )
        assert f"SetStage({progress_stage})" in completion_body

    terminal = _member_body(patch, "fragment_stage_0484_item_00")
    assert "PlaceAtMe" not in terminal
    assert "SetStage(489)" in terminal
    assert "SetStage(485)" in terminal

    for stage, percentage in ((486, 25), (487, 50), (488, 75), (489, 100)):
        body = _member_body(patch, f"fragment_stage_{stage:04d}_item_00")
        assert f"progressScript.CurrPercentage = {percentage}.0" in body
        if stage != 489:
            assert "SetObjectiveDisplayed(77, True, True)" in body

    completion = _member_body(patch, "fragment_stage_0489_item_00")
    assert "SetObjectiveCompleted(77)" in completion
    assert "SetObjectiveCompleted(80)" in completion
    assert "SetObjectiveDisplayed(85)" in completion
    assert "SetStage(490)" in completion
    vine_handoff = _member_body(patch, "fragment_stage_0490_item_00")
    assert "Alias_Player.GetActorReference()" in vine_handoff
    assert "playerRef.AddKeyword(BS02_MQ03_Blue_AllowVineDestroy)" in vine_handoff


def test_lockdown_sequence_preserves_boss_scene_door_and_objectives() -> None:
    patch = _script_patch_source(FRAGMENT_SCRIPT)
    assert patch is not None

    boss_start = _member_body(patch, "fragment_stage_0520_item_00")
    assert "SetObjectiveCompleted(90)" in boss_start
    assert "SetObjectiveDisplayed(95)" in boss_start
    assert "Alias_boss_Imposterling.GetReference()" in boss_start
    assert "bossRef.Enable()" in boss_start

    boss_dead = _member_body(patch, "fragment_stage_0530_item_00")
    assert "SetObjectiveCompleted(95)" in boss_dead
    assert "SetObjectiveDisplayed(100)" in boss_dead
    assert "refcol_BossDeadEnable.GetAt(refIndex)" in boss_dead
    assert "BS02_MQ03_Tunnel_z_Comments_KilledBoss.Start()" in boss_dead

    lockdown = _member_body(patch, "fragment_stage_0540_item_00")
    assert "SetObjectiveCompleted(100)" in lockdown
    assert "SetObjectiveDisplayed(110)" in lockdown
    assert "BS02_MQ03_Tunnel_AriesTerminal.Start()" in lockdown

    door = _member_body(patch, "fragment_stage_0570_item_00")
    assert "Alias_door_WallDoor.GetReference()" in door
    assert "wallDoor.Lock(False)" in door
    assert "wallDoor.SetOpen(True)" in door
    assert "SetObjectiveCompleted(115)" in door
    assert "SetObjectiveDisplayed(120)" in door


def test_password_and_camp_clues_converge_on_all_required_evidence() -> None:
    patch = _script_patch_source(FRAGMENT_SCRIPT)
    assert patch is not None

    for stage, prerequisites in (
        (421, (422, 423)),
        (422, (421, 423)),
        (423, (421, 422)),
    ):
        body = _member_body(patch, f"fragment_stage_{stage:04d}_item_00")
        for prerequisite in prerequisites:
            assert f"IsStageDone({prerequisite})" in body
        assert "!IsStageDone(420)" in body
        assert "SetStage(420)" in body

    password_complete = _member_body(patch, "fragment_stage_0420_item_00")
    assert (
        "playerRef.SetValue(BS02_MQ03_Blue_AllowPWEntry_AV, 1.0)"
        in password_complete
    )
    assert "BS02_MQ03_Tunnel_z_Comments_AllDone.Start()" in password_complete

    for stage, prerequisites in (
        (611, (612, 613)),
        (612, (611, 613)),
        (613, (611, 612)),
    ):
        body = _member_body(patch, f"fragment_stage_{stage:04d}_item_00")
        assert "BS02_MQ03_Blue_CluesCount_NotSaved) + 1.0" in body
        assert "SetObjectiveDisplayed(135, True, True)" in body
        for prerequisite in prerequisites:
            assert f"IsStageDone({prerequisite})" in body
        assert "!IsStageDone(610)" in body
        assert "SetStage(610)" in body

    clue_complete = _member_body(patch, "fragment_stage_0610_item_00")
    assert "SetObjectiveCompleted(130)" in clue_complete
    assert "SetObjectiveCompleted(135)" in clue_complete
    assert "SetObjectiveDisplayed(140)" in clue_complete

    review = _member_body(patch, "fragment_stage_0620_item_00")
    for clue in ("CampClue1_Base", "CampClue2_Base", "CampClue3_Base"):
        assert f"playerRef.RemoveItem({clue}, 1, True)" in review
    assert "BS02_MQ03_Tunnel_Rahmani_ReviewEvidence.Start()" in review

    displays = _member_body(patch, "fragment_stage_0627_item_00")
    for alias in (
        "Alias_note_CampManifest_CannotTake",
        "Alias_note_CampRadstorms_CannotTake",
        "Alias_note_CampJournal_CannotTake",
    ):
        assert f"{alias}.GetReference()" in displays
    assert displays.count("clueDisplay.Enable()") == 3


def test_completion_uses_bound_player_and_exact_next_quest_keyword() -> None:
    patch = _script_patch_source(FRAGMENT_SCRIPT)
    assert patch is not None
    completion = _member_body(patch, "fragment_stage_9000_item_00")

    assert "Alias_Player.GetActorReference()" in completion
    assert "!BS02_MQ04_Conscience.IsRunning()" in completion
    assert "!BS02_MQ04_Conscience.IsCompleted()" in completion
    assert (
        "BS02_MQ04_Conscience_StartKeyword.SendStoryEventAndWait(None, playerRef, playerRef)"
        in completion
    )
    assert "StartTimer(5.0, 9000)" in completion
    assert completion.count("SendStoryEventAndWait(") == 1
    assert completion.index("SendStoryEventAndWait(") < completion.index(
        "StartTimer(5.0, 9000)"
    )
    assert "While " not in completion
    assert "Utility.Wait" not in completion
    assert "exitScript.ReconcileSuccessorAcceptance()" in completion
    assert "BS02_MQ04_Conscience.Start()" not in completion
    assert "SetStage(9999)" not in completion
    assert "Stop()" not in completion
    assert "BS02_MQ03_Tunnel_z_PostQuest_Raries.Start()" in completion

    timer = _member_body(patch, "ontimer")
    assert "aiTimerID != 9000" in timer
    assert "!IsStageDone(9000)" in timer
    assert "IsStageDone(9999)" in timer
    assert "!BS02_MQ04_Conscience.IsRunning()" in timer
    assert "!BS02_MQ04_Conscience.IsCompleted()" in timer
    assert (
        "BS02_MQ04_Conscience_StartKeyword.SendStoryEventAndWait(None, playerRef, playerRef)"
        in timer
    )
    assert "StartTimer(5.0, 9000)" in timer
    assert "exitScript.ReconcileSuccessorAcceptance()" in timer
    assert timer.count("SendStoryEventAndWait(") == 1
    assert timer.index("SendStoryEventAndWait(") < timer.rindex(
        "StartTimer(5.0, 9000)"
    )
    assert timer.index("exitScript.ReconcileSuccessorAcceptance()") < timer.rindex(
        "StartTimer(5.0, 9000)"
    )
    assert "While " not in timer
    assert "Utility.Wait" not in timer
    assert "BS02_MQ04_Conscience.Start()" not in timer
    assert "SetStage(9999)" not in timer
    assert "Stop()" not in timer


def test_instance_exit_helper_reconciles_bound_player_and_cleanup_stage() -> None:
    patch = _script_patch_source(EXIT_SCRIPT)
    assert patch is not None

    alias_init = _member_body(patch, "onaliasinit")
    assert "GetActorReference()" in alias_init
    assert "GetCurrentLocation() == Loc_Tunnel" in alias_init
    assert "IsStageDone(Stage_VinesReadyForDestruction)" in alias_init
    assert "playerRef.AddKeyword(BS02_MQ03_Blue_AllowVineDestroy)" in alias_init
    assert "playerRef.RemoveKeyword(BS02_MQ03_Blue_AllowVineDestroy)" in alias_init
    assert "IsStageDone(Stage_QuestCompleted)" in alias_init
    assert 'Game.GetFormFromFile(0x005F3CF5, "SeventySix.esm")' in alias_init
    assert "successorQuest.IsRunning() || successorQuest.IsCompleted()" in alias_init
    assert "SetStage(StageToSet_CleanUpQuest)" in alias_init

    location_change = _member_body(patch, "onlocationchange")
    assert "akNewLoc == Loc_Tunnel" in location_change
    assert "playerRef.AddKeyword(BS02_MQ03_Blue_AllowVineDestroy)" in location_change
    assert "playerRef.RemoveKeyword(BS02_MQ03_Blue_AllowVineDestroy)" in location_change
    assert (
        "successorQuest.IsRunning() || successorQuest.IsCompleted()"
        in location_change
    )
    assert "SetStage(StageToSet_CleanUpQuest)" in location_change

    reconcile = _member_body(patch, "reconcilesuccessoracceptance")
    assert "successorQuest == None" in reconcile
    assert "successorQuest.IsRunning() || successorQuest.IsCompleted()" in reconcile
    assert "playerRef.GetCurrentLocation() != Loc_Tunnel" in reconcile
    assert "SetStage(StageToSet_CleanUpQuest)" in reconcile


def test_final_cleanup_replaces_bound_default_player_item_cleanup() -> None:
    patch = _script_patch_source(FRAGMENT_SCRIPT)
    assert patch is not None
    cleanup = _member_body(patch, "fragment_stage_9999_item_00")

    for alias in ("PWClue1_Alias", "PWClue2_Alias", "PWClue3_Alias"):
        assert f"{alias}.GetReference()" in cleanup
    assert cleanup.count("playerRef.RemoveItem(passwordClue, 1, True)") == 3
    assert "playerRef.RemoveKeyword(BS02_MQ03_Blue_AllowVineDestroy)" in cleanup
    assert "SetStage(9010)" in cleanup
    assert "Stop()" in cleanup


def test_barstool_uses_bound_player_remote_sit_and_exact_furniture() -> None:
    patch = _script_patch_source(BARSTOOL_SCRIPT)
    assert patch is not None

    alias_init = _member_body(patch, "onaliasinit")
    assert "Alias_Player.GetActorReference()" in alias_init
    assert "GetReference() != None" in alias_init
    assert 'RegisterForRemoteEvent(playerRef, "OnSit")' in alias_init
    assert "OnActivate" not in patch

    on_sit = _member_body(patch, "actor.onsit")
    assert "Quest owningQuest = GetOwningQuest()" in on_sit
    assert "!owningQuest.IsObjectiveDisplayed(35)" in on_sit
    assert "owningQuest.IsObjectiveCompleted(35)" in on_sit
    assert "akSender != playerRef" in on_sit
    assert "akFurniture != GetReference()" in on_sit
    assert "playerRef.SetValue(BS02_MQ03_Blue_Barstool_AV, 1.0)" in on_sit
    assert on_sit.index("SetValue(") < on_sit.index("Scene_DrinkTalk.Start()")
    assert "Scene_DrinkTalk.IsPlaying()" in on_sit
