from __future__ import annotations

import os
from pathlib import Path

import pytest

from bacup_lib.workflows.unified import (
    _iter_papyrus_members_in_state,
    _iter_papyrus_states,
    _iter_top_level_papyrus_members,
    _merge_script_method_patches,
    _script_patch_source,
)
from creation_lib.pex import decompile_pex
from creation_lib.pex.native_runtime import compile_psc


REPO_ROOT = Path(__file__).resolve().parents[5]
SCRIPT_NAME = "HandScannerScript"
DEPLOYED_PEX = (
    REPO_ROOT / "mods" / "SeventySix" / "data" / "Scripts" / "handscannerscript.pex"
)
MOD_SOURCE_DIR = REPO_ROOT / "mods" / "SeventySix" / "Scripts" / "Source" / "User"
DEPLOYED_SCRIPTS_DIR = REPO_ROOT / "mods" / "SeventySix" / "data" / "Scripts"


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


def _patch_source() -> str:
    patch = _script_patch_source(SCRIPT_NAME)
    assert patch is not None
    return patch


def _production_skeleton() -> str:
    if not DEPLOYED_PEX.is_file():
        pytest.skip(f"deployed production PEX unavailable: {DEPLOYED_PEX}")
    return decompile_pex(DEPLOYED_PEX, fo4_api_compat=True)


def _initial_state_body(source: str) -> str:
    lines = source.splitlines()
    states = {name: (start, end) for name, start, end in _iter_papyrus_states(lines)}
    assert "initial" in states
    start, end = states["initial"]
    return "\n".join(lines[start : end + 1])


def test_patch_is_a_member_fragment_scoped_to_the_initial_state():
    patch = _patch_source()
    assert not any(
        line.strip().lower().startswith("scriptname ") for line in patch.splitlines()
    )
    assert not _iter_top_level_papyrus_members(patch.splitlines())

    initial = _initial_state_body(patch)
    assert initial.count("Function Init_SetMySyncActivator()") == 1


def test_patch_rechecks_the_array_bound_on_every_iteration():
    initial = _initial_state_body(_patch_source())
    assert "While !foundMultiple2StateActivators && i < refs.Length" in initial
    assert "i += 1" in initial
    assert "temp7" not in initial


def test_patch_preserves_unique_and_multiple_activator_behavior():
    initial = _initial_state_body(_patch_source())
    assert "current = refs[i] as default2stateactivator" in initial
    assert "If mySyncActivator == None" in initial
    assert "mySyncActivator = current" in initial
    assert "foundMultiple2StateActivators = True" in initial
    assert "mySyncActivator = None" in initial


def test_merge_replaces_only_the_initial_state_member():
    skeleton = """Scriptname HandScannerScript Extends RestrictedAccessScript

Function Init_SetMySyncActivator()
EndFunction

Auto State Initial
    Function Init_SetMySyncActivator()
        Bool loopCondition = True
        While loopCondition
        EndWhile
    EndFunction

    Event OnInit()
        Self.Init_SetMySyncActivator()
    EndEvent
EndState
"""

    merged = _merge_script_method_patches(skeleton, _patch_source())
    top_level = _iter_top_level_papyrus_members(merged.splitlines())
    assert [(kind, name) for kind, name, _start, _end in top_level] == [
        ("function", "init_setmysyncactivator")
    ]

    initial = _initial_state_body(merged)
    assert initial.count("Function Init_SetMySyncActivator()") == 1
    assert "While loopCondition" not in initial
    assert "While !foundMultiple2StateActivators && i < refs.Length" in initial
    assert "Event OnInit()" in initial


def test_production_merge_keeps_the_repair_in_the_initial_state():
    merged = _merge_script_method_patches(_production_skeleton(), _patch_source())
    merged_lines = merged.splitlines()
    initial_start, initial_end = next(
        (start, end)
        for name, start, end in _iter_papyrus_states(merged_lines)
        if name == "initial"
    )
    initial = "\n".join(merged_lines[initial_start : initial_end + 1])
    members = _iter_papyrus_members_in_state(
        merged_lines,
        initial_start,
        initial_end,
    )

    assert ("function", "init_setmysyncactivator") in [
        (kind, name) for kind, name, _start, _end in members
    ]
    assert "While !foundMultiple2StateActivators && i < refs.Length" in initial
    assert "temp7" not in initial


def test_production_merge_native_compiles_for_fo4():
    base_source = _fo4_base_source()
    if base_source is None:
        pytest.skip("FO4 base Papyrus sources unavailable")
    if not MOD_SOURCE_DIR.is_dir() or not DEPLOYED_SCRIPTS_DIR.is_dir():
        pytest.skip("SeventySix generated/deployed script directories unavailable")

    merged = _merge_script_method_patches(_production_skeleton(), _patch_source())
    result = compile_psc(
        merged,
        imports=[str(MOD_SOURCE_DIR), str(DEPLOYED_SCRIPTS_DIR), str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=f"{SCRIPT_NAME}.psc",
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None
