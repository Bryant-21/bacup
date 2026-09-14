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
FRAGMENT_SCRIPT = "Fragments:Quests:QF_MTR06_PhysicalExam_0000D783"
RACE_SCRIPT = "MTR06_RaceQuestScript"
TERMINAL_SCRIPT = "PhysicalExamTerminalRefScript"
MAIN_SCRIPT = "MTR06_QuestScript"
SCRIPT_NAMES = (FRAGMENT_SCRIPT, RACE_SCRIPT, TERMINAL_SCRIPT)
STAGE_MEMBERS = {
    f"fragment_stage_{stage:04d}_item_00"
    for stage in (10, 20, 30, 50, 100, 110, 120, 150)
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


def _button_transition(stage: int, button: str) -> int | None:
    transitions = {(20, "start"): 30, (30, "checkpoint"): 50, (50, "start"): 100}
    return transitions.get((stage, button))


def _race_result(elapsed: float, minimum: int, trial_length: float) -> int:
    if elapsed >= minimum and elapsed <= trial_length:
        return 110
    return 120


def test_physical_exam_patches_merge_once_idempotently_and_compile(tmp_path: Path) -> None:
    merged_sources = {script_name: _merged(script_name) for script_name in SCRIPT_NAMES}
    merged_sources[MAIN_SCRIPT] = _merged(MAIN_SCRIPT)

    for script_name in SCRIPT_NAMES:
        patch = _patch(script_name)
        merged = merged_sources[script_name]
        merged_members = _member_names(merged)
        for member in _member_names(patch):
            assert merged_members.count(member) == 1
            assert _member_body(merged, member) == _member_body(patch, member)
        assert _merge_script_method_patches(merged, patch) == merged

    for script_name, source in merged_sources.items():
        path = tmp_path / _script_relative_path(script_name, ".psc")
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(source, encoding="utf-8")

    base_source = _fo4_base_source()
    assert base_source is not None, "FO4 base Papyrus sources unavailable"
    for script_name in SCRIPT_NAMES:
        result = compile_psc(
            merged_sources[script_name],
            imports=[str(tmp_path), str(SOURCE_ROOT), str(base_source)],
            game="fo4",
            flags=str(base_source / "Institute_Papyrus_Flags.flg"),
            source_path=str(_script_relative_path(script_name, ".psc")),
        )
        diagnostics = "\n".join(str(item) for item in result.diagnostics)
        assert result.ok, f"{script_name}: {diagnostics}"
        assert result.pex_bytes is not None


def test_physical_exam_fragment_covers_exact_stage_contract() -> None:
    patch = _patch(FRAGMENT_SCRIPT)
    member_names = _member_names(patch)

    assert set(member_names) == STAGE_MEMBERS | {"mtr06_getcontroller"}
    assert len(member_names) == len(STAGE_MEMBERS) + 1

    stage_20 = _member_body(patch, "fragment_stage_0020_item_00")
    assert "SetObjectiveCompleted(10)" in stage_20
    assert "SetObjectiveDisplayed(15)" in stage_20
    assert "controller.MTR06_BeginRace()" in stage_20

    stage_100 = _member_body(patch, "fragment_stage_0100_item_00")
    assert stage_100.count("controller.MTR06_WrapUpRace()") == 1

    success = _member_body(patch, "fragment_stage_0110_item_00")
    assert "controller.MTR06_CompletePhysicalExam()" in success
    assert "controller.MTR06_ShowSuccess()" in success
    assert "SetStage(150)" in success

    failure = _member_body(patch, "fragment_stage_0120_item_00")
    assert "SetObjectiveFailed(15)" in failure
    assert "controller.MTR06_ShowFailure()" in failure
    assert "SetStage(150)" in failure

    cleanup = _member_body(patch, "fragment_stage_0150_item_00")
    assert "controller.MTR06_EndAttempt()" in cleanup


def test_race_controller_uses_bound_timer_and_elapsed_time_thresholds() -> None:
    patch = _patch(RACE_SCRIPT)
    init = _member_body(patch, "onquestinit")
    timer = _member_body(patch, "ontimer")
    begin = _member_body(patch, "mtr06_beginrace")
    wrapup = _member_body(patch, "mtr06_wrapuprace")
    complete = _member_body(patch, "mtr06_completephysicalexam")

    assert "StartTimer(iRaceCountdownTimerLength as Float, iRaceCountdownTimerID)" in init
    assert "SetStage(iRaceStartStage)" in timer
    assert "fRaceStartTime = Utility.GetCurrentRealTime()" in begin
    assert "Utility.GetCurrentRealTime() - fRaceStartTime" in wrapup
    assert "fFinalRaceTime >= MinNumSeconds" in wrapup
    assert "fFinalRaceTime <= trialLength" in wrapup
    assert "SetStage(110)" in wrapup
    assert "SetStage(120)" in wrapup
    assert "playerRef.SetValue(MTR06_PhysExamCompleted, 1.0)" in complete
    assert "mainController.MTR06_HandlePhysicalExamComplete()" in complete


def test_terminal_prefills_required_alias_before_correct_story_event_order() -> None:
    handler = _member_body(_patch(TERMINAL_SCRIPT), "onmenuitemrun")

    assert 'Game.GetFormFromFile(0x0000D783, "SeventySix.esm") as Quest' in handler
    assert 'Game.GetFormFromFile(0x0006EFC6, "SeventySix.esm") as Keyword' in handler
    assert 'Game.GetFormFromFile(0x002E5A6F, "SeventySix.esm") as Location' in handler
    assert "physicalExam.Reset()" in handler
    assert "physicalExam.GetAlias(1) as ReferenceAlias" in handler
    assert "activatingTerminal.ForceRefTo(akTerminalRef)" in handler
    send = "startKeyword.SendStoryEventAndWait(examLocation, akTerminalRef, playerRef)"
    assert send in handler
    assert handler.index("activatingTerminal.ForceRefTo(akTerminalRef)") < handler.index(send)
    assert ".Start()" not in handler


@pytest.mark.parametrize(
    ("stage", "button", "expected"),
    [
        (20, "start", 30),
        (30, "checkpoint", 50),
        (50, "start", 100),
        (20, "checkpoint", None),
        (30, "start", None),
        (100, "start", None),
    ],
)
def test_physical_exam_ordered_button_transition_matrix(
    stage: int, button: str, expected: int | None
) -> None:
    assert _button_transition(stage, button) == expected


@pytest.mark.parametrize(
    ("elapsed", "expected"),
    [(0.0, 120), (8.99, 120), (9.0, 110), (100.0, 110), (100.01, 120)],
)
def test_physical_exam_elapsed_time_transition_matrix(
    elapsed: float, expected: int
) -> None:
    assert _race_result(elapsed, minimum=9, trial_length=100.0) == expected
