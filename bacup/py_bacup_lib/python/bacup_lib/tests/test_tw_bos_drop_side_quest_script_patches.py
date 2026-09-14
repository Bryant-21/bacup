from __future__ import annotations

import re
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
    "TW002_Script": {
        "onquestinit",
        "registersecuritystation",
        "tw002securitystationscript.tw002securitystationscript_tw002gottape",
    },
    "TW002WardenTalk": {"onactivate"},
    "Fragments:Quests:QF_TW002_0010E201": {
        "fragment_stage_0051_item_00",
        "fragment_stage_0052_item_00",
        "fragment_stage_0053_item_00",
        "fragment_stage_0054_item_00",
        "fragment_stage_0100_item_00",
        "fragment_stage_0200_item_00",
        "fragment_stage_0300_item_00",
        "fragment_stage_1000_item_00",
    },
    "BoSZ01PlayerScript": {
        "onload",
        "onunload",
        "onitemadded",
        "ontimer",
        "trystartbosz01",
    },
    "BoSZ01PlayerAliasScript": {
        "onaliasinit",
        "onplayerloadgame",
        "resettechnicaldocumentlistener",
        "objectreference.onitemremoved",
        "onaliasshutdown",
    },
    "Fragments:Quests:QF_BoSZ01_0010D89F": {
        "fragment_stage_0001_item_00",
        "fragment_stage_0100_item_00",
        "fragment_stage_0150_item_00",
        "fragment_stage_0200_item_00",
        "fragment_stage_0300_item_00",
        "fragment_stage_0310_item_00",
        "fragment_stage_0400_item_00",
    },
    "Fragments:Quests:QF_GQ_DropGovtIntro_00006F3C": {
        "fragment_stage_0100_item_00",
        "fragment_stage_0200_item_00",
        "fragment_stage_0300_item_00",
    },
    "DropGovtIntroRadioStationAliasScript": {
        "onaliasinit",
        "terminal.onmenuitemrun",
        "onaliasshutdown",
    },
    "GQ_DropGovtIntroScript": {
        "onquestinit",
        "objectreference.onitemremoved",
        "ontimer",
        "onquestshutdown",
    },
    "GQ_DropGovt01HolotapeScript": {
        "oninit",
        "oncontainerchanged",
        "requestgovernmentdropstart",
    },
    "GQ_AirDropHolotapeTerminalScript": {"onmenuitemrun"},
}


@pytest.mark.parametrize(("script_name", "expected_members"), PATCHED_MEMBERS.items())
def test_tw_bos_drop_patches_merge_once_and_compile(
    script_name: str, expected_members: set[str]
):
    source_path = SOURCE_ROOT / _script_relative_path(script_name, ".psc")
    skeleton = source_path.read_text(encoding="utf-8")
    patch = _script_patch_source(script_name)

    assert patch is not None
    assert "Scriptname" not in patch
    assert "Property" not in patch

    merged = _merge_script_method_patches(skeleton, patch)
    members = [
        name
        for _kind, name, _start, _end in _iter_top_level_papyrus_members(
            merged.splitlines()
        )
    ]
    for member in expected_members:
        assert members.count(member) == 1
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


