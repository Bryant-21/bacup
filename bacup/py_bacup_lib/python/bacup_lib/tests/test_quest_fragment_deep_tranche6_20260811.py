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
    "Fragments:Quests:qf_e01c_tales_mansion_00468b74": {"fragment_stage_0100_item_00", "fragment_stage_0200_item_00", "fragment_stage_0300_item_00", "fragment_stage_0400_item_00", "fragment_stage_0650_item_00", "fragment_stage_0900_item_00", "fragment_stage_1050_item_00", "fragment_stage_1350_item_00"},
    "Fragments:Quests:QF_E08A_Moonshine_006216DA": {"fragment_stage_0200_item_00", "fragment_stage_0800_item_00", "fragment_stage_0810_item_00", "fragment_stage_0820_item_00", "fragment_stage_0900_item_00", "fragment_stage_0950_item_00", "fragment_stage_1100_item_00", "fragment_stage_1200_item_00", "fragment_stage_1500_item_00", "fragment_stage_3000_item_00", "fragment_stage_10000_item_00"},
    "Fragments:Quests:qf_e08b_evictionnotice_006431ce": {"fragment_stage_0100_item_00"},
    "Fragments:Quests:qf_ff08_projectbeanstalk_0004695c": {"fragment_stage_0097_item_00", "fragment_stage_0098_item_00", "fragment_stage_0099_item_00"},
    "Fragments:Quests:qf_fss02_vigilant_000a73dc": {"fragment_stage_0100_item_00", "fragment_stage_0250_item_00", "fragment_stage_10000_item_00"},
    "Fragments:Quests:QF_MTNM03_Meditation_0012E67E": {"fragment_stage_0255_item_00", "fragment_stage_0500_item_00"},
    "Fragments:Quests:qf_mtns06_uranium_000364d0": {"fragment_stage_0150_item_00", "fragment_stage_0200_item_00", "fragment_stage_0201_item_00", "fragment_stage_0202_item_00", "fragment_stage_0203_item_00", "fragment_stage_0300_item_00", "fragment_stage_0325_item_00", "fragment_stage_0400_item_00"},
    "Fragments:Quests:qf_rs02_beat_0015d682": {"fragment_stage_0200_item_00", "fragment_stage_0400_item_00", "fragment_stage_0600_item_00", "fragment_stage_0800_item_00", "fragment_stage_1000_item_00", "fragment_stage_1200_item_00", "fragment_stage_1401_item_00", "fragment_stage_5500_item_00", "fragment_stage_6000_item_00"},
    "Fragments:Quests:QF_SFS09_Habitat_005109AF": {"fragment_stage_0100_item_00", "fragment_stage_0150_item_00", "fragment_stage_0190_item_00", "fragment_stage_0200_item_00", "fragment_stage_0300_item_00", "fragment_stage_0350_item_00", "fragment_stage_0450_item_00", "fragment_stage_9000_item_00", "fragment_stage_9990_item_00", "fragment_stage_9991_item_00", "fragment_stage_9992_item_00", "fragment_stage_10000_item_00"},
    "Fragments:Quests:qf_sfz08_fear_00275bc5": {"fragment_stage_1000_item_00"},
    "Fragments:Quests:qf_twz07_0025c0f4": {"fragment_stage_1000_item_00"},
}

LIVE_MEMBER_COUNTS = {
    "Fragments:Quests:qf_e01c_tales_mansion_00468b74": 40,
    "Fragments:Quests:QF_E08A_Moonshine_006216DA": 28,
    "Fragments:Quests:qf_e08b_evictionnotice_006431ce": 17,
    "Fragments:Quests:qf_ff08_projectbeanstalk_0004695c": 14,
    "Fragments:Quests:qf_fss02_vigilant_000a73dc": 10,
    "Fragments:Quests:QF_MTNM03_Meditation_0012E67E": 18,
    "Fragments:Quests:qf_mtns06_uranium_000364d0": 15,
    "Fragments:Quests:qf_rs02_beat_0015d682": 26,
    "Fragments:Quests:QF_SFS09_Habitat_005109AF": 30,
    "Fragments:Quests:qf_sfz08_fear_00275bc5": 18,
    "Fragments:Quests:qf_twz07_0025c0f4": 6,
}


def _members(source: str) -> list[str]:
    return [name for _kind, name, _start, _end in _iter_top_level_papyrus_members(source.splitlines())]


