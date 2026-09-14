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
    "Fragments:Packages:PF_RD01_Enc04_EyebotPackage_0078BEC3": (
        "RefCollectionAlias Property Generators Auto Mandatory",
    ),
    "Fragments:Scenes:SF_V94_1_0015FB0A": (
        "Keyword Property V94AccessQuestKeyword Auto Mandatory",
        "Scene Property V94Radio_1 Auto Mandatory",
    ),
    "Fragments:Scenes:SF_V94_3_0015FB12": (
        "Keyword Property V94AccessQuestKeyword Auto Mandatory",
        "Scene Property V94Radio_3 Auto Mandatory",
    ),
    "Fragments:Quests:QF_76ExitEventQuest_003ADDF9": (
        "Quest Property p76ExitEventAnnounce Auto Mandatory",
        "Keyword Property p76ExitEventAnnounce_Keyword Auto Mandatory",
    ),
    "Fragments:Quests:QF_BS02_E01_Metal_005FE4D7": (
        "MusicType Property Music_CombatMusic Auto Mandatory",
    ),
    "Fragments:Quests:QF_P02L_McCreary_FetchQuest_0041B865": (
        "Int Property SuppliesCount Auto Mandatory",
    ),
    "Fragments:Quests:QF_RSVP00_Quest_Master_0050A2EE": (
        "Keyword Property pRSVP00_Keyword_QuestActive_VectorToFlatwoods Auto Mandatory",
        "ReferenceAlias Property Alias_Container_Mailbox_BridgeStreet14_Jeremiah Auto Mandatory",
    ),
    "Fragments:Quests:QF_Storm_SE09_00783A58": (
        "ReferenceAlias Property Trigger_CallStrikes Auto Mandatory",
    ),
}
DECLARATION_CASES = tuple(
    (script_name, declaration)
    for script_name, declarations in DECLARATIONS.items()
    for declaration in declarations
)


@pytest.mark.parametrize(("script_name", "declarations"), DECLARATIONS.items())
def test_tranche5_exact_vmad_declarations_are_idempotent(
    script_name: str, declarations: tuple[str, ...]
) -> None:
    parent = "Quest"
    if ":Packages:" in script_name:
        parent = "Package"
    elif ":Scenes:" in script_name:
        parent = "SceneInstance"
    skeleton = f"Scriptname {script_name} Extends {parent} hidden\n"

    augmented = _augment_fo76_to_fo4_script_skeleton(script_name, skeleton)

    for declaration in declarations:
        assert augmented.count(declaration) == 1
    assert _augment_fo76_to_fo4_script_skeleton(script_name, augmented) == augmented


@pytest.mark.parametrize(("script_name", "declaration"), DECLARATION_CASES)
def test_tranche5_exact_vmad_declarations_reject_conflicts(
    script_name: str, declaration: str
) -> None:
    property_name = declaration.split()[2]
    skeleton = (
        f"Scriptname {script_name} Extends Quest hidden\n\n"
        f"String Property {property_name} Auto Mandatory\n"
    )

    with pytest.raises(ValueError, match="conflicting Papyrus property"):
        _augment_fo76_to_fo4_script_skeleton(script_name, skeleton)


@pytest.mark.parametrize("script_name", DECLARATIONS)
def test_tranche5_augmented_current_full_source_native_compiles(
    script_name: str, tmp_path: Path
) -> None:
    relative = _script_relative_path(script_name, ".psc")
    current_source = (SOURCE_ROOT / relative).read_text(encoding="utf-8")
    augmented = _augment_fo76_to_fo4_script_skeleton(script_name, current_source)
    for declaration in DECLARATIONS[script_name]:
        name = declaration.split()[2]
        assert len(
            re.findall(
                rf"(?im)^\s*\w+\s+Property\s+{re.escape(name)}\b", augmented
            )
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
