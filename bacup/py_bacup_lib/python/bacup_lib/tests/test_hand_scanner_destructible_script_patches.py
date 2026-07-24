from __future__ import annotations

import os
from pathlib import Path

import pytest

from bacup_lib.workflows.unified import (
    _iter_papyrus_states,
    _iter_papyrus_members_in_state,
    _iter_top_level_papyrus_members,
    _merge_script_method_patches,
    _script_patch_source,
)
from creation_lib.pex import decompile_pex
from creation_lib.pex.native_runtime import compile_psc


REPO_ROOT = Path(__file__).resolve().parents[5]
SCRIPT_NAME = "HandScannerDestructibleScript"
DEPLOYED_PEX = (
    REPO_ROOT / "mods" / "SeventySix" / "data" / "Scripts" / "handscannerdestructiblescript.pex"
)
PARENT_SOURCE_DIR = REPO_ROOT / "mods" / "SeventySix" / "Scripts" / "Source" / "User"
DEPLOYED_SCRIPTS_DIR = REPO_ROOT / "mods" / "SeventySix" / "data" / "Scripts"

# States the child fragment must guard with a destroyed check before delegating to
# the parent's real transition logic via `parent.SetNextState()`.
GUARDED_DELEGATE_STATES = (
    "resetredtoblue",
    "scanbluetogreen",
    "resetgreentoblue",
    "scanbluetored",
)

# States that must instead be an explicit no-op — these have no parent-level
# equivalent, so an unguarded/undefined SetNextState() here would fall through the
# state-fallback chain straight to the parent's real (color-changing) SetNextState().
NOOP_TERMINAL_STATES = ("destroyed", "startsdestroyed")


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


def _production_skeleton() -> str:
    if not DEPLOYED_PEX.is_file():
        pytest.skip(f"deployed production PEX unavailable: {DEPLOYED_PEX}")
    return decompile_pex(DEPLOYED_PEX, fo4_api_compat=True)


def _patch_source() -> str:
    patch = _script_patch_source(SCRIPT_NAME)
    assert patch is not None
    return patch


def _merged_production_source() -> str:
    return _merge_script_method_patches(_production_skeleton(), _patch_source())


def _patch_state_body(state_name: str) -> str:
    patch_lines = _patch_source().splitlines()
    states = {name: (start, end) for name, start, end in _iter_papyrus_states(patch_lines)}
    assert state_name.lower() in states, f"patch has no {state_name!r} state block"
    start, end = states[state_name.lower()]
    return "\n".join(patch_lines[start : end + 1])


def test_patch_exists_with_no_scriptname_line():
    patch = _patch_source()
    assert not any(
        line.strip().lower().startswith("scriptname ") for line in patch.splitlines()
    )


def test_patch_declares_no_states_beyond_the_hollow_skeleton():
    # The merger cannot safely introduce new states — every state named in the
    # patch must already exist (empty) in the production skeleton.
    skeleton_states = {
        name for name, _start, _end in _iter_papyrus_states(_production_skeleton().splitlines())
    }
    patch_states = {
        name for name, _start, _end in _iter_papyrus_states(_patch_source().splitlines())
    }
    assert patch_states <= skeleton_states


@pytest.mark.parametrize("state_name", GUARDED_DELEGATE_STATES)
def test_transition_states_guard_and_delegate_to_parent(state_name: str):
    body = _patch_state_body(state_name)
    assert "Function SetNextState()" in body
    # Lock the guard's polarity, not just the presence of IsDestroyed() — an
    # inverted guard (missing the `!`) would delegate only while destroyed.
    assert "!Self.IsDestroyed()" in body
    assert "parent.SetNextState()" in body


def test_initial_state_consumes_shouldstartdestroyed_and_delegates_otherwise():
    body = _patch_state_body("Initial")
    assert "Event OnInit()" in body
    assert "shouldStartDestroyed" in body
    assert "SetDestroyed(true)" in body
    assert 'GoToState("startsdestroyed")' in body
    assert "parent.OnInit()" in body


@pytest.mark.parametrize("state_name", NOOP_TERMINAL_STATES)
def test_terminal_states_override_setnextstate_as_a_noop(state_name: str):
    body = _patch_state_body(state_name)
    assert "Function SetNextState()" in body
    # Must NOT delegate to the parent — that would undo the terminal-state guard.
    assert "parent.SetNextState()" not in body


