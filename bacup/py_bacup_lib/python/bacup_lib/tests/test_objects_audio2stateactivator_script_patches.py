from __future__ import annotations

import re
import os
from pathlib import Path

import pytest

from bacup_lib.workflows.unified import (
    _merge_script_method_patches,
    _script_patch_source,
)
from creation_lib.pex.native_runtime import compile_psc


REPO_ROOT = Path(__file__).resolve().parents[5]
SCRIPT_NAME = "Objects:Audio2StateActivator"
SOURCE_PATH = (
    REPO_ROOT
    / "mods"
    / "SeventySix"
    / "Scripts"
    / "Source"
    / "User"
    / "Objects"
    / "Audio2StateActivator.psc"
)


def _merged_source() -> str:
    patch = _script_patch_source(SCRIPT_NAME)
    assert patch is not None
    return _merge_script_method_patches(SOURCE_PATH.read_text(encoding="utf-8"), patch)


def _state_block(source: str, state_name: str) -> str:
    match = re.search(
        rf"^[ \t]*(?:Auto\s+)?State\s+{re.escape(state_name)}\b[^\n]*\n"
        rf"(?P<body>.*?)"
        rf"^[ \t]*EndState\b",
        source,
        re.IGNORECASE | re.MULTILINE | re.DOTALL,
    )
    assert match is not None, f"state {state_name!r} not found"
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


def test_initial_state_synchronizes_an_optional_linked_audio_reference():
    patch = _script_patch_source(SCRIPT_NAME)
    assert patch is not None
    assert not any(
        line.strip().lower().startswith("scriptname ") for line in patch.splitlines()
    )

    initial = _state_block(_merged_source(), "Initial")
    assert initial.count("Event OnInit()") == 1
    assert "ObjectReference linkedAudio = GetLinkedRef()" in initial
    assert "If linkedAudio != None" in initial
    assert "If IsOpen" in initial
    assert "linkedAudio.Enable(False)" in initial
    assert "linkedAudio.Disable(False)" in initial
    assert "Self.GetLinkedRef" not in initial
    assert "parent.OnInit" not in initial


def test_audio_transition_handlers_remain_in_their_named_states():
    merged = _merged_source()
    closing = _state_block(merged, "closing")
    opening = _state_block(merged, "open")

    assert closing.count("Event OnBeginState") == 1
    assert closing.count("Event OnTimer") == 1
    assert "StartTimer(Delay, SoundOffDelay)" in closing
    assert "linkedAudio.Disable(False)" in closing
    assert opening.count("Event OnBeginState") == 1
    assert "CancelTimer(SoundOffDelay)" in opening
    assert "linkedAudio.Enable(False)" in opening


def test_merged_script_native_compiles_for_fo4():
    base_source = _fo4_base_source()
    if base_source is None:
        pytest.skip("FO4 base Papyrus sources unavailable")

    result = compile_psc(
        _merged_source(),
        imports=[str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path="Objects/Audio2StateActivator.psc",
    )
    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None
