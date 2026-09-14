from __future__ import annotations

import csv
import os
from pathlib import Path

import pytest

from bacup_lib.workflows.unified import (
    _merge_script_method_patches,
    _script_patch_source,
)
from creation_lib.pex.native_runtime import compile_psc


REPO_ROOT = Path(__file__).resolve().parents[5]
SOURCE_ROOT = REPO_ROOT / "mods" / "SeventySix" / "Scripts" / "Source" / "User"
ROW_LEDGER = (
    REPO_ROOT
    / "bacup"
    / "docs"
    / "stub_restoration"
    / "contracts"
    / "nonquest-fragment-row-ledger-2026-08-11.csv"
)
AMBUSH_SCRIPT = "Fragments:Packages:PF_AmbushFromLinkRefChain_001B5900"
PACKAGE_CASES = {
    AMBUSH_SCRIPT: (
        "Function Fragment_End(Actor akActor)",
        "akActor.SetValue(AmbushRelease, 1.0)",
    ),
    "Fragments:Packages:PF_ATX_COMP_Inspector_Daphne_0061F708": (
        "Function Fragment_Begin(Actor akActor)",
        "akActor.UnequipItem(newClothes, False, True)",
        "akActor.EquipItem(oldClothes, False, True)",
    ),
    "Fragments:Packages:PF_ATX_COMP_MasterPackage_In_0061F709": (
        "Function Fragment_Begin(Actor akActor)",
        "akActor.EquipItem(newClothes, False, True)",
    ),
    "Fragments:Packages:PF_E01B_Herd_BrahminTravelPa_004787CC": (
        "Function Fragment_End(Actor akActor)",
        "akActor.RemoveKeyword(E01B_Herd_ScaredBrahminKeyword)",
        "akActor.EvaluatePackage()",
    ),
    "Fragments:Packages:PF_FF09_ShutDown_002B00CE": (
        "Function Fragment_Begin(Actor akActor)",
        "Actor botRef = bot.GetActorReference()",
        "botRef.Disable()",
    ),
    "Fragments:Packages:PF_XPD_AC02_ShowmanLeaveAfte_006BAB20": (
        "Function Fragment_End(Actor akActor)",
        "Actor showman = IntroShowman.GetActorReference()",
        "showman.Disable()",
    ),
}
EVIDENCE_BLOCKED_DISABLE_PACKAGES = (
    "Fragments:Packages:PF_BS01_MQ06A_Raiders_Packag_005D2B38",
    "Fragments:Packages:PF_COMP_Astronaut_Package_As_00573B15",
    "Fragments:Packages:PF_COMP_Astronaut_Package_As_00573B17",
    "Fragments:Packages:PF_COMP_Astronaut_Package_Em_00573B12",
    "Fragments:Packages:PF_COMP_Astronaut_Package_Em_00573B16",
    "Fragments:Packages:PF_COMP_Astronaut_Package_Pa_00573B11",
    "Fragments:Packages:PF_FFZ16_Swatter_VertibirdLe_002C0883",
    "Fragments:Packages:PF_RS02_Beat_SteelheartGoHom_002BA515",
    "Fragments:Packages:PF_W05_Beckett_FinalAlliesLe_005A13C2",
    "Fragments:Packages:PF_W05_Daily_R02_Package_Tra_00555E4E",
    "Fragments:Packages:PF_W05_MQ_002P_Wayward_Secon_0040F690",
    "Fragments:Packages:PF_W05_MQ_003P_Muscle_SolExi_0041A4E5",
    "Fragments:Packages:PF_W05_MQ_004P_Crane_ExitWay_0055ADC7",
    "Fragments:Packages:PF_W05_MQR_203P_8100_CrowdMe_00594A84",
    "Fragments:Packages:PF_W05_MQR_203P_Sargento_Exi_0042F5D1",
    "Fragments:Packages:PF_W05_MQS_205P_07_PennyLeav_00570D66",
    "Fragments:Packages:PF_XPD_AC01_AuditorsExit_Pac_0074428C",
    "Fragments:Packages:PF_XPD_AC01_TaxmanLeave_Pack_006F274B",
    "Fragments:Packages:PF_XPD_AC02_Package_NaughtyS_006E1D60",
    "Fragments:Packages:PF_XPD_AC02_Package_NaughtyS_006E1D61",
    "Fragments:Packages:PF_XPD_AC02_Package_NaughtyS_006E1D62",
    "Fragments:Packages:PF_XPD_AC02_Package_NaughtyS_006E2AAD",
    "Fragments:Packages:PF_XPD_ObjMod_FreePrisoners__006464EC",
)
SCENE_CASES = {
    "Fragments:Scenes:SF_BS02_MQ03_Tunnel_AriesRea_005FB561": (
        "Function Fragment_Phase_01_Begin()",
        "rahmani.MoveTo(doorSpot)",
    ),
    "Fragments:Scenes:SF_FF08_Initalizing_0035678A": (
        "Function Fragment_Phase_01_Begin()",
        "pharmabotRef.PlayIdle(InitIdle)",
    ),
    "Fragments:Scenes:SF_FF11_Raid_VertibotScene_0055CCED": (
        "Function Fragment_Phase_01_End()",
        "vertibot.SetValue(VertibirdLand, 1.0)",
        "vertibot.EvaluatePackage()",
    ),
    "Fragments:Scenes:SF_W05_RE_TravelAF02_KillAct_0056A249": (
        "Function Fragment_Action_01()",
        "scavengerRef.SetValue(Health, 0.0)",
    ),
}
STOP_SCENES = (
    "Fragments:Scenes:SF_E05_Radiation_FailureScen_0056FB66",
    "Fragments:Scenes:SF_M01C_Archery_Failure_0041F517",
    "Fragments:Scenes:SF_M01C_Archery_Success_00417C09",
)


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


