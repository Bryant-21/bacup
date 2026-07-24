from __future__ import annotations

import os
from pathlib import Path

import pytest

from bacup_lib.workflows.unified import _merge_script_method_patches, _script_patch_source
from creation_lib.pex.native_runtime import compile_psc


REPO_ROOT = Path(__file__).resolve().parents[5]
SOURCE_PATH = (
    REPO_ROOT
    / "mods"
    / "SeventySix"
    / "Scripts"
    / "Source"
    / "User"
    / "Creatures"
    / "mrhandyselfdestructscript.psc"
)
SOURCE_ROOT = SOURCE_PATH.parents[2]
SCRIPT_NAME = "Creatures:MrHandySelfDestructScript"


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


def _merged_source() -> str:
    patch = _script_patch_source(SCRIPT_NAME)
    assert patch is not None
    return _merge_script_method_patches(
        SOURCE_PATH.read_text(encoding="utf-8"), patch
    )


def test_mr_handy_self_destruct_patch_preserves_hand_weapon_gate():
    patch = _script_patch_source(SCRIPT_NAME)

    assert patch is not None
    assert "Scriptname " not in patch

    merged = _merged_source()
    waiting_start = merged.index("Auto State Waiting")
    waiting_end = merged.index("EndState", waiting_start)
    waiting = merged[waiting_start:waiting_end]

    assert waiting.count("Event OnCripple(") == 1
    assert waiting.count("Event OnActivate(") == 1
    assert "targetActor.GetEquippedWeapon() == None" in waiting
    assert "targetActor.HasKeyword(LeftHandWeaponKeyword)" in waiting
    assert "targetActor.HasKeyword(RightHandWeaponKeyword)" in waiting
    assert "targetActor.HasKeyword(MiddleHandWeaponKeyword)" in waiting
    assert waiting.count('GoToState("selfdestruct")') == 2
    assert "SelfDestructActivator != None" in waiting


def test_mr_handy_self_destruct_patch_merges_and_compiles_for_fo4():
    base_source = _fo4_base_source()
    if base_source is None:
        pytest.skip("FO4 base Papyrus sources unavailable")

    result = compile_psc(
        _merged_source(),
        imports=[str(SOURCE_ROOT), str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path="Creatures/mrhandyselfdestructscript.psc",
    )
    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None
