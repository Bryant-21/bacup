from __future__ import annotations

from pathlib import Path

import pytest

from bacup_lib.tests.test_terminal_fragment_script_patches import _fo4_base_source
from bacup_lib.workflows.unified import (
    _iter_top_level_papyrus_members,
    _script_patch_source,
    _script_relative_path,
)
from creation_lib.pex.native_runtime import compile_psc


REPO_ROOT = Path(__file__).resolve().parents[5]
SOURCE_ROOT = REPO_ROOT / "mods" / "SeventySix" / "Scripts" / "Source" / "User"
CONTRACT_ROOT = REPO_ROOT / "bacup" / "docs" / "stub_restoration" / "contracts"
SCENE_CONTRACT = CONTRACT_ROOT / "w05-scene-quest-dependency-repairs-2026-09-01.md"
RESIDUAL_CONTRACT = (
    CONTRACT_ROOT / "residual-local-dependency-repairs-2026-09-01.md"
)

CUT_SETTLER_DAILIES = (
    "Fragments:Quests:QF_W05_SettlersDaily_Clinic_003F2DC7",
    "Fragments:Quests:QF_W05_SettlersDaily_Fieldha_00403436",
    "Fragments:Quests:QF_W05_SettlersDaily_Restock_0041B725",
    "Fragments:Quests:QF_W05_SettlersDaily_Stew_003F2DC9",
)
NATIVE_OR_SUPERSEDED_CARRIERS = (
    "Fragments:Quests:QF_W05_DialogueRaiderRC_Retu_00402CA0",
    "Fragments:Packages:PF_W05_Daily_R02_Package_Tra_00555E4E",
    "Fragments:TopicInfos:TIF_W05_DialogueRadicals_Ext_005895B2",
)
ALL_NONDEFECTS = CUT_SETTLER_DAILIES + NATIVE_OR_SUPERSEDED_CARRIERS
RETIREMENT_QUEST = "Fragments:Quests:QF_W05_Daily_R02_Retirement_0054DCF9"


def _generated_source(script_name: str) -> str:
    path = SOURCE_ROOT / _script_relative_path(script_name, ".psc")
    return path.read_text(encoding="utf-8")


def _member_body(source: str, member_name: str) -> str:
    lines = source.splitlines()
    start, end = next(
        (start, end)
        for _kind, name, start, end in _iter_top_level_papyrus_members(lines)
        if name == member_name.lower()
    )
    return "\n".join(lines[start : end + 1])


@pytest.mark.parametrize("script_name", ALL_NONDEFECTS)
def test_source_carriers_remain_memberless_and_unpatched(script_name: str) -> None:
    source = _generated_source(script_name)

    assert _script_patch_source(script_name) is None
    assert list(_iter_top_level_papyrus_members(source.splitlines())) == []


@pytest.mark.parametrize(
    "script_name",
    (
        "Fragments:Packages:PF_W05_Daily_R02_Package_Tra_00555E4E",
        "Fragments:TopicInfos:TIF_W05_DialogueRadicals_Ext_005895B2",
    ),
)
def test_propertyless_callbacks_do_not_gain_invented_operands(script_name: str) -> None:
    source = _generated_source(script_name).lower()

    assert " property " not in source
    assert "function " not in source
    assert "event " not in source


def test_retirement_stage_1330_supersedes_the_propertyless_package_end() -> None:
    patch = _script_patch_source(RETIREMENT_QUEST)
    assert patch is not None

    body = _member_body(patch, "fragment_stage_1330_item_00")
    assert body.splitlines() == [
        "Function Fragment_Stage_1330_Item_00()",
        "    SetStage(1000)",
        "EndFunction",
    ]


def test_checked_in_contracts_preserve_the_seven_nondefect_dispositions() -> None:
    scene_contract = SCENE_CONTRACT.read_text(encoding="utf-8")
    residual_contract = RESIDUAL_CONTRACT.read_text(encoding="utf-8")

    assert "scene\n`402CAD` uses `BeginOnQuestStart`" in scene_contract
    assert "Story Manager routing, not this Papyrus fragment" in scene_contract
    assert "stage-1330 fragment performs the required `SetStage(1000)`" in scene_contract
    assert "All four are `zzz_` crafting quests" in scene_contract
    assert "single-player quest-start owner" not in scene_contract

    assert "owner has no stages for this INFO to advance" in residual_contract
    assert "Recommended status:** `non-defect`" in residual_contract
    assert "choosing a stage from narrative proximity" not in residual_contract


@pytest.mark.parametrize("script_name", ALL_NONDEFECTS)
def test_memberless_carriers_compile_for_fo4(script_name: str) -> None:
    base_source = _fo4_base_source()
    if base_source is None:
        pytest.skip("FO4 base Papyrus sources unavailable")

    result = compile_psc(
        _generated_source(script_name),
        imports=[str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=f"{script_name.rsplit(':', 1)[-1]}.psc",
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None
