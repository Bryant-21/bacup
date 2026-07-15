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
DEPLOYED_TERMINAL_ROOT = (
    REPO_ROOT / "mods" / "SeventySix" / "data" / "Scripts" / "fragments" / "terminals"
)

PATCH_CASES = {
    "TERM_FS03_MQ_Fruition_Armory_00002CD5": {1},
    "TERM_FS03_MQ_Fruition_Armory_004EA3B4": {1, 2, 3, 4},
    "TERM_LC043_SecurityTerminalC_0027BA0C": {1, 2, 3},
    "TERM_LC084_nativeControlRoom_004EB1C1": {1, 2, 4},
    "TERM_LC084_nativeManufacturi_004EB1AD": {1, 2, 4},
    "TERM_LC084_nativeReactorRoom_004EB15D": {1, 2, 4},
    "TERM_LC084_nativeResidential_004E7844": {1, 2, 4},
    "TERM_LC084_nativeStorageRobo_004E7849": {1, 2, 4},
    "TERM_LC184_SecurityTerminalH_002E1535": {1, 2, 3},
    "TERM_MSilo_Storage_Facilitie_0051B04A": {1},
    "TERM_nativeBankMegaSecurityD_00176E19": {1, 2, 3, 4},
    "TERM_nativeBlastShieldTermin_002ED0CE": {1, 2, 3, 4},
    "TERM_nativeDeconArchControlT_002ED20B": {1, 2, 3, 4},
    "TERM_nativeDisplayCaseTermin_002ED0CD": {1, 2, 3, 4},
    "TERM_nativeGarageDoorActivat_002ED0ED": {1, 2, 3, 4},
    "TERM_nativeGarageDoorTermina_0002009C": {1, 2, 3, 4},
    "TERM_nativeRobotTerminalSubM_002C506F": {1, 2, 4},
    "TERM_nativeVaultDeconArchCon_003F6D22": {1, 2, 3, 4},
    "term_nativeturretterminalsub_0011c738": {1, 2, 3},
    "TERM_V96_1_Engineering_Decon_00547926": {1, 2, 3, 4, 5},
    "TERM_V96_1_Engineering_Decon_0054AB6D": {1, 2, 3, 4, 5},
    "TERM_V96_1_Engineering_Decon_0054AB6E": {1, 2, 3, 4, 5},
    "TERM_V96_1_Engineering_Decon_0054AB6F": {1, 2, 3, 4, 5},
    "TERM_V96_1_Engineering_Decon_0054AB70": {1, 2, 3, 4, 5},
    "TERM_V96_1_Engineering_Decon_0054AB71": {1, 2, 3, 4, 5},
    "TERM_V96_1_Engineering_Decon_0054AB72": {1, 2, 3, 4, 5},
    "TERM_V96_2_Cryo_CryoOptional_00427832": {2},
}


def _script_name(base_name: str) -> str:
    return f"Fragments:Terminals:{base_name}"


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


def _merged_production_source(base_name: str) -> str:
    pex_path = DEPLOYED_TERMINAL_ROOT / f"{base_name}.pex"
    if not pex_path.is_file():
        pytest.skip(f"deployed production PEX unavailable: {pex_path}")
    skeleton = decompile_pex(pex_path, fo4_api_compat=True)
    patch = _script_patch_source(_script_name(base_name))
    assert patch is not None
    return _merge_script_method_patches(skeleton, patch)


@pytest.mark.parametrize(("base_name", "fragment_ids"), PATCH_CASES.items())
def test_terminal_patch_supplies_every_vmad_fragment(
    base_name: str, fragment_ids: set[int]
):
    patch = _script_patch_source(_script_name(base_name))

    assert patch is not None
    assert not any(
        line.strip().lower().startswith("scriptname ")
        for line in patch.splitlines()
    )
    members = {
        name
        for kind, name, _start, _end in _iter_top_level_papyrus_members(
            patch.splitlines()
        )
        if kind == "function"
    }
    expected = {f"fragment_terminal_{fragment_id:02d}" for fragment_id in fragment_ids}
    assert expected <= members


@pytest.mark.parametrize("base_name", PATCH_CASES)
def test_terminal_production_merge_native_compiles_for_fo4(base_name: str):
    base_source = _fo4_base_source()
    if base_source is None:
        pytest.skip("FO4 base Papyrus sources unavailable")

    result = compile_psc(
        _merged_production_source(base_name),
        imports=[str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=f"Fragments/Terminals/{base_name}.psc",
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None


def test_terminal_patch_count_matches_confirmed_linked_control_batch():
    assert len(PATCH_CASES) == 27
    assert sum(len(fragment_ids) for fragment_ids in PATCH_CASES.values()) == 97


def test_nonsequential_terminal_actions_keep_their_vmad_fragment_numbers():
    armory = _script_patch_source(
        _script_name("TERM_FS03_MQ_Fruition_Armory_004EA3B4")
    )
    bank = _script_patch_source(_script_name("TERM_nativeBankMegaSecurityD_00176E19"))
    decon = _script_patch_source(
        _script_name("TERM_nativeDeconArchControlT_002ED20B")
    )

    assert armory is not None
    assert "Function Fragment_Terminal_02" in armory
    assert "UnlockLinkedDoors(akTerminalRef)" in armory
    assert bank is not None
    assert "Function Fragment_Terminal_02" in bank
    assert "SetLinkedDoorsOpen(akTerminalRef, False)" in bank
    assert decon is not None
    assert "Function Fragment_Terminal_02" in decon
    assert "SetLinkedDeconArchesActive(akTerminalRef, False)" in decon
