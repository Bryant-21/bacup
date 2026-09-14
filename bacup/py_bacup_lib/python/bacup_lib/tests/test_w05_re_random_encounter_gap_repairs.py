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
CONTRACT_ROOT = REPO_ROOT / "bacup" / "docs" / "stub_restoration" / "contracts"
CHEM_VENDOR_INFO = "Fragments:TopicInfos:TIF_W05_RE_CampAF01_0056386D"
SNIPER_TIMER = "W05_RE_ObjectBB02_TimerScript"


def _merged_source(script_name: str) -> tuple[str, str]:
    source_path = SOURCE_ROOT / _script_relative_path(script_name, ".psc")
    patch = _script_patch_source(script_name)
    assert source_path.is_file(), source_path
    assert patch is not None
    source = source_path.read_text(encoding="utf-8")
    return _merge_script_method_patches(source, patch), patch


def _member_names(source: str) -> set[str]:
    return {
        name
        for kind, name, _start, _end in _iter_top_level_papyrus_members(
            source.splitlines()
        )
        if kind in {"function", "event"}
    }


def test_chem_vendor_acceptance_opens_barter_after_the_response_finishes() -> None:
    merged, patch = _merged_source(CHEM_VENDOR_INFO)

    assert "Scriptname " not in patch
    assert _member_names(merged) == {"fragment_end"}
    cast = merged.find("Actor kVendor = akSpeakerRef as Actor")
    guard = merged.find("If kVendor == None")
    wait = merged.find("Utility.Wait(0.25)")
    barter = merged.find("kVendor.ShowBarterMenu()")
    assert -1 not in (cast, guard, wait, barter)
    assert cast < guard < wait < barter
    assert "SetStage(" not in patch


