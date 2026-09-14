from __future__ import annotations

import os
from pathlib import Path

import pytest

from bacup_lib.workflows.unified import (
    _augment_fo76_to_fo4_script_skeleton,
    _iter_top_level_papyrus_members,
    _merge_script_method_patches,
    _script_patch_source,
    _script_relative_path,
)
from creation_lib.pex.native_runtime import compile_psc


REPO_ROOT = Path(__file__).resolve().parents[5]
SOURCE_ROOT = REPO_ROOT / "mods" / "SeventySix" / "Scripts" / "Source" / "User"
CONTRACT = (
    REPO_ROOT
    / "bacup"
    / "docs"
    / "stub_restoration"
    / "contracts"
    / "wastelanders-ally-quest-runtime-2026-09-03.md"
)

PATCH_CASES = {
    "CompanionScript": {
        "oninit",
        "onload",
        "initializesingleplayerstate",
        "synchronizeactorvalues",
        "startdailyquest",
        "findradiantquestdataindex",
        "startradiantquestbyindex",
        "getcurrentradiantfirstobjective",
        "getcurrentradiantsecondobjective",
        "getcurrentradiantquestcooldown",
        "getcurrentradiantquest",
        "setrqacceptancestage",
        "trystartoutroquest",
    },
    "CompanionVisitorScript": {
        "onload",
        "onunload",
        "ondeath",
        "ontimer",
        "checkandprocessvisitors",
        "visitordatummatches",
        "hasspawnedvisitor",
        "spawnvisitor",
        "cleanupexpiredvisitors",
        "findvisitordatum",
    },
    "CompanionConversationScript": {"onload", "onunload", "ondeath", "ontimer"},
    "COMP_RQ_Restore_Fetch_Script": {"restorequestcustom", "clearobjectaliases"},
    "Fragments:Quests:QF_COMP_Visitor_0055FD53": {
        "fragment_stage_0100_item_00",
        "fragment_stage_0200_item_00",
        "fragment_stage_8000_item_00",
        "fragment_stage_9000_item_00",
    },
}


def _fo4_base_source() -> Path | None:
    candidates: list[Path] = []
    configured = os.environ.get("FO4_DIR", "").strip().strip('"')
    if configured:
        candidates.append(Path(configured))
    env_path = REPO_ROOT / ".env"
    if env_path.is_file():
        for line in env_path.read_text(encoding="utf-8").splitlines():
            if line.startswith("FO4_DIR="):
                value = line.split("=", 1)[1].strip().strip('"')
                if value:
                    candidates.append(Path(value))
                break
    for game_root in candidates:
        source_root = game_root / "Data" / "Scripts" / "Source" / "Base"
        if source_root.is_dir():
            return source_root
    return None


def _member_names(source: str) -> list[str]:
    return [
        name
        for _kind, name, _start, _end in _iter_top_level_papyrus_members(
            source.splitlines()
        )
    ]


def _merged_source(script_name: str) -> str:
    source_path = SOURCE_ROOT / _script_relative_path(script_name, ".psc")
    patch = _script_patch_source(script_name)
    assert source_path.is_file(), source_path
    assert patch is not None
    skeleton = _augment_fo76_to_fo4_script_skeleton(
        script_name, source_path.read_text(encoding="utf-8")
    )
    return _merge_script_method_patches(skeleton, patch)


@pytest.mark.parametrize(("script_name", "expected_members"), PATCH_CASES.items())
def test_wastelanders_ally_patch_merges_once(
    script_name: str, expected_members: set[str]
) -> None:
    patch = _script_patch_source(script_name)
    assert patch is not None
    assert not any(
        line.strip().lower().startswith("scriptname ") for line in patch.splitlines()
    )
    assert set(_member_names(patch)) == expected_members

    merged = _merged_source(script_name)
    assert merged.lower().count("scriptname ") == 1
    for member in expected_members:
        assert _member_names(merged).count(member) == 1


