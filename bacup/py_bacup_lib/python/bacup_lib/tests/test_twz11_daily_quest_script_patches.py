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
QUEST_SCRIPT = "TWZ11_Script"
QUEST_FRAGMENT = "Fragments:Quests:QF_TWZ11_0015B1E2"
PERK_FRAGMENT = "Fragments:Perks:PRKF_TWZ11_BarrelActivation_0004C445"

PATCH_MEMBERS = {
    QUEST_SCRIPT: {"checkforallbarrels", "checkforalldumped"},
    QUEST_FRAGMENT: {
        "fragment_stage_0051_item_00",
        "fragment_stage_0052_item_00",
        "fragment_stage_0053_item_00",
        "fragment_stage_0100_item_00",
        "fragment_stage_0150_item_00",
        "fragment_stage_0200_item_00",
        "fragment_stage_0201_item_00",
        "fragment_stage_0202_item_00",
        "fragment_stage_0220_item_00",
        "fragment_stage_0221_item_00",
        "fragment_stage_0222_item_00",
        "fragment_stage_0300_item_00",
        "fragment_stage_1000_item_00",
    },
    PERK_FRAGMENT: {
        "fragment_entry_00",
        "fragment_entry_01",
        "fragment_entry_02",
    },
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
    return _merge_script_method_patches(
        source_path.read_text(encoding="utf-8"), patch
    )


@pytest.mark.parametrize(("script_name", "members"), PATCH_MEMBERS.items())
def test_twz11_patch_is_member_only_and_complete(
    script_name: str, members: set[str]
):
    patch = _script_patch_source(script_name)

    assert patch is not None
    assert set(_member_names(patch)) == members
    assert not any(
        line.strip().lower().startswith(("scriptname ", "state "))
        for line in patch.splitlines()
    )
    assert not any(
        " property " in f" {line.strip().lower()} " for line in patch.splitlines()
    )


@pytest.mark.parametrize(("script_name", "members"), PATCH_MEMBERS.items())
def test_twz11_patch_merges_once_and_native_compiles(
    script_name: str, members: set[str], tmp_path: Path
):
    patch = _script_patch_source(script_name)
    assert patch is not None
    merged = _merged_production_source(script_name)
    names = _member_names(merged)

    for member in members:
        assert names.count(member) == 1
    assert _merge_script_method_patches(merged, patch) == merged

    merged_controller = _merged_production_source(QUEST_SCRIPT)
    (tmp_path / "TWZ11_Script.psc").write_text(
        merged_controller, encoding="utf-8"
    )

    base_source = _fo4_base_source()
    assert base_source is not None, "FO4 base Papyrus sources unavailable"
    result = compile_psc(
        merged,
        imports=[str(tmp_path), str(SOURCE_ROOT), str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=str(_script_relative_path(script_name, ".psc")),
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None


def test_twz11_helper_advances_only_after_all_three_bound_stages():
    quest = _script_patch_source(QUEST_SCRIPT)
    assert quest is not None

    collected = _member_body(quest, "checkforallbarrels")
    disposed = _member_body(quest, "checkforalldumped")

    for stage in ("Barrel01Stage", "Barrel02Stage", "Barrel03Stage"):
        assert f"IsStageDone({stage})" in collected
    assert "!IsStageDone(AllBarrelsStage)" in collected
    assert "SetStage(AllBarrelsStage)" in collected

    for stage in ("Dumped01Stage", "Dumped02Stage", "Dumped03stage"):
        assert f"IsStageDone({stage})" in disposed
    assert "!IsStageDone(AllDumpedStage)" in disposed
    assert "SetStage(AllDumpedStage)" in disposed


def test_twz11_pickup_sink_and_dump_paths_converge_on_completion():
    quest = _script_patch_source(QUEST_FRAGMENT)
    perk = _script_patch_source(PERK_FRAGMENT)
    assert quest is not None
    assert perk is not None

    start = _member_body(quest, "fragment_stage_0100_item_00")
    assert "playerRef.AddPerk(TWZ11_BarrelActivation)" in start
    for objective in (100, 101, 102):
        assert f"SetObjectiveDisplayed({objective}, True)" in start

    for index, objective in ((200, 100), (201, 101), (202, 102)):
        pickup = _member_body(
            quest, f"fragment_stage_0{index}_item_00"
        )
        assert f"SetObjectiveCompleted({objective}, True)" in pickup
        assert "controller.CheckForAllBarrels()" in pickup

    for index, pickup_stage in ((220, 200), (221, 201), (222, 202)):
        sink = _member_body(quest, f"fragment_stage_0{index}_item_00")
        assert "QSTTWZ11BarrelSink.Play(barrelRef)" in sink
        assert "barrelRef.Disable()" in sink
        assert f"SetStage({pickup_stage})" in sink

    for index in (51, 52, 53):
        dumped = _member_body(quest, f"fragment_stage_00{index}_item_00")
        assert "controller.CheckForAllDumped()" in dumped

    for entry, pickup_stage, disposed_stage in (
        (0, "Barrel01PickupStage", "Barrel01DisposedStage"),
        (1, "Barrel02PickupStage", "Barrel02DisposedStage"),
        (2, "Barrel03PickupStage", "Barrel03DisposedStage"),
    ):
        fragment = _member_body(perk, f"fragment_entry_0{entry}")
        assert f"TWZ11.SetStage({pickup_stage})" in fragment
        assert f"TWZ11.SetStage({disposed_stage})" in fragment

    completed = _member_body(quest, "fragment_stage_1000_item_00")
    for barrel in (
        "TWZ11_ToxicBarrel01",
        "TWZ11_ToxicBarrel02",
        "TWZ11_ToxicBarrel03",
    ):
        assert f"playerRef.RemoveItem({barrel}" in completed
    assert "QSTTWZ11BarrelDump.Play(dumpRef)" in completed
    assert "playerRef.RemovePerk(TWZ11_BarrelActivation)" in completed
    assert "CompleteAllObjectives()" in completed
    assert "Stop()" in completed
