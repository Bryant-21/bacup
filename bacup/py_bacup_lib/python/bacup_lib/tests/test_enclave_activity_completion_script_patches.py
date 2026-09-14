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

EVENT_SCRIPT = "EnclaveEventQuestScript"
OFFICER_SCRIPT = "EN05_MQ_QuestScript"
SUCCESS_FRAGMENTS = {
    "Fragments:Quests:QF_ENs02_Blast_0000D053": 150,
    "Fragments:Quests:QF_ENz01_Above_00003AF2": 150,
    "Fragments:Quests:QF_ENz04_Bots_0000D02C": 190,
}
PATCH_MEMBERS = {
    EVENT_SCRIPT: {"enevent_recordcompletion"},
    **{
        script_name: {f"fragment_stage_{stage:04d}_item_00"}
        for script_name, stage in SUCCESS_FRAGMENTS.items()
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


def _merged_source(script_name: str) -> str:
    source_path = SOURCE_ROOT / _script_relative_path(script_name, ".psc")
    patch = _script_patch_source(script_name)
    assert source_path.is_file(), source_path
    assert patch is not None
    return _merge_script_method_patches(source_path.read_text(encoding="utf-8"), patch)


@pytest.mark.parametrize(("script_name", "members"), PATCH_MEMBERS.items())
def test_enclave_activity_patches_are_member_only_and_exact(
    script_name: str, members: set[str]
) -> None:
    patch = _script_patch_source(script_name)

    assert patch is not None
    assert set(_member_names(patch)) == members
    assert len(_member_names(patch)) == len(members)
    assert not any(
        line.strip().casefold().startswith(("scriptname ", "state ", "extends "))
        for line in patch.splitlines()
    )
    assert not any(
        " property " in f" {line.strip().casefold()} " for line in patch.splitlines()
    )


@pytest.mark.parametrize(("script_name", "members"), PATCH_MEMBERS.items())
def test_enclave_activity_merges_are_unique_and_idempotent(
    script_name: str, members: set[str]
) -> None:
    patch = _script_patch_source(script_name)
    assert patch is not None
    merged = _merged_source(script_name)
    merged_members = _member_names(merged)

    for member in members:
        assert merged_members.count(member) == 1
        assert _member_body(merged, member) == _member_body(patch, member)
    assert _merge_script_method_patches(merged, patch) == merged


def test_event_completion_tracks_once_then_uses_officer_controller_contract() -> None:
    patch = _script_patch_source(EVENT_SCRIPT)
    assert patch is not None
    body = _member_body(patch, "enevent_recordcompletion")

    tracking = (
        "player.SetValue(CompletionTrackingValue, "
        "player.GetValue(CompletionTrackingValue) + 1.0)"
    )
    award = "officer.EN05MQ_AddCommendations(iCommendationValue)"
    assert "Actor player = Game.GetPlayer()" in body
    assert "CompletionTrackingValue != None" in body
    assert tracking in body
    assert "EN05_MQ_Officer == None || !EN05_MQ_Officer.IsRunning()" in body
    assert "EN05_MQ_Officer as EN05_MQ_QuestScript" in body
    assert "!officer.IsStageDone(officer.iPlayerRegisteredStage)" in body
    assert "officer.IsStageDone(officer.iCommendationsCompletedStage)" in body
    assert "supportStage = officer.iCompletedBunkerPromotion" in body
    assert "officer.SetObjectiveCompleted(supportStage)" in body
    assert "officer.SetStage(supportStage)" in body
    assert body.count(award) == 1
    assert body.index(tracking) < body.index(award)
    assert "SendStoryEvent" not in body
    assert "CurrentPlayers" not in body


@pytest.mark.parametrize(("script_name", "stage"), SUCCESS_FRAGMENTS.items())
def test_success_stage_delegates_once_to_attached_event_script(
    script_name: str, stage: int
) -> None:
    patch = _script_patch_source(script_name)
    assert patch is not None
    body = _member_body(patch, f"fragment_stage_{stage:04d}_item_00")

    assert "(Self as Quest) as EnclaveEventQuestScript" in body
    assert "eventQuest != None" in body
    assert body.count("eventQuest.ENEvent_RecordCompletion()") == 1
    assert "EN05_MQ_Officer" not in body


@pytest.mark.parametrize("script_name", PATCH_MEMBERS)
def test_enclave_activity_production_merges_compile(
    script_name: str, tmp_path: Path
) -> None:
    base_source = _fo4_base_source()
    assert base_source is not None, "FO4 base Papyrus sources unavailable"

    dependency_names = {OFFICER_SCRIPT, *PATCH_MEMBERS}
    for dependency_name in dependency_names:
        dependency_path = tmp_path / _script_relative_path(dependency_name, ".psc")
        dependency_path.parent.mkdir(parents=True, exist_ok=True)
        dependency_path.write_text(
            _merged_source(dependency_name),
            encoding="utf-8",
        )

    result = compile_psc(
        _merged_source(script_name),
        imports=[str(tmp_path), str(SOURCE_ROOT), str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=str(_script_relative_path(script_name, ".psc")),
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None
