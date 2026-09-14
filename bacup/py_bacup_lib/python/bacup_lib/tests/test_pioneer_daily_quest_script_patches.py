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
OPERATION_TIDY = "D01C_OperationTidyScript"
STINGS_AND_THINGS = "D01C_StingsAndThingsScript"
DISPENSER_ALIAS = "D01C_Tidy_DispenserActivate"
TADPOLE_ROOT = "P01C_TadpoleQuest"
OPERATION_FRAGMENT = "Fragments:Quests:QF_D01C_OperationTidy_0045E3D2"
STINGS_FRAGMENT = "Fragments:Quests:QF_D01C_StingsAndThings_0045E3D3"
TADPOLE_FRAGMENT = "Fragments:Quests:QF_P01C_Tadpole_0047B7B9"
POMPY_POINTER = "Fragments:Quests:QF_D01C_Misc_Pompy_00536D7F"
TREADLY_POINTER = "Fragments:Quests:QF_D01C_Misc_Treadly_00536D80"

PATCH_MEMBERS = {
    OPERATION_TIDY: {
        "onquestinit",
        "registeraliasactivation",
        "ispompy",
        "isdailyavailabletoday",
        "tryrestartdaily",
        "objectreference.onactivate",
        "setupwastedispensers",
        "binddispenseralias",
        "collectwaste",
        "cleanupwastedispensers",
    },
    STINGS_AND_THINGS: {
        "onquestinit",
        "istreadly",
        "isdailyavailabletoday",
        "tryrestartdaily",
        "objectreference.onactivate",
        "checkexistingparts",
        "checkallpartscollected",
    },
    DISPENSER_ALIAS: {"onactivate"},
    TADPOLE_ROOT: {
        "onquestinit",
        "onactivitycompleted",
        "updatevalueprogress",
    },
    OPERATION_FRAGMENT: {
        "fragment_stage_0010_item_00",
        "fragment_stage_0020_item_00",
        "fragment_stage_0030_item_00",
        "fragment_stage_0040_item_00",
        "fragment_stage_0050_item_00",
        "fragment_stage_0100_item_00",
        "fragment_stage_0200_item_00",
        "fragment_stage_0300_item_00",
        "fragment_stage_0900_item_00",
        "fragment_stage_1000_item_00",
        "fragment_stage_10000_item_00",
    },
    STINGS_FRAGMENT: {
        "fragment_stage_0010_item_00",
        "fragment_stage_0020_item_00",
        "fragment_stage_0030_item_00",
        "fragment_stage_0040_item_00",
        "fragment_stage_0050_item_00",
        "fragment_stage_0100_item_00",
        "fragment_stage_0200_item_00",
        "fragment_stage_0800_item_00",
        "fragment_stage_0900_item_00",
        "fragment_stage_1000_item_00",
        "fragment_stage_10000_item_00",
    },
    TADPOLE_FRAGMENT: {
        "fragment_stage_0200_item_00",
        "fragment_stage_0300_item_00",
        "fragment_stage_0400_item_00",
        "fragment_stage_0500_item_00",
        "fragment_stage_1000_item_00",
        "fragment_stage_1100_item_00",
        "fragment_stage_1200_item_00",
        "fragment_stage_1300_item_00",
        "fragment_stage_2000_item_00",
        "fragment_stage_2100_item_00",
        "fragment_stage_2200_item_00",
        "fragment_stage_2300_item_00",
        "fragment_stage_9000_item_00",
    },
    POMPY_POINTER: {
        "onquestinit",
        "ispompy",
        "isdailyavailabletoday",
        "trystartdaily",
        "objectreference.onactivate",
        "fragment_stage_0100_item_00",
        "fragment_stage_9000_item_00",
    },
    TREADLY_POINTER: {
        "onquestinit",
        "istreadly",
        "isdailyavailabletoday",
        "trystartdaily",
        "objectreference.onactivate",
        "fragment_stage_0100_item_00",
        "fragment_stage_9000_item_00",
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
def test_pioneer_daily_patches_are_member_only_and_merge_once(
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

    merged = _merged_production_source(script_name)
    names = _member_names(merged)
    for member in members:
        assert names.count(member) == 1
    assert _merge_script_method_patches(merged, patch) == merged


def test_pioneer_daily_merged_sources_native_compile(tmp_path: Path):
    merged_sources = {
        script_name: _merged_production_source(script_name)
        for script_name in PATCH_MEMBERS
    }
    base_source = _fo4_base_source()
    assert base_source is not None, "FO4 base Papyrus sources unavailable"

    patched_imports = tmp_path / "patched_imports"
    patched_imports.mkdir()
    for script_name, source in merged_sources.items():
        import_path = patched_imports / _script_relative_path(script_name, ".psc")
        import_path.parent.mkdir(parents=True, exist_ok=True)
        import_path.write_text(source, encoding="utf-8")

    for script_name, source in merged_sources.items():
        result = compile_psc(
            source,
            imports=[
                str(patched_imports),
                str(SOURCE_ROOT),
                str(base_source),
            ],
            game="fo4",
            flags=str(base_source / "Institute_Papyrus_Flags.flg"),
            source_path=str(_script_relative_path(script_name, ".psc")),
        )
        diagnostics = "\n".join(str(item) for item in result.diagnostics)
        assert result.ok, f"{script_name}\n{diagnostics}"
        assert result.pex_bytes is not None


def test_tadpole_stage_500_uses_story_manager_for_both_dailies():
    patch = _script_patch_source(TADPOLE_FRAGMENT)
    assert patch is not None
    stage_500 = _member_body(patch, "fragment_stage_0500_item_00")

    assert "OpTidyQuest.Start()" not in stage_500
    assert "StingsAndThingsQuest.Start()" not in stage_500
    assert "TidyQuestKeyword.SendStoryEventAndWait(None, playerRef, playerRef)" in stage_500
    assert "StingsQuestKeyword.SendStoryEventAndWait(None, playerRef, playerRef)" in stage_500
    assert "owningQuest.SetObjectiveDisplayed(60)" in stage_500
    assert "owningQuest.SetObjectiveDisplayed(70)" in stage_500


def test_operation_tidy_preserves_location_aliases_and_local_activation():
    root = _script_patch_source(OPERATION_TIDY)
    alias = _script_patch_source(DISPENSER_ALIAS)
    assert root is not None
    assert alias is not None

    initialization = _member_body(root, "onquestinit")
    setup = _member_body(root, "setupwastedispensers")
    activation = _member_body(root, "objectreference.onactivate")
    collection = _member_body(root, "collectwaste")

    assert "RegisterAliasActivation(0)" in initialization
    assert "RegisterAliasActivation(14)" in initialization
    assert "ChosenGooPiles.GetCount() < 5" in setup
    assert "BindDispenserAlias(ToxicWasteDispenser05, 4)" in setup
    assert "akActionRef != playerRef" in activation
    assert "IsStageDone(300)" in activation
    assert "dispenserLock = true" in collection
    assert "playerRef.AddItem(D01C_ToxicWaste, 1, false)" in collection
    assert "tidyQuest.CollectWaste(GetReference(), StageToSet)" in alias


def test_stings_tracks_existing_inventory_and_requires_return_activation():
    root = _script_patch_source(STINGS_AND_THINGS)
    fragment = _script_patch_source(STINGS_FRAGMENT)
    assert root is not None
    assert fragment is not None

    activation = _member_body(root, "objectreference.onactivate")
    existing = _member_body(root, "checkexistingparts")
    completion = _member_body(fragment, "fragment_stage_1000_item_00")

    assert "akActionRef != PlayerRef" in activation
    assert "IsStageDone(StageCollected)" in activation
    assert "SetStage(1000)" in activation
    assert "PlayerRef.GetItemCount(ItemsNeeded[partIndex].Item) > 0" in existing
    assert "SetStage(ItemsNeeded[partIndex].Stage)" in existing
    assert "playerRef.RemoveItem(BloatflyGland, 1, true)" in completion
    assert "playerRef.RemoveItem(TickBloodSac, 1, true)" in completion
    assert (
        'Game.GetFormFromFile(0x0045E3F6, "SeventySix.esm") as Potion'
        in completion
    )
    assert "playerRef.AddItem(insectRepellent, 1, true)" in completion
    assert "AddItem(co_D01C_InsectRepellent_WorkbenchChemLab" not in completion


@pytest.mark.parametrize(
    ("script_name", "daily_id", "keyword_id", "timestamp_id"),
    [
        (POMPY_POINTER, "0045E3D2", "0045E3EB", "0045E3E5"),
        (TREADLY_POINTER, "0045E3D3", "0046A629", "0046C95A"),
    ],
)
def test_pointer_activations_reenter_next_day_through_story_manager(
    script_name: str, daily_id: str, keyword_id: str, timestamp_id: str
):
    patch = _script_patch_source(script_name)
    assert patch is not None

    availability = _member_body(patch, "isdailyavailabletoday")
    restart = _member_body(patch, "trystartdaily")
    activation = _member_body(patch, "objectreference.onactivate")

    assert f"0x{timestamp_id}" in availability
    assert "(Utility.GetCurrentGameTime() as Int) > (completedAt as Int)" in availability
    assert f"0x{daily_id}" in restart
    assert f"0x{keyword_id}" in restart
    assert "dailyQuest.IsCompleted() || dailyQuest.IsStopped()" in restart
    assert "dailyQuest.Reset()" in restart
    assert "startKeyword.SendStoryEventAndWait(None, akPlayer, akPlayer)" in restart
    assert ".Start()" not in restart
    assert "akActionRef != playerRef" in activation
    assert "IsDailyAvailableToday(playerRef)" in activation


@pytest.mark.parametrize(
    ("script_name", "keyword_id", "timestamp_id"),
    [
        (OPERATION_TIDY, "0045E3EB", "0045E3E5"),
        (STINGS_AND_THINGS, "0046A629", "0046C95A"),
    ],
)
def test_completed_daily_leader_activation_has_story_manager_fallback(
    script_name: str, keyword_id: str, timestamp_id: str
):
    patch = _script_patch_source(script_name)
    assert patch is not None

    availability = _member_body(patch, "isdailyavailabletoday")
    restart = _member_body(patch, "tryrestartdaily")
    activation = _member_body(patch, "objectreference.onactivate")

    assert f"0x{timestamp_id}" in availability
    assert f"0x{keyword_id}" in restart
    assert "dailyQuest.IsRunning()" in restart
    assert "dailyQuest.Stop()" in restart
    assert "dailyQuest.Reset()" in restart
    assert "startKeyword.SendStoryEventAndWait(None, akPlayer, akPlayer)" in restart
    assert ".Start()" not in restart
    assert "IsStageDone(1000)" in activation


def test_daily_completion_keeps_leader_registration_alive_until_next_day():
    operation = _script_patch_source(OPERATION_FRAGMENT)
    stings = _script_patch_source(STINGS_FRAGMENT)
    pompy = _script_patch_source(POMPY_POINTER)
    treadly = _script_patch_source(TREADLY_POINTER)
    assert operation is not None
    assert stings is not None
    assert pompy is not None
    assert treadly is not None

    operation_completion = _member_body(
        operation, "fragment_stage_1000_item_00"
    )
    stings_completion = _member_body(stings, "fragment_stage_1000_item_00")
    pompy_completion = _member_body(pompy, "fragment_stage_9000_item_00")
    treadly_completion = _member_body(treadly, "fragment_stage_9000_item_00")

    assert ".Stop()" not in operation_completion
    assert ".Stop()" not in stings_completion
    assert ".Stop()" not in pompy_completion
    assert ".Stop()" not in treadly_completion
    assert "tadpole.OnActivityCompleted(0)" in operation_completion
    assert "tadpole.OnActivityCompleted(1)" in stings_completion


def test_daily_completion_notifies_tadpole_once_by_activity_index():
    operation = _script_patch_source(OPERATION_FRAGMENT)
    stings = _script_patch_source(STINGS_FRAGMENT)
    tadpole = _script_patch_source(TADPOLE_ROOT)
    assert operation is not None
    assert stings is not None
    assert tadpole is not None

    operation_completion = _member_body(
        operation, "fragment_stage_1000_item_00"
    )
    stings_completion = _member_body(stings, "fragment_stage_1000_item_00")
    activity = _member_body(tadpole, "onactivitycompleted")

    assert "tadpole.OnActivityCompleted(0)" in operation_completion
    assert "tadpole.OnActivityCompleted(1)" in stings_completion
    assert "ActivityCompletionStatus[aiActivityIndex]" in activity
    assert "SetStage(DailyTidyStage)" in activity
    assert "SetStage(DailyStingsStage)" in activity
