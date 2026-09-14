from __future__ import annotations

import json
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
FIXTURE_ROOT = Path(__file__).with_name("fixtures")

# Deterministic one-shot guarded handlers (no named states/timers) — a full
# compile of the merged patch is sufficient coverage; see repair-papyrus-stubs
# SKILL.md's dedicated-test-file criteria.
PATCH_CASES = {
    "COMP_AllyPermanentSpawnMarkerScript": {"ontriggerenter"},
    "COMP_AllySpawnMarkerScript": {"oncellattach"},
    "CompanionLiteAllyEffectScript": {"oneffectstart"},
    "W05_Comp_Lite_AllySpawnScript": {"onunload"},
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


def _member_names(source: str) -> set[str]:
    return {
        name
        for _kind, name, _start, _end in _iter_top_level_papyrus_members(
            source.splitlines()
        )
    }


def _merged_source(script_name: str) -> str:
    source_path = SOURCE_ROOT / _script_relative_path(script_name, ".psc")
    patch = _script_patch_source(script_name)
    assert source_path.is_file(), source_path
    assert patch is not None
    return _merge_script_method_patches(source_path.read_text(encoding="utf-8"), patch)


@pytest.mark.parametrize(("script_name", "expected_members"), PATCH_CASES.items())
def test_companions_allies_misc_patch_supplies_expected_members_and_merges_cleanly(
    script_name: str, expected_members: set[str]
):
    patch = _script_patch_source(script_name)

    assert patch is not None
    assert not any(
        line.strip().lower().startswith("scriptname ") for line in patch.splitlines()
    )
    assert expected_members <= _member_names(patch)

    merged = _merged_source(script_name)
    assert expected_members <= _member_names(merged)
    assert merged.lower().count("scriptname ") == 1
    (expected_member,) = expected_members
    assert merged.lower().count(f"event {expected_member}(") == 1
    assert _merge_script_method_patches(merged, patch) == merged


@pytest.mark.parametrize("script_name", PATCH_CASES)
def test_companions_allies_misc_patch_native_compiles_for_fo4(script_name: str):
    base_source = _fo4_base_source()
    if base_source is None:
        pytest.skip("FO4 base Papyrus sources unavailable")

    merged = _merged_source(script_name)
    result = compile_psc(
        merged,
        imports=[str(SOURCE_ROOT), str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=str(_script_relative_path(script_name, ".psc")),
    )
    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None


def test_lite_ally_effect_sends_the_bound_ally_and_location_through_story_manager():
    merged = _merged_source("CompanionLiteAllyEffectScript")

    assert "Event OnEffectStart(Actor akTarget, Actor akCaster)" in merged
    assert "!InitialQuest.IsRunning() && !InitialQuest.IsCompleted()" in merged
    assert (
        "InitialQuestStartKeyword.SendStoryEventAndWait(akTarget.GetCurrentLocation(), akTarget)"
        in merged
    )
    assert "InitialQuest.Start()" not in merged


def test_raider_punk_permanent_copy_disables_only_after_recruitment_unload():
    merged = _merged_source("W05_Comp_Lite_AllySpawnScript")

    assert "Event OnUnload()" in merged
    assert "playerRef.GetValue(CampObjectAvailable) > 0.0" in merged
    assert "Disable()" in merged
    assert "Enable()" not in merged
    assert "OnLoad" not in merged


def test_lite_ally_source_live_carriers_freeze_the_record_loss_boundary():
    fixture = json.loads(
        (FIXTURE_ROOT / "w05_lite_ally_spawn_source_live.json").read_text(
            encoding="utf-8"
        )
    )

    assert fixture["spawn_carrier"]["source"] == fixture["spawn_carrier"]["live"]
    assert (
        fixture["availability_actor_value"]["source"]
        == fixture["availability_actor_value"]["live"]
    )
    assert fixture["availability_actor_value"]["source"]["default"] == 0.0
    assert fixture["initial_effect"]["source"] == fixture["initial_effect"]["live"]
    assert fixture["initial_effect"]["source"]["casting"] == "FireAndForget"

    source_quest = fixture["initial_quest"]["source"]
    live_quest = fixture["initial_quest"]["live"]
    assert source_quest["event"] == "SCPT"
    assert live_quest["event"] is None
    assert source_quest["quest_start_data"] is True
    assert live_quest["quest_start_data"] is False
    assert source_quest["event_ally"] == "Reference1"
    assert live_quest["event_ally"] is None

    source_node = fixture["shared_story_node"]["source"]
    assert fixture["shared_story_node"]["live"] is None
    assert source_node["shares_event"] is True
    assert source_node["max_concurrent_quests"] == 0
    assert source_node["quest_count"] == 5
    assert len(source_node["start_keywords"]) == len(source_node["quests"]) == 5
