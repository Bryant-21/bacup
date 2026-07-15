from __future__ import annotations

import os
from pathlib import Path

import pytest

from bacup_lib.workflows.unified import (
    _iter_top_level_papyrus_members,
    _merge_script_method_patches,
    _script_patch_source,
)
from creation_lib.pex import decompile_pex
from creation_lib.pex.native_runtime import compile_psc


REPO_ROOT = Path(__file__).resolve().parents[5]
OLD_PACKAGE_PEX_ROOT = (
    REPO_ROOT
    / "mods"
    / "SeventySixOld"
    / "data"
    / "Scripts"
    / "fragments"
    / "packages"
)

PATCH_CASES = (
    "PF_AC_MQ01_Opportunity_Scave_006C2D35",
    "PF_AC_MQ02_Stage_Evelyn_Trav_006FC62A",
    "PF_AC_MQ02_Stage_Evelyn_Trav_00727F28",
    "PF_AC_MQ02_Stage_ShowmanSnit_0075222E",
    "PF_AC_MQ02_Stage_Stan_Travel_0075F8A3",
    "PF_AC_MQ02_Stage_TravelToCas_006F3E70",
    "PF_AC_MQ02_Stage_Zayde_RunTo_0072637E",
    "PF_AC_MQ04_AbbieDespawnBadEn_006F9A09",
    "PF_AC_SQ03_Custodial_Client__00747873",
    "PF_AC_SQ03_Custodial_Sam_Tra_0072A608",
    "PF_AC_SQ04_Reopening_Justice_00739A26",
    "PF_AC_SQ04_Reopening_Raider__0073AB1D",
    "PF_BS01_MQ07_Over_Package_Tr_005E9EB2",
    "PF_COMP_Lite_RaiderPunk_Pack_005726BF",
    "PF_E05_Caravan_Guard_TravelT_00596572",
    "PF_E05_Caravan_PostGame_Trav_0058925B",
    "PF_GHL00_Quest_Package_Asher_007A1995",
    "PF_GHL00_Quest_Package_Asher_007A252C",
    "PF_GHL00_Quest_Package_Madel_00799D57",
    "PF_GHL00_Quest_TransformPlay_007A86EE",
    "PF_GHL00_Quest_TransformPlay_007ABDF5",
    "PF_W05_MQ_101P_A_TravelToD_0041B853_1",
    "PF_XPD_AC01_SalDespawn_Packa_006F58A7",
    "PF_XPD_AC02_Package_DefendNP_006E83E3",
    "PF_XPD_AC02_Package_JullianW_006D37BA",
    "PF_XPD_AC02_Package_Twins_Tr_00732815",
    "PF_XPD_AC03_DefendNPC_PostMo_0075F771",
    "PF_XPD_Pitt02_Package_Labore_00666FF0",
    "PF_XPD_Pitt02_Package_Skepti_00661875",
    "PF_XPD_Pitt02_Package_Skepti_00661876",
)


def _script_name(base_name: str) -> str:
    return f"Fragments:Packages:{base_name}"


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


def _merged_old_source(base_name: str) -> str:
    pex_path = OLD_PACKAGE_PEX_ROOT / f"{base_name}.pex"
    assert pex_path.is_file(), f"old package PEX unavailable: {pex_path}"
    skeleton = decompile_pex(pex_path, fo4_api_compat=True)
    patch = _script_patch_source(_script_name(base_name))
    assert patch is not None
    return _merge_script_method_patches(skeleton, patch)


@pytest.mark.parametrize("base_name", PATCH_CASES)
def test_package_patch_restores_end_disable_fragment(base_name: str):
    patch = _script_patch_source(_script_name(base_name))

    assert patch is not None
    assert not any(
        line.strip().lower().startswith("scriptname ")
        for line in patch.splitlines()
    )
    members = [
        (kind, name)
        for kind, name, _start, _end in _iter_top_level_papyrus_members(
            patch.splitlines()
        )
    ]
    assert members == [("function", "fragment_end")]
    assert "Function Fragment_End(Actor akActor)" in patch
    assert "akActor.Disable()" in patch


@pytest.mark.parametrize("base_name", PATCH_CASES)
def test_package_patch_merges_and_compiles_against_old_production_pex(base_name: str):
    base_source = _fo4_base_source()
    if base_source is None:
        pytest.skip("FO4 base Papyrus sources unavailable")

    result = compile_psc(
        _merged_old_source(base_name),
        imports=[str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=f"Fragments/Packages/{base_name}.psc",
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None


def test_package_patch_batch_contains_only_confirmed_propertyless_disable_cases():
    assert len(PATCH_CASES) == 30
