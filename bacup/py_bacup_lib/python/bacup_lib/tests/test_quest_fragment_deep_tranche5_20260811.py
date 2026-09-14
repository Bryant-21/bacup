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
LEDGER_PATH = REPO_ROOT / "bacup" / "docs" / "stub_restoration" / "contracts" / "quest-fragment-production-tranche-2026-08-11.csv"

SCRIPT_MEMBERS = {
    "Fragments:Quests:QF_BS01_Dialogue_DaggerThron_005CBED5": {"fragment_stage_0100_item_00"},
    "Fragments:Quests:QF_BS02_E01_Metal_005FE4D7": {
        "fragment_stage_0300_item_00",
        "fragment_stage_10000_item_00",
    },
    "Fragments:Quests:QF_EN05_Intro_Misc_00341B06": {"fragment_stage_0090_item_00", "fragment_stage_0100_item_00"},
    "Fragments:Quests:QF_GHL00_Quest_DisguisePoint_007D4961": {"fragment_stage_9000_item_00"},
    "Fragments:Quests:QF_LC170_DialogueWatogaVoice_0050A604": {
        "fragment_stage_0010_item_00",
        "fragment_stage_0015_item_00",
        "fragment_stage_0020_item_00",
        "fragment_stage_0030_item_00",
        "fragment_stage_0040_item_00",
        "fragment_stage_0050_item_00",
        "fragment_stage_0060_item_00",
        "fragment_stage_0070_item_00",
        "fragment_stage_0080_item_00",
        "fragment_stage_0090_item_00",
    },
    "Fragments:Quests:QF_M01C_Atheletics_004178BE": {"fragment_stage_0100_item_00", "fragment_stage_0800_item_00", "fragment_stage_0900_item_00"},
    "Fragments:Quests:QF_MTR03_Misc_00151C17": {"fragment_stage_0010_item_00", "fragment_stage_0100_item_00"},
    "Fragments:Quests:QF_MTR15_WelchMisc_0050FDC0": {"fragment_stage_0100_item_00"},
    "Fragments:Quests:QF_P03T_MarshallSceneQuest_003F4BE0": {"fragment_stage_9000_item_00"},
    "Fragments:Quests:QF_P02L_McCreary_FetchQuest_0041B865": {
        "fragment_stage_0100_item_00",
        "fragment_stage_0210_item_00",
        "fragment_stage_0220_item_00",
        "fragment_stage_0230_item_00",
    },
    "Fragments:Quests:QF_RD01_Enc05_ResearchLab_00788127": {"fragment_stage_0100_item_00", "fragment_stage_9000_item_00"},
    "Fragments:Quests:QF_RSVP00_Quest_Master_0050A2EE": {"fragment_stage_0300_item_00"},
    "Fragments:Quests:QF_SFM04_Organic_Blooms_00049B4F": {"fragment_stage_1000_item_00"},
    "Fragments:Quests:QF_SFM04_Organic_Radio_00052DBF": {"fragment_stage_1000_item_00"},
    "Fragments:Quests:QF_SHELS01_FindTheKeyMiscObj_005DC5DB": {"fragment_stage_0300_item_00"},
    "Fragments:Quests:QF_SQ_CampAttack_0050BEC1": {"fragment_stage_0100_item_00"},
    "Fragments:Quests:QF_Storm_SE09_00783A58": {"fragment_stage_0200_item_00"},
    "Fragments:Quests:QF_Vault79VaultQuest_0058303C": {
        "fragment_stage_0100_item_00",
        "fragment_stage_0200_item_00",
        "fragment_stage_0500_item_00",
    },
    "Fragments:Quests:qf_tw043patrol_0005a243": {
        "fragment_stage_0010_item_00",
        "fragment_stage_0020_item_00",
        "fragment_stage_0030_item_00",
        "fragment_stage_0040_item_00",
        "fragment_stage_0060_item_00",
        "fragment_stage_0100_item_00",
        "fragment_stage_0110_item_00",
        "fragment_stage_0115_item_00",
        "fragment_stage_0120_item_00",
        "fragment_stage_0121_item_00",
        "fragment_stage_0130_item_00",
        "fragment_stage_0140_item_00",
        "fragment_stage_0150_item_00",
        "fragment_stage_0170_item_00",
        "fragment_stage_0200_item_00",
        "fragment_stage_0220_item_00",
        "fragment_stage_1000_item_00",
    },
}

