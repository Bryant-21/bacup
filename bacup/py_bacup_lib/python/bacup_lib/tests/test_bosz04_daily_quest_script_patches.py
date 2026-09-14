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
QUEST_FRAGMENT = "Fragments:Quests:QF_BoSZ04_00065DFE"
PERK_FRAGMENT = "Fragments:Perks:PRKF_BoSZ04HarvestSBDNAPerk_0015A032"
GRANT_TERMINAL = "Fragments:Terminals:TERM_BoS_GrantTerminal_002C5442"
POWER_TERMINAL = "Fragments:Terminals:TERM_BoSZ04_PowerTerminal_0015A020"
TEST_TERMINAL = "Fragments:Terminals:TERM_BoSZ04_VTUTestTerminal_001D616D"

PATCH_MEMBERS = {
    "DefaultDailyQuestScript": {"onstoryscript"},
    "BoSz04MissionNoteScript": {"onread"},
    "BoSStartQuestTriggerScript": {"ontriggerenter"},
    "BoSz04_CentrifugeScript": {"onactivate"},
    QUEST_FRAGMENT: {
        "fragment_stage_0001_item_00",
        "fragment_stage_0002_item_00",
        "fragment_stage_0025_item_00",
        "fragment_stage_0050_item_00",
        "fragment_stage_0075_item_00",
        "fragment_stage_0080_item_00",
        "fragment_stage_0095_item_00",
        "fragment_stage_0100_item_00",
        "fragment_stage_0200_item_00",
        "fragment_stage_0300_item_00",
        "fragment_stage_0350_item_00",
        "fragment_stage_0500_item_00",
        "fragment_stage_9900_item_00",
    },
    PERK_FRAGMENT: {"setbosz04stage"},
    GRANT_TERMINAL: {"fragment_terminal_01"},
    POWER_TERMINAL: {"fragment_terminal_01"},
    TEST_TERMINAL: {
        "getbosz04",
        "fragment_terminal_01",
        "fragment_terminal_02",
        "fragment_terminal_03",
        "fragment_terminal_04",
    },
}


def _member_names(source: str) -> list[str]:
    return [
        name
        for _kind, name, _start, _end in _iter_top_level_papyrus_members(
            source.splitlines()
        )
    ]


def _member_body(source: str, member_name: str) -> str:
    lines = source.splitlines()
    start, end = next(
        (start, end)
        for _kind, name, start, end in _iter_top_level_papyrus_members(lines)
        if name == member_name.lower()
    )
    return "\n".join(lines[start : end + 1])


def _merged_production_source(script_name: str) -> str:
    source_path = SOURCE_ROOT / _script_relative_path(script_name, ".psc")
    patch = _script_patch_source(script_name)
    assert source_path.is_file(), source_path
    assert patch is not None
    return _merge_script_method_patches(
        source_path.read_text(encoding="utf-8"), patch
    )


@pytest.mark.parametrize(("script_name", "members"), PATCH_MEMBERS.items())
def test_bosz04_patch_is_member_only_and_complete(
    script_name: str, members: set[str]
):
    patch = _script_patch_source(script_name)

    assert patch is not None
    assert set(_member_names(patch)) == members
    assert not any(
        line.strip().lower().startswith(("scriptname ", "state "))
        for line in patch.splitlines()
    )
    assert not any(
        " property " in f" {line.strip().lower()} " for line in patch.splitlines()
    )


@pytest.mark.parametrize(("script_name", "members"), PATCH_MEMBERS.items())
def test_bosz04_patch_merges_once_and_native_compiles(
    script_name: str, members: set[str]
):
    patch = _script_patch_source(script_name)
    assert patch is not None
    merged = _merged_production_source(script_name)
    names = _member_names(merged)

    for member in members:
        assert names.count(member) == 1
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


def test_default_daily_script_only_dispatches_the_story_start_branch():
    patch = _script_patch_source("DefaultDailyQuestScript")
    assert patch is not None
    story_event = _member_body(patch, "onstoryscript")

    assert "akKeyword == SQ_RegionDailyQuestKeyword" in story_event
    assert "SetStage(StartingStage_DailyQuestKeyword)" in story_event
    assert "SetStage(StartingStage_OtherKeyword)" in story_event
    assert "RegisterFor" not in patch
    assert ".Reset()" not in patch


