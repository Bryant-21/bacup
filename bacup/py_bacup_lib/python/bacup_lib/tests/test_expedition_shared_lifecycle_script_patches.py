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
MASTER = "Expeditions:Master"
RANDOMIZER = "Expeditions:ObjectiveRandomizer"
DIALOGUE = "Expeditions:DialogueModuleHandler"
PATCHED_SCRIPTS = {
    MASTER: {
        "onquestinit",
        "actor.onplayerloadgame",
        "onstageset",
        "onquestshutdown",
        "initializesingleplayeraliases",
        "registerobjectivemodule",
        "getobjectivemodulequest",
        "resumelocalmission",
        "startobjectivemoduleforphase",
        "forcelocalmodulelocation",
        "stopobjectivemoduleforphase",
        "stopallobjectivemodules",
        "clearlocalruntimestate",
    },
    RANDOMIZER: {
        "onquestinit",
        "applydeterministicselections",
        "selectobjectiveforphase",
        "getselectedobjectivestage",
        "getrequestedselection",
        "iscandidateenabled",
        "isstageexcluded",
    },
    DIALOGUE: {
        "onquestinit",
        "onstageset",
        "evaluateconfigureddialoguemodules",
        "startlocaldialoguemodule",
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
def test_expedition_shared_patch_is_member_only_unique_and_idempotent(
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
    assert not has_unregistered_inventory_handler(patch)
    assert not sibling_script_casts(patch)
    assert Counter(name for name, _start, _end in _members(patch)) == Counter(
        {name: 1 for name in expected}
    )

    merged = _merged(script_name)
    merged_names = [name for name, _start, _end in _members(merged)]
    for member_name in expected:
        assert merged_names.count(member_name) == 1
        assert _member_body(merged, member_name) == _member_body(patch, member_name)
    assert _merge_script_method_patches(merged, patch) == merged


def test_master_owns_single_player_aliases_resume_and_child_cleanup():
    patch = _patch(MASTER)
    init = _member_body(patch, "onquestinit")
    aliases = _member_body(patch, "initializesingleplayeraliases")
    resume = _member_body(patch, "resumelocalmission")
    register = _member_body(patch, "registerobjectivemodule")
    cleanup = _member_body(patch, "stopallobjectivemodules")

    assert 'RegisterForRemoteEvent(playerRef, "OnPlayerLoadGame")' in init
    assert "(Self as Quest) as Expeditions:ObjectiveRandomizer" in init
    assert "ApplyDeterministicSelections()" in init
    assert "Alias_ExpeditionLeader.ForceRefTo(playerRef)" in aliases
    assert "Alias_ExpeditionTeam.Find(playerRef) < 0" in aliases
    assert "Alias_ExpeditionTeam.AddRef(playerRef)" in aliases
    assert "GetObjectiveModuleQuest(aiPhase)" in register
    assert "If !akModuleQuest.IsStopped()" in register
    assert "akModuleQuest.Reset()" in register
    assert resume.index("ST_OBJ_C_PHASE_START") < resume.index("ST_OBJ_B_PHASE_START")
    assert "StopObjectiveModuleForPhase(1)" in cleanup
    assert "StopObjectiveModuleForPhase(2)" in cleanup
    assert "StopObjectiveModuleForPhase(3)" in cleanup
    assert "PilotDialogue_ModuleQI.Stop()" in cleanup
    assert "Game.GetFormFromFile" not in patch


def test_master_prefills_module_location_before_child_start():
    patch = _patch(MASTER)
    start = _member_body(patch, "startobjectivemoduleforphase")
    force = _member_body(patch, "forcelocalmodulelocation")

    assert start.index("ForceLocalModuleLocation(moduleQuest)") < start.index(
        "moduleQuest.Start()"
    )
    assert "moduleQuest.GetAlias(3) as LocationAlias" in force
    assert "playerRef.GetCurrentLocation()" in force
    assert "moduleLocation.ForceLocationTo(currentLocation)" in force


def test_randomizer_is_deterministic_enabled_and_restart_safe():
    patch = _patch(RANDOMIZER)
    select = _member_body(patch, "selectobjectiveforphase")
    selected = _member_body(patch, "getselectedobjectivestage")
    enabled = _member_body(patch, "iscandidateenabled")
    excluded = _member_body(patch, "isstageexcluded")

    assert "GetSelectedObjectiveStage(aiPhase)" in select
    assert "requestedSelection >= 1 && requestedSelection <= 3" in select
    assert select.index("IsCandidateEnabled(aiPhase, 1)") < select.index(
        "IsCandidateEnabled(aiPhase, 2)"
    )
    assert select.index("IsCandidateEnabled(aiPhase, 2)") < select.index(
        "IsCandidateEnabled(aiPhase, 3)"
    )
    assert "SetStage(selectedStage)" in select
    assert "IsStageDone(firstStage)" in selected
    assert "enableGlobal == None || enableGlobal.GetValue() > 0.0" in enabled
    assert "IsStageDone(exclusion.Conditional_ObjStage)" in excluded
    assert "RandomInt" not in patch
    assert "RandomFloat" not in patch


def test_dialogue_handler_bypasses_unavailable_qmdl_service_safely():
    patch = _patch(DIALOGUE)
    evaluate = _member_body(patch, "evaluateconfigureddialoguemodules")
    start = _member_body(patch, "startlocaldialoguemodule")

    assert "moduleData.StartOnQuestInit" in evaluate
    assert "moduleData.StageToStart == aiStage" in evaluate
    assert "moduleData.ModuleLoc == playerLocation" in evaluate
    assert "StartLocalDialogueModule(moduleData)" in evaluate
    assert "Return False" in start
    assert ".Start()" not in start
    assert "SendCustomEvent" not in patch


def test_online_ambient_encounter_service_remains_unpatched():
    assert _script_patch_source("Expeditions:AmbientEWS") is None


@pytest.mark.parametrize("script_name", PATCHED_SCRIPTS)
def test_expedition_shared_full_production_merge_native_compiles_for_fo4(
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
