from __future__ import annotations

import json
from pathlib import Path

import pytest

from bacup_lib.workflows.unified import _script_patch_source


REPO_ROOT = Path(__file__).resolve().parents[5]
FIXTURE = (
    Path(__file__).resolve().parent
    / "fixtures"
    / "residual_record_dependency_topology.json"
)
CONTRACT = (
    REPO_ROOT
    / "bacup"
    / "docs"
    / "stub_restoration"
    / "contracts"
    / "residual-record-dependent-topology-2026-09-02.md"
)


def _entries() -> list[dict[str, object]]:
    payload = json.loads(FIXTURE.read_text(encoding="utf-8"))
    assert payload["schema_version"] == 1
    assert payload["terminal_state"] == "record-dependency"
    return payload["scripts"]


# The fixture records the September topology; SH-01 has since closed the keypad
# row with a reviewed adapter, so only the others must stay unpatched.
SUPERSEDED_BY_KEYPAD_ADAPTER = {"DefaultAliasSetStageOnKeypadSuccess"}


@pytest.mark.parametrize(
    "entry",
    [e for e in _entries() if e["script_name"] not in SUPERSEDED_BY_KEYPAD_ADAPTER],
    ids=lambda row: row["script_name"],
)
def test_topology_does_not_reintroduce_an_unproven_member_patch(
    entry: dict[str, object],
) -> None:
    assert _script_patch_source(str(entry["script_name"])) is None


def test_keypad_topology_row_is_closed_by_the_adapter_contract() -> None:
    patch = _script_patch_source("DefaultAliasSetStageOnKeypadSuccess")

    assert patch is not None
    assert "DefaultKeypadScript.KeypadSuccess" in patch


def test_contract_pins_each_carrier_and_prohibited_substitute() -> None:
    contract = CONTRACT.read_text(encoding="utf-8")

    for entry in _entries():
        assert f'`{entry["script_name"]}`' in contract
        assert str(entry["missing_contract"]) in contract
        assert str(entry["prohibited_substitute"]) in contract
        for carrier_id in entry["carrier_ids"]:
            assert f"`{carrier_id}`" in contract


def test_fixture_covers_only_the_three_targeted_record_dependencies() -> None:
    assert {entry["script_name"] for entry in _entries()} == {
        "DefaultAliasSetStageOnKeypadSuccess",
        "MoMParlorLaserGridManagerScript",
        "NewRiverGorgeBridgeDestructionScript",
    }
