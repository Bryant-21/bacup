from __future__ import annotations

import os
import re
from pathlib import Path

import pytest

from bacup_lib.workflows.unified import (
    _merge_script_method_patches,
    _script_patch_source,
    _script_relative_path,
)
from creation_lib.pex.native_runtime import compile_psc


REPO_ROOT = Path(__file__).resolve().parents[5]
SOURCE_ROOT = REPO_ROOT / "mods" / "SeventySix" / "Scripts" / "Source" / "User"

SQ01 = "Fragments:Quests:QF_BURN_SQ01_007DF76B"
SQ01_ON_CONNECT = "Fragments:Quests:QF_BURN_SQ01_OnConnect_007F79A9"
SQ01_LEVEL = "Fragments:Quests:QF_BURN_SQ01_LevelIncrease_007F79AA"
SQ01_RADIO = "Fragments:Quests:QF_BURN_SQ01_Radio_007F79AB"
MQ03 = "Fragments:Quests:QF_Burn_MQ03_RustKingMid_00838038"
SQ01_REENTRY = "Burn_SQ01_MoveToRustKingdom"

PATCH_CASES = (
    SQ01,
    SQ01_ON_CONNECT,
    SQ01_LEVEL,
    SQ01_RADIO,
    MQ03,
    SQ01_REENTRY,
)

SQ01_FRAGMENTS = {
    "Fragment_Stage_0001_Item_00",
    "Fragment_Stage_0002_Item_00",
    "Fragment_Stage_0003_Item_00",
    "Fragment_Stage_0004_Item_00",
    "Fragment_Stage_0005_Item_00",
    "Fragment_Stage_0006_Item_00",
    "Fragment_Stage_0007_Item_00",
    "Fragment_Stage_0008_Item_00",
    "Fragment_Stage_0009_Item_00",
    "Fragment_Stage_0012_Item_00",
    "Fragment_Stage_0100_Item_00",
    "Fragment_Stage_0200_Item_00",
    "Fragment_Stage_0200_Item_01",
    "Fragment_Stage_0201_Item_00",
    "Fragment_Stage_0400_Item_00",
    "Fragment_Stage_0402_Item_00",
    "Fragment_Stage_0405_Item_00",
    "Fragment_Stage_0500_Item_00",
    "Fragment_Stage_0535_Item_00",
    "Fragment_Stage_0540_Item_00",
    "Fragment_Stage_0550_Item_00",
    "Fragment_Stage_0570_Item_00",
    "Fragment_Stage_0580_Item_00",
    "Fragment_Stage_0601_Item_00",
    "Fragment_Stage_0602_Item_00",
    "Fragment_Stage_0700_Item_00",
    "Fragment_Stage_0701_Item_00",
    "Fragment_Stage_0800_Item_00",
    "Fragment_Stage_0900_Item_00",
    "Fragment_Stage_0902_Item_00",
    "Fragment_Stage_0910_Item_00",
    "Fragment_Stage_1000_Item_00",
    "Fragment_Stage_1100_Item_00",
    "Fragment_Stage_1110_Item_00",
    "Fragment_Stage_1120_Item_00",
    "Fragment_Stage_1200_Item_00",
    "Fragment_Stage_1280_Item_00",
    "Fragment_Stage_1290_Item_00",
    "Fragment_Stage_1300_Item_00",
    "Fragment_Stage_1400_Item_00",
    "Fragment_Stage_1500_Item_00",
    "Fragment_Stage_9000_Item_00",
    "Fragment_Stage_9990_Item_00",
    "Fragment_Stage_9999_Item_00",
}

MQ03_FRAGMENTS = {
    "Fragment_Stage_0010_Item_00",
    "Fragment_Stage_0100_Item_00",
    "Fragment_Stage_0125_Item_00",
    "Fragment_Stage_0150_Item_00",
    "Fragment_Stage_0200_Item_00",
    "Fragment_Stage_0300_Item_00",
    "Fragment_Stage_0400_Item_00",
    "Fragment_Stage_0500_Item_00",
    "Fragment_Stage_0600_Item_00",
    "Fragment_Stage_0700_Item_00",
    "Fragment_Stage_9000_Item_00",
    "Fragment_Stage_9999_Item_00",
}


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


