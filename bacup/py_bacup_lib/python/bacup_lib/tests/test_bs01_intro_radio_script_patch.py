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
SCRIPT_NAME = "Fragments:Quests:QF_BS01_Radio_IntroBroadcast_005E9591"
PATCH_MEMBERS = {
    "fragment_stage_0020_item_00",
    "fragment_stage_9000_item_00",
}
VMAD_SKELETON = """Scriptname Fragments:Quests:QF_BS01_Radio_IntroBroadcast_005E9591 Extends Quest hidden

referencealias Property Alias_Player Auto mandatory
quest Property BS01_MQ00_Misc_Breadcrumb Auto mandatory

Function Fragment_Stage_0020_Item_00()
EndFunction

Function Fragment_Stage_9000_Item_00()
EndFunction
"""


def _member_body(source: str, member_name: str) -> str:
    lines = source.splitlines()
    start, end = next(
        (start, end)
        for _kind, name, start, end in _iter_top_level_papyrus_members(lines)
        if name == member_name.lower()
    )
    return "\n".join(lines[start : end + 1])


def _member_names(source: str) -> list[str]:
    return [
        name
        for _kind, name, _start, _end in _iter_top_level_papyrus_members(
            source.splitlines()
        )
    ]


def _merged_production_source() -> str:
    source_path = SOURCE_ROOT / _script_relative_path(SCRIPT_NAME, ".psc")
    patch = _script_patch_source(SCRIPT_NAME)
    assert source_path.is_file(), source_path
    assert patch is not None
    return _merge_script_method_patches(
        source_path.read_text(encoding="utf-8"), patch
    )


def test_patch_is_member_only_and_contains_all_bound_fragments_once():
    patch = _script_patch_source(SCRIPT_NAME)
    assert patch is not None

    member_names = _member_names(patch)
    assert set(member_names) == PATCH_MEMBERS
    for member_name in PATCH_MEMBERS:
        assert member_names.count(member_name) == 1
    assert not any(
        line.strip().lower().startswith(("scriptname ", "state "))
        for line in patch.splitlines()
    )
    assert not any(
        " property " in f" {line.strip().lower()} " for line in patch.splitlines()
    )


def test_exact_vmad_member_merge_is_unique_and_idempotent():
    patch = _script_patch_source(SCRIPT_NAME)
    assert patch is not None
    merged = _merge_script_method_patches(VMAD_SKELETON, patch)
    member_names = _member_names(merged)

    assert merged.count("referencealias Property Alias_Player Auto mandatory") == 1
    assert (
        merged.count("quest Property BS01_MQ00_Misc_Breadcrumb Auto mandatory") == 1
    )
    for member_name in PATCH_MEMBERS:
        assert member_names.count(member_name) == 1
        assert _member_body(merged, member_name) == _member_body(patch, member_name)
    assert _merge_script_method_patches(merged, patch) == merged


def test_production_merge_adds_missing_members_without_duplicate_declarations():
    patch = _script_patch_source(SCRIPT_NAME)
    assert patch is not None
    merged = _merged_production_source()
    member_names = _member_names(merged)

    assert merged.lower().count("scriptname ") == 1
    assert merged.count("referencealias Property Alias_Player Auto mandatory") == 1
    assert (
        merged.count("quest Property BS01_MQ00_Misc_Breadcrumb Auto mandatory") == 1
    )
    for member_name in PATCH_MEMBERS:
        assert member_names.count(member_name) == 1
        assert _member_body(merged, member_name) == _member_body(patch, member_name)
    assert _merge_script_method_patches(merged, patch) == merged


def test_radio_handoff_is_local_idempotent_and_cleans_up_through_stage_9000():
    patch = _script_patch_source(SCRIPT_NAME)
    assert patch is not None
    stage_20 = _member_body(patch, "fragment_stage_0020_item_00")
    stage_9000 = _member_body(patch, "fragment_stage_9000_item_00")

    local_player_guard = "Alias_Player.GetReference() != Game.GetPlayer()"
    breadcrumb_guard = "BS01_MQ00_Misc_Breadcrumb.IsStageDone(200)"
    breadcrumb_handoff = "BS01_MQ00_Misc_Breadcrumb.SetStage(200)"

    assert stage_20.count(local_player_guard) == 1
    assert stage_20.count("BS01_MQ00_Misc_Breadcrumb.IsRunning()") == 1
    assert stage_20.count(breadcrumb_guard) == 1
    assert stage_20.count(breadcrumb_handoff) == 1
    assert stage_20.count("SetStage(9000)") == 1
    assert stage_20.index(local_player_guard) < stage_20.index(breadcrumb_handoff)
    assert stage_20.index(breadcrumb_handoff) < stage_20.index("SetStage(9000)")
    assert "Stop()" not in stage_20

    assert stage_9000.count("Stop()") == 1
    assert ".Start()" not in patch
    assert "SendStoryEvent" not in patch
    assert "BS01_MQ01" not in patch


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
