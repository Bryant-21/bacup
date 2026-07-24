from __future__ import annotations

import os
from pathlib import Path

from bacup_lib.workflows.unified import (
    _iter_top_level_papyrus_members,
    _merge_script_method_patches,
    _script_patch_source,
)
from creation_lib.pex import decompile_pex
from creation_lib.pex.native_runtime import compile_psc


REPO_ROOT = Path(__file__).resolve().parents[5]
RAW_FO76_SCRIPTS_DIR = REPO_ROOT / "extracted" / "fo76" / "scripts" / "client"

PATCH_CASES = {
    "DefaultMultiStateClientSideActivator": {
        "pex": "defaultmultistateclientsideactivator.pex",
        "members": {("function", "clientplayanimation")},
    },
    "DefaultSequentialStateActivator": {
        "pex": "defaultsequentialstateactivator.pex",
        "members": {
            ("event", "onsyncvariablenetworkchanged"),
            ("event", "onload"),
        },
    },
}


def _fo4_base_source() -> Path:
    candidates: list[Path] = []
    configured = os.environ.get("FO4_DIR", "").strip().strip('"')
    if configured:
        candidates.append(Path(configured))

    for line in (REPO_ROOT / ".env").read_text(encoding="utf-8").splitlines():
        if line.startswith("FO4_DIR="):
            value = line.split("=", 1)[1].strip().strip('"')
            if value:
                candidates.append(Path(value))
            break

    for game_root in candidates:
        source_root = game_root / "Data" / "Scripts" / "Source" / "Base"
        if source_root.is_dir():
            return source_root

    raise AssertionError("FO4 base Papyrus sources unavailable; compile skip is a failure")


def _production_skeleton(pex_filename: str) -> str:
    path = RAW_FO76_SCRIPTS_DIR / pex_filename
    assert path.is_file(), f"missing raw FO76 PEX: {path}"
    return decompile_pex(path, fo4_api_compat=True)


def _merged_source(script_name: str) -> str:
    case = PATCH_CASES[script_name]
    patch = _script_patch_source(script_name)
    assert patch is not None
    return _merge_script_method_patches(_production_skeleton(case["pex"]), patch)


def test_patches_are_member_fragments_with_expected_members():
    for script_name, case in PATCH_CASES.items():
        patch = _script_patch_source(script_name)
        assert patch is not None
        assert not any(line.strip().lower().startswith("scriptname ") for line in patch.splitlines())
        members = {
            (kind, name)
            for kind, name, _start, _end in _iter_top_level_papyrus_members(
                patch.splitlines()
            )
        }
        assert case["members"] <= members


def test_client_side_state_lookup_preserves_default_and_rejects_unknown_names():
    merged = _merged_source("DefaultMultiStateClientSideActivator")
    assert merged.count("Function ClientPlayAnimation(String animationStateName)") == 1
    assert 'If animationStateName != ""' in merged
    assert "animationIndex = -1" in merged
    assert "AnimationStates[i].StateName == animationStateName" in merged
    assert "i += 1" in merged
    assert "If animationIndex < 0 || !Is3DLoaded()" in merged
    assert "StateStartAnim != \"\"" in merged
    assert "StateJumpAnim" in merged


def test_sequential_state_handlers_guard_empty_and_invalid_arrays_before_indexing():
    merged = _merged_source("DefaultSequentialStateActivator")
    assert merged.count("Event OnLoad()") == 1
    assert merged.count("Event OnSyncVariableNetworkChanged(String varName)") == 1
    assert merged.count("States == None || States.Length == 0") == 2
    assert merged.count("stateIndex = StartState") == 2
    assert "currentState = stateIndex" not in merged
    assert merged.count("clientState = stateIndex") == 2
    assert "PlayAnimation(States[stateIndex].IdleAnim)" in merged
    assert "PlayAnimation(States[stateIndex].TransitionAnim)" in merged


def test_production_merges_compile_for_fo4_without_skips():
    base_source = _fo4_base_source()
    for script_name in PATCH_CASES:
        result = compile_psc(
            _merged_source(script_name),
            imports=[str(base_source)],
            game="fo4",
            flags=str(base_source / "Institute_Papyrus_Flags.flg"),
            source_path=f"{script_name}.psc",
        )
        diagnostics = "\n".join(str(item) for item in result.diagnostics)
        assert result.ok, f"{script_name}: {diagnostics}"
        assert result.pex_bytes is not None
