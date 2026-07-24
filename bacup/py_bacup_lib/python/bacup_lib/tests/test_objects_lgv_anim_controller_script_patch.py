from __future__ import annotations

import os
import re
from pathlib import Path

import pytest

from bacup_lib.workflows.unified import _merge_script_method_patches, _script_patch_source
from creation_lib.pex.native_runtime import compile_psc


REPO_ROOT = Path(__file__).resolve().parents[5]
SCRIPT_NAME = "Objects:LGVAnimController"

_SKELETON = """Scriptname Objects:LGVAnimController Extends ObjectReference

Actor UsingPlayer

String Property JumpToOn = "JumpState02" Auto
String Property JumpToOff = "JumpState01" Auto
String Property TransOnToOff = "Play02" Auto

Function TurnOff(Actor akSendingPlayer)
EndFunction

State on

    Event OnBeginState(String asOldState)
        Self.PlayAnimation(JumpToOn)
        Self.RegisterForMenuOpenCloseEvent("NPCVendingMenu")
    EndEvent

    Event OnMenuOpenCloseEvent(String asMenuName, Bool abOpening)
        Bool temp9
        Bool temp12
        Var[] tmp
        If !abOpening as Bool
            temp9 = (asMenuName == "NPCVendingMenu") as Bool
        EndIf
        If temp9 as Bool
            temp12 = (UsingPlayer == game.GetPlayer()) as Bool
        EndIf
        If temp12
            tmp = new Var[0]
            Self.SendRMIToServer("TurnOff", tmp)
        EndIf
    EndEvent

    Event OnLoad()
        Self.PlayAnimation(JumpToOn)
    EndEvent

EndState

Auto State Off
    Event OnBeginState(String asOldState)
        Self.PlayAnimation(TransOnToOff)
    EndEvent

    Event OnActivate(ObjectReference akActivator)
        If akActivator.IsAPlayer()
            UsingPlayer = akActivator as Actor
        EndIf
    EndEvent

    Event OnLoad()
        Self.PlayAnimation(JumpToOff)
    EndEvent

EndState
"""


def _merged_source() -> str:
    patch = _script_patch_source(SCRIPT_NAME)
    assert patch is not None
    return _merge_script_method_patches(_SKELETON, patch)


def _state_block(source: str, state_name: str) -> str:
    match = re.search(
        rf"^[ \t]*(?:Auto\s+)?State\s+{re.escape(state_name)}\b[^\n]*\n"
        rf"(?P<body>.*?)^[ \t]*EndState\b",
        source,
        re.IGNORECASE | re.MULTILINE | re.DOTALL,
    )
    assert match is not None
    return match.group("body")


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


def test_lgv_patch_replaces_online_menu_calls_with_local_safe_members():
    patch = _script_patch_source(SCRIPT_NAME)
    assert patch is not None
    assert "RegisterForMenuOpenCloseEvent" not in patch
    assert "SendRMIToServer" not in patch
    assert "IsAPlayer" not in patch

    merged = _merged_source()
    assert "RegisterForMenuOpenCloseEvent" not in merged
    assert "SendRMIToServer" not in merged
    assert "IsAPlayer" not in merged
    assert "GoToState(\"Off\")" in merged
    assert "akSendingPlayer == Game.GetPlayer()" in merged
    assert "akSendingPlayer == UsingPlayer" in merged
    assert "UsingPlayer = None" in merged
    assert "PlayAnimation(JumpToOn)" in _state_block(merged, "on")
    assert "akActivator == Game.GetPlayer()" in _state_block(merged, "Off")


def test_lgv_production_merge_native_compiles_for_fo4():
    base_source = _fo4_base_source()
    if base_source is None:
        pytest.skip("FO4 base Papyrus sources unavailable")

    result = compile_psc(
        _merged_source(),
        imports=[str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path="Objects/LGVAnimController.psc",
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None