def _patch(script_name: str) -> str:
    patch = _script_patch_source(script_name)
    assert patch is not None
    return patch


def _merged_source(script_name: str) -> str:
    source_path = SOURCE_ROOT / _script_relative_path(script_name, ".psc")
    assert source_path.is_file(), source_path
    return _merge_script_method_patches(
        source_path.read_text(encoding="utf-8"), _patch(script_name)
    )


def _member_body(source: str, header: str, end_keyword: str = "EndFunction") -> str:
    start = source.find(header)
    assert start != -1, f"{header!r} not found"
    end = source.find(end_keyword, start)
    assert end != -1, f"{end_keyword!r} not found after {header!r}"
    return source[start:end]


def _fragment_names(script_name: str) -> set[str]:
    return set(
        re.findall(
            r"(?im)^Function\s+(Fragment_[A-Za-z0-9_]+)\s*\(",
            _patch(script_name),
        )
    )


@pytest.mark.parametrize("script_name", PATCH_CASES)
def test_patch_merges_into_production_skeleton_idempotently(script_name: str):
    source_path = SOURCE_ROOT / _script_relative_path(script_name, ".psc")
    skeleton = source_path.read_text(encoding="utf-8")
    patch = _patch(script_name)
    assert not any(
        line.strip().lower().startswith("scriptname ") for line in patch.splitlines()
    )

    merged = _merge_script_method_patches(skeleton, patch)
    assert merged.lower().count("scriptname ") == 1
    assert _merge_script_method_patches(merged, patch) == merged


def test_every_bound_sq01_and_mq03_fragment_has_a_member():
    assert _fragment_names(SQ01) == SQ01_FRAGMENTS
    assert _fragment_names(MQ03) == MQ03_FRAGMENTS
    assert _fragment_names(SQ01_ON_CONNECT) == {"Fragment_Stage_0100_Item_00"}
    assert _fragment_names(SQ01_LEVEL) == {"Fragment_Stage_0100_Item_00"}
    assert _fragment_names(SQ01_RADIO) == {
        "Fragment_Stage_0200_Item_00",
        "Fragment_Stage_9999_Item_00",
    }


def test_sq01_startup_radio_and_story_manager_routes_are_event_scoped():
    source = _merged_source(SQ01)
    startup = _member_body(source, "Function Fragment_Stage_0100_Item_00()")
    assert "BURN_SQ01_Radio_QuestStartKeyword.SendStoryEventAndWait(" in startup
    assert "SetObjectiveDisplayed(10)" in startup

    for child in (SQ01_ON_CONNECT, SQ01_LEVEL):
        child_source = _merged_source(child)
        assert "BURN_SQ01_QuestStartKeyword.SendStoryEventAndWait(" in child_source
        assert "BURN_SQ01.Start(" not in child_source

    radio = _merged_source(SQ01_RADIO)
    listened = _member_body(radio, "Function Fragment_Stage_0200_Item_00()")
    assert "BURN_SQ01.SetStage(200)" in listened
    assert "BURN_SQ01.IsStageDone(200)" in listened
    assert "SetStage(9999)" in listened


def test_sq01_contract_restores_objectives_convergence_and_reentry():
    source = _merged_source(SQ01)
    assert "SetObjectiveDisplayed(10)" in source
    assert "SetObjectiveCompleted(120)" in source

    eugene = _member_body(source, "Function Fragment_Stage_0601_Item_00()")
    silas = _member_body(source, "Function Fragment_Stage_0602_Item_00()")
    assert "IsStageDone(602)" in eugene and "SetStage(700)" in eugene
    assert "IsStageDone(601)" in silas and "SetStage(700)" in silas

    stage_701 = _member_body(source, "Function Fragment_Stage_0701_Item_00()")
    assert "UnlockAliasDoor(Alias_RKI_Silas_JailDoor)" in stage_701
    assert "UnlockAliasDoor(Alias_RKI_Eugene_Jaildoor)" in stage_701
    assert "UnlockAliasDoor(Alias_RKI_Player_Jaildoor)" in stage_701
    assert "StartBoundScene(BURN_EugeneSilasWalkToArena)" in stage_701

    assert "SetStage(570)" in _member_body(
        source, "Function Fragment_Stage_0007_Item_00()"
    )
    assert "SetStage(800)" in _member_body(
        source, "Function Fragment_Stage_0008_Item_00()"
    )
    assert "SetStage(580)" in _member_body(
        source, "Function Fragment_Stage_0009_Item_00()"
    )
    assert "SetStage(1200)" in _member_body(
        source, "Function Fragment_Stage_0012_Item_00()"
    )


