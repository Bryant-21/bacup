from __future__ import annotations

import os
from pathlib import Path

import pytest

from bacup_lib.workflows.unified import _merge_script_method_patches, _script_patch_source
from creation_lib.pex.native_runtime import compile_psc


REPO_ROOT = Path(__file__).resolve().parents[5]
SOURCE_ROOT = REPO_ROOT / "mods" / "SeventySix" / "Scripts" / "Source" / "User"
SCRIPT_NAME = "Objects:ATXHolidayNuclearTree"

SKELETON = """Scriptname Objects:ATXHolidayNuclearTree Extends ObjectReference

ObjectReference placedHazard

hazard Property HazardToPlace Auto mandatory

State working
EndState

Function EnsureHazard()
EndFunction

Function ClearHazard()
EndFunction

Event OnLoad()
EndEvent

Event OnUnload()
EndEvent
"""


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


def _merged_source() -> str:
    patch = _script_patch_source(SCRIPT_NAME)
    assert patch is not None
    return _merge_script_method_patches(SKELETON, patch)


def test_holiday_nuclear_tree_patch_replaces_each_stub_once():
    merged = _merged_source()

    assert merged.lower().count("function ensurehazard()") == 1
    assert merged.lower().count("function clearhazard()") == 1
    assert merged.lower().count("event onload()") == 1
    assert merged.lower().count("event onunload()") == 1
    assert "Function EnsureHazard()\nEndFunction" not in merged
    assert "Function ClearHazard()\nEndFunction" not in merged
    assert "Event OnLoad()\nEndEvent" not in merged
    assert "Event OnUnload()\nEndEvent" not in merged


def test_holiday_nuclear_tree_patch_preserves_declarations_and_working_state():
    merged = _merged_source()

    assert "Scriptname Objects:ATXHolidayNuclearTree Extends ObjectReference" in merged
    assert "ObjectReference placedHazard" in merged
    assert "hazard Property HazardToPlace Auto mandatory" in merged
    assert "State working\nEndState" in merged


def test_holiday_nuclear_tree_hazard_lifecycle_is_idempotent_and_ordered():
    merged = _merged_source()

    ensure_guard = merged.find("placedHazard == None && HazardToPlace != None")
    place = merged.find("placedHazard = PlaceAtMe(HazardToPlace)")
    clear_guard = merged.find("If placedHazard != None")
    disable = merged.find("placedHazard.Disable()")
    delete = merged.find("placedHazard.Delete()")
    clear = merged.find("placedHazard = None")

    assert ensure_guard != -1
    assert place != -1
    assert ensure_guard < place
    assert clear_guard != -1
    assert disable != -1
    assert delete != -1
    assert clear != -1
    assert clear_guard < disable < delete < clear


def test_holiday_nuclear_tree_load_and_unload_call_the_lifecycle_helpers():
    merged = _merged_source()

    load_start = merged.find("Event OnLoad()")
    unload_start = merged.find("Event OnUnload()")

    assert load_start != -1
    assert unload_start != -1
    assert "EnsureHazard()" in merged[load_start:unload_start]
    assert "ClearHazard()" in merged[unload_start:]


def test_holiday_nuclear_tree_merged_source_compiles_for_fo4():
    base_source = _fo4_base_source()
    if base_source is None:
        pytest.skip("FO4 base Papyrus sources unavailable")

    result = compile_psc(
        _merged_source(),
        imports=[str(SOURCE_ROOT), str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path="Objects/ATXHolidayNuclearTree.psc",
    )
    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None
