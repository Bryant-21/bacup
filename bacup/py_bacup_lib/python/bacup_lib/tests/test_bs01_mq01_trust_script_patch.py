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
SCRIPT_NAME = "Fragments:Quests:QF_BS01_NewBrothers_005C70CC"
STAGE_MEMBERS = {
    "fragment_stage_0100_item_00",
    "fragment_stage_0110_item_00",
    "fragment_stage_0120_item_00",
    "fragment_stage_0150_item_00",
    "fragment_stage_0160_item_00",
    "fragment_stage_0170_item_00",
    "fragment_stage_0180_item_00",
    "fragment_stage_0200_item_00",
    "fragment_stage_0210_item_00",
    "fragment_stage_0220_item_00",
    "fragment_stage_0230_item_00",
    "fragment_stage_0240_item_00",
    "fragment_stage_0300_item_00",
    "fragment_stage_0400_item_00",
    "fragment_stage_0500_item_00",
    "fragment_stage_9000_item_00",
}
TIMER_MEMBER = "ontimer"
HANDOFF_MEMBER = "bs01_trystartinvention"


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
    source_path = SOURCE_ROOT / _script_relative_path(SCRIPT_NAME, ".psc")
    patch = _script_patch_source(SCRIPT_NAME)
    assert source_path.is_file(), source_path
    assert patch is not None
    return _merge_script_method_patches(
        source_path.read_text(encoding="utf-8"), patch
    )


def test_patch_merges_all_exact_vmad_members_once_without_duplicates():
    patch = _script_patch_source(SCRIPT_NAME)
    assert patch is not None
    patch_members = _member_names(patch)

    expected_members = STAGE_MEMBERS | {TIMER_MEMBER, HANDOFF_MEMBER}
    assert set(patch_members) == expected_members
    assert len(STAGE_MEMBERS) == 16
    assert len(patch_members) == len(expected_members) == 18
    assert all(
        patch_members.count(member_name) == 1 for member_name in expected_members
    )
    assert not any(
        line.strip().lower().startswith(("scriptname ", "state "))
        for line in patch.splitlines()
    )
    assert not any(
        " property " in f" {line.strip().lower()} " for line in patch.splitlines()
    )

    merged = _merged_production_source()
    merged_members = _member_names(merged)
    assert all(
        merged_members.count(member_name) == 1 for member_name in expected_members
    )
    assert _merge_script_method_patches(merged, patch) == merged
    assert merged.lower().count("scriptname ") == 1


def test_stage_graph_preserves_intro_petitioner_and_access_handoffs():
    patch = _script_patch_source(SCRIPT_NAME)
    assert patch is not None

    stage_100 = _member_body(patch, "fragment_stage_0100_item_00")
    assert "Alias_Player.ForceRefIfEmpty(playerRef)" in stage_100
    assert "Alias_Player_KeywordRef.ForceRefIfEmpty(playerRef)" in stage_100
    assert "playerRef.SetValue(BS01_RussellDorsey_DialogueGate_AV, 1.0)" in stage_100
    assert "SetObjectiveDisplayed(100)" in stage_100

    stage_120 = _member_body(patch, "fragment_stage_0120_item_00")
    assert "Alias_Actors_AtlasInitiates_Keyword.AddRef(initiateRef)" in stage_120
    assert "shinRef.MoveTo(shinMarker)" in stage_120
    assert "rahmaniRef.MoveTo(rahmaniMarker)" in stage_120

    stage_150 = _member_body(patch, "fragment_stage_0150_item_00")
    assert stage_150.index("SetObjectiveDisplayed(200, False)") < stage_150.index(
        "SetObjectiveDisplayed(210)"
    )
    assert "BS01_MQ01_Trust_IntroScene.Start()" in _member_body(
        patch, "fragment_stage_0160_item_00"
    )
    stage_170 = _member_body(patch, "fragment_stage_0170_item_00")
    assert stage_170.index("SetObjectiveCompleted(210)") < stage_170.index(
        "SetObjectiveDisplayed(200)"
    )

    for branch_stage in (210, 220, 230, 240):
        body = _member_body(
            patch, f"fragment_stage_{branch_stage:04d}_item_00"
        )
        for required_stage in (210, 220, 230, 240):
            assert f"IsStageDone({required_stage})" in body
        assert body.count("SetStage(300)") == 1
        assert "!IsStageDone(300)" in body

    assert "SetObjectiveCompleted(300)" in _member_body(
        patch, "fragment_stage_0300_item_00"
    )
    assert "SetObjectiveDisplayed(400)" in _member_body(
        patch, "fragment_stage_0300_item_00"
    )
    assert "SetObjectiveCompleted(400)" in _member_body(
        patch, "fragment_stage_0400_item_00"
    )
    assert "SetObjectiveDisplayed(500)" in _member_body(
        patch, "fragment_stage_0400_item_00"
    )

    stage_500 = _member_body(patch, "fragment_stage_0500_item_00")
    assert "laserGrid.Disable()" in stage_500
    assert stage_500.index("bankDoor.Lock(False)") < stage_500.index(
        "bankDoor.SetOpen(True)"
    )
    assert stage_500.count("SetStage(9000)") == 1
    assert "If !IsStageDone(9000)" in stage_500
    assert stage_500.index("bankDoor.SetOpen(True)") < stage_500.index(
        "If !IsStageDone(9000)"
    )
    assert stage_500.index("If !IsStageDone(9000)") < stage_500.index(
        "SetStage(9000)"
    )

    for generic_stage in (120, 150, 160, 180, 500):
        assert f"SetStage({generic_stage})" not in patch


