from __future__ import annotations

from collections import Counter
from pathlib import Path

import pytest

from bacup_lib.tests.test_script_patch_conventions import (
    has_unregistered_inventory_handler,
    sibling_script_casts,
)
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
ASSASSINATION = "Expeditions:ObjectiveModule_Assassination"
FREE_PRISONERS = "Expeditions:ObjectiveModule_FreePrisoners"
RACE = "Expeditions:ObjectiveModule_Race"
PROXIMITY = "Expeditions:ObjectiveModule_ProximityTracker"
GATHER = "Expeditions:ObjectiveModule_GatherAndDeposit"
SOLVE_LOCKS = "Expeditions:ObjectiveModule_SolveLocks"
PATCHED_SCRIPTS = {
    ASSASSINATION,
    FREE_PRISONERS,
    RACE,
    PROXIMITY,
    GATHER,
    SOLVE_LOCKS,
}


def _members(source: str) -> list[tuple[str, int, int]]:
    return [
        (name, start, end)
        for kind, name, start, end in _iter_top_level_papyrus_members(
            source.splitlines()
        )
        if kind in {"function", "event"}
    ]


def _member_body(source: str, member_name: str) -> str:
    lines = source.splitlines()
    start, end = next(
        (start, end)
        for name, start, end in _members(source)
        if name == member_name.lower()
    )
    return "\n".join(lines[start : end + 1])


def _patch(script_name: str) -> str:
    patch = _script_patch_source(script_name)
    assert patch is not None
    return patch


def _merged(script_name: str) -> str:
    source_path = SOURCE_ROOT / _script_relative_path(script_name, ".psc")
    assert source_path.is_file(), source_path
    return _merge_script_method_patches(
        source_path.read_text(encoding="utf-8"), _patch(script_name)
    )


@pytest.mark.parametrize("script_name", sorted(PATCHED_SCRIPTS))
def test_group_a_patches_are_member_only_unique_and_idempotent(script_name: str):
    patch = _patch(script_name)
    assert not any(
        line.strip().lower().startswith(("scriptname ", "state ", "auto state "))
        for line in patch.splitlines()
    )
    assert not any(
        " property " in f" {line.strip().lower()} " for line in patch.splitlines()
    )
    assert not has_unregistered_inventory_handler(patch)
    assert not sibling_script_casts(patch)
    member_names = [name for name, _start, _end in _members(patch)]
    assert Counter(member_names) == Counter(set(member_names))

    merged = _merged(script_name)
    merged_names = [name for name, _start, _end in _members(merged)]
    for member_name in member_names:
        assert merged_names.count(member_name) == 1
        assert _member_body(merged, member_name) == _member_body(patch, member_name)
    assert _merge_script_method_patches(merged, patch) == merged


@pytest.mark.parametrize("script_name", sorted(PATCHED_SCRIPTS))
def test_group_a_modules_use_local_player_team_and_complete_stage(script_name: str):
    patch = _patch(script_name)
    init = _member_body(patch, "onquestinit")
    location = _member_body(patch, "ensurelocalmodulelocation")
    assert init.index("EnsureLocalModuleLocation()") < init.index("Game.GetPlayer()")
    assert "GetAlias(3) as LocationAlias" in location
    assert "playerRef.GetCurrentLocation()" in location
    assert "moduleLocation.ForceLocationTo(currentLocation)" in location
    assert "Game.GetPlayer()" in init
    assert "AddRef(playerRef)" in init
    assert 'RegisterForRemoteEvent(playerRef, "OnPlayerLoadGame")' in init
    assert "SetStage(CompleteStage)" in patch
    assert "SendCustomEvent" not in patch
    assert "RandomInt" not in patch
    assert "RandomFloat" not in patch


