from __future__ import annotations

import os
from pathlib import Path

import pytest

from bacup_lib.tests.test_script_patch_conventions import (
    has_unregistered_inventory_handler,
    sibling_script_casts,
)
from bacup_lib.workflows.unified import (
    _merge_script_method_patches,
    _script_patch_source,
    _script_relative_path,
)
from creation_lib.pex.native_runtime import compile_psc


REPO_ROOT = Path(__file__).resolve().parents[5]
SOURCE_ROOT = REPO_ROOT / "mods" / "SeventySix" / "Scripts" / "Source" / "User"
PATCH_CASES = (
    "Fragments:Quests:QF_AC_MQ01_Opportunity_006C1F50",
    "Quests:AC_MQ01_Opportunity:QuestScript",
    "Quests:AC_MQ01_Opportunity:SetStageOnSit",
    "Fragments:Quests:QF_AC_MQ02_Stage_006F0128",
    "Quests:AC_MQ02_Stage:QuestScript",
    "Quests:AC_MQ02_Stage:PlayerScript",
    "Quests:AC_MQ02_Stage:ClientMusicScript",
    "Quests:AC_MQ02_Stage:EquipItemOnActivate",
    "Quests:AC_MQ02_Stage:ClownScript",
    "Quests:AC_MQ02_Stage:HighRollersDoorScript",
    "Quests:AC_MQ02_Stage:HulaHoopsScript",
    "Quests:AC_MQ02_Stage:KnifeThrowing_TestScript",
    "Quests:AC_MQ02_Stage:StartSceneOnActivate",
)


def _fo4_base_source() -> Path | None:
    candidates: list[Path] = []
    configured = os.environ.get("FO4_DIR", "").strip().strip('"')
    if configured:
        candidates.append(Path(configured))
    env_path = REPO_ROOT / ".env"
    if env_path.is_file():
        for line in env_path.read_text(encoding="utf-8").splitlines():
            if line.startswith("FO4_DIR="):
                value = line.split("=", 1)[1].strip().strip('"')
                if value:
                    candidates.append(Path(value))
                break
    for game_root in candidates:
        source_root = game_root / "Data" / "Scripts" / "Source" / "Base"
        if source_root.is_dir():
            return source_root
    return None


def _merged_source(script_name: str) -> str:
    source_path = SOURCE_ROOT / _script_relative_path(script_name, ".psc")
    patch = _script_patch_source(script_name)
    assert source_path.is_file(), source_path
    assert patch is not None
    return _merge_script_method_patches(
        source_path.read_text(encoding="utf-8"), patch
    )


def _member_body(source: str, header: str, end_keyword: str = "EndFunction") -> str:
    start = source.find(header)
    assert start != -1, f"{header!r} not found"
    end = source.find(end_keyword, start)
    assert end != -1, f"{end_keyword!r} not found after {header!r}"
    return source[start:end]


@pytest.mark.parametrize("script_name", PATCH_CASES)
def test_atlantic_city_patch_merges_idempotently_and_follows_conventions(
    script_name: str,
):
    source_path = SOURCE_ROOT / _script_relative_path(script_name, ".psc")
    skeleton = source_path.read_text(encoding="utf-8")
    patch = _script_patch_source(script_name)
    assert patch is not None
    assert not any(
        line.strip().lower().startswith(("scriptname ", "state ", "auto state "))
        for line in patch.splitlines()
    )
    assert not has_unregistered_inventory_handler(patch)
    assert not sibling_script_casts(patch)

    merged = _merge_script_method_patches(skeleton, patch)
    assert merged.lower().count("scriptname ") == 1
    assert _merge_script_method_patches(merged, patch) == merged


