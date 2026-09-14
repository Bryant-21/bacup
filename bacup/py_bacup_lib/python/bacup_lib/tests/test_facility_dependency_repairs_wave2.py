from __future__ import annotations

import json
from pathlib import Path

import pytest

from bacup_lib.tests.test_terminal_fragment_script_patches import _fo4_base_source
from bacup_lib.workflows.unified import _merge_script_method_patches
from creation_lib.pex import parse_pex
from creation_lib.pex.native_runtime import compile_psc


REPO_ROOT = Path(__file__).resolve().parents[5]
PATCH_ROOT = (
    REPO_ROOT / "bacup" / "py_bacup_lib" / "python" / "bacup_lib" / "script_patches"
)
SOURCE_PEX_ROOT = REPO_ROOT / "extracted" / "fo76" / "scripts" / "client"
FIXTURE_ROOT = Path(__file__).with_name("fixtures")

SKELETONS = {
    "EN06_FireworksLauncherScript": """Scriptname EN06_FireworksLauncherScript Extends ObjectReference

String DejaChannel = "EN06"
Int iTimerID = 1

keyword Property EN06_ActiveFireworksLauncherKeyword Auto mandatory
Float Property fSingleShotPercentage = 0.75 Auto
Int Property iMaxNumShots = 4 Auto
Int Property iMinShots = 1 Auto
Int Property iFireCheckTimerLength = 5 Auto
formlist Property EN06_SingleuseFireworks Auto mandatory
formlist Property EN06_MultiuseFireworks Auto mandatory
""",
    "BS01_InstSwapEnableStatePlus": """Scriptname BS01_InstSwapEnableStatePlus Extends ObjectReference

Struct EnableCriteria
    quest SwapQuest
    Bool CheckBothQuestAndAV = False
    Bool TargetValueLessThanNow = False
    Bool MaintainStateOnLessThanTargetValue = False
    Bool MaintainStateOnGreaterThanTargetValue = True
    Float TargetValue = 1.0
    actorvalue SwapAV
EndStruct

EnableCriteria[] Property EnableStates Auto mandatory
Bool Property ORCriteria = True Auto
String Property DejaChannel Auto
Bool Property EnableObject = True Auto
""",
    "LC080ConditionalElevatorScript": """Scriptname LC080ConditionalElevatorScript Extends ObjectReference

Int testState = 0

Quest Property EN02_MQ_Us Auto mandatory
ObjectReference Property TEMPMarkerRef Auto mandatory
ObjectReference Property LC080ElevatorDoorFoyer Auto mandatory
ObjectReference Property LC080ElevatorDoorExam Auto mandatory
""",
}


def _merged(script_name: str) -> str:
    patch = (PATCH_ROOT / f"{script_name}.psc").read_text(encoding="utf-8")
    merged = _merge_script_method_patches(SKELETONS[script_name], patch)
    assert _merge_script_method_patches(merged, patch) == merged
    return merged


@pytest.mark.parametrize("script_name", sorted(SKELETONS))
def test_facility_wave2_patch_merges_idempotently_and_compiles(script_name: str):
    merged = _merged(script_name)
    base = _fo4_base_source()
    assert base is not None
    result = compile_psc(
        merged,
        imports=[str(base)],
        game="fo4",
        flags=str(base / "Institute_Papyrus_Flags.flg"),
        source_path=f"{script_name}.psc",
    )
    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None


def test_en06_launcher_only_fires_while_the_active_alias_keyword_is_present():
    folded = _merged("EN06_FireworksLauncherScript").casefold()
    assert folded.count("event ontimer(int aitimerid)") == 1
    assert "haskeyword(en06_activefireworkslauncherkeyword)" in folded
    assert (
        """if utility.randomfloat() > fsingleshotpercentage
        fireworks = en06_singleusefireworks
        shotcount = 1
    else
        fireworks = en06_multiusefireworks
        shotcount = utility.randomint(iminshots, imaxnumshots)"""
        in folded
    )
    assert "if fireworks == none" in folded
    assert "if fireworkscount == 0" in folded
    assert "firework.fire(self)" in folded
    assert "starttimer(ifirechecktimerlength as float, itimerid)" in folded


def test_en06_source_pex_documents_the_single_and_multi_list_selection():
    source_pex = SOURCE_PEX_ROOT / "en06_fireworkslauncherscript.pex"
    assert source_pex.is_file()
    script = parse_pex(source_pex).objects[0]
    properties = {prop.name: prop.docstring for prop in script.properties}

    assert "higher than to fire the single shot" in properties["fSingleShotPercentage"]
    assert "fired one at a time" in properties["EN06_SingleuseFireworks"]
    assert "fired back to back" in properties["EN06_MultiuseFireworks"]


def test_bs01_swap_monitor_implements_the_live_quest_and_player_av_contract():
    folded = _merged("BS01_InstSwapEnableStatePlus").casefold()
    patch = (PATCH_ROOT / "BS01_InstSwapEnableStatePlus.psc").read_text(
        encoding="utf-8"
    )
    assert "akcriteria.swapquest.iscompleted()" in folded
    assert "game.getplayer().getvalue(akcriteria.swapav)" in folded
    assert "akcriteria.maintainstateonlessthantargetvalue" in folded
    assert "akcriteria.maintainstateongreaterthantargetvalue" in folded
    assert "if hasquest && (!hasactorvalue || akcriteria.checkbothquestandav)" in folded
    assert "return questmatches && actorvaluematches" in folded
    assert "return questmatches || actorvaluematches" not in folded
    assert "return actorvaluematches" in folded
    assert "TargetValueLessThanNow" not in patch
    assert "if orcriteria" in folded
    assert "if !criterionmatches(enablestates[i])" in folded
    assert "if !enableobject" in folded
    assert "enablenowait()" in folded
    assert "disablenowait()" in folded
    assert folded.count("event ontimer(int aitimerid)") == 1


