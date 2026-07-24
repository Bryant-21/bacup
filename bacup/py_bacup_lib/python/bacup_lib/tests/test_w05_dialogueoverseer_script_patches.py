from __future__ import annotations

from pathlib import Path

import pytest

from bacup_lib.workflows.unified import (
    _iter_top_level_papyrus_members,
    _merge_script_method_patches,
    _script_patch_source,
)
from creation_lib.pex import decompile_pex
from creation_lib.pex.native_runtime import compile_psc

from bacup_lib.tests.test_terminal_fragment_script_patches import _fo4_base_source


REPO_ROOT = Path(__file__).resolve().parents[5]
DEPLOYED_TOPICINFO_ROOT = (
    REPO_ROOT
    / "mods"
    / "SeventySix"
    / "data"
    / "Scripts"
    / "fragments"
    / "topicinfos"
)
DEPLOYED_QUESTS_ROOT = (
    REPO_ROOT
    / "mods"
    / "SeventySix"
    / "data"
    / "Scripts"
    / "fragments"
    / "quests"
)

TOPICINFO_PATCH_CASES: dict[str, tuple[str, ...]] = {
    "TIF_W05_DialogueOverseer_00589696": (
        "W05_OverseerEpilogueSceneDone != None",
        "Game.GetPlayer().SetValue(W05_OverseerEpilogueSceneDone, 1.0)",
    ),
    "TIF_W05_DialogueOverseer_00595A2A": (
        "Actor playerRef = Game.GetPlayer()",
        "playerRef != None && pRS01A_CheckpointValue != None",
        "playerRef.SetValue(pRS01A_CheckpointValue, 1.0)",
        "playerRef != None && pRS01A_Contact_Keyword != None",
        "pRS01A_Contact_Keyword.SendStoryEvent(None, playerRef, playerRef)",
    ),
    "TIF_W05_DialogueOverseer_00595A2B": (
        "Actor playerRef = Game.GetPlayer()",
        "playerRef != None && pRS03_Inoculation_Keyword != None",
        "pRS03_Inoculation_Keyword.SendStoryEvent(None, playerRef, playerRef)",
    ),
    "TIF_W05_DialogueOverseer_00595A2C": (
        "Actor playerRef = Game.GetPlayer()",
        "playerRef != None && pTW005_StartKeyword != None",
        "pTW005_StartKeyword.SendStoryEvent(None, playerRef, playerRef)",
    ),
    "TIF_W05_DialogueOverseer_00595A2F": (
        "pW05_MQ_101P != None && !pW05_MQ_101P.IsStageDone(155)",
        "pW05_MQ_101P.SetStage(155)",
    ),
}


def _topicinfo_script_name(base_name: str) -> str:
    return f"Fragments:TopicInfos:{base_name}"


def _quest_script_name() -> str:
    return "Fragments:Quests:QF_W05_DialogueOverseer_003FB7B0"


def _merged_production_topicinfo_source(base_name: str) -> str:
    pex_path = DEPLOYED_TOPICINFO_ROOT / f"{base_name.lower()}.pex"
    if not pex_path.is_file():
        pytest.skip(f"deployed production PEX unavailable: {pex_path}")
    skeleton = decompile_pex(pex_path, fo4_api_compat=True)
    patch = _script_patch_source(_topicinfo_script_name(base_name))
    assert patch is not None
    return _merge_script_method_patches(skeleton, patch)


def _merged_production_quest_source() -> str:
    pex_path = DEPLOYED_QUESTS_ROOT / "qf_w05_dialogueoverseer_003fb7b0.pex"
    if not pex_path.is_file():
        pytest.skip(f"deployed production PEX unavailable: {pex_path}")
    skeleton = decompile_pex(pex_path, fo4_api_compat=True)
    patch = _script_patch_source(_quest_script_name())
    assert patch is not None
    return _merge_script_method_patches(skeleton, patch)


@pytest.mark.parametrize(("base_name", "expected_snippets"), TOPICINFO_PATCH_CASES.items())
def test_topicinfo_patch_restores_confirmed_fragment_call(
    base_name: str, expected_snippets: tuple[str, ...]
):
    patch = _script_patch_source(_topicinfo_script_name(base_name))

    assert patch is not None
    assert not any(
        line.strip().lower().startswith("scriptname ")
        for line in patch.splitlines()
    )
    members = {
        name
        for kind, name, _start, _end in _iter_top_level_papyrus_members(
            patch.splitlines()
        )
        if kind == "function"
    }
    assert members == {"fragment_end"}
    for snippet in expected_snippets:
        assert snippet in patch


@pytest.mark.parametrize("base_name", TOPICINFO_PATCH_CASES)
def test_topicinfo_production_merge_native_compiles_for_fo4(base_name: str):
    base_source = _fo4_base_source()
    if base_source is None:
        pytest.skip("FO4 base Papyrus sources unavailable")

    result = compile_psc(
        _merged_production_topicinfo_source(base_name),
        imports=[str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=f"Fragments/TopicInfos/{base_name}.psc",
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None


def test_quest_patch_restores_stage_fragment_member():
    patch = _script_patch_source(_quest_script_name())

    assert patch is not None
    assert not any(
        line.strip().lower().startswith("scriptname ")
        for line in patch.splitlines()
    )
    members = {
        name
        for kind, name, _start, _end in _iter_top_level_papyrus_members(
            patch.splitlines()
        )
        if kind == "function"
    }
    assert members == {"fragment_stage_0500_item_00"}
    assert "ObjectReference playerRef = Alias_Player.GetReference()" in patch
    assert "playerRef != None && AV_TalkDone != None" in patch
    assert "playerRef.SetValue(AV_TalkDone, 1.0)" in patch


def test_quest_production_merge_native_compiles_for_fo4():
    base_source = _fo4_base_source()
    if base_source is None:
        pytest.skip("FO4 base Papyrus sources unavailable")

    result = compile_psc(
        _merged_production_quest_source(),
        imports=[str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path="Fragments/Quests/QF_W05_DialogueOverseer_003FB7B0.psc",
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None


def test_dialogueoverseer_patch_count_matches_shard_runbook():
    assert len(TOPICINFO_PATCH_CASES) == 5
