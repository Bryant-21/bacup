from __future__ import annotations

from collections import Counter
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
FRAGMENT_SCRIPT = "Fragments:Quests:QF_BoS01_00052C77"
CERTIFICATE_SCRIPT = "BoS01GiveCertificateTopic"
BOUND_FRAGMENT_MEMBERS = (
    "fragment_stage_0500_item_00",
    "fragment_stage_0002_item_00",
    "fragment_stage_0210_item_00",
    "fragment_stage_0400_item_00",
    "fragment_stage_0100_item_00",
    "fragment_stage_0220_item_00",
    "fragment_stage_0600_item_00",
    "fragment_stage_0230_item_00",
    "fragment_stage_0001_item_00",
    "fragment_stage_0350_item_00",
    "fragment_stage_0240_item_00",
    "fragment_stage_0200_item_00",
    "fragment_stage_0300_item_00",
    "fragment_stage_0325_item_00",
)
HANDOFF_MEMBERS = ("bos01_trystartbos02", "ontimer")
UNBOUND_NOTE_SCRIPTS = (
    "BoS01NoteScript",
    "BoS01NoteReadScript",
    "BoS01ReadNoteScript",
)


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
        for kind, name, start, end in _iter_top_level_papyrus_members(lines)
        if kind in {"function", "event"} and name == member_name.casefold()
    )
    return "\n".join(lines[start : end + 1])


def _merged_source(script_name: str) -> str:
    source_path = SOURCE_ROOT / _script_relative_path(script_name, ".psc")
    patch = _script_patch_source(script_name)
    assert source_path.is_file(), source_path
    assert patch is not None
    return _merge_script_method_patches(
        source_path.read_text(encoding="utf-8"), patch
    )


def _assert_in_order(source: str, *tokens: str) -> None:
    positions = [source.index(token) for token in tokens]
    assert positions == sorted(positions)


def test_fragment_patch_covers_the_exact_source_vmad_contract_once() -> None:
    patch = _script_patch_source(FRAGMENT_SCRIPT)
    assert patch is not None
    members = _member_names(patch)

    assert Counter(members) == Counter(BOUND_FRAGMENT_MEMBERS + HANDOFF_MEMBERS)
    assert len(members) == 16
    assert not any(
        line.strip().casefold().startswith(("scriptname ", "state "))
        for line in patch.splitlines()
    )
    assert not any(
        " property " in f" {line.strip().casefold()} "
        for line in patch.splitlines()
    )


@pytest.mark.parametrize("script_name", (FRAGMENT_SCRIPT, CERTIFICATE_SCRIPT))
def test_production_merges_are_unique_and_idempotent(script_name: str) -> None:
    patch = _script_patch_source(script_name)
    assert patch is not None
    merged = _merged_source(script_name)
    merged_members = _member_names(merged)

    for member in _member_names(patch):
        assert merged_members.count(member) == 1
        assert _member_body(merged, member) == _member_body(patch, member)
    assert merged.casefold().count("scriptname ") == 1
    assert _merge_script_method_patches(merged, patch) == merged


def test_unbound_note_skeletons_remain_unpatched() -> None:
    for script_name in UNBOUND_NOTE_SCRIPTS:
        assert _script_patch_source(script_name) is None


def test_objective_and_branch_transitions_match_the_quest_contract() -> None:
    patch = _script_patch_source(FRAGMENT_SCRIPT)
    assert patch is not None
    contracts = {
        1: "SetObjectiveDisplayed(100, True)",
        100: "SetObjectiveDisplayed(100, True)",
        200: "SetObjectiveDisplayed(200, True)",
        210: "SetObjectiveDisplayed(210, True)",
        220: "SetObjectiveDisplayed(220, True)",
        230: "SetObjectiveCompleted(220, True)",
        240: "SetObjectiveCompleted(210, True)",
        300: "SetObjectiveDisplayed(300, True)",
        325: "SetObjectiveDisplayed(300, True)",
        350: "SetObjectiveDisplayed(400, True)",
        400: "SetObjectiveDisplayed(500, True)",
        500: "SetObjectiveCompleted(500, True)",
    }
    for stage, token in contracts.items():
        assert token in _member_body(
            patch, f"fragment_stage_{stage:04d}_item_00"
        )

    powered = _member_body(patch, "fragment_stage_0400_item_00")
    assert "pAlleghenyAsylumPoweredUp.SetValue(1.0)" in powered


def test_completion_rewards_checkpoint_then_hands_off_through_story_manager() -> None:
    patch = _script_patch_source(FRAGMENT_SCRIPT)
    assert patch is not None
    finish = _member_body(patch, "fragment_stage_0500_item_00")
    handoff = _member_body(patch, "bos01_trystartbos02")
    retry = _member_body(patch, "ontimer")

    _assert_in_order(
        finish,
        "SetObjectiveCompleted(500, True)",
        "playerRef.SetValue(pBoS01CompletedAV, 1.0)",
        "playerRef.SetValue(pBoS01_CheckpointValue, 1.0)",
        "CompleteQuest()",
        "BoS01_TryStartBoS02()",
        "SetStage(600)",
    )
    assert "playerRef.GetValue(pBoS01_CheckpointValue) < 1.0" in finish
    assert finish.count("pCheckpointMessage.Show()") == 1
    assert "StartTimer(5.0, 500)" in finish
    assert "pBoS02.IsRunning() || pBoS02.IsCompleted()" in handoff
    assert "pBoS02_QuestStartKeyword.SendStoryEventAndWait" in handoff
    assert "accepted ||" in handoff
    assert "playerRef.SetValue(pBoS02StartedAV, 1.0)" in handoff
    assert "aiTimerID != 500" in retry
    assert "GetStageDone(600)" in retry
    assert "BoS01_TryStartBoS02()" in retry
    assert "StartTimer(5.0, 500)" in retry
    assert ".Start()" not in finish
    assert ".Start()" not in handoff

    cleanup = _member_body(patch, "fragment_stage_0600_item_00")
    _assert_in_order(cleanup, "Alias_Player.Clear()", "Stop()")


def test_certificate_topic_grants_only_one_bound_certificate() -> None:
    patch = _script_patch_source(CERTIFICATE_SCRIPT)
    assert patch is not None
    on_begin = _member_body(patch, "onbegin")

    assert "Event OnBegin(ObjectReference akSpeakerRef, Bool abHasBeenSaid)" in on_begin
    assert "playerRef.GetItemCount(pBoS02SoldierCertificate) == 0" in on_begin
    assert on_begin.count("playerRef.AddItem(pBoS02SoldierCertificate, 1, False)") == 1


@pytest.mark.parametrize("script_name", (FRAGMENT_SCRIPT, CERTIFICATE_SCRIPT))
def test_full_production_merge_native_compiles_for_fo4(script_name: str) -> None:
    base_source = _fo4_base_source()
    assert base_source is not None, "FO4 base Papyrus sources unavailable"

    result = compile_psc(
        _merged_source(script_name),
        imports=[str(SOURCE_ROOT), str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=str(_script_relative_path(script_name, ".psc")),
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None
