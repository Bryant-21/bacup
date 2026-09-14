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

PATCH_MEMBERS = {
    "DefaultSendStoryEventOnMenuItemRun": ["onmenuitemrun"],
    "AddToGroupQuestOnMenuItemRun": ["onmenuitemrun"],
    "DefaultAliasSetStageOnMenuItemRun": [
        "onaliasinit",
        "onaliasshutdown",
        "terminal.onmenuitemrun",
        "getaliasterminal",
        "applyvalue",
    ],
}


def _member_names(source: str) -> list[str]:
    return [
        name
        for _kind, name, _start, _end in _iter_top_level_papyrus_members(
            source.splitlines()
        )
    ]


def _merged_production_source(script_name: str) -> str:
    source_path = SOURCE_ROOT / _script_relative_path(script_name, ".psc")
    patch = _script_patch_source(script_name)
    assert source_path.is_file(), source_path
    assert patch is not None
    return _merge_script_method_patches(
        source_path.read_text(encoding="utf-8"), patch
    )


@pytest.mark.parametrize(("script_name", "members"), PATCH_MEMBERS.items())
def test_en05_terminal_story_helper_patch_is_member_only(
    script_name: str, members: list[str]
):
    patch = _script_patch_source(script_name)

    assert patch is not None
    assert _member_names(patch) == members
    assert not any(
        line.strip().lower().startswith(("scriptname ", "state "))
        for line in patch.splitlines()
    )
    assert not any(
        " property " in f" {line.strip().lower()} " for line in patch.splitlines()
    )


@pytest.mark.parametrize(("script_name", "members"), PATCH_MEMBERS.items())
def test_en05_terminal_story_helper_merges_once_and_native_compiles(
    script_name: str, members: list[str]
):
    patch = _script_patch_source(script_name)
    assert patch is not None
    merged = _merged_production_source(script_name)

    merged_members = _member_names(merged)
    for member in members:
        assert merged_members.count(member) == 1
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


def test_en05_course_terminals_fallback_only_after_story_event_start_fails():
    course_patch = _script_patch_source("DefaultSendStoryEventOnMenuItemRun")
    combat_patch = _script_patch_source("AddToGroupQuestOnMenuItemRun")
    assert course_patch is not None
    assert combat_patch is not None

    assert "entry.iMenuItemTarget == auiMenuItemID" in course_patch
    assert "playerRef.HasKeyword(entry.ActiveQuestKeyword)" in course_patch
    assert "playerRef.GetValue(entry.BlockingActorValue)" in course_patch
    assert "akTerminalRef.GetBaseObject() == Self" in course_patch
    assert "fallbackQuest = entry.UniqueQuestToAddPlayer" in course_patch
    assert "fallbackQuest = entry.ExpectedQuestToStart" in course_patch
    assert (
        "entry.UniqueQuestToAddPlayer == entry.ExpectedQuestToStart"
        in course_patch
    )
    assert "!fallbackQuest.IsRunning()" in course_patch
    assert (
        "entry.StoryEventToSend.SendStoryEventAndWait(eventLocation, playerRef, "
        "akTerminalRef, entry.Value1)"
    ) in course_patch
    course_send_index = course_patch.index("entry.StoryEventToSend.SendStoryEventAndWait")
    course_start_index = course_patch.index("fallbackQuest.Start()")
    assert course_send_index < course_start_index

    assert "entry.iMenuItemTarget == auiMenuItemID" in combat_patch
    assert "playerRef.HasKeyword(entry.ActiveQuestKeyword)" in combat_patch
    assert "!entry.QuestToStart.IsRunning()" in combat_patch
    assert (
        "entry.StoryEventToSend.SendStoryEventAndWait(playerRef.GetCurrentLocation(), "
        "playerRef, akTerminalRef, entry.Value1)"
    ) in combat_patch
    combat_send_index = combat_patch.index("entry.StoryEventToSend.SendStoryEventAndWait")
    combat_start_index = combat_patch.index("entry.QuestToStart.Start()")
    assert combat_send_index < combat_start_index

    combined = course_patch + combat_patch
    assert ".AddPlayer(" not in combined


def test_en05_direct_start_requires_the_exact_native_two_member_opt_in():
    patch = _script_patch_source("DefaultSendStoryEventOnMenuItemRun")
    assert patch is not None

    opt_in = (
        "Bool directStartAllowed = entry.ExpectedQuestToStart != None && "
        "entry.UniqueQuestToAddPlayer == entry.ExpectedQuestToStart"
    )
    assert opt_in in patch
    assert (
        "If directStartAllowed && fallbackQuest != None && "
        "!fallbackQuest.IsRunning()"
    ) in patch
    assert patch.count("fallbackQuest.Start()") == 1


def test_rs01b_null_target_uses_the_alias_terminal_reference_exactly():
    patch = _script_patch_source("DefaultAliasSetStageOnMenuItemRun")
    assert patch is not None

    assert "Terminal aliasTerminal = GetAliasTerminal()" in patch
    assert "terminalToRegister = aliasTerminal" in patch
    assert "terminalToUnregister = aliasTerminal" in patch
    assert "entry.TargetTerminal == akSender" in patch
    assert (
        "aliasTerminal == akSender && aliasRef == akTerminalRef" in patch
    )
    assert "Return aliasRef.GetBaseObject() as Terminal" in patch
    assert "entry.iMenuItemTarget == auiMenuItemID" in patch
