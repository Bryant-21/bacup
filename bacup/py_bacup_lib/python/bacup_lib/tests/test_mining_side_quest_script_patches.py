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

PATCH_MEMBERS = {
    "MTR07_EarthReactorTriggerScript": {"onactivate"},
    "MTR07_EarthQuestScript": {
        "onstageset",
        "onquestinit",
        "onquestshutdown",
        "actor.onplayerloadgame",
        "ontimer",
        "registerforplayerloadreconciliation",
        "ensureattacktimer",
    },
    "MTRZ05_MapScript": {"onequipped"},
    "MTR02_MinerAliasScript": {
        "onaliasinit",
        "actor.onitemequipped",
        "actor.onplayerloadgame",
        "onaliasshutdown",
        "reconcileequippedarmor",
    },
    "MTR02_MinerQuestScript": {
        "onstageset",
        "grantexcavatorarmorforlocalprogress",
        "checkarmorprogress",
    },
    "MTR02_MinerRegControlScript": {"onactivate"},
    "MTR05QuestScript": {"beaconissued"},
    "MTR05_MasterScript": {"triggermotherlodebreach"},
    "Fragments:Quests:QF_MTRZ05_Lucky_00019487": {
        "fragment_stage_0100_item_00",
        "fragment_stage_0255_item_00",
    },
    "Fragments:Quests:QF_MTR07_Earth_003443FB": {
        "tryadvanceaftercoreinstalled",
        "fragment_stage_0000_item_00",
        "fragment_stage_0010_item_00",
        "fragment_stage_0020_item_00",
        "fragment_stage_0030_item_00",
        "fragment_stage_0040_item_00",
        "fragment_stage_0050_item_00",
        "fragment_stage_0060_item_00",
        "fragment_stage_0070_item_00",
        "fragment_stage_0200_item_00",
        "fragment_stage_0220_item_00",
        "fragment_stage_0255_item_00",
    },
    "Fragments:Quests:QF_MTR02_Miner_0033C4DE": {
        "fragment_stage_0001_item_00",
        "fragment_stage_0005_item_00",
        "fragment_stage_0010_item_00",
        "fragment_stage_0020_item_00",
        "fragment_stage_0030_item_00",
        "fragment_stage_0040_item_00",
        "fragment_stage_0050_item_00",
        "fragment_stage_0060_item_00",
        "fragment_stage_0070_item_00",
        "fragment_stage_0080_item_00",
        "fragment_stage_0090_item_00",
        "fragment_stage_0100_item_00",
        "fragment_stage_0110_item_00",
        "fragment_stage_0120_item_00",
        "fragment_stage_0255_item_00",
    },
    "Fragments:Quests:QF_Tutorial_MineOre_0046ED03": {
        "fragment_stage_0010_item_00",
        "fragment_stage_0020_item_00",
        "fragment_stage_0030_item_00",
        "fragment_stage_0040_item_00",
        "fragment_stage_0100_item_00",
    },
    "Fragments:Quests:QF_MTR05_Mother_0006A379": {
        "fragment_stage_0001_item_00",
        "fragment_stage_0004_item_00",
        "fragment_stage_0005_item_00",
        "fragment_stage_0010_item_00",
        "fragment_stage_0020_item_00",
        "fragment_stage_0030_item_00",
        "fragment_stage_0050_item_00",
        "fragment_stage_0053_item_00",
        "fragment_stage_0055_item_00",
        "fragment_stage_0057_item_00",
        "fragment_stage_0060_item_00",
        "fragment_stage_0070_item_00",
        "fragment_stage_0090_item_00",
        "fragment_stage_0100_item_00",
        "fragment_stage_0150_item_00",
        "fragment_stage_0200_item_00",
        "fragment_stage_0210_item_00",
        "fragment_stage_0240_item_00",
        "fragment_stage_0250_item_00",
        "fragment_stage_0300_item_00",
        "fragment_stage_0305_item_00",
        "fragment_stage_0306_item_00",
        "fragment_stage_0307_item_00",
        "fragment_stage_0308_item_00",
        "fragment_stage_0310_item_00",
        "fragment_stage_0500_item_00",
        "fragment_stage_0510_item_00",
        "fragment_stage_0999_item_00",
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
        if name == member_name.casefold()
    )
    return "\n".join(lines[start : end + 1])


def _merged_source(script_name: str) -> str:
    source_path = SOURCE_ROOT / _script_relative_path(script_name, ".psc")
    patch = _script_patch_source(script_name)
    assert source_path.is_file(), source_path
    assert patch is not None
    return _merge_script_method_patches(source_path.read_text(encoding="utf-8"), patch)


@pytest.mark.parametrize(("script_name", "members"), PATCH_MEMBERS.items())
def test_mining_side_quest_patches_are_complete_member_fragments(
    script_name: str, members: set[str]
) -> None:
    patch = _script_patch_source(script_name)

    assert patch is not None
    if script_name == "MTR07_EarthReactorTriggerScript":
        assert patch.casefold().count("event onactivate(") == 1
        assert patch.casefold().count("event oninit(") == 1
    else:
        assert set(_member_names(patch)) == members
    assert not any(
        line.strip().casefold().startswith("scriptname ")
        for line in patch.splitlines()
    )
    if script_name not in {"MTR07_EarthReactorTriggerScript", "MTRZ05_MapScript"}:
        assert not any(
            line.strip().casefold().startswith("state ")
            for line in patch.splitlines()
        )
    assert not any(
        " property " in f" {line.strip().casefold()} " for line in patch.splitlines()
    )


@pytest.mark.parametrize(("script_name", "members"), PATCH_MEMBERS.items())
def test_mining_side_quest_patches_merge_once_and_compile(
    script_name: str, members: set[str]
) -> None:
    patch = _script_patch_source(script_name)
    assert patch is not None
    merged = _merged_source(script_name)
    merged_members = _member_names(merged)

    if script_name == "MTR07_EarthReactorTriggerScript":
        assert merged.casefold().count("event onactivate(") == 1
        assert merged.casefold().count("event oninit(") == 1
        assert 'Event OnInit()\n    GoToState("ready")' in merged
        assert "State ready\n    Event OnActivate" in merged
        assert "State busy\nEndState" in merged
    else:
        for member in members:
            assert merged_members.count(member) == 1
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


def test_reactor_slots_consume_one_core_and_advance_their_bound_stage() -> None:
    source = _script_patch_source("MTR07_EarthReactorTriggerScript")
    assert source is not None

    busy_index = source.index('GoToState("busy")')
    start_index = source.index("MTR07_EarthQuestStartKeyword.SendStoryEventAndWait(")
    running_recheck_index = source.index(
        "If !MTR07_Earth.IsRunning()", start_index
    )
    remove_index = source.index(
        "akActionRef.RemoveItem(IgnitionReactorCore01, 1, True)"
    )

    assert "akActionRef.GetItemCount(IgnitionReactorCore01) < 1" in source
    assert "MTR07_Earth == None" in source
    assert "MTR07_Earth.IsCompleted()" in source
    assert "akRef1 = akActionRef, akRef2 = Self" in source
    assert busy_index < start_index < running_recheck_index < remove_index
    assert 'GoToState("ready")' in source[running_recheck_index:remove_index]
    assert "MTR07_Earth.SetStage(QuestStageToSet)" in source
    assert "MTR07_EarthNoCoresMessage.Show()" in source


def test_miner_miracles_records_each_bound_piece_before_registration() -> None:
    alias_patch = _script_patch_source("MTR02_MinerAliasScript")
    coordinator_patch = _script_patch_source("MTR02_MinerQuestScript")
    registration_patch = _script_patch_source("MTR02_MinerRegControlScript")
    fragment_patch = _script_patch_source(
        "Fragments:Quests:QF_MTR02_Miner_0033C4DE"
    )
    assert alias_patch is not None
    assert coordinator_patch is not None
    assert registration_patch is not None
    assert fragment_patch is not None

    equipped = _member_body(alias_patch, "actor.onitemequipped")
    assert "minerQuest.GetStage() < 30" in equipped
    assert "MTR02_MinerBuiltLeftArm" in equipped
    assert "MTR02_MinerBuiltRightLeg" in equipped
    assert "minerQuest.SetStage(40)" in equipped
    assert "minerQuest.SetStage(90)" in equipped
    assert "SetValue(MTR02_Miner_LeftArmDone, 1.0)" in _member_body(
        fragment_patch, "fragment_stage_0040_item_00"
    )
    assert "SetValue(MTR02_Miner_RightLegDone, 1.0)" in _member_body(
        fragment_patch, "fragment_stage_0090_item_00"
    )

    coordinator = _member_body(coordinator_patch, "checkarmorprogress")
    assert "IsStageDone(40)" in coordinator
    assert "IsStageDone(50)" in coordinator
    assert "IsStageDone(60)" in coordinator
    assert "IsStageDone(70)" in coordinator
    assert "IsStageDone(80)" in coordinator
    assert "IsStageDone(90)" in coordinator
    assert "GetValue(" not in coordinator
    assert "SetStage(100)" in coordinator
    assert "SetStage(110)" in coordinator
    assert "SetStage(120)" in coordinator
    assert "auiStageID >= 40 && auiStageID <= 90" in _member_body(
        coordinator_patch, "onstageset"
    )

    registration = _member_body(registration_patch, "onactivate")
    assert "!MTR02_Miner.IsStageDone(120)" in registration
    assert registration.count("player.WornHasKeyword(") == 6
    assert "MTR02_Miner.SetStage(255)" in registration
    assert "MTR02_MinerRegSuccess.Show()" in registration
    assert "MTR02_MinerRegFailure.Show()" in registration


def test_miner_miracles_reconciles_an_already_equipped_full_suit() -> None:
    alias_patch = _script_patch_source("MTR02_MinerAliasScript")
    coordinator_patch = _script_patch_source("MTR02_MinerQuestScript")
    assert alias_patch is not None
    assert coordinator_patch is not None

    alias_init = _member_body(alias_patch, "onaliasinit")
    load_game = _member_body(alias_patch, "actor.onplayerloadgame")
    equipped = _member_body(alias_patch, "actor.onitemequipped")
    shutdown = _member_body(alias_patch, "onaliasshutdown")
    reconcile = _member_body(alias_patch, "reconcileequippedarmor")
    assert 'RegisterForRemoteEvent(player, "OnPlayerLoadGame")' in alias_init
    assert "ReconcileEquippedArmor()" in alias_init
    assert "ReconcileEquippedArmor()" in load_game
    assert "minerQuestScript.GrantExcavatorArmorForLocalProgress()" in load_game
    assert "minerQuestScript.GrantExcavatorArmorForLocalProgress()" in equipped
    assert 'UnregisterForRemoteEvent(player, "OnItemEquipped")' in shutdown
    assert 'UnregisterForRemoteEvent(player, "OnPlayerLoadGame")' in shutdown
    assert "minerQuest.GetStage() < 30" in reconcile
    assert reconcile.count("player.WornHasKeyword(") == 6
    assert "minerQuest.SetStage(40)" in reconcile
    assert "minerQuest.SetStage(90)" in reconcile

    stage_set = _member_body(coordinator_patch, "onstageset")
    assert "auiStageID == GetSchematicsStage" in stage_set
    assert "GrantExcavatorArmorForLocalProgress()" in stage_set
    assert "playerAliasScript.ReconcileEquippedArmor()" in stage_set

    grant = _member_body(
        coordinator_patch, "grantexcavatorarmorforlocalprogress"
    )
    assert "!IsRunning()" in grant
    assert "GetStage() < GetSchematicsStage" in grant
    assert "IsStageDone(255)" in grant
    for armor_property in (
        "Armor_Power_Excavator_ArmLeft",
        "Armor_Power_Excavator_ArmRight",
        "Armor_Power_Excavator_Helmet",
        "Armor_Power_Excavator_Torso",
        "Armor_Power_Excavator_LegLeft",
        "Armor_Power_Excavator_LegRight",
    ):
        assert f"player.GetItemCount({armor_property}) == 0" in grant
        assert f"player.AddItem({armor_property}, 1, False)" in grant
    assert "Recipe_" not in grant


def test_miner_miracles_objectives_follow_the_bound_stage_meanings() -> None:
    fragment_patch = _script_patch_source(
        "Fragments:Quests:QF_MTR02_Miner_0033C4DE"
    )
    assert fragment_patch is not None

    stage_20 = _member_body(fragment_patch, "fragment_stage_0020_item_00")
    stage_30 = _member_body(fragment_patch, "fragment_stage_0030_item_00")
    assert "SetObjectiveCompleted(10, True)" in stage_20
    assert "SetObjectiveDisplayed(15, True)" in stage_20
    assert "SetObjectiveDisplayed(20, True)" in stage_20
    assert "SetObjectiveCompleted(20, True)" in stage_30
    for objective in (40, 50, 60, 70):
        assert f"SetObjectiveDisplayed({objective}, True)" in stage_30

    assert "SetObjectiveCompleted(40, True)" in _member_body(
        fragment_patch, "fragment_stage_0100_item_00"
    )
    assert "SetObjectiveCompleted(50, True)" in _member_body(
        fragment_patch, "fragment_stage_0110_item_00"
    )
    stage_120 = _member_body(fragment_patch, "fragment_stage_0120_item_00")
    for objective in (40, 50, 60, 70):
        assert f"SetObjectiveCompleted({objective}, True)" in stage_120
    assert "SetObjectiveDisplayed(80, True)" in stage_120
    assert "SetObjectiveCompleted(80, True)" in _member_body(
        fragment_patch, "fragment_stage_0255_item_00"
    )


def test_miner_miracles_stage_convergence_does_not_depend_on_fragment_order() -> None:
    coordinator_patch = _script_patch_source("MTR02_MinerQuestScript")
    fragment_patch = _script_patch_source(
        "Fragments:Quests:QF_MTR02_Miner_0033C4DE"
    )
    assert coordinator_patch is not None
    assert fragment_patch is not None

    coordinator = _member_body(coordinator_patch, "checkarmorprogress")
    assert "GetValue(" not in coordinator
    assert "IsStageDone(40) && IsStageDone(50)" in coordinator
    assert "IsStageDone(80) && IsStageDone(90)" in coordinator
    assert "IsStageDone(60) && IsStageDone(70)" in coordinator
    assert "SetValue(MTR02_Miner_RightLegDone, 1.0)" in _member_body(
        fragment_patch, "fragment_stage_0090_item_00"
    )


def test_lucky_strike_records_its_checkpoint_and_shuts_down_the_mining_node() -> None:
    fragment_patch = _script_patch_source(
        "Fragments:Quests:QF_MTRZ05_Lucky_00019487"
    )
    assert fragment_patch is not None

    assert "SetObjectiveDisplayed(10, True)" in _member_body(
        fragment_patch, "fragment_stage_0100_item_00"
    )
    assert "MTRZ05_MinerCheckpointValue.SetValue(100.0)" in _member_body(
        fragment_patch, "fragment_stage_0100_item_00"
    )
    complete = _member_body(fragment_patch, "fragment_stage_0255_item_00")
    assert "Alias_MTRZ05_MiningNode.GetReference().Disable()" in complete
    assert "CompleteQuest()" in complete
    assert "Stop()" in complete


def test_lucky_strike_consumes_only_the_map_that_successfully_started_the_quest() -> None:
    map_patch = _script_patch_source("MTRZ05_MapScript")
    assert map_patch is not None

    equipped = _member_body(map_patch, "onequipped")
    send_index = equipped.index("MTRZ05MapKeyword.SendStoryEventAndWait(")
    running_recheck_index = equipped.index(
        "If MTRZ05_Lucky.IsRunning() || MTRZ05_Lucky.IsCompleted()", send_index
    )
    fire_once_index = equipped.index("FireOnce = 1", running_recheck_index)
    remove_index = equipped.index("akActor.RemoveItem(Self, 1, True)")
    assert send_index < running_recheck_index < fire_once_index < remove_index
    assert equipped[:send_index].count("RemoveItem(") == 0


def test_motherlode_uses_quest_bound_scene_stages_and_beacon_entrypoint() -> None:
    root_patch = _script_patch_source("MTR05QuestScript")
    master_patch = _script_patch_source("MTR05_MasterScript")
    fragment_patch = _script_patch_source(
        "Fragments:Quests:QF_MTR05_Mother_0006A379"
    )
    assert root_patch is not None
    assert master_patch is not None
    assert fragment_patch is not None

    assert "SetStage(iBeaconIssuedStage)" in _member_body(root_patch, "beaconissued")
    breach = _member_body(master_patch, "triggermotherlodebreach")
    assert "MTR05_Mother.SetStage(iMotherlodeStandardSceneStage)" in breach
    assert "MTR05_Mother.SetStage(iMotherlodeAltSceneStage)" in breach
    assert "MTR05_Mother_0306_MotherlodeRevealScene.Start()" in _member_body(
        fragment_patch, "fragment_stage_0306_item_00"
    )
    assert "MTR05_Mother_0307_MotherlodeRevealAltScene.Start()" in _member_body(
        fragment_patch, "fragment_stage_0307_item_00"
    )


def test_gauley_mine_ore_counter_legs_are_order_independent_and_idempotent() -> None:
    patch = _script_patch_source("Fragments:Quests:QF_Tutorial_MineOre_0046ED03")
    assert patch is not None

    # The generated skeleton declares no properties, so every member may only use
    # the Quest self-API. Objective 10 is the quest's only objective.
    assert "SetObjectiveDisplayed(10)" in _member_body(
        patch, "fragment_stage_0010_item_00"
    )

    # Stages 20/30/40 are produced independently by DefaultAliasOnActivate on aliases
    # 3/5/6, and none of them has a NextStage, so each leg must observe the other two
    # rather than itself: the engine's own stage flag for the running fragment must not
    # be assumed, and the three activations can arrive in any order.
    legs = {
        "fragment_stage_0020_item_00": (30, 40),
        "fragment_stage_0030_item_00": (20, 40),
        "fragment_stage_0040_item_00": (20, 30),
    }
    for member_name, (other_a, other_b) in legs.items():
        body = _member_body(patch, member_name)
        assert f"IsStageDone({other_a})" in body
        assert f"IsStageDone({other_b})" in body
        assert "!IsStageDone(100)" in body
        assert body.count("SetStage(100)") == 1
        assert body.index("SetObjectiveCompleted(10)") < body.index("SetStage(100)")
        own_stage = int(member_name.split("_")[2])
        assert f"IsStageDone({own_stage})" not in body

    # Stage 100 already carries the CompleteQuest flag and B21:QuestRewards keys its caps
    # reward on stage 100, so the fragment must not complete or reward the quest again.
    stage_100 = _member_body(patch, "fragment_stage_0100_item_00")
    assert "!IsObjectiveCompleted(10)" in stage_100
    assert "CompleteQuest()" not in stage_100
    assert "AddItem(" not in stage_100
    assert "SetStage(" not in stage_100


def test_motherlode_launch_sequence_picks_the_single_player_breach_scene() -> None:
    patch = _script_patch_source("Fragments:Quests:QF_MTR05_Mother_0006A379")
    assert patch is not None

    # MTR05_MasterScript.TriggerMotherlodeBreach() owns the 306/307 choice
    # (iMotherlodeStandardSceneStage=306, iMotherlodeAltSceneStage=307) but nothing calls
    # it, so stage 305 carries the same predicate with the quest's own stages.
    stage_305 = _member_body(patch, "fragment_stage_0305_item_00")
    assert stage_305.index("IsStageDone(306)") < stage_305.index("SetStage(307)")
    assert stage_305.count("SetStage(306)") == 1
    assert stage_305.count("SetStage(307)") == 1
