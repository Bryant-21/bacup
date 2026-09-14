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

MASTER = "Quests:U01A_Brewing:MasterScript"
DAILY = "Quests:U01A_Brewing:DailyScript"
TIPSY_PLAYER = "Quests:U01A_Brewing:TipsyPlayerScript"
WASTED_PLAYER = "Quests:U01A_Brewing:WastedPlayerScript"
TIPSY_FRAGMENT = "Fragments:Quests:QF_D01A_Tipsy_0047CF15"
WASTED_FRAGMENT = "Fragments:Quests:QF_D01A_Wasted_0047CF16"
TOPIC_FRAGMENTS = (
    "Fragments:TopicInfos:TIF_P01A_Robobrain_DialogueQ_003EC1A9",
    "Fragments:TopicInfos:TIF_P01A_Robobrain_DialogueQ_003EC1AA",
    "Fragments:TopicInfos:TIF_P01A_Robobrain_DialogueQ_003EC1AB",
    "Fragments:TopicInfos:TIF_P01A_Robobrain_DialogueQ_003EC1AC",
    "Fragments:TopicInfos:TIF_P01A_Robobrain_DialogueQ_003F04C7",
    "Fragments:TopicInfos:TIF_P01A_Robobrain_DialogueQ_003F04C8",
    "Fragments:TopicInfos:TIF_P01A_Robobrain_DialogueQ_003F04C9",
    "Fragments:TopicInfos:TIF_P01A_Robobrain_DialogueQ_003F04CA",
    "Fragments:TopicInfos:TIF_P01A_Robobrain_DialogueQ_0047CF19",
)

