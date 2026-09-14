from __future__ import annotations

from collections import Counter
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
SET_STAGE = "Quests:_Default:SetStageOnEnterInstancedLoc"
SHOW_MESSAGE = "Quests:_Default:ShowMessageOnActivateAlias"
COLLECTION_COMBAT = "Quests:_Default:SetPreferredCombatTargets"
ALIAS_COMBAT = "Quests:_Default:SetAliasPreferredCombatTargets"
PATCHED_SCRIPTS = {
    SET_STAGE: {
        "onquestinit",
        "onquestshutdown",
        "actor.onlocationchange",
        "actor.onplayerloadgame",
        "onstageset",
        "checkplayerlocation",
        "resolvetargetlocation",
    },
    SHOW_MESSAGE: {"onaliasinit", "onactivate"},
    COLLECTION_COMBAT: {
        "onaliasinit",
        "onload",
        "onworkshopobjectrepaired",
        "reconcilepreferredcombattargets",
        "applypreferredcombattarget",
        "findpreferredcombattarget",
    },
    ALIAS_COMBAT: {
        "onaliasinit",
        "onload",
        "onworkshopobjectrepaired",
        "applypreferredcombattarget",
        "findpreferredcombattarget",
    },
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


@pytest.mark.parametrize(("script_name", "expected"), PATCHED_SCRIPTS.items())
def test_shared_helper_patch_is_member_only_unique_and_idempotent(
    script_name: str, expected: set[str]
):
    patch = _patch(script_name)
    assert not any(
        line.strip().lower().startswith(("scriptname ", "state ", "auto state "))
        for line in patch.splitlines()
    )
    assert not any(
        " property " in f" {line.strip().lower()} " for line in patch.splitlines()
    )
    assert Counter(name for name, _start, _end in _members(patch)) == Counter(
        {name: 1 for name in expected}
    )

    merged = _merged(script_name)
    merged_names = [name for name, _start, _end in _members(merged)]
    for member_name in expected:
        assert merged_names.count(member_name) == 1
        assert _member_body(merged, member_name) == _member_body(patch, member_name)
    assert _merge_script_method_patches(merged, patch) == merged


def test_instanced_location_helper_reconciles_init_load_stage_and_player_location():
    patch = _patch(SET_STAGE)
    init = _member_body(patch, "onquestinit")
    shutdown = _member_body(patch, "onquestshutdown")
    location_change = _member_body(patch, "actor.onlocationchange")
    load = _member_body(patch, "actor.onplayerloadgame")
    stage_set = _member_body(patch, "onstageset")
    check = _member_body(patch, "checkplayerlocation")
    resolve = _member_body(patch, "resolvetargetlocation")

    assert 'RegisterForRemoteEvent(playerRef, "OnLocationChange")' in init
    assert 'RegisterForRemoteEvent(playerRef, "OnPlayerLoadGame")' in init
    assert 'UnregisterForRemoteEvent(playerRef, "OnLocationChange")' in shutdown
    assert 'UnregisterForRemoteEvent(playerRef, "OnPlayerLoadGame")' in shutdown
    assert "CheckPlayerLocation(playerRef.GetCurrentLocation())" in init
    assert "akSender == Game.GetPlayer()" in location_change
    assert "CheckPlayerLocation(akNewLoc)" in location_change
    assert "CheckPlayerLocation(akSender.GetCurrentLocation())" in load
    assert "CheckPlayerLocation(playerRef.GetCurrentLocation())" in stage_set

    assert "stageData.StageToSet >= 0" in check
    assert "!IsStageDone(stageData.StageToSet)" in check
    assert "stageData.PrereqStage < 0 || IsStageDone(stageData.PrereqStage)" in check
    assert "stageData.TurnOffStage < 0 || GetStage() < stageData.TurnOffStage" in check
    assert check.index("targetLocation == playerLocation") < check.index(
        "SetStage(stageData.StageToSet)"
    )

    assert "stageData.InstancedRefAlias.GetReference()" in resolve
    assert "instancedRef.GetCurrentLocation()" in resolve
    assert resolve.index("stageData.InstancedRefAlias") < resolve.index(
        "stageData.TargetLocation != None"
    )
    assert resolve.index("stageData.TargetLocation != None") < resolve.index(
        "stageData.TargetLocationAlias != None"
    )


def test_message_helper_is_player_guarded_stage_gated_and_reentry_safe():
    patch = _patch(SHOW_MESSAGE)
    init = _member_body(patch, "onaliasinit")
    activate = _member_body(patch, "onactivate")

    assert "OwningQuest = GetOwningQuest()" in init
    assert "MessageEnabled = True" in init
    assert "If !MessageEnabled" in activate
    assert "ShowIfActivePlayer && akActionRef != playerRef" in activate
    assert "OwningQuest == None || MessageToShow == None" in activate
    assert "PrereqStage >= 0 && !OwningQuest.IsStageDone(PrereqStage)" in activate
    assert "TurnOffStage >= 0 && OwningQuest.GetStage() >= TurnOffStage" in activate
    assert activate.index("MessageEnabled = False") < activate.index(
        "MessageToShow.Show()"
    )
    assert activate.index("MessageToShow.Show()") < activate.rindex(
        "MessageEnabled = True"
    )
    assert "buttonPressed < ResponseMessagesToShow.Length" in activate
    assert "buttonPressed < ButtonStagesToSet.Length" in activate
    assert "buttonStage >= 0 && !OwningQuest.IsStageDone(buttonStage)" in activate
    assert "StageToSet >= 0 && !OwningQuest.IsStageDone(StageToSet)" in activate


@pytest.mark.parametrize("script_name", (COLLECTION_COMBAT, ALIAS_COMBAT))
def test_combat_target_helpers_use_only_live_bound_targets_and_explicit_flags(
    script_name: str,
):
    patch = _patch(script_name)
    apply = _member_body(patch, "applypreferredcombattarget")
    find = _member_body(patch, "findpreferredcombattarget")

    assert "Game.GetPlayer()" not in patch
    assert "sourceActor == None || sourceActor.IsDead()" in apply
    assert "targetActor == None" in apply
    assert "If PersistTargets" in apply
    assert "!sourceActor.HasKeyword(PersistCombatTargetsAfterCombatEndKeyword)" in apply
    assert apply.index("If StartCombat") < apply.index(
        "sourceActor.StartCombat(targetActor, True)"
    )
    assert "targetActor != sourceActor && !targetActor.IsDead()" in find
    assert "PreferredTargets != None" in find
    assert "PreferredTargetCollection != None" in find


def test_collection_and_alias_timing_survive_reentry_without_unscoped_combat():
    collection = _patch(COLLECTION_COMBAT)
    collection_init = _member_body(collection, "onaliasinit")
    collection_load = _member_body(collection, "onload")
    collection_repair = _member_body(collection, "onworkshopobjectrepaired")
    assert "If !UseOnRefAddedTiming" in collection_init
    assert "ReconcilePreferredCombatTargets()" in collection_init
    assert "ApplyPreferredCombatTarget(akSenderRef)" in collection_load
    assert "If SetTargetOnRepaired" in collection_repair

    alias = _patch(ALIAS_COMBAT)
    alias_init = _member_body(alias, "onaliasinit")
    alias_load = _member_body(alias, "onload")
    alias_repair = _member_body(alias, "onworkshopobjectrepaired")
    assert "If !UseOnLoadTiming" in alias_init
    assert "If UseOnLoadTiming" in alias_load
    assert "ApplyPreferredCombatTarget()" in alias_load
    assert "If SetTargetOnRepaired" in alias_repair


def test_broad_progress_and_enable_helpers_remain_unpatched_without_full_contracts():
    assert _script_patch_source("DefaultAliasProgressBarScript") is None
    assert _script_patch_source("Quests:_Default:AliasEnableOnLoad") is None


@pytest.mark.parametrize("script_name", PATCHED_SCRIPTS)
def test_shared_helper_full_production_merge_native_compiles_for_fo4(
    script_name: str,
):
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
