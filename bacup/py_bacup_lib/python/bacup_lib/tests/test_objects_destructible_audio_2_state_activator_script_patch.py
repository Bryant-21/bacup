from __future__ import annotations

import os
from pathlib import Path

import pytest

from bacup_lib.workflows.unified import (
    _iter_top_level_papyrus_members,
    _merge_script_method_patches,
    _script_patch_source,
)
from creation_lib.pex.native_runtime import compile_psc


SCRIPT_NAME = "Objects:DestructibleAudio2StateActivator"
REPO_ROOT = Path(__file__).resolve().parents[5]

_SKELETON = """Scriptname Objects:DestructibleAudio2StateActivator Extends DefaultDestructible2StateActivator default conditional

Int PlaybackInstanceID = 0
Int TimerID_SoundOffDelay = 451

sound[] Property SoundsToPlay Auto mandatory
Float Property Delay = 0.0 Auto

Function StopSoundOnClient()
	If PlaybackInstanceID > 0
		sound.StopInstance(PlaybackInstanceID)
		PlaybackInstanceID = 0
	EndIf
EndFunction

Function PlaySoundOnClient()
	Int randomIndex
	sound chosenSound
	If PlaybackInstanceID == 0
		If SoundsToPlay.Length > 0
			randomIndex = utility.RandomInt(0, SoundsToPlay.Length - 1)
			chosenSound = SoundsToPlay[randomIndex]
			PlaybackInstanceID = chosenSound.Play(Self as objectreference)
		EndIf
	EndIf
EndFunction

Event OnUnload()
	Self.StopSoundOnClient()
EndEvent

State closing
	Event OnBeginState(String asOldState)
		If Delay > 0.0
			Self.StartTimer(Delay, TimerID_SoundOffDelay)
		Else
			Self.StopSoundOnClient()
		EndIf
		parent.OnBeginState(asOldState)
	EndEvent

	Event OnTimer(Int aiTimerID)
		If aiTimerID == TimerID_SoundOffDelay
			Self.StopSoundOnClient()
		EndIf
		parent.OnTimer(aiTimerID)
	EndEvent
EndState

State open
	Event OnBeginState(String asOldState)
		parent.OnBeginState(asOldState)
		If Self.Is3DLoaded()
			Self.PlaySoundOnClient()
		EndIf
	EndEvent

	Event OnLoad()
		parent.OnLoad()
		Self.PlaySoundOnClient()
	EndEvent
EndState

State destroyed
	Event OnBeginState(String asOldState)
		Self.StopSoundOnClient()
		parent.OnBeginState(asOldState)
	EndEvent
EndState
"""


def _patch_source() -> str:
    patch = _script_patch_source(SCRIPT_NAME)
    assert patch is not None
    return patch


def _merged_source() -> str:
    return _merge_script_method_patches(_SKELETON, _patch_source())


def test_patch_has_no_declarations_or_states():
    patch = _patch_source()
    assert "Scriptname" not in patch
    assert "State " not in patch


def test_fo4_parent_bridge_members_are_merged_once():
    top_level_members = [
        (kind, name)
        for kind, name, _start, _end in _iter_top_level_papyrus_members(
            _merged_source().splitlines()
        )
    ]
    members = set(top_level_members)
    expected_members = {
        ("function", "setopen"),
        ("event", "onload"),
        ("event", "onreset"),
        ("event", "ontimer"),
        ("event", "ondestructionstagechanged"),
    }
    assert expected_members <= members
    for member in expected_members:
        assert top_level_members.count(member) == 1


def test_audio_timer_does_not_reenter_the_fo4_open_close_parent_timer():
    patch = _patch_source()
    timer_event = patch.split("Event OnTimer(Int aiTimerID)", 1)[1].split(
        "EndEvent", 1
    )[0]
    assert "aiTimerID == TimerID_SoundOffDelay" in timer_event
    assert "Self.StopSoundOnClient()" in timer_event
    assert "Else\n\t\tparent.OnTimer(aiTimerID)" in timer_event


def test_existing_fo76_state_members_remain_intact_for_future_parent_recovery():
    merged = _merged_source()
    assert merged.count("State closing") == 1
    assert merged.count("State open") == 1
    assert merged.count("State destroyed") == 1
    assert "Event OnUnload()\n\tSelf.StopSoundOnClient()\nEndEvent" in merged


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


def test_merged_child_compiles_against_the_actual_fo4_parent_chain():
    base_source = _fo4_base_source()
    if base_source is None:
        pytest.skip("FO4 base Papyrus sources unavailable")

    result = compile_psc(
        _merged_source(),
        imports=[
            str(REPO_ROOT / "mods" / "SeventySix" / "Scripts" / "Source" / "User"),
            str(base_source),
        ],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path="Objects/DestructibleAudio2StateActivator.psc",
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None
