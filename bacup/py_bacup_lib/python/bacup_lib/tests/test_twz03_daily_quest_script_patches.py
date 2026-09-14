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
QUEST_FRAGMENT = "Fragments:Quests:QF_TWZ03_001155AF"

PATCH_MEMBERS = {
    "TWZ03_TargetActivateScript": {"onactivate"},
    QUEST_FRAGMENT: {
        "completeifalltargetsplaced",
        "placetarget",
        "fragment_stage_0011_item_00",
        "fragment_stage_0012_item_00",
        "fragment_stage_0013_item_00",
        "fragment_stage_0014_item_00",
        "fragment_stage_0015_item_00",
        "fragment_stage_0100_item_00",
        "fragment_stage_0200_item_00",
        "fragment_stage_0300_item_00",
        "fragment_stage_1000_item_00",
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
def test_twz03_patch_is_member_only_and_complete(
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
def test_twz03_patch_merges_once_and_native_compiles(
    script_name: str, members: set[str]
):
    patch = _script_patch_source(script_name)
    assert patch is not None
    merged = _merged_production_source(script_name)
    names = _member_names(merged)

    for member in members:
        assert names.count(member) == 1
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


def test_twz03_target_feedback_preserves_the_bound_stage_contract():
    target = _script_patch_source("TWZ03_TargetActivateScript")
    assert target is not None

    activation = _member_body(target, "onactivate")
    assert "akActivator != Alias_Player.GetReference()" in activation
    assert "owningQuest.IsStageDone(StageToCheck)" in activation
    assert "TWZ03_AlreadyPlaced.Show()" in activation
    assert "!owningQuest.IsStageDone(HasTargetsStage)" in activation
    assert "TWZ03_NoTargets.Show()" in activation
    assert "SetStage(" not in activation


@pytest.mark.parametrize(
    ("stage", "target_alias", "paper_target"),
    [
        (11, "Alias_Target01", "TWZ03PaperTarget01Ref"),
        (12, "Alias_Target02", "TWZ03PaperTarget02Ref"),
        (13, "Alias_Target03", "TWZ03PaperTarget03Ref"),
        (14, "Alias_Target04", "TWZ03PaperTarget04Ref"),
        (15, "Alias_Target05", "TWZ03PaperTarget05Ref"),
    ],
)
def test_twz03_fixed_target_fragments_pair_the_retained_aliases_and_visuals(
    stage: int, target_alias: str, paper_target: str
):
    quest = _script_patch_source(QUEST_FRAGMENT)
    assert quest is not None

    fragment = _member_body(
        quest, f"fragment_stage_{stage:04d}_item_00"
    )
    assert f"PlaceTarget({target_alias}, {paper_target}, {stage})" in fragment


def test_twz03_stage_chain_advances_through_all_five_target_holders():
    quest = _script_patch_source(QUEST_FRAGMENT)
    assert quest is not None

    start = _member_body(quest, "fragment_stage_0100_item_00")
    acquired = _member_body(quest, "fragment_stage_0300_item_00")
    complete = _member_body(quest, "completeifalltargetsplaced")

    assert "SetObjectiveDisplayed(100)" in start
    assert "TWZ03_Intro.Start()" in start
    assert "SetObjectiveCompleted(200)" in acquired
    for stage in range(11, 16):
        assert f"SetObjectiveDisplayed({stage})" in acquired
        assert f"IsStageDone({stage})" in complete
    assert "SetStage(1000)" in complete
