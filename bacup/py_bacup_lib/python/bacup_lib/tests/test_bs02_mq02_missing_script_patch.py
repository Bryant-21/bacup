from __future__ import annotations

from pathlib import Path

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
SCRIPT_NAME = "Fragments:Quests:QF_BS02_MQ02_Missing_005F56C4_1"
PATCH_MEMBERS = {
    f"fragment_stage_{stage:04d}_item_00"
    for stage in (
        30,
        40,
        100,
        200,
        300,
        400,
        550,
        600,
        650,
        675,
        700,
        750,
        755,
        760,
        770,
        775,
        800,
        850,
        900,
        950,
        1000,
        1025,
        1050,
        1100,
        1175,
        1180,
        1183,
        1185,
        1190,
        1195,
        1198,
        1200,
        1250,
        1300,
        1400,
        1425,
        1450,
        1500,
        1550,
        1560,
        1570,
        1575,
        1600,
        1650,
        1675,
        1700,
        1750,
        1775,
        1800,
        1850,
        1875,
        1900,
        9000,
    )
}
PATCH_MEMBER_NAMES = PATCH_MEMBERS | {"ontimer"}
UNBOUND_PROPERTY_NAMES = {
    "Alias_RefColl_BasementActors",
    "Alias_Actor_Marcia_FortAtlas",
    "Alias_Actor_Sheena_WarRoom",
    "Alias_Actor_Burke_WarRoom",
    "Alias_Actor_CoweringMercenary",
    "Alias_EnableMarker_AMSRobots",
    "Alias_EnableMarker_Mercenaries",
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
        if name == member_name.casefold()
    )
    return "\n".join(lines[start : end + 1])


def _patch_source() -> str:
    patch = _script_patch_source(SCRIPT_NAME)
    assert patch is not None
    return patch


def _merged_source() -> str:
    source_path = SOURCE_ROOT / _script_relative_path(SCRIPT_NAME, ".psc")
    assert source_path.is_file(), source_path
    return _merge_script_method_patches(
        source_path.read_text(encoding="utf-8"), _patch_source()
    )


def test_patch_defines_each_bound_fragment_exactly_once() -> None:
    patch = _patch_source()
    member_names = _member_names(patch)

    assert len(PATCH_MEMBERS) == 53
    assert set(member_names) == PATCH_MEMBER_NAMES
    assert all(member_names.count(member) == 1 for member in PATCH_MEMBER_NAMES)
    assert not any(
        line.strip().casefold().startswith(("scriptname ", "state "))
        for line in patch.splitlines()
    )
    assert not any(
        " property " in f" {line.strip().casefold()} " for line in patch.splitlines()
    )
    assert not any(name in patch for name in UNBOUND_PROPERTY_NAMES)


