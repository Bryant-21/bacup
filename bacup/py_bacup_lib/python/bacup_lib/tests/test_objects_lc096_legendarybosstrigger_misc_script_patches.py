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

SCRIPT_NAME = "Objects:LC096_LegendaryBossTrigger"
EXPECTED_MEMBERS = {"oninit", "ontriggerenter"}
KEYWORD_PROPERTIES = ("FilterKeyword", "Sandbox", "Hold", "HoldPreferred", "HoldEngaged")


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


def test_lc096_legendary_boss_trigger_patch_supplies_expected_members():
    patch = _script_patch_source(SCRIPT_NAME)

    assert patch is not None
    assert "Scriptname " not in patch
    assert EXPECTED_MEMBERS <= _member_names(patch)

    merged = _merged_source()
    assert EXPECTED_MEMBERS <= _member_names(merged)
    assert merged.lower().count("scriptname ") == 1


def test_lc096_legendary_boss_trigger_merged_source_has_single_handlers():
    merged = _merged_source()

    assert merged.lower().count("event oninit(") == 1
    assert merged.lower().count("event ontriggerenter(") == 1


def test_lc096_legendary_boss_trigger_oninit_caches_linked_refs():
    merged = _merged_source()

    assert "BossSpawnMarker = GetLinkedRef(LinkCustom01)" in merged
    assert "CombatVolume = GetLinkedRef(LinkCustom02)" in merged


def test_lc096_legendary_boss_trigger_idempotency_guard_precedes_spawn():
    merged = _merged_source()

    player_guard_index = merged.find("If akActionRef != Game.GetPlayer()")
    guard_index = merged.find("aBoss != None && !aBoss.IsDead()")
    spawn_index = merged.find("BossSpawnMarker.PlaceActorAtMe(ScorchedBoss, 3)")

    assert player_guard_index != -1
    assert guard_index != -1
    assert spawn_index != -1
    assert player_guard_index < guard_index
    assert guard_index < spawn_index


def test_lc096_legendary_boss_trigger_none_guards_precede_spawn():
    merged = _merged_source()

    none_guard_index = merged.find("BossSpawnMarker == None || ScorchedBoss == None")
    spawn_index = merged.find("BossSpawnMarker.PlaceActorAtMe(ScorchedBoss, 3)")
    post_spawn_guard_index = merged.find("aBoss == None", spawn_index)

    assert none_guard_index != -1
    assert none_guard_index < spawn_index
    assert post_spawn_guard_index != -1


def test_lc096_legendary_boss_trigger_keyword_guards_present():
    merged = _merged_source()

    for keyword in KEYWORD_PROPERTIES:
        guard_index = merged.find(f"If {keyword} != None")
        add_index = merged.find(f"aBoss.AddKeyword({keyword})")
        assert guard_index != -1, keyword
        assert add_index != -1, keyword
        assert guard_index < add_index, keyword


def test_lc096_legendary_boss_trigger_setvalue_follows_spawn():
    """LC096_FirstEntry must be set only after the spawn call, per the approved contract's
    explicit ordering requirement (coordinator condition 2)."""
    merged = _merged_source()

    spawn_index = merged.find("BossSpawnMarker.PlaceActorAtMe(ScorchedBoss, 3)")
    set_value_index = merged.find("SetValue(LC096_FirstEntry, 1)")

    assert spawn_index != -1
    assert set_value_index != -1
    assert spawn_index < set_value_index


def test_lc096_legendary_boss_trigger_applies_combat_link_before_visit_flag():
    merged = _merged_source()

    link_guard_index = merged.find("If CombatVolume != None")
    link_index = merged.find("aBoss.SetLinkedRef(CombatVolume)")
    set_value_index = merged.find("SetValue(LC096_FirstEntry, 1)")

    assert link_guard_index != -1
    assert link_index != -1
    assert set_value_index != -1
    assert link_guard_index < link_index < set_value_index


def test_lc096_legendary_boss_trigger_keeps_unsupported_epic_rank_unconsumed():
    patch = _script_patch_source(SCRIPT_NAME)

    assert patch is not None
    assert "EpicRankAV" not in patch


def test_lc096_legendary_boss_trigger_patch_set_native_compiles_for_fo4():
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
