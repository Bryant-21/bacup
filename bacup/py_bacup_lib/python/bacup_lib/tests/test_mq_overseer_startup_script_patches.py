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
ROOT_SCRIPT = "MQ_OverseerQuestScript"
FRAGMENT_SCRIPT = "Fragments:Quests:QF_MQ_Overseer_004E49D9"
PATCH_MEMBERS = {
    ROOT_SCRIPT: {"onquestinit"},
    FRAGMENT_SCRIPT: {"fragment_stage_0005_item_00"},
}


def _member_body(source: str, member_name: str) -> str:
    lines = source.splitlines()
    start, end = next(
        (start, end)
        for _kind, name, start, end in _iter_top_level_papyrus_members(lines)
        if name == member_name.lower()
    )
    return "\n".join(lines[start : end + 1])


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


@pytest.mark.parametrize("script_name", PATCH_MEMBERS)
def test_overseer_startup_patch_is_member_only(script_name: str):
    patch = _script_patch_source(script_name)
    assert patch is not None
    assert set(_member_names(patch)) == PATCH_MEMBERS[script_name]
    assert not any(
        line.strip().lower().startswith(("scriptname ", "state "))
        for line in patch.splitlines()
    )
    assert not any(
        " property " in f" {line.strip().lower()} " for line in patch.splitlines()
    )


def test_onquestinit_sets_stage_five_once():
    patch = _script_patch_source(ROOT_SCRIPT)
    assert patch is not None
    init = _member_body(patch, "onquestinit")
    assert init.count("IsStageDone(5)") == 1
    assert init.count("SetStage(5)") == 1
    assert init.index("!IsStageDone(5)") < init.index("SetStage(5)")


def test_stage_five_displays_only_objective_ten():
    patch = _script_patch_source(FRAGMENT_SCRIPT)
    assert patch is not None
    stage = _member_body(patch, "fragment_stage_0005_item_00")
    assert stage.count("SetObjectiveDisplayed(10)") == 1
    assert "MQ_OverseerStarted" not in stage
    assert "SetValue(" not in stage


@pytest.mark.parametrize("script_name", PATCH_MEMBERS)
def test_production_merge_is_unique_and_idempotent(script_name: str):
    patch = _script_patch_source(script_name)
    assert patch is not None
    merged = _merged_production_source(script_name)
    names = _member_names(merged)

    for member_name in PATCH_MEMBERS[script_name]:
        assert names.count(member_name) == 1
        assert _member_body(merged, member_name) == _member_body(patch, member_name)
    assert _merge_script_method_patches(merged, patch) == merged


@pytest.mark.parametrize("script_name", PATCH_MEMBERS)
def test_full_production_merge_native_compiles_for_fo4(script_name: str):
    base_source = _fo4_base_source()
    assert base_source is not None, "FO4 base Papyrus sources unavailable"

    result = compile_psc(
        _merged_production_source(script_name),
        imports=[str(SOURCE_ROOT), str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=str(_script_relative_path(script_name, ".psc")),
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None