def test_chem_vendor_patch_merges_once_and_native_compiles_for_fo4() -> None:
    base_source = _fo4_base_source()
    if base_source is None:
        pytest.skip("FO4 base Papyrus sources unavailable")

    merged, patch = _merged_source(CHEM_VENDOR_INFO)
    assert _merge_script_method_patches(merged, patch) == merged
    assert merged.lower().count("function fragment_end(") == 1

    result = compile_psc(
        merged,
        imports=[str(SOURCE_ROOT), str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=str(_script_relative_path(CHEM_VENDOR_INFO, ".psc")),
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, f"{CHEM_VENDOR_INFO}:\n{diagnostics}"
    assert result.pex_bytes is not None


def test_sniper_controller_maps_result_stages_to_record_authored_packages() -> None:
    merged, patch = _merged_source(SNIPER_TIMER)

    assert "Scriptname " not in patch
    assert _member_names(merged) == {"setbadguystate", "onstageset"}
    assert "If !IsRunning()" in merged
    assert "BadGuy == None || W05_RE_ObjectBB02_BadGuyAV == None" in merged
    assert "Actor kBadGuy = BadGuy.GetActorReference()" in merged
    assert "kBadGuy.SetValue(W05_RE_ObjectBB02_BadGuyAV, afState)" in merged
    assert "kBadGuy.EvaluatePackage(False)" in merged

    charge = merged.find("auiStageID == 100")
    charge_state = merged.find("SetBadGuyState(1.0)", charge)
    happy = merged.find("auiStageID == 200")
    happy_state = merged.find("SetBadGuyState(2.0)", happy)
    angry = merged.find("auiStageID == 250")
    angry_state = merged.find("SetBadGuyState(3.0)", angry)
    assert -1 not in (charge, charge_state, happy, happy_state, angry, angry_state)
    assert charge < charge_state < happy < happy_state < angry < angry_state
    assert merged.count("SetBadGuyState(") == 4
    assert "SetStage(" not in patch
    assert "StartTimer(" not in patch
    assert "OnTimer(" not in patch


def test_sniper_controller_patch_merges_once_and_native_compiles_for_fo4() -> None:
    base_source = _fo4_base_source()
    if base_source is None:
        pytest.skip("FO4 base Papyrus sources unavailable")

    merged, patch = _merged_source(SNIPER_TIMER)
    assert _merge_script_method_patches(merged, patch) == merged
    assert merged.lower().count("function setbadguystate(") == 1
    assert merged.lower().count("event onstageset(") == 1
    assert "Int Property BadGuyJogHappy = 10 Auto" in merged
    assert "Int Property BadGuyAngry = 20 Auto" in merged

    result = compile_psc(
        merged,
        imports=[str(SOURCE_ROOT), str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=str(_script_relative_path(SNIPER_TIMER, ".psc")),
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, f"{SNIPER_TIMER}:\n{diagnostics}"
    assert result.pex_bytes is not None


def test_checked_in_random_encounter_contracts_match_the_reviewed_dispositions() -> None:
    category_contract = (
        CONTRACT_ROOT / "wastelanders-random-encounters-2026-09-03.md"
    ).read_text(encoding="utf-8")
    dialogue_contract = (CONTRACT_ROOT / "w3b-re-dtr-b.md").read_text(
        encoding="utf-8"
    )
    sniper_contract = (
        CONTRACT_ROOT / "w05-re-objectbb02-controller-repairs-2026-09-01.md"
    ).read_text(encoding="utf-8")
    toy_contract = (
        CONTRACT_ROOT / "w05-re-objectbb01-toy-repairs-2026-09-01.md"
    ).read_text(encoding="utf-8")
    camp_contract = (
        CONTRACT_ROOT / "w05-camp-object-controller-recheck-2026-09-01.md"
    ).read_text(encoding="utf-8")
    dependency_contract = (
        CONTRACT_ROOT / "w05-re-dependency-repairs-2026-09-01.md"
    ).read_text(encoding="utf-8")
    inventory_contract = (
        CONTRACT_ROOT / "w05-re-objectbb02-inventory-repairs-2026-09-01.md"
    ).read_text(encoding="utf-8")
    infrastructure_contract = (CONTRACT_ROOT / "w3b-re-infra.md").read_text(
        encoding="utf-8"
    )

    assert "MaxConcurrentQuests: 0" in category_contract
    assert "This is cut/unstarted content" in category_contract
    assert "`PatientChance` is declared but not VMAD-bound" in category_contract
    assert "nudge event, axis, magnitude, ordering, reset, or save/load" in (
        category_contract
    )
    assert "no substitute is fabricated" in category_contract

    assert "opening barter with the speaking actor" in dialogue_contract
    assert "waits 0.25 seconds" in dialogue_contract
    assert "0 evidence-blocked" in dialogue_contract
    assert "evidence-blocked (zero properties" not in dialogue_contract

    assert "## Timer controller contract" in sniper_contract
    assert "| 100 | 1.0 | charge |" in sniper_contract
    assert "| 200 | 2.0 | jog/happy |" in sniper_contract
    assert "| 250 | 3.0 | angry |" in sniper_contract
    assert "Timer remains a precise record dependency" not in sniper_contract

    assert "found no new movement producer, constant, or" in toy_contract
    assert "axis, magnitude," in toy_contract
    assert "found no new patient-selection or reputation" in camp_contract
    assert "interprets `PatientChance`" in camp_contract

    assert "The two unsuperseded rows still" in dependency_contract
    assert "owner stages 100/200/250 select actor-value states 1/2/3" in (
        dependency_contract
    )
    assert "opens barter after a 0.25-second dialogue-release wait" in (
        dependency_contract
    )
    assert "Timer entrypoint/duration" not in dependency_contract
    assert "100/200/250 select actor-value states 1/2/3" in inventory_contract
    assert "## Package and timer controllers repaired" in inventory_contract
    assert "It also remains intentionally unpatched" not in inventory_contract
    assert "later direct QUST/PACK trace supersedes that ruling" in (
        infrastructure_contract
    )
    assert "ItemRemovedScript` stayed evidence-blocked at this point" in (
        infrastructure_contract
    )
    assert "The later inventory trace supplied exactly that missing stage evidence" in (
        infrastructure_contract
    )
