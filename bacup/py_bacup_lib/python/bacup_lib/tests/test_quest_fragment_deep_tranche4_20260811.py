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
    "Fragments:Quests:QF_AC_MQ01_Opportunity_OnIns_0074D9A7": {"fragment_stage_0100_item_00"},
    "Fragments:Quests:QF_BS01_Dialogue_Rahmani_005C6571": {"fragment_stage_0200_item_00"},
    "Fragments:Quests:QF_GHL00_Quest_OnIncreaseLev_0078DB37": {"fragment_stage_0100_item_00"},
    "Fragments:Quests:QF_M01C_Archery_00417BE8": {"fragment_stage_1000_item_00", "fragment_stage_1100_item_00", "fragment_stage_9000_item_00"},
    "Fragments:Quests:QF_M01C_Roboticist_00417C0A": {"fragment_stage_0100_item_00", "fragment_stage_0110_item_00", "fragment_stage_0120_item_00", "fragment_stage_0130_item_00", "fragment_stage_0140_item_00", "fragment_stage_0150_item_00", "fragment_stage_0160_item_00", "fragment_stage_0170_item_00", "fragment_stage_0180_item_00", "fragment_stage_9000_item_00"},
    "Fragments:Quests:QF_M01C_Swimming_004178BD": {"fragment_stage_0100_item_00", "fragment_stage_0900_item_00", "fragment_stage_0998_item_00", "fragment_stage_0999_item_00"},
    "Fragments:Quests:qf_mtnz05_messenger_0001a634": {"fragment_stage_0400_item_00"},
    "Fragments:Quests:QF_MTR11_Delve_002B7C4D": {"fragment_stage_0200_item_00", "fragment_stage_0400_item_00", "fragment_stage_0600_item_00"},
    "Fragments:Quests:QF_P01B_Master_0047F444": {"fragment_stage_0002_item_00", "fragment_stage_0010_item_00", "fragment_stage_0020_item_00", "fragment_stage_0030_item_00", "fragment_stage_0040_item_00", "fragment_stage_0050_item_00", "fragment_stage_0060_item_00", "fragment_stage_0070_item_00", "fragment_stage_0080_item_00"},
}


def _members(source: str) -> list[str]:
    return [
        name
        for _kind, name, _start, _end in _iter_top_level_papyrus_members(
            source.splitlines()
        )
    ]


def _merged(script_name: str) -> str:
    patch = _script_patch_source(script_name)
    assert patch is not None
    source = (SOURCE_ROOT / _script_relative_path(script_name, ".psc")).read_text(encoding="utf-8")
    return _merge_script_method_patches(source, patch)


def test_deep_tranche4_manifest_and_ledger_classification():
    assert len(SCRIPT_MEMBERS) == 9
    assert sum(map(len, SCRIPT_MEMBERS.values())) == 33
    with LEDGER_PATH.open(encoding="utf-8", newline="") as stream:
        rows = [row for row in csv.DictReader(stream) if row["evidence_gate"].startswith("tranche-4 ")]
    assert len(rows) == 9
    assert sum(row["disposition"] == "patched-local-quest-action" for row in rows) == 4
    assert sum(row["disposition"] == "partial-local-quest-action" for row in rows) == 5
    by_script = {row["generated_file"]: row for row in rows}
    assert "intentional dummy/no-op nondefect" in by_script["QF_M01C_Roboticist_00417C0A.psc"]["evidence_gate"]
    assert "partial 3/31" in by_script["QF_MTR11_Delve_002B7C4D.psc"]["evidence_gate"]
    assert "partial 9/35" in by_script["QF_P01B_Master_0047F444.psc"]["evidence_gate"]


def test_deep_tranche4_source_only_owners_remain_blocked():
    with LEDGER_PATH.open(encoding="utf-8", newline="") as stream:
        rows = {row["generated_file"]: row for row in csv.DictReader(stream)}
    for script in ("qf_bosr01_00311433.psc", "qf_bosr02_000044d0.psc"):
        assert rows[script]["disposition"] == "blocked-insufficient-body-evidence"
        assert "current converted SeventySix.esm has no" in rows[script]["evidence_gate"]


@pytest.mark.parametrize(("script_name", "expected"), SCRIPT_MEMBERS.items())
def test_deep_tranche4_member_only_merge_is_idempotent(script_name: str, expected: set[str]):
    patch = _script_patch_source(script_name)
    assert patch is not None
    assert len(_members(patch)) == len(expected)
    assert set(_members(patch)) == expected
    assert not any(line.strip().lower().startswith(("scriptname ", "state ", "auto state ")) for line in patch.splitlines())
    merged = _merged(script_name)
    for member in expected:
        assert sum(name == member for name in _members(merged)) == 1
    assert _merge_script_method_patches(merged, patch) == merged


def test_deep_tranche4_p01b_story_keyword_mapping():
    patch = _script_patch_source("Fragments:Quests:QF_P01B_Master_0047F444")
    assert patch is not None
    mapping = {2: "Keyword_Lying_01", 10: "keyword_Random03", 20: "keyword_Random04", 30: "keyword_Random02", 40: "keyword_Random01", 50: "keyword_Squatch01", 60: "keyword_Albino01", 70: "keyword_Robot01", 80: "Keyword_Lying_02"}
    for stage, keyword in mapping.items():
        member = f"Function Fragment_Stage_{stage:04d}_Item_00()"
        send = f"{keyword}.SendStoryEventAndWait(None, playerRef)"
        assert member in patch
        assert send in patch


def test_deep_tranche4_scene_and_av_semantics():
    roboticist = _script_patch_source("Fragments:Quests:QF_M01C_Roboticist_00417C0A")
    archery = _script_patch_source("Fragments:Quests:QF_M01C_Archery_00417BE8")
    swimming = _script_patch_source("Fragments:Quests:QF_M01C_Swimming_004178BD")
    assert roboticist and archery and swimming
    for scene in ("Start", "Monorail", "TortureRoom", "RoboticsResearchLab", "RoboticsAssembly", "QualityControl", "EyebotStatue", "RobobrainAssembly", "ManagementOffices", "End"):
        assert f"M01C_Roboticist_{scene}_Scene.Start()" in roboticist
    assert archery.count("SetValue(M01C_Archery_HasFailed, 1.0)") == 2
    assert archery.count("SetValue(M01C_Archery_HasFailed, 2.0)") == 1
    assert swimming.count("SetValue(AV_HasFailed, 1.0)") == 3
    assert "Scene_Start.Start()" in swimming


@pytest.mark.parametrize("script_name", SCRIPT_MEMBERS)
def test_deep_tranche4_merged_full_source_native_compiles(script_name: str, tmp_path: Path):
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