def test_destroyed_and_startsdestroyed_setnextstate_is_not_top_level():
    """Regression guard for the highest-risk merge outcome: a SetNextState body
    landing top-level instead of inside destroyed/startsdestroyed would silently
    invert the state-fallback fix (it would become the new default-state override,
    blocking SetNextState() in EVERY state instead of only the two terminal ones)."""
    top_level_functions = {
        name
        for kind, name, _start, _end in _iter_top_level_papyrus_members(
            _patch_source().splitlines()
        )
        if kind == "function"
    }
    assert "setnextstate" not in top_level_functions


def test_ondestructionstagechanged_is_top_level_not_state_scoped():
    # Must live in the default state so it fires regardless of which color/transition
    # state the object is currently in.
    top_level_events = {
        name
        for kind, name, _start, _end in _iter_top_level_papyrus_members(
            _patch_source().splitlines()
        )
        if kind == "event"
    }
    assert "ondestructionstagechanged" in top_level_events


def test_ondestructionstagechanged_drives_destroy_and_repair_transitions():
    patch = _patch_source()
    assert 'GoToState("destroyed")' in patch
    assert "aiCurrentStage > 0" in patch
    assert "aiCurrentStage == 0" in patch
    assert "aiOldStage > 0" in patch


def test_repair_branch_does_not_call_the_dead_init_sync_activator():
    """Regression guard: parent.Init_SetMySyncActivator() resolves through the
    parent's SAME-NAMED state first, but the object is in destroyed/startsdestroyed
    when the repair branch runs — states the parent doesn't declare — so it falls
    to the parent's EMPTY default-state stub (the real body lives only in the
    parent's Initial state). That call is dead and must not be reintroduced; repair
    relies solely on the persisted mySyncActivator plus parent.SetNextState()
    (which, for the same reason, resolves to the parent's non-empty DEFAULT-state
    body — the one that actually restores color)."""
    patch = _patch_source()
    assert "Init_SetMySyncActivator" not in patch
    repair_body = patch.split("aiCurrentStage == 0", 1)[1]
    assert "parent.SetNextState()" in repair_body


def test_merged_production_source_preserves_declarations_and_all_seven_states():
    merged = _merged_production_source()
    assert "Scriptname HandScannerDestructibleScript Extends HandScannerScript" in merged
    assert "Bool Property shouldStartDestroyed Auto" in merged

    merged_lines = merged.splitlines()
    merged_states = {
        name: (start, end) for name, start, end in _iter_papyrus_states(merged_lines)
    }
    expected_states = {
        "resetredtoblue",
        "initial",
        "startsdestroyed",
        "scanbluetogreen",
        "destroyed",
        "resetgreentoblue",
        "scanbluetored",
    }
    assert set(merged_states) == expected_states

    for state_name in GUARDED_DELEGATE_STATES:
        start, end = merged_states[state_name]
        members = _iter_papyrus_members_in_state(merged_lines, start, end)
        assert ("function", "setnextstate") in [(k, n) for k, n, _s, _e in members]

    for state_name in NOOP_TERMINAL_STATES:
        start, end = merged_states[state_name]
        members = _iter_papyrus_members_in_state(merged_lines, start, end)
        names = [(k, n) for k, n, _s, _e in members]
        assert ("function", "setnextstate") in names

    init_start, init_end = merged_states["initial"]
    init_members = _iter_papyrus_members_in_state(merged_lines, init_start, init_end)
    assert ("event", "oninit") in [(k, n) for k, n, _s, _e in init_members]

    top_level = _iter_top_level_papyrus_members(merged_lines)
    assert ("event", "ondestructionstagechanged") in [
        (k, n) for k, n, _s, _e in top_level
    ]


def test_production_merge_native_compiles_for_fo4():
    base_source = _fo4_base_source()
    if base_source is None:
        pytest.skip("FO4 base Papyrus sources unavailable")
    if not PARENT_SOURCE_DIR.is_dir() or not DEPLOYED_SCRIPTS_DIR.is_dir():
        pytest.skip("SeventySix generated/deployed script directories unavailable")

    result = compile_psc(
        _merged_production_source(),
        # FO4 base first, then the mod's own generated custom-parent source
        # (HandScannerScript.psc), then the deployed compiled Scripts dir so the
        # grandparent (RestrictedAccessScript — PEX-only, never decompiled to
        # Source/User because the decompiler pass only covers record-bound
        # scripts) resolves from its compiled bytecode.
        imports=[str(base_source), str(PARENT_SOURCE_DIR), str(DEPLOYED_SCRIPTS_DIR)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=f"{SCRIPT_NAME}.psc",
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None
