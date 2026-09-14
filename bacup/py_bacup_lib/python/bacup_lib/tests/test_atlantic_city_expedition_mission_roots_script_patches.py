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
TAX_CONTROLLER = "XPD_AC02_Mission_QuestScript"
SENSATION_CONTROLLER = "Expeditions:XPD_AC02_Sensation:QuestScript"
TAX_FRAGMENT = "Fragments:Quests:QF_XPD_AC02_Mission_006BAA3D"
SENSATION_FRAGMENT = "Fragments:Quests:QF_XPD_AC01_Mission_006B231C"
MASTER = "Expeditions:Master"
PATCHED_SCRIPTS = {
    TAX_CONTROLLER,
    SENSATION_CONTROLLER,
    TAX_FRAGMENT,
    SENSATION_FRAGMENT,
}

TAX_FRAGMENT_STAGES = {
    0,
    100,
    115,
    150,
    200,
    215,
    245,
    250,
    300,
    345,
    350,
    400,
    450,
    460,
    500,
    600,
    605,
    610,
    612,
    615,
    620,
    800,
    899,
    1000,
    1010,
    1020,
    1025,
    1030,
    1031,
    1032,
    1033,
    1034,
    1035,
    1036,
    1037,
    1038,
    1040,
    1045,
    1050,
    1599,
    1600,
    1650,
    1700,
    1725,
    1750,
    1775,
    1800,
    1899,
    2000,
    2100,
    2599,
    2600,
    2650,
    2700,
    2899,
    3000,
    3010,
    3150,
    3500,
    3599,
    3600,
    3651,
    3652,
    3653,
    3654,
    3655,
    3656,
    3657,
    3658,
    3659,
    3670,
    3671,
    3672,
    3673,
    3674,
    3680,
    3681,
    3682,
    3683,
    3684,
    3690,
    3700,
    4499,
    4500,
    4510,
    4515,
    4530,
    4699,
    4700,
    4710,
    4720,
    5000,
    5010,
    5100,
    5200,
    5400,
    5600,
    9000,
}
SENSATION_FRAGMENT_STAGES = {
    0,
    100,
    200,
    300,
    350,
    400,
    450,
    460,
    500,
    600,
    800,
    850,
    860,
    899,
    1000,
    1100,
    1200,
    1210,
    1300,
    1599,
    1600,
    1750,
    1800,
    1801,
    1805,
    1806,
    1810,
    1820,
    1850,
    1860,
    1870,
    1875,
    1880,
    1885,
    1899,
    2000,
    2100,
    2200,
    2300,
    2599,
    2600,
    2700,
    2800,
    2899,
    3000,
    3100,
    3110,
    3200,
    3210,
    3220,
    3230,
    3240,
    3290,
    3400,
    3410,
    3420,
    3430,
    3440,
    3599,
    3600,
    4499,
    4500,
    4510,
    4530,
    4540,
    4699,
    4700,
    5200,
    5400,
    5600,
    9000,
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


def _fragment_members(script_name: str) -> set[str]:
    return {
        name
        for name, _start, _end in _members(_patch(script_name))
        if name.startswith("fragment_stage_")
    }


def _expected_fragment_members(stages: set[int]) -> set[str]:
    return {f"fragment_stage_{stage:04d}_item_00" for stage in stages}


@pytest.mark.parametrize("script_name", sorted(PATCHED_SCRIPTS))
def test_ac_root_patches_are_member_only_unique_and_idempotent(script_name: str):
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


def test_ac_root_fragment_coverage_matches_source_vmad_exactly():
    assert len(TAX_FRAGMENT_STAGES) == 98
    assert len(SENSATION_FRAGMENT_STAGES) == 71
    assert _fragment_members(TAX_FRAGMENT) == _expected_fragment_members(
        TAX_FRAGMENT_STAGES
    )
    assert _fragment_members(SENSATION_FRAGMENT) == _expected_fragment_members(
        SENSATION_FRAGMENT_STAGES
    )


def test_tax_recovers_real_children_and_uses_viable_fixed_third_phase():
    patch = _patch(TAX_CONTROLLER)
    register = _member_body(patch, "registerlocaltaxselection")
    for form_id in (
        "0x0064CD4E",
        "0x0064CD4D",
        "0x0064BC52",
        "0x0064DD30",
        "0x0064BC50",
        "0x006507B5",
    ):
        assert form_id in register
    assert "aiSelectionStage >= 31 && aiSelectionStage <= 33" in register
    assert "moduleFormID = 0x0064BC50" in register
    assert "0x00696D93" not in patch
    assert 'Game.GetFormFromFile(moduleFormID, "SeventySix.esm") as Quest' in register
    assert "RegisterObjectiveModule(phase, moduleQuest, abResetModule)" in register


def test_sensation_recovers_all_nine_real_children_from_stage_semantics():
    patch = _patch(SENSATION_CONTROLLER)
    register = _member_body(patch, "registerlocalsensationselection")
    for form_id in (
        "0x0064BC52",
        "0x0064D26C",
        "0x006BABB6",
        "0x006507B5",
        "0x0064DD30",
        "0x0064BC50",
        "0x0064CD4E",
        "0x0064EA14",
        "0x0064CD4D",
    ):
        assert form_id in register
    assert "0x00696D93" not in patch
    assert "RegisterObjectiveModule(phase, moduleQuest, abResetModule)" in register


@pytest.mark.parametrize(
    ("script_name", "poll_member"),
    [
        (TAX_CONTROLLER, "polllocaltaxphase"),
        (SENSATION_CONTROLLER, "polllocalsensationphase"),
    ],
)
def test_ac_roots_resume_child_progress_and_advance_parent_once(
    script_name: str, poll_member: str
):
    patch = _patch(script_name)
    init = _member_body(patch, "onquestinit")
    load = _member_body(patch, "actor.onplayerloadgame")
    poll = _member_body(patch, poll_member)
    stage = _member_body(patch, "onstageset")

    assert "InitializeLocal" in init
    assert "InitializeLocal" in load
    assert 'RegisterForRemoteEvent(playerRef, "OnPlayerLoadGame")' in patch
    assert "moduleQuest.IsStageDone(9000)" in poll
    assert "IsStageDone(aiCompletionStage)" in poll
    assert "SetStage(aiCompletionStage)" in poll
    assert "PollLocal" in _member_body(patch, "ontimer")
    assert "auiStageID == 1000" in stage
    assert "auiStageID == 2000" in stage
    assert "auiStageID == 3000" in stage


@pytest.mark.parametrize("script_name", [TAX_CONTROLLER, SENSATION_CONTROLLER])
def test_ac_roots_use_local_travel_cleanup_and_no_duplicate_rewards(
    script_name: str,
):
    patch = _patch(script_name)
    stage = _member_body(patch, "onstageset")
    positions = [
        stage.index(f"auiStageID == {value}") for value in (400, 450, 460, 500)
    ]
    assert positions == sorted(positions)
    for value in (450, 460, 500, 600, 5400, 5600, 9000, 10000):
        assert str(value) in stage
    assert "StopAllObjectiveModules()" in patch
    assert "Stop()" in stage
    assert "AddItem(" not in patch
    assert "Game.AddAchievement" not in patch
    assert "SendCustomEvent" not in patch


@pytest.mark.parametrize("script_name", [TAX_FRAGMENT, SENSATION_FRAGMENT])
def test_ac_root_completion_is_idempotent_and_reward_free(script_name: str):
    patch = _patch(script_name)
    completion = _member_body(patch, "fragment_stage_9000_item_00")
    assert "If !IsCompleted()" in completion
    assert "CompleteQuest()" in completion
    assert "10000" not in completion
    assert "AddItem(" not in patch
    assert "Reward" not in patch


@pytest.mark.parametrize(
    ("script_name", "init_member", "schedule_member", "cleanup_member", "setter"),
    [
        (
            TAX_CONTROLLER,
            "initializelocaltaxmission",
            "schedulelocaltaxshutdown",
            "cleanuplocaltaxmission",
            "SetLocalTaxStage(10000)",
        ),
        (
            SENSATION_CONTROLLER,
            "initializelocalsensationmission",
            "schedulelocalsensationshutdown",
            "cleanuplocalsensationmission",
            "SetLocalSensationStage(10000)",
        ),
    ],
)
def test_ac_root_delays_shutdown_until_stage_9000_reward_handlers_finish(
    script_name: str,
    init_member: str,
    schedule_member: str,
    cleanup_member: str,
    setter: str,
):
    patch = _patch(script_name)
    stage = _member_body(patch, "onstageset")
    timer = _member_body(patch, "ontimer")
    init = _member_body(patch, init_member)
    schedule = _member_body(patch, schedule_member)
    cleanup = _member_body(patch, cleanup_member)

    assert "auiStageID == 9000" in stage
    assert schedule_member in stage.lower()
    assert "auiStageID == 10000" in stage
    assert "Stop()" in stage
    assert "aiTimerID == 99" in timer
    assert "IsStageDone(9000) && !IsStageDone(10000)" in timer
    assert setter in timer
    assert "IsStageDone(9000)" in init
    assert schedule_member in init.lower()
    assert "CancelTimer(99)" in schedule
    assert "StartTimer(2.0, 99)" in schedule
    assert "CancelTimer(99)" in cleanup


@pytest.mark.parametrize(
    (
        "script_name",
        "ensure_member",
        "stop_member",
        "entry_stage",
        "form_ids",
        "dialogue_vars",
    ),
    [
        (
            TAX_CONTROLLER,
            "ensurelocaltaxdialoguequests",
            "stoplocaltaxdialoguequests",
            240,
            ("0x006D5082", "0x006C3517"),
            ("billyDialogue", "salDialogue"),
        ),
        (
            SENSATION_CONTROLLER,
            "ensurelocalsensationdialoguequests",
            "stoplocalsensationdialoguequests",
            300,
            ("0x006FA1DF", "0x006FAA69"),
            ("charlotteDialogue", "veracioDialogue"),
        ),
    ],
)
def test_ac_roots_recover_dropped_dialogue_modules_and_clean_up(
    script_name: str,
    ensure_member: str,
    stop_member: str,
    entry_stage: int,
    form_ids: tuple[str, str],
    dialogue_vars: tuple[str, str],
):
    patch = _patch(script_name)
    init_member = (
        "initializelocaltaxmission"
        if script_name == TAX_CONTROLLER
        else "initializelocalsensationmission"
    )
    init = _member_body(patch, init_member)
    stage = _member_body(patch, "onstageset")
    ensure = _member_body(patch, ensure_member)
    stop = _member_body(patch, stop_member)
    cleanup_member = (
        "cleanuplocaltaxmission"
        if script_name == TAX_CONTROLLER
        else "cleanuplocalsensationmission"
    )
    cleanup = _member_body(patch, cleanup_member)

    for form_id in form_ids:
        assert form_id in ensure
        assert form_id in stop
    assert f"auiStageID == {entry_stage}" in stage
    assert ensure_member in stage.lower()
    assert ensure_member in init.lower()
    assert f"IsStageDone({entry_stage})" in ensure
    assert "IsStageDone(9000)" in ensure
    assert ".IsRunning()" in ensure
    assert ".IsStarting()" in ensure
    assert ".Start()" in ensure
    for dialogue_var in dialogue_vars:
        stopped_guard = (
            f"If {dialogue_var} != None && !{dialogue_var}.IsRunning() "
            f"&& !{dialogue_var}.IsStarting()"
        )
        replay_guard = (
            f"If {dialogue_var}.IsCompleted() || "
            f"{dialogue_var}.GetCurrentStageID() > 0"
        )
        reset = f"{dialogue_var}.Reset()"
        start = f"{dialogue_var}.Start()"
        assert stopped_guard in ensure
        assert replay_guard in ensure
        assert ensure.index(stopped_guard) < ensure.index(replay_guard)
        assert ensure.index(replay_guard) < ensure.index(reset)
        assert ensure.index(reset) < ensure.index(start)
    assert ".Stop()" in stop
    assert ".Reset()" not in stop
    assert stop_member in cleanup.lower()


@pytest.mark.parametrize("script_name", sorted(PATCHED_SCRIPTS))
def test_ac_root_full_production_merge_native_compiles_for_fo4(
    script_name: str, tmp_path: Path
):
    base_source = _fo4_base_source()
    assert base_source is not None, "FO4 base Papyrus sources unavailable"

    patched_imports = tmp_path / "patched_imports"
    master_path = patched_imports / _script_relative_path(MASTER, ".psc")
    master_path.parent.mkdir(parents=True, exist_ok=True)
    master_path.write_text(_merged(MASTER), encoding="utf-8")

    result = compile_psc(
        _merged(script_name),
        imports=[str(patched_imports), str(SOURCE_ROOT), str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=str(_script_relative_path(script_name, ".psc")),
    )
    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None