def test_sq01_wave_only_services_have_explicit_single_player_continuations():
    source = _merged_source(SQ01)
    checkpoint = _member_body(source, "Function Fragment_Stage_0402_Item_00()")
    arena = _member_body(source, "Function Fragment_Stage_1100_Item_00()")
    deathclaw = _member_body(source, "Function Fragment_Stage_1110_Item_00()")
    resolution = _member_body(source, "Function Fragment_Stage_1120_Item_00()")
    assert "SetStage(405)" in checkpoint
    assert "SetStage(1110)" in arena
    assert "SetStage(1120)" in deathclaw
    assert "silas.Kill()" in resolution
    assert "SetStage(1200)" in resolution


def test_sq01_knockout_teleport_completion_and_reward_ownership():
    patch = _patch(SQ01)
    source = _merged_source(SQ01)
    assert "BURNFadeToBlack.Cast(playerRef, playerRef)" in source
    assert "playerRef.MoveTo(destination)" in source
    assert "playerRef.MoveTo(RustKingdomExtTeleport)" in source
    completion = _member_body(source, "Function Fragment_Stage_9000_Item_00()")
    assert "BURN_MQ03_MidQuest_QuestStartKeyword.SendStoryEventAndWait(" in completion
    assert completion.index("SendStoryEventAndWait") < completion.index(
        "SetStage(9999)"
    )
    assert "QuestReward" not in patch
    assert "RewardCaps" not in patch


def test_sq01_reentry_activator_is_player_stage_and_binding_guarded():
    source = _merged_source(SQ01_REENTRY)
    activate = _member_body(source, "Event OnActivate(", "EndEvent")
    assert "akActionRef != Game.GetPlayer()" in activate
    assert "QuestReq.IsRunning()" in activate
    assert "currentStage >= iStageMin && currentStage <= iStageMax" in activate
    assert "akActionRef.MoveTo(ObjRefToTPTo)" in activate
    assert "InaccessibleMessage.Show()" in activate
    assert "JoinLeaderMessage" not in _patch(SQ01_REENTRY)
    assert "JoinOwnMessage" not in _patch(SQ01_REENTRY)


def test_mq03_gates_the_challenge_phases_instead_of_bypassing_them():
    """The two Rust King tests must be earned, not skipped.

    An earlier pass had stage 300 set 400 and stage 500 set 600 in the same
    frame, because ChallengeSetOne/Two are null. That is a false success path:
    the player never performs the objective the stage stands for. Both stages
    now hand off to Burn_MQ03_MidQuestChallengeGrants, which advances only when
    the bounty-hunt count reaches the shipped StageCompletionTargets globals.
    """
    patch = _patch(MQ03)
    source = _merged_source(MQ03)
    stage_300 = _member_body(source, "Function Fragment_Stage_0300_Item_00()")
    stage_500 = _member_body(source, "Function Fragment_Stage_0500_Item_00()")
    for body in (stage_300, stage_500):
        assert "controller.EvaluateChallengeProgress()" in body
    assert "SetStage(400)" not in stage_300
    assert "SetStage(600)" not in stage_500
    # The null CHAL properties must stay unread on both sides of the hand-off.
    assert "Chal_TestOfMight_Gater" not in patch
    assert "Chal_TestOfDominance_Gater" not in patch
    grants = [
        line
        for line in _patch("Burn_MQ03_MidQuestChallengeGrants").splitlines()
        if not line.lstrip().startswith(";")
    ]
    assert not any("ChallengeSetOne" in line for line in grants)
    assert not any("ChallengeSetTwo" in line for line in grants)

    restore = _member_body(source, "Function RestoreHighwayTownActors()")
    assert "IsStageDone(700)" in restore
    assert "SetPlayerValue(BURN_MQ03_RuntHWT_AV, 0.0)" in restore
    assert "SetPlayerValue(BURN_MQ03_RuntCorpseHWT_AV, 1.0)" in restore
    assert "SetPlayerValue(BURN_SQ02_EugeneHWT_AV, 1.0)" in restore
    assert "runt.Disable()" in restore
    assert "runtCorpse.Enable()" in restore
    assert "eugene.EvaluatePackage()" in restore


