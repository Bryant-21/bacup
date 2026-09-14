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

FRAGMENT_SCRIPT = "Fragments:Quests:QF_MTNM01_Mayhem_0009732E"
QUEST_SCRIPT = "MTNM01QuestScript"
PLAYER_SCRIPT = "mtnm01_playerscript"
CHEM_SCRIPT = "MTNM01_ChemDartEffectScript"
CANNIBAL_SCRIPT = "mtnm01_cannibalperkentryscript"
BAIT_SCRIPT = "MTNM01_BaitMineScript"
DEATHCLAW_SCRIPT = "mtnm01_deathclawfriendperkscript"

FRAGMENTS = {
    (1, 0),
    (50, 0),
    (100, 0),
    (125, 0),
    (150, 0),
    (200, 0),
    (300, 0),
    (310, 0),
    (311, 0),
    (312, 0),
    (313, 0),
    (314, 0),
    (315, 0),
    (350, 0),
    (351, 0),
    (352, 0),
    (400, 0),
    (420, 0),
    (421, 0),
    (422, 0),
    (423, 0),
    (424, 0),
    (425, 0),
    (500, 0),
    (600, 0),
    (650, 0),
    (650, 1),
    (660, 0),
    (661, 0),
    (700, 0),
    (800, 0),
    (800, 1),
    (900, 0),
    (901, 0),
    (960, 0),
    (1000, 0),
    (1100, 0),
}

