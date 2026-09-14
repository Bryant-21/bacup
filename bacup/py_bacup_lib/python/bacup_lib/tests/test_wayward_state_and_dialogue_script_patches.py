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

# Scripts whose skeletons still carry FO76-only decompiler artifacts
# (GetRefsLinkedToMe arity, dropped CustomEvent declarations) and therefore
# cannot compile yet. Their patches are still checked for a clean merge.
MERGE_ONLY = {
    "WL029_WaywardStateChangeManagerScript",
}

PATCHED_SCRIPTS = {
    "W05_MQ_TheWayward_QuestScript",
    "W05_Wayward_SwapMarkerOnCriteria",
    "W05_WaywardStateSwapRefScript",
    "WL029_WaywardStateChangeManagerScript",
}


def _merged(script_name: str) -> str:
    skeleton = (SOURCE_ROOT / _script_relative_path(script_name, ".psc")).read_text(
        encoding="utf-8"
    )
    patch = _script_patch_source(script_name)
    assert patch is not None, f"no patch registered for {script_name}"
    assert "Scriptname" not in patch
    assert "Property" not in patch
    merged = _merge_script_method_patches(skeleton, patch)
    assert _merge_script_method_patches(merged, patch) == merged
    return merged


@pytest.mark.parametrize("script_name", sorted(PATCHED_SCRIPTS))
def test_wayward_patch_merges_each_member_once(script_name: str):
    merged = _merged(script_name)
    members = [
        name
        for _kind, name, _start, _end in _iter_top_level_papyrus_members(
            merged.splitlines()
        )
    ]
    for member in members:
        assert members.count(member) == 1, f"{script_name}: duplicate member {member}"


def test_wayward_state_patches_replace_the_rmi_round_trip():
    manager = _merged("WL029_WaywardStateChangeManagerScript")
    swap = _merged("W05_WaywardStateSwapRefScript")
    assert "SendRMIToServer" not in manager
    assert "SendRMIToServer" not in swap
    assert "Self.RequestEventState(Game.GetPlayer())" in manager
    assert "Self.CheckPlayerWaywardValue(Game.GetPlayer())" in swap


@pytest.mark.parametrize(
    "script_name", sorted(set(PATCHED_SCRIPTS) - MERGE_ONLY)
)
def test_wayward_patch_compiles(script_name: str):
    base_source = _fo4_base_source()
    if base_source is None:
        pytest.skip("FO4 base Papyrus sources unavailable")

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
