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
SCRIPT_NAME = "Fragments:Quests:QF_EN02_MQ_Us_000293A3"
SOURCE_ROOT = REPO_ROOT / "mods" / "SeventySix" / "Scripts" / "Source" / "User"


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
    source_path = SOURCE_ROOT / _script_relative_path(SCRIPT_NAME, ".psc")
    patch = _script_patch_source(SCRIPT_NAME)
    assert source_path.is_file(), source_path
    assert patch is not None
    return _merge_script_method_patches(
        source_path.read_text(encoding="utf-8"), patch
    )


def _member_body(source: str, member_name: str) -> str:
    lines = source.splitlines()
    start, end = next(
        (start, end)
        for _kind, name, start, end in _iter_top_level_papyrus_members(lines)
        if name == member_name.lower()
    )
    return "\n".join(lines[start : end + 1])


def test_midquest_enters_gmrw_stages_without_substituting_reward_items():
    merged = _merged_source()

    # Exact GMRW delivery is owned by shared conversion. These fragments must
    # reach its attachment stages without substituting a different bound LVLI.
    stage_260 = _member_body(merged, "fragment_stage_0260_item_00")
    assert "If !IsStageDone(261)" in stage_260
    assert stage_260.count("SetStage(261)") == 1

    stage_360 = _member_body(merged, "fragment_stage_0360_item_00")
    assert "If !IsStageDone(361)" in stage_360
    assert stage_360.count("SetStage(361)") == 1
    assert "AddItem(EN02_LL_MidQuestRewardItem" not in merged


def test_completion_routes_the_next_quest_through_story_manager():
    stage_400 = _member_body(
        _merged_source(), "fragment_stage_0400_item_00"
    )

    assert "EN05_Basic != None && EN05_Basic.IsCompleted()" in stage_400
    assert "playerRef.SetValue(EN05_FinishedBasicEarly, 1.0)" in stage_400
    assert (
        stage_400.count(
            "EN05_MQ_QuestStartKeyword.SendStoryEvent(None, playerRef, playerRef)"
        )
        == 1
    )
    assert (
        stage_400.count(
            "EN05_IntroMisc_QuestStartKeyword.SendStoryEvent(None, playerRef, playerRef)"
        )
        == 1
    )
    assert "EN05_Basic.Start()" not in stage_400
    assert "EN05_QuestStartKeyword.SendStoryEvent" not in stage_400
    assert stage_400.index("CompleteQuest()") < stage_400.index("SendStoryEvent(")


def test_en02_handoff_and_reward_stage_patch_native_compiles_for_fo4():
    base_source = _fo4_base_source()
    if base_source is None:
        pytest.skip("FO4 base Papyrus sources unavailable")

    result = compile_psc(
        _merged_source(),
        imports=[str(SOURCE_ROOT), str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=str(_script_relative_path(SCRIPT_NAME, ".psc")),
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None
