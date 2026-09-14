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
DIRTY_QF = "Fragments:Quests:QF_Burn_SQ04_Visions_007F1922"
DIRTY_TRACKER = "Burn_SQ04_Collectables_Script"
DIRTY_PICKUP = "Burn_SQ04_PickupScript"
DIRTY_CACHE_ITEM = "Burn_SQ04_Item_Script"
DIRTY_CACHE_QFS = (
    "Fragments:Quests:QF_Burn_SQ04_Cache1_Misc_00848FEE",
    "Fragments:Quests:QF_Burn_SQ04_Cache2_Misc_00848FED",
    "Fragments:Quests:QF_Burn_SQ04_Cache3_Misc_00848FEF",
)
DIRTY_TIFS = (
    "Fragments:TopicInfos:TIF_Burn_SQ04_DirtyLaundry_0083F22F",
    "Fragments:TopicInfos:TIF_Burn_SQ04_DirtyLaundry_0083F244",
    "Fragments:TopicInfos:TIF_Burn_SQ04_DirtyLaundry_0083F24B",
    "Fragments:TopicInfos:TIF_Burn_SQ04_DirtyLaundry_0083F24F",
)
DIRTY_EXEC_TIF = "Fragments:TopicInfos:TIF_Burn_SQ04_DirtyLaundry_008404C5"
GRUNT_QF = "Fragments:Quests:QF_Burn_BountyHunt_Daily_007D6A80"
GRUNT_ACTIVATOR = "Burn:Burn_Bounty:Burn_Bounty_GruntBountyActivator"
GRUNT_MASTER = "Burn:Burn_Bounty:Burn_Bounty_BountyGiverScript"
GRUNT_QUEST = "Burn:Burn_Bounty:Burn_BountyHunt_QuestScript"
GRUNT_SPAWN = "Burn:Burn_Bounty:Burn_Bounty_GruntSpawnScript"
GRUNT_DYING = "Burn:Burn_Bounty:Burn_Bounty_HandleOnDying"
PATCHED_SCRIPTS = (
    DIRTY_QF,
    DIRTY_TRACKER,
    DIRTY_PICKUP,
    DIRTY_CACHE_ITEM,
    *DIRTY_CACHE_QFS,
    *DIRTY_TIFS,
    DIRTY_EXEC_TIF,
    GRUNT_QF,
    GRUNT_ACTIVATOR,
    GRUNT_MASTER,
    GRUNT_QUEST,
    GRUNT_SPAWN,
    GRUNT_DYING,
)

DIRTY_FRAGMENT_STAGES = {
    100, 101, 102, 104, 105, 106, 107, 110, 150, 200, 300, 400, 500,
    600, 700, 800, 900, 1000, 1100, 1200, 1300, 1400, 1500, 1600,
    1650, 1700, 1800, 1900, 2000, 2100, 2200, 2300, 2400, 2450, 2500,
    2600, 2700, 2800, 2900, 3000, 3050, 7000, 7100, 7200, 7300, 7400,
    7450, 8500, 9000,
}
GRUNT_FRAGMENT_STAGES = {0, 50, 100, 200, 250, 300, 9000, 9990, 9999}


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
        if name == member_name.lower()
    )
    return "\n".join(lines[start : end + 1])


def _merged_production_source(script_name: str) -> str:
    source_path = SOURCE_ROOT / _script_relative_path(script_name, ".psc")
    patch = _script_patch_source(script_name)
    assert source_path.is_file(), source_path
    assert patch is not None
    return _merge_script_method_patches(
        source_path.read_text(encoding="utf-8"), patch
    )


@pytest.mark.parametrize("script_name", PATCHED_SCRIPTS)
def test_burn_side_quest_patch_is_member_only(script_name: str):
    patch = _script_patch_source(script_name)
    assert patch is not None
    assert not any(
        line.strip().lower().startswith(("scriptname ", "state ", "endstate"))
        for line in patch.splitlines()
    )
    assert not any(
        " property " in f" {line.strip().lower()} " for line in patch.splitlines()
    )


@pytest.mark.parametrize("script_name", PATCHED_SCRIPTS)
def test_burn_side_quest_production_merge_is_unique_and_idempotent(
    script_name: str,
):
    patch = _script_patch_source(script_name)
    assert patch is not None
    merged = _merged_production_source(script_name)
    merged_names = _member_names(merged)
    for member_name in _member_names(patch):
        assert merged_names.count(member_name) == 1
        assert _member_body(merged, member_name) == _member_body(patch, member_name)
    assert _merge_script_method_patches(merged, patch) == merged


def _fragment_stages(script_name: str) -> set[int]:
    return {
        int(name[len("fragment_stage_") : len("fragment_stage_") + 4])
        for name in _member_names(_script_patch_source(script_name))
        if name.startswith("fragment_stage_")
    }


