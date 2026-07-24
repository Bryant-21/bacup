from __future__ import annotations

from pathlib import Path

from bacup_lib.tests.test_terminal_fragment_script_patches import _fo4_base_source
from bacup_lib.workflows.unified import (
    _fo76_to_fo4_script_type,
    _iter_papyrus_states,
    _iter_top_level_papyrus_members,
    _merge_script_method_patches,
    _script_patch_source,
)
from creation_lib.pex import decompile_pex
from creation_lib.pex.native_runtime import compile_psc


REPO_ROOT = Path(__file__).resolve().parents[5]
SCRIPT_NAME = "CampObjectScript"
RAW_PEX = (
    REPO_ROOT
    / "extracted"
    / "fo76"
    / "scripts"
    / "client"
    / "campobjectscript.pex"
)


def _merged_source() -> str:
    patch = _script_patch_source(SCRIPT_NAME)
    assert patch is not None
    skeleton = decompile_pex(
        RAW_PEX,
        type_adapter=_fo76_to_fo4_script_type,
        drop_script_const=True,
        skip_internal_functions=True,
        fo4_api_compat=True,
    )
    return _merge_script_method_patches(skeleton, patch)


def test_patch_replaces_fo76_trace_log_with_fo4_user_log():
    patch = _script_patch_source(SCRIPT_NAME)
    assert patch is not None
    assert "Scriptname" not in patch
    assert "Property" not in patch
    assert not list(_iter_papyrus_states(patch.splitlines()))

    members = [
        (kind, name)
        for kind, name, _start, _end in _iter_top_level_papyrus_members(
            patch.splitlines()
        )
    ]
    assert members == [("function", "trace")]

    merged = _merged_source()
    assert merged.count("Bool Function Trace(") == 1
    assert "Debug.TraceLog" not in merged
    assert 'Debug.OpenUserLog("Camp")' in merged
    assert 'Return Debug.TraceUser("Camp"' in merged


def test_merged_patch_native_compiles_for_fo4():
    base_source = _fo4_base_source()
    assert base_source is not None, "FO4 base Papyrus sources unavailable"

    result = compile_psc(
        _merged_source(),
        imports=[str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path="CampObjectScript.psc",
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None
