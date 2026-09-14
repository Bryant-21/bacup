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

RSVP03_SCRIPT = "Fragments:Quests:QF_RSVP03_Quest_003A1FE0"
RSVP04_SCRIPT = "Fragments:Quests:QF_RSVP04_QuestPatrol_0050A2ED"
RSVP04_TERMINAL_SCRIPT = "Fragments:Terminals:TERM_RSVP04_Terminal_Main_Co_004F6809"

REGISTERED_RSVP03_STAGES = frozenset(
    {
        0,
        100,
        500,
        1000,
        1100,
        1110,
        1200,
        1225,
        1250,
        1300,
        1310,
        1325,
        1350,
        1400,
        1410,
        1500,
        2000,
        2100,
        2200,
        2300,
        2400,
        2500,
        9000,
        9500,
        9999,
    }
)
REGISTERED_RSVP04_STAGES = frozenset(
    {
        1,
        500,
        1000,
        1001,
        1800,
        1900,
        1901,
        2000,
        2001,
        2200,
        2800,
        2900,
        2901,
        3000,
        3001,
        3100,
        3200,
        3800,
        3900,
        3901,
        4000,
        4001,
        4100,
        4200,
        4800,
        4900,
        5000,
        5100,
        5500,
        9000,
        9500,
        9999,
    }
)

RSVP03_STAGE_CLASSES = {
    "operational": REGISTERED_RSVP03_STAGES - {100, 9500, 9999},
    "no_op": frozenset(),
    "debug": frozenset({100}),
    "shutdown": frozenset({9500, 9999}),
    "insufficient": frozenset(),
}
RSVP04_STAGE_CLASSES = {
    "operational": REGISTERED_RSVP04_STAGES - {4100, 9500, 9999},
    "no_op": frozenset({4100}),
    "debug": frozenset(),
    "shutdown": frozenset({9500, 9999}),
    "insufficient": frozenset(),
}


def _stage_member_name(stage: int) -> str:
    return f"fragment_stage_{stage:04d}_item_00"


def _patched_stage_members(stage_classes: dict[str, frozenset[int]]) -> set[str]:
    return {
        _stage_member_name(stage)
        for stage in stage_classes["operational"] | stage_classes["shutdown"]
    }


PATCH_MEMBERS = {
    RSVP03_SCRIPT: _patched_stage_members(RSVP03_STAGE_CLASSES),
    RSVP04_SCRIPT: _patched_stage_members(RSVP04_STAGE_CLASSES),
    RSVP04_TERMINAL_SCRIPT: {"fragment_terminal_02"},
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
        if name == member_name.lower()
    )
    return "\n".join(lines[start : end + 1])


def _merged_production_source(script_name: str) -> str:
    source_path = SOURCE_ROOT / _script_relative_path(script_name, ".psc")
    patch = _script_patch_source(script_name)
    assert source_path.is_file(), source_path
    assert patch is not None
    return _merge_script_method_patches(source_path.read_text(encoding="utf-8"), patch)


