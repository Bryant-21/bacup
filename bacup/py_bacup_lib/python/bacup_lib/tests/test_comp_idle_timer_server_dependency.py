from __future__ import annotations

import hashlib
import json
from pathlib import Path

import pytest

from bacup_lib.tests.test_terminal_fragment_script_patches import _fo4_base_source
from bacup_lib.workflows.unified import _script_patch_source
from creation_lib.pex import parse_pex
from creation_lib.pex.native_runtime import compile_psc


REPO_ROOT = Path(__file__).resolve().parents[5]
CONTRACT = (
    REPO_ROOT
    / "bacup"
    / "docs"
    / "stub_restoration"
    / "contracts"
    / "comp-idle-timer-server-dependency-2026-09-01.md"
)
LOCAL_SOURCE_PEX = (
    REPO_ROOT
    / "extracted"
    / "fo76"
    / "scripts"
    / "client"
    / "COMP_IdleTimerScript.pex"
)
FIXTURE = (
    Path(__file__).with_name("fixtures") / "comp_idle_timer_client_surfaces.json"
)

SKELETON = """Scriptname COMP_IdleTimerScript Extends Actor

ActorValue Property IdleChatterTimeMax Auto mandatory
GlobalVariable Property COMP_IdleChatterTimeMin Auto mandatory
GlobalVariable Property COMP_IdleChatterTimeMax Auto mandatory
ActorValue Property IdleChatterTimeMin Auto mandatory
"""


def _fixture() -> dict:
    return json.loads(FIXTURE.read_text(encoding="utf-8"))


def test_idle_timer_frozen_client_surfaces_match_logically() -> None:
    fixture = _fixture()
    retail = fixture["retail"]
    playtest = fixture["playtest"]
    surface = fixture["logical_surface"]

    assert retail["archive_member"] == playtest["archive_member"]
    assert retail["byte_size"] == playtest["byte_size"] == 663
    assert len(retail["sha256"]) == len(playtest["sha256"]) == 64
    assert retail["sha256"] != playtest["sha256"]
    assert surface["object_name"] == "COMP_IdleTimerScript"
    assert surface["parent"] == "Actor"
    assert surface["is_const"] is True
    assert surface["auto_state"] == ""
    assert surface["guard_count"] == 0
    assert surface["debug_function_count"] == 0
    assert surface["state_function_count"] == 0
    assert surface["properties"] == {
        "COMP_IdleChatterTimeMin": "globalvariable",
        "COMP_IdleChatterTimeMax": "globalvariable",
        "IdleChatterTimeMin": "actorvalue",
        "IdleChatterTimeMax": "actorvalue",
    }


def test_idle_timer_client_pex_contains_no_recoverable_member_abi() -> None:
    if not LOCAL_SOURCE_PEX.is_file():
        pytest.skip("local FO76 client PEX evidence is unavailable")

    fixture = _fixture()
    expected = fixture["logical_surface"]
    assert hashlib.sha256(LOCAL_SOURCE_PEX.read_bytes()).hexdigest().upper() == (
        fixture["retail"]["sha256"]
    )

    pex = parse_pex(LOCAL_SOURCE_PEX)
    assert len(pex.objects) == 1
    script = pex.objects[0]

    assert script.name == expected["object_name"]
    assert script.parent == expected["parent"]
    assert script.is_const is expected["is_const"]
    assert script.auto_state == expected["auto_state"]
    assert {prop.name: prop.type for prop in script.properties} == expected["properties"]
    assert len(script.guards) == expected["guard_count"]
    assert len(script.states) == 1
    assert script.states[0].name == expected["auto_state"]
    assert len(script.states[0].functions) == expected["state_function_count"]
    assert len(pex.debug_info.functions) == expected["debug_function_count"]
    assert len(script.variables) == len(expected["properties"])

    symbols = {value.casefold() for value in pex.string_table}
    assert set(expected["forbidden_symbols"]).isdisjoint(symbols)


def test_idle_timer_dependency_has_no_guessed_durable_patch() -> None:
    assert _script_patch_source("COMP_IdleTimerScript") is None


def test_idle_timer_contract_names_the_exact_server_dependency_and_negatives() -> None:
    source = CONTRACT.read_text(encoding="utf-8")
    normalized = " ".join(source.split())

    for required in (
        "32 NPC bases",
        "Seven carriers",
        "zero functions, events, guards, or debug-function entries",
        "FollowersScript.CompanionDataToggle",
        "support, but do not prove, the likely action direction",
        "member body or native consumer contract",
        "unresolved producer/server-runtime dependency",
        "Do not add `OnInit`, `OnLoad`, `OnCellLoad`, `OnCellAttach`, or `OnReset`",
        "Do not call `StartTimer`",
        "Do not call `EvaluatePackage`",
    ):
        assert required in normalized


def test_idle_timer_declaration_only_skeleton_compiles_for_fo4() -> None:
    folded = SKELETON.casefold()
    assert folded.count("scriptname comp_idletimerscript extends actor") == 1
    assert "event " not in folded
    assert "function " not in folded
    assert folded.count(" property ") == 4

    base = _fo4_base_source()
    if base is None:
        pytest.skip("FO4 base Papyrus sources unavailable")
    result = compile_psc(
        SKELETON,
        imports=[str(base)],
        game="fo4",
        flags=str(base / "Institute_Papyrus_Flags.flg"),
        source_path="COMP_IdleTimerScript.psc",
    )
    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None
