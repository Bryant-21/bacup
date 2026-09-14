from __future__ import annotations

from pathlib import Path

import pytest

from bacup_lib.tests.test_terminal_fragment_script_patches import _fo4_base_source
from bacup_lib.workflows.unified import (
    _iter_top_level_papyrus_members,
    _merge_script_method_patches,
    _script_patch_source,
)
from creation_lib.pex.native_runtime import compile_psc


SCRIPT_NAME = "DefaultCheckpointingScript"
SKELETON = """Scriptname DefaultCheckpointingScript Extends Quest default conditional

Struct checkpointStage
    Int checkpointValue
    String description
    Bool showMessage = True
    Int stageToSet
EndStruct

actorvalue Property CheckpointAV Auto mandatory
Bool Property CheckpointingInProgress Auto hidden conditional
message Property CheckpointMessage Auto mandatory
Int Property currentCheckpointValue Auto hidden conditional
checkpointStage[] Property CheckpointStages Auto
"""


def _patch() -> str:
    patch = _script_patch_source(SCRIPT_NAME)
    assert patch is not None
    return patch


def _merged() -> str:
    return _merge_script_method_patches(SKELETON, _patch())


def _member_names(source: str) -> list[str]:
    return [
        name
        for _kind, name, _start, _end in _iter_top_level_papyrus_members(
            source.splitlines()
        )
    ]


def _highest_eligible_stage(
    checkpoints: list[tuple[int, int]], current_value: int
) -> int | None:
    eligible = [item for item in checkpoints if item[0] <= current_value]
    if not eligible:
        return None
    return max(eligible, key=lambda item: item[0])[1]


def test_default_checkpointing_patch_preserves_source_declarations_and_merges_once():
    patch = _patch()
    merged = _merged()
    names = _member_names(merged)

    assert "Scriptname " not in patch
    assert " Property " not in patch
    assert names.count("onquestinit") == 1
    assert names.count("findcheckpointstageindex") == 1
    assert "Struct checkpointStage" in merged
    assert "checkpointStage[] Property CheckpointStages Auto" in merged
    assert "Bool Property CheckpointingInProgress Auto hidden conditional" in merged
    assert _merge_script_method_patches(merged, patch) == merged


@pytest.mark.parametrize(
    ("current_value", "expected_stage"),
    [
        (9, None),
        (10, 300),
        (19, 300),
        (20, 400),
        (99, 400),
    ],
)
def test_default_checkpointing_selects_highest_eligible_unsorted_boundary(
    current_value: int, expected_stage: int | None
):
    checkpoints = [(20, 400), (10, 300)]
    patch = _patch()

    assert "checkpointValue <= aiCheckpointValue" in patch
    assert (
        "checkpointValue > CheckpointStages[selectedIndex].checkpointValue"
        in patch
    )
    assert _highest_eligible_stage(checkpoints, current_value) == expected_stage


def test_default_checkpointing_guards_reentry_and_optional_bindings():
    patch = _patch()

    assert patch.index("If CheckpointingInProgress") < patch.index(
        "CheckpointingInProgress = True"
    )
    assert patch.count("CheckpointingInProgress = True") == 1
    assert patch.count("CheckpointingInProgress = False") == 1
    assert "Actor playerRef = Game.GetPlayer()" in patch
    assert "playerRef != None && CheckpointAV != None" in patch
    assert "CheckpointStages == None || CheckpointStages.Length == 0" in patch
    assert "showMessage && CheckpointMessage != None" in patch
    assert "GetGroup" not in patch
    assert "QuestInstance" not in patch


def test_default_checkpointing_full_merged_source_native_compiles():
    base_source = _fo4_base_source()
    assert base_source is not None, "FO4 base Papyrus sources unavailable"

    result = compile_psc(
        _merged(),
        imports=[str(base_source)],
        game="fo4",
        flags=str(Path(base_source) / "Institute_Papyrus_Flags.flg"),
        source_path="DefaultCheckpointingScript.psc",
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None