def _merged(script_name: str) -> str:
    patch = _script_patch_source(script_name)
    assert patch is not None
    source = (SOURCE_ROOT / _script_relative_path(script_name, ".psc")).read_text(encoding="utf-8")
    return _merge_script_method_patches(source, patch)


def test_deep_tranche6_manifest_and_exhaustive_ledger():
    assert len(SCRIPT_MEMBERS) == 11
    assert sum(map(len, SCRIPT_MEMBERS.values())) == 59
    assert sum(LIVE_MEMBER_COUNTS.values()) == 222
    assert all(len(SCRIPT_MEMBERS[name]) < count for name, count in LIVE_MEMBER_COUNTS.items())
    with LEDGER_PATH.open(encoding="utf-8", newline="") as stream:
        ledger = list(csv.DictReader(stream))
    rows = [row for row in ledger if row["evidence_gate"].startswith("tranche-6 partial ")]
    assert len(rows) == 11
    assert all(row["disposition"] == "partial-local-quest-action" for row in rows)
    assert sum(row["disposition"] == "blocked-insufficient-body-evidence" for row in ledger) == 236
    assert sum(row["evidence_gate"].startswith("tranche-6 reviewed:") for row in ledger) == 233


@pytest.mark.parametrize(("script_name", "expected"), SCRIPT_MEMBERS.items())
def test_deep_tranche6_member_only_merge_is_idempotent(script_name: str, expected: set[str]):
    patch = _script_patch_source(script_name)
    assert patch is not None
    assert len(_members(patch)) == len(expected)
    assert set(_members(patch)) == expected
    merged = _merged(script_name)
    for member in expected:
        assert sum(name == member for name in _members(merged)) == 1
    assert _merge_script_method_patches(merged, patch) == merged


def test_deep_tranche6_exact_scene_and_lifecycle_contracts():
    beat = _script_patch_source("Fragments:Quests:qf_rs02_beat_0015d682")
    habitat = _script_patch_source("Fragments:Quests:QF_SFS09_Habitat_005109AF")
    uranium = _script_patch_source("Fragments:Quests:qf_mtns06_uranium_000364d0")
    assert beat and habitat and uranium
    assert "RS02_Beat_Loc2TravelScene.Start()" in beat
    assert "RS02_Beat_Loc3AlarmScene.Start()" in beat
    assert habitat.count("Scene_QuestFail.Start()") == 3
    assert "Scene_QuestComplete.Start()" in habitat
    assert uranium.count("MTNS06_Uranium_PA_BossSpawn.Start()") == 3
    assert uranium.count("MTNS06_Uranium_PA_ActivityEnd.Start()") == 2
    mansion = _script_patch_source("Fragments:Quests:qf_e01c_tales_mansion_00468b74")
    assert mansion is not None
    for scene in ("Scene_Intro", "Scene_P", "Scene_PP", "Scene_PPP", "Scene_PN", "Scene_N", "Scene_NP", "Scene_NN"):
        assert f"{scene}.Start()" in mansion
    moonshine = _script_patch_source("Fragments:Quests:QF_E08A_Moonshine_006216DA")
    assert moonshine is not None
    assert moonshine.count("PA_StillDestroyed.Start()") == 3
    for scene in ("PA_TruckExploded", "PA_VenomDeposited", "PA_VenomHalfway", "PA_RequiredVenomGoal", "PA_ExtraVenomGoal", "PA_EventSuccess", "PA_EventFailure"):
        assert f"{scene}.Start()" in moonshine
    assert all(source.count("Stop()") == 1 for source in (
        _script_patch_source("Fragments:Quests:qf_fss02_vigilant_000a73dc"),
        _script_patch_source("Fragments:Quests:QF_MTNM03_Meditation_0012E67E"),
        _script_patch_source("Fragments:Quests:qf_sfz08_fear_00275bc5"),
    ) if source)


@pytest.mark.parametrize("script_name", SCRIPT_MEMBERS)
def test_deep_tranche6_merged_full_source_native_compiles(script_name: str, tmp_path: Path):
    merged = _merged(script_name)
    source_root = tmp_path / "Scripts" / "Source" / "User"
    relative = _script_relative_path(script_name, ".psc")
    path = source_root / relative
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(merged, encoding="utf-8")
    base = _fo4_base_source()
    assert base is not None
    result = compile_psc(merged, imports=[str(source_root), str(SOURCE_ROOT), str(base)], game="fo4", flags=str(base / "Institute_Papyrus_Flags.flg"), source_path=str(relative))
    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, f"{script_name}\n{diagnostics}"
    assert result.pex_bytes is not None