def test_bs01_source_pex_documents_quest_precedence_and_array_combination():
    source_pex = SOURCE_PEX_ROOT / "bs01_instswapenablestateplus.pex"
    assert source_pex.is_file()
    script = parse_pex(source_pex).objects[0]
    criteria = {member.name: member.docstring for member in script.structs[0].members}
    properties = {prop.name: prop.docstring for prop in script.properties}

    assert "ignored if a SwapAV is set" in criteria["SwapQuest"]
    assert "use both SwapAV and SwapQuest" in criteria["CheckBothQuestAndAV"]
    assert "LESS THAN TargetValue" in criteria["MaintainStateOnLessThanTargetValue"]
    assert (
        "equal to or greater than TargetValue"
        in criteria["MaintainStateOnGreaterThanTargetValue"]
    )
    assert "at least one is valid" in properties["ORCriteria"]
    assert "Otherwise, all values must be true" in properties["ORCriteria"]
    assert "criteria is met, enable" in properties["EnableObject"]


def test_facility_wave2_lifecycle_and_none_guards_are_bounded():
    launcher = _merged("EN06_FireworksLauncherScript").casefold()
    swap = _merged("BS01_InstSwapEnableStatePlus").casefold()

    for folded in (launcher, swap):
        assert "event oninit()" in folded
        assert "event oncellattach()" in folded
        assert "event oncelldetach()" in folded
        assert "canceltimer(" in folded

    assert "if fireworks == none" in launcher
    assert "if firework != none" in launcher
    assert "if enablestates == none || enablestates.length == 0" in swap
    assert "akcriteria.swapquest != none" in swap
    assert "akcriteria.swapav != none" in swap


def test_lc080_routes_the_player_by_authoritative_quest_completion():
    merged = _merged("LC080ConditionalElevatorScript")
    folded = merged.casefold()
    patch = (
        (PATCH_ROOT / "LC080ConditionalElevatorScript.psc")
        .read_text(encoding="utf-8")
        .casefold()
    )

    assert folded.count("event onactivate(objectreference akactivator)") == 1
    assert "if akactivator != playerref" in folded
    assert "en02_mq_us != none && en02_mq_us.iscompleted()" in folded
    assert "en02_mq_us.iscompleted()" in folded
    assert (
        folded.index("en02_mq_us.iscompleted()")
        < folded.index("lc080elevatordoorfoyer.activate(playerref)")
        < folded.index("elseIf lc080elevatordoorexam != none".casefold())
        < folded.index("lc080elevatordoorexam.activate(playerref)")
    )
    assert "tempmarkerref" not in patch
    assert "teststate" not in patch
    assert "setstage(" not in folded
    assert "moveto(" not in folded


def test_lc080_source_pex_documents_the_quest_marker_and_two_door_roles():
    source_pex = SOURCE_PEX_ROOT / "lc080conditionalelevatorscript.pex"
    assert source_pex.is_file()
    script = parse_pex(source_pex).objects[0]
    properties = {prop.name: prop.docstring for prop in script.properties}

    assert "track the player's entrance" in properties["EN02_MQ_Us"]
    assert "XMarkerHeading" in properties["TEMPMarkerRef"]
    assert "load door to the Foyer" in properties["LC080ElevatorDoorFoyer"]
    assert "load door to the Exam Room" in properties["LC080ElevatorDoorExam"]


def test_lc080_frozen_source_live_carrier_and_cell_topology_agree():
    fixture = json.loads(
        (FIXTURE_ROOT / "lc080_conditional_elevator_source_live.json").read_text(
            encoding="utf-8"
        )
    )

    cell = fixture["cell"]
    assert cell["source_child_count"] == cell["live_child_count"] == 16335
    assert cell["source_group_types"] == cell["live_group_types"]

    source_carriers = fixture["source_carriers"]
    live_carriers = fixture["live_carriers"]
    assert source_carriers.keys() == live_carriers.keys() == {"2A3ED6", "2A3ED7"}
    expected_bindings = {
        "2A3ED6": {
            "base": "13C380",
            "quest": "0293A3",
            "marker": "286697",
            "exam_door": "2A793E",
            "foyer_door": "2A793F",
        },
        "2A3ED7": {
            "base": "13C380",
            "quest": "0293A3",
            "marker": "286699",
            "exam_door": "2A7940",
            "foyer_door": "2A7941",
        },
    }
    for form_id, source in source_carriers.items():
        live = live_carriers[form_id]
        assert source["base_plugin"] == "SeventySix.esm"
        assert live["base_plugin"] == "Fallout4.esm"
        source_bindings = {
            key: value for key, value in source.items() if key != "base_plugin"
        }
        live_bindings = {
            key: value for key, value in live.items() if key != "base_plugin"
        }
        assert source_bindings == live_bindings == expected_bindings[form_id]

    expected_teleport_pairs = {
        "2A793A": "2A793E",
        "2A793B": "2A7940",
        "2A793C": "2A793F",
        "2A793D": "2A7941",
        "2A793E": "2A793A",
        "2A793F": "2A793C",
        "2A7940": "2A793B",
        "2A7941": "2A793D",
    }
    assert (
        fixture["source_teleport_pairs"]
        == fixture["live_teleport_pairs"]
        == expected_teleport_pairs
    )
    assert set(fixture["exam_marker_z"].values()) == {-48.0}
    assert set(fixture["foyer_destination_z"].values()) == {-304.001953125}