PATCH_MEMBERS = {
    MASTER: {"chooseandstartdailyquest"},
    DAILY: {
        "onquestinit",
        "hasavailablewastedrecipe",
        "initializewastedquest",
        "initializetipsyquest",
        "begintipsytest",
        "completetipsytest",
        "playerhasreadyalcohol",
        "completedailyquest",
    },
    TIPSY_PLAYER: {
        "onaliasinit",
        "refreshselectedstat",
        "hasrequiredalcoholeffect",
        "hasdrunkrequiredalcohol",
        "revealselectedtest",
        "completeselectedtest",
        "actor.onitemequipped",
        "actor.onkill",
        "actor.onplayeruseworkbench",
        "onitemadded",
    },
    WASTED_PLAYER: {
        "onaliasinit",
        "refreshobjectives",
        "onitemadded",
        "onitemremoved",
    },
    TIPSY_FRAGMENT: {
        "fragment_stage_0100_item_00",
        "fragment_stage_0200_item_00",
        "fragment_stage_0300_item_00",
        "fragment_stage_0400_item_00",
        "fragment_stage_0500_item_00",
        "fragment_stage_0600_item_00",
        "fragment_stage_0700_item_00",
        "fragment_stage_0800_item_00",
        "fragment_stage_8000_item_00",
        "fragment_stage_9000_item_00",
    },
    WASTED_FRAGMENT: {
        "fragment_stage_0100_item_00",
        "fragment_stage_0400_item_00",
        "fragment_stage_9000_item_00",
    },
    **{script_name: {"fragment_end"} for script_name in TOPIC_FRAGMENTS},
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
def test_biv_patch_is_member_only_and_complete(
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
def test_biv_patch_merges_once(script_name: str, members: set[str]):
    patch = _script_patch_source(script_name)
    assert patch is not None
    merged = _merged_production_source(script_name)
    names = _member_names(merged)

    for member in members:
        assert names.count(member) == 1
    assert _merge_script_method_patches(merged, patch) == merged


def test_biv_full_production_sources_native_compile(tmp_path: Path):
    merged_sources = {
        script_name: _merged_production_source(script_name)
        for script_name in PATCH_MEMBERS
    }
    merged_source_root = tmp_path / "Scripts" / "Source" / "User"
    for script_name, merged in merged_sources.items():
        source_path = merged_source_root / _script_relative_path(script_name, ".psc")
        source_path.parent.mkdir(parents=True, exist_ok=True)
        source_path.write_text(merged, encoding="utf-8")

    base_source = _fo4_base_source()
    assert base_source is not None, "FO4 base Papyrus sources unavailable"

    for script_name, merged in merged_sources.items():
        result = compile_psc(
            merged,
            imports=[
                str(merged_source_root),
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


def test_biv_master_uses_story_manager_and_enforces_daily_selection():
    master = _script_patch_source(MASTER)
    assert master is not None

    choose = _member_body(master, "chooseandstartdailyquest")
    assert "P01A_Nukashine.IsCompleted()" in choose
    assert "Utility.GetCurrentGameTime() - lastDailyTime < 1.0" in choose
    assert "D01A_Tipsy.IsRunning() || D01A_Wasted.IsRunning()" in choose
    assert "tipsyStreak >= NumConsecutiveTipsyAllowed" in choose
    assert "HasAvailableWastedRecipe(playerRef)" in choose
    assert "D01A_Wasted_StartKeyword.SendStoryEventAndWait" in choose
    assert "D01A_Tipsy_StartKeyword.SendStoryEventAndWait" in choose
    assert ".Start(" not in choose


def test_biv_wasted_sequence_does_not_get_overwritten_by_tipsy_selection():
    daily = _script_patch_source(DAILY)
    assert daily is not None

    tipsy = _member_body(daily, "initializetipsyquest")
    finish = _member_body(daily, "completedailyquest")
    availability = _member_body(daily, "hasavailablewastedrecipe")
    assert "SetValue(D01A_ChosenAlcohol" not in tipsy
    assert "GetValue(D01A_ChosenAlcohol)" in availability
    assert "nextRecipeIndex" in finish
    assert "SetValue(D01A_ChosenAlcohol, nextRecipeIndex as Float)" in finish


def test_biv_alias_scripts_cover_the_local_fo4_gameplay_events():
    daily = _script_patch_source(DAILY)
    tipsy = _script_patch_source(TIPSY_PLAYER)
    wasted = _script_patch_source(WASTED_PLAYER)
    assert daily is not None
    assert tipsy is not None
    assert wasted is not None

    tipsy_init = _member_body(daily, "initializetipsyquest")
    wasted_init = _member_body(daily, "initializewastedquest")
    tipsy_alias_init = _member_body(tipsy, "onaliasinit")
    tipsy_refresh = _member_body(tipsy, "refreshselectedstat")
    wasted_refresh = _member_body(wasted, "refreshobjectives")
    assert "playerScript.RefreshSelectedStat()" in tipsy_init
    assert "RefreshSelectedStat()" in tipsy_alias_init
    assert "QS.ChosenStatIndex" in tipsy_refresh
    assert "playerScript.RefreshObjectives()" in wasted_init
    assert "QS = OwningQuest as Quests:U01A_Brewing:DailyScript" in wasted_refresh
    assert "AddInventoryEventFilter(None)" in tipsy
    assert "Actor.OnItemEquipped" in tipsy
    assert "Actor.OnKill" in tipsy
    assert "Actor.OnPlayerUseWorkBench" in tipsy
    assert "HasMagicEffect(QS.ChosenAlcohol.DurationEffect)" in tipsy
    assert "AddInventoryEventFilter(None)" in wasted
    assert "QS.ChosenAlcohol.FermDrink" in wasted
    assert "QS.PlayerHasReadyAlcohol()" in wasted
    assert "QS.HasAlcohol = True" in wasted


def test_biv_completion_fragments_timestamp_stop_and_advance_once():
    tipsy = _script_patch_source(TIPSY_FRAGMENT)
    wasted = _script_patch_source(WASTED_FRAGMENT)
    assert tipsy is not None
    assert wasted is not None

    tipsy_finish = _member_body(tipsy, "fragment_stage_9000_item_00")
    wasted_finish = _member_body(wasted, "fragment_stage_9000_item_00")
    assert "CompleteDailyQuest()" in tipsy_finish
    assert "CompleteDailyQuest(True)" in wasted_finish
    assert "Stop()" in tipsy_finish
    assert "Stop()" in wasted_finish


def test_only_live_bound_biv_greetings_trigger_the_master():
    for script_name in TOPIC_FRAGMENTS:
        patch = _script_patch_source(script_name)
        assert patch is not None
        assert "ChooseAndStartDailyQuest()" in patch

    unbound_greeting = (
        "Fragments:TopicInfos:TIF_P01A_Robobrain_DialogueQ_003F04C6"
    )
    assert _script_patch_source(unbound_greeting) is None
