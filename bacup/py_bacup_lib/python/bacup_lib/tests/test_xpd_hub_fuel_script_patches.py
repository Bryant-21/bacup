from __future__ import annotations

from collections import Counter
from pathlib import Path

import pytest

from bacup_lib.tests.test_script_patch_conventions import (
    has_unregistered_inventory_handler,
    sibling_script_casts,
)
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

HUB_ROOT = "Quests:XPD_Hub_Responders:XPD_Hub_Responders_QuestScript"
HUB_QF = "Fragments:Quests:QF_XPD_Hub_Responders_0062F5D6"
HUB_ON_CONNECT = "Fragments:Quests:QF_XPD_Hub_Responders_OnConn_0064D323"
CODE_BLUE_ROOT = "Quests:XPD_Hub_CodeBlue:QuestScript"
CODE_BLUE_QF = "Fragments:Quests:QF_XPD_Fuel_CodeBlue_0063BED4"
MUTUAL_ROOT = "Quests:XPD_Hub_Mutual:XPD_Hub_Mutual_QuestScript"
MUTUAL_QF = "Fragments:Quests:QF_XPD_Hub_Mutual_0063D5BD"
RECIPE_ROOT = "Quests:XPD_Hub_Recipe:QuestScript"
RECIPE_QF = "Fragments:Quests:QF_XPD_Hub_Recipe_00621FB7"
ARG_ROOT = "XPD_Fuel_ARGScript"
ARG_QF = "Fragments:Quests:QF_XPD_Fuel_ARefugeesGuide_0063D33F"

PATCHED_SCRIPTS = (
    HUB_ROOT,
    HUB_QF,
    HUB_ON_CONNECT,
    CODE_BLUE_ROOT,
    CODE_BLUE_QF,
    MUTUAL_ROOT,
    MUTUAL_QF,
    RECIPE_ROOT,
    RECIPE_QF,
    ARG_ROOT,
    ARG_QF,
)

BOUND_FRAGMENT_STAGES = {
    HUB_QF: (
        100,
        200,
        201,
        299,
        300,
        310,
        400,
        500,
        600,
        700,
        800,
        804,
        807,
        900,
        1000,
        9000,
    ),
    CODE_BLUE_QF: (50, 100, 110, 200, 205, 210, 500, 9000),
    MUTUAL_QF: (100, 200, 250, 300, 400, 500, 600, 700, 800, 900, 1000, 8000, 9000),
    RECIPE_QF: (
        100,
        300,
        305,
        308,
        309,
        310,
        320,
        330,
        340,
        343,
        345,
        347,
        350,
        353,
        355,
        360,
        361,
        362,
        364,
        366,
        368,
        370,
        375,
        380,
        400,
        410,
        420,
        430,
        450,
        500,
        510,
        520,
        530,
        550,
        560,
        570,
        580,
        9000,
    ),
    ARG_QF: (
        100,
        101,
        104,
        105,
        106,
        109,
        200,
        201,
        202,
        203,
        204,
        205,
        206,
        800,
        820,
        830,
        840,
        850,
        870,
        880,
        890,
        900,
        920,
        930,
        940,
        950,
        970,
        980,
        990,
        1000,
        1020,
        1030,
        1040,
        1050,
        1070,
        1080,
        1090,
        1100,
        1120,
        1130,
        1140,
        2000,
        9000,
        9990,
    ),
}


def _members(source: str) -> list[tuple[str, int, int]]:
    return [
        (name, start, end)
        for kind, name, start, end in _iter_top_level_papyrus_members(
            source.splitlines()
        )
        if kind in {"function", "event"}
    ]


def _member_body(source: str, member_name: str) -> str:
    lines = source.splitlines()
    start, end = next(
        (start, end)
        for name, start, end in _members(source)
        if name == member_name.casefold()
    )
    return "\n".join(lines[start : end + 1])


def _patch(script_name: str) -> str:
    patch = _script_patch_source(script_name)
    assert patch is not None
    return patch


def _merged(script_name: str) -> str:
    source_path = SOURCE_ROOT / _script_relative_path(script_name, ".psc")
    assert source_path.is_file(), source_path
    return _merge_script_method_patches(
        source_path.read_text(encoding="utf-8"), _patch(script_name)
    )


def _fragment_names(script_name: str) -> set[str]:
    return {
        name
        for name, _start, _end in _members(_patch(script_name))
        if name.startswith("fragment_stage_")
    }


