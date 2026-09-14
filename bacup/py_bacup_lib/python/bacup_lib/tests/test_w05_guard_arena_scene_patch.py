from __future__ import annotations

from pathlib import Path

import pytest

from bacup_lib.workflows.unified import (
    _iter_top_level_papyrus_members,
    _merge_script_method_patches,
    _script_patch_source,
)
from creation_lib.pex.native_runtime import compile_psc

from bacup_lib.tests.test_terminal_fragment_script_patches import _fo4_base_source


REPO_ROOT = Path(__file__).resolve().parents[5]
SOURCE = (
    REPO_ROOT
    / "mods"
    / "SeventySix"
    / "Scripts"
    / "Source"
    / "User"
    / "fragments"
    / "Scenes"
    / "SF_W05_MQR_203P_GuardArena_E_005A2CD4.psc"
)
SCRIPT_NAME = "Fragments:Scenes:SF_W05_MQR_203P_GuardArena_E_005A2CD4"


def _merged_source() -> str:
    skeleton = SOURCE.read_text(encoding="utf-8")
    patch = _script_patch_source(SCRIPT_NAME)
    assert patch is not None
    return _merge_script_method_patches(skeleton, patch)


def test_guard_arena_scene_patch_merges_exact_phase_callbacks() -> None:
    merged = _merged_source()
    lines = merged.splitlines()
    members = {
        name: "\n".join(lines[start : end + 1])
        for _kind, name, start, end in _iter_top_level_papyrus_members(lines)
    }

    assert set(members) == {"fragment_phase_02_end", "fragment_phase_03_end"}
    assert merged.count("Function Fragment_Phase_02_End()") == 1
    assert merged.count("Function Fragment_Phase_03_End()") == 1
    assert "guardRef.Disable()" in members["fragment_phase_02_end"]
    assert "guardRef.MoveTo(exitRef)" in members["fragment_phase_03_end"]
    assert "exitRef != None" in members["fragment_phase_03_end"]
    assert "referencealias Property alias_Guard Auto" in merged
    assert "referencealias Property xmarker_Exit Auto" in merged


def test_guard_arena_scene_merged_source_native_compiles_for_fo4() -> None:
    base_source = _fo4_base_source()
    if base_source is None:
        pytest.skip("FO4 base Papyrus sources unavailable")

    result = compile_psc(
        _merged_source(),
        imports=[str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=SOURCE.name,
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None
