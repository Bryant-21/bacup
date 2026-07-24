from __future__ import annotations

import os
from pathlib import Path

import pytest

from bacup_lib.workflows.unified import _merge_script_method_patches, _script_patch_source
from creation_lib.pex.native_runtime import compile_psc


REPO_ROOT = Path(__file__).resolve().parents[5]
SCRIPT_NAME = "Objects:UD002OldTunnelTerminal"
SKELETON = """Scriptname Objects:UD002OldTunnelTerminal Extends ObjectReference

default2stateactivator myActivator
Int SteamTimerID = 666
Int DoorTimerID = 451
Bool bDoorToggle = False conditional
ObjectReference ValveRef
ObjectReference ValveSteamRef
ObjectReference DoorRef

keyword Property LinkCustom01 Auto
sound Property SteamSound Auto
Float Property SteamTimerLength = 5.0 Auto
Float Property DoorTimerLength = 15.0 Auto
keyword Property LinkCustom03 Auto
keyword Property LinkCustom02 Auto
"""


def _patch() -> str:
    patch = _script_patch_source(SCRIPT_NAME)
    assert patch is not None
    return patch


def _merged_source() -> str:
    return _merge_script_method_patches(SKELETON, _patch())


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


def test_patch_merges_the_valve_listener_and_independent_timers_once():
    patch = _patch()
    merged = _merged_source()

    assert not any(
        line.strip().lower().startswith("scriptname ") for line in patch.splitlines()
    )
    for member in (
        "Event OnInit()",
        "Event ObjectReference.OnActivate(ObjectReference akSender, ObjectReference akActivator)",
        "Event OnTimer(Int aiTimerID)",
        "Event OnReset()",
    ):
        assert merged.count(member) == 1
    assert 'RegisterForRemoteEvent(ValveRef, "OnActivate")' in merged
    assert "StartTimer(SteamTimerLength, SteamTimerID)" in merged
    assert "StartTimer(DoorTimerLength, DoorTimerID)" in merged
    assert "ValveSteamRef.DisableNoWait()" in merged
    assert "bDoorToggle = False" in merged


def test_merged_patch_compiles_against_fo4_base_sources():
    base_source = _fo4_base_source()
    if base_source is None:
        pytest.skip("FO4 base Papyrus sources unavailable")

    result = compile_psc(
        _merged_source(),
        imports=[str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path="Objects/UD002OldTunnelTerminal.psc",
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None
