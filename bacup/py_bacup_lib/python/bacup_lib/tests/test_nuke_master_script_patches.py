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
SCRIPT_NAME = "Nuke_MasterScript"


def _skeleton() -> str:
    return (SOURCE_ROOT / _script_relative_path(SCRIPT_NAME, ".psc")).read_text(
        encoding="utf-8"
    )


def _merged() -> str:
    skeleton = _skeleton()
    patch = _script_patch_source(SCRIPT_NAME)
    assert patch is not None
    assert "Scriptname" not in patch
    assert "Property" not in patch
    merged = _merge_script_method_patches(skeleton, patch)
    assert _merge_script_method_patches(merged, patch) == merged
    return merged


def _on_quest_init(source: str) -> str:
    start = source.index("Event OnQuestInit()")
    end = source.index("EndEvent", start) + len("EndEvent")
    return source[start:end]


def test_nuke_master_patch_preserves_exact_generated_declarations() -> None:
    skeleton = _skeleton().rstrip()
    merged = _merged()

    assert merged.startswith(skeleton)
    for declaration in (
        "Bool checkCodeResetBusy",
        "Bool Property Initialized = False Auto hidden conditional",
        "quest Property EN07_MQ_Nuke_Master Auto mandatory",
        "globalvariable Property SQ_TimestampToday Auto mandatory",
        "Bool Property NukeCodesSolutionAvailable = False Auto conditional",
        "Bool Property NukeCodesActive = False Auto conditional",
        "Float Property currentCodeStartTimestamp = -1.0 Auto",
        "Int Property CurrentCodeIndex = -1 Auto",
        "keyword Property Nuke_CodesStartQuest Auto mandatory",
        "actorvalue Property Nuke_CodeYearAV Auto",
        "actorvalue Property Nuke_CodeIndexAV Auto",
    ):
        assert merged.count(declaration) == 1


def test_nuke_master_onquestinit_merges_once_and_is_idempotent() -> None:
    merged = _merged()
    members = [
        name.lower()
        for _kind, name, _start, _end in _iter_top_level_papyrus_members(
            merged.splitlines()
        )
    ]
    assert members.count("onquestinit") == 1

    init = _on_quest_init(merged)
    assert "If Initialized" in init
    assert init.index("If Initialized") < init.index("checkCodeResetBusy = False")
    assert "CurrentCodeIndex = 0" in init
    assert "currentCodeStartTimestamp = SQ_TimestampToday.GetValue()" in init
    assert "currentCodeStartTimestamp = 0.0" in init
    assert "NukeCodesSolutionAvailable = False" in init
    assert "NukeCodesActive = True" in init
    assert "player.SetValue(Nuke_CodeIndexAV, 0.0)" in init
    assert "player.SetValue(Nuke_CodeYearAV, 0.0)" in init
    assert init.index("Initialized = True") < init.index(
        "Nuke_CodesStartQuest.SendStoryEvent(None, player, player)"
    )


def test_nuke_master_starts_nuke_codes_only_through_story_manager() -> None:
    merged = _merged()
    init = _on_quest_init(merged)

    assert "Nuke_CodesStartQuest.SendStoryEvent(None, player, player)" in init
    assert "SendStoryEventAndWait" not in init
    assert "Game.GetFormFromFile(0x003DA647" not in merged
    assert "EN07_MQ_Nuke_Master.Start()" not in merged
    assert ".Start()" not in merged


def test_nuke_master_patch_compiles() -> None:
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
