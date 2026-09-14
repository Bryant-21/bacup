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
PATCHED_SCRIPTS = (
    "W05_RE_ObjectBB02_ItemAddedScript",
    "W05_RE_ObjectBB02_ItemRemovedScript",
)


def _member_names(source: str) -> set[str]:
    return {
        name
        for kind, name, _start, _end in _iter_top_level_papyrus_members(
            source.splitlines()
        )
        if kind in {"function", "event"}
    }


def _merged_source(script_name: str) -> str:
    source_path = SOURCE_ROOT / _script_relative_path(script_name, ".psc")
    patch = _script_patch_source(script_name)
    assert source_path.is_file(), source_path
    assert patch is not None
    return _merge_script_method_patches(
        source_path.read_text(encoding="utf-8"), patch
    )


def _patch_source(script_name: str) -> str:
    patch = _script_patch_source(script_name)
    assert patch is not None
    return patch


@pytest.mark.parametrize("script_name", PATCHED_SCRIPTS)
def test_objectbb02_inventory_patches_register_for_all_items(script_name: str):
    patch = _patch_source(script_name)

    assert "Scriptname " not in patch
    assert "onaliasinit" in _member_names(patch)
    assert "AddInventoryEventFilter(None)" in patch


def test_objectbb02_added_item_uses_player_source_and_unit_value_threshold():
    merged = _merged_source("W05_RE_ObjectBB02_ItemAddedScript")

    alias_guard = merged.find("PlayerAlias == None")
    alias_lookup = merged.find("PlayerAlias.GetReference()")
    source_guard = merged.find("akSourceContainer != kPlayer")
    value_check = merged.find("akBaseItem.GetGoldValue() >= 20")
    good_stage = merged.find("kQuest.SetStage(200)")
    bad_stage = merged.find("kQuest.SetStage(250)")

    assert -1 not in (
        alias_guard,
        alias_lookup,
        source_guard,
        value_check,
        good_stage,
        bad_stage,
    )
    assert alias_guard < alias_lookup < source_guard < value_check < good_stage < bad_stage
    assert "GetGoldValue() * aiItemCount" not in merged
    assert _member_names(merged) == {"onaliasinit", "onitemadded"}


def test_objectbb02_removed_item_only_reports_player_takeback():
    merged = _merged_source("W05_RE_ObjectBB02_ItemRemovedScript")

    alias_guard = merged.find("PlayerAlias == None")
    alias_lookup = merged.find("PlayerAlias.GetReference()")
    destination_guard = merged.find("akDestContainer != kPlayer")
    removed_stage = merged.find("kQuest.SetStage(100)")

    assert -1 not in (alias_guard, alias_lookup, destination_guard, removed_stage)
    assert alias_guard < alias_lookup < destination_guard < removed_stage
    assert _member_names(merged) == {"onaliasinit", "onitemremoved"}


@pytest.mark.parametrize("script_name", PATCHED_SCRIPTS)
def test_objectbb02_inventory_patches_merge_once_and_native_compile_for_fo4(
    script_name: str,
):
    base_source = _fo4_base_source()
    if base_source is None:
        pytest.skip("FO4 base Papyrus sources unavailable")

    merged = _merged_source(script_name)
    merged_again = _merge_script_method_patches(merged, _patch_source(script_name))
    event_name = "onitemadded" if "Added" in script_name else "onitemremoved"
    assert merged_again == merged
    assert "ReferenceAlias Property PlayerAlias Auto mandatory" in merged
    assert merged.lower().count("event onaliasinit()") == 1
    assert merged.lower().count(f"event {event_name}(") == 1

    result = compile_psc(
        merged,
        imports=[str(SOURCE_ROOT), str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=str(_script_relative_path(script_name, ".psc")),
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, f"{script_name}:\n{diagnostics}"
    assert result.pex_bytes is not None