def _merged_ambush_source() -> str:
    skeleton_path = (
        SOURCE_ROOT
        / "Fragments"
        / "Packages"
        / "PF_AmbushFromLinkRefChain_001B5900.psc"
    )
    patch = _script_patch_source(AMBUSH_SCRIPT)
    assert patch is not None
    return _merge_script_method_patches(
        skeleton_path.read_text(encoding="utf-8"), patch
    )


def _merged_source(script_name: str) -> str:
    relative = Path(*script_name.split(":"))
    skeleton_path = SOURCE_ROOT / relative.with_suffix(".psc")
    patch = _script_patch_source(script_name)
    assert patch is not None
    return _merge_script_method_patches(
        skeleton_path.read_text(encoding="utf-8"), patch
    )


def test_ambush_package_fragment_merges_once_with_bound_actor_value_guard():
    merged = _merged_ambush_source()

    assert merged.count("Function Fragment_End(Actor akActor)") == 1
    assert "akActor != None && AmbushRelease != None" in merged
    assert "akActor.SetValue(AmbushRelease, 1.0)" in merged
    assert _merge_script_method_patches(
        merged, _script_patch_source(AMBUSH_SCRIPT)
    ) == merged


def test_ambush_package_fragment_native_compiles_for_fo4():
    base_source = _fo4_base_source()
    if base_source is None:
        pytest.skip("FO4 base Papyrus sources unavailable")

    result = compile_psc(
        _merged_ambush_source(),
        imports=[str(base_source), str(SOURCE_ROOT)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path="Fragments/Packages/PF_AmbushFromLinkRefChain_001B5900.psc",
    )
    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None


@pytest.mark.parametrize("script_name", PACKAGE_CASES)
def test_local_package_batch_merges_once(script_name: str):
    merged = _merged_source(script_name)
    for snippet in PACKAGE_CASES[script_name]:
        assert snippet in merged
    assert _merge_script_method_patches(
        merged, _script_patch_source(script_name)
    ) == merged


@pytest.mark.parametrize("script_name", PACKAGE_CASES)
def test_local_package_batch_native_compiles_for_fo4(script_name: str):
    base_source = _fo4_base_source()
    if base_source is None:
        pytest.skip("FO4 base Papyrus sources unavailable")
    result = compile_psc(
        _merged_source(script_name),
        imports=[str(base_source), str(SOURCE_ROOT)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=str(Path(*script_name.split(":")).with_suffix(".psc")),
    )
    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None


@pytest.mark.parametrize("script_name", EVIDENCE_BLOCKED_DISABLE_PACKAGES)
def test_propertyless_exit_package_callbacks_remain_unpatched(script_name: str):
    assert _script_patch_source(script_name) is None


@pytest.mark.parametrize("script_name", SCENE_CASES)
def test_local_scene_batch_merges_once(script_name: str):
    merged = _merged_source(script_name)
    for snippet in SCENE_CASES[script_name]:
        assert snippet in merged
    assert _merge_script_method_patches(
        merged, _script_patch_source(script_name)
    ) == merged


@pytest.mark.parametrize("script_name", SCENE_CASES)
def test_local_scene_batch_native_compiles_for_fo4(script_name: str):
    base_source = _fo4_base_source()
    if base_source is None:
        pytest.skip("FO4 base Papyrus sources unavailable")
    result = compile_psc(
        _merged_source(script_name),
        imports=[str(base_source), str(SOURCE_ROOT)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=str(Path(*script_name.split(":")).with_suffix(".psc")),
    )
    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None


@pytest.mark.parametrize("script_name", STOP_SCENES)
def test_result_scene_batch_merges_once_and_stops_owner(script_name: str):
    merged = _merged_source(script_name)
    assert merged.count("Function Fragment_Phase_01_End()") == 1
    assert "Quest owningQuest = GetOwningQuest()" in merged
    assert "owningQuest.Stop()" in merged
    assert _merge_script_method_patches(
        merged, _script_patch_source(script_name)
    ) == merged


@pytest.mark.parametrize("script_name", STOP_SCENES)
def test_result_scene_batch_native_compiles_for_fo4(script_name: str):
    base_source = _fo4_base_source()
    if base_source is None:
        pytest.skip("FO4 base Papyrus sources unavailable")
    result = compile_psc(
        _merged_source(script_name),
        imports=[str(base_source), str(SOURCE_ROOT)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=str(Path(*script_name.split(":")).with_suffix(".psc")),
    )
    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None


def test_remaining_perk_candidates_are_intact_or_nonproduction_shells():
    perk_root = SOURCE_ROOT / "Fragments" / "Perks"
    intact_members = {
        "prkf_ac_sq03_custodial_capti_00746186.psc": "Fragment_Entry_00",
        "prkf_ffz10_light_mothmanacti_0038cd2d.psc": "Fragment_Entry_00",
        "prkf_roadkill_perk_008a418f.psc": "Fragment_Entry_00",
        "prkf_testtameperk_00004168.psc": "Fragment_Entry_00",
    }
    for filename, member in intact_members.items():
        assert member in (perk_root / filename).read_text(encoding="utf-8")

    repaired = _merged_source(
        "Fragments:Perks:PRKF_TWZ09AerosilzerKick_002FD347"
    )
    assert "quest Property TWZ09 Auto mandatory" in repaired
    assert "TWZ09.SetStage(10)" in repaired


def test_row_ledger_exhaustively_partitions_the_inventory():
    with ROW_LEDGER.open(encoding="utf-8", newline="") as stream:
        rows = list(csv.DictReader(stream))

    assert len(rows) == 496
    assert len({(row["family"], row["file"].lower()) for row in rows}) == 496
    counts: dict[tuple[str, str], int] = {}
    for row in rows:
        key = (row["family"], row["disposition"])
        counts[key] = counts.get(key, 0) + 1

    assert counts == {
        ("Packages", "evidence-blocked"): 97,
        ("Packages", "missing-record"): 2,
        ("Packages", "no-live-fragment"): 40,
        ("Packages", "online-or-unused"): 4,
        ("Packages", "patched"): 6,
        ("Perks", "intact-source"): 4,
        ("Perks", "patched"): 1,
        ("Scenes", "evidence-blocked"): 94,
        ("Scenes", "missing-record"): 6,
        ("Scenes", "no-live-fragment"): 35,
        ("Scenes", "online-or-unused"): 200,
        ("Scenes", "patched"): 7,
    }


def test_deep_tranche_two_keeps_unproven_fragment_rows_unpatched():
    with ROW_LEDGER.open(encoding="utf-8", newline="") as stream:
        rows = list(csv.DictReader(stream))

    blocked = [row for row in rows if row["disposition"] == "evidence-blocked"]
    assert sum(row["family"] == "Packages" for row in blocked) == 97
    assert sum(row["family"] == "Scenes" for row in blocked) == 94
    assert sum(row["family"] == "Perks" for row in blocked) == 0

    for row in blocked:
        script_name = f"Fragments:{row['family']}:{Path(row['file']).stem}"
        assert _script_patch_source(script_name) is None, script_name