def test_dirty_laundry_implements_every_live_bound_qf_fragment():
    assert _fragment_stages(DIRTY_QF) == DIRTY_FRAGMENT_STAGES


@pytest.mark.parametrize("script_name", DIRTY_CACHE_QFS)
def test_dirty_laundry_cache_children_implement_start_complete_shutdown(script_name: str):
    assert _fragment_stages(script_name) == {100, 200, 9000}
    patch = _script_patch_source(script_name)
    assert "SetObjectiveDisplayed(10, True)" in patch
    assert "SetStage(9000)" in patch
    assert "Stop()" in patch


def test_dirty_laundry_batch_hand_in_is_gated_and_idempotent():
    patch = _script_patch_source(DIRTY_TRACKER)
    hand_in = _member_body(patch, "handinreadybatch")
    assert "IsStageDone(BatchRewardStages[batchIndex])" in hand_in
    assert hand_in.index("heldIntel < neededIntel") < hand_in.index(
        "ModValue(Burn_SQ04_CollectablesNotHandedIn"
    )
    assert hand_in.index("GrantBatchReward(batchIndex)") < hand_in.index(
        "SetStage(BatchRewardStages[batchIndex])"
    )
    assert "CompleteQuest" not in patch


@pytest.mark.parametrize("script_name", DIRTY_TIFS)
def test_dirty_laundry_bound_bodhi_tifs_hand_in_once(script_name: str):
    patch = _script_patch_source(script_name)
    body = _member_body(patch, "fragment_end")
    assert body.count("tracker.HandInReadyBatch()") == 1


def test_dirty_laundry_cache_uses_live_child_location_not_cache_number():
    patch = _script_patch_source(DIRTY_CACHE_ITEM)
    selector = _member_body(patch, "activecachequestatplayer")
    activation = _member_body(patch, "onactivate")
    assert "cacheQuest.GetAlias(0) as LocationAlias" in selector
    assert "GetLocation() == playerLocation" in selector
    assert "CacheNumber" not in patch
    assert "playerRef.AddItem(CacheRewards, 1, False)" in activation
    assert "activeCache.SetStage(200)" in activation


def test_grunt_hunt_starts_only_through_location_scoped_story_manager():
    activator = _script_patch_source(GRUNT_ACTIVATOR)
    master = _script_patch_source(GRUNT_MASTER)
    assert ".Start()" not in activator
    assert ".Start()" not in master
    assert "bountyMaster.StartLocalGruntHunt(playerRef)" in activator
    assert (
        "GruntBountyStartKeyword.SendStoryEventAndWait"
        "(chosenLocation, akPlayer, akPlayer)"
    ) in master
    assert "candidate.IsGruntHuntLoc" in master


def test_grunt_hunt_preserves_repeat_failure_and_reward_ownership():
    patch = _script_patch_source(GRUNT_QF)
    assert _fragment_stages(GRUNT_QF) == GRUNT_FRAGMENT_STAGES
    assert "AddItem(" not in patch
    assert "SetStage(9990)" in _member_body(
        patch, "fragment_stage_0200_item_00"
    )
    assert "RecordLocalGruntCompletion" in _member_body(
        patch, "fragment_stage_9000_item_00"
    )
    shutdown = _member_body(patch, "fragment_stage_9999_item_00")
    assert shutdown.index("CleanupLocalGruntHunt") < shutdown.index("Stop()")


def test_grunt_target_handler_never_invents_wave_actors():
    patch = _script_patch_source(GRUNT_SPAWN)
    prepare = _member_body(patch, "preparelocaltargets")
    assert "Act_BountyTarget.GetCount() == 0" in prepare
    assert "PlaceAtMe" not in patch
    assert "ActorBase" not in patch


def test_grunt_on_dying_advances_stage_without_direct_completion():
    patch = _script_patch_source(GRUNT_DYING)
    body = _member_body(patch, "ondying")
    grunt_branch = body[body.index("If IsGruntHunt") : body.index("Actor playerRef")]
    assert "bountyQuest.SetStage(300)" in grunt_branch
    assert "CompleteQuest" not in grunt_branch
    assert "AddItem" not in grunt_branch


@pytest.mark.parametrize("script_name", PATCHED_SCRIPTS)
def test_burn_side_quest_full_production_merge_native_compiles_for_fo4(
    script_name: str, tmp_path: Path,
):
    base_source = _fo4_base_source()
    assert base_source is not None, "FO4 base Papyrus sources unavailable"
    for patched_script in PATCHED_SCRIPTS:
        relative_path = _script_relative_path(patched_script, ".psc")
        merged_path = tmp_path / relative_path
        merged_path.parent.mkdir(parents=True, exist_ok=True)
        merged_path.write_text(
            _merged_production_source(patched_script), encoding="utf-8"
        )
    result = compile_psc(
        _merged_production_source(script_name),
        imports=[str(tmp_path), str(SOURCE_ROOT), str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=str(_script_relative_path(script_name, ".psc")),
    )
    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None
