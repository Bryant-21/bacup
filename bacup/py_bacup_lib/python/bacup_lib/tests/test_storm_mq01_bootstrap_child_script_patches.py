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
RADIO = "Fragments:Quests:QF_Storm_MQ01_Breadcrumb_Rad_00699466"
LEVEL_CONNECTOR = "Fragments:Quests:QF_Storm_MQ01_Breadcrumb_OnI_0072AEAF"
PATCHED_SCRIPTS = (RADIO, LEVEL_CONNECTOR)


def _patch(script_name: str) -> str:
    patch = _script_patch_source(script_name)
    assert patch is not None
    return patch


def _members(source: str) -> list[str]:
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
        if name == member_name.lower()
    )
    return "\n".join(lines[start : end + 1])


def _merged(script_name: str) -> str:
    source_path = SOURCE_ROOT / _script_relative_path(script_name, ".psc")
    assert source_path.is_file(), source_path
    return _merge_script_method_patches(
        source_path.read_text(encoding="utf-8"), _patch(script_name)
    )


@pytest.mark.parametrize("script_name", PATCHED_SCRIPTS)
def test_storm_mq01_child_patches_are_member_only(script_name: str):
    patch = _patch(script_name)
    assert not any(
        line.strip().lower().startswith(("scriptname ", "state ", "endstate"))
        for line in patch.splitlines()
    )
    assert not any(
        " property " in f" {line.strip().lower()} " for line in patch.splitlines()
    )


@pytest.mark.parametrize("script_name", PATCHED_SCRIPTS)
def test_storm_mq01_child_merge_is_unique_and_idempotent(script_name: str):
    patch = _patch(script_name)
    merged = _merged(script_name)
    patch_members = _members(patch)
    merged_members = Counter(_members(merged))

    assert Counter(patch_members) == Counter({name: 1 for name in patch_members})
    for member_name in patch_members:
        assert merged_members[member_name] == 1
        assert _member_body(merged, member_name) == _member_body(patch, member_name)
    assert _merge_script_method_patches(merged, patch) == merged


def test_radio_broadcast_advances_main_quest_and_stops_its_child_quest():
    patch = _patch(RADIO)
    stage_20 = _member_body(patch, "fragment_stage_0020_item_00")
    stage_9000 = _member_body(patch, "fragment_stage_9000_item_00")

    assert "Alias_Player.GetReference() != Game.GetPlayer()" in stage_20
    assert "Storm_MQ01_Breadcrumb.IsRunning()" in stage_20
    assert "!Storm_MQ01_Breadcrumb.IsStageDone(200)" in stage_20
    assert stage_20.count("Storm_MQ01_Breadcrumb.SetStage(200)") == 1
    assert stage_20.count("SetStage(9000)") == 1
    assert stage_9000.count("Stop()") == 1


def test_level_connector_sends_the_breadcrumb_story_event_once_then_stops():
    stage_100 = _member_body(_patch(LEVEL_CONNECTOR), "fragment_stage_0100_item_00")

    assert "Alias_Player.GetReference()" in stage_100
    assert "!Storm_MQ01_Breadcrumb.IsRunning()" in stage_100
    assert "!Storm_MQ01_Breadcrumb.IsCompleted()" in stage_100
    assert (
        stage_100.count("Storm_MQ01_Breadcrumb_QuestStartKeyword.SendStoryEventAndWait")
        == 1
    )
    assert stage_100.count("Stop()") == 1
    assert ".Start()" not in stage_100


@pytest.mark.parametrize("script_name", PATCHED_SCRIPTS)
def test_storm_mq01_child_full_production_merge_native_compiles_for_fo4(
    script_name: str,
):
    base_source = _fo4_base_source()
    assert base_source is not None, "FO4 base Papyrus sources unavailable"

    result = compile_psc(
        _merged(script_name),
        imports=[str(SOURCE_ROOT), str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=str(_script_relative_path(script_name, ".psc")),
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None
