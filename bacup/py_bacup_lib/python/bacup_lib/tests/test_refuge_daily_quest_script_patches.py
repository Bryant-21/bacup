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

HUB_QUEST = "Quests:XPD_Hub_Responders:XPD_Hub_Responders_QuestScript"
HUB_FRAGMENT = "Fragments:Quests:QF_XPD_Hub_Responders_0062F5D6"
CODE_BLUE = "Quests:XPD_Hub_CodeBlue:QuestScript"
MUTUAL = "Quests:XPD_Hub_Mutual:XPD_Hub_Mutual_QuestScript"
RECIPE = "Quests:XPD_Hub_Recipe:QuestScript"

PATCH_MEMBERS = {
    "DefaultCollectionAliasOnMenuItemRun": {
        "onaliasinit",
        "terminal.onmenuitemrun",
        "onaliasshutdown",
    },
    HUB_QUEST: {
        "onquestinit",
        "onstageset",
        "ontimer",
        "checkrequiredquests",
        "onquestshutdown",
    },
    HUB_FRAGMENT: {
        "fragment_stage_0100_item_00",
        "fragment_stage_0200_item_00",
        "fragment_stage_0201_item_00",
        "fragment_stage_0299_item_00",
        "fragment_stage_0300_item_00",
        "fragment_stage_0310_item_00",
        "fragment_stage_0400_item_00",
        "fragment_stage_0500_item_00",
        "fragment_stage_0600_item_00",
        "fragment_stage_0700_item_00",
        "fragment_stage_0800_item_00",
        "fragment_stage_0804_item_00",
        "fragment_stage_0807_item_00",
        "fragment_stage_0900_item_00",
        "fragment_stage_1000_item_00",
        "fragment_stage_9000_item_00",
        "movealiastomarker",
    },
    CODE_BLUE: {
        "onquestinit",
        "selectscenario",
        "onstageset",
    },
    MUTUAL: {
        "onquestinit",
        "selectdonation",
        "displaydonationobjectives",
        "completedonation",
        "onstageset",
    },
    RECIPE: {
        "onquestinit",
        "referencealias.onactivate",
        "ontimer",
        "checkingredientcollection",
        "checkingredientpreparation",
        "checkspices",
        "onstageset",
        "onquestshutdown",
    },
    "XPD_Fuel_ARGScript": {
        "onquestinit",
        "selectplantregions",
        "selectphotoobjectives",
        "ontimer",
        "checkphotoobjectives",
        "checktaskcompletion",
        "onstageset",
        "onquestshutdown",
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
def test_refuge_patch_is_member_only_and_complete(
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
        " property " in f" {line.strip().lower()} "
        for line in patch.splitlines()
    )


@pytest.mark.parametrize(("script_name", "members"), PATCH_MEMBERS.items())
def test_refuge_patch_merges_once_and_native_compiles(
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
    assert result.ok, f"{script_name}\n{diagnostics}"
    assert result.pex_bytes is not None


def test_code_blue_terminal_helper_restores_menu_dispatch_contract():
    patch = _script_patch_source("DefaultCollectionAliasOnMenuItemRun")
    assert patch is not None
    menu_event = _member_body(patch, "terminal.onmenuitemrun")

    assert 'RegisterForRemoteEvent(targetTerminal, "OnMenuItemRun")' in patch
    assert "Find(akTerminalRef) < 0" in menu_event
    assert "menuEntry.TargetTerminal == akSender" in menu_event
    assert "menuEntry.iMenuItemTarget == auiMenuItemID" in menu_event
    assert "menuEntry.QuestOwningPlayerAlias.GetReference() == playerRef" in menu_event
    assert "owningQuest.SetStage(menuEntry.iStageToSet)" in menu_event
    assert 'UnregisterForRemoteEvent(targetTerminal, "OnMenuItemRun")' in patch


def test_hub_preserves_completed_fuel_quests_and_starts_only_required_quests():
    fragment = _script_patch_source(HUB_FRAGMENT)
    hub = _script_patch_source(HUB_QUEST)
    assert fragment is not None
    assert hub is not None

    start = _member_body(fragment, "fragment_stage_0310_item_00")
    assert "CodeBlue_StartKeyword.SendStoryEventAndWait" in start
    assert "Mutual_StartKeyword.SendStoryEventAndWait" in start
    assert "Recipe_StartKeyword.SendStoryEventAndWait" in start
    for quest_id, variable_name, keyword_name in (
        ("0063BED4", "codeBlue", "CodeBlue_StartKeyword"),
        ("0063D5BD", "mutualAid", "Mutual_StartKeyword"),
        ("00621FB7", "recipeForSuccess", "Recipe_StartKeyword"),
    ):
        assert (
            f'Game.GetFormFromFile(0x{quest_id}, "SeventySix.esm") as Quest'
            in start
        )
        assert (
            f"!{variable_name}.IsRunning() && !{variable_name}.IsCompleted()"
            in start
        )
        assert f"If {variable_name}.IsStopped()" in start
        reset = f"{variable_name}.Reset()"
        send = f"{keyword_name}.SendStoryEventAndWait"
        assert reset in start
        assert start.index(reset) < start.index(send)
    assert (
        'Game.GetFormFromFile(0x0063D33F, "SeventySix.esm") as Quest'
        not in start
    )
    assert (
        'Game.GetFormFromFile(0x0063D35F, "SeventySix.esm") as Keyword'
        not in start
    )
    assert "refugeesGuide" not in start
    assert ".Start()" not in start
    assert "requiredQuest.Start()" not in hub
    assert "requiredQuest.IsCompleted()" in hub
    assert "entry.Quest_Required" in hub
    assert "entry.StageToSet" in hub


def test_code_blue_waits_for_container_transfer_after_terminal_stage():
    patch = _script_patch_source(CODE_BLUE)
    assert patch is not None

    select = _member_body(patch, "selectscenario")
    stages = _member_body(patch, "onstageset")
    assert "ScenarioData[scenarioIndex]" in select
    assert "Loc_Current.ForceLocationTo" in select
    assert "Terminal_Current.ForceRefTo" in select
    assert "Container_RandomQuestContainer.ForceRefTo" in select
    assert "auiStageID == 205" in stages
    stage_205 = stages[
        stages.index("ElseIf auiStageID == 205") :
        stages.index("ElseIf auiStageID == 210")
    ]
    assert "SetStage(210)" not in stage_205
    assert "SetObjectiveDisplayed(60)" in stage_205
    assert "chosenScenario.RetrieveItemObjective" in stages


def test_mutual_selects_only_bound_donations_and_charges_on_completion():
    patch = _script_patch_source(MUTUAL)
    assert patch is not None

    select = _member_body(patch, "selectdonation")
    display = _member_body(patch, "displaydonationobjectives")
    complete = _member_body(patch, "completedonation")
    stages = _member_body(patch, "onstageset")
    assert "Items[Utility.RandomInt(0, Items.Length - 1)]" in select
    assert "selectedDonation = ChosenItem.ItemToDonate" in select
    assert "numToDonate = StandardQuantity" in select
    assert "SetObjectiveDisplayed(ChosenItem.StageToSet)" in display
    assert "SetStage(ChosenItem.StageToSet)" in display
    assert "SetStage(ChosenItem.StageToSet + 50)" in stages
    assert "myPlayer.GetItemCount(selectedDonation) < numToDonate" in complete
    assert "myPlayer.RemoveItem(selectedDonation, numToDonate, true)" in complete
    assert "SetStage(9000)" in complete
    assert "auiStageID == CompletionStage" in stages
    assert "CompleteDonation()" in stages


def test_recipe_uses_alias_activations_and_a_local_cooking_timer():
    patch = _script_patch_source(RECIPE)
    assert patch is not None

    ladle = _member_body(patch, "referencealias.onactivate")
    stages = _member_body(patch, "onstageset")
    assert 'RegisterForRemoteEvent(Activator_Ladle, "OnActivate")' in patch
    assert "akActionRef != PlayerRef" in ladle
    assert "XPD_Fuel_Recipe_StirMessage.Show()" in ladle
    assert "XPD_Fuel_Recipe_CantStir.Show()" in ladle
    assert "IsStageDone(310) && IsStageDone(320) && IsStageDone(330)" in patch
    assert "IsStageDone(345) && IsStageDone(350) && IsStageDone(355)" in patch
    assert "IsStageDone(362) && IsStageDone(364)" in patch
    assert "auiStageID == 380" in stages
    assert "SetStage(9000)" in stages


def test_refugees_guide_replaces_photo_service_with_proximity_checks():
    patch = _script_patch_source("XPD_Fuel_ARGScript")
    assert patch is not None

    photos = _member_body(patch, "checkphotoobjectives")
    completion = _member_body(patch, "checktaskcompletion")
    assert "PhotoObjectives[candidateIndex]" in patch
    assert "PlayerRef.GetDistance(objectiveData.PhotoRef) <= 768.0" in photos
    assert "SetStage(objectiveData.PhotoObjective_CompletionStage)" in photos
    for stage in (800, 850, 900, 950, 1000, 1050, 1100):
        assert f"IsStageDone({stage})" in completion
    assert "plantCounter >= totalPlants && photoCount >= totalPhotos" in completion
    assert "GetFormFromFile" not in patch


def test_refugees_guide_maps_harvest_selection_and_completion_objectives():
    patch = _script_patch_source("XPD_Fuel_ARGScript")
    assert patch is not None

    stages = _member_body(patch, "onstageset")
    objective_stages = {
        200: (10, 800),
        201: (60, 850),
        202: (110, 900),
        203: (260, 950),
        204: (160, 1000),
        205: (310, 1100),
        206: (210, 1050),
    }
    for selection_stage, (objective, completion_stage) in objective_stages.items():
        assert (
            f"auiStageID == {selection_stage}\n"
            f"        SetObjectiveDisplayed({objective})"
            in stages
        )
        assert (
            f"auiStageID == {completion_stage}\n"
            f"        SetObjectiveCompleted({objective})"
            in stages
        )