def test_mq03_replaces_missing_location_helper_with_quest_local_events():
    source = _merged_source(MQ03)
    location = _member_body(source, "Function AdvanceForPlayerLocation(")
    assert "GetAlias(10) as LocationAlias" in location
    assert "GetAlias(6) as LocationAlias" in location
    assert "IsStageDone(100) && !IsStageDone(125)" in location
    assert "IsStageDone(150) && !IsStageDone(200)" in location
    assert "IsStageDone(600) && !IsStageDone(700)" in location
    assert location.index("SetStage(700)") < location.index("SetStage(200)")

    init = _member_body(source, "Event OnQuestInit()", "EndEvent")
    load = _member_body(source, "Event Actor.OnPlayerLoadGame(", "EndEvent")
    shutdown = _member_body(source, "Event OnQuestShutdown()", "EndEvent")
    assert "RegisterForPlayerLocation()" in init
    assert "AdvanceForPlayerLocation(player.GetCurrentLocation())" in init
    assert "RegisterForPlayerLocation()" in load
    assert "AdvanceForPlayerLocation(akSender.GetCurrentLocation())" in load
    assert 'UnregisterForRemoteEvent(player, "OnLocationChange")' in shutdown
    assert 'UnregisterForRemoteEvent(player, "OnPlayerLoadGame")' in shutdown


def test_mq03_completion_preserves_story_manager_and_reward_owner():
    patch = _patch(MQ03)
    source = _merged_source(MQ03)
    completion = _member_body(source, "Function Fragment_Stage_9000_Item_00()")
    assert "CompleteAllObjectives()" in completion
    assert "BURN_SQ02_Outro_QuestStartKeyword.SendStoryEventAndWait(" in completion
    assert completion.index("SendStoryEventAndWait") < completion.index(
        "SetStage(9999)"
    )
    assert "QuestReward" not in patch
    assert "RewardCaps" not in patch
    assert "BURN_SQ02_Outro.Start(" not in patch


@pytest.mark.parametrize(
    "script_name",
    (
        "BurnSQ01PlayerControlHelperScript",
        "Burn_MQ03_MidQuestHackSuccess",
        "Burn_MQ03_MidQuestLockpickSuccess",
        "Fragments:Packages:PF_BURN_SQ01_Eugene_LeaveAre_00851492",
        "Fragments:TopicInfos:TIF_BURN_SQ01_008308F8",
        "Fragments:Quests:QF_BURN_SQ01_OnConnect_01000F55",
        "Fragments:Quests:QF_BURN_SQ01_LevelIncrease_01000F56",
        "Fragments:Quests:QF_BURN_SQ01_Radio_01000C06",
    ),
)
def test_online_redundant_or_orphan_shells_remain_unpatched(script_name: str):
    assert _script_patch_source(script_name) is None


@pytest.mark.parametrize("script_name", PATCH_CASES)
def test_merged_patch_native_compiles_for_fo4(script_name: str):
    base_source = _fo4_base_source()
    if base_source is None:
        pytest.skip("FO4 base Papyrus sources unavailable")

    result = compile_psc(
        _merged_source(script_name),
        imports=[str(base_source), str(SOURCE_ROOT)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=str(_script_relative_path(script_name, ".psc")),
    )
    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None
