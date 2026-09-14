from __future__ import annotations

from pathlib import Path

import pytest

from bacup_lib.workflows.unified import _script_patch_source


REPO_ROOT = Path(__file__).resolve().parents[5]
CONTRACT = (
    REPO_ROOT
    / "bacup"
    / "docs"
    / "stub_restoration"
    / "contracts"
    / "residual-local-dependency-repairs-2026-09-01.md"
)


def _code_lines(patch: str) -> str:
    """Patch text with comment lines dropped, so prose cannot fail a guard."""
    return "\n".join(
        line for line in patch.splitlines() if not line.lstrip().startswith(";")
    )


TARGETS = (
    "MoMParlorLaserGridManagerScript",
    "NewRiverGorgeBridgeDestructionScript",
    "Fragments:TopicInfos:TIF_W05_DialogueRadicals_Ext_005895B2",
    "DefaultAliasSetStageOnKeypadSuccess",
    "DefaultInstanceCellQuestSupportScript",
)
RECORD_DEPENDENCIES = (
    "MoMParlorLaserGridManagerScript",
    "NewRiverGorgeBridgeDestructionScript",
    "DefaultAliasSetStageOnKeypadSuccess",
)


# The FO4 keypad adapter supplies an audited patch for this row; every other
# target must still have no durable patch.
SUPERSEDED_BY_KEYPAD_ADAPTER = ("DefaultAliasSetStageOnKeypadSuccess",)


@pytest.mark.parametrize(
    "script_name",
    [name for name in TARGETS if name not in SUPERSEDED_BY_KEYPAD_ADAPTER],
)
def test_residual_local_dependencies_have_no_unaudited_durable_patch(
    script_name: str,
) -> None:
    assert _script_patch_source(script_name) is None


@pytest.mark.parametrize("script_name", SUPERSEDED_BY_KEYPAD_ADAPTER)
def test_keypad_dependency_is_closed_by_the_adapter_contract(
    script_name: str,
) -> None:
    patch = _script_patch_source(script_name)

    assert patch is not None
    assert "DefaultKeypadScript.KeypadSuccess" in patch
    assert "OnActivate" not in _code_lines(patch)


def test_residual_local_dependency_contract_accounts_for_every_target() -> None:
    source = CONTRACT.read_text(encoding="utf-8")

    for script_name in TARGETS:
        assert f"`{script_name}`" in source

    assert source.count("**Recommended status:** `record-dependency`.") == len(
        RECORD_DEPENDENCIES
    )
    assert "**Final status:** `unsupported-online`" in source
    assert "Removed obsolete patch" in source
    assert "Menu close can also follow cancellation or failed entry" in source
    assert "astronaut-seeker-local-wave-2026-09-01.md" in source


def test_mom_laser_grid_manager_does_not_call_the_inert_grid_request_path() -> None:
    contract = CONTRACT.read_text(encoding="utf-8")
    laser_grid_patch = _script_patch_source("LaserGridScript")

    assert _script_patch_source("MoMParlorLaserGridManagerScript") is None
    assert laser_grid_patch is not None
    assert "Function RequestReevaluateConditions()" in laser_grid_patch
    assert "Function ReevaluateConditions(" not in laser_grid_patch
    assert "RiversideManorLaserGrids" in contract
    assert "MoMLaserGridRefType" in contract
    assert "Exact future direct-reader request" in contract
    assert "compile-clean no-op" in contract
