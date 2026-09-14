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
    "Fragments:Terminals:TERM_CB02_Vending_Terminal_T_0051AA03": (
        "LeveledItem Property LGND_PossibleLegendaryItemBaseLists Auto Mandatory",
    ),
    "Fragments:Terminals:TERM_nativeRobotTerminalSubM_002C506F": (
        "Keyword Property LinkTerminalProtectron Auto Mandatory",
        "ActorValue Property MiscStatRobotHasBeenDisabled Auto Mandatory",
    ),
    "Fragments:Terminals:TERM_SFL02_Track_VertibotTer_00184A03": (
        "GlobalVariable Property LCP_BoSZ01 Auto Mandatory",
    ),
    "Fragments:Terminals:TERM_V94_Access_Terminal_002FB273": (
        "Holotape Property V94_AccessCodeHolotape Auto Mandatory",
    ),
    "Fragments:TopicInfos:TIF_V94_3_Personal_003EE27B": (
        "ActorValue Property V94_3_Pump_RobotHasPlayedSpawnLineValue Auto Mandatory",
    ),
    "Fragments:TopicInfos:TIF_V94_3_Personal_003EE27E": (
        "ActorValue Property V94_3_Pump_RobotHasPlayedSpawnLineValue Auto Mandatory",
    ),
}
DECLARATION_CASES = tuple(
    (script_name, declaration)
    for script_name, declarations in DECLARATIONS.items()
    for declaration in declarations
)


@pytest.mark.parametrize(("script_name", "declarations"), DECLARATIONS.items())
def test_tranche4_exact_vmad_declarations_are_idempotent(
    script_name: str, declarations: tuple[str, ...]
) -> None:
    skeleton = f"Scriptname {script_name} Extends Terminal hidden\n"

    augmented = _augment_fo76_to_fo4_script_skeleton(script_name, skeleton)

    for declaration in declarations:
        assert augmented.count(declaration) == 1
    assert _augment_fo76_to_fo4_script_skeleton(script_name, augmented) == augmented


@pytest.mark.parametrize(("script_name", "declaration"), DECLARATION_CASES)
def test_tranche4_exact_vmad_declarations_reject_conflicts(
    script_name: str, declaration: str
) -> None:
    property_name = declaration.split()[2]
    skeleton = (
        f"Scriptname {script_name} Extends Terminal hidden\n\n"
        f"Int Property {property_name} Auto Mandatory\n"
    )

    with pytest.raises(ValueError, match="conflicting Papyrus property"):
        _augment_fo76_to_fo4_script_skeleton(script_name, skeleton)


@pytest.mark.parametrize("script_name", DECLARATIONS)
def test_tranche4_augmented_current_full_source_native_compiles(
    script_name: str, tmp_path: Path
) -> None:
    relative = _script_relative_path(script_name, ".psc")
    current_source = (SOURCE_ROOT / relative).read_text(encoding="utf-8")
    augmented = _augment_fo76_to_fo4_script_skeleton(script_name, current_source)
    property_names = [
        declaration.split()[2] for declaration in DECLARATIONS[script_name]
    ]
    current_has_all = all(
        re.search(rf"(?im)^\s*\w+\s+Property\s+{re.escape(name)}\b", current_source)
        for name in property_names
    )
    if current_has_all:
        assert augmented == current_source
    for name in property_names:
        assert len(
            re.findall(rf"(?im)^\s*\w+\s+Property\s+{re.escape(name)}\b", augmented)
        ) == 1
    source_root = tmp_path / "Scripts" / "Source" / "User"
    output = source_root / relative
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(augmented, encoding="utf-8")
    base = _fo4_base_source()
    assert base is not None

    result = compile_psc(
        augmented,
        imports=[str(source_root), str(SOURCE_ROOT), str(base)],
        game="fo4",
        flags=str(base / "Institute_Papyrus_Flags.flg"),
        source_path=str(relative),
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, f"{script_name}\n{diagnostics}"
    assert result.pex_bytes is not None
