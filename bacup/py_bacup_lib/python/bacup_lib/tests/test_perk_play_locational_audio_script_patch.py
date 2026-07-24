from __future__ import annotations

from pathlib import Path

from bacup_lib.tests.test_terminal_fragment_script_patches import _fo4_base_source
from bacup_lib.workflows.unified import (
    _iter_top_level_papyrus_members,
    _merge_script_method_patches,
    _script_patch_source,
)
from creation_lib.pex.native_runtime import compile_psc


REPO_ROOT = Path(__file__).resolve().parents[5]
SCRIPT_NAME = "PerkPlayLocationalAudio"
SOURCE_PATH = (
    REPO_ROOT
    / "mods"
    / "SeventySix"
    / "Scripts"
    / "Source"
    / "User"
    / f"{SCRIPT_NAME}.psc"
)


def _merged_source() -> str:
    patch = _script_patch_source(SCRIPT_NAME)
    assert patch is not None
    assert SOURCE_PATH.is_file(), SOURCE_PATH
    return _merge_script_method_patches(
        SOURCE_PATH.read_text(encoding="utf-8"), patch
    )


def _member_body(source: str, member_name: str) -> str:
    lines = source.splitlines()
    start, end = next(
        (start, end)
        for _kind, name, start, end in _iter_top_level_papyrus_members(lines)
        if name == member_name.lower()
    )
    return "\n".join(lines[start : end + 1])


def test_distance_helper_uses_the_fo4_player_api_and_guards_the_result():
    body = _member_body(_merged_source(), "getwaittimefordistance")
    player_lookup = "localPlayer = Game.GetPlayer()"
    player_guard = "If localPlayer == None"

    assert "GetLocalPlayer" not in body
    assert body.index(player_lookup) < body.index(player_guard)
    assert body.index(player_guard) < body.index("localPlayer.GetPositionX()")


def test_timer_restores_the_optional_perk_short_circuit():
    body = _member_body(_merged_source(), "ontimer")

    assert "GetLocalPlayer" not in body
    assert "GetCurrentRealTime" not in body
    assert "If RequiredPerk == None || localPlayer.HasPerk(RequiredPerk)" in body
    assert body.index("If localPlayer == None") < body.index("localPlayer.GetPositionX()")


def test_load_initializes_the_fo4_timer_countdown():
    body = _member_body(_merged_source(), "onload")

    assert body.index("LastSoundTimestamp = 0.0") < body.index("Self.StartTimer")
    assert body.index("LastDelay = TimerFrequency") < body.index("Self.StartTimer")


def test_activation_uses_the_fo4_player_api():
    body = _member_body(_merged_source(), "onactivate")

    assert "Game.GetPlayer()" in body
    assert "GetLocalPlayer" not in body


def test_perk_play_locational_audio_full_merged_source_native_compiles_for_fo4():
    base_source = _fo4_base_source()
    assert base_source is not None, "FO4 base Papyrus sources unavailable"

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