@pytest.mark.parametrize("script_name", PATCHED_SCRIPTS)
def test_xpd_hub_fuel_patch_is_member_only_unique_and_idempotent(
    script_name: str,
) -> None:
    patch = _patch(script_name)
    assert not any(
        line.strip().casefold().startswith(("scriptname ", "state ", "auto state "))
        for line in patch.splitlines()
    )
    assert not any(
        " property " in f" {line.strip().casefold()} " for line in patch.splitlines()
    )
    assert not has_unregistered_inventory_handler(patch)
    assert not sibling_script_casts(patch)

    patch_names = [name for name, _start, _end in _members(patch)]
    assert Counter(patch_names) == Counter(set(patch_names))

    merged = _merged(script_name)
    merged_names = [name for name, _start, _end in _members(merged)]
    for member_name in patch_names:
        assert merged_names.count(member_name) == 1
        assert _member_body(merged, member_name) == _member_body(patch, member_name)
    assert _merge_script_method_patches(merged, patch) == merged


@pytest.mark.parametrize(("script_name", "stages"), BOUND_FRAGMENT_STAGES.items())
def test_xpd_hub_fuel_fragment_patch_matches_bound_stage_set(
    script_name: str, stages: tuple[int, ...]
) -> None:
    expected = {f"fragment_stage_{stage:04d}_item_00" for stage in stages}
    assert _fragment_names(script_name) == expected


def test_hub_starts_event_scoped_fuel_quests_without_skipping_dialogue() -> None:
    root = _patch(HUB_ROOT)
    init = _member_body(root, "onquestinit")
    polling = _member_body(root, "checkrequiredquests")
    start_fuels = _member_body(_patch(HUB_QF), "fragment_stage_0310_item_00")
    on_connect = _patch(HUB_ON_CONNECT)

    assert "SetStage(100)" in init
    assert "SetStage(310)" not in init
    assert "StartTimer" not in init
    assert ".Start()" not in polling
    assert start_fuels.count("SendStoryEventAndWait") == 3
    assert "0x0063BED4" in start_fuels
    assert "0x0063D5BD" in start_fuels
    assert "0x00621FB7" in start_fuels
    assert "0x0063D33F" not in start_fuels
    assert "0x0063D35F" not in start_fuels
    assert "Quest_Reborn.Start(" not in on_connect
    assert "Responders_Keyword.SendStoryEventAndWait" in on_connect


def test_hub_completion_continues_through_skippy_lennox_and_tutorial() -> None:
    patch = _patch(HUB_QF)
    stage_700 = _member_body(patch, "fragment_stage_0700_item_00")
    stage_807 = _member_body(patch, "fragment_stage_0807_item_00")
    stage_900 = _member_body(patch, "fragment_stage_0900_item_00")
    stage_1000 = _member_body(patch, "fragment_stage_1000_item_00")

    assert "SetObjectiveDisplayed(700)" in stage_700
    assert "MoveAliasToMarker(Alias_Actor_Skippy, Marker_Skippy)" in stage_700
    assert "MoveAliasToMarker(Alias_Actor_Rucker, Marker_Rucker)" in stage_700
    assert "FadeOut.Apply()" in stage_807
    assert "Fade_Spell.Cast(playerRef, playerRef)" in stage_807
    assert "SetObjectiveCompleted(800)" in stage_900
    assert "Placeholder_Tutorial.Show()" in stage_900
    assert "SetStage(1000)" in stage_900
    assert "SetStage(9000)" in stage_1000


def test_code_blue_preserves_dialogue_and_return_to_clinic() -> None:
    patch = _patch(CODE_BLUE_ROOT)
    assert "SetStage(" not in _member_body(patch, "onquestinit")

    stage_set = _member_body(patch, "onstageset")
    assert "SelectScenario()" in stage_set
    assert "SetStage(110)" in stage_set
    assert "SetObjectiveDisplayed(500)" in stage_set
    assert "SetStage(500)" not in stage_set
    assert "SetStage(9000)" in stage_set
    assert "AddSpell(" not in patch
    assert "AddItem(" not in patch


