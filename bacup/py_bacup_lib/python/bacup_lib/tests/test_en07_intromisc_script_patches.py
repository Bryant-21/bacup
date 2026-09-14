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

QUEST_SCRIPT = "EN07_IntroMiscScript"
FRAGMENT_SCRIPT = "Fragments:Quests:QF_EN07_MQ_IntroMisc_002D0F6B"
PATCHED_SCRIPTS = (QUEST_SCRIPT, FRAGMENT_SCRIPT)

# The 18 stage fragments declared by EN07_MQ_Death (002D0F6B) VMAD.
# Anything outside this set is pruned silently by the game.
DECLARED_STAGES = (
    10, 15, 20, 25, 27, 30, 39, 40, 49, 50, 59, 60, 65, 67, 69, 70, 100, 105,
)


def _merged(script_name: str) -> str:
    skeleton = (SOURCE_ROOT / _script_relative_path(script_name, ".psc")).read_text(
        encoding="utf-8"
    )
    patch = _script_patch_source(script_name)
    assert patch is not None, f"no patch registered for {script_name}"
    assert "Scriptname" not in patch
    assert "Property" not in patch
    merged = _merge_script_method_patches(skeleton, patch)
    assert _merge_script_method_patches(merged, patch) == merged
    return merged


@pytest.mark.parametrize("script_name", PATCHED_SCRIPTS)
def test_en07_intromisc_members_merge_once(script_name: str) -> None:
    members = [
        name
        for _kind, name, _start, _end in _iter_top_level_papyrus_members(
            _merged(script_name).splitlines()
        )
    ]
    for member in members:
        assert members.count(member) == 1, f"{script_name}: duplicate member {member}"


def test_en07_intromisc_fragments_match_declared_stages() -> None:
    merged = _merged(FRAGMENT_SCRIPT)
    members = {
        name
        for _kind, name, _start, _end in _iter_top_level_papyrus_members(
            merged.splitlines()
        )
    }
    expected = {
        f"Fragment_Stage_{stage:04d}_Item_00".lower() for stage in DECLARED_STAGES
    }
    fragments = {
        name.lower() for name in members if name.lower().startswith("fragment_stage_")
    }
    assert fragments == expected


def test_en07_intromisc_quest_owns_stage_orchestration() -> None:
    merged = _merged(QUEST_SCRIPT)
    # Stage 100 is the CompleteQuest stage; it must be reachable from the
    # locally-resolved nuke launch, not from a stripped server event.
    assert "SendRMIToServer" not in merged
    assert "player.GetValue(EN07_Death_LaunchedNuke) >= 1.0" in merged
    assert "SetStage(iCompletionStage)" in merged
    # Stage 80 gate: launch card + code piece + cipher intel.
    assert "SetStage(iUnlockNukeLaunchQTs)" in merged
    assert "HandleStage(iUnlockNukeLaunchQTs)" in merged
    assert "nukeMaster.Start()" in merged
    assert "HasLocalAnyCodePiece(player)" in merged
    assert "HasLocalAnyCompleteCodeSet(player)" not in merged.split("Function EvaluateLocalProgress", 1)[1]
    assert "SetValue(EN07_Death_CK_TutorialState, iTutorialsCompleted as Float)" not in merged
    assert "SetValue(EN07_Death_CK_TutorialState, 2.0)" in merged
    # Tutorial credit is local, driven by the quest's own station aliases.
    assert "RegisterForRemoteEvent(stationRef, \"OnActivate\")" in merged
    assert "SetStage(iTutorialsDoneStage)" in merged


def test_en07_intromisc_starts_non_event_nuke_masters_before_polling() -> None:
    merged = _merged(QUEST_SCRIPT)
    start = merged.index("Event OnQuestInit()")
    end = merged.index("EndEvent", start) + len("EndEvent")
    init = merged[start:end]

    init_lines = [line.strip() for line in init.splitlines()]
    assert init_lines.count("EN07_MQ_Nuke_Master.Start()") == 1
    assert init_lines.count("Nuke_Master.Start()") == 1
    assert init.index("EN07_MQ_Nuke_Master.Start()") < init.index(
        "RecountLocalTutorials(player)"
    )
    assert init.index("Nuke_Master.Start()") < init.index(
        "RecountLocalTutorials(player)"
    )
    assert "003DA647" not in init
    assert "nukeCodes.Start()" not in merged
    assert "Nuke_Codes.Start()" not in merged


def test_en07_intromisc_fragments_delegate_to_quest_script() -> None:
    merged = _merged(FRAGMENT_SCRIPT)
    assert "SendRMIToServer" not in merged
    assert "introScript.HandleStage(aiStage)" in merged
    for stage in DECLARED_STAGES:
        assert f"HandleStage({stage})" in merged
    # Stage 15 starts the MODUS intro scene; stage 100 hands off to BoS01.
    assert "EN07_MQ_IntroMisc_Scene.Start()" in merged
    assert "BoS01_QuestStartKeyword.SendStoryEvent" in merged


@pytest.mark.parametrize("script_name", PATCHED_SCRIPTS)
def test_en07_intromisc_patch_compiles(script_name: str) -> None:
    base_source = _fo4_base_source()
    if base_source is None:
        pytest.skip("FO4 base Papyrus sources unavailable")

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
