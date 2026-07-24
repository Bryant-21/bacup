from __future__ import annotations

import os
from pathlib import Path

import pytest

from bacup_lib.workflows.unified import (
    _iter_top_level_papyrus_members,
    _merge_script_method_patches,
    _script_patch_source,
    _script_relative_path,
)
from creation_lib.pex.native_runtime import compile_psc


REPO_ROOT = Path(__file__).resolve().parents[5]
SOURCE_ROOT = REPO_ROOT / "mods" / "SeventySix" / "Scripts" / "Source" / "User"

SCRIPT_NAME = "Creatures:_Default:CreatureVariantScript"
EXPECTED_MEMBERS = {"oneffectstart"}


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


def _member_names(source: str) -> set[str]:
    return {
        name
        for _kind, name, _start, _end in _iter_top_level_papyrus_members(
            source.splitlines()
        )
    }


def _merged_source() -> str:
    source_path = SOURCE_ROOT / _script_relative_path(SCRIPT_NAME, ".psc")
    patch = _script_patch_source(SCRIPT_NAME)
    assert source_path.is_file(), source_path
    assert patch is not None
    return _merge_script_method_patches(
        source_path.read_text(encoding="utf-8"), patch
    )


def test_creature_variant_script_patch_supplies_oneffectstart():
    patch = _script_patch_source(SCRIPT_NAME)

    assert patch is not None
    assert "Scriptname " not in patch
    assert EXPECTED_MEMBERS <= _member_names(patch)

    merged = _merged_source()
    assert EXPECTED_MEMBERS <= _member_names(merged)
    assert merged.lower().count("scriptname ") == 1


def test_creature_variant_script_merged_source_has_single_oneffectstart_handler():
    merged = _merged_source()

    assert merged.lower().count("event oneffectstart(") == 1


def test_creature_variant_script_guards_race_match_before_mutating_target():
    merged = _merged_source()

    reset_index = merged.find("currentCreatureData.CreatureRace = None")
    match_index = merged.find("CreatureData[i].CreatureRace == targetRace")
    no_match_guard_index = merged.find("!currentCreatureData.CreatureRace")
    attach_index = merged.find("targetRef.AttachMod(currentCreatureData.VariantMod)")
    add_keyword_index = merged.find("targetRef.AddKeyword(VariantKeyword)")

    assert -1 not in (
        reset_index,
        match_index,
        no_match_guard_index,
        attach_index,
        add_keyword_index,
    )
    assert reset_index < match_index < no_match_guard_index < attach_index < add_keyword_index


def test_creature_variant_script_guards_required_bindings_and_repeat_effects():
    merged = _merged_source()

    required_guard_index = merged.find(
        "If !targetRef || !CreatureData || !VariantKeyword || !TransitionShaderVFX || !VariantQuest"
    )
    repeat_guard_index = merged.find("If targetRef.HasKeyword(VariantKeyword)")
    mutation_index = merged.find("targetRef.RemoveFromFaction(factionsToRemove[j])")

    assert -1 not in (required_guard_index, repeat_guard_index, mutation_index)
    assert required_guard_index < repeat_guard_index < mutation_index


def test_creature_variant_script_guards_optional_properties_before_use():
    merged = _merged_source()

    faction_guard = merged.find("If VariantFaction")
    faction_use = merged.find("targetRef.AddToFaction(VariantFaction)")
    explosion_guard = merged.find("If OnBecomeVariantExplosion")
    explosion_use = merged.find("targetRef.PlaceAtMe(OnBecomeVariantExplosion)")

    assert -1 not in (faction_guard, faction_use, explosion_guard, explosion_use)
    assert faction_guard < faction_use
    assert explosion_guard < explosion_use


def test_creature_variant_script_guards_alias_cast_before_registering():
    merged = _merged_source()

    cast_index = merged.find(
        "RefCollectionAlias nameCollection = VariantQuest.GetAlias(1) as RefCollectionAlias"
    )
    guard_index = merged.find("If nameCollection")
    add_ref_index = merged.find("nameCollection.AddRef(targetRef)")

    assert -1 not in (cast_index, guard_index, add_ref_index)
    assert cast_index < guard_index < add_ref_index


def test_creature_variant_script_patch_set_native_compiles_for_fo4():
    base_source = _fo4_base_source()
    if base_source is None:
        pytest.skip("FO4 base Papyrus sources unavailable")

    merged = _merged_source()
    result = compile_psc(
        merged,
        imports=[str(SOURCE_ROOT), str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=str(_script_relative_path(SCRIPT_NAME, ".psc")),
    )
    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None