def test_mutual_aid_requires_and_consumes_the_exact_donation_once() -> None:
    patch = _patch(MUTUAL_ROOT)
    assert "SetStage(" not in _member_body(patch, "onquestinit")

    stage_set = _member_body(patch, "onstageset")
    display = _member_body(patch, "displaydonationobjectives")
    assert "SetStage(200)" in stage_set
    assert "SetStage(ChosenItem.StageToSet + 50)" in stage_set
    assert "SetObjectiveDisplayed(ChosenItem.StageToSet + 10)" in stage_set
    assert "auiStageID == 250" in stage_set
    assert "DisplayDonationObjectives()" in stage_set

    assert "selectedDonation == None || numToDonate <= 0" in display
    assert "SetObjectiveCompleted(10)" in display
    assert "SetObjectiveDisplayed(ChosenItem.StageToSet)" in display
    assert "!IsStageDone(ChosenItem.StageToSet)" in display
    assert "SetStage(ChosenItem.StageToSet)" in display
    assert display.index("SetObjectiveDisplayed(ChosenItem.StageToSet)") < display.index(
        "SetStage(ChosenItem.StageToSet)"
    )

    completion = _member_body(patch, "completedonation")
    assert "GetItemCount(selectedDonation) < numToDonate" in completion
    assert "RemoveItem(selectedDonation, numToDonate, true)" in completion
    assert completion.count("RemoveItem(") == 1
    assert "SetObjectiveCompleted(ChosenItem.StageToSet)" in completion
    assert "SetObjectiveCompleted(ChosenItem.StageToSet + 10)" in completion
    assert "!IsStageDone(9000)" in completion
    assert "SetStage(9000)" in completion
    assert completion.index("RemoveItem(selectedDonation, numToDonate, true)") < (
        completion.index("SetStage(9000)")
    )
    assert "auiStageID == CompletionStage" in stage_set
    assert "CompleteDonation()" in stage_set
    assert "RemoveItem(" not in stage_set

    fragments = _patch(MUTUAL_QF)
    assert "AttractScene.Start()" in fragments
    assert "AttractScene.Stop()" in fragments


def test_recipe_uses_local_timer_items_scenes_and_proven_convergence() -> None:
    root = _patch(RECIPE_ROOT)
    fragments = _patch(RECIPE_QF)
    assert "SetStage(" not in _member_body(root, "onquestinit")

    timer = _member_body(root, "ontimer")
    assert "IsStageDone(308)" in timer
    assert "SetStage(375)" in timer
    assert "SetStage(380)" not in timer

    stage_set = _member_body(root, "onstageset")
    assert "SetStage(370)" in stage_set
    assert "SetStage(9000)" in stage_set
    assert "SetObjectiveDisplayed(60)" in stage_set
    assert "SetObjectiveDisplayed(70)" in stage_set

    assert "AddQuestItemIfMissing(XPD_Hub_Recipe_Soup)" in fragments
    assert "AddQuestItemIfMissing(XPD_Hub_Recipe_SoupBurned)" in fragments
    assert "AddQuestItemIfMissing(XPD_Fuel_Recipe_ChefOutfit_NONPLAYABLE)" in fragments
    assert fragments.count("AdvanceToStage(450)") == 4
    assert fragments.count("AdvanceToStage(530)") == 3
    assert fragments.count("AdvanceToStage(580)") == 3
    assert "XPD_Fuel_Recipe_RewardBuff_Spell" not in fragments


def test_optional_arg_waits_for_skippy_and_completes_locally() -> None:
    root = _patch(ARG_ROOT)
    init = _member_body(root, "onquestinit")
    stage_set = _member_body(root, "onstageset")
    fragments = _patch(ARG_QF)

    assert "SetStage(" not in init
    assert "SelectPlantRegions()" not in init
    assert "SelectPhotoObjectives()" not in init
    assert "StartTimer(" not in init
    assert "auiStageID == 104" in stage_set
    assert "SelectPlantRegions()" in stage_set
    assert "SelectPhotoObjectives()" in stage_set
    assert "StartTimer(2.0, 1)" in stage_set
    assert "StartTimer(1.0, 2)" not in root
    assert "SetObjectiveDisplayed(5)" in fragments
    assert "SetObjectiveCompleted(5)" in fragments
    assert "SetObjectiveDisplayed(500)" in fragments


def test_hub_fuel_rewards_remain_pipeline_owned() -> None:
    combined = "\n".join(_patch(script_name) for script_name in PATCHED_SCRIPTS)
    assert "QuestReward" not in combined
    assert "RewardCaps" not in combined
    assert "AddSpell(" not in combined


@pytest.mark.parametrize("script_name", PATCHED_SCRIPTS)
def test_xpd_hub_fuel_full_production_merge_native_compiles_for_fo4(
    script_name: str,
) -> None:
    base_source = _fo4_base_source()
    assert base_source is not None, "FO4 base Papyrus sources unavailable"

    result = compile_psc(
        _merged(script_name),
        imports=[str(SOURCE_ROOT), str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=str(_script_relative_path(script_name, ".psc")),
    )
    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, f"{script_name}: {diagnostics}"
    assert result.pex_bytes is not None
