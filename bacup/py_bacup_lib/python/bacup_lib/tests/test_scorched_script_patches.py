from __future__ import annotations

import os
from pathlib import Path

import pytest

from bacup_lib.workflows.unified import (
    _merge_script_method_patches,
    _script_patch_source,
)
from creation_lib.pex import decompile_pex
from creation_lib.pex.native_runtime import compile_psc


REPO_ROOT = Path(__file__).resolve().parents[5]
OLD_PEX_ROOT = REPO_ROOT / "mods" / "SeventySixOld" / "data" / "Scripts"

PATCH_CASES = {
    "ScorchedStatueScript": Path("ScorchedStatueScript.pex"),
    "ScorchedStatueFurnitureScript": Path("scorchedstatuefurniturescript.pex"),
    "CreatureCombatStyle": Path("CreatureCombatStyle.pex"),
    "Creatures:ScorchedStatueVariantScript": Path(
        "Creatures/ScorchedStatueVariantScript.pex"
    ),
    "Creatures:ScorchedSuiciderScript": Path(
        "Creatures/ScorchedSuiciderScript.pex"
    ),
}


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


def _merged_old_source(script_name: str, pex_relative_path: Path) -> str:
    pex_path = OLD_PEX_ROOT / pex_relative_path
    assert pex_path.is_file(), f"old Scorched PEX unavailable: {pex_path}"
    patch = _script_patch_source(script_name)
    assert patch is not None
    return _merge_script_method_patches(
        decompile_pex(pex_path, fo4_api_compat=True), patch
    )


def test_scorched_combat_style_patch_restores_weapon_aware_ai_refresh():
    patch = _script_patch_source("CreatureCombatStyle")

    assert patch is not None
    assert "GetEquippedItemType(0)" in patch
    assert "SetCombatStyle(RangedCombatStyle)" in patch
    assert "SetCombatStyle(MeleeCombatStyle)" in patch
    assert "EvaluatePackage()" in patch
    assert 'RegisterForRemoteEvent(akTarget, "OnItemEquipped")' in patch


def test_scorched_suicider_patch_restores_proximity_detonation():
    patch = _script_patch_source("Creatures:ScorchedSuiciderScript")

    assert patch is not None
    assert "RegisterForDistanceLessThanEvent(Self, akTarget, DistanceToExplode)" in patch
    assert "Event OnCombatStateChanged" in patch
    assert "Event OnDistanceLessThan" in patch
    assert "PlaceAtMe(DeathExplosion)" in patch
    assert 'GoToState("explode")' in patch


def test_scorched_statue_patches_restore_proxy_and_skin_lifecycle():
    furniture = _script_patch_source("ScorchedStatueFurnitureScript")
    variant = _script_patch_source("Creatures:ScorchedStatueVariantScript")

    assert furniture is not None
    assert "PlaceAtMe(MyScorchedStatue" in furniture
    assert 'GoToState("spawned")' in furniture
    assert "Event OnExitFurniture" in furniture
    assert "myStatueRef.Delete()" in furniture
    assert variant is not None
    assert "akTarget.EquipItem(SkinScorchedStatue" in variant
    assert "akTarget.RemoveItem(SkinScorchedStatue" in variant


def test_unportable_scorched_effects_remain_explicit_review_items():
    assert _script_patch_source("Creatures:ScorchedRaceScript") is None
    assert _script_patch_source("Creatures:Festive_LegendaryScorched") is None


@pytest.mark.parametrize(("script_name", "pex_relative_path"), PATCH_CASES.items())
def test_scorched_patch_merges_and_compiles_against_old_production_pex(
    script_name: str, pex_relative_path: Path
):
    base_source = _fo4_base_source()
    if base_source is None:
        pytest.skip("FO4 base Papyrus sources unavailable")

    source_path = pex_relative_path.with_suffix(".psc").as_posix()
    result = compile_psc(
        _merged_old_source(script_name, pex_relative_path),
        imports=[str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=source_path,
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None
