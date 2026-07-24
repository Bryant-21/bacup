from __future__ import annotations

import os
from pathlib import Path

import pytest

from bacup_lib.workflows.unified import (
    _iter_top_level_papyrus_members,
    _merge_script_method_patches,
    _script_patch_source,
    _script_relative_path,
)
from creation_lib.pex import parse_pex
from creation_lib.pex.native_runtime import compile_psc
from creation_lib.pex.opcodes import PexOpcode


REPO_ROOT = Path(__file__).resolve().parents[5]
SOURCE_ROOT = REPO_ROOT / "mods" / "SeventySix" / "Scripts" / "Source" / "User"

SCRIPT_MEMBERS = {
    "DefaultStartQuestOnHolotapeEvent": {
        "onquestinit",
        "objectreference.onholotapeplay",
        "objectreference.onitemadded",
        "processholotape",
        "getplayedholotape",
        "applyholotapedatum",
    },
    "DefaultQuestAddMapMarkerOnHolotapeEnd": {
        "onquestinit",
        "objectreference.onholotapeplay",
        "objectreference.onitemadded",
        "processholotape",
        "getplayedholotape",
    },
    "MoMHolotapeScript": {
        "onquestinit",
        "objectreference.onholotapeplay",
        "objectreference.onitemadded",
        "processholotape",
        "getplayedholotape",
    },
    "W05_HolotapeScript": {
        "onquestinit",
        "objectreference.onholotapeplay",
        "objectreference.onitemadded",
        "processholotape",
        "getplayedholotape",
        "istriggeringtape",
    },
    "W05_MortTapeQuestScript": {
        "onquestinit",
        "objectreference.onholotapeplay",
        "objectreference.onitemadded",
        "getplayedholotape",
        "processholotape",
        "ontimer",
        "showtutorialentry",
    },
    "DefaultQuestRemovePlayersScript": {
        "onquestinit",
        "onquestshutdown",
        "fillplayeraliases",
        "cleanupquestitems",
        "removequestreference",
    },
    "COMP_RQ_SpecificAliasesScript": {
        "onquestinit",
        "copyreferencealias",
    },
    "Fragments:Quests:QF_HolotapeQuest_00011B82": {
        "fragment_stage_0001_item_00"
    },
    "Fragments:Quests:QF_HolotapeQuest_TS_00511A82": {
        "fragment_stage_0001_item_00"
    },
    "Fragments:Quests:QF_MQ_OverseerNukeHolotapeVi_00437987": {
        "fragment_stage_0010_item_00",
        "fragment_stage_0100_item_00",
    },
    "Fragments:Quests:QF_E06_PocketWatch_Colossus__00599357": {
        "fragment_stage_0100_item_00",
        "fragment_stage_0110_item_00",
        "fragment_stage_0125_item_00",
        "fragment_stage_0150_item_00",
        "fragment_stage_9000_item_00",
    },
    "Fragments:Quests:QF_MOON_HolotapeQuest_006A21E2": {
        "fragment_stage_0100_item_00",
        "fragment_stage_0200_item_00",
        "fragment_stage_0300_item_00",
        "fragment_stage_0410_item_00",
        "fragment_stage_0420_item_00",
        "fragment_stage_0430_item_00",
        "fragment_stage_0440_item_00",
        "fragment_stage_0450_item_00",
        "fragment_stage_0500_item_00",
        "fragment_stage_0600_item_00",
        "fragment_stage_0610_item_00",
        "fragment_stage_0620_item_00",
        "fragment_stage_0700_item_00",
        "fragment_stage_9000_item_00",
    },
}


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


def _member_names(source: str) -> set[str]:
    return {
        name
        for _kind, name, _start, _end in _iter_top_level_papyrus_members(
            source.splitlines()
        )
    }


def _merged_source(script_name: str) -> str:
    source_path = SOURCE_ROOT / _script_relative_path(script_name, ".psc")
    patch = _script_patch_source(script_name)
    assert source_path.is_file(), source_path
    assert patch is not None
    return _merge_script_method_patches(
        source_path.read_text(encoding="utf-8"), patch
    )


@pytest.mark.parametrize(("script_name", "expected"), SCRIPT_MEMBERS.items())
def test_holotape_patch_supplies_vmad_behavior(
    script_name: str, expected: set[str]
):
    patch = _script_patch_source(script_name)

    assert patch is not None
    assert "Scriptname " not in patch
    assert expected <= _member_names(patch)

    merged = _merged_source(script_name)
    assert expected <= _member_names(merged)
    assert merged.lower().count("scriptname ") == 1


