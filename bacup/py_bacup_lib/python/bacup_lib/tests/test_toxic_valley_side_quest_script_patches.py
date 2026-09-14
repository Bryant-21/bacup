from __future__ import annotations

from pathlib import Path

import pytest

from bacup_lib.tests.test_terminal_fragment_script_patches import _fo4_base_source
from bacup_lib.workflows.unified import (
    _iter_top_level_papyrus_members,
    _merge_script_method_patches,
    _script_patch_source,
    _script_relative_path,
)
from creation_lib.pex.native_runtime import compile_psc


REPO_ROOT = Path(__file__).resolve().parents[5]
SOURCE_ROOT = REPO_ROOT / "mods" / "SeventySix" / "Scripts" / "Source" / "User"

REGISTERED_MEMBERS = {
    "SFM04_Organic_PlayerAliasScript": {
        "onaliasinit",
        "onplayerloadgame",
        "onitemadded",
        "reconcileradshield",
    },
    "Fragments:Quests:QF_SFM04_Organic_0010AE02": {
        "tryadvancetochemicaldeposit",
        "fragment_stage_0000_item_00",
        "fragment_stage_0001_item_00",
        "fragment_stage_0002_item_00",
        "fragment_stage_0003_item_00",
        "fragment_stage_0010_item_00",
        "fragment_stage_0050_item_00",
        "fragment_stage_0100_item_00",
        "fragment_stage_0200_item_00",
        "fragment_stage_0300_item_00",
        "fragment_stage_0310_item_00",
        "fragment_stage_0315_item_00",
        "fragment_stage_0320_item_00",
        "fragment_stage_0325_item_00",
        "fragment_stage_0330_item_00",
        "fragment_stage_0340_item_00",
        "fragment_stage_0350_item_00",
        "fragment_stage_0360_item_00",
        "fragment_stage_0370_item_00",
        "fragment_stage_0380_item_00",
        "fragment_stage_0400_item_00",
        "fragment_stage_0410_item_00",
        "fragment_stage_0420_item_00",
        "fragment_stage_0430_item_00",
        "fragment_stage_0435_item_00",
        "fragment_stage_0440_item_00",
        "fragment_stage_0450_item_00",
        "fragment_stage_0460_item_00",
        "fragment_stage_0500_item_00",
        "fragment_stage_0600_item_00",
        "fragment_stage_0700_item_00",
        "fragment_stage_1000_item_00",
    },
    "Fragments:Terminals:TERM_SFM04_Organic_EllasHo_002D4006_2": {
        "fragment_terminal_05"
    },
    "Fragments:Quests:QF_TW005_0007A2D8": {
        "fragment_stage_0050_item_00",
        "fragment_stage_0100_item_00",
        "fragment_stage_0200_item_00",
        "fragment_stage_0300_item_00",
        "fragment_stage_0301_item_00",
        "fragment_stage_0400_item_00",
        "fragment_stage_0401_item_00",
        "fragment_stage_0500_item_00",
        "fragment_stage_0501_item_00",
        "fragment_stage_0600_item_00",
        "fragment_stage_0601_item_00",
        "fragment_stage_1000_item_00",
    },
    "Fragments:Quests:QF_TW007_ColdCase_002C460C": {
        "fragment_stage_0001_item_00",
        "fragment_stage_0099_item_00",
        "fragment_stage_0100_item_00",
        "fragment_stage_0200_item_00",
        "fragment_stage_0225_item_00",
        "fragment_stage_0250_item_00",
        "fragment_stage_0300_item_00",
        "fragment_stage_0350_item_00",
        "fragment_stage_0375_item_00",
        "fragment_stage_0380_item_00",
        "fragment_stage_0400_item_00",
        "fragment_stage_0500_item_00",
        "fragment_stage_0550_item_00",
        "fragment_stage_0600_item_00",
        "fragment_stage_0700_item_00",
        "fragment_stage_0800_item_00",
        "fragment_stage_1000_item_00",
        "fragment_stage_1100_item_00",
        "fragment_stage_1200_item_00",
        "fragment_stage_1300_item_00",
        "fragment_stage_1400_item_00",
        "fragment_stage_1600_item_00",
        "fragment_stage_1700_item_00",
        "fragment_stage_2000_item_00",
    },
}

