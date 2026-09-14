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
FRAGMENT_SCRIPT = "Fragments:Quests:QF_MTN_MQ_Missing_003A5FA0"
QUEST_SCRIPT = "MTN_MQ_QuestScript"
PLAYER_SCRIPT = "MTN_MQ_Playerscript"
CHECKPOINT_SCRIPT = "DefaultCheckpointingScript"
SCRIPT_NAMES = (FRAGMENT_SCRIPT, QUEST_SCRIPT, PLAYER_SCRIPT)
CHECKPOINT_TABLE = ((1, 100), (2, 200), (3, 300), (4, 400), (5, 500))
STAGE_MEMBERS = {
    f"fragment_stage_{stage:04d}_item_00"
    for stage in (50, 100, 200, 300, 350, 400, 500, 600)
}


def _patch_source(script_name: str) -> str:
    patch = _script_patch_source(script_name)
    assert patch is not None
    return patch


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
    assert source_path.is_file(), source_path
    return _merge_script_method_patches(
        source_path.read_text(encoding="utf-8"), _patch_source(script_name)
    )


def _checkpoint_stage(checkpoint_value: int) -> int | None:
    eligible = [entry for entry in CHECKPOINT_TABLE if entry[0] <= checkpoint_value]
    if not eligible:
        return None
    return max(eligible, key=lambda entry: entry[0])[1]


def _run_init_reconciliation(
    checkpoint_value: int,
    completed_stages: list[int] | None = None,
    *,
    raiders_completed: bool = False,
    has_damaged_uplink: bool = False,
) -> list[int]:
    stages = list(completed_stages or [])
    checkpoint_stage = _checkpoint_stage(checkpoint_value)
    if checkpoint_stage is not None and checkpoint_stage not in stages:
        stages.append(checkpoint_stage)

    if 500 in stages:
        return stages
    if 300 in stages and 400 not in stages and has_damaged_uplink:
        stages.append(400)
    elif 200 in stages and 300 not in stages and raiders_completed:
        stages.append(300)
    return stages


def test_fragment_patch_defines_exact_bound_stage_set() -> None:
    patch = _patch_source(FRAGMENT_SCRIPT)
    member_names = _member_names(patch)

    assert set(member_names) == STAGE_MEMBERS
    assert len(member_names) == len(STAGE_MEMBERS)
    assert all(member_names.count(member) == 1 for member in STAGE_MEMBERS)


def test_all_missing_link_patches_merge_once_idempotently_and_compile(
    tmp_path: Path,
) -> None:
    base_source = _fo4_base_source()
    assert base_source is not None, "FO4 base Papyrus sources unavailable"

    checkpoint_path = tmp_path / _script_relative_path(CHECKPOINT_SCRIPT, ".psc")
    checkpoint_path.parent.mkdir(parents=True, exist_ok=True)
    checkpoint_path.write_text(_merged_source(CHECKPOINT_SCRIPT), encoding="utf-8")

    for script_name in SCRIPT_NAMES:
        patch = _patch_source(script_name)
        merged = _merged_source(script_name)
        merged_members = _member_names(merged)

        for member in _member_names(patch):
            assert merged_members.count(member) == 1
            assert _member_body(merged, member) == _member_body(patch, member)
        assert _merge_script_method_patches(merged, patch) == merged

        result = compile_psc(
            merged,
            imports=[str(tmp_path), str(SOURCE_ROOT), str(base_source)],
            game="fo4",
            flags=str(base_source / "Institute_Papyrus_Flags.flg"),
            source_path=str(_script_relative_path(script_name, ".psc")),
        )
        diagnostics = "\n".join(str(item) for item in result.diagnostics)
        assert result.ok, f"{script_name}: {diagnostics}"
        assert result.pex_bytes is not None