LIVE_MEMBER_COUNTS = {
    "Fragments:Quests:QF_BS01_Dialogue_DaggerThron_005CBED5": 1,
    "Fragments:Quests:QF_BS02_E01_Metal_005FE4D7": 21,
    "Fragments:Quests:QF_EN05_Intro_Misc_00341B06": 2,
    "Fragments:Quests:QF_GHL00_Quest_DisguisePoint_007D4961": 2,
    "Fragments:Quests:QF_LC170_DialogueWatogaVoice_0050A604": 10,
    "Fragments:Quests:QF_M01C_Atheletics_004178BE": 9,
    "Fragments:Quests:QF_MTR03_Misc_00151C17": 3,
    "Fragments:Quests:QF_MTR15_WelchMisc_0050FDC0": 2,
    "Fragments:Quests:QF_P03T_MarshallSceneQuest_003F4BE0": 6,
    "Fragments:Quests:QF_P02L_McCreary_FetchQuest_0041B865": 7,
    "Fragments:Quests:QF_RD01_Enc05_ResearchLab_00788127": 2,
    "Fragments:Quests:QF_RSVP00_Quest_Master_0050A2EE": 9,
    "Fragments:Quests:QF_SFM04_Organic_Blooms_00049B4F": 2,
    "Fragments:Quests:QF_SFM04_Organic_Radio_00052DBF": 1,
    "Fragments:Quests:QF_SHELS01_FindTheKeyMiscObj_005DC5DB": 4,
    "Fragments:Quests:QF_SQ_CampAttack_0050BEC1": 1,
    "Fragments:Quests:QF_Storm_SE09_00783A58": 2,
    "Fragments:Quests:QF_Vault79VaultQuest_0058303C": 3,
    "Fragments:Quests:qf_tw043patrol_0005a243": 32,
}

AUGMENTED_DECLARATIONS = {
    "Fragments:Quests:QF_BS02_E01_Metal_005FE4D7": "MusicType Property Music_CombatMusic Auto mandatory",
    "Fragments:Quests:QF_P02L_McCreary_FetchQuest_0041B865": "Int Property SuppliesCount Auto",
}


def _members(source: str) -> list[str]:
    return [name for _kind, name, _start, _end in _iter_top_level_papyrus_members(source.splitlines())]


def _source(script_name: str) -> str:
    source = (SOURCE_ROOT / _script_relative_path(script_name, ".psc")).read_text(encoding="utf-8")
    declaration = AUGMENTED_DECLARATIONS.get(script_name)
    if declaration and declaration not in source:
        source = source.replace("\n", f"\n\n{declaration}\n", 1)
    return source


def _merged(script_name: str) -> str:
    patch = _script_patch_source(script_name)
    assert patch is not None
    return _merge_script_method_patches(_source(script_name), patch)


def test_deep_tranche5_manifest_and_ledger_classification():
    assert len(SCRIPT_MEMBERS) == 19
    assert sum(map(len, SCRIPT_MEMBERS.values())) == 55
    assert LIVE_MEMBER_COUNTS.keys() == SCRIPT_MEMBERS.keys()
    assert sum(LIVE_MEMBER_COUNTS.values()) == 119
    assert all(len(SCRIPT_MEMBERS[name]) <= count for name, count in LIVE_MEMBER_COUNTS.items())
    with LEDGER_PATH.open(encoding="utf-8", newline="") as stream:
        ledger = list(csv.DictReader(stream))
    rows = [
        row
        for row in ledger
        if row["evidence_gate"].startswith("tranche-5 ")
        and row["disposition"] in {"patched-local-quest-action", "partial-local-quest-action"}
    ]
    assert len(rows) == 19
    assert sum(row["disposition"] == "patched-local-quest-action" for row in rows) == 7
    assert sum(row["disposition"] == "partial-local-quest-action" for row in rows) == 12
    manifest_by_stem = {
        _script_relative_path(name, ".psc").stem.casefold(): name
        for name in SCRIPT_MEMBERS
    }
    for row in rows:
        script_name = manifest_by_stem[Path(row["generated_file"]).stem.casefold()]
        restored = len(SCRIPT_MEMBERS[script_name])
        live = LIVE_MEMBER_COUNTS[script_name]
        if row["disposition"] == "patched-local-quest-action":
            assert restored == live
        else:
            assert restored < live
    assert sum(row["disposition"] == "blocked-insufficient-body-evidence" for row in ledger) <= 251