def test_default_daily_script_logs_story_dispatch_and_stage_result():
    patch = _script_patch_source("DefaultDailyQuestScript")
    assert patch is not None

    assert "[B21 Daily] DefaultDailyQuest OnStoryScript" in patch
    assert "DefaultDailyQuest setting daily stage=" in patch
    assert "DefaultDailyQuest daily stage result currentStage=" in patch


def test_bosz04_first_run_and_repeat_run_converge_on_the_retained_quest():
    trigger = _script_patch_source("BoSStartQuestTriggerScript")
    grant = _script_patch_source(GRANT_TERMINAL)
    note = _script_patch_source("BoSz04MissionNoteScript")
    quest = _script_patch_source(QUEST_FRAGMENT)
    assert trigger is not None
    assert grant is not None
    assert note is not None
    assert quest is not None

    assert "QuestKeyword.SendStoryEventAndWait" in trigger
    assert "QuestToStart.Start()" in trigger
    assert "pBoSz04_StartKeyword.SendStoryEventAndWait" in grant
    assert "pBoSZ04.SetStage(50)" in grant
    assert "pBoSz04_StartKeyword.SendStoryEventAndWait" in note
    assert "pBoSZ04.SetStage(75)" in note

    collect_dna = _member_body(quest, "fragment_stage_0100_item_00")
    harvested_dna = _member_body(quest, "fragment_stage_0200_item_00")
    centrifuged_dna = _member_body(quest, "fragment_stage_0300_item_00")
    completed = _member_body(quest, "fragment_stage_0500_item_00")

    assert "playerRef.AddPerk(pBoSZ04HarvestSBDNAPerk)" in collect_dna
    assert "playerRef.AddItem(pBoSZ04VultureDNA, 1, False)" in harvested_dna
    assert "playerRef.RemoveItem(pBoSZ04VultureDNA, 1, True)" in centrifuged_dna
    assert "pBoSZ04VultureDNALoadedMessage.Show()" in centrifuged_dna
    assert "Stop()" in completed


@pytest.mark.parametrize(
    "script_name",
    [
        "BoSStartQuestTriggerScript",
        "BoSz04MissionNoteScript",
        "BoSz04_CentrifugeScript",
        QUEST_FRAGMENT,
        PERK_FRAGMENT,
        GRANT_TERMINAL,
        POWER_TERMINAL,
        TEST_TERMINAL,
    ],
)
def test_bosz04_runtime_paths_emit_durable_trace(script_name: str):
    patch = _script_patch_source(script_name)

    assert patch is not None
    assert "[B21 BoSZ04]" in patch


def test_bosz04_start_entrypoints_log_story_and_direct_start_results():
    for script_name in [
        "BoSStartQuestTriggerScript",
        "BoSz04MissionNoteScript",
        GRANT_TERMINAL,
    ]:
        patch = _script_patch_source(script_name)
        assert patch is not None

        assert "story result=" in patch
        assert "direct result running=" in patch


def test_bosz04_world_interactions_restore_the_observed_stage_chain():
    quest = _script_patch_source(QUEST_FRAGMENT)
    perk = _script_patch_source(PERK_FRAGMENT)
    power = _script_patch_source(POWER_TERMINAL)
    terminal = _script_patch_source(TEST_TERMINAL)
    assert quest is not None
    assert perk is not None
    assert power is not None
    assert terminal is not None

    temporary_scorchbeast = _member_body(
        quest, "fragment_stage_0002_item_00"
    )
    assert "Alias_Player.GetReference()" in temporary_scorchbeast
    assert (
        "playerRef.PlaceAtMe(pLvlVulture, 1, False, False, True)"
        in temporary_scorchbeast
    )

    assert "pBoSZ04.SetStage(200)" in _member_body(perk, "setbosz04stage")
    assert "bosz04.SetStage(95)" in _member_body(power, "fragment_terminal_01")
    assert "bosz04.SetStage(350)" in _member_body(
        terminal, "fragment_terminal_02"
    )
    assert "bosz04.SetStage(80)" in _member_body(
        terminal, "fragment_terminal_03"
    )
    assert "bosz04.SetStage(100)" in _member_body(
        terminal, "fragment_terminal_04"
    )


def test_bosz04_montgomery_log_terminal_fragment_is_an_explicit_noop():
    terminal = _script_patch_source(TEST_TERMINAL)
    assert terminal is not None

    montgomery_log = _member_body(terminal, "fragment_terminal_01")

    assert "\tReturn" in montgomery_log
    assert "SetStage(" not in montgomery_log
    assert ".Start(" not in montgomery_log
