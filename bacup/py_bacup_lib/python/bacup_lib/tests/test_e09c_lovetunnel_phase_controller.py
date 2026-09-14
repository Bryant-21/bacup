"""Tunnel of Love (65B0A8) phase controller.

`QF_E09C_LoveTunnel_0065B0A8` consumes stages 190 / 290 / 390 / 500, which the
sibling `E09C_QuestScript` owns but converts without bodies, so the event can
never finish. The tests pin that each phase completes on a world observable, not
a timer or activation: a patch granting the stage outright would still compile
and bind while faking completion.
"""

from __future__ import annotations

from pathlib import Path

from bacup_lib.workflows.unified import (
    _iter_top_level_papyrus_members,
    _merge_script_method_patches,
    _script_patch_source,
    _script_relative_path,
)


REPO_ROOT = Path(__file__).resolve().parents[5]
SOURCE_ROOT = REPO_ROOT / "mods" / "SeventySix" / "Scripts" / "Source" / "User"
SCRIPT_NAME = "E09C_QuestScript"

EXPECTED_MEMBERS = {
    "phasepolltimerid",
    "onquestinit",
    "onstageset",
    "ontimer",
    "armphasepoll",
    "evaluatephases",
    "spawnphasecontent",
    "spawnweddingfood",
    "countenabledaliasrefs",
    "countrepairedtracks",
    "countcollectedrobotparts",
    "playerattendingwedding",
    "getstagedoneelapsed",
    "runtimedcallouts",
}


def _member_names(source: str) -> list[str]:
    return [
        name
        for _kind, name, _start, _end in _iter_top_level_papyrus_members(
            source.splitlines()
        )
    ]


def _merged() -> str:
    source_path = SOURCE_ROOT / _script_relative_path(SCRIPT_NAME, ".psc")
    patch = _script_patch_source(SCRIPT_NAME)
    assert source_path.is_file(), source_path
    assert patch is not None
    return _merge_script_method_patches(
        source_path.read_text(encoding="utf-8"), patch
    )


def test_patch_is_member_only_and_complete():
    patch = _script_patch_source(SCRIPT_NAME)
    assert patch is not None
    assert set(_member_names(patch)) == EXPECTED_MEMBERS
    # The merger keeps declarations from the skeleton; a patch that redeclares
    # them silently wins the merge and changes the bound property set.
    for banned in ("Scriptname", "Extends", "Struct ", "Property "):
        assert banned not in patch


def test_merge_is_idempotent_and_adds_no_duplicates():
    once = _merged()
    twice = _merge_script_method_patches(once, _script_patch_source(SCRIPT_NAME))
    assert once == twice
    names = _member_names(once)
    assert len(names) == len(set(names))


def test_phase_boundaries_come_from_bound_properties_not_literals():
    """No phase boundary may be a hard-coded stage number.

    Every boundary is a bound property on the live record (iDecoStartStage 110,
    iDecoStageToSetOnComplete 190, iTrackStartStage 200 -> 290, iHandyStartStage
    300 -> 390, iWeddingStartStage 400 -> 500). Hard-coding one would survive a
    record change silently.
    """
    patch = _script_patch_source(SCRIPT_NAME)
    assert patch is not None
    for literal in ("SetStage(190)", "SetStage(290)", "SetStage(390)", "SetStage(500)"):
        assert literal not in patch
    for bound in (
        "iDecoStartStage",
        "iDecoStageToSetOnComplete",
        "iTrackStartStage",
        "iTrackStageToSetOnComplete",
        "iHandyStartStage",
        "iHandyStageToSetOnComplete",
        "iWeddingStartStage",
        "iWeddingStageToSetOnComplete",
    ):
        assert bound in patch


def test_each_phase_is_gated_on_a_world_observable():
    patch = _script_patch_source(SCRIPT_NAME)
    assert patch is not None
    # decorations -> enabled decoration references
    assert "CountEnabledAliasRefs(DecorationEnablers) >= iDecoObjectiveMax" in patch
    # tracks -> repaired track references
    assert "CountRepairedTracks() >= iTracksObjectiveMax" in patch
    # fake Miss Handy -> the robot parts actually held by the player
    assert "CountCollectedRobotParts() >= iFakeHandyObjectiveMax" in patch
    # wedding -> attendance beside Mr Lovely
    assert "PlayerAttendingWedding()" in patch


def test_poll_stops_once_the_event_is_over():
    """A finished or failed run must stop re-arming the poll timer."""
    patch = _script_patch_source(SCRIPT_NAME)
    assert patch is not None
    assert "If IsStageDone(iWeddingStageToSetOnComplete) || IsCompleted() || IsStopped()" in patch
    assert "If EvaluatePhases()" in patch


def test_wedding_spread_cannot_be_placed_twice():
    patch = _script_patch_source(SCRIPT_NAME)
    assert patch is not None
    assert "If cleanupDone || WeddingFoodSpawns == None" in patch
    assert "cleanupDone = True" in patch
