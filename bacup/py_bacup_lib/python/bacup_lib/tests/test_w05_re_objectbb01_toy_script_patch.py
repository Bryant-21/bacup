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
TOY_ALIAS_SCRIPT = "W05_RE_ObjectBB01_ToyAliasScript"
TOY_NUDGE_SCRIPT = "W05_RE_ObjectBB01_ToyNudgeScript"


def _member_names(source: str) -> set[str]:
    return {
        name
        for kind, name, _start, _end in _iter_top_level_papyrus_members(
            source.splitlines()
        )
        if kind in {"function", "event"}
    }


def _source(script_name: str) -> str:
    source_path = SOURCE_ROOT / _script_relative_path(script_name, ".psc")
    assert source_path.is_file(), source_path
    return source_path.read_text(encoding="utf-8")


def _merged_toy_alias_source() -> tuple[str, str]:
    patch = _script_patch_source(TOY_ALIAS_SCRIPT)
    assert patch is not None
    return _merge_script_method_patches(_source(TOY_ALIAS_SCRIPT), patch), patch


def test_toy_alias_pickup_sets_stage_100_only_for_bound_player_alias():
    merged, patch = _merged_toy_alias_source()

    assert "Scriptname " not in patch
    assert _member_names(merged) == {"oncontainerchanged"}
    assert "Event OnActivate(" not in merged

    alias_guard = merged.find("PlayerAlias == None")
    early_return = merged.find("Return", alias_guard)
    player_ref = merged.find("ObjectReference PlayerRef = PlayerAlias.GetReference()")
    quest_ref = merged.find("Quest OwningQuest = GetOwningQuest()")
    player_ref_guard = merged.find("PlayerRef != None")
    container_guard = merged.find("akNewContainer == PlayerRef")
    quest_guard = merged.find("OwningQuest != None")
    stage_set = merged.find("OwningQuest.SetStage(100)")

    assert -1 not in (
        alias_guard,
        early_return,
        player_ref,
        quest_ref,
        player_ref_guard,
        container_guard,
        quest_guard,
        stage_set,
    )
    assert alias_guard < early_return < player_ref
    assert player_ref < player_ref_guard < container_guard < stage_set
    assert alias_guard < quest_ref < quest_guard < stage_set
    assert patch.count("akOldContainer") == 1
    assert patch.count("SetStage(100)") == 1
    assert "Game.GetPlayer()" not in patch


def test_toy_nudge_stays_unpatched_until_event_operation_and_constants_survive():
    source = _source(TOY_NUDGE_SCRIPT)

    assert _member_names(source) == set()
    assert "ReferenceAlias Property CenterMarker Auto mandatory" in source
    assert "objectreference Property TestMarker Auto" in source
    assert _script_patch_source(TOY_NUDGE_SCRIPT) is None


def test_toy_alias_patch_merges_once_and_native_compiles_for_fo4():
    base_source = _fo4_base_source()
    if base_source is None:
        pytest.skip("FO4 base Papyrus sources unavailable")

    merged, patch = _merged_toy_alias_source()
    merged_again = _merge_script_method_patches(merged, patch)

    assert merged_again == merged
    assert merged.lower().count("event oncontainerchanged(") == 1
    assert "ReferenceAlias Property PlayerAlias Auto mandatory" in merged

    result = compile_psc(
        merged,
        imports=[str(SOURCE_ROOT), str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=str(_script_relative_path(TOY_ALIAS_SCRIPT, ".psc")),
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, f"{TOY_ALIAS_SCRIPT}:\n{diagnostics}"
    assert result.pex_bytes is not None