def test_assassination_uses_deterministic_set_and_idempotent_leader_death():
    patch = _patch(ASSASSINATION)
    prepare = _member_body(patch, "preparelocalassassination")
    death = _member_body(patch, "actor.ondeath")
    complete = _member_body(patch, "completelocalassassination")
    assert "CurrentAssassinationSet = 0" in prepare
    assert "Territories_RefCollAlias.GetAt(0)" in prepare
    assert "StartLocalEncounterWave(setData.WaveIndex)" in prepare
    assert "akSender == leaderRef" in death
    assert "If IsStageDone(CompleteStage)" in complete
    assert complete.index("SetObjectiveCompleted") < complete.index(
        "SetStage(CompleteStage)"
    )


def test_free_prisoners_recounts_unlocked_doors_before_completion():
    patch = _patch(FREE_PRISONERS)
    reconcile = _member_body(patch, "reconcilelocalprisondoors")
    assert "DoorsUnlocked = 0" in reconcile
    assert "If !prisonDoor.IsLocked()" in reconcile
    assert "DoorsUnlocked += 1" in reconcile
    assert "DoorsUnlocked >= DoorsToUnlockTotal" in reconcile
    assert reconcile.index("SetObjectiveCompleted(600, True)") < reconcile.index(
        "SetStage(CompleteStage)"
    )


def test_race_requires_ordered_checkpoints_and_completes_once():
    patch = _patch(RACE)
    checkpoint = _member_body(patch, "handlelocalracecheckpoint")
    timer = _member_body(patch, "ontimer")
    resume = _member_body(patch, "actor.onplayerloadgame")
    assert "CPArray[NextCP] != checkpointRef" in checkpoint
    assert "NextCP += 1" in checkpoint
    assert "RaceCompleted = True" in checkpoint
    assert "CancelTimer(1)" in checkpoint
    assert "NextCP = 0" in timer
    assert "If CPArray == None || CPArray.Length == 0" in resume
    assert "PrepareLocalRace()" in resume


def test_proximity_tracker_removes_each_target_before_advancing_set():
    patch = _patch(PROXIMITY)
    target = _member_body(patch, "handlelocalsignaltarget")
    complete = _member_body(patch, "completecurrentlocalsignalset")
    assert "pendingIndex = AllPendingTargets.Find(targetRef)" in target
    assert "If pendingIndex < 0" in target
    assert "AllPendingTargets.Remove(pendingIndex)" in target
    assert target.index("AllPendingTargets.Remove") < target.index(
        "CompleteCurrentLocalSignalSet()"
    )
    assert "TargetSetsTrackedDown += 1" in complete


def test_gather_deposit_marks_set_before_inventory_transfer():
    patch = _patch(GATHER)
    deposit = _member_body(patch, "depositlocalitems")
    assert "If !setData.ItemsDeposited" in deposit
    assert deposit.index("setData.ItemsDeposited = True") < deposit.index(
        "playerRef.RemoveItem"
    )
    assert "ItemDepositsCompleted >= CountLocalItemSets()" in deposit
    assert deposit.index("SetObjectiveCompleted(300, True)") < deposit.index(
        "SetStage(CompleteStage)"
    )


def test_solve_locks_supports_container_and_terminal_completion():
    patch = _patch(SOLVE_LOCKS)
    prepare = _member_body(patch, "preparelocallockset")
    terminal = _member_body(patch, "terminal.onmenuitemrun")
    complete = _member_body(patch, "completelocallockset")
    assert 'RegisterForRemoteEvent(lockedRef, "OnLockStateChanged")' in prepare
    assert 'RegisterForRemoteEvent(lockedTerminal, "OnMenuItemRun")' in prepare
    assert "setData.LockedTerminal_MenuItem == auiMenuItemID" in terminal
    assert "IsStageDone(setData.StageToSetOnThisSetCompleted)" in complete
    assert complete.index("SetObjectiveCompleted") < complete.index(
        "ReconcileLocalLocks()"
    )


@pytest.mark.parametrize("script_name", sorted(PATCHED_SCRIPTS))
def test_group_a_full_production_merge_native_compiles_for_fo4(script_name: str):
    base_source = _fo4_base_source()
    assert base_source is not None, "FO4 base Papyrus sources unavailable"

    result = compile_psc(
        _merged(script_name),
        imports=[str(SOURCE_ROOT), str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=str(_script_relative_path(script_name, ".psc")),
    )
    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None
