"""LC060_Whitespring01 "Unruly Guests": the golfer-ghoul kill counter."""
from __future__ import annotations

from bacup_lib.workflows.unified import _iter_top_level_papyrus_members, _script_patch_source


def _patch() -> str:
    patch = _script_patch_source("WS01script")
    assert patch is not None
    return patch


def _member(patch: str, name: str) -> str:
    lines = patch.splitlines()
    start, end = next((s, e) for _kind, member, s, e in _iter_top_level_papyrus_members(lines) if member.lower() == name.lower())
    return "\n".join(lines[start : end + 1])


def test_golfer_kills_are_not_filtered_by_the_robots_faction():
    # FO76 WhitespringFaction (1FC69F) holds only the resort's robots, never a ghoul.
    assert "IsInFaction(WhitespringFaction)" not in _patch()


def test_the_objective_counter_is_published():
    body = _member(_patch(), "UpdateKillCount")
    assert 'SetVariable("NumberKilled", playerRef.GetValue(WS01status))' in body
    assert 'SetVariable("KillsNeeded", WS01MaxKills.GetValue())' in body
    assert "(Self as Quest) as B21:QuestVariables" in body
    assert "UpdateKillCount(playerRef)" in _member(_patch(), "Actor.OnKill")


def test_the_completion_stage_completes_the_quest():
    body = _member(_patch(), "OnStageSet")
    for call in ("SetObjectiveCompleted(ObjectiveToDisplay)", "CompleteQuest()", "Stop()"):
        assert call in body
