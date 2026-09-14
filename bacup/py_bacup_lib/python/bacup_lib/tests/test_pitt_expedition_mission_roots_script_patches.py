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
PITT01_CONTROLLER = "Expeditions:XPD_Pitt01:QuestScript"
PITT02_CONTROLLER = "Expeditions:XPD_Pitt02:QuestScript"
PITT01_FRAGMENT = "Fragments:Quests:QF_XPD_Pitt01_006274EC"
PITT02_FRAGMENT = "Fragments:Quests:QF_XPD_Pitt02_Mission_00648280"
MASTER = "Expeditions:Master"
FOREMAN_TRIGGER = "Expeditions:XPD_Pitt02:AliasSetStageOnTriggerEnter"
PATCHED_SCRIPTS = {
    PITT01_CONTROLLER,
    PITT02_CONTROLLER,
    PITT01_FRAGMENT,
    PITT02_FRAGMENT,
    FOREMAN_TRIGGER,
}

PITT01_FRAGMENT_STAGES = {
    0,
    100,
    200,
    400,
    450,
    460,
    500,
    600,
    700,
    760,
    799,
    800,
    1000,
    1297,
    1410,
    1420,
    1430,
    1440,
    1450,
    1500,
    1525,
    1550,
    1599,
    1600,
    1650,
    1660,
    1665,
    1670,
    1675,
    1680,
    1690,
    1695,
    1699,
    1700,
    1800,
    1825,
    1850,
    1925,
    1950,
    1975,
    2000,
    2100,
    2200,
    2400,
    2450,
    2451,
    2452,
    2599,
    2600,
    3000,
    3100,
    3250,
    3251,
    3252,
    3253,
    3254,
    3440,
    3441,
    3442,
    3450,
    3451,
    3452,
    3460,
    3461,
    3462,
    3599,
    3600,
    4499,
    4500,
    4505,
    4510,
    4520,
    4530,
    4531,
    4532,
    4540,
    4699,
    4700,
    4750,
    4900,
    4950,
    5200,
    5400,
    5600,
    9000,
    10000,
}
PITT02_FRAGMENT_STAGES = {
    0,
    100,
    200,
    400,
    450,
    460,
    500,
    600,
    799,
    800,
    810,
    820,
    1000,
    1100,
    1200,
    1300,
    1400,
    1599,
    1600,
    1700,
    1710,
    1720,
    1750,
    1760,
    1800,
    1810,
    1820,
    1850,
    1899,
    2000,
    2200,
    2300,
    2310,
    2320,
    2330,
    2400,
    2599,
    2600,
    2700,
    2899,
    3000,
    3010,
    3020,
    3030,
    3100,
    3110,
    3120,
    3200,
    3300,
    3310,
    3320,
    3330,
    3340,
    3400,
    3450,
    3599,
    3600,
    4499,
    4500,
    4600,
    4650,
    4660,
    4699,
    4700,
    4750,
    5000,
    5001,
    5005,
    5010,
    5015,
    5018,
    5019,
    5020,
    5025,
    5028,
    5029,
    5030,
    5035,
    5038,
    5039,
    5090,
    5100,
    5110,
    5200,
    5210,
    5400,
    5600,
    6900,
    7000,
    7010,
    7100,
    7110,
    7200,
    7210,
    7300,
    7400,
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
def test_pitt_root_patches_are_member_only_unique_and_idempotent(script_name: str):
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


def test_pitt_root_fragment_coverage_matches_source_vmad_exactly():
    assert len(PITT01_FRAGMENT_STAGES) == 86
    assert len(PITT02_FRAGMENT_STAGES) == 97
    assert _fragment_members(PITT01_FRAGMENT) == _expected_fragment_members(
        PITT01_FRAGMENT_STAGES
    )
    assert _fragment_members(PITT02_FRAGMENT) == _expected_fragment_members(
        PITT02_FRAGMENT_STAGES
    )


@pytest.mark.parametrize(
    ("script_name", "register_member", "form_ids"),
    [
        (
            PITT01_CONTROLLER,
            "registerlocalpitt01selection",
            {
                "0x0064BC52",
                "0x0064CD4E",
                "0x0064EA14",
                "0x0064D26C",
                "0x006507B5",
                "0x0064BC50",
                "0x0064DD30",
                "0x0064CD4D",
                "0x0064C2D0",
            },
        ),
        (
            PITT02_CONTROLLER,
            "registerlocalpitt02selection",
            {
                "0x0064DD30",
                "0x0064C2D0",
                "0x006507B5",
                "0x0064D26C",
                "0x0064CD4D",
                "0x0064BC52",
                "0x0064BC50",
                "0x0064CD4E",
                "0x0064EA14",
            },
        ),
    ],
)
def test_pitt_roots_recover_all_concrete_child_quests(
    script_name: str, register_member: str, form_ids: set[str]
):
    patch = _patch(script_name)
    register = _member_body(patch, register_member)
    for form_id in form_ids:
        assert form_id in register
    assert 'Game.GetFormFromFile(moduleFormID, "SeventySix.esm") as Quest' in register
    assert "RegisterObjectiveModule(phase, moduleQuest, abResetModule)" in register
    assert "QuestModule" not in register


@pytest.mark.parametrize(
    ("script_name", "poll_member"),
    [
        (PITT01_CONTROLLER, "polllocalpitt01phase"),
        (PITT02_CONTROLLER, "polllocalpitt02phase"),
    ],
)
def test_pitt_roots_resume_children_and_advance_parent_once(
    script_name: str, poll_member: str
):
    patch = _patch(script_name)
    poll = _member_body(patch, poll_member)
    stage = _member_body(patch, "onstageset")
    assert 'RegisterForRemoteEvent(playerRef, "OnPlayerLoadGame")' in patch
    assert "RestoreLocal" in patch
    assert "ResumeLocal" in patch
    assert "moduleQuest.IsStageDone(9000)" in poll
    assert "IsStageDone(aiCompletionStage)" in poll
    assert "SetStage(aiCompletionStage)" in poll
    for phase_stage in (1000, 1600, 2000, 2600, 3000, 3600):
        assert f"auiStageID == {phase_stage}" in stage


def test_pitt01_finale_optional_tracks_union_fighters_locally():
    patch = _patch(PITT01_CONTROLLER)
    register = _member_body(patch, "registerlocalfinalefighters")
    reconcile = _member_body(patch, "reconcilelocalfinalefighters")
    death = _member_body(patch, "actor.ondeath")
    assert 'RegisterForRemoteEvent(fighterRef, "OnDeath")' in register
    assert "fighterRef.IsDead()" in reconcile
    assert "fighterRef.IsBleedingOut()" in reconcile
    assert "SetObjectiveDisplayed(fighterData.FighterReviveObjective" in reconcile
    assert "SetLocalPitt01Stage(ProtectUnionersFailedStage)" in reconcile
    assert "SetLocalPitt01Stage(4531)" in reconcile
    assert "IsLocalFinaleFighter(akSender)" in death


def test_pitt02_survivor_optional_reconciles_rescue_and_death_once():
    patch = _patch(PITT02_CONTROLLER)
    register = _member_body(patch, "registerlocalsurvivors")
    reconcile = _member_body(patch, "reconcilelocalsurvivors")
    assert 'RegisterForRemoteEvent(survivorRef, "OnDeath")' in register
    assert "firstSurvivor.IsDead()" in reconcile
    assert "secondSurvivor.IsDead()" in reconcile
    assert "thirdSurvivor.IsDead()" in reconcile
    assert "firstDone && secondDone && thirdDone" in reconcile
    assert "SetLocalPitt02Stage(5100)" in reconcile
    assert "SetLocalPitt02Stage(5110)" in reconcile
    assert reconcile.index("SetLocalPitt02Stage(5090)") < reconcile.index(
        "SetLocalPitt02Stage(5100)"
    )


@pytest.mark.parametrize("script_name", [PITT01_CONTROLLER, PITT02_CONTROLLER])
def test_pitt_roots_use_static_travel_cleanup_and_no_direct_rewards(
    script_name: str,
):
    patch = _patch(script_name)
    stage = _member_body(patch, "onstageset")
    positions = [
        stage.index(f"auiStageID == {value}\n") for value in (400, 450, 460, 500)
    ]
    assert positions == sorted(positions)
    for value in (450, 460, 500, 5200, 5400, 5600, 9000, 10000):
        assert str(value) in stage
    assert "StopAllObjectiveModules()" in patch
    assert "Stop()" in stage
    assert "AddItem(" not in patch
    assert "SendCustomEvent" not in patch


@pytest.mark.parametrize("script_name", [PITT01_FRAGMENT, PITT02_FRAGMENT])
def test_pitt_root_completion_is_once_cleanup_chained_and_reward_free(
    script_name: str,
):
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
            PITT01_CONTROLLER,
            "initializelocalpitt01mission",
            "schedulelocalpitt01shutdown",
            "cleanuplocalpitt01mission",
            "SetLocalPitt01Stage(10000)",
        ),
        (
            PITT02_CONTROLLER,
            "initializelocalpitt02mission",
            "schedulelocalpitt02shutdown",
            "cleanuplocalpitt02mission",
            "SetLocalPitt02Stage(10000)",
        ),
    ],
)
def test_pitt_root_delays_shutdown_until_stage_9000_reward_handlers_finish(
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


def test_pitt02_foreman_trigger_is_player_only_gated_and_idempotent():
    patch = _patch(FOREMAN_TRIGGER)
    trigger = _member_body(patch, "ontriggerenter")
    assert "akActionRef == Game.GetPlayer()" in trigger
    assert "PrereqStage < 0 || owningQuest.IsStageDone(PrereqStage)" in trigger
    assert "TurnOffStage < 0 || !owningQuest.IsStageDone(TurnOffStage)" in trigger
    assert "!owningQuest.IsStageDone(StageToSet)" in trigger
    assert "owningQuest.SetStage(StageToSet)" in trigger


@pytest.mark.parametrize("script_name", sorted(PATCHED_SCRIPTS))
def test_pitt_root_full_production_merge_native_compiles_for_fo4(
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
