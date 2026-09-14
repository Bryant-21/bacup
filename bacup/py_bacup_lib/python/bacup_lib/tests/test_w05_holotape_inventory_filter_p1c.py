from __future__ import annotations

from collections import Counter
from pathlib import Path

from bacup_lib.tests.test_script_patch_conventions import (
    has_unregistered_inventory_handler,
)
from bacup_lib.tests.test_terminal_fragment_script_patches import _fo4_base_source
from bacup_lib.workflows.unified import (
    _iter_top_level_papyrus_members,
    _merge_script_method_patches,
    _script_patch_source,
    _script_relative_path,
)
from creation_lib.pex.native_runtime import compile_psc


REPO_ROOT = Path(__file__).resolve().parents[5]
SOURCE_ROOT = REPO_ROOT / "mods" / "SeventySix" / "Scripts" / "Source" / "User"
SCRIPT_NAME = "W05_HolotapeScript"


def _members(source: str) -> list[tuple[str, int, int]]:
    return [
        (name, start, end)
        for kind, name, start, end in _iter_top_level_papyrus_members(
            source.splitlines()
        )
        if kind in {"function", "event"}
    ]


def _member_body(source: str, member_name: str) -> str:
    lines = source.splitlines()
    start, end = next(
        (start, end)
        for name, start, end in _members(source)
        if name == member_name.casefold()
    )
    return "\n".join(lines[start : end + 1])


def _patch() -> str:
    patch = _script_patch_source(SCRIPT_NAME)
    assert patch is not None
    return patch


def _merged() -> str:
    source_path = SOURCE_ROOT / _script_relative_path(SCRIPT_NAME, ".psc")
    assert source_path.is_file(), source_path
    return _merge_script_method_patches(
        source_path.read_text(encoding="utf-8"), _patch()
    )


def test_w05_holotape_filter_patch_merges_once_and_is_idempotent():
    patch = _patch()
    patch_names = [name for name, _start, _end in _members(patch)]
    assert Counter(patch_names) == Counter(set(patch_names))
    assert not has_unregistered_inventory_handler(patch)

    merged = _merged()
    merged_names = [name for name, _start, _end in _members(merged)]
    for member_name in patch_names:
        assert merged_names.count(member_name) == 1
        assert _member_body(merged, member_name) == _member_body(patch, member_name)
    assert _merge_script_method_patches(merged, patch) == merged


def test_w05_holotape_listener_owns_the_two_exact_trigger_filters():
    patch = _patch()
    reset = _member_body(patch, "resettriggeringtapelistener")
    first = _member_body(patch, "getfirsttriggeringtape")
    second = _member_body(patch, "getsecondtriggeringtape")
    triggering = _member_body(patch, "istriggeringtape")

    assert 'Game.GetFormFromFile(0x00569C98, "SeventySix.esm") as Holotape' in first
    assert 'Game.GetFormFromFile(0x005852F0, "SeventySix.esm") as Holotape' in second
    assert "AddInventoryEventFilter(firstTape)" in reset
    assert "AddInventoryEventFilter(secondTape)" in reset
    assert "secondTape && secondTape != firstTape" in reset
    assert "AddInventoryEventFilter(None)" not in patch
    assert ".AddInventoryEventFilter(" not in patch
    assert "playedTape == GetFirstTriggeringTape()" in triggering
    assert "playedTape == GetSecondTriggeringTape()" in triggering


def test_w05_holotape_filter_lifecycle_is_restart_and_load_safe():
    patch = _patch()
    initialized = _member_body(patch, "onquestinit")
    loaded = _member_body(patch, "actor.onplayerloadgame")
    reset = _member_body(patch, "resettriggeringtapelistener")
    disarm = _member_body(patch, "disarmtriggeringtapelistener")
    quest_reset = _member_body(patch, "onreset")
    shutdown = _member_body(patch, "onquestshutdown")

    assert initialized.count(
        'RegisterForRemoteEvent(player, "OnPlayerLoadGame")'
    ) == 1
    assert initialized.count("ResetTriggeringTapeListener(player)") == 1
    assert loaded.count("ResetTriggeringTapeListener(akSender)") == 1
    assert "akSender == Game.GetPlayer()" in loaded
    assert reset.count("DisarmTriggeringTapeListener()") == 1
    assert reset.count("AddInventoryEventFilter(") == 2
    assert reset.count('RegisterForRemoteEvent(player, "OnItemAdded")') == 1
    assert reset.index("DisarmTriggeringTapeListener()") < reset.index(
        "AddInventoryEventFilter(firstTape)"
    ) < reset.index('RegisterForRemoteEvent(player, "OnItemAdded")')
    assert disarm.count('UnregisterForRemoteEvent(player, "OnItemAdded")') == 1
    assert disarm.count("RemoveAllInventoryEventFilters()") == 1
    assert disarm.index(
        'UnregisterForRemoteEvent(player, "OnItemAdded")'
    ) < disarm.index("RemoveAllInventoryEventFilters()")
    assert quest_reset.count(
        'RegisterForRemoteEvent(player, "OnPlayerLoadGame")'
    ) == 1
    assert quest_reset.count("ResetTriggeringTapeListener(player)") == 1
    assert quest_reset.index(
        'RegisterForRemoteEvent(player, "OnPlayerLoadGame")'
    ) < quest_reset.index("ResetTriggeringTapeListener(player)")
    assert 'UnregisterForRemoteEvent(player, "OnPlayerLoadGame")' not in quest_reset
    assert "DisarmTriggeringTapeListener()" not in quest_reset
    assert shutdown.count(
        'UnregisterForRemoteEvent(player, "OnPlayerLoadGame")'
    ) == 1
    assert shutdown.count("DisarmTriggeringTapeListener()") == 1
    assert shutdown.index(
        'UnregisterForRemoteEvent(player, "OnPlayerLoadGame")'
    ) < shutdown.index("DisarmTriggeringTapeListener()")


def test_w05_holotape_remote_handler_keeps_sender_count_and_dispatch_behavior():
    patch = _patch()
    played = _member_body(patch, "objectreference.onholotapeplay")
    added = _member_body(patch, "objectreference.onitemadded")
    process = _member_body(patch, "processholotape")

    assert "Holotape playedTape = GetPlayedHolotape(akSender)" in played
    assert "ProcessHolotape(playedTape, akSender)" in played
    assert "akSender != Game.GetPlayer() || aiItemCount <= 0" in added
    assert "ProcessHolotape(akBaseItem as Holotape, eventRef)" in added
    assert "If !IsTriggeringTape(playedTape)" in process
    assert (
        "pW05_MQ00_Completed && player.GetValue(pW05_MQ00_Completed) > 0.0"
        in process
    )
    assert (
        "pW05_MQ00_CodeAV && player.GetValue(pW05_MQ00_CodeAV) < 0.0"
        in process
    )
    assert (
        "player.SetValue(pW05_MQ00_CodeAV, Utility.RandomInt(100000, 999999))"
        in process
    )
    assert (
        "pW05_MQ_00P_StartKeyword.SendStoryEventAndWait(akRef1 = eventRef)"
        in process
    )
    assert "!started && W05_MQ_00P && !W05_MQ_00P.IsRunning()" in process
    assert "W05_MQ_00P.Start()" in process


def test_w05_holotape_filter_patch_native_compiles_for_fo4():
    base_source = _fo4_base_source()
    assert base_source is not None, "FO4 base Papyrus sources unavailable"

    result = compile_psc(
        _merged(),
        imports=[str(SOURCE_ROOT), str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=str(_script_relative_path(SCRIPT_NAME, ".psc")),
    )
    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None
