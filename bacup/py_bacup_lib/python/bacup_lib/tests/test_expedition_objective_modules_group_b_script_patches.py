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
REPEL = "Expeditions:ObjectiveModule_RepelEnemies"
DEFEND = "Expeditions:ObjectiveModule_DefendNPC"
CARRY = "Expeditions:ObjectiveModule_CarryAndThrow"
DESTRUCTION = "Expeditions:ObjectiveModule_ObjectDestruction"
SIGNAL = "Expeditions:ObjectiveModule_SignalTracker"
REPEL_REF = "Expeditions:ObjectiveModule_RepelEnemies_refr"
PATCHED_SCRIPTS = {REPEL, DEFEND, CARRY, DESTRUCTION, SIGNAL}
TEAM_MODULES = {REPEL, DEFEND, CARRY, SIGNAL}
MODULE_LOCATION_MODULES = {REPEL, DEFEND, CARRY, DESTRUCTION}


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
def test_group_b_patches_are_member_only_unique_and_idempotent(script_name: str):
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
def test_group_b_modules_resume_locally_and_complete_bound_stage(script_name: str):
    patch = _patch(script_name)
    if script_name in MODULE_LOCATION_MODULES:
        init = _member_body(patch, "onquestinit")
        location = _member_body(patch, "ensurelocalmodulelocation")
        assert init.index("EnsureLocalModuleLocation()") < init.index("Game.GetPlayer()")
        assert "GetAlias(3) as LocationAlias" in location
        assert "playerRef.GetCurrentLocation()" in location
        assert "moduleLocation.ForceLocationTo(currentLocation)" in location
    assert 'RegisterForRemoteEvent(playerRef, "OnPlayerLoadGame")' in patch
    assert "SetStage(CompleteStage)" in patch
    assert "SendCustomEvent" not in patch
    assert "RandomInt" not in patch
    assert "RandomFloat" not in patch
    if script_name in TEAM_MODULES:
        assert "AddRef(playerRef)" in patch


def test_repel_requires_beacon_and_advances_only_clear_area_time():
    patch = _patch(REPEL)
    activate = _member_body(patch, "objectreference.onactivate")
    timer = _member_body(patch, "ontimer")
    thresholds = _member_body(patch, "applylocalrepelthresholds")
    complete = _member_body(patch, "completelocalsecurearea")
    assert "akSender == SecureAreaActivator_Alias.GetReference()" in activate
    assert "EnemiesInSecureArea_RefCollAlias.GetCount() == 0" in timer
    assert "ProgressTime += PROGRESS_INTERVAL" in timer
    assert "oldProgressPercent < thresholdData.ProgressThreshold_Percent" in thresholds
    assert thresholds.count("StartLocalEncounterWave") == 1
    assert complete.index("SetObjectiveCompleted") < complete.index(
        "SetStage(CompleteStage)"
    )


def test_defend_pauses_for_health_distance_and_resumes_bound_activities():
    patch = _patch(DEFEND)
    timer = _member_body(patch, "ontimer")
    revive = _member_body(patch, "revivelocaldefendnpc")
    activity = _member_body(patch, "completecurrentlocaldefendactivity")
    assert "NPC_Actor.GetValue(Health) <= 0.0" in timer
    assert "IsLocalPlayerTooFarFromNPC()" in timer
    assert "StartTimer(PROGRESS_INTERVAL, PROGRESS_TIMER)" in timer
    assert "NPC_Actor.Resurrect()" in revive
    assert "NPC_Actor.RestoreValue(Health, healthToRestore)" in revive
    assert "SetStage(CurrentActivity.StageToSetOnActivityComplete)" in activity


def test_carry_counts_only_unique_bound_thrown_objects():
    patch = _patch(CARRY)
    thrown = _member_body(patch, "handlelocalthrownobject")
    target = _member_body(patch, "completelocalcarrytarget")
    shutdown = _member_body(patch, "onquestshutdown")
    assert "thrownRef.GetBaseObject() != CarryObject_Weapon" in thrown
    assert "SuccessfulThrownObjects.Find(thrownRef) >= 0" in thrown
    assert thrown.index("SuccessfulThrownObjects.Add(thrownRef)") < thrown.index(
        "targetData.SuccessfulHits += 1"
    )
    assert "IsStageDone(targetData.StageToSetOnCompleted)" in target
    assert "playerRef.RemoveItem(CarryObject_Weapon, -1, True)" in shutdown


def test_object_destruction_recomputes_counts_and_completes_each_set_once():
    patch = _patch(DESTRUCTION)
    reconcile = _member_body(patch, "reconcilelocaldestructionsets")
    count = _member_body(patch, "countdestroyedlocalobjects")
    assert "DestructionSetsComplete = 0" in reconcile
    assert "CountDestroyedLocalObjects(setData)" in reconcile
    assert "destructibleRef.IsDestroyed() || destructibleRef.IsDisabled()" in count
    assert reconcile.index("SetObjectiveCompleted") < reconcile.index(
        "DestructionSetsComplete += 1"
    )
    assert "DestructionSetsComplete >= setLimit" in reconcile


def test_signal_tracker_uses_each_bound_local_completion_criterion():
    patch = _patch(SIGNAL)
    register = _member_body(patch, "registerlocalsignaltargetevents")
    complete = _member_body(patch, "completelocalsignaltarget")
    stage = _member_body(patch, "onstageset")
    assert 'RegisterForRemoteEvent(targetRef, "OnActivate")' in register
    assert 'RegisterForRemoteEvent(targetRef, "OnDestructionStageChanged")' in register
    assert 'RegisterForRemoteEvent(targetRef as Actor, "OnDeath")' in register
    assert "TrackedDownCriteriaStage == auiStageID" in stage
    assert "pendingIndex = ActiveSignalTargetSets.Find(targetRef)" in complete
    assert complete.index("SetStage(setData.TrackedDownDoneStage)") < complete.index(
        "ActiveSignalTargetSets.Remove(pendingIndex)"
    )


def test_existing_repel_reference_helper_remains_local_and_unchanged_in_scope():
    patch = _patch(REPEL_REF)
    assert "UpdateProgress(Float afCurrentProgressPercent)" in patch
    assert "Self.PlayAnimation(CurrentAnim)" in patch
    assert "; @drop-member OnSyncVariableNetworkChanged" in patch


@pytest.mark.parametrize("script_name", sorted(PATCHED_SCRIPTS))
def test_group_b_full_production_merge_native_compiles_for_fo4(script_name: str):
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