def test_radiant_start_and_acceptance_require_a_running_story_quest() -> None:
    companion = _merged_source("CompanionScript")
    start_body = companion.split("Bool Function StartRadiantQuestByIndex", 1)[1].split(
        "EndFunction", 1
    )[0]
    accept_body = companion.split("Function SetRQAcceptanceStage()", 1)[1].split(
        "EndFunction", 1
    )[0]

    assert "SendStoryEventAndWait" in start_body
    assert "_COMP_RQ_Ins && _COMP_RQ_Ins.IsRunning()" in start_body
    assert "started = False" in start_body
    assert "_COMP_RQ_Ins = None" in start_body
    assert "fClearedPlayerRadiantQuestDataIndex" in start_body
    assert ".Start()" not in start_body
    failed_start = start_body.split("If !started", 1)[1]
    assert "_COMP_RQ_Ins = None" in failed_start

    running_guard = "_COMP_RQ_Ins && _COMP_RQ_Ins.IsRunning()"
    assert running_guard in accept_body
    assert accept_body.find(running_guard) < accept_body.find("_COMP_RQ_Ins.SetStage")


def test_fetch_restore_bounds_stale_known_object_indexes() -> None:
    restore = _merged_source("COMP_RQ_Restore_Fetch_Script")
    size = "Int objectCount = COMP_RQ_Fetch_KnownObjectsList.GetSize()"
    bounds = "If objectIndex < 0 || objectIndex >= objectCount"
    get = "Form objectBase = COMP_RQ_Fetch_KnownObjectsList.GetAt(objectIndex)"

    assert restore.find(size) < restore.find(bounds) < restore.find(get)
    assert "If objectCount <= 0" in restore
    assert "objectIndex = 0" in restore


def test_dead_allies_do_not_rearm_visitor_or_conversation_timers() -> None:
    visitor = _merged_source("CompanionVisitorScript")
    conversation = _merged_source("CompanionConversationScript")

    visitor_load = visitor.split("Event OnLoad()", 1)[1].split("EndEvent", 1)[0]
    conversation_load = conversation.split("Event OnLoad()", 1)[1].split(
        "EndEvent", 1
    )[0]
    for body in (visitor_load, conversation_load):
        assert "If IsDead()" in body
        assert body.find("If IsDead()") < body.find("StartTimer")
        assert "Return" in body.split("StartTimer", 1)[0]
    assert "CleanupExpiredVisitors(True)" in visitor_load


def test_visitor_stage_machine_is_bounded_and_idempotent() -> None:
    visitor = _merged_source("Fragments:Quests:QF_COMP_Visitor_0055FD53")

    assert "!IsObjectiveDisplayed(100)" in visitor
    assert "!IsObjectiveCompleted(100)" in visitor
    assert "!IsObjectiveFailed(100)" in visitor
    assert visitor.count("If !IsStageDone(9000)") == 2
    assert visitor.count("SetStage(9000)") == 2
    assert "If !IsStopped()" in visitor
    assert "Stop()" in visitor
    assert "AddItem" not in visitor
    assert "AddPerk" not in visitor


def test_unproven_radiant_quest_fragments_remain_unpatched() -> None:
    contract = CONTRACT.read_text(encoding="utf-8")

    for quest_id in ("0054F1A4", "0056FB76", "005727AD", "0055FD53"):
        assert quest_id in contract
    assert "No topic fragment is added" in contract
    assert "Restore dispatch therefore remains evidence-blocked" in contract
    assert _script_patch_source("Fragments:Quests:QF_COMP_RQ_Fetch_0054F1A4") is None
    assert _script_patch_source("Fragments:Quests:QF_COMP_RQ_Kill_0056FB76") is None
    assert _script_patch_source("Fragments:Quests:QF_COMP_RQ_Rescue_005727AD") is None


def test_idle_timer_remains_unpatched_without_a_producer_event() -> None:
    contract = CONTRACT.read_text(encoding="utf-8")

    assert "`COMP_IdleTimerScript` remains a record dependency" in contract
    assert "the evidence does not select either event" in contract
    assert _script_patch_source("COMP_IdleTimerScript") is None


@pytest.mark.parametrize("script_name", PATCH_CASES)
def test_wastelanders_ally_full_merged_source_compiles_for_fo4(
    script_name: str, tmp_path: Path
) -> None:
    base_source = _fo4_base_source()
    if base_source is None:
        pytest.skip("FO4 base Papyrus sources unavailable")

    merged_dependency_root = tmp_path / "merged"
    merged_dependency_root.mkdir()
    (merged_dependency_root / "CompanionScript.psc").write_text(
        _merged_source("CompanionScript"), encoding="utf-8"
    )
    result = compile_psc(
        _merged_source(script_name),
        imports=[str(merged_dependency_root), str(SOURCE_ROOT), str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=str(_script_relative_path(script_name, ".psc")),
    )
    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None
