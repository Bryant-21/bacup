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

SCRIPT_MEMBERS = {
    "Fragments:Terminals:TERM_MoM_StudyTerminalJourna_0036671F": {
        "fragment_terminal_01",
        "fragment_terminal_11",
    },
    "Fragments:Terminals:TERM_MoM02C_AdvancedResearch_00366711": {
        "fragment_terminal_07"
    },
    "Fragments:Terminals:TERM_MoM02C_AdvancedResearch_00366712": {
        "fragment_terminal_02"
    },
    "Fragments:Terminals:TERM_MoM02C_AdvancedResearch_00366716": {
        "fragment_terminal_04"
    },
    "Fragments:Terminals:TERM_MoM02C_AdvancedResearch_00366717": {
        "fragment_terminal_02"
    },
    "Fragments:Terminals:TERM_MoM02C_DataExfiltration_00357FB5": {
        "setmom02cstage",
        "fragment_terminal_01",
        "fragment_terminal_02",
    },
    "Fragments:Terminals:TERM_MoM02C_SigIntAnalysisTe_00366715": {
        "fragment_terminal_01"
    },
    "Fragments:Terminals:TERM_MoM03_CryptosSiphonHolo_00509D5E": {
        "setmom03stage",
        "fragment_terminal_03",
        "fragment_terminal_04",
        "fragment_terminal_05",
    },
    "Fragments:Terminals:TERM_MoM03_PleasantValleyNet_00366710": {
        "fragment_terminal_01"
    },
    "Fragments:Terminals:TERM_MoM03_RaiderCommonRoomT_00357FE9": {
        "fragment_terminal_01"
    },
    "Fragments:Terminals:TERM_MoM04_OliviasTerminalJo_003694FC": {
        "fragment_terminal_01"
    },
    "Fragments:Terminals:TERM_MoM_Cryptos_ViewMission_00347309": {
        "startmission",
        "fragment_terminal_01",
        "fragment_terminal_02",
        "fragment_terminal_03",
        "fragment_terminal_04",
    },
    "Fragments:Terminals:TERM_MoM_Cryptos_RequestSupp_0034730C": {
        "recorddatabasequery",
        "fragment_terminal_01",
        "fragment_terminal_02",
        "fragment_terminal_03",
        "fragment_terminal_04",
    },
    "Fragments:Terminals:TERM_MoM_FabricatiorTerminal_00357F74": {
        "completefabrication",
        "fragment_terminal_01",
        "fragment_terminal_02",
        "fragment_terminal_03",
        "fragment_terminal_04",
        "fragment_terminal_08",
        "fragment_terminal_09",
    },
    "Fragments:Terminals:TERM_MoM02BWhitespringPresid_00357EA3": {
        "fragment_terminal_01"
    },
}


def _merged(script_name: str) -> str:
    relative_path = _script_relative_path(script_name, ".psc")
    skeleton = (SOURCE_ROOT / relative_path).read_text(encoding="utf-8")
    patch = _script_patch_source(script_name)
    assert patch is not None
    return _merge_script_method_patches(skeleton, patch)


@pytest.mark.parametrize(("script_name", "expected_members"), SCRIPT_MEMBERS.items())
def test_terminal_deep_tranche3_exact_members_merge_and_compile(
    script_name: str, expected_members: set[str]
):
    patch = _script_patch_source(script_name)
    assert patch is not None
    patch_members = [
        name
        for _kind, name, _start, _end in _iter_top_level_papyrus_members(
            patch.splitlines()
        )
    ]
    assert len(patch_members) == len(expected_members)
    assert set(patch_members) == expected_members

    merged = _merged(script_name)
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
    if base_source is None:
        pytest.skip("FO4 base Papyrus sources unavailable")
    relative_path = _script_relative_path(script_name, ".psc")
    result = compile_psc(
        merged,
        imports=[str(SOURCE_ROOT), str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=str(relative_path),
    )
    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None


def test_terminal_deep_tranche3_uses_live_mom_contract_indices_and_constants():
    mission = _script_patch_source(
        "Fragments:Terminals:TERM_MoM_Cryptos_ViewMission_00347309"
    )
    query = _script_patch_source(
        "Fragments:Terminals:TERM_MoM_Cryptos_RequestSupp_0034730C"
    )
    fabricator = _script_patch_source(
        "Fragments:Terminals:TERM_MoM_FabricatiorTerminal_00357F74"
    )
    assert mission is not None
    assert query is not None
    assert fabricator is not None

    for expected in (
        "StartMission(4, masterScript.CONST_MoM02A_SearchedForTarget)",
        "StartMission(5, masterScript.CONST_MoM02B_SearchedForTarget)",
        "StartMission(6, masterScript.CONST_MoM02C_SearchedForTarget)",
        "StartMission(7, masterScript.CONST_MOM03_AcceptedPleasantValleyMission)",
    ):
        assert expected in mission
    for expected in (
        "RecordDatabaseQuery(2, masterScript.CONST_MoM01_RequestedMentorAssignment)",
        "RecordDatabaseQuery(4, masterScript.CONST_MoM02A_SearchedForTarget)",
        "RecordDatabaseQuery(5, masterScript.CONST_MoM02B_SearchedForTarget)",
        "RecordDatabaseQuery(6, masterScript.CONST_MoM02C_SearchedForTarget)",
    ):
        assert expected in query
    assert "ClientFabricateCloth()" in fabricator
    assert "ClientFabricateMetal()" in fabricator
    assert "MoM_ClothesMistressOfMysteryVeilCorpse" in fabricator


def test_terminal_deep_tranche3_stage_actions_are_guarded_and_local():
    for script_name in SCRIPT_MEMBERS:
        patch = _script_patch_source(script_name)
        assert patch is not None
        assert "SetStage(" in patch
        assert "IsStageDone(" in patch
        assert "Game.GetForm" not in patch
        assert "Debug." not in patch