def test_terminal_stage_dispatches_guarded_story_event_before_stopping():
    patch = _script_patch_source(SCRIPT_NAME)
    assert patch is not None
    terminal = _member_body(patch, "fragment_stage_9000_item_00")
    assert (
        "If BS01_TryStartInvention()\n        Stop()\n    Else\n        StartTimer(5.0, 9000)"
        in terminal
    )
    assert "CompleteQuest()" in terminal
    assert terminal.index("CompleteQuest()") < terminal.index(
        "BS01_TryStartInvention()"
    )
    assert terminal.count("Stop()") == 1
    assert terminal.count("StartTimer(5.0, 9000)") == 1
    assert ".Start()" not in terminal


def test_terminal_timer_gates_retries_and_stops_only_after_acceptance():
    patch = _script_patch_source(SCRIPT_NAME)
    assert patch is not None
    timer = _member_body(patch, TIMER_MEMBER)
    assert "If aiTimerID != 9000 || !IsStageDone(9000)" in timer
    assert timer.index("If aiTimerID != 9000 || !IsStageDone(9000)") < timer.index(
        "Return"
    )
    assert (
        "If BS01_TryStartInvention()\n        Stop()\n    Else\n        StartTimer(5.0, 9000)"
        in timer
    )
    assert timer.count("Stop()") == 1
    assert timer.count("StartTimer(5.0, 9000)") == 1
    assert ".Start()" not in timer


def test_handoff_helper_accepts_existing_successor_or_story_event():
    patch = _script_patch_source(SCRIPT_NAME)
    assert patch is not None
    helper = _member_body(patch, HANDOFF_MEMBER)

    assert (
        'Game.GetFormFromFile(0x005B79EB, "SeventySix.esm") as Quest'
        in helper
    )
    assert "inventionQuest.IsRunning() || inventionQuest.IsCompleted()" in helper
    assert "BS01_Invention_QuestStartKeyword == None" in helper
    assert (
        "BS01_Invention_QuestStartKeyword."
        "SendStoryEventAndWait(None, playerRef, playerRef)"
        in helper
    )
    assert "Return accepted ||" in helper
    assert ".Start()" not in helper


def test_full_production_merge_native_compiles_for_fo4():
    base_source = _fo4_base_source()
    assert base_source is not None, "FO4 base Papyrus sources unavailable"

    result = compile_psc(
        _merged_production_source(),
        imports=[str(SOURCE_ROOT), str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=str(_script_relative_path(SCRIPT_NAME, ".psc")),
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None
