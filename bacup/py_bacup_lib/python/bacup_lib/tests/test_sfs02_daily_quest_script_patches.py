from __future__ import annotations

from pathlib import Path

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
QUEST_FRAGMENT = "Fragments:Quests:QF_SFS02_Play_0018C91A"
PATCH_MEMBERS = {
    f"fragment_stage_{stage:04d}_item_00"
    for stage in (
        10,
        50,
        100,
        150,
        175,
        200,
        210,
        211,
        212,
        250,
        300,
        350,
        400,
        410,
        450,
        500,
        510,
        511,
        512,
        550,
        600,
        1000,
    )
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


def _merged_production_source() -> str:
    source_path = SOURCE_ROOT / _script_relative_path(QUEST_FRAGMENT, ".psc")
    patch = _script_patch_source(QUEST_FRAGMENT)
    assert source_path.is_file(), source_path
    assert patch is not None
    return _merge_script_method_patches(
        source_path.read_text(encoding="utf-8"), patch
    )


def test_sfs02_patch_is_member_only_and_covers_every_bound_fragment():
    patch = _script_patch_source(QUEST_FRAGMENT)

    assert patch is not None
    assert set(_member_names(patch)) == PATCH_MEMBERS
    assert not any(
        line.strip().lower().startswith(("scriptname ", "state "))
        for line in patch.splitlines()
    )
    assert not any(
        " property " in f" {line.strip().lower()} " for line in patch.splitlines()
    )


def test_sfs02_patch_merges_once_and_native_compiles():
    patch = _script_patch_source(QUEST_FRAGMENT)
    assert patch is not None
    merged = _merged_production_source()
    names = _member_names(merged)

    for member in PATCH_MEMBERS:
        assert names.count(member) == 1
    assert _merge_script_method_patches(merged, patch) == merged

    base_source = _fo4_base_source()
    assert base_source is not None, "FO4 base Papyrus sources unavailable"
    result = compile_psc(
        merged,
        imports=[str(SOURCE_ROOT), str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=str(_script_relative_path(QUEST_FRAGMENT, ".psc")),
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None


def test_sfs02_start_and_assignment_selection_form_a_complete_chain():
    patch = _script_patch_source(QUEST_FRAGMENT)
    assert patch is not None

    startup = _member_body(patch, "fragment_stage_0010_item_00")
    investigate = _member_body(patch, "fragment_stage_0050_item_00")
    meet_chloe = _member_body(patch, "fragment_stage_0100_item_00")
    assignment = _member_body(patch, "fragment_stage_0150_item_00")

    assert "SetStage(50)" in startup
    assert "SetObjectiveDisplayed(50, True)" in investigate
    assert "SetObjectiveCompleted(50, True)" in meet_chloe
    assert "SetObjectiveDisplayed(100, True)" in meet_chloe
    assert "SetObjectiveCompleted(100, True)" in assignment
    assert "Utility.RandomInt(1, 4)" in assignment
    for stage in (200, 300, 400, 500):
        assert f"SetStage({stage})" in assignment


def test_sfs02_first_and_repeat_intros_use_the_retained_tracking_value():
    patch = _script_patch_source(QUEST_FRAGMENT)
    assert patch is not None

    intro = _member_body(patch, "fragment_stage_0175_item_00")
    shutdown = _member_body(patch, "fragment_stage_1000_item_00")

    assert "GetValue(SFS02_Play_QuestTrackingValue) > 0.0" in intro
    assert "SFS02_Play_SophieIntro.Start()" in intro
    assert "SFS02_Play_ChloeIntro.Start()" in intro
    assert "SetValue(SFS02_Play_QuestTrackingValue, 1.0)" in shutdown
    assert "SFS02_Play_ActivityGlobal.SetValueInt(0)" in shutdown


def test_sfs02_collection_branches_only_finish_after_all_required_items():
    patch = _script_patch_source(QUEST_FRAGMENT)
    assert patch is not None

    for stage, objective, peers in (
        (210, 200, (211, 212)),
        (211, 210, (210, 212)),
        (212, 220, (210, 211)),
    ):
        body = _member_body(patch, f"fragment_stage_{stage:04d}_item_00")
        assert f"SetObjectiveCompleted({objective}, True)" in body
        for peer in peers:
            assert f"IsStageDone({peer})" in body
        assert "SetStage(250)" in body

    for stage, peers in (
        (510, (511, 512)),
        (511, (510, 512)),
        (512, (510, 511)),
    ):
        body = _member_body(patch, f"fragment_stage_{stage:04d}_item_00")
        for peer in peers:
            assert f"IsStageDone({peer})" in body
        assert "SetStage(550)" in body


def test_sfs02_branch_completion_retains_items_and_returns_to_chloe():
    patch = _script_patch_source(QUEST_FRAGMENT)
    assert patch is not None

    assert "RemoveItem(" not in patch
    for stage, tracking_value in (
        (250, "SFS02_Play_FrogTrackingValue"),
        (350, "SFS02_Play_FlowerTrackingValue"),
        (450, "SFS02_Play_PlayDateTrackingValue"),
        (550, "SFS02_Play_ToyTrackingValue"),
    ):
        body = _member_body(patch, f"fragment_stage_{stage:04d}_item_00")
        assert f"SetValue({tracking_value}, 1.0)" in body
        assert "SetStage(600)" in body

    return_to_chloe = _member_body(patch, "fragment_stage_0600_item_00")
    shutdown = _member_body(patch, "fragment_stage_1000_item_00")

    assert "SetObjectiveDisplayed(600, True)" in return_to_chloe
    assert "SFS02_Play_ChloeEnd.Start()" in return_to_chloe
    assert "SetObjectiveCompleted(600, True)" in shutdown
    assert "CompleteQuest()" in shutdown
    assert "Stop()" in shutdown