def test_tw_bos_drop_patches_preserve_the_recovered_progression_contract():
    tw002 = _script_patch_source("TW002_Script")
    tw002_fragments = _script_patch_source("Fragments:Quests:QF_TW002_0010E201")
    bosz01 = _script_patch_source("BoSZ01PlayerScript")
    bosz01_fragments = _script_patch_source("Fragments:Quests:QF_BoSZ01_0010D89F")
    govt_station_alias = _script_patch_source("DropGovtIntroRadioStationAliasScript")
    govt_intro = _script_patch_source("GQ_DropGovtIntroScript")
    govt_terminal = _script_patch_source("GQ_AirDropHolotapeTerminalScript")
    drop_fragments = _script_patch_source(
        "Fragments:Quests:QF_GQ_DropGovtIntro_00006F3C"
    )

    assert tw002 is not None
    assert tw002_fragments is not None
    assert bosz01 is not None
    assert bosz01_fragments is not None
    assert govt_station_alias is not None
    assert govt_intro is not None
    assert govt_terminal is not None
    assert drop_fragments is not None
    assert (
        'RegisterForCustomEvent(station, "tw002securitystationscript_TW002GotTape")'
        in tw002
    )
    assert "SetObjectiveDisplayed(200, True)" in tw002_fragments
    assert "SetObjectiveDisplayed(300, True)" in tw002_fragments
    assert "Self as TW002_Script" not in tw002_fragments
    assert (
        "IsStageDone(51) && IsStageDone(52) && IsStageDone(53) && IsStageDone(54)"
        in tw002_fragments
    )
    assert "SetStage(300)" in tw002_fragments
    assert "AddInventoryEventFilter(BoSTechnicalDocument)" in bosz01
    assert "RemoveInventoryEventFilter(BoSTechnicalDocument)" in bosz01
    assert bosz01.index("AddInventoryEventFilter(BoSTechnicalDocument)") < bosz01.index(
        "Event OnItemAdded"
    )
    assert "BoSZ01_QuestStartKeyword.SendStoryEventAndWait(" in bosz01
    assert not re.search(r"BoSZ01_QuestStartKeyword\.SendStoryEvent\s*\(", bosz01)
    assert not re.search(
        r"=\s*BoSZ01_QuestStartKeyword\.SendStoryEventAndWait\s*\(", bosz01
    )
    send_index = bosz01.index("BoSZ01_QuestStartKeyword.SendStoryEventAndWait(")
    first_running_check = bosz01.index("BoSZ01.IsRunning()")
    second_running_check = bosz01.index("!BoSZ01.IsRunning()")
    assert first_running_check < send_index < second_running_check
    assert "BoSZ01.IsCompleted()" not in bosz01
    assert "GetItemCount(BoSTechnicalDocument) <= 0" in bosz01
    assert "GetItemCount(BoSTechnicalDocument) > 0" in bosz01
    assert "StartTimer(1.0, 1)" in bosz01
    assert bosz01.count("TryStartBoSZ01()") == 4
    assert "SetStage(310)" in bosz01_fragments
    assert (
        'RegisterForRemoteEvent(stationTerminal, "OnMenuItemRun")' in govt_station_alias
    )
    assert "AddInventoryEventFilter(GQ_DropGovt01Holotape)" in govt_intro
    assert "RemoveAllInventoryEventFilters()" in govt_intro
    assert "SetStage(Stage_HolotapeUsed)" in govt_intro
    assert govt_intro.count("CancelTimer(holotapeRemovedTimerID)") == 2
    assert govt_intro.index("CancelTimer(holotapeRemovedTimerID)") < govt_intro.index(
        "StartTimer(holotapeRemovedTime, holotapeRemovedTimerID)"
    )
    assert "akTerminalRef.HasRefType(RadioStationRefType)" in govt_terminal
    assert "GQ_DropGovt01Keyword.SendStoryEvent" in govt_terminal
    assert "SetObjectiveDisplayed(100, True)" in drop_fragments
    assert "SetObjectiveCompleted(200, True)" in drop_fragments


def test_superseded_govt_holotape_player_script_has_no_durable_patch():
    assert _script_patch_source("GQ_GovtHolotape_PlayerScript") is None

    live_intro = _script_patch_source("GQ_DropGovtIntroScript")
    assert live_intro is not None
    assert 'RegisterForRemoteEvent(playerRef, "OnItemRemoved")' in live_intro
    assert "AddInventoryEventFilter(GQ_DropGovt01Holotape)" in live_intro
    assert 'UnregisterForRemoteEvent(playerRef, "OnItemRemoved")' in live_intro
    assert "RemoveAllInventoryEventFilters()" in live_intro
    assert "GQ_DropGovt01Holotape" in live_intro

    init = live_intro.split("Event OnQuestInit()", 1)[1].split("EndEvent", 1)[0]
    shutdown = live_intro.split("Event OnQuestShutdown()", 1)[1].split(
        "EndEvent", 1
    )[0]
    assert init.index('RegisterForRemoteEvent(playerRef, "OnItemRemoved")') < init.index(
        "AddInventoryEventFilter(GQ_DropGovt01Holotape)"
    )
    assert shutdown.index(
        'UnregisterForRemoteEvent(playerRef, "OnItemRemoved")'
    ) < shutdown.index("RemoveAllInventoryEventFilters()")