def test_production_merge_is_unique_idempotent_and_compiles() -> None:
    patch = _patch_source()
    merged = _merged_source()
    merged_members = _member_names(merged)

    for member in PATCH_MEMBER_NAMES:
        assert merged_members.count(member) == 1
        assert _member_body(merged, member) == _member_body(patch, member)
    assert _merge_script_method_patches(merged, patch) == merged

    base_source = _fo4_base_source()
    assert base_source is not None, "FO4 base Papyrus sources unavailable"
    result = compile_psc(
        merged,
        imports=[str(SOURCE_ROOT), str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=str(_script_relative_path(SCRIPT_NAME, ".psc")),
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None


def test_objectives_items_and_scenes_follow_the_bound_stage_contract() -> None:
    patch = _patch_source()

    stage_100 = _member_body(patch, "fragment_stage_0100_item_00")
    assert "SetObjectiveDisplayed(10, True)" in stage_100
    assert "Alias_Player.TryToSetValue(BS02_MarciaAwayValue, 1.0)" in stage_100

    stage_800 = _member_body(patch, "fragment_stage_0800_item_00")
    assert "GetItemCount(BS02_MQ02_Missing_BurkesNecklace) < 1" in stage_800
    assert stage_800.count("AddItem(BS02_MQ02_Missing_BurkesNecklace") == 1
    assert "z_BS02_MQ02_Missing_Marcia02.Start()" in stage_800

    stage_1000 = _member_body(patch, "fragment_stage_1000_item_00")
    assert "GetItemCount(BS02_MQ02_Missing_TerminalPassword) < 1" in stage_1000
    assert stage_1000.count("AddItem(BS02_MQ02_Missing_TerminalPassword") == 1
    assert "z_BS02_MQ02_Missing_Marcia04.Start()" in stage_1000

    stage_1250 = _member_body(patch, "fragment_stage_1250_item_00")
    assert "GetItemCount(BS02_MQ02_Missing_BlackburnsLetter) < 1" in stage_1250
    assert stage_1250.count("AddItem(BS02_MQ02_Missing_BlackburnsLetter") == 1

    stage_1400 = _member_body(patch, "fragment_stage_1400_item_00")
    assert "GetItemCount(BS02_MQ02_Missing_BasementKey) < 1" in stage_1400
    assert stage_1400.count("AddItem(BS02_MQ02_Missing_BasementKey") == 1

    stage_1675 = _member_body(patch, "fragment_stage_1675_item_00")
    assert "GetItemCount(BS02_MQ02_Missing_ResearchNotes) < 1" in stage_1675
    assert stage_1675.count("AddItem(BS02_MQ02_Missing_ResearchNotes") == 1


def test_local_encounters_replace_online_wave_spawning_without_guessed_forms() -> None:
    patch = _patch_source()

    first_wave = _member_body(patch, "fragment_stage_1183_item_00")
    assert "Alias_RefColl_AMS02_Actors.GetCount()" in first_wave
    assert "Alias_RefColl_AMS02_Actors.GetAt(index)" in first_wave
    assert first_wave.count("EnableNoWait()") == 1

    second_wave = _member_body(patch, "fragment_stage_1190_item_00")
    assert "Alias_RefColl_AMS02_Robots.GetCount()" in second_wave
    assert "Alias_RefColl_AMS02_Robots.GetAt(index)" in second_wave
    assert second_wave.count("EnableNoWait()") == 1

    combat = _member_body(patch, "fragment_stage_1198_item_00")
    assert "kitRef.StartCombat(playerRef)" in combat
    assert "Alias_Player.GetActorReference()" in combat

    basement = _member_body(patch, "fragment_stage_1450_item_00")
    assert "Alias_Actors_MercenaryGuards.GetCount()" in basement
    assert "Alias_Actors_MercenaryGuards.GetAt(index)" in basement

    assert "DefaultQuestEncounterWaveScript" not in patch
    assert "Game.GetFormFromFile" not in patch
    assert "Game.GetPlayer" not in patch


def test_rescue_and_leave_raiders_remain_distinct_branches() -> None:
    patch = _patch_source()

    agree_to_rescue = _member_body(patch, "fragment_stage_1600_item_00")
    assert "SetObjectiveDisplayed(156, False)" in agree_to_rescue
    assert "SetObjectiveDisplayed(158, True)" in agree_to_rescue
    assert "SetObjectiveCompleted(155, True)" not in agree_to_rescue
    assert "SetOpen(True)" not in agree_to_rescue

    open_cells = _member_body(patch, "fragment_stage_1700_item_00")
    assert open_cells.count("SetOpen(True)") == 2
    assert "Alias_Door_SheenasCell.GetRef()" in open_cells
    assert "Alias_Door_BurkesCell.GetRef()" in open_cells
    assert "Alias_Actor_Sheena.GetActorReference()" in open_cells
    assert "GetAlias(12) as ReferenceAlias" in open_cells
    assert "burkeAlias.GetActorReference()" in open_cells
    assert "If burkeRef != None" in open_cells
    assert open_cells.count("RemoveFromFaction(BoundCaptiveFaction)") == 2
    assert open_cells.count("EvaluatePackage()") == 2

    rescued = _member_body(patch, "fragment_stage_1750_item_00")
    assert "SetObjectiveCompleted(155, True)" in rescued
    assert "SetObjectiveCompleted(158, True)" in rescued
    assert "TryToSetValue(BS02_SheenaAwayValue, 0.0)" in rescued
    assert "TryToSetValue(BS02_BurkeAwayValue, 0.0)" in rescued

    left = _member_body(patch, "fragment_stage_1775_item_00")
    assert "SetObjectiveCompleted(156, True)" in left
    assert "SetOpen(True)" not in left
    assert "RemoveFromFaction" not in left
    assert "BS02_SheenaAwayValue" not in left
    assert "BS02_BurkeAwayValue" not in left


def test_marcia_stay_and_return_outcomes_do_not_override_dialogue_gate() -> None:
    patch = _patch_source()

    stays = _member_body(patch, "fragment_stage_1850_item_00")
    assert "SetObjectiveCompleted(165, True)" in stays
    assert "TryToSetValue(BS02_MarciaAwayValue, 1.0)" in stays
    assert "Alias_Actor_Marcia_WarRoom.TryToEnableNoWait()" in stays
    assert "Alias_Actor_Marcia_FortAtlas" not in stays

    returns = _member_body(patch, "fragment_stage_1875_item_00")
    assert "SetObjectiveCompleted(160, True)" in returns
    assert "TryToSetValue(BS02_MarciaAwayValue, 0.0)" in returns
    assert "TryToSetValue(BS02_MarciaWarRoomValue, 0.0)" in returns
    assert "Alias_Actor_Marcia_AMSBasement.TryToDisableNoWait()" in returns
    assert "Alias_Actor_Marcia_WarRoom.TryToEnableNoWait()" not in returns

    assert "fragment_stage_1860_item_00" not in patch.casefold()
    assert "BS01_MQ01_Trust" not in patch
    assert "SetStage(1860)" not in patch


def test_completion_hands_off_to_out_of_the_blue_through_story_manager_once() -> None:
    patch = _patch_source()
    terminal = _member_body(patch, "fragment_stage_1175_item_00")
    completion = _member_body(patch, "fragment_stage_9000_item_00")

    assert "SetObjectiveCompleted(80, True)" in terminal
    assert "Alias_Door_TopFloor.GetRef()" in terminal
    assert "topFloorDoor.Lock(False)" in terminal
    assert "topFloorDoor.SetOpen(True)" in terminal

    assert completion.count("CompleteQuest()") == 1
    assert completion.count(
        "BS02_MQ03_Blue_QuestStartKeyword.SendStoryEventAndWait(None, playerRef, playerRef)"
    ) == 1
    assert "Bool storyAccepted =" in completion
    assert "If storyAccepted" in completion
    assert "BS02_MQ03_Blue != None" in completion
    assert "!BS02_MQ03_Blue.IsRunning()" in completion
    assert "!BS02_MQ03_Blue.IsCompleted()" in completion
    assert "BS02_MQ03_Blue_QuestStartKeyword != None" in completion
    assert completion.index("CompleteQuest()") < completion.index(
        "SendStoryEventAndWait("
    )
    assert completion.index("If storyAccepted") < completion.index(
        "StartTimer(5.0, 9000)"
    )
    assert completion.count("StartTimer(5.0, 9000)") == 1
    assert "SendStoryEvent(" not in completion
    assert "BS02_MQ03_Blue.Start()" not in completion
    assert "Stop()" not in completion
    assert "CompleteAllObjectives()" not in completion


def test_rejected_story_handoff_retries_until_blue_accepts_or_starts() -> None:
    timer = _member_body(_patch_source(), "ontimer")

    assert "Event OnTimer(Int aiTimerID)" in timer
    assert "aiTimerID == 9000" in timer
    assert "IsStageDone(9000)" in timer
    assert "Alias_Player.GetActorReference()" in timer
    assert timer.count(
        "BS02_MQ03_Blue_QuestStartKeyword.SendStoryEventAndWait(None, playerRef, playerRef)"
    ) == 1
    assert "Bool storyAccepted =" in timer
    assert "If storyAccepted" in timer
    assert timer.index("If storyAccepted") < timer.index("StartTimer(5.0, 9000)")
    assert timer.count("!BS02_MQ03_Blue.IsRunning()") == 2
    assert timer.count("!BS02_MQ03_Blue.IsCompleted()") == 2
    assert timer.count("StartTimer(5.0, 9000)") == 1
    assert "SendStoryEvent(" not in timer
    assert "BS02_MQ03_Blue.Start()" not in timer
    assert "Stop()" not in timer
