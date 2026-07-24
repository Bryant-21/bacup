from __future__ import annotations

import os
import re
from pathlib import Path

import pytest

from bacup_lib.workflows.unified import (
    _iter_top_level_papyrus_members,
    _merge_script_method_patches,
    _script_patch_source,
)
from creation_lib.pex.native_runtime import compile_psc


REPO_ROOT = Path(__file__).resolve().parents[5]
SCRIPT_NAME = "Default2StateSyncActivator"

# Mirrors mods/SeventySix/Scripts/Source/User/default2statesyncactivator.psc
# (canonical spelling per the merger's case-insensitive match / preserve-source-
# spelling rule) verbatim: same Extends type, same 21 properties, same 7 named
# states with their pre-existing (non-hollow) members intact.
_SKELETON = """Scriptname Default2StateSyncActivator Extends ObjectReference

Int CONST_CloseAnimationEndEventID = 3
Int CONST_OpenAnimationEndEventID = 1

keyword Property TwoStateCollisionKeyword Auto mandatory
sound Property SyncCloseSound Auto
sound Property SyncOpenSound Auto
Int Property CONST_AutoCloseTimerID
\tInt Function Get()
\t\tReturn 3
\tEndFunction
EndProperty
Int Property CONST_CloseTimerID
\tInt Function Get()
\t\tReturn 2
\tEndFunction
EndProperty
Int Property CONST_OpenTimerID
\tInt Function Get()
\t\tReturn 1
\tEndFunction
EndProperty
Bool Property hasDoneOnce Auto hidden
Bool Property InvertCollision Auto
Float Property AutoCloseDelay Auto
Bool Property ShouldAutoClose Auto
Bool Property ShouldDoOnce Auto
Float Property SyncCloseDuration Auto
Bool Property IsOpen Auto
Int Property OpenState Auto hidden
String Property SyncCloseProgressVariable Auto
globalvariable Property DefaultAutoCloseDelay Auto mandatory
keyword Property HasSyncAnimation Auto mandatory
String Property SyncOpenAnim Auto
String Property SyncOpenProgressVariable Auto
Float Property SyncOpenDuration Auto
String Property SyncCloseAnim Auto

Function SetAutoClose(Bool abAutoClose)
\tShouldAutoClose = abAutoClose
EndFunction

Function PlaySyncSFXOnClients()
EndFunction

Function SetActivatorOpenAndWait(Bool abOpen)
EndFunction

Function SetActivatorOpen(Bool abOpen)
EndFunction

Function StartAutoCloseTimer()
EndFunction

State closing
\tFunction PlaySyncSFXOnClients()
\t\tSyncCloseSound.Play(Self as ObjectReference)
\tEndFunction

EndState

Auto State Initial
EndState

State open
EndState

State startsclosed
EndState

State opening
\tFunction PlaySyncSFXOnClients()
\t\tSyncOpenSound.Play(Self as ObjectReference)
\tEndFunction

EndState

State closed
EndState

State startsopen
EndState
"""


def _merged_source() -> str:
    patch = _script_patch_source(SCRIPT_NAME)
    assert patch is not None
    return _merge_script_method_patches(_SKELETON, patch)


def _state_block(source: str, state_name: str) -> str:
    """Return the text between `State <state_name>` (optionally `Auto State`) and
    its matching `EndState` — state names never nest or repeat in valid Papyrus.

    The declaration-line suffix uses `[^\\n]*` rather than a DOTALL `.*` — a
    greedy DOTALL `.*` would backtrack past the intended state and match the
    *last* `EndState` in the file instead of the nearest one.
    """
    pattern = re.compile(
        rf"^[ \t]*(?:Auto\s+)?State\s+{re.escape(state_name)}\b[^\n]*\n"
        rf"(?P<body>.*?)"
        rf"^[ \t]*EndState\b",
        re.IGNORECASE | re.MULTILINE | re.DOTALL,
    )
    match = pattern.search(source)
    assert match is not None, f"state {state_name!r} not found in merged source"
    return match.group("body")


def test_patch_exists_with_no_scriptname_line():
    patch = _script_patch_source(SCRIPT_NAME)
    assert patch is not None
    assert not any(
        line.strip().lower().startswith("scriptname ") for line in patch.splitlines()
    )


def test_patch_supplies_the_three_hollow_top_level_callables():
    patch = _script_patch_source(SCRIPT_NAME)
    assert patch is not None
    members = {
        name
        for kind, name, _start, _end in _iter_top_level_papyrus_members(
            patch.splitlines()
        )
        if kind == "function"
    }
    assert {
        "setactivatoropen",
        "setactivatoropenandwait",
        "startautoclosetimer",
    } <= members


def test_merged_source_drops_the_hollow_stub_bodies():
    merged = _merged_source()
    assert "Function SetActivatorOpen(Bool abOpen)\nEndFunction" not in merged
    assert "Function SetActivatorOpenAndWait(Bool abOpen)\nEndFunction" not in merged
    assert "Function StartAutoCloseTimer()\nEndFunction" not in merged


def test_repaired_top_level_members_appear_exactly_once():
    merged = _merged_source()
    for name in (
        "Function SetActivatorOpen(",
        "Function SetActivatorOpenAndWait(",
        "Function StartAutoCloseTimer(",
        "Function ReconcileSyncState(",
        "Event OnInit(",
        "Event OnLoad(",
        "Event OnReset(",
    ):
        assert merged.count(name) == 1, f"{name} should appear exactly once"


@pytest.mark.parametrize(
    ("state_name", "member_name"),
    [
        ("opening", "Event OnBeginState"),
        ("opening", "Event OnTimer"),
        ("closing", "Event OnBeginState"),
        ("closing", "Event OnTimer"),
        ("open", "Event OnBeginState"),
        ("open", "Event OnEndState"),
        ("open", "Event OnTimer"),
        ("startsopen", "Event OnBeginState"),
        ("startsclosed", "Event OnBeginState"),
    ],
)
def test_repaired_member_lands_in_its_intended_state(
    state_name: str, member_name: str
):
    block = _state_block(_merged_source(), state_name)
    assert member_name in block, f"{member_name} missing from State {state_name}"


def test_preexisting_state_local_sound_overrides_survive_the_merge():
    merged = _merged_source()
    opening = _state_block(merged, "opening")
    closing = _state_block(merged, "closing")
    assert "SyncOpenSound.Play(Self as ObjectReference)" in opening
    assert "SyncCloseSound.Play(Self as ObjectReference)" in closing
    # Exactly one PlaySyncSFXOnClients per state — the patch must not duplicate them.
    assert opening.count("Function PlaySyncSFXOnClients()") == 1
    assert closing.count("Function PlaySyncSFXOnClients()") == 1


def test_closed_and_initial_states_are_untouched():
    merged = _merged_source()
    assert _state_block(merged, "closed").strip() == ""
    assert _state_block(merged, "Initial").strip() == ""


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


def test_production_merge_native_compiles_for_fo4():
    base_source = _fo4_base_source()
    if base_source is None:
        pytest.skip("FO4 base Papyrus sources unavailable")

    result = compile_psc(
        _merged_source(),
        imports=[str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=f"{SCRIPT_NAME}.psc",
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None
