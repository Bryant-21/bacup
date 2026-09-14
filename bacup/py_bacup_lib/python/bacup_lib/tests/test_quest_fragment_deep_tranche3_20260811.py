from __future__ import annotations

import csv
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
LEDGER_PATH = (
    REPO_ROOT
    / "bacup"
    / "docs"
    / "stub_restoration"
    / "contracts"
    / "quest-fragment-production-tranche-2026-08-11.csv"
)

SCRIPT_MEMBERS = {
    "Fragments:Quests:QF_AC_SQ04_Reopening_007399D7": {"fragment_stage_0650_item_00"},
    "Fragments:Quests:QF_BS01_Dialogue_VernonDodge_005CF0C6": {
        "fragment_stage_0100_item_00",
        "fragment_stage_0200_item_00",
        "fragment_stage_0300_item_00",
        "fragment_stage_0400_item_00",
    },
    "Fragments:Quests:qf_e01c_tales_mary_new_0041a676": {
        "fragment_stage_0050_item_00",
        "fragment_stage_0100_item_00",
        "fragment_stage_0110_item_00",
        "fragment_stage_0120_item_00",
        "fragment_stage_0200_item_00",
        "fragment_stage_0300_item_00",
        "fragment_stage_0350_item_00",
        "fragment_stage_0500_item_00",
    },
    "Fragments:Quests:QF_EN06_Intro_0052BDCA": {
        "fragment_stage_0010_item_00",
        "fragment_stage_0020_item_00",
    },
    "Fragments:Quests:QF_EN06_Seal_002B477C": {"fragment_stage_0110_item_00"},
    "Fragments:Quests:QF_EN07_MQ_Fissure_SpawnerQu_002D0F66": {"fragment_stage_0010_item_00"},
    "Fragments:Quests:QF_EXP14_HighRollersLounge_D_006F28D3": {"fragment_stage_0100_item_00"},
    "Fragments:Quests:QF_FF11_Raid_002D64EE": {"fragment_stage_0050_item_00"},
    "Fragments:Quests:qf_ff_small01_00036191": {"fragment_stage_2000_item_00"},
    "Fragments:Quests:QF_GHL00_Quest_0078DE64": {
        "fragment_stage_0400_item_00",
        "fragment_stage_1100_item_00",
    },
    "Fragments:Quests:QF_MTR01_Intro_00056F63": {"fragment_stage_0070_item_00"},
    "Fragments:Quests:qf_mtr08_lode_00042f7e": {"fragment_stage_0005_item_00"},
    "Fragments:Quests:QF_MTR08_Lode_Token_Misc_0043C60D": {"fragment_stage_0110_item_00"},
    "Fragments:Quests:QF_P03L_McCreary_Scenes_004273D8": {"fragment_stage_0150_item_00"},
    "Fragments:Quests:QF_P01B_Lying_01_00478DD3": {"fragment_stage_0250_item_00"},
    "Fragments:Quests:QF_P01B_Lying_02_0047F443": {
        "fragment_stage_0550_item_00",
        "fragment_stage_0950_item_00",
    },
    "Fragments:Quests:QF_P01B_Wolf_003FBF2E": {"fragment_stage_0300_item_00"},
    "Fragments:Quests:QF_SHELS01_OpenHouse_Interio_005C785E": {"fragment_stage_0110_item_00"},
    "Fragments:Quests:QF_Storm_SE04_006E2CFC": {"fragment_stage_0150_item_00"},
    "Fragments:Quests:QF_TestUD002_003ECC12": {
        "fragment_stage_0250_item_00",
        "fragment_stage_0300_item_00",
        "fragment_stage_0350_item_00",
    },
    "Fragments:Quests:qf_tw008_003df3ff": {
        "fragment_stage_0510_item_00",
        "fragment_stage_0610_item_00",
        "fragment_stage_0710_item_00",
    },
    "Fragments:Quests:qf_tw009_0025a836": {"fragment_stage_0500_item_00"},
    "Fragments:Quests:QF_W05_MQ_101P_OnIncreaseLev_00591E06": {"fragment_stage_0010_item_00"},
    "Fragments:Quests:QF_W05_MQ_101P_OnLocationCha_00591AB3": {"fragment_stage_0010_item_00"},
}