PATCH_MEMBERS = {
    FRAGMENT_SCRIPT: {
        *(f"fragment_stage_{stage:04d}_item_{item:02d}" for stage, item in FRAGMENTS),
        "mtnm01_getcontroller",
        "mtnm01_getplayer",
        "mtnm01_setcheckpoint",
        "mtnm01_sayrosetopic",
        "mtnm01_beginkarmakill",
        "mtnm01_playerhasbaitmaterials",
        "mtnm01_movealiasitemtocontainer",
        "mtnm01_seedbaitmaterials",
        "mtnm01_checkbaitmaterials",
        "mtnm01_clearbaitalias",
    },
    QUEST_SCRIPT: {
        "mtnm01_getplayer",
        "onquestinit",
        "onstageset",
        "ontimer",
        "onquestshutdown",
    },
    PLAYER_SCRIPT: {
        "mtnm01_getowningquest",
        "mtnm01_reconcilekarmastage",
        "startsupermutantstage",
        "onaliasinit",
        "onplayerloadgame",
        "onitemequipped",
    },
    CHEM_SCRIPT: {"oneffectstart"},
    CANNIBAL_SCRIPT: {"onentryrun"},
    BAIT_SCRIPT: {"onload", "onunload", "ontimer"},
    DEATHCLAW_SCRIPT: {"onentryrun"},
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
def test_mtnm01_patches_are_member_only_and_exact(
    script_name: str, members: set[str]
) -> None:
    patch = _script_patch_source(script_name)

    assert patch is not None
    assert set(_member_names(patch)) == members
    assert not any(
        line.strip().casefold().startswith(("scriptname ", "state ", "extends "))
        for line in patch.splitlines()
    )
    assert not any(
        " property " in f" {line.strip().casefold()} " for line in patch.splitlines()
    )


@pytest.mark.parametrize(("script_name", "members"), PATCH_MEMBERS.items())
def test_mtnm01_merges_are_unique_and_idempotent(
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
def test_mtnm01_full_production_merges_compile(
    script_name: str, tmp_path: Path
) -> None:
    base_source = _fo4_base_source()
    assert base_source is not None, "FO4 base Papyrus sources unavailable"

    for dependency_name in PATCH_MEMBERS:
        dependency_path = tmp_path / _script_relative_path(dependency_name, ".psc")
        dependency_path.parent.mkdir(parents=True, exist_ok=True)
        dependency_path.write_text(_merged_source(dependency_name), encoding="utf-8")

    result = compile_psc(
        _merged_source(script_name),
        imports=[str(tmp_path), str(SOURCE_ROOT), str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=str(_script_relative_path(script_name, ".psc")),
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None


def test_mtnm01_karma_branches_converge_on_bound_kill_stage() -> None:
    fragment = _script_patch_source(FRAGMENT_SCRIPT)
    chem = _script_patch_source(CHEM_SCRIPT)
    assert fragment is not None
    assert chem is not None

    expected = {
        310: (0, "ChemdartRobot"),
        311: (1, "ChemdartEasy"),
        312: (2, "ChemdartOther"),
        313: (3, "ChemdartYaoGuai"),
        314: (4, "ChemdartDifficult"),
        315: (5, "ChemdartPlayer"),
    }
    for stage, (reward_value, topic_suffix) in expected.items():
        body = _member_body(fragment, f"fragment_stage_{stage:04d}_item_00")
        assert f"MTNM01_BeginKarmaKill({reward_value}," in body
        assert topic_suffix in body

    begin = _member_body(fragment, "mtnm01_beginkarmakill")
    assert "controller.iChemDartRewardValue = aiRewardValue" in begin
    assert "!IsStageDone(350)" in begin
    assert "SetStage(350)" in begin

    effect = _member_body(chem, "oneffectstart")
    for stage_property in (
        "KarmaRobotStage",
        "KarmaEasyStage",
        "KarmaOtherStage",
        "KarmaYaoGuaiStage",
        "KarmaDifficultStage",
        "KarmaPlayerStage",
    ):
        assert f"controller.SetStage(controller.{stage_property})" in effect
    assert "controller.KarmaCreature.ForceRefTo(akTarget)" in effect


def test_mtnm01_karma_syringer_waits_for_craft_completion() -> None:
    player = _script_patch_source(PLAYER_SCRIPT)
    assert player is not None

    reconcile = _member_body(player, "mtnm01_reconcilekarmastage")
    assert "owner.IsStageDone(250)" in reconcile
    assert "owner.IsStageDone(ModKarmaStage)" not in reconcile
    assert "!owner || !owner.IsRunning()" in reconcile
    assert "owner.IsStageDone(UseKarmaStage)" in reconcile
    assert reconcile.count("owner.SetStage(UseKarmaStage)") == 1

    alias_init = _member_body(player, "onaliasinit")
    player_load = _member_body(player, "onplayerloadgame")
    equipped = _member_body(player, "onitemequipped")
    assert "MTNM01_ReconcileKarmaStage()" in alias_init
    assert player_load == "Event OnPlayerLoadGame()\n    MTNM01_ReconcileKarmaStage()\nEndEvent"
    assert "MTNM01_ReconcileKarmaStage(akBaseObject)" in equipped


def test_mtnm01_bait_and_deathclaw_transactions_advance_once() -> None:
    fragment = _script_patch_source(FRAGMENT_SCRIPT)
    bait = _script_patch_source(BAIT_SCRIPT)
    deathclaw = _script_patch_source(DEATHCLAW_SCRIPT)
    assert fragment is not None
    assert bait is not None
    assert deathclaw is not None

    for stage, alias_name in {
        421: "Alias_Mine",
        422: "Alias_RadstagMeatBait",
        423: "Alias_Copper",
        424: "Alias_Adhesive01",
        425: "Alias_Adhesive02",
    }.items():
        body = _member_body(fragment, f"fragment_stage_{stage:04d}_item_00")
        assert f"MTNM01_ClearBaitAlias({alias_name})" in body

    bait_timer = _member_body(bait, "ontimer")
    assert "!MTNM01_Mayhem.IsStageDone(600)" in bait_timer
    assert bait_timer.count("MTNM01_Mayhem.SetStage(600)") == 1

    bait_load = _member_body(bait, "onload")
    assert "!MTNM01_Mayhem || !MTNM01_Mayhem.IsRunning()" in bait_load
    assert "myHazard.Delete()" in bait_load
    assert "myHazard = None" in bait_load
    assert bait_load.count("PlaceAtMe(MTNM01_BaitMineScentAttractorMeatHazard)") == 1
    assert bait_load.count("StartTimer(ExplosionCheckTimerSeconds, 1)") == 1
    assert bait_load.index("Return") < bait_load.index("PlaceAtMe(")

    bait_unload = _member_body(bait, "onunload")
    assert bait_unload.index("CancelTimer(1)") < bait_unload.index("myHazard.Delete()")
    assert "myHazard = None" in bait_unload
    assert "SetStage(" not in bait_unload
    assert "StartTimer(" not in bait_unload
    assert "Self.Delete()" not in bait_unload
    assert ".Clear(" not in bait_unload

    stopped_timer = bait_timer[: bait_timer.index("ObjectReference[] nearby")]
    assert "!MTNM01_Mayhem.IsRunning()" in stopped_timer
    assert "myHazard.Delete()" in stopped_timer
    assert "myHazard = None" in stopped_timer
    assert "SetStage(" not in stopped_timer

    triggered_timer = bait_timer[bait_timer.index("If triggered") :]
    assert "PlaceAtMe(MTNM01_Expl_BaitMine)" in triggered_timer
    assert "myHazard.Delete()" in triggered_timer
    assert "myHazard = None" in triggered_timer
    assert "!MTNM01_Mayhem.IsStageDone(600)" in triggered_timer
    assert triggered_timer.count("MTNM01_Mayhem.SetStage(600)") == 1

    friend = _member_body(deathclaw, "onentryrun")
    assert "controller.Deathclaw.ForceRefTo(targetActor)" in friend
    assert friend.count("controller.SetStage(controller.KillDeathClawStage)") == 1


def test_mtnm01_cannibal_branch_and_walkaway_are_repeat_safe() -> None:
    fragment = _script_patch_source(FRAGMENT_SCRIPT)
    cannibal = _script_patch_source(CANNIBAL_SCRIPT)
    quest = _script_patch_source(QUEST_SCRIPT)
    assert fragment is not None
    assert cannibal is not None
    assert quest is not None

    entry = _member_body(cannibal, "onentryrun")
    assert "controller.IsStageDone(controller.CannibalStage)" in entry
    assert "controller.IsStageDone(controller.CannibalSuccessStage)" in entry
    assert "controller.GhoulCorpse.ForceRefTo(targetActor)" in entry
    assert entry.count("controller.SetStage(controller.CannibalSuccessStage)") == 1

    success = _member_body(fragment, "fragment_stage_0901_item_00")
    assert "SetObjectiveCompleted(901)" in success
    assert "!IsStageDone(903)" in success
    assert "SetStage(903)" in success

    timer = _member_body(quest, "ontimer")
    assert "aiTimerID == CannibalWalkAwayStage" in timer
    assert "IsStageDone(CannibalSuccessStage)" in timer
    assert "SetStage(CannibalWalkAwayStage)" in timer


def test_mtnm01_completion_uses_bound_reward_stage_and_story_handoff() -> None:
    fragment = _script_patch_source(FRAGMENT_SCRIPT)
    assert fragment is not None

    completion = _member_body(fragment, "fragment_stage_1000_item_00")
    assert "CompleteAllObjectives()" in completion
    assert "MTNM01_SetCheckpoint(10.0)" in completion
    assert (
        "MTNL01_Raiders_Quest_Keyword.SendStoryEventAndWait(None, playerRef, playerRef)"
        in completion
    )
    assert completion.index("CompleteAllObjectives()") < completion.index(
        "SendStoryEventAndWait("
    )
    assert "AddItem(" not in completion
