from __future__ import annotations

import re
from pathlib import Path

import pytest

from bacup_lib.tests.test_terminal_fragment_script_patches import _fo4_base_source
from bacup_lib.workflows.unified import (
    _augment_fo76_to_fo4_script_skeleton,
    _script_relative_path,
)
from creation_lib.pex.native_runtime import compile_psc


REPO_ROOT = Path(__file__).resolve().parents[5]
SOURCE_ROOT = REPO_ROOT / "mods" / "SeventySix" / "Scripts" / "Source" / "User"
DECLARATIONS = {
    "Fragments:Terminals:TERM_MTR08_ClaimTokenExchang_0004C6E5": (
        "MTR08_ClaimToken",
        "MiscObject Property MTR08_ClaimToken Auto Mandatory",
    ),
    "Fragments:Terminals:TERM_Arcade_PrizeTerminal_Ti_0065CEC5": (
        "PointsAV",
        "ActorValue Property PointsAV Auto Mandatory",
    ),
    "Fragments:Terminals:TERM_Arcade_PrizeTerminal_Ti_0065CEC6": (
        "PointsAV",
        "ActorValue Property PointsAV Auto Mandatory",
    ),
    "Fragments:Terminals:TERM_Arcade_PrizeTerminal_Ti_0065CEC7": (
        "PointsAV",
        "ActorValue Property PointsAV Auto Mandatory",
    ),
    "Fragments:Terminals:TERM_Arcade_PrizeTerminal_Ti_0065CEC8": (
        "PointsAV",
        "ActorValue Property PointsAV Auto Mandatory",
    ),
    "Fragments:Terminals:TERM_Arcade_PrizeTerminal__0065CEC9_1": (
        "PointsAV",
        "ActorValue Property PointsAV Auto Mandatory",
    ),
    "Fragments:Terminals:TERM_EN06_PresidentialVendor_0052BDE0": (
        "EN06_PresidentialSeal",
        "MiscObject Property EN06_PresidentialSeal Auto Mandatory",
    ),
}


@pytest.mark.parametrize(("script_name", "property_data"), DECLARATIONS.items())
def test_tranche7_sibling_vmad_declaration_is_idempotent(
    script_name: str, property_data: tuple[str, str]
) -> None:
    property_name, declaration = property_data
    relative = _script_relative_path(script_name, ".psc")
    current = (SOURCE_ROOT / relative).read_text(encoding="utf-8")
    assert not re.search(
        rf"(?im)^\s*\w+\s+Property\s+{re.escape(property_name)}\b", current
    )

    augmented = _augment_fo76_to_fo4_script_skeleton(script_name, current)

    assert augmented.count(declaration) == 1
    assert _augment_fo76_to_fo4_script_skeleton(script_name, augmented) == augmented


@pytest.mark.parametrize(("script_name", "property_data"), DECLARATIONS.items())
def test_tranche7_sibling_vmad_declaration_rejects_conflict(
    script_name: str, property_data: tuple[str, str]
) -> None:
    property_name, _declaration = property_data
    skeleton = (
        f"Scriptname {script_name} Extends Terminal hidden\n\n"
        f"Quest Property {property_name} Auto Mandatory\n"
    )

    with pytest.raises(ValueError, match="conflicting Papyrus property"):
        _augment_fo76_to_fo4_script_skeleton(script_name, skeleton)


@pytest.mark.parametrize("script_name", DECLARATIONS)
def test_tranche7_augmented_full_source_native_compiles(script_name: str) -> None:
    base = _fo4_base_source()
    if base is None:
        pytest.skip("FO4 base Papyrus sources unavailable")
    relative = _script_relative_path(script_name, ".psc")
    current = (SOURCE_ROOT / relative).read_text(encoding="utf-8")
    augmented = _augment_fo76_to_fo4_script_skeleton(script_name, current)

    result = compile_psc(
        augmented,
        imports=[str(SOURCE_ROOT), str(base)],
        game="fo4",
        flags=str(base / "Institute_Papyrus_Flags.flg"),
        source_path=str(relative),
    )
    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None
