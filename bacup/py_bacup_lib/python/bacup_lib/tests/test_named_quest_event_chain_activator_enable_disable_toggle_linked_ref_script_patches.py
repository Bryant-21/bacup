from __future__ import annotations

import os
from pathlib import Path

import pytest

from bacup_lib.workflows.unified import (
    _iter_top_level_papyrus_members,
    _merge_script_method_patches,
    _script_patch_source,
    _script_relative_path,
)
from creation_lib.pex.native_runtime import compile_psc


REPO_ROOT = Path(__file__).resolve().parents[5]
SOURCE_ROOT = REPO_ROOT / "mods" / "SeventySix" / "Scripts" / "Source" / "User"

PATCH_CASES = {
    "E05_Caravan_Obstacle": {"ondestructionstagechanged"},
    "LC158LumberMillNoteScript": {"onread"},
}


def _fo4_base_source() -> Path | None:
    candidates: list[Path] = []
    configured = os.environ.get("FO4_DIR", "").strip().strip('"')
    if configured:
        candidates.append(Path(configured))
    env_path = REPO_ROOT / ".env"
    if env_path.is_file():
        for line in env_path.read_text(encoding="utf-8").splitlines():
            if line.startswith("FO4_DIR="):
                value = line.split("=", 1)[1].strip().strip('"')
                if value:
                    candidates.append(Path(value))
                break
    for game_root in candidates:
        source_root = game_root / "Data" / "Scripts" / "Source" / "Base"
        if source_root.is_dir():
            return source_root
    return None


def _member_names(source: str) -> set[str]:
    return {
        name
        for _kind, name, _start, _end in _iter_top_level_papyrus_members(
            source.splitlines()
        )
    }


def _merged_source(script_name: str) -> str:
    source_path = SOURCE_ROOT / _script_relative_path(script_name, ".psc")
    patch = _script_patch_source(script_name)
    assert source_path.is_file(), source_path
    assert patch is not None
    return _merge_script_method_patches(
        source_path.read_text(encoding="utf-8"), patch
    )


@pytest.mark.parametrize(("script_name", "expected_members"), PATCH_CASES.items())
def test_patch_supplies_expected_members(script_name: str, expected_members: set[str]):
    patch = _script_patch_source(script_name)

    assert patch is not None
    assert "Scriptname " not in patch
    assert expected_members <= _member_names(patch)

    merged = _merged_source(script_name)
    assert expected_members <= _member_names(merged)
    assert merged.lower().count("scriptname ") == 1


def test_e05_caravan_obstacle_merged_source_has_single_handler():
    merged = _merged_source("E05_Caravan_Obstacle")

    assert merged.lower().count("event ondestructionstagechanged(") == 1


def test_e05_caravan_obstacle_guards_on_isdestroyed_before_effects():
    merged = _merged_source("E05_Caravan_Obstacle")

    guard_index = merged.find("Self.IsDestroyed()")
    goto_index = merged.find('GoToState("destroyed")')
    explosive_index = merged.find("PlaceAtMe(Explosive)")
    debris_index = merged.find("PlaceAtMe(Debris)")
    wait_index = merged.find("Utility.Wait(DisableDelay)")
    disable_index = merged.find("Self.DisableNoWait()")

    assert guard_index != -1
    assert goto_index != -1
    assert explosive_index != -1
    assert debris_index != -1
    assert wait_index != -1
    assert disable_index != -1
    assert (
        guard_index
        < goto_index
        < explosive_index
        < debris_index
        < wait_index
        < disable_index
    )


def test_lc158_lumber_mill_note_merged_source_has_single_handler():
    merged = _merged_source("LC158LumberMillNoteScript")

    assert merged.lower().count("event onread(") == 1


def test_lc158_lumber_mill_note_enables_poi_marker():
    merged = _merged_source("LC158LumberMillNoteScript")

    assert "POI287Marker.Enable()" in merged


@pytest.mark.parametrize("script_name", PATCH_CASES)
def test_patch_set_native_compiles_for_fo4(script_name: str):
    base_source = _fo4_base_source()
    if base_source is None:
        pytest.skip("FO4 base Papyrus sources unavailable")

    merged = _merged_source(script_name)
    result = compile_psc(
        merged,
        imports=[str(SOURCE_ROOT), str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=str(_script_relative_path(script_name, ".psc")),
    )
    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None
