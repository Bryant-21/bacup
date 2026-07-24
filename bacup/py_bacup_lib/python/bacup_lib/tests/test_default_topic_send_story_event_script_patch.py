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
SOURCE_PATH = (
    REPO_ROOT
    / "mods"
    / "SeventySix"
    / "Scripts"
    / "Source"
    / "User"
    / "DefaultTopicSendStoryEvent.psc"
)
SCRIPT_NAME = "DefaultTopicSendStoryEvent"
PATCH_MEMBERS = {"onbegin", "onend", "sendevent"}


def _member_body(source: str, member_name: str) -> str:
    lines = source.splitlines()
    start, end = next(
        (start, end)
        for _kind, name, start, end in _iter_top_level_papyrus_members(lines)
        if name == member_name.lower()
    )
    return "\n".join(lines[start : end + 1])


def _member_names(source: str) -> list[str]:
    return [
        name
        for _kind, name, _start, _end in _iter_top_level_papyrus_members(
            source.splitlines()
        )
    ]


def _merged_production_source() -> str:
    patch = _script_patch_source(SCRIPT_NAME)
    assert SOURCE_PATH.is_file(), SOURCE_PATH
    assert patch is not None
    return _merge_script_method_patches(
        SOURCE_PATH.read_text(encoding="utf-8"), patch
    )


def test_patch_is_member_only():
    patch = _script_patch_source(SCRIPT_NAME)
    assert patch is not None
    assert set(_member_names(patch)) == PATCH_MEMBERS
    assert not any(
        line.strip().lower().startswith(("scriptname ", "state "))
        for line in patch.splitlines()
    )
    assert not any(
        " property " in f" {line.strip().lower()} " for line in patch.splitlines()
    )


def test_onbegin_and_onend_honor_send_on_end():
    patch = _script_patch_source(SCRIPT_NAME)
    assert patch is not None

    on_begin = _member_body(patch, "onbegin")
    assert "If !SendOnEnd" in on_begin
    assert on_begin.count("SendEvent(akSpeakerRef)") == 1

    on_end = _member_body(patch, "onend")
    assert "If SendOnEnd" in on_end
    assert "If !SendOnEnd" not in on_end
    assert on_end.count("SendEvent(akSpeakerRef)") == 1


def test_send_event_preserves_single_player_property_fallbacks():
    patch = _script_patch_source(SCRIPT_NAME)
    assert patch is not None
    send = _member_body(patch, "sendevent")

    assert "If EventKeyword == None" in send
    assert "Location eventLocation = akLoc" in send
    assert "eventLocation = akSpeakerRef.GetCurrentLocation()" in send
    assert "ObjectReference eventRef1 = akRef1" in send
    assert "eventRef1 = akSpeakerRef" in send
    assert "ObjectReference eventRef2 = akRef2" in send
    assert "eventRef2 = Game.GetPlayer()" in send
    assert (
        "EventKeyword.SendStoryEvent(eventLocation, eventRef1, eventRef2, "
        "aiValue1, aiValue2)"
    ) in send
    assert "AddNearbyPlayers" not in send
    assert "AddPlayersToSameInstance" not in send


def test_production_merge_is_unique_and_idempotent():
    patch = _script_patch_source(SCRIPT_NAME)
    assert patch is not None
    merged = _merged_production_source()
    names = _member_names(merged)

    for member_name in PATCH_MEMBERS:
        assert names.count(member_name) == 1
        assert _member_body(merged, member_name) == _member_body(patch, member_name)
    assert _merge_script_method_patches(merged, patch) == merged


def test_full_production_merge_native_compiles_for_fo4():
    base_source = _fo4_base_source()
    assert base_source is not None, "FO4 base Papyrus sources unavailable"

    result = compile_psc(
        _merged_production_source(),
        imports=[str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path="DefaultTopicSendStoryEvent.psc",
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None
