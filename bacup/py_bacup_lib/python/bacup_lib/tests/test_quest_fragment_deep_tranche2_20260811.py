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
PATCH_ROOT = (
    REPO_ROOT
    / "bacup"
    / "py_bacup_lib"
    / "python"
    / "bacup_lib"
    / "script_patches"
    / "Fragments"
    / "Quests"
)
LEDGER_PATH = (
    REPO_ROOT
    / "bacup"
    / "docs"
    / "stub_restoration"
    / "contracts"
    / "quest-fragment-production-tranche-2026-08-11.csv"
)

AUDITED_STEMS = (
    "QF_BS_RE_AssaultCMB01_005C8E8A",
    "QF_BS_RE_AssaultCMB02_005EFA80",
    "QF_BS_RE_AssaultCMB03_005EFA81",
    "QF_BS_RE_AssaultCMB04_005EFA84",
    "QF_BS_RE_CampDWD01_005CBE85",
    "QF_BS_RE_CampJN01_005C5F59",
    "QF_BS_RE_SceneJN01_005CB063",
    "QF_BS_RE_TravelCMB01_005C8DBC",
    "QF_BS_RE_TravelCMB02_005EDFE4",
    "QF_BS_RE_TravelCMB03_005EDFE5",
    "QF_BS_RE_TravelCMB04_005EDFE6",
    "QF_BS_RE_TravelCMB05_005EFA82",
    "QF_BS_RE_TravelCMB06_005EFA83",
    "QF_BS_RE_TravelDWD01_005C5497",
    "QF_BS_RE_TravelDWD02_005C7186",
    "QF_BS_RE_TravelDWD03_005C751B",
    "QF_BS_RE_TravelJN01_005CB062",
    "QF_Burn_RE_Assault_ArmouredD_0081EFC2",
    "QF_Burn_RE_Assault_Deathclaw_0081EFBE",
    "QF_Burn_RE_Assault_Deathclaw_0081EFC0",
    "QF_Burn_RE_Assault_MT01_00876382",
    "QF_Burn_RE_Assault_MT02_00876383",
    "QF_Burn_RE_Assault_RKR_EncRa_0081EFC1",
    "QF_Burn_RE_Assault_RKR_Ogua_0081E20C",
    "QF_Burn_RE_Assault_RKR_Sting_0081EFC4",
    "QF_Burn_RE_Assault_Stingwing_0081EFBF",
    "QF_Burn_RE_Assault_Stingwing_0081EFC3",
    "QF_Burn_RE_Object_GR_01_00834CFD",
    "QF_Burn_RE_Object_GR_04_008457B3",
    "QF_Burn_RE_Object_LovelandFr_0081ACCE",
    "QF_Burn_RE_Object_ShippingCo_0081ACCD",
    "QF_Burn_RE_Scene_BadDeathcla_00823682",
    "QF_Burn_RE_Scene_BuriedGhoul_0082114E",
    "QF_Burn_RE_Scene_GO01_0083D29B",
    "QF_Burn_RE_Scene_MT01_00876384",
    "QF_Burn_RE_Scene_RK01_008261A4",
    "QF_Burn_RE_Scene_ThirstyPers_0081ACB2",
    "QF_Burn_RE_Travel_KK01_008438FA",
    "QF_Burn_RE_Travel_MT01_00876380",
    "QF_Burn_RE_Travel_MT02_00876381",
    "QF_Burn_RE_Travel_Propaganda_0081ACD1",
    "QF_Burn_RE_Travel_Travelling_0081ACB0",
    "qf_re_assaultdwd05_005901c8",
    "qf_re_assaultdwd06_005901c9",
    "qf_re_assaultdwd07_005901ca",
    "qf_re_objectdwd04_005910ff",
    "qf_re_objectmp09_0035ec0f",
    "qf_re_objectzw01_0056368f",
    "qf_re_objectzw02_0056407e",
    "qf_re_scene_jp01_0055c542",
    "qf_re_scene_jp02_0055c541",
    "qf_re_sceneaf01_0055c408",
    "qf_re_sceneaf02_0055c407",
    "qf_re_sceneaf03_0055c409",
    "qf_re_scenesm04_004845aa",
    "qf_re_scenets02_0052b8e4",
    "qf_re_traveldwd04_005902ee",
    "qf_re_traveldwd06_005902f0",
    "qf_re_traveldwd07_005902f1",
)

TEMPLATE_REJECTED_STEMS = {
    "QF_BS_RE_AssaultCMB01_005C8E8A",
    "QF_BS_RE_AssaultCMB02_005EFA80",
    "QF_BS_RE_AssaultCMB03_005EFA81",
    "QF_BS_RE_AssaultCMB04_005EFA84",
    "QF_BS_RE_CampDWD01_005CBE85",
    "QF_BS_RE_CampJN01_005C5F59",
    "QF_BS_RE_SceneJN01_005CB063",
    "QF_Burn_RE_Assault_MT01_00876382",
    "QF_Burn_RE_Assault_MT02_00876383",
    "QF_Burn_RE_Object_GR_01_00834CFD",
    "QF_Burn_RE_Object_GR_04_008457B3",
    "qf_re_assaultdwd05_005901c8",
    "qf_re_assaultdwd06_005901c9",
    "qf_re_assaultdwd07_005901ca",
    "qf_re_objectdwd04_005910ff",
    "qf_re_objectzw01_0056368f",
    "qf_re_objectzw02_0056407e",
    "qf_re_scene_jp01_0055c542",
    "qf_re_scene_jp02_0055c541",
    "qf_re_sceneaf01_0055c408",
    "qf_re_sceneaf02_0055c407",
    "qf_re_sceneaf03_0055c409",
    "qf_re_scenesm04_004845aa",
    "qf_re_scenets02_0052b8e4",
}