def test_mq01_restores_local_performances_fight_and_single_handoff():
    source = _merged_source(
        "Fragments:Quests:QF_AC_MQ01_Opportunity_006C1F50"
    )
    assert source.count("Function Fragment_Stage_") == 27
    performance = _member_body(
        source, "Function Fragment_Stage_0300_Item_00()"
    )
    assert "SetObjectiveDisplayed(30)" in performance
    assert "AC_MQ01_Opportunity_EvelynPerformance" in performance
    combat = _member_body(source, "Function Fragment_Stage_0600_Item_00()")
    assert "SetHitmenCombatState(True)" in combat
    defeat = _member_body(source, "Function TryFinishHitmanFight()")
    assert "IsStageDone(670)" in defeat
    assert "IsStageDone(680)" in defeat
    assert "IsStageDone(690)" in defeat
    captive = _member_body(source, "Function Fragment_Stage_0700_Item_00()")
    assert "leader.AddToFaction(CaptiveFaction)" in captive

    completion = _member_body(source, "Function Fragment_Stage_9000_Item_00()")
    story_event = "AC_MQ02_Stage_StartKeyword.SendStoryEvent(None, player, player)"
    assert completion.count(story_event) == 1
    assert "AC_MQ02_Stage.Start(" not in completion
    assert "AddItem(" not in completion
    assert "Caps001" not in completion


def test_mq01_seat_alias_is_prerequisite_and_shutoff_guarded():
    source = _merged_source("Quests:AC_MQ01_Opportunity:SetStageOnSit")
    gate = _member_body(source, "Bool Function CanAdvance()")
    assert "!owner.IsStageDone(PrereqStage)" in gate
    assert "owner.IsStageDone(ShutoffStage)" in gate
    event = _member_body(source, "Event OnSit(", "EndEvent")
    assert "CanAdvance()" in event
    assert "SetStage(StageToSet)" in event


def test_mq02_starts_hub_prerequisite_only_through_bound_story_keyword():
    source = _merged_source("Fragments:Quests:QF_AC_MQ02_Stage_006F0128")
    stage_100 = _member_body(source, "Function Fragment_Stage_0100_Item_00()")
    assert "XPD_Hub_Responders.IsCompleted()" in stage_100
    assert "player.HasKeyword(XPD_Hub_Responders_QuestActiveKeyword)" in stage_100
    story_event = (
        "XPD_Hub_Responders_QuestStartKeyword.SendStoryEvent(None, player, player)"
    )
    assert stage_100.count(story_event) == 1
    assert "XPD_Hub_Responders.Start(" not in stage_100


def test_mq02_restores_costume_show_knife_and_return_checkpoints():
    source = _merged_source("Fragments:Quests:QF_AC_MQ02_Stage_006F0128")
    assert source.count("Function Fragment_Stage_") == 76
    dressing_room = _member_body(
        source, "Function Fragment_Stage_1300_Item_00()"
    )
    assert "SetObjectiveDisplayed(111)" in dressing_room
    assert "SetObjectiveDisplayed(112)" in dressing_room
    assert "EnableAlias(Alias_EnableMarker_ClownDressingRoom)" in dressing_room
    show = _member_body(source, "Function Fragment_Stage_1600_Item_00()")
    assert "EnableCollection(Alias_Actors_PerformanceClowns)" in show
    assert "AC_MQ02_Stage_ClownIntroduction" in show
    knife = _member_body(source, "Function Fragment_Stage_2260_Item_00()")
    assert "controller.PrepareKnifeThrowing()" in knife
    boardwalk = _member_body(source, "Function Fragment_Stage_2700_Item_00()")
    assert "SetObjectiveDisplayed(210)" in boardwalk
    assert "AC_MQ02_Stage_StanToCasinoQuarterScene" in boardwalk
    rose_room = _member_body(source, "Function Fragment_Stage_2850_Item_00()")
    assert "SetObjectiveDisplayed(220)" in rose_room
    assert "AC_MQ02_Stage_EvelynAbbieAmbient" in rose_room


