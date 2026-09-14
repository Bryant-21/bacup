from __future__ import annotations

from collections import Counter
from pathlib import Path

from bacup_lib.tests.test_terminal_fragment_script_patches import _fo4_base_source
from bacup_lib.workflows.unified import (
    _augment_fo76_to_fo4_script_skeleton,
    _iter_top_level_papyrus_members,
    _merge_script_method_patches,
    _script_patch_source,
    _script_relative_path,
)
from creation_lib.pex.native_runtime import compile_psc


REPO_ROOT = Path(__file__).resolve().parents[5]
SOURCE_ROOT = REPO_ROOT / "mods" / "SeventySix" / "Scripts" / "Source" / "User"
EXPECTED_MEMBERS = {
    "DefaultSetStageOnInstanceLoadQuest": {
        "onquestinit",
        "onquestshutdown",
        "actor.onlocationchange",
        "actor.onplayerloadgame",
        "onstageset",
        "checkplayerinstancelocation",
    },
    "DefaultQuestEnterInstancedLocScript": {
        "onquestinit",
        "onquestshutdown",
        "actor.onlocationchange",
        "actor.onplayerloadgame",
        "onstageset",
        "checkplayerlocation",
    },
    "DefaultCollectionAliasRemoveOnDeath": {
        "ondeath",
        "ondying",
        "removedeadreference",
    },
    "DefaultAliasOnActivateGiveItem": {"onactivate"},
    "defaultquestencounterwavescript": {
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
    patch = _patch(script_name)
    skeleton = _augment_fo76_to_fo4_script_skeleton(
        script_name, source_path.read_text(encoding="utf-8")
    )
    return _merge_script_method_patches(skeleton, patch)


def test_shared_runtime_patches_merge_every_member_once_and_are_idempotent():
    for script_name, expected in EXPECTED_MEMBERS.items():
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


def test_no_live_bound_shared_runtime_member_is_hollow():
    for script_name, expected in EXPECTED_MEMBERS.items():
        patch = _patch(script_name)
        for member_name in expected:
            body = _member_body(patch, member_name).splitlines()
            executable = [
                line.strip()
                for line in body[1:-1]
                if line.strip() and not line.lstrip().startswith(";")
            ]
            assert executable, f"{script_name}.{member_name} is hollow"


def test_instance_load_helpers_recheck_on_load_and_honor_bound_stage_gates():
    instance_patch = _patch("DefaultSetStageOnInstanceLoadQuest")
    init = _member_body(instance_patch, "onquestinit")
    shutdown = _member_body(instance_patch, "onquestshutdown")
    load = _member_body(instance_patch, "actor.onplayerloadgame")
    check = _member_body(instance_patch, "checkplayerinstancelocation")

    assert 'RegisterForRemoteEvent(playerRef, "OnLocationChange")' in init
    assert 'RegisterForRemoteEvent(playerRef, "OnPlayerLoadGame")' in init
    assert 'UnregisterForRemoteEvent(playerRef, "OnLocationChange")' in shutdown
    assert 'UnregisterForRemoteEvent(playerRef, "OnPlayerLoadGame")' in shutdown
    assert "CheckPlayerInstanceLocation(akSender.GetCurrentLocation())" in load
    assert "locationData.TargetLocation == playerLocation" in check
    assert "locationData.StageToSet >= 0" in check
    assert (
        "locationData.PreReqStage < 0 || IsStageDone(locationData.PreReqStage)" in check
    )
    assert (
        "locationData.ShutdownStage < 0 || GetStage() < locationData.ShutdownStage"
        in check
    )
    assert check.index("locationData.TargetLocation == playerLocation") < check.index(
        "SetStage(locationData.StageToSet)"
    )

    enter_patch = _patch("DefaultQuestEnterInstancedLocScript")
    assert 'RegisterForRemoteEvent(playerRef, "OnPlayerLoadGame")' in _member_body(
        enter_patch, "onquestinit"
    )
    assert 'UnregisterForRemoteEvent(playerRef, "OnPlayerLoadGame")' in _member_body(
        enter_patch, "onquestshutdown"
    )
    assert "stageData.StageToSet >= 0" in _member_body(
        enter_patch, "checkplayerlocation"
    )


def test_collection_lifecycle_notifies_wave_then_removes_and_sets_empty_stage():
    patch = _patch("DefaultCollectionAliasRemoveOnDeath")
    assert "If !UseOnDyingInstead" in _member_body(patch, "ondeath")
    assert "If UseOnDyingInstead" in _member_body(patch, "ondying")

    remove = _member_body(patch, "removedeadreference")
    notify = "encounterQuest.HandleLocalEncounterActorDeath(akSenderRef)"
    assert remove.index(notify) < remove.index("RemoveRef(akSenderRef)")
    assert remove.index("RemoveRef(akSenderRef)") < remove.index(
        "owningQuest.SetStage(StageToSetOnEmpty)"
    )
    assert "GetCount() <= 0" in remove
    assert "!owningQuest.IsStageDone(StageToSetOnEmpty)" in remove


def test_give_item_alias_preserves_player_av_and_parent_stage_filters():
    activate = _member_body(_patch("DefaultAliasOnActivateGiveItem"), "onactivate")
    assert "akActionRef != playerRef" in activate
    assert "playerRef.GetValue(RequiredActorValue)" in activate
    assert "RequiredActorValueValue" in activate
    assert activate.index("bBusy = True") < activate.index("playerRef.AddItem(")
    assert activate.index("playerRef.AddItem(") < activate.index("TryToSetStage(")
    for bound_filter in (
        "PlayerActivateOnly",
        "ActivatedByReferences",
        "ActivatedByAliases",
        "ActivatedByFactions",
    ):
        assert bound_filter in activate


def test_local_encounter_wave_starts_bound_actors_and_emits_only_bound_stages():
    patch = _patch("defaultquestencounterwavescript")
    start = _member_body(patch, "startlocalencounterwave")
    register = _member_body(patch, "registerlocalwavecollection")
    evaluate = _member_body(patch, "evaluatelocalencounterwave")
    death = _member_body(patch, "handlelocalencounteractordeath")

    assert start.count("RegisterLocalWaveCollection(") == 3
    assert start.index("RegisterLocalWaveCollection(") < start.index(
        "SetLocalEncounterStageIfPending(waveData.StageToSetOnFirstWaveSpawn)"
    )
    assert "waveActor.Enable()" in register
    assert "waveActor.StartCombat(playerRef)" in register
    assert register.index("waveActor.Enable()") < register.index(
        "waveActor.StartCombat(playerRef)"
    )
    assert 'RegisterForRemoteEvent(waveActor, "OnDeath")' in register
    assert "waveData.StageToSetOnXAttackersRemaining" in evaluate
    assert "waveData.XAttackersRemaining" in evaluate
    assert "abAllowEmpty || LocalWaveHasAnyReference(waveData)" in evaluate
    assert "SetLocalEncounterStageIfPending(waveData.StageToSetAtEnd)" in evaluate
    assert "LocalWaveContainsReference(waveData, akSenderRef)" in death
    assert "EvaluateLocalEncounterWave(waveIndex, False, akSenderRef)" in death
    count = _member_body(patch, "countlivingcollectionactors")
    assert "waveActor != akExcludedRef" in count

    set_stage = _member_body(patch, "setlocalencounterstageifpending")
    assert "aiStage >= 0 && !IsStageDone(aiStage)" in set_stage
    assert set_stage.count("SetStage(aiStage)") == 1


def test_wave_runtime_restores_registrations_and_cleans_up_on_shutdown():
    patch = _patch("defaultquestencounterwavescript")
    init = _member_body(patch, "onquestinit")
    load = _member_body(patch, "actor.onplayerloadgame")
    shutdown = _member_body(patch, "onquestshutdown")

    assert "ReconcileLocalEncounterWaveRegistrations()" in init
    assert "ReconcileLocalEncounterWaveRegistrations()" in load
    assert "EvaluateLocalEncounterWave(waveIndex, False)" in load
    assert "EvaluateLocalEncounterWave(waveIndex, False)" not in init
    assert "QuestShuttingDown = True" in shutdown
    assert shutdown.count("UnregisterLocalWaveCollection(") == 3
    assert 'UnregisterForRemoteEvent(playerRef, "OnPlayerLoadGame")' in shutdown


def test_bs01_death_path_is_explicitly_scoped_away_from_generic_quests():
    patch = _patch("defaultquestencounterwavescript")
    start = _member_body(patch, "startbs01fieldtestinglocalencounter")
    death = _member_body(patch, "actor.ondeath")
    complete = _member_body(patch, "completebs01fieldtestinglocalencounterifalldead")

    assert "TotalNumLegendaryCreaturesToSpawn = -1" in start
    assert "If TotalNumLegendaryCreaturesToSpawn < 0" in death
    assert death.index("CompleteBS01FieldTestingLocalEncounterIfAllDead") < death.index(
        "Else"
    )
    assert death.index("Else") < death.index("HandleLocalEncounterActorDeath")
    assert "TotalNumLegendaryCreaturesToSpawn = 0" in complete


def test_shared_runtime_full_production_merges_compile_for_fo4(tmp_path: Path):
    base_source = _fo4_base_source()
    assert base_source is not None, "FO4 base Papyrus sources unavailable"

    encounter_script = "defaultquestencounterwavescript"
    encounter_source = _merged(encounter_script)
    encounter_result = compile_psc(
        encounter_source,
        imports=[str(base_source), str(SOURCE_ROOT)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=f"{encounter_script}.psc",
    )
    encounter_diagnostics = "\n".join(
        str(item) for item in encounter_result.diagnostics
    )
    assert encounter_result.ok, encounter_diagnostics
    assert encounter_result.pex_bytes is not None
    (tmp_path / f"{encounter_script}.psc").write_text(
        encounter_source, encoding="utf-8"
    )

    for script_name in (
        "DefaultSetStageOnInstanceLoadQuest",
        "DefaultQuestEnterInstancedLocScript",
        "DefaultCollectionAliasRemoveOnDeath",
        "DefaultAliasOnActivateGiveItem",
    ):
        result = compile_psc(
            _merged(script_name),
            imports=[str(tmp_path), str(base_source), str(SOURCE_ROOT)],
            game="fo4",
            flags=str(base_source / "Institute_Papyrus_Flags.flg"),
            source_path=str(_script_relative_path(script_name, ".psc")),
        )
        diagnostics = "\n".join(str(item) for item in result.diagnostics)
        assert result.ok, diagnostics
        assert result.pex_bytes is not None
