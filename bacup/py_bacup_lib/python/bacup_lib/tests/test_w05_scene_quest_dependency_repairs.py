from __future__ import annotations

import csv
from pathlib import Path

import pytest

from bacup_lib.workflows.unified import (
    _iter_top_level_papyrus_members,
    _script_patch_source,
    _script_relative_path,
)
from creation_lib.pex.native_runtime import compile_psc

from bacup_lib.tests.test_terminal_fragment_script_patches import _fo4_base_source


REPO_ROOT = Path(__file__).resolve().parents[5]
DOCS = REPO_ROOT / "bacup" / "docs" / "stub_restoration"
SOURCE_ROOT = REPO_ROOT / "mods" / "SeventySix" / "Scripts" / "Source" / "User"
CONTRACT = DOCS / "contracts" / "w05-scene-quest-dependency-repairs-2026-09-01.md"

RECORD_DEPENDENCY_SCRIPTS = (
    "Fragments:Scenes:SF_W05_RE_Scene_JP04_Settler_00563844",
    "Fragments:Packages:PF_W05_TravelToNukeLinkRef_0053820F",
)
# 57507F is assigned by no source or live record, so `Fragment_Change` has no
# runtime owner. The two still-bound carriers above keep the record-dependency state.
UNASSIGNED_PACKAGE_SCRIPTS = (
    "Fragments:Packages:PF_W05_Derek_Vault79Operatio_0057507F",
)
DEPENDENT_SCRIPTS = RECORD_DEPENDENCY_SCRIPTS + UNASSIGNED_PACKAGE_SCRIPTS
TERMINAL_STATES = dict.fromkeys(RECORD_DEPENDENCY_SCRIPTS, "record-dependency") | dict.fromkeys(
    UNASSIGNED_PACKAGE_SCRIPTS, "non-defect"
)
SOURCE_CLOSURE_CONTRACT = (
    "contracts/wastelanders-keypad-package-source-closure-2026-09-03.md"
)
RARA_TURRET_SCRIPT = "Fragments:Scenes:SF_W05_MQR_202P_RaRaVent_105_00577ECC"
BECKETT_PACKAGE_SCRIPT = "Fragments:Packages:PF_W05_Beckett_FinalAlliesLe_005A13C2"


def _status_by_script() -> dict[str, dict[str, str]]:
    with (DOCS / "status.csv").open(encoding="utf-8", newline="") as stream:
        return {row["script_name"].lower(): row for row in csv.DictReader(stream)}


def _generated_source(script_name: str) -> str:
    source_path = SOURCE_ROOT / _script_relative_path(script_name, ".psc")
    return source_path.read_text(encoding="utf-8")


@pytest.mark.parametrize("script_name", DEPENDENT_SCRIPTS)
def test_dependency_rows_remain_unpatched_declaration_shells(script_name: str) -> None:
    statuses = _status_by_script()
    contract = CONTRACT.read_text(encoding="utf-8")
    generated = _generated_source(script_name)
    members = list(_iter_top_level_papyrus_members(generated.splitlines()))

    assert statuses[script_name.lower()]["terminal_state"] == TERMINAL_STATES[script_name]
    carrier_id = script_name.rsplit("_", 1)[-1].lstrip("0").lower()
    assert carrier_id in contract.lower()
    assert _script_patch_source(script_name) is None
    assert members == []


@pytest.mark.parametrize("script_name", DEPENDENT_SCRIPTS)
def test_dependency_shells_native_compile_for_fo4(script_name: str) -> None:
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


def test_dependency_contract_accounts_for_all_rows() -> None:
    contract = CONTRACT.read_text(encoding="utf-8")

    assert len(DEPENDENT_SCRIPTS) == 3
    assert contract.count("Exact unresolved dependency") == 2
    assert "superseded by the bounded FO4 hit-event substitute" in contract
    assert "xmarker_Destination2" in contract
    assert "exact two-callback captive-release sequence remains required" in contract
    assert "authoritative proof that it is redundant is still required" in contract
    assert "four excluded `zzz_` crafting quests" in contract


def test_derek_package_is_a_shipped_topology_nondefect() -> None:
    script_name = UNASSIGNED_PACKAGE_SCRIPTS[0]
    row = _status_by_script()[script_name.lower()]
    closure = (DOCS / SOURCE_CLOSURE_CONTRACT).read_text(encoding="utf-8")

    assert row["terminal_state"] == "non-defect"
    assert row["evidence"] == SOURCE_CLOSURE_CONTRACT
    assert _script_patch_source(script_name) is None
    assert "record references `57507F`, and source/live quest alias stacks omit it" in closure
    assert "evidence-backed shipped-topology non-defect/unused content" in closure


def test_rara_turret_carrier_is_a_record_owned_nondefect() -> None:
    statuses = _status_by_script()
    row = statuses[RARA_TURRET_SCRIPT.lower()]
    generated = _generated_source(RARA_TURRET_SCRIPT)

    assert row["terminal_state"] == "non-defect"
    assert row["evidence"] == (
        "contracts/w05-scene-quest-dependency-repairs-2026-09-01.md"
    )
    assert _script_patch_source(RARA_TURRET_SCRIPT) is None
    assert list(_iter_top_level_papyrus_members(generated.splitlines())) == []


def test_beckett_package_is_closed_by_the_bounded_actor_cleanup() -> None:
    contract = CONTRACT.read_text(encoding="utf-8")
    patch = _script_patch_source(BECKETT_PACKAGE_SCRIPT)
    status = _status_by_script()[BECKETT_PACKAGE_SCRIPT.lower()]

    assert status["terminal_state"] == "patched"
    assert status["evidence"] == (
        "contracts/w05-scene-quest-dependency-repairs-2026-09-01.md"
    )
    assert patch is not None
    assert "Function Fragment_End(Actor akActor)" in patch
    assert "akActor.Disable()" in patch
    assert "SetStage(" not in patch
    assert "`Fragment_End(Actor akActor)`; owner quest `5A05DE`" in contract
    assert "null-checks and disables `akActor`" in contract
