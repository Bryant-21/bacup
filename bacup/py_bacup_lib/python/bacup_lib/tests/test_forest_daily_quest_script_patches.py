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
ECOLOGICAL_BALANCE = "Fragments:Quests:QF_RS03_Balance_0001C035"
STRANGE_BREW = "Fragments:Quests:QF_FFZ13_StrangeBrew_0004695D"

PATCH_MEMBERS = {
    "FF05_Balance_SensorMessageScript": {"onactivate"},
    "ffz13_questscript": {"disablehivemarker"},
    "FFZ13_HiveCollectionScript": {
        "onaliasinit",
        "onaliasreset",
        "registerhiveinventoryevents",
        "onitemremoved",
    },
    ECOLOGICAL_BALANCE: {
        "fragment_stage_0010_item_00",
        "fragment_stage_0015_item_00",
        "fragment_stage_0020_item_00",
        "fragment_stage_0025_item_00",
        "fragment_stage_0030_item_00",
        "fragment_stage_0040_item_00",
        "fragment_stage_0050_item_00",
        "fragment_stage_0100_item_00",
        "fragment_stage_0200_item_00",
        "fragment_stage_0300_item_00",
        "fragment_stage_0350_item_00",
        "fragment_stage_0400_item_00",
        "fragment_stage_0500_item_00",
        "fragment_stage_0600_item_00",
        "fragment_stage_0650_item_00",
        "fragment_stage_1000_item_00",
        "fragment_stage_2000_item_00",
    },
    STRANGE_BREW: {
        "fragment_stage_0001_item_00",
        "fragment_stage_0005_item_00",
        "fragment_stage_0007_item_00",
        "fragment_stage_0010_item_00",
        "fragment_stage_0015_item_00",
        "fragment_stage_0030_item_00",
        "fragment_stage_0041_item_00",
        "fragment_stage_0042_item_00",
        "fragment_stage_0043_item_00",
        "fragment_stage_0044_item_00",
        "fragment_stage_0050_item_00",
        "fragment_stage_0070_item_00",
        "fragment_stage_0090_item_00",
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
def test_forest_daily_patches_are_member_only_and_complete(
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
def test_forest_daily_patches_merge_once_and_native_compile(
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


def test_ecological_balance_first_run_and_daily_repeat_diverge_at_start():
    patch = _script_patch_source(ECOLOGICAL_BALANCE)
    assert patch is not None

    start = _member_body(patch, "fragment_stage_0010_item_00")
    sensor_setup = _member_body(patch, "fragment_stage_0050_item_00")

    assert "playerRef.SetValue(FF05_Balance_Started, 1.0)" in start
    assert "playerRef.GetValue(FF05_Balance_Completed) >= 1.0" in start
    assert "SetStage(50)" in start
    assert "SetObjectiveDisplayed(2, True)" in start
    assert "SetObjectiveDisplayed(10, True)" in sensor_setup
    assert "SetObjectiveDisplayed(20, True)" in sensor_setup
    assert "SetObjectiveDisplayed(30, True)" in sensor_setup


def test_ecological_balance_sensor_collection_and_upload_are_inventory_backed():
    patch = _script_patch_source(ECOLOGICAL_BALANCE)
    assert patch is not None

    collected = [
        _member_body(patch, f"fragment_stage_0{stage:03d}_item_00")
        for stage in (100, 200, 300)
    ]
    uploaded = [
        _member_body(patch, f"fragment_stage_0{stage:03d}_item_00")
        for stage in (400, 500, 600)
    ]

    for body in collected:
        assert "playerRef.AddItem(dataHolotape, 1, False)" in body
    for body in uploaded:
        assert "playerRef.RemoveItem(dataHolotape, 1, True)" in body
        assert "SetStage(650)" in body

    completion = _member_body(patch, "fragment_stage_1000_item_00")
    shutdown = _member_body(patch, "fragment_stage_2000_item_00")
    assert "playerRef.SetValue(FF05_Balance_Completed, 1.0)" in completion
    assert "Stop()" in completion
    assert "playerRef.SetValue(FF05_Balance_Started, 0.0)" in shutdown


def test_ecological_balance_sensor_activation_keeps_the_quest_guard():
    patch = _script_patch_source("FF05_Balance_SensorMessageScript")
    assert patch is not None
    activation = _member_body(patch, "onactivate")

    assert "akActionRef == Game.GetPlayer()" in activation
    assert "FF05_Balance.IsRunning()" in activation
    assert "FF05_Balance_SensorMessage.Show()" in activation


def test_strange_brew_clears_the_bound_hive_marker():
    patch = _script_patch_source("ffz13_questscript")
    assert patch is not None

    marker_cleanup = _member_body(patch, "disablehivemarker")

    assert "ChosenHiveQTs[aiHiveIndex].Clear()" in marker_cleanup


def test_strange_brew_hive_collections_reset_and_count_each_hive_once():
    patch = _script_patch_source("FFZ13_HiveCollectionScript")
    assert patch is not None
    initialized = _member_body(patch, "onaliasinit")
    reset = _member_body(patch, "onaliasreset")
    removed = _member_body(patch, "onitemremoved")

    assert "HivesLooted = 0" in initialized
    assert "HivesLooted = 0" in reset
    assert "akDestContainer != Alias_Player.GetReference()" in removed
    assert "Find(akSenderRef) < 0" in removed
    assert "akSenderRef.GetItemCount(akBaseItem) > 0" in removed
    assert "Int hiveCount = GetCount() + HivesLooted" in removed
    assert "RemoveRef(akSenderRef)" in removed
    assert "HivesLooted += 1" in removed
    assert "(HivesLooted as Float) / hiveCount" in removed
    assert "owningQuest.SetStage(StageToSetToDisableQT)" in removed


def test_strange_brew_preserves_location_start_scenes_honey_and_completion():
    patch = _script_patch_source(STRANGE_BREW)
    assert patch is not None

    start = _member_body(patch, "fragment_stage_0001_item_00")
    welcome = _member_body(patch, "fragment_stage_0015_item_00")
    honey_collection = _member_body(patch, "fragment_stage_0030_item_00")
    honey_ready = _member_body(patch, "fragment_stage_0050_item_00")
    turn_in = _member_body(patch, "fragment_stage_0070_item_00")
    completion = _member_body(patch, "fragment_stage_0090_item_00")

    assert "startLocation == teapotLocation" in start
    assert "SetStage(10)" in start
    assert "SetStage(5)" in start
    assert "WelcomeScene.Start()" in welcome
    assert "SetStage(30)" in welcome
    assert "playerRef.GetItemCount(Honey) >= 10" in honey_collection
    assert "SetStage(50)" in honey_collection
    assert "SetObjectiveCompleted(10, True)" in honey_ready
    assert "SetObjectiveDisplayed(20, True)" in honey_ready
    assert "playerRef.RemoveItem(Honey, 10, True)" in turn_in
    assert "GoodbyeScene.Start()" in turn_in
    assert "SetStage(90)" in turn_in
    assert "CompleteAllObjectives()" in completion
    assert "playerRef.SetValue(FFZ13_Brew_Completed, 1.0)" in completion
    assert "Stop()" in completion
