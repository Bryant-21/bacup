from __future__ import annotations

from collections import Counter
from pathlib import Path
import re

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
SCRIPT_NAME = "Fragments:Quests:QF_MILE_MeltdownPointerQuest_0078E2CC"
STAGES = (1000, 1100, 1200, 2000, 3000, 4000, 9000)
STAGE_MEMBERS = {
    f"fragment_stage_{stage:04d}_item_00" for stage in STAGES
}


def _patch() -> str:
    patch = _script_patch_source(SCRIPT_NAME)
    assert patch is not None
    return patch


def _member_names(source: str) -> list[str]:
    return [
        name
        for kind, name, _start, _end in _iter_top_level_papyrus_members(
            source.splitlines()
        )
        if kind in {"function", "event"}
    ]


def _member_body(source: str, member_name: str) -> str:
    lines = source.splitlines()
    start, end = next(
        (start, end)
        for _kind, name, start, end in _iter_top_level_papyrus_members(lines)
        if name == member_name.casefold()
    )
    return "\n".join(lines[start : end + 1])


def _merged() -> str:
    source_path = SOURCE_ROOT / _script_relative_path(SCRIPT_NAME, ".psc")
    assert source_path.is_file(), source_path
    return _merge_script_method_patches(
        source_path.read_text(encoding="utf-8"), _patch()
    )


def _merged_stage(stage: int) -> str:
    return _member_body(
        _merged(), f"fragment_stage_{stage:04d}_item_00"
    )


def _assert_ordered(body: str, *statements: str) -> None:
    cursor = -1
    for statement in statements:
        cursor = body.find(statement, cursor + 1)
        assert cursor >= 0, statement


def test_patch_defines_exact_live_vmad_fragment_members() -> None:
    patch = _patch()
    members = _member_names(patch)

    assert set(members) == STAGE_MEMBERS
    assert Counter(members) == Counter({member: 1 for member in STAGE_MEMBERS})
    assert "Scriptname " not in patch


def test_production_merge_is_unique_and_idempotent() -> None:
    patch = _patch()
    merged = _merged()
    merged_members = Counter(_member_names(merged))

    for member in STAGE_MEMBERS:
        assert merged_members[member] == 1
        assert _member_body(merged, member) == _member_body(patch, member)
    assert _merge_script_method_patches(merged, patch) == merged


def test_local_breadcrumb_contract_is_complete_and_uses_the_no_crafting_substitute() -> None:
    assert "SetStage(1100)" in _merged_stage(1000)
    assert "SetObjectiveDisplayed(5)" in _merged_stage(1100)
    _assert_ordered(
        _merged_stage(1200),
        "SetObjectiveCompleted(5)",
        "SetObjectiveDisplayed(10)",
    )
    _assert_ordered(
        _merged_stage(2000),
        "SetObjectiveCompleted(10)",
        "SetObjectiveDisplayed(20)",
        "Actor playerRef = Alias_Player.GetActorReference()",
        'Weapon meltdownCarbine = Game.GetFormFromFile(0x006F5790, "SeventySix.esm") as Weapon',
        "If playerRef.GetItemCount(meltdownCarbine) < 1",
        "playerRef.AddItem(meltdownCarbine, 1, True)",
        "If !IsStageDone(3000)",
        "SetStage(3000)",
    )
    stage_2000 = _merged_stage(2000)
    assert stage_2000.count("playerRef.AddItem(meltdownCarbine, 1, True)") == 1
    assert stage_2000.count("SetStage(3000)") == 1

    stage_3000 = _merged_stage(3000)
    _assert_ordered(
        stage_3000,
        "SetObjectiveCompleted(20)",
        "SetObjectiveDisplayed(30)",
        "ObjectReference playerRef = Alias_Player.GetReference()",
        "If playerRef != None && MILE_SQ_CraftedMeltdownAV != None",
        "playerRef.SetValue(MILE_SQ_CraftedMeltdownAV, 1.0)",
        "EndIf",
    )
    assert stage_3000.count(
        "playerRef.SetValue(MILE_SQ_CraftedMeltdownAV, 1.0)"
    ) == 1
    assert "ModValue(" not in stage_3000

    merged_members = "\n".join(_merged_stage(stage) for stage in STAGES)
    assert re.search(r"\.\s*Start\s*\(", merged_members, re.IGNORECASE) is None
    assert re.search(
        r"(?<![.\w])Start\s*\(", merged_members, re.IGNORECASE
    ) is None
    merged_members_folded = merged_members.casefold()
    assert "storyevent" not in merged_members_folded
    assert merged_members_folded.count("additem(") == 1
    assert "reward" not in merged_members_folded
    assert "theoemployeetierav" not in merged_members_folded
    assert "ammomerchantgreetingav" not in merged_members_folded
    assert "alias_ammomerchantvendorchest" not in merged_members_folded


def test_completion_and_stop_are_ordered_terminal_members() -> None:
    _assert_ordered(
        _merged_stage(4000),
        "SetObjectiveCompleted(30)",
        "SetStage(9000)",
    )

    stage_9000 = _merged_stage(9000)
    _assert_ordered(
        stage_9000,
        "Actor playerRef = Game.GetPlayer()",
        "If playerRef != None && MeltdownSchematics != None",
        "playerRef.RemoveItem(MeltdownSchematics, 1, True)",
        "EndIf",
        "Stop()",
    )
    assert stage_9000.count("Game.GetPlayer()") == 1
    assert stage_9000.count("RemoveItem(") == 1
    assert "RemoveAllItems(" not in stage_9000
    assert "Alias_" not in stage_9000
    assert stage_9000.splitlines()[-2:] == ["\tStop()", "EndFunction"]


def test_full_production_merge_native_compiles_for_fo4() -> None:
    base_source = _fo4_base_source()
    assert base_source is not None, "FO4 base Papyrus sources unavailable"

    result = compile_psc(
        _merged(),
        imports=[str(SOURCE_ROOT), str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=str(_script_relative_path(SCRIPT_NAME, ".psc")),
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None
