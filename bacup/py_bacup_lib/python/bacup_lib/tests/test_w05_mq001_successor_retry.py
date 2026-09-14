from __future__ import annotations

import json
from pathlib import Path

import pytest

from bacup_lib.workflows.unified import (
    _iter_top_level_papyrus_members,
    _merge_script_method_patches,
    _script_patch_source,
)
from creation_lib.pex import decompile_pex
from creation_lib.pex.native_runtime import compile_psc

from bacup_lib.tests.test_terminal_fragment_script_patches import _fo4_base_source

REPO_ROOT = Path(__file__).resolve().parents[5]
DEPLOYED_PEX = (
    REPO_ROOT
    / "mods"
    / "SeventySix"
    / "data"
    / "Scripts"
    / "fragments"
    / "quests"
    / "qf_w05_mq_001p_wayward_00405e14.pex"
)
SCRIPT_NAME = "Fragments:Quests:QF_W05_MQ_001P_Wayward_00405E14"
STAGE_FLAGS_FIXTURE = (
    REPO_ROOT
    / "bacup"
    / "py_bacup_lib"
    / "python"
    / "bacup_lib"
    / "tests"
    / "fixtures"
    / "w05_mq001_stage_flags.json"
)


def _member_body(source: str, member_name: str) -> str:
    start, end = next(
        (start, end)
        for kind, name, start, end in _iter_top_level_papyrus_members(source.splitlines())
        if kind in {"function", "event"} and name == member_name.lower()
    )
    return "\n".join(source.splitlines()[start : end + 1])


def test_wayward_completion_is_independent_of_radical_story_event_acceptance():
    patch = _script_patch_source(SCRIPT_NAME)
    assert patch is not None

    for stage in (900, 905):
        body = _member_body(patch, f"fragment_stage_{stage:04d}_item_00")
        assert body.index("SetStage(9000)") < body.index("AttemptRadicalHandoff()")
        assert "SendStoryEventAndWait" not in body
        assert ".Start()" not in body

    completion = _member_body(patch, "fragment_stage_9000_item_00")
    assert "CompleteQuest()" in completion


def test_wayward_handoff_retries_only_after_completion_and_never_direct_starts():
    patch = _script_patch_source(SCRIPT_NAME)
    assert patch is not None

    attempt = _member_body(patch, "attemptradicalhandoff")
    state_ack = "accepted = radicalQuest.IsRunning() || radicalQuest.IsCompleted()"
    send = (
        "W05_MQ_002P_Radical_QuestStartKeyword."
        "SendStoryEventAndWait(None, playerRef, playerRef)"
    )
    assert 'Game.GetFormFromFile(0x0040F5BE, "SeventySix.esm") as Quest' in attempt
    assert attempt.count(state_ack) == 2
    assert attempt.index(state_ack) < attempt.index(send) < attempt.rindex(state_ack)
    assert attempt.count(send) == 1
    assert f"accepted = {send}" not in attempt
    assert "If !accepted && IsStageDone(9000)" in attempt
    assert "StartTimer(5.0, 9000)" in attempt
    assert ".Start()" not in attempt
    assert "SetStage(9000)" not in attempt

    timer = _member_body(patch, "ontimer")
    assert "aiTimerID == 9000 && IsStageDone(9000)" in timer
    assert timer.count("AttemptRadicalHandoff()") == 1
    assert "SendStoryEventAndWait" not in timer


def test_wayward_completion_keeps_retry_alive_until_successor_accepts():
    evidence = json.loads(STAGE_FLAGS_FIXTURE.read_text(encoding="utf-8"))
    expected_flags = {"9000": ["CompleteQuest"], "10000": []}
    assert evidence["source"]["editor_id"] == "W05_MQ_001P_Wayward"
    assert evidence["live"]["editor_id"] == "W05_MQ_001P_Wayward"
    assert evidence["source"]["stages"] == expected_flags
    assert evidence["live"]["stages"] == expected_flags
    assert all(
        "ShutDown" not in flags
        for record in evidence.values()
        for flags in record["stages"].values()
    )

    patch = _script_patch_source(SCRIPT_NAME)
    assert patch is not None
    retry_members = [
        _member_body(patch, "fragment_stage_0900_item_00"),
        _member_body(patch, "fragment_stage_0905_item_00"),
        _member_body(patch, "fragment_stage_9000_item_00"),
        _member_body(patch, "attemptradicalhandoff"),
        _member_body(patch, "ontimer"),
    ]
    assert "CompleteQuest()" in retry_members[2]
    assert "StartTimer(5.0, 9000)" in retry_members[3]
    assert "AttemptRadicalHandoff()" in retry_members[4]
    assert all("Stop()" not in member for member in retry_members[:3])
    assert "Stop()" not in retry_members[4]

    attempt = retry_members[3]
    retry_guard = "If !accepted && IsStageDone(9000)"
    shutdown_guard = "ElseIf accepted && IsStageDone(9000)"
    retry = attempt.split(retry_guard, 1)[1].split(shutdown_guard, 1)[0]
    shutdown = attempt.split(shutdown_guard, 1)[1].split("EndIf", 1)[0]
    assert "StartTimer(5.0, 9000)" in retry
    assert "Stop()" not in retry
    assert "CancelTimer" not in retry
    assert shutdown.index("CancelTimer(9000)") < shutdown.index("Stop()")
    state_ack = "accepted = radicalQuest.IsRunning() || radicalQuest.IsCompleted()"
    assert attempt.rindex(state_ack) < attempt.index(shutdown_guard)
    assert attempt.count("Stop()") == 1
    assert "StartTimer" not in shutdown


def test_wayward_successor_retry_merges_once_and_compiles_for_fo4():
    base_source = _fo4_base_source()
    if base_source is None:
        pytest.skip("FO4 base Papyrus sources unavailable")
    if not DEPLOYED_PEX.is_file():
        pytest.skip(f"deployed production PEX unavailable: {DEPLOYED_PEX}")

    patch = _script_patch_source(SCRIPT_NAME)
    assert patch is not None
    merged = _merge_script_method_patches(
        decompile_pex(DEPLOYED_PEX, fo4_api_compat=True), patch
    )
    for member in (
        "fragment_stage_0900_item_00",
        "fragment_stage_0905_item_00",
        "attemptradicalhandoff",
        "ontimer",
    ):
        assert [
            name
            for _kind, name, _start, _end in _iter_top_level_papyrus_members(
                merged.splitlines()
            )
            if name == member
        ] == [member]

    result = compile_psc(
        merged,
        imports=[str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path="QF_W05_MQ_001P_Wayward_00405E14.psc",
    )
    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None