NO_RUN_ON_STOP_STEMS = {"QF_Burn_RE_Travel_Travelling_0081ACB0"}

PARTIAL_STEMS = {
    "QF_Burn_RE_Assault_ArmouredD_0081EFC2",
    "QF_Burn_RE_Assault_Deathclaw_0081EFBE",
    "QF_Burn_RE_Assault_Deathclaw_0081EFC0",
    "QF_Burn_RE_Assault_RKR_EncRa_0081EFC1",
    "QF_Burn_RE_Assault_RKR_Ogua_0081E20C",
    "QF_Burn_RE_Assault_RKR_Sting_0081EFC4",
    "QF_Burn_RE_Assault_Stingwing_0081EFBF",
    "QF_Burn_RE_Assault_Stingwing_0081EFC3",
}

REJECTED_STEMS = TEMPLATE_REJECTED_STEMS | NO_RUN_ON_STOP_STEMS
PATCH_STEMS = tuple(stem for stem in AUDITED_STEMS if stem not in REJECTED_STEMS)
SCRIPT_NAMES = tuple(f"Fragments:Quests:{stem}" for stem in PATCH_STEMS)


def _member_names(source: str) -> list[str]:
    return [
        name
        for _kind, name, _start, _end in _iter_top_level_papyrus_members(
            source.splitlines()
        )
    ]


def _merged_source(script_name: str) -> str:
    source_path = SOURCE_ROOT / _script_relative_path(script_name, ".psc")
    patch = _script_patch_source(script_name)
    assert source_path.is_file(), source_path
    assert patch is not None
    return _merge_script_method_patches(
        source_path.read_text(encoding="utf-8"), patch
    )


def test_deep_tranche2_ledger_has_exact_semantic_dispositions():
    with LEDGER_PATH.open(encoding="utf-8", newline="") as stream:
        rows = list(csv.DictReader(stream))

    audited_rows = [
        row for row in rows if row["evidence_gate"].startswith("tranche-2 ")
    ]
    assert len(audited_rows) == 59
    assert {Path(row["generated_file"]).stem.casefold() for row in audited_rows} == {
        stem.casefold() for stem in AUDITED_STEMS
    }

    retained_rows = [
        row
        for row in audited_rows
        if row["disposition"] in {"patched-local-trigger", "partial-local-trigger"}
    ]
    assert len(retained_rows) == 34
    assert sum(row["disposition"] == "patched-local-trigger" for row in retained_rows) == 26
    assert sum(row["disposition"] == "partial-local-trigger" for row in retained_rows) == 8
    assert all("non-template Filter + RunOnStop" in row["evidence_gate"] for row in retained_rows)

    template_rows = [
        row
        for row in audited_rows
        if Path(row["generated_file"]).stem in TEMPLATE_REJECTED_STEMS
    ]
    assert len(template_rows) == 24
    assert all(row["disposition"] == "intentional-unused" for row in template_rows)
    assert all("_Templates" in row["evidence_gate"] for row in template_rows)

    no_run_on_stop_rows = [
        row
        for row in audited_rows
        if Path(row["generated_file"]).stem in NO_RUN_ON_STOP_STEMS
    ]
    assert len(no_run_on_stop_rows) == 1
    assert no_run_on_stop_rows[0]["disposition"] == "blocked-insufficient-body-evidence"
    assert "lacks RunOnStop" in no_run_on_stop_rows[0]["evidence_gate"]


@pytest.mark.parametrize("stem", sorted(REJECTED_STEMS))
def test_deep_tranche2_semantic_rejections_have_no_patch(stem: str):
    assert not (PATCH_ROOT / f"{stem}.psc").exists()


@pytest.mark.parametrize("script_name", SCRIPT_NAMES)
def test_deep_tranche2_patch_is_member_only_and_rearms_trigger(script_name: str):
    patch = _script_patch_source(script_name)
    assert patch is not None
    assert _member_names(patch) == ["fragment_stage_1000_item_00"]
    assert "Alias_TRIGGER.GetReference() as RETriggerScript" in patch
    assert "If triggerScript != None" in patch
    assert "triggerScript.ReArmTrigger()" in patch
    assert ".Disable()" not in patch
    assert not any(
        line.strip().lower().startswith(("scriptname ", "state ", "auto state "))
        for line in patch.splitlines()
    )

    source_path = SOURCE_ROOT / _script_relative_path(script_name, ".psc")
    source = source_path.read_text(encoding="utf-8")
    assert "referencealias property alias_trigger" in source.casefold()

    merged = _merged_source(script_name)
    assert _member_names(merged).count("fragment_stage_1000_item_00") == 1
    assert _merge_script_method_patches(merged, patch) == merged


@pytest.mark.parametrize("script_name", SCRIPT_NAMES)
def test_deep_tranche2_merged_full_source_native_compiles(
    script_name: str, tmp_path: Path
):
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
