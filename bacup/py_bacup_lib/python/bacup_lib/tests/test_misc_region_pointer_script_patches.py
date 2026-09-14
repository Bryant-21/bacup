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

POINTER = "GQ_MiscRegionPointerScript"
COMPLETION = "GQ_MiscRegionPointerCompletionScript"

PATCH_MEMBERS = {
    POINTER: {
        "onquestinit",
        "ontimer",
        "onquestshutdown",
        "evaluatecompletionquests",
        "isadvertisedquestreached",
    },
    COMPLETION: {
        "onstageset",
        "stagereachesmiscobjective",
        "ismiscobjectivecomplete",
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
        if name == member_name.casefold()
    )
    return "\n".join(lines[start : end + 1])


def _merged(script_name: str) -> str:
    source_path = SOURCE_ROOT / _script_relative_path(script_name, ".psc")
    patch = _script_patch_source(script_name)
    assert source_path.is_file(), source_path
    assert patch is not None
    return _merge_script_method_patches(source_path.read_text(encoding="utf-8"), patch)


@pytest.mark.parametrize(("script_name", "members"), PATCH_MEMBERS.items())
def test_pointer_patches_are_member_only_and_merge_once(
    script_name: str, members: set[str]
) -> None:
    patch = _script_patch_source(script_name)
    assert patch is not None

    assert set(_member_names(patch)) == members
    assert not any(
        line.strip().casefold().startswith(("scriptname ", "state "))
        for line in patch.splitlines()
    )
    assert not any(
        " property " in f" {line.strip().casefold()} " for line in patch.splitlines()
    )

    merged = _merged(script_name)
    merged_names = _member_names(merged)
    for member in members:
        assert merged_names.count(member) == 1
    assert _merge_script_method_patches(merged, patch) == merged


def test_merged_pointer_pair_compiles_together(tmp_path: Path) -> None:
    base_source = _fo4_base_source()
    assert base_source is not None, "FO4 base Papyrus sources unavailable"

    # The pointer resolves its advertised quests by casting to the completion script, so
    # both merged sources have to be on the import path at once.
    merged_root = tmp_path / "merged"
    merged_root.mkdir()
    sources = {}
    for script_name in PATCH_MEMBERS:
        merged = _merged(script_name)
        sources[script_name] = merged
        (merged_root / f"{script_name}.psc").write_text(merged, encoding="utf-8")

    for script_name, merged in sources.items():
        result = compile_psc(
            merged,
            imports=[str(merged_root), str(SOURCE_ROOT), str(base_source)],
            game="fo4",
            flags=str(base_source / "Institute_Papyrus_Flags.flg"),
            source_path=f"{script_name}.psc",
        )
        diagnostics = "\n".join(str(item) for item in result.diagnostics)
        assert result.ok, f"{script_name}: {diagnostics}"
        assert result.pex_bytes is not None


def test_pointer_shows_its_bound_objective_and_polls_only_while_running() -> None:
    patch = _script_patch_source(POINTER)
    assert patch is not None

    init = _member_body(patch, "onquestinit")
    assert "SetObjectiveDisplayed(ObjectiveID)" in init
    assert "StartTimer(1.0, 100)" in init

    evaluate = _member_body(patch, "evaluatecompletionquests")
    assert evaluate.index("!IsRunning()") < evaluate.index("CompletionQuests.Length")
    # GQ_MiscRegionPointer_FF05 003EBB93 has a CompleteQuest stage and QuestCompletionXP
    # 098952, so a bare Stop() forfeits both. Complete then stop, as FO4's own misc
    # objective pointer QF_MS02_000229F7 does.
    assert evaluate.index("SetObjectiveCompleted(ObjectiveID)") < evaluate.index(
        "CompleteQuest()"
    )
    assert evaluate.index("CompleteQuest()") < evaluate.index("Stop()")
    # The re-arm must sit after the loop, so a pointer that has already completed does
    # not keep a timer alive.
    assert evaluate.index("Stop()") < evaluate.index("StartTimer(10.0, 100)")

    shutdown = _member_body(patch, "onquestshutdown")
    assert "CancelTimer(100)" in shutdown

    # The pointer never starts the quest it advertises; it only observes it.
    assert ".Start()" not in patch
    assert "SetStage(" not in patch


def test_completion_script_latches_its_bound_stage_contract() -> None:
    patch = _script_patch_source(COMPLETION)
    assert patch is not None

    stage_set = _member_body(patch, "onstageset")
    assert stage_set.index("!EventSent") < stage_set.index(
        "StageReachesMiscObjective(auiStageID)"
    )
    assert "EventSent = True" in stage_set

    reaches = _member_body(patch, "stagereachesmiscobjective")
    # StageToCompleteMiscObjective defaults to -1, which means "this quest publishes no
    # misc-objective stage"; CompleteOnHigherStages widens the match to >=.
    assert "StageToCompleteMiscObjective < 0" in reaches
    assert "aiStageID >= StageToCompleteMiscObjective" in reaches
    assert "aiStageID == StageToCompleteMiscObjective" in reaches

    complete = _member_body(patch, "ismiscobjectivecomplete")
    assert complete.index("If EventSent") < complete.index("Return True")
    assert "IsStageDone(StageToCompleteMiscObjective)" in complete
    assert "GetStage() >= StageToCompleteMiscObjective" in complete
    assert "IsCompleted()" in complete


FF05_POINTER_FRAGMENT = "Fragments:Quests:QF_GQ_MiscRegionPointer_FF05_003EBB93"


def test_ff05_pointer_fragment_owns_only_its_two_bound_stages() -> None:
    """QUST 003EBB93 binds exactly two fragments: stage 100 (RunOnStart, "Investigate
    cabin") and stage 1000 (CompleteQuest, "Completion stage"). Its objective 100
    "Investigate the Cabin" targets alias 2 HolotapeLocation. The root
    GQ_MiscRegionPointerScript displays the same objective from OnQuestInit, so both
    fragments have to be idempotent, and stage 1000 must not re-complete the quest that
    the CompleteQuest flag already completes."""
    patch = _script_patch_source(FF05_POINTER_FRAGMENT)
    assert patch is not None

    assert set(_member_names(patch)) == {
        "fragment_stage_0100_item_00",
        "fragment_stage_1000_item_00",
    }
    assert not any(
        line.strip().casefold().startswith(("scriptname ", "state "))
        for line in patch.splitlines()
    )

    start = _member_body(patch, "fragment_stage_0100_item_00")
    assert "SetObjectiveDisplayed(100)" in start
    assert "!IsObjectiveCompleted(100)" in start

    complete = _member_body(patch, "fragment_stage_1000_item_00")
    assert "SetObjectiveCompleted(100)" in complete
    # The stage flag owns completion and the XNAM reward; the fragment must not repeat it.
    assert "CompleteQuest()" not in complete
    assert "Stop()" not in complete
    assert "SetStage(" not in patch


def test_ff05_pointer_fragment_merges_once_and_compiles() -> None:
    base_source = _fo4_base_source()
    assert base_source is not None, "FO4 base Papyrus sources unavailable"

    merged = _merged(FF05_POINTER_FRAGMENT)
    names = _member_names(merged)
    for member in ("fragment_stage_0100_item_00", "fragment_stage_1000_item_00"):
        assert names.count(member) == 1
    patch = _script_patch_source(FF05_POINTER_FRAGMENT)
    assert patch is not None
    assert _merge_script_method_patches(merged, patch) == merged

    result = compile_psc(
        merged,
        imports=[str(SOURCE_ROOT), str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=str(_script_relative_path(FF05_POINTER_FRAGMENT, ".psc")),
    )
    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None
