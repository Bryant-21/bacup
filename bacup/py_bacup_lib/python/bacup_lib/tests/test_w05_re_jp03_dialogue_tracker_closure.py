from __future__ import annotations

from pathlib import Path

import pytest

from bacup_lib.workflows.unified import (
    _iter_top_level_papyrus_members,
    _script_patch_source,
    _script_relative_path,
)
from creation_lib.pex.native_runtime import compile_psc

from bacup_lib.tests.test_terminal_fragment_script_patches import _fo4_base_source


REPO_ROOT = Path(__file__).resolve().parents[5]
SOURCE_ROOT = REPO_ROOT / "mods" / "SeventySix" / "Scripts" / "Source" / "User"
CONTRACT = (
    REPO_ROOT
    / "bacup"
    / "docs"
    / "stub_restoration"
    / "contracts"
    / "w05-re-jp03-dialogue-tracker-closure-2026-09-01.md"
)
TRACKER = "W05_RE_Scene_JP03_DialogueTracker"
TOPICINFOS = (
    "Fragments:TopicInfos:TIF_W05_RE_Scene_JP03_0055F818",
    "Fragments:TopicInfos:TIF_W05_RE_Scene_JP03_0055F83D",
)
QUEST_FRAGMENT = "Fragments:Quests:QF_RE_Scene_JP03_0055C543"


def _generated_source(script_name: str) -> str:
    path = SOURCE_ROOT / _script_relative_path(script_name, ".psc")
    assert path.is_file(), path
    return path.read_text(encoding="utf-8")


def _members(source: str) -> set[str]:
    return {
        name
        for _kind, name, _start, _end in _iter_top_level_papyrus_members(
            source.splitlines()
        )
    }


def test_tracker_is_an_unpatched_property_shell_without_callable_abi() -> None:
    source = _generated_source(TRACKER)

    assert _script_patch_source(TRACKER) is None
    assert _members(source) == set()
    assert source.count("actorvalue Property") == 2
    assert "W05_RE_JP03_AVThief" in source
    assert "W05_RE_JP03_AVSettler" in source


@pytest.mark.parametrize("script_name", TOPICINFOS)
def test_qa_topicinfos_remain_unpatched_native_transition_shells(
    script_name: str,
) -> None:
    source = _generated_source(script_name)

    assert _script_patch_source(script_name) is None
    assert _members(source) == set()
    assert " Property " not in source


def test_existing_stop_fragment_is_the_only_patched_jp03_quest_effect() -> None:
    patch = _script_patch_source(QUEST_FRAGMENT)
    assert patch is not None

    assert _members(patch) == {"fragment_stage_1000_item_00"}
    assert "ReArmTrigger()" in patch
    assert "W05_RE_JP03_AVThief" not in patch
    assert "W05_RE_JP03_AVSettler" not in patch


def test_contract_records_native_transitions_and_inactive_template_evidence() -> None:
    contract = CONTRACT.read_text(encoding="utf-8")

    for evidence in (
        "W05_RE_Scene_JP03_DoNotUse",
        "TEMPLATE - duplicate to use",
        "55F818",
        "55F83D",
        "Thief Test 1",
        "Settler Test 1",
        "StartSceneOnEnd",
        "EndRunningScene",
        "orphan_no_story_manager_node",
        "choice-to-value mapping to restore",
        "**non-defect**",
    ):
        assert evidence in contract


@pytest.mark.parametrize("script_name", (TRACKER, *TOPICINFOS))
def test_unpatched_shells_native_compile_for_fo4(script_name: str) -> None:
    base_source = _fo4_base_source()
    if base_source is None:
        pytest.skip("FO4 base Papyrus sources unavailable")

    result = compile_psc(
        _generated_source(script_name),
        imports=[str(SOURCE_ROOT), str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=str(_script_relative_path(script_name, ".psc")),
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, f"{script_name}:\n{diagnostics}"
    assert result.pex_bytes is not None