def test_mq02_completion_hands_off_once_without_duplicate_reward_logic():
    source = _merged_source("Fragments:Quests:QF_AC_MQ02_Stage_006F0128")
    completion = _member_body(source, "Function Fragment_Stage_9000_Item_00()")
    story_event = (
        "AC_MQ03_HonorBound_StartKeyword.SendStoryEvent(None, player, player)"
    )
    assert completion.count(story_event) == 1
    assert "AC_MQ03_HonorBound.Start(" not in completion
    assert "AddItem(" not in completion
    assert "Caps001" not in completion
    assert "SetStage(9100)" in completion
    shutdown = _member_body(source, "Function Fragment_Stage_9100_Item_00()")
    assert shutdown.count("Stop()") == 1


def test_mq02_alias_minigames_have_local_player_and_stage_guards():
    player = _merged_source("Quests:AC_MQ02_Stage:PlayerScript")
    assert "AllRequiredEquipmentEquipped()" in player
    assert "SetObjectiveCompleted(required.Obj_EquipItem" in player
    assert "SetStage(Stage_ClownImpersonationStarted)" in player

    clown = _merged_source("Quests:AC_MQ02_Stage:ClownScript")
    hit = _member_body(clown, "Event OnHit(", "EndEvent")
    assert "akAggressor != Game.GetPlayer()" in hit
    assert "akSource != Weap_ThrowingPie" in hit
    assert "SetStage(Stage_PieThrowingCompleted)" in hit

    hoops = _merged_source("Quests:AC_MQ02_Stage:HulaHoopsScript")
    trigger = _member_body(hoops, "Event OnTriggerEnter(", "EndEvent")
    assert "GetBaseObject() != AC_MQ02_Stage_JugglingGrenade_Projectile" in trigger
    assert "SetStage(Stage_GrenadeJugglingCompleted)" in trigger


def test_mq02_activate_helpers_restore_equipment_door_and_scene_contracts():
    equipment = _merged_source("Quests:AC_MQ02_Stage:EquipItemOnActivate")
    activate = _member_body(equipment, "Event OnActivate(", "EndEvent")
    assert "player != Game.GetPlayer()" in activate
    assert "player.AddItem(WeaponToEquip" in activate
    assert "player.EquipItem(WeaponToEquip" in activate

    door = _merged_source("Quests:AC_MQ02_Stage:HighRollersDoorScript")
    door_activate = _member_body(door, "Event OnActivate(", "EndEvent")
    assert "IsStageDone(Stage_PassedCharismaCheck)" in door_activate
    assert "SetStage(Stage_BrokeInMainDoor)" in door_activate
    assert "doorRef.Activate(akActionRef, True)" in door_activate

    scene = _merged_source("Quests:AC_MQ02_Stage:StartSceneOnActivate")
    scene_activate = _member_body(scene, "Event OnActivate(", "EndEvent")
    assert "akActionRef != ActivatedByAlias.GetReference()" in scene_activate
    assert "!OwningQuest.IsStageDone(Stage_Prereq)" in scene_activate
    assert "OwningQuest.IsStageDone(Stage_TurnOff)" in scene_activate
    assert "SceneToStart.Start()" in scene_activate


@pytest.mark.parametrize(
    "script_name",
    (
        "Fragments:Packages:PF_AC_MQ01_Opportunity_Abbie_006C2DAF",
        "Fragments:Packages:PF_AC_MQ02_Stage_Abbie_Trave_00769756",
        "Fragments:Packages:PF_AC_MQ02_Stage_StayAtSelfS_0076AAD2",
    ),
)
def test_begin_only_package_shells_remain_unpatched_without_behavior_evidence(
    script_name: str,
):
    assert _script_patch_source(script_name) is None


@pytest.mark.parametrize("script_name", PATCH_CASES)
def test_atlantic_city_merged_patch_native_compiles_for_fo4(script_name: str):
    base_source = _fo4_base_source()
    if base_source is None:
        pytest.skip("FO4 base Papyrus sources unavailable")

    result = compile_psc(
        _merged_source(script_name),
        imports=[str(base_source), str(SOURCE_ROOT)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=str(_script_relative_path(script_name, ".psc")),
    )
    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None
