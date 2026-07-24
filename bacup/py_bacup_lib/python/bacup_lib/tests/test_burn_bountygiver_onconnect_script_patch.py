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
SCRIPT_NAME = "Fragments:Quests:QF_BURN_BountyGiver_Appalach_00864CC7"
SOURCE_ROOT = REPO_ROOT / "mods" / "SeventySix" / "Scripts" / "Source" / "User"


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


def test_bounty_giver_onconnect_patch_is_member_only_and_bounded():
    patch = _script_patch_source(SCRIPT_NAME)
    assert patch is not None

    lines = patch.splitlines()
    members = [
        name
        for _kind, name, _start, _end in _iter_top_level_papyrus_members(lines)
    ]
    assert members == ["fragment_stage_0100_item_00"]
    assert not any(
        line.strip().lower().startswith(("scriptname ", "state "))
        for line in lines
    )
    assert not any(" property " in f" {line.strip().lower()} " for line in lines)


def test_bounty_giver_onconnect_stage_starts_once_through_story_manager_then_stops():
    patch = _script_patch_source(SCRIPT_NAME)
    assert patch is not None
    stage = _member_body(patch, "fragment_stage_0100_item_00")

    player = "ObjectReference playerRef = Alias_Player.GetReference()"
    active_guard = (
        "!playerRef.HasKeyword(Appalachia_Dialogue_QuestActiveKeyword)"
    )
    start_event = (
        "Appalachia_Dialogue_QuestStartKeyword.SendStoryEventAndWait"
        "(None, playerRef, playerRef)"
    )
    stop = "Stop()"

    assert stage.count(start_event) == 1
    assert stage.index(player) < stage.index(active_guard)
    assert stage.index(active_guard) < stage.index(start_event) < stage.index(stop)
    assert ".Start()" not in stage


def test_bounty_giver_onconnect_production_merge_is_unique_and_idempotent():
    patch = _script_patch_source(SCRIPT_NAME)
    assert patch is not None
    merged = _merged_production_source()

    members = [
        name
        for _kind, name, _start, _end in _iter_top_level_papyrus_members(
            merged.splitlines()
        )
    ]
    assert members.count("fragment_stage_0100_item_00") == 1
    assert _member_body(merged, "fragment_stage_0100_item_00") == _member_body(
        patch, "fragment_stage_0100_item_00"
    )
    assert _merge_script_method_patches(merged, patch) == merged


def test_bounty_giver_onconnect_full_production_merge_native_compiles_for_fo4():
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