NO_OP_MEMBERS = {
    "Fragments:Quests:QF_SFM04_Organic_0010AE02": {
        "fragment_stage_0000_item_00",
        "fragment_stage_0001_item_00",
        "fragment_stage_0002_item_00",
        "fragment_stage_0003_item_00",
        "fragment_stage_0010_item_00",
        "fragment_stage_0330_item_00",
    },
    "Fragments:Quests:QF_TW005_0007A2D8": {
        "fragment_stage_0301_item_00",
        "fragment_stage_0401_item_00",
        "fragment_stage_0501_item_00",
        "fragment_stage_0601_item_00",
    },
    "Fragments:Quests:QF_TW007_ColdCase_002C460C": {
        "fragment_stage_0380_item_00",
    },
}

PATCH_MEMBERS = {
    script_name: registered - NO_OP_MEMBERS.get(script_name, set())
    for script_name, registered in REGISTERED_MEMBERS.items()
}


@pytest.mark.parametrize(("script_name", "expected_members"), PATCH_MEMBERS.items())
def test_toxic_valley_side_quest_patches_merge_once_and_compile(
    script_name: str, expected_members: set[str]
):
    no_op_members = NO_OP_MEMBERS.get(script_name, set())
    assert expected_members.isdisjoint(no_op_members)
    assert expected_members | no_op_members == REGISTERED_MEMBERS[script_name]

    source_path = SOURCE_ROOT / _script_relative_path(script_name, ".psc")
    skeleton = source_path.read_text(encoding="utf-8")
    patch = _script_patch_source(script_name)

    assert patch is not None
    assert "Scriptname" not in patch
    assert "Property" not in patch

    merged = _merge_script_method_patches(skeleton, patch)
    members = [
        name
        for _kind, name, _start, _end in _iter_top_level_papyrus_members(
            merged.splitlines()
        )
    ]
    for member in expected_members:
        assert members.count(member) == 1
    assert set(members) == expected_members
    assert _merge_script_method_patches(merged, patch) == merged

    base_source = _fo4_base_source()
    assert base_source is not None, "FO4 base Papyrus sources unavailable"
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


def test_toxic_valley_patches_preserve_the_local_progression_contract():
    sfm04_patch = _script_patch_source("Fragments:Quests:QF_SFM04_Organic_0010AE02")
    terminal_patch = _script_patch_source(
        "Fragments:Terminals:TERM_SFM04_Organic_EllasHo_002D4006_2"
    )
    tw005_patch = _script_patch_source("Fragments:Quests:QF_TW005_0007A2D8")
    tw007_patch = _script_patch_source("Fragments:Quests:QF_TW007_ColdCase_002C460C")

    assert sfm04_patch is not None
    assert terminal_patch is not None
    assert tw005_patch is not None
    assert tw007_patch is not None
    assert "SetObjectiveDisplayed(350, True)" in sfm04_patch
    assert "SFM04_Organic.SetStage(300)" in terminal_patch
    assert "TW005MayorIntro.Start()" in tw005_patch
    assert "SetStage(1000)" in tw005_patch
    assert "SetObjectiveDisplayed(26, True)" in tw007_patch
    assert "IsStageDone(250)" in tw007_patch
    assert "IsStageDone(375)" in tw007_patch
    assert "CompleteQuest()" not in tw007_patch


def test_tw005_uses_the_exact_info_story_event_carrier():
    patch = _script_patch_source(
        "Fragments:TopicInfos:TIF_W05_DialogueOverseer_00595A2C"
    )

    assert patch is not None
    assert "pTW005_StartKeyword.SendStoryEvent(None, playerRef, playerRef)" in patch
    assert ".Start()" not in patch


def test_tw005_sets_lifecycle_status_and_enters_each_midpoint_reward_stage():
    patch = _script_patch_source("Fragments:Quests:QF_TW005_0007A2D8")

    assert patch is not None
    assert "playerRef.SetValue(TW005status, 1.0)" in patch
    assert "playerRef.SetValue(TW005status, 2.0)" in patch
    for stage in (301, 401, 501, 601):
        assert f"If !IsStageDone({stage})" in patch
        assert f"SetStage({stage})" in patch
    assert patch.count("TW005status != None") == 2
    assert "TW002.Start()" not in patch
