from __future__ import annotations

import os
from pathlib import Path

import pytest

from bacup_lib.workflows.unified import (
    _merge_script_method_patches,
    _script_patch_source,
    _script_relative_path,
)
from creation_lib.pex.native_runtime import compile_psc


REPO_ROOT = Path(__file__).resolve().parents[5]
SOURCE_ROOT = REPO_ROOT / "mods" / "SeventySix" / "Scripts" / "Source" / "User"

PATCH_CASES = (
    "Fragments:Quests:QF_Storm_MQ04_HugoPt1_0072EE0A",
    "Fragments:Quests:QF_Storm_MQ05_HugoPt2_006F8B09",
    "Fragments:Quests:QF_Storm_MQ06_HugoPt3_0073D7C1",
    "Quests:Storm:MQ06:PlayerAliasScript",
    "Quests:Storm:MQ06:StartSceneOnTriggerEnterScript",
    "Fragments:Scenes:SF_Storm_MQ06_HugoPt3_AlexSc_0075DD75",
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
def test_patch_merges_into_production_skeleton_idempotently(script_name: str):
    source_path = SOURCE_ROOT / _script_relative_path(script_name, ".psc")
    skeleton = source_path.read_text(encoding="utf-8")
    patch = _script_patch_source(script_name)
    assert patch is not None
    assert not any(
        line.strip().lower().startswith("scriptname ") for line in patch.splitlines()
    )

    merged = _merge_script_method_patches(skeleton, patch)
    assert merged.lower().count("scriptname ") == 1
    assert _merge_script_method_patches(merged, patch) == merged


def test_mq04_contract_restores_objectives_evidence_flags_and_handoff():
    source = _merged_source("Fragments:Quests:QF_Storm_MQ04_HugoPt1_0072EE0A")
    assert "SetObjectiveDisplayed(10)" in source
    assert "SetObjectiveCompleted(80)" in source
    assert "SetValue(AV_HitmanLetter, 1.0)" in source
    assert "SetValue(AV_LetterFromAlex, 1.0)" in source
    completion = _member_body(source, "Function Fragment_Stage_9000_Item_00()")
    assert (
        "Storm_MQ05_HugoPt2_StartKeyword.SendStoryEvent(None, PlayerReference(), PlayerReference())"
        in completion
    )


def test_mq04_stage_305_opens_every_bound_pump_access_door():
    source = _merged_source("Fragments:Quests:QF_Storm_MQ04_HugoPt1_0072EE0A")
    helper = _member_body(source, "Function OpenPumpAccessDoor(")
    assert "accessDoor.BlockActivation(False)" in helper
    assert "accessDoor.RemoveKeyword(BlockPlayerActivation)" in helper
    assert "accessDoor.Lock(False)" in helper
    stage_305 = _member_body(source, "Function Fragment_Stage_0305_Item_00()")
    assert "Alias_PumpAccessDoor2.GetReference()" in stage_305
    assert "Alias_PumpAccessDoor as ReferenceAlias" in stage_305


def test_mq05_contract_restores_key_scene_panel_and_handoff():
    source = _merged_source("Fragments:Quests:QF_Storm_MQ05_HugoPt2_006F8B09")
    stage_150 = _member_body(source, "Function Fragment_Stage_0150_Item_00()")
    assert "GiveIfMissing(VisitorCenterKey)" in stage_150
    stage_507 = _member_body(source, "Function Fragment_Stage_0507_Item_00()")
    assert "KevinArrivedAtBunkerScene.Start()" in stage_507
    completion = _member_body(source, "Function Fragment_Stage_9000_Item_00()")
    assert "Alias_Static_ElectricSparks.GetReference().Disable()" in completion
    assert "Alias_BrokenPanel.GetReference().Disable()" in completion
    assert "Alias_RepairedPanel.GetReference().Enable()" in completion
    assert "Storm_MQ06_HugoPt3_StartKeyword.SendStoryEvent(" in completion


def test_mq05_stage_508_swaps_kevin_instances_and_advances_walk_package():
    source = _merged_source("Fragments:Quests:QF_Storm_MQ05_HugoPt2_006F8B09")
    stage_508 = _member_body(source, "Function Fragment_Stage_0508_Item_00()")
    assert "Alias_Actor_Kevin.GetActorReference()" in stage_508
    assert "Alias_Actor_Kevin_Bunker.GetActorReference()" in stage_508
    assert "Alias_Marker_BunkerIntKevinTeleport.GetReference()" in stage_508
    assert stage_508.index("visitorKevin.Disable()") < stage_508.index(
        "bunkerKevin.Enable()"
    )
    assert stage_508.index("bunkerKevin.Enable()") < stage_508.index(
        "bunkerKevin.MoveTo(bunkerMarker)"
    )
    assert stage_508.index("bunkerKevin.MoveTo(bunkerMarker)") < stage_508.index(
        "bunkerKevin.EvaluatePackage()"
    )
    assert "If !IsStageDone(509)" in stage_508
    assert "SetStage(509)" in stage_508
    assert "SetStage(510)" not in stage_508


def test_mq05_completion_sends_one_handoff_then_runs_shutdown_lifecycle():
    source = _merged_source("Fragments:Quests:QF_Storm_MQ05_HugoPt2_006F8B09")
    completion = _member_body(source, "Function Fragment_Stage_9000_Item_00()")
    handoff = "Storm_MQ06_HugoPt3_StartKeyword.SendStoryEvent("
    assert completion.count(handoff) == 1
    assert completion.index(handoff) < completion.index("SetStage(9999)")
    shutdown = _member_body(source, "Function Fragment_Stage_9999_Item_00()")
    assert shutdown.count("Stop()") == 1


def test_mq06_contract_restores_choices_combat_completion_and_finale_gate():
    source = _merged_source("Fragments:Quests:QF_Storm_MQ06_HugoPt3_0073D7C1")
    assert "SetValue(Storm_MQ06_AlexChoice, 1.0)" in source
    assert "SetValue(Storm_MQ06_AlexChoice, 2.0)" in source
    combat = _member_body(source, "Function Fragment_Stage_1100_Item_00()")
    assert "EndDisguise()" in combat
    assert "MakeCultistsHostile()" in combat
    completion = _member_body(source, "Function Fragment_Stage_9000_Item_00()")
    tracker_read = (
        "Float trackerValue = player.GetValue(Storm_MQ00_QuestProgressionTracker)"
    )
    tracker_write = "player.SetValue(Storm_MQ00_QuestProgressionTracker, trackerValue)"
    story_event = "Storm_MQ13_Finale_StartKeyword.SendStoryEvent(None, player, player)"
    assert completion.count(tracker_read) == 1
    assert completion.count("If trackerValue < 2.0") == 1
    assert completion.count("trackerValue += 1.0") == 1
    assert completion.count("If trackerValue > 2.0") == 1
    assert completion.count("trackerValue = 2.0") == 1
    assert completion.count(tracker_write) == 1
    assert completion.count("If trackerValue >= 2.0") == 1
    assert completion.count("startFinale = True") == 1
    assert completion.count("If startFinale") == 1
    assert completion.count(story_event) == 1
    assert completion.index(tracker_read) < completion.index("If trackerValue < 2.0")
    assert completion.index("trackerValue += 1.0") < completion.index(tracker_write)
    assert completion.index(tracker_write) < completion.index("If trackerValue >= 2.0")
    assert completion.index("If trackerValue >= 2.0") < completion.index(
        "startFinale = True"
    )
    assert completion.index(
        "SetValue(Storm_MQ_HugoAwayValue, 0.0)"
    ) < completion.index("If startFinale")
    assert completion.index("If startFinale") < completion.index(story_event)
    assert "ModValue(Storm_MQ00_QuestProgressionTracker" not in completion


def test_mq06_instance_reentry_restores_unfinished_cultist_combat():
    source = _merged_source("Fragments:Quests:QF_Storm_MQ06_HugoPt3_0073D7C1")
    stage_10 = _member_body(source, "Function Fragment_Stage_0010_Item_00()")
    assert "IsStageDone(1100) && !IsStageDone(1110)" in stage_10
    assert "EndDisguise()" in stage_10
    assert "MakeCultistsHostile()" in stage_10
    assert "SetObjectiveDisplayed(70)" in stage_10


def test_mq06_conversation_actors_are_restored_before_objective_display():
    source = _merged_source("Fragments:Quests:QF_Storm_MQ06_HugoPt3_0073D7C1")
    stage_90 = _member_body(source, "Function Fragment_Stage_0090_Item_00()")
    assert "SetObjectiveDisplayed(10)" not in stage_90
    stage_100 = _member_body(source, "Function Fragment_Stage_0100_Item_00()")
    assert "Alias_Actor_Audrey.GetActorReference()" in stage_100
    assert "Alias_Ref_AudreyTeleport_Stage50.GetReference()" in stage_100
    assert stage_100.index("audrey.MoveTo(destination)") < stage_100.index(
        "audrey.EvaluatePackage()"
    )
    assert stage_100.index("audrey.EvaluatePackage()") < stage_100.index(
        "SetObjectiveDisplayed(10)"
    )
    stage_110 = _member_body(source, "Function Fragment_Stage_0110_Item_00()")
    assert "Alias_Actor_Hugo_WeatherLab.GetActorReference()" in stage_110
    assert stage_110.index("hugo.Enable()") < stage_110.index("hugo.MoveTo(audrey)")
    assert stage_110.index("hugo.MoveTo(audrey)") < stage_110.index(
        "hugo.EvaluatePackage()"
    )


def test_mq06_alex_choice_waits_for_bleedout_or_death_before_combat():
    source = _merged_source("Fragments:Quests:QF_Storm_MQ06_HugoPt3_0073D7C1")
    stage_1010 = _member_body(source, "Function Fragment_Stage_1010_Item_00()")
    assert "SetValue(Storm_MQ06_AlexChoice, 1.0)" in stage_1010
    assert "SetStage(1100)" not in stage_1010
    stage_1040 = _member_body(source, "Function Fragment_Stage_1040_Item_00()")
    assert "Alias_Actor_Alex.GetActorReference()" in stage_1040
    assert "alex.StopCombat()" in stage_1040
    assert "SetStage(1050)" in stage_1040
    stage_1050 = _member_body(source, "Function Fragment_Stage_1050_Item_00()")
    assert "SetStage(1100)" in stage_1050


def test_mq06_disguise_alias_tracks_equip_and_unequip_locally():
    source = _merged_source("Quests:Storm:MQ06:PlayerAliasScript")
    equipped = _member_body(source, "Event OnItemEquipped(", "EndEvent")
    unequipped = _member_body(source, "Event OnItemUnequipped(", "EndEvent")
    assert "item.HasKeyword(DisguiseKeyword)" in source
    assert "player.AddToFaction(DisguiseFaction)" in equipped
    assert "owner.SetStage(iOnEquip_SetStage)" in equipped
    assert "player.RemoveFromFaction(DisguiseFaction)" in unequipped


def test_mq06_warning_trigger_is_player_stage_and_reentry_guarded():
    source = _merged_source("Quests:Storm:MQ06:StartSceneOnTriggerEnterScript")
    trigger = _member_body(source, "Event OnTriggerEnter(", "EndEvent")
    assert "akActionRef != PlayerAlias.GetReference()" in trigger
    assert "iPreReqStage != -1" in trigger
    assert "iTurnOffStage != -1" in trigger
    assert "!SceneToStart.IsPlaying()" in trigger


def test_mq06_alex_scene_activates_bound_trap_as_alex():
    source = _merged_source(
        "Fragments:Scenes:SF_Storm_MQ06_HugoPt3_AlexSc_0075DD75"
    )
    phase = _member_body(source, "Function Fragment_Phase_05_Begin()")
    assert "Alias_Ref_AlexTrapBookFurniture.GetReference()" in phase
    assert "Alias_Actor_Alex.GetActorReference()" in phase
    assert "furniture.Activate(alex)" in phase


@pytest.mark.parametrize(
    "script_name",
    (
        "Storm_MQ04Pt1_DocumentCounter",
        "Quests:Storm:MQ05:QuestScript",
        "Quests:Storm:MQ06:CultistsKillAliasScript",
        "Fragments:TopicInfos:TIF_Storm_MQ06_HugoPt3_007665AA",
    ),
)
def test_ambiguous_or_redundant_shells_remain_unpatched(script_name: str):
    assert _script_patch_source(script_name) is None


@pytest.mark.parametrize("script_name", PATCH_CASES)
def test_merged_patch_native_compiles_for_fo4(script_name: str):
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


def test_mq04_atrium_entry_stage_resyncs_quest_scoped_player_state():
    body = _member_body(
        _script_patch_source("Fragments:Quests:QF_Storm_MQ04_HugoPt1_0072EE0A"),
        "Function Fragment_Stage_0010_Item_00()",
    )
    assert "IsStageDone(100) && !IsStageDone(9000)" in body
    assert "SetValue(Storm_MQ_AudreyAwayValue, 1.0)" in body
    assert "IsStageDone(500)" in body
    assert "SetValue(Storm_MQ_WeatherLabGridAccess, 1.0)" in body
    assert "SetStage(" not in body


def test_mq04_hugo_confrontation_reevaluates_hugo_without_advancing():
    body = _member_body(
        _script_patch_source("Fragments:Quests:QF_Storm_MQ04_HugoPt1_0072EE0A"),
        "Function Fragment_Stage_0328_Item_00()",
    )
    assert "Actor_Hugo.GetActorReference()" in body
    assert "hugo.EvaluatePackage()" in body
    assert "SetStage(" not in body
    assert "MoveTo(" not in body


@pytest.mark.parametrize("stage", (50, 60, 70))
def test_mq05_instance_setup_stages_are_documented_no_ops(stage: int):
    patch = _script_patch_source("Fragments:Quests:QF_Storm_MQ05_HugoPt2_006F8B09")
    body = _member_body(patch, f"Function Fragment_Stage_{stage:04d}_Item_00()")
    assert body.splitlines()[1:] == []
    assert "Instance Set up" in patch


def test_mq05_found_machine_only_reasserts_the_retrieve_objective():
    body = _member_body(
        _script_patch_source("Fragments:Quests:QF_Storm_MQ05_HugoPt2_006F8B09"),
        "Function Fragment_Stage_0520_Item_00()",
    )
    assert "!IsObjectiveCompleted(60)" in body
    assert "SetObjectiveDisplayed(60)" in body
    assert "SetStage(" not in body
    assert "AddItem(" not in body


def test_mq06_disguise_collected_stage_does_not_skip_the_equip_step():
    body = _member_body(
        _script_patch_source("Fragments:Quests:QF_Storm_MQ06_HugoPt3_0073D7C1"),
        "Function Fragment_Stage_0005_Item_00()",
    )
    assert "IsStageDone(300) && !IsStageDone(320)" in body
    assert "SetObjectiveDisplayed(30)" in body
    assert "SetStage(320)" not in body
    assert "SetObjectiveCompleted(" not in body
