from __future__ import annotations

import os
from pathlib import Path

import pytest

from bacup_lib.workflows.unified import (
    _augment_fo76_to_fo4_script_skeleton,
    _iter_top_level_papyrus_members,
    _merge_script_method_patches,
    _script_patch_source,
    _script_relative_path,
)
from creation_lib.pex.native_runtime import compile_psc


REPO_ROOT = Path(__file__).resolve().parents[5]
SOURCE_ROOT = REPO_ROOT / "mods" / "SeventySix" / "Scripts" / "Source" / "User"
SCRIPT_NAME = "Fragments:Quests:QF_COMP_Quest_Full_Beckett_I_00574625"
CONTRACT = (
    REPO_ROOT
    / "bacup"
    / "docs"
    / "stub_restoration"
    / "contracts"
    / "beckett-first-half-gap-repair-2026-09-03.md"
)
MATERIALIZER_FIXUP = (
    REPO_ROOT
    / "bacup"
    / "py_bacup_lib"
    / "native"
    / "conversion"
    / "src"
    / "fixups"
    / "materialize_fo76_local_encounter_waves.rs"
)


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


def _merged() -> str:
    patch = _script_patch_source(SCRIPT_NAME)
    assert patch is not None
    source_path = SOURCE_ROOT / _script_relative_path(SCRIPT_NAME, ".psc")
    skeleton = _augment_fo76_to_fo4_script_skeleton(
        SCRIPT_NAME, source_path.read_text(encoding="utf-8")
    )
    return _merge_script_method_patches(skeleton, patch)


def _member_names(source: str) -> list[str]:
    return [
        name
        for _kind, name, _start, _end in _iter_top_level_papyrus_members(
            source.splitlines()
        )
    ]


def _body(source: str, stage: int) -> str:
    marker = f"Function Fragment_Stage_{stage:04d}_Item_00()"
    return source.split(marker, 1)[1].split("EndFunction", 1)[0]


def test_narrow_escape_patch_merges_once_and_idempotently() -> None:
    patch = _script_patch_source(SCRIPT_NAME)
    assert patch is not None
    assert "scriptname " not in patch.lower()

    merged = _merged()
    assert merged.lower().count("scriptname ") == 1
    for member in _member_names(patch):
        assert _member_names(merged).count(member) == 1
    assert _merge_script_method_patches(merged, patch) == merged


def test_narrow_escape_local_stage_path_is_reachable() -> None:
    merged = _merged()
    expected_edges = {
        50: 100,
        100: 200,
        490: 500,
        590: 595,
        595: 600,
        690: 700,
        790: 1000,
        1090: 2000,
        2090: 9000,
    }
    for source_stage, target_stage in expected_edges.items():
        assert f"SetStage({target_stage})" in _body(merged, source_stage)

    for objective in (200, 400, 500, 600, 700, 1000, 2000):
        assert f"SetObjectiveDisplayed({objective})" in merged
    for objective in (200, 400, 500, 600, 700, 1000, 2000):
        assert f"SetObjectiveCompleted({objective})" in merged

    assert "materializer.PrepareEligibleWaves()" in _body(merged, 595)
    assert "encounterWaves.StartLocalEncounterWave(0)" in _body(merged, 595)
    assert "beckett.EvaluatePackage()" in _body(merged, 690)
    assert "Stop()" in _body(merged, 9000)
    assert "Stop()" in _body(merged, 9990)


def test_narrow_escape_wave_has_a_production_materializer_contract() -> None:
    production = MATERIALIZER_FIXUP.read_text(encoding="utf-8").split(
        "#[cfg(test)]", 1
    )[0]
    quest_specs = production.split("const QUEST_SPECS", 1)[1]
    wave_spec = production.split(
        "const BECKETT_NARROW_ESCAPE_WAVES", 1
    )[1].split("];", 1)[0]

    assert "source_quest: 0x574625" in quest_specs
    assert 'source_quest_eid: "COMP_Quest_Intro_Full_Beckett"' in quest_specs
    assert "waves: BECKETT_NARROW_ESCAPE_WAVES" in quest_specs
    assert "source_wave: 0x55DE43" in wave_spec
    assert 'source_wave_eid: "WaveTypeBloodEagle"' in wave_spec
    assert "collection_alias: 24" in wave_spec
    assert "spawn_marker_alias: 27" in wave_spec
    assert "preparation_stage: 595" in wave_spec
    assert "actor_count: 8" in wave_spec
    assert "spawn_slot_count: 4" in wave_spec


def test_key_setup_is_idempotent_and_camp_completion_is_not_fabricated() -> None:
    stage_500 = _body(_merged(), 500)
    stage_1090 = _body(_merged(), 1090)
    contract = CONTRACT.read_text(encoding="utf-8")

    assert "player.GetItemCount(Key_Jail) <= 0" in stage_500
    assert "bossChest.GetItemCount(Key_Jail) <= 0" in stage_500
    assert "Alias_JailKeys.ForceRefTo(jailKey)" in stage_500
    assert "bossChest.AddItem(jailKey, 1, True)" in stage_500
    assert "SetStage(2090)" not in stage_1090
    assert "CampObjectRecipe" not in _script_patch_source(SCRIPT_NAME)
    assert "final `2000 -> 2090` transition is record-dependent" in contract


def test_beckett_title_rows_are_mapped_to_real_wrappers() -> None:
    contract = CONTRACT.read_text(encoding="utf-8")
    expected = {
        "0059671E": "0058215B",
        "0059671C": "00582164",
        "0059671A": "00582163",
        "00596715": "00582160",
        "00596714": "00582167",
    }
    for title_form_id, wrapper_form_id in expected.items():
        assert title_form_id in contract
        assert wrapper_form_id in contract
    assert contract.count("| `005967") == 5
    assert "none is an independent quest" in contract


def test_umbrella_and_return_reward_carriers_remain_evidence_blocked() -> None:
    contract = CONTRACT.read_text(encoding="utf-8")

    assert "`00582162` is one real `QUST`" in contract
    assert "receives no speculative fragment" in contract
    assert "COMP_PlayerReturnToQuestGiverInfo` has no live INFO VMAD binding" in contract
    assert _script_patch_source(
        "Fragments:Quests:QF_COMP_Quest_Camp_Full_Beck_00582162"
    ) is None


def test_narrow_escape_full_merged_source_compiles_for_fo4(tmp_path: Path) -> None:
    base_source = _fo4_base_source()
    if base_source is None:
        pytest.skip("FO4 base Papyrus sources unavailable")

    result = compile_psc(
        _merged(),
        imports=[str(SOURCE_ROOT), str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=str(_script_relative_path(SCRIPT_NAME, ".psc")),
    )
    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None