@pytest.mark.parametrize(("script_name", "members"), PATCH_MEMBERS.items())
def test_rsvp_patches_merge_once_and_compile(script_name: str, members: set[str]):
    patch = _script_patch_source(script_name)
    assert patch is not None
    assert set(_member_names(patch)) == members
    assert not any(line.strip().lower().startswith("scriptname ") for line in patch.splitlines())

    merged = _merged_production_source(script_name)
    merged_members = _member_names(merged)
    for member in members:
        assert merged_members.count(member) == 1
    assert _merge_script_method_patches(merged, patch) == merged

    base_source = _fo4_base_source()
    assert base_source is not None, "FO4 base Papyrus sources unavailable"
    result = compile_psc(
        merged,
        imports=[str(SOURCE_ROOT), str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=str(_script_relative_path(script_name, ".psc")),
    )
    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None


@pytest.mark.parametrize(
    ("registered", "stage_classes"),
    (
        (REGISTERED_RSVP03_STAGES, RSVP03_STAGE_CLASSES),
        (REGISTERED_RSVP04_STAGES, RSVP04_STAGE_CLASSES),
    ),
)
def test_rsvp_registered_stage_audit_is_exhaustive(
    registered: frozenset[int], stage_classes: dict[str, frozenset[int]]
):
    classified = set().union(*stage_classes.values())
    assert classified == registered
    assert sum(len(stages) for stages in stage_classes.values()) == len(registered)


def test_rsvp03_camp_build_branches_converge_and_complete():
    patch = _script_patch_source(RSVP03_SCRIPT)
    assert patch is not None
    deployed = _member_body(patch, "fragment_stage_2100_item_00")
    assert "GetValue(pRSVP03_AV_StashObjective) > 0.0" in deployed
    assert "SetObjectiveDisplayed(2200)" in deployed
    assert "SetObjectiveDisplayed(2300)" in deployed
    assert "SetObjectiveDisplayed(2400)" in deployed
    assert "SetStage(2500)" in _member_body(patch, "fragment_stage_2200_item_00")
    assert "SetStage(2500)" in _member_body(patch, "fragment_stage_2300_item_00")
    assert "SetStage(2500)" in _member_body(patch, "fragment_stage_2400_item_00")
    assert "SetValue(pRSVP03_AV_QuestCompletion, 1.0)" in _member_body(
        patch, "fragment_stage_9000_item_00"
    )
    assert "SetValue(pRSVP03_AV_SpawnEncWave0, 1.0)" in _member_body(
        patch, "fragment_stage_1110_item_00"
    )
    assert "SetValue(pRSVP03_AV_SpawnEncWave1, 1.0)" in _member_body(
        patch, "fragment_stage_1310_item_00"
    )
    assert "SetValue(pRSVP03_AV_SpawnEncWave2, 1.0)" in _member_body(
        patch, "fragment_stage_1410_item_00"
    )
    for stage, other_flag in (
        (1325, "pRSVP03_AV_GotProgram"),
        (1350, "pRSVP03_AV_GotSchematics"),
    ):
        body = _member_body(patch, _stage_member_name(stage))
        assert other_flag in body
        assert "SetStage(1400)" in body
    assert "SetStage(9000)" in _member_body(patch, "fragment_stage_2500_item_00")


def test_rsvp04_patrol_objectives_follow_existing_alias_stage_chain():
    patch = _script_patch_source(RSVP04_SCRIPT)
    assert patch is not None
    for stage, completed, displayed in (
        (1000, 500, 1000),
        (2000, 1900, 2000),
        (2200, 2000, 2900),
        (3000, 3000, 3100),
        (3100, 3100, 3200),
        (3200, 3200, 3500),
        (4000, 4000, 4100),
        (4200, 4100, 4900),
        (5000, 5000, 5100),
        (5100, 5100, 5200),
        (5500, 5200, 5500),
    ):
        body = _member_body(patch, f"fragment_stage_{stage:04d}_item_00")
        assert f"SetObjectiveCompleted({completed})" in body
        assert f"SetObjectiveDisplayed({displayed})" in body
    assert "SetValue(pRSVP04_AV_QuestCompletion, 1.0)" in _member_body(
        patch, "fragment_stage_9000_item_00"
    )


def test_rsvp04_holotape_recovery_and_container_handoffs_are_idempotent():
    patch = _script_patch_source(RSVP04_SCRIPT)
    assert patch is not None
    for stage, holotape in (
        (1001, "pRSVP04_Holotape_Patrol_Part1"),
        (1901, "pRSVP04_Holotape_Patrol_Part2"),
        (2901, "pRSVP04_Holotape_Patrol_Part3"),
        (3901, "pRSVP04_Holotape_Patrol_Part4"),
    ):
        body = _member_body(patch, _stage_member_name(stage))
        assert f"GetItemCount({holotape}) == 0" in body
        assert f"AddItem({holotape}, 1, False)" in body
    for stage, alias, holotape in (
        (2001, "Alias_Container_02", "pRSVP04_Holotape_Patrol_Part2"),
        (3001, "Alias_Container_03", "pRSVP04_Holotape_Patrol_Part3"),
        (4001, "Alias_Container_CharlieStation", "pRSVP04_Holotape_Patrol_Part4"),
    ):
        body = _member_body(patch, _stage_member_name(stage))
        assert f"{alias}.GetReference()" in body
        assert f"GetItemCount({holotape}) == 0" in body
        assert f"AddItem({holotape}, 1, True)" in body


def test_rsvp04_resource_request_grants_the_bound_holotape_once():
    patch = _script_patch_source(RSVP04_TERMINAL_SCRIPT)
    assert patch is not None
    body = _member_body(patch, "fragment_terminal_02")
    assert "playerRef.GetItemCount(Holotape_End) == 0" in body
    assert "playerRef.AddItem(Holotape_End, 1, False)" in body
    assert "playerRef.SetValue(AV_gotHolotape, 1.0)" in body