EXPECTED_SNIPPETS = {
    "QF_AC_SQ04_Reopening_007399D7": "AC_SQ04_Reopening_ReneJusticeAmbient.Start()",
    "QF_BS01_Dialogue_VernonDodge_005CF0C6": "playerRef.SetValue(AV_Relationship, 4.0)",
    "qf_e01c_tales_mary_new_0041a676": "Scene_TheRing.Start()",
    "QF_EN06_Intro_0052BDCA": "EN06_Intro_0020_MODUSIntro.Start()",
    "QF_EN06_Seal_002B477C": "playerRef.AddToFaction(EN06_EnclavePresidentFaction)",
    "QF_EN07_MQ_Fissure_SpawnerQu_002D0F66": "fissureRef.Enable(False)",
    "QF_EXP14_HighRollersLounge_D_006F28D3": "doorRef.Unlock()",
    "QF_FF11_Raid_002D64EE": "FF11_Raid_AirRaidSirenMarker.Disable(False)",
    "qf_ff_small01_00036191": "flowerRef.Disable(False)",
    "QF_GHL00_Quest_0078DE64": "Scene_GHL00_08_GhoulCamp_PartheniaLeamonAmbient.Start()",
    "QF_MTR01_Intro_00056F63": "SetObjectiveDisplayed(70, True)",
    "qf_mtr08_lode_00042f7e": "SetObjectiveDisplayed(5, True)",
    "QF_MTR08_Lode_Token_Misc_0043C60D": "SetObjectiveDisplayed(10, True)",
    "QF_P03L_McCreary_Scenes_004273D8": "playerRef.AddItem(CageKey, 1, False)",
    "QF_P01B_Lying_01_00478DD3": "holotapeRef.Enable(False)",
    "QF_P01B_Lying_02_0047F443": "passwordRef.Enable(False)",
    "QF_P01B_Wolf_003FBF2E": "playerRef.AddItem(P01B_Wolf_RecallKey, 1, False)",
    "QF_SHELS01_OpenHouse_Interio_005C785E": "SetObjectiveDisplayed(10, True)",
    "QF_Storm_SE04_006E2CFC": "Storm_SE04_TalkScene.Start()",
    "QF_TestUD002_003ECC12": "SetObjectiveCompleted(300, True)",
    "qf_tw008_003df3ff": "SetObjectiveCompleted(700, True)",
    "qf_tw009_0025a836": "Alias_ConfederateGutsy05.GetReference()",
    "QF_W05_MQ_101P_OnIncreaseLev_00591E06": "W05_MQ_101P_QuestStartKeyword.SendStoryEventAndWait(None, playerRef)",
    "QF_W05_MQ_101P_OnLocationCha_00591AB3": "W05_MQ_101P_QuestStartKeyword.SendStoryEventAndWait(None, playerRef)",
}


def _member_names(source: str) -> list[str]:
    return [
        name
        for _kind, name, _start, _end in _iter_top_level_papyrus_members(source.splitlines())
    ]


def _merged_source(script_name: str) -> str:
    source_path = SOURCE_ROOT / _script_relative_path(script_name, ".psc")
    patch = _script_patch_source(script_name)
    assert source_path.is_file(), source_path
    assert patch is not None
    return _merge_script_method_patches(source_path.read_text(encoding="utf-8"), patch)


def test_deep_tranche3_manifest_and_ledger_are_exact():
    assert len(SCRIPT_MEMBERS) == 24
    assert sum(len(members) for members in SCRIPT_MEMBERS.values()) == 41

    with LEDGER_PATH.open(encoding="utf-8", newline="") as stream:
        rows = list(csv.DictReader(stream))
    tranche_rows = [row for row in rows if row["evidence_gate"].startswith("tranche-3 retained:")]
    assert len(tranche_rows) == 24
    assert {Path(row["generated_file"]).stem.casefold() for row in tranche_rows} == {
        script_name.rsplit(":", 1)[-1].casefold() for script_name in SCRIPT_MEMBERS
    }


@pytest.mark.parametrize(("script_name", "members"), SCRIPT_MEMBERS.items())
def test_deep_tranche3_patch_is_member_only_and_merges_idempotently(
    script_name: str, members: set[str]
):
    patch = _script_patch_source(script_name)
    assert patch is not None
    assert set(_member_names(patch)) == members
    assert EXPECTED_SNIPPETS[script_name.rsplit(":", 1)[-1]] in patch
    assert not any(
        line.strip().lower().startswith(("scriptname ", "state ", "auto state "))
        for line in patch.splitlines()
    )

    merged = _merged_source(script_name)
    merged_names = _member_names(merged)
    for member in members:
        assert merged_names.count(member) == 1
    assert _merge_script_method_patches(merged, patch) == merged


@pytest.mark.parametrize("script_name", SCRIPT_MEMBERS)
def test_deep_tranche3_merged_full_source_native_compiles(script_name: str, tmp_path: Path):
    merged = _merged_source(script_name)
    merged_source_root = tmp_path / "Scripts" / "Source" / "User"
    source_path = merged_source_root / _script_relative_path(script_name, ".psc")
    source_path.parent.mkdir(parents=True, exist_ok=True)
    source_path.write_text(merged, encoding="utf-8")

    base_source = _fo4_base_source()
    assert base_source is not None, "FO4 base Papyrus sources unavailable"
    result = compile_psc(
        merged,
        imports=[str(merged_source_root), str(SOURCE_ROOT), str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=str(_script_relative_path(script_name, ".psc")),
    )
    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, f"{script_name}\n{diagnostics}"
    assert result.pex_bytes is not None