def test_stages_preserve_objectives_completion_and_story_handoff() -> None:
    patch = _patch_source(FRAGMENT_SCRIPT)

    stage_50 = _member_body(patch, "fragment_stage_0050_item_00")
    assert "playerRef.SetValue(MTN_MQ_StartedValue, 1.0)" in stage_50
    assert "SetStage(" not in stage_50

    expected_objectives = {
        100: "SetObjectiveDisplayed(100, True)",
        200: "SetObjectiveDisplayed(200, True)",
        300: "SetObjectiveDisplayed(300, True)",
        400: "SetObjectiveDisplayed(400, True)",
    }
    for stage, expected in expected_objectives.items():
        assert expected in _member_body(
            patch, f"fragment_stage_{stage:04d}_item_00"
        )

    stage_500 = _member_body(patch, "fragment_stage_0500_item_00")
    assert stage_500.count("CompleteQuest()") == 1
    assert stage_500.count(
        "FS01_Warn_QuestStartKeyword.SendStoryEventAndWait(None, playerRef, playerRef)"
    ) == 1
    assert "FS01_MQ_Warn_QuestActiveKeyword" in stage_500
    assert "Alias_currentPlayer.TryToSetValue(FS01_MQ_Warn_StartedValue, 1.0)" in stage_500
    assert ".Start()" not in stage_500
    assert "AddItem(" not in stage_500


@pytest.mark.parametrize(
    ("checkpoint_value", "expected_stage"),
    [
        (0, None),
        (1, 100),
        (2, 200),
        (3, 300),
        (4, 400),
        (5, 500),
        (6, 500),
    ],
)
def test_bound_checkpoint_table_selects_highest_eligible_stage(
    checkpoint_value: int, expected_stage: int | None
) -> None:
    assert _checkpoint_stage(checkpoint_value) == expected_stage


@pytest.mark.parametrize(
    ("checkpoint_value", "raiders_completed", "has_uplink", "expected"),
    [
        (1, True, True, [100]),
        (2, True, False, [200, 300]),
        (3, False, True, [300, 400]),
        (4, True, True, [400]),
        (5, True, True, [500]),
    ],
)
def test_child_reconciliation_only_advances_from_inherited_checkpoint(
    checkpoint_value: int,
    raiders_completed: bool,
    has_uplink: bool,
    expected: list[int],
) -> None:
    first = _run_init_reconciliation(
        checkpoint_value,
        raiders_completed=raiders_completed,
        has_damaged_uplink=has_uplink,
    )
    assert first == expected
    assert (
        _run_init_reconciliation(
            checkpoint_value,
            first,
            raiders_completed=raiders_completed,
            has_damaged_uplink=has_uplink,
        )
        == expected
    )


def test_child_quest_init_preserves_parent_checkpointing_once_and_first() -> None:
    body = _member_body(_patch_source(QUEST_SCRIPT), "onquestinit")

    assert body.count("Parent.OnQuestInit()") == 1
    assert body.count("MTNMQ_ReconcileProgress()") == 1
    assert body.index("Parent.OnQuestInit()") < body.index(
        "MTNMQ_ReconcileProgress()"
    )
    assert "SetStage(" not in body


def test_uplink_alias_and_scene_controller_are_reentry_safe() -> None:
    quest_patch = _patch_source(QUEST_SCRIPT)
    player_patch = _patch_source(PLAYER_SCRIPT)
    fragment_patch = _patch_source(FRAGMENT_SCRIPT)

    uplink = _member_body(quest_patch, "mtnmq_handledamageduplinkadded")
    assert "akBaseItem != FS01_MQ_Warn_BrokenUplinkMiscItem" in uplink
    assert "IsStageDone(400)" in uplink
    assert "!IsStageDone(300)" in uplink
    assert uplink.count("DamagedUplink.ForceRefTo(akItemReference)") == 1
    assert uplink.count("SetStage(400)") == 1

    item_added = _member_body(player_patch, "onitemadded")
    assert item_added.count("MTNMQ_HandleDamagedUplinkAdded") == 1
    assert "AddInventoryEventFilter(FS01_MQ_Warn_BrokenUplinkMiscItem)" in player_patch
    assert "RemoveAllInventoryEventFilters()" in player_patch
    assert "MTNMQ_ReconcileProgress()" in _member_body(
        player_patch, "onplayerloadgame"
    )

    scene = _member_body(quest_patch, "mtnmq_playmadiganscene")
    assert "bMadiganScenePlayed" in scene
    assert scene.count("MTN_MQ_Rose_MadiganScene.Start()") == 1
    assert "MTNMQ_PlayMadiganScene()" in _member_body(
        fragment_patch, "fragment_stage_0350_item_00"
    )

    shutdown = _member_body(fragment_patch, "fragment_stage_0600_item_00")
    assert "Alias_PlayerFoundMadigan.Clear()" in shutdown
    assert "Alias_DamagedUplink.Clear()" in shutdown
    assert "RemoveItem(" not in shutdown
