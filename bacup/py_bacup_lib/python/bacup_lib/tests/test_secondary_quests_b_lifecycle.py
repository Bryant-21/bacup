from __future__ import annotations

from pathlib import Path

import pytest

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

PATCHED_MEMBERS = {
    "TW002SecurityStationScript": {"onmenuitemrun"},
    "MTR07_EarthGeneratorScript": {
        "onaliasinit",
        "onaliasshutdown",
        "objectreference.onactivate",
    },
    "Fragments:Quests:QF_MTR07_Earth_003443FB": {
        "fragment_stage_0010_item_00",
    },
    "Fragments:Quests:QF_BoSZ01_0010D89F": {
        "fragment_stage_0350_item_00",
    },
}


def _member_body(source: str, member_name: str) -> str:
    lines = source.splitlines()
    for _kind, name, start, end in _iter_top_level_papyrus_members(lines):
        if name == member_name.casefold():
            return "\n".join(lines[start:end])
    raise AssertionError(f"missing member {member_name}")


@pytest.mark.parametrize(("script_name", "expected_members"), PATCHED_MEMBERS.items())
def test_secondary_b_patches_merge_once_and_compile(
    script_name: str, expected_members: set[str]
) -> None:
    source_path = SOURCE_ROOT / _script_relative_path(script_name, ".psc")
    skeleton = source_path.read_text(encoding="utf-8")
    patch = _script_patch_source(script_name)

    assert patch is not None
    assert not any(
        line.strip().casefold().startswith("scriptname ")
        for line in patch.splitlines()
    )
    assert not any(
        " property " in f" {line.strip().casefold()} "
        for line in patch.splitlines()
    )

    merged = _merge_script_method_patches(skeleton, patch)
    merged_members = [
        name
        for _kind, name, _start, _end in _iter_top_level_papyrus_members(
            merged.splitlines()
        )
    ]
    for member in expected_members:
        assert merged_members.count(member) == 1
    assert _merge_script_method_patches(merged, patch) == merged

    base_source = _fo4_base_source()
    assert base_source is not None, "FO4 base Papyrus sources unavailable"
    result = compile_psc(
        merged,
        imports=[str(SOURCE_ROOT), str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=str(_script_relative_path(script_name, ".psc")),
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None


def test_tw002_station_uses_the_fo4_terminal_event_contract() -> None:
    patch = _script_patch_source("TW002SecurityStationScript")
    assert patch is not None

    handler = _member_body(patch, "onmenuitemrun")
    assert (
        "Event OnMenuItemRun(Int auiMenuItemID, ObjectReference akTerminalRef)"
        in handler
    )
    assert "Terminal akTerminalBase" not in handler
    assert "ObjectReference akActivatorRef" not in handler
    assert "Quest akQuestInstance" not in handler
    assert "akTerminalRef.HasKeyword(TW002Terminal)" in handler
    assert 'Self.SendCustomEvent("TW002GotTape", None)' in handler


def test_earth_mover_has_a_bounded_noncrafting_core_path() -> None:
    fragments = _script_patch_source(
        "Fragments:Quests:QF_MTR07_Earth_003443FB"
    )
    reactor = _script_patch_source("MTR07_EarthReactorTriggerScript")
    assert fragments is not None
    assert reactor is not None

    stage_10 = _member_body(fragments, "fragment_stage_0010_item_00")
    assert 'Game.GetFormFromFile(0x0015C3, "SeventySix.esm")' in stage_10
    assert "If coreCount < 4" in stage_10
    assert "playerRef.AddItem(ignitionCore, 4 - coreCount, True)" in stage_10
    assert "SetStage(20)" in stage_10
    assert "RemoveItem(IgnitionReactorCore01, 1, True)" in reactor
    assert reactor.index("MTR07_EarthQuestStartKeyword.SendStoryEventAndWait") < (
        reactor.index("GetItemCount(IgnitionReactorCore01)")
    )


def test_earth_mover_power_button_advances_operation_once_ready() -> None:
    generator = _script_patch_source("MTR07_EarthGeneratorScript")
    assert generator is not None

    alias_init = _member_body(generator, "onaliasinit")
    activation = _member_body(generator, "objectreference.onactivate")
    shutdown = _member_body(generator, "onaliasshutdown")
    assert 'Game.GetFormFromFile(0x003DA80F, "SeventySix.esm")' in alias_init
    assert 'RegisterForRemoteEvent(powerButton, "OnActivate")' in alias_init
    assert "earthQuest.IsStageDone(70)" in activation
    assert "!earthQuest.IsStageDone(200)" in activation
    assert "earthQuest.SetStage(200)" in activation
    assert 'UnregisterForRemoteEvent(powerButton, "OnActivate")' in shutdown


def test_bosz01_failure_path_reaches_shared_cleanup() -> None:
    fragments = _script_patch_source("Fragments:Quests:QF_BoSZ01_0010D89F")
    assert fragments is not None

    failed = _member_body(fragments, "fragment_stage_0350_item_00")
    cleanup = _member_body(fragments, "fragment_stage_0400_item_00")
    assert "SetStage(400)" in failed
    assert "playerRef.SetValue(pBoSz01StartedAV, 0.0)" in cleanup
    assert "Stop()" in cleanup


def test_bosz01_starter_allows_a_completed_quest_to_repeat() -> None:
    player = _script_patch_source("BoSZ01PlayerScript")
    assert player is not None

    try_start = _member_body(player, "trystartbosz01")
    assert "BoSZ01.IsRunning()" in try_start
    assert "BoSZ01.IsCompleted()" not in try_start
    assert "BoSZ01_QuestStartKeyword.SendStoryEventAndWait" in try_start