def test_fo76_end_only_holotape_callbacks_have_documented_fo4_fallback():
    start_patch = _script_patch_source("DefaultStartQuestOnHolotapeEvent")
    marker_patch = _script_patch_source("DefaultQuestAddMapMarkerOnHolotapeEnd")

    assert start_patch is not None
    assert marker_patch is not None
    assert "fall back to the supported pickup event" in start_patch
    assert "reveal on pickup" in marker_patch


@pytest.mark.parametrize(
    "script_name",
    (
        "DefaultStartQuestOnHolotapeEvent",
        "DefaultQuestAddMapMarkerOnHolotapeEnd",
        "MoMHolotapeScript",
        "W05_HolotapeScript",
        "W05_MortTapeQuestScript",
    ),
)
def test_quest_scripts_register_player_pickup_not_base_form_play(
    script_name: str,
):
    patch = _script_patch_source(script_name)
    assert patch is not None

    assert 'RegisterForRemoteEvent(player, "OnItemAdded")' in patch
    assert 'RegisterForRemoteEvent(TargetTape' not in patch
    assert 'RegisterForRemoteEvent(clueTape' not in patch
    assert 'RegisterForRemoteEvent(duchessTape' not in patch
    assert 'RegisterForRemoteEvent(HolotapeData' not in patch
    assert 'RegisterForRemoteEvent(HolotapeMapMarkerData' not in patch
    assert 'RegisterForRemoteEvent(MoMHolotapeData' not in patch
    assert '"OnHolotapePlay")' not in patch


@pytest.mark.parametrize(
    "script_name",
    (
        "DefaultStartQuestOnHolotapeEvent",
        "DefaultQuestAddMapMarkerOnHolotapeEnd",
        "MoMHolotapeScript",
        "W05_HolotapeScript",
        "W05_MortTapeQuestScript",
    ),
)
def test_pickup_fallback_filters_to_the_player_and_a_holotape(script_name: str):
    patch = _script_patch_source(script_name)
    assert patch is not None

    assert "Event ObjectReference.OnItemAdded(" in patch
    assert "Game.GetPlayer()" in patch
    assert "akBaseItem as Holotape" in patch


def test_remove_players_waits_for_the_quest_to_enable_before_forcing_aliases():
    patch = _script_patch_source("DefaultQuestRemovePlayersScript")
    assert patch is not None

    start = patch.index("Event OnQuestInit()")
    end = patch.index("EndEvent", start)
    on_quest_init = patch[start:end]

    assert on_quest_init.index("Utility.Wait(0.1)") < on_quest_init.index(
        "If IsRunning()"
    )
    assert on_quest_init.index("If IsRunning()") < on_quest_init.index(
        "FillPlayerAliases()"
    )


def test_remove_players_compiled_loops_advance(tmp_path: Path):
    base_source = _fo4_base_source()
    if base_source is None:
        pytest.skip("FO4 base Papyrus sources unavailable")

    script_name = "DefaultQuestRemovePlayersScript"
    source = _merged_source(script_name)
    result = compile_psc(
        source,
        imports=[str(tmp_path), str(SOURCE_ROOT), str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=str(_script_relative_path(script_name, ".psc")),
    )
    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None

    pex_path = tmp_path / f"{script_name}.pex"
    pex_path.write_bytes(result.pex_bytes)
    functions = {
        function.name.casefold(): function
        for obj in parse_pex(pex_path).objects
        for state in obj.states
        for function in state.functions
    }

    assert sum(
        instruction.opcode == PexOpcode.IADD
        for instruction in functions["fillplayeraliases"].instructions
    ) == 2
    assert sum(
        instruction.opcode == PexOpcode.IADD
        for instruction in functions["cleanupquestitems"].instructions
    ) == 2


def test_holotape_patch_set_native_compiles_for_fo4(tmp_path: Path):
    base_source = _fo4_base_source()
    if base_source is None:
        pytest.skip("FO4 base Papyrus sources unavailable")

    merged_sources: dict[str, str] = {}
    for script_name in SCRIPT_MEMBERS:
        source = _merged_source(script_name)
        merged_sources[script_name] = source
        source_path = tmp_path / _script_relative_path(script_name, ".psc")
        source_path.parent.mkdir(parents=True, exist_ok=True)
        source_path.write_text(source, encoding="utf-8")

    for script_name, source in merged_sources.items():
        result = compile_psc(
            source,
            imports=[str(tmp_path), str(SOURCE_ROOT), str(base_source)],
            game="fo4",
            flags=str(base_source / "Institute_Papyrus_Flags.flg"),
            source_path=str(_script_relative_path(script_name, ".psc")),
        )
        diagnostics = "\n".join(str(item) for item in result.diagnostics)
        assert result.ok, f"{script_name}:\n{diagnostics}"
        assert result.pex_bytes is not None