@pytest.mark.parametrize(("script_name", "expected"), SCRIPT_MEMBERS.items())
def test_deep_tranche5_member_only_merge_and_static_live_manifest(script_name: str, expected: set[str]):
    patch = _script_patch_source(script_name)
    assert patch is not None
    assert len(_members(patch)) == len(expected)
    assert set(_members(patch)) == expected
    merged = _merged(script_name)
    for member in expected:
        assert sum(name == member for name in _members(merged)) == 1
    assert _merge_script_method_patches(merged, patch) == merged


def test_deep_tranche5_behavior_contracts():
    stop_scripts = (
        "Fragments:Quests:QF_GHL00_Quest_DisguisePoint_007D4961",
        "Fragments:Quests:QF_SFM04_Organic_Blooms_00049B4F",
        "Fragments:Quests:QF_SFM04_Organic_Radio_00052DBF",
        "Fragments:Quests:QF_SQ_CampAttack_0050BEC1",
    )
    for script_name in stop_scripts:
        patch = _script_patch_source(script_name)
        assert patch is not None
        assert patch.count("Stop()") == 1
    athletics = _script_patch_source("Fragments:Quests:QF_M01C_Atheletics_004178BE")
    research = _script_patch_source("Fragments:Quests:QF_RD01_Enc05_ResearchLab_00788127")
    dagger = _script_patch_source("Fragments:Quests:QF_BS01_Dialogue_DaggerThron_005CBED5")
    assert athletics and research and dagger
    for scene in ("Scene_Start", "Scene_Fail", "Scene_Success"):
        assert f"{scene}.Start()" in athletics
    assert "enableMarker.Enable()" in research
    assert "enableMarker.Disable()" in research
    assert "daggerRef.IsDead()" in dagger
    patrol = _script_patch_source("Fragments:Quests:qf_tw043patrol_0005a243")
    assert patrol is not None
    for scene in (
        "TW043_010_Start",
        "TW043_020_AWing",
        "TW043_030_SecurityA",
        "TW043_040_DownloadA",
        "TW043_100_BWing",
        "TW043_110_DiningHall",
        "TW043_115_PrisonYard",
        "TW043_120_Solitary",
        "TW043_121_Solitary2",
        "TW043_130_DWing",
        "TW043_140_SecurityD",
        "TW043_150_DownloadD",
        "TW043_200_SecurityDoor",
        "TW043_220_End",
    ):
        assert f"{scene}.Start()" in patrol
    assert patrol.count("TW043_DownloadComplete.Start()") == 2
    watoga = _script_patch_source("Fragments:Quests:QF_LC170_DialogueWatogaVoice_0050A604")
    vault = _script_patch_source("Fragments:Quests:QF_Vault79VaultQuest_0058303C")
    assert watoga and vault
    assert watoga.count(".Start()") == 10
    assert "Vault79VaultQuest_Ventilation.Start()" in vault
    assert "Vault79VaultQuest_SentryBotScene.Start()" in vault
    assert "Vault79VaultQuest_GoldAnalysisScene.Start()" in vault
    en05 = _script_patch_source("Fragments:Quests:QF_EN05_Intro_Misc_00341B06")
    assert en05 is not None
    assert en05.index("SetValue(EN05_IntroMisc_CompletedValue, 1.0)") < en05.index("Stop()")
    metal = _script_patch_source("Fragments:Quests:QF_BS02_E01_Metal_005FE4D7")
    fetch = _script_patch_source("Fragments:Quests:QF_P02L_McCreary_FetchQuest_0041B865")
    rsvp = _script_patch_source("Fragments:Quests:QF_RSVP00_Quest_Master_0050A2EE")
    assert metal and fetch and rsvp
    assert metal.index("Music_CombatMusic.Add()") < metal.index("Music_CombatMusic.Remove()")
    assert fetch.count("SuppliesCount += 1") == 3
    assert fetch.count("SetStage(300)") == 3
    assert "SetValue(pRSVP00_AV_StartedRSVP01, 1.0)" in rsvp


@pytest.mark.parametrize("script_name", SCRIPT_MEMBERS)
def test_deep_tranche5_merged_full_source_native_compiles(script_name: str, tmp_path: Path):
    merged = _merged(script_name)
    source_root = tmp_path / "Scripts" / "Source" / "User"
    relative = _script_relative_path(script_name, ".psc")
    path = source_root / relative
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(merged, encoding="utf-8")
    base = _fo4_base_source()
    assert base is not None
    result = compile_psc(
        merged,
        imports=[str(source_root), str(SOURCE_ROOT), str(base)],
        game="fo4",
        flags=str(base / "Institute_Papyrus_Flags.flg"),
        source_path=str(relative),
    )
    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, f"{script_name}\n{diagnostics}"
    assert result.pex_bytes is not None
