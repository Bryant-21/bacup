from __future__ import annotations

from pathlib import Path

import pytest

from bacup_lib.workflows.unified import (
    _iter_top_level_papyrus_members,
    _merge_script_method_patches,
    _script_patch_source,
    _script_relative_path,
)
from creation_lib.pex import decompile_pex
from creation_lib.pex.native_runtime import compile_psc

from bacup_lib.tests.test_terminal_fragment_script_patches import _fo4_base_source


REPO_ROOT = Path(__file__).resolve().parents[5]
DEPLOYED_SCRIPTS_ROOT = REPO_ROOT / "mods" / "SeventySix" / "data" / "Scripts"

# contracts/w3c-w05.md -- rows from Parts C/E1/E2/E7/E10/E11 of the shard's own 82-row
# contract (batches A/D range, #1-24 and #65-82), approved for authoring this wave per
# team-lead's Q2 (RaiderBlock: ships full body w/ record-dependency residual) and Q7
# (band cluster: consumer-pull design) rulings, plus this shard's own re-trace of the
# remaining fully- or high-confidence-evidenced rows. Rows still needing a dedicated
# follow-up pass (E2b/c, E3a/b, the Graffiti/Secret SettlersDaily sub-shapes, E9) are not
# authored here -- see the contract's own scope disclosures.
PATCH_CASES: dict[str, dict[str, set[str] | tuple[str, ...]]] = {
    "W05_RaiderBlock_Quest_Script": {
        "members": {"onquestinit", "objectreference.ontriggerenter"},
        "snippets": (
            "RegisterForRemoteEvent(BlockadeTriggerAlias.GetReference(), \"OnTriggerEnter\")",
            "LetPlayerPass = False",
            "currentPlayer.GetActorReference().AddToFaction(W05_Community_RaiderBlockade_Faction)",
            "currentPlayer.GetActorReference().AddToFaction(W05_Community_RaiderBlockadeEnemy_Faction)",
        ),
    },
    "W05_Raider_BandAudienceScript": {
        "members": {"oninit", "objectreference.ontriggerenter"},
        "snippets": (
            'RegisterForRemoteEvent(AudienceTrigger.GetReference(), "OnTriggerEnter")',
            "Audience.GetAt(Utility.RandomInt(0, Audience.GetCount() - 1))",
            "W05_Crater_Talent.GetValue() == 1.0",
            "speaker.SayCustom(W05_RaiderCheerLines)",
            "speaker.SayCustom(W05_RaiderJeerLines)",
        ),
    },
    "W05_Crater_BandFurnitureAliasScript": {
        "members": {"onactivate"},
        "snippets": (
            "furnitureRef.IsFurnitureInUse()",
            "iFurnituresInUse == 0",
            "W05_Crater_Talent.SetValue(Utility.RandomInt(0, 1) as Float)",
        ),
    },
    "DefaultOnPlayerConnect": {
        "members": {"oninit"},
        "snippets": (
            "player.GetValue(PlayerDataList[i].ActorValueToCheck) == PlayerDataList[i].ValueToCheck as Float",
            "player.AddToFaction(PlayerDataList[i].FactionToAdd)",
            "player.SetValue(PlayerDataList[i].ActorValueToSet, PlayerDataList[i].ValueToSet as Float)",
        ),
    },
    "Fragments:Quests:QF_W05_Daily_F01_Radio_0054A71B": {
        "members": {"fragment_stage_1000_item_00"},
        "snippets": ("Stop()",),
    },
    "Fragments:Quests:QF_W05_Daily_Photo_MiscPoint_005A5E29": {
        "members": {"fragment_stage_0100_item_00", "fragment_stage_9000_item_00"},
        "snippets": (
            "SetObjectiveDisplayed(10, True, True)",
            "SetObjectiveCompleted(10, True)",
            "Stop()",
        ),
    },
    "Fragments:Quests:QF_W05_LGV01_PointerQuest_0059C22B": {
        "members": {"fragment_stage_0200_item_00", "fragment_stage_9000_item_00"},
        "snippets": (
            "W05_Tutorial_LegendaryScrip.Show()",
            "SetObjectiveDisplayed(200, True, True)",
            "SetObjectiveCompleted(200, True)",
            "Stop()",
        ),
    },
    "Fragments:TopicInfos:TIF_W05_Wayward_DanielScene_0054AFA3": {
        "members": {"fragment_end"},
        "snippets": ("W05_Wayward_DanielScene_Scene.Start()",),
    },
    "W05_RoperAliasScript": {
        "members": {"ondeath"},
        "snippets": (
            "player = OwningPlayer.GetActorReference()",
            "player == None || akKiller != player",
            "player.SetValue(W05_MQ_002P_Radical_PlayerKilledRoper, 1.0)",
        ),
    },
    "W05_RadicalCollectionScript": {
        "members": {"oninit", "quest.onstageset"},
        "snippets": (
            'RegisterForRemoteEvent(GetOwningQuest(), "OnStageSet")',
            "auiStageID != 1260",
            "MoveAllTo(MoveToMarker.GetReference())",
        ),
    },
    "W05_QuestToggleAliasObjectsOnAV": {
        "members": {"evaluateenablestates"},
        "snippets": (
            "currentValue >= EnableStates[i].TargetValue",
            "targetRef.Enable()",
            "targetRef.Disable()",
            "MaintainStateOnGreaterThanTargetValue",
        ),
    },
    "W05_Wayward_TopicInfoSetValueOnAlias": {
        "members": {"fragment_end"},
        "snippets": ("akSpeakerRef.SetValue(W05_Wayward_RC_MortMostRecentClue, NewValue)",),
    },
    "W05_Wayward_IntTriggerRCScript": {
        "members": {"ontriggerenter"},
        "snippets": (
            "OwningPlayer.GetActorReference().GetValue(W05_Wayward_PlayerCompletedQuestline) >= 1.0",
            "akActionRef.AddKeyword(W05_Wayward_Interior_RandomConvoHandlerStartKeyword)",
        ),
    },
    "W05_Wayward_PatronsColl": {
        "members": {"broadcastduchessshout"},
        "snippets": (
            "duchessActor.Say(W05_Wayward_DuchessViolenceShouts, None, False, patron)",
        ),
    },
    "W05_WaywardMiscPointerScript": {
        "members": {"evaluatemiscpointer"},
        "snippets": (
            "CharGen_ReclamationDay.IsCompleted()",
            "CompletedQuests[i] != None && CompletedQuests[i].IsCompleted()",
            "player.HasKeyword(BlockingKeywords[i])",
            "player.AddKeyword(W05_Wayward_MiscPointer_QuestStartKeyword)",
        ),
    },
    "Fragments:Quests:QF_W05_Daily_Foundation01_0054A71A": {
        "members": {
            "fragment_stage_0100_item_00",
            "fragment_stage_0200_item_00",
            "fragment_stage_0300_item_00",
            "fragment_stage_0400_item_00",
            "fragment_stage_9990_item_00",
            "fragment_stage_9992_item_00",
            "fragment_stage_9998_item_00",
            "fragment_stage_9999_item_00",
        },
        "snippets": (
            "SetObjectiveDisplayed(100, True, True)",
            "SetObjectiveCompleted(100, True)",
            "SetObjectiveCompleted(300, True)",
            "player.ModValue(Reputation_AV_Foundation, Rep_Mod_DailyS_Add.GetValue())",
            "player.AddItem(Caps001, 60, True)",
            "player.ModValue(Reputation_AV_Foundation, W05_Daily_Foundation01_DonationRepValue.GetValue())",
            "player.ModValue(Reputation_AV_Crater, Rep_Mod_Add_Small.GetValue())",
            "player.ModValue(Reputation_AV_Foundation, Rep_Mod_Subtract_Small.GetValue())",
            "player.AddItem(Caps001, 70, True)",
            "SetStage(9995)",
            "Stop()",
        ),
    },
    "Fragments:Quests:QF_W05_Daily_R02_Retirement_0054DCF9": {
        "members": {
            "fragment_stage_0050_item_00",
            "fragment_stage_0100_item_00",
            "fragment_stage_0200_item_00",
            "fragment_stage_1000_item_00",
            "fragment_stage_1100_item_00",
            "fragment_stage_1200_item_00",
            "fragment_stage_1310_item_00",
            "fragment_stage_1320_item_00",
            "fragment_stage_1330_item_00",
            "fragment_stage_1340_item_00",
            "fragment_stage_1360_item_00",
            "fragment_stage_5000_item_00",
            "fragment_stage_5100_item_00",
            "fragment_stage_9000_item_00",
            "fragment_stage_9990_item_00",
        },
        "snippets": (
            "SetObjectiveDisplayed(100, True, True)",
            "SetObjectiveDisplayed(2000, True, True)",
            "SetStage(1000)",
            "SetObjectiveCompleted(1000, True)",
            "SetObjectiveDisplayed(5000, True, True)",
            "SetStage(9000)",
            "player.ModValue(pReputation_AV_Crater, Rep_Mod_DailyR_Add.GetValue())",
            "Stop()",
        ),
    },
    "Fragments:Quests:QF_W05_SettlersDaily_Paintin_003F28CC": {
        "members": {
            "fragment_stage_0100_item_00",
            "fragment_stage_0101_item_00",
            "fragment_stage_0102_item_00",
            "fragment_stage_0110_item_00",
            "fragment_stage_0120_item_00",
            "fragment_stage_0130_item_00",
            "fragment_stage_0199_item_00",
            "fragment_stage_9000_item_00",
        },
        "snippets": (
            "Alias_currentPlayer.GetActorReference().SetValue(W05_SettlersDaily_Graffiti01, 1.0)",
            "Alias_currentPlayer.GetActorReference().SetValue(W05_SettlersDaily_Graffiti02, 1.0)",
            "Alias_currentPlayer.GetActorReference().SetValue(W05_SettlersDaily_Graffiti03, 1.0)",
            "SetObjectiveCompleted(100, True)",
        ),
    },
    "Fragments:Quests:QF_W05_SettlersDaily_Secret_00541685": {
        "members": {
            "fragment_stage_0100_item_00",
            "fragment_stage_0199_item_00",
            "fragment_stage_0201_item_00",
            "fragment_stage_0250_item_00",
            "fragment_stage_0299_item_00",
            "fragment_stage_9000_item_00",
        },
        "snippets": (
            "Alias_currentPlayer.GetActorReference().SetValue(W05_SettlersDaily_Secret01, 1.0)",
            "SetObjectiveCompleted(201, True)",
            "SetObjectiveCompleted(200, True)",
        ),
    },
    "Fragments:Quests:qf_w05_daily_r01_0054fa50": {
        "members": {
            "fragment_stage_0010_item_00",
            "fragment_stage_0100_item_00",
            "fragment_stage_0200_item_00",
            "fragment_stage_0220_item_00",
            "fragment_stage_0221_item_00",
            "fragment_stage_0222_item_00",
            "fragment_stage_0223_item_00",
            "fragment_stage_0224_item_00",
            "fragment_stage_0225_item_00",
            "fragment_stage_0226_item_00",
            "fragment_stage_0227_item_00",
            "fragment_stage_0230_item_00",
            "fragment_stage_0231_item_00",
            "fragment_stage_0232_item_00",
            "fragment_stage_0233_item_00",
            "fragment_stage_0234_item_00",
            "fragment_stage_0235_item_00",
            "fragment_stage_0236_item_00",
            "fragment_stage_0237_item_00",
            "fragment_stage_0240_item_00",
            "fragment_stage_0241_item_00",
            "fragment_stage_0242_item_00",
            "fragment_stage_0243_item_00",
            "fragment_stage_0244_item_00",
            "fragment_stage_0245_item_00",
            "fragment_stage_0246_item_00",
            "fragment_stage_0247_item_00",
            "fragment_stage_0300_item_00",
            "fragment_stage_0400_item_00",
            "fragment_stage_0410_item_00",
            "fragment_stage_9000_item_00",
            "fragment_stage_9990_item_00",
            "fragment_stage_10000_item_00",
        },
        "snippets": (
            "SetObjectiveDisplayed(100, True, True)",
            "SetObjectiveCompleted(100, True)",
            "Alias_TechItemPlacement01.GetReference().PlaceAtMe(W05_Daily_R01_Tech)",
            "Alias_TechItemPlacement01.GetReference().PlaceAtMe(W05_Daily_R01_TechBroken)",
            "spawned.AddKeyword(W05_Daily_R01_TechKeyword)",
            "SetObjectiveCompleted(300, True)",
            "SetObjectiveCompleted(400, True)",
        ),
    },
}

RECORD_DEPENDENT_SETTLERS_DAILY_FRAGMENTS = (
    "Fragments:Quests:QF_W05_SettlersDaily_Clinic_003F2DC7",
    "Fragments:Quests:QF_W05_SettlersDaily_Fieldha_00403436",
    "Fragments:Quests:QF_W05_SettlersDaily_Restock_0041B725",
    "Fragments:Quests:QF_W05_SettlersDaily_Stew_003F2DC9",
)


def _member_names(source: str) -> set[str]:
    return {
        name
        for kind, name, _start, _end in _iter_top_level_papyrus_members(
            source.splitlines()
        )
        if kind in {"function", "event"}
    }


def _merged_production_source(base_name: str) -> str:
    pex_path = DEPLOYED_SCRIPTS_ROOT / _script_relative_path(base_name, ".pex")
    if not pex_path.is_file():
        pytest.skip(f"deployed production PEX unavailable: {pex_path}")
    skeleton = decompile_pex(pex_path, fo4_api_compat=True)
    patch = _script_patch_source(base_name)
    assert patch is not None
    return _merge_script_method_patches(skeleton, patch)


@pytest.mark.parametrize(("base_name", "case"), PATCH_CASES.items())
def test_batchD_patch_has_expected_members_and_calls(
    base_name: str, case: dict[str, set[str] | tuple[str, ...]]
):
    patch = _script_patch_source(base_name)

    assert patch is not None
    assert not any(
        line.strip().lower().startswith("scriptname ") for line in patch.splitlines()
    )
    assert _member_names(patch) == case["members"]
    for snippet in case["snippets"]:
        assert snippet in patch


@pytest.mark.parametrize("base_name", PATCH_CASES)
def test_batchD_production_merge_native_compiles_for_fo4(base_name: str):
    base_source = _fo4_base_source()
    if base_source is None:
        pytest.skip("FO4 base Papyrus sources unavailable")

    result = compile_psc(
        _merged_production_source(base_name),
        imports=[str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=f"{base_name.rsplit(':', 1)[-1]}.psc",
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None


def test_raiderblock_uses_letplayerpass_local_not_invented_variable():
    # Contract Part C1, coordinator Q2 ruling ("ships full body w/ record-dependency
    # residual"): the skeleton already declares script-local `Bool LetPlayerPass` and
    # `actor[] PlayerCombatChange` -- the patch must reuse the pre-declared local for the
    # pass/block state rather than inventing a new one. PlayerCombatChange stays
    # unconsumed/dormant (no evidenced undo mechanism), disclosed in the contract.
    patch = _script_patch_source("W05_RaiderBlock_Quest_Script")
    assert patch is not None
    assert "LetPlayerPass" in patch
    assert "PlayerCombatChange" not in patch


def test_wayward_misc_pointer_completed_quests_are_any_completed_blockers():
    patch = _script_patch_source("W05_WaywardMiscPointerScript")
    assert patch is not None
    assert "CompletedQuests[i] != None && CompletedQuests[i].IsCompleted()" in patch
    assert "CompletedQuests[i] == None || !CompletedQuests[i].IsCompleted()" not in patch
    assert patch.rfind("EndWhile") < patch.index(
        "player.AddKeyword(W05_Wayward_MiscPointer_QuestStartKeyword)"
    )


def test_crater_band_writer_reaches_binary_setvalue_and_consumer_never_rolls_percentage():
    furniture_patch = _script_patch_source("W05_Crater_BandFurnitureAliasScript")
    audience_patch = _script_patch_source("W05_Raider_BandAudienceScript")
    assert furniture_patch is not None
    assert audience_patch is not None
    assert "iFurnituresInUse == 0" in furniture_patch
    assert "W05_Crater_Talent.SetValue(Utility.RandomInt(0, 1) as Float)" in furniture_patch
    assert "W05_Crater_Talent.GetValue() == 1.0" in audience_patch
    assert "RandomFloat" not in furniture_patch
    assert "RandomFloat" not in audience_patch


@pytest.mark.parametrize(
    ("killer_case", "writes_player_kill"),
    (("owning_player", True), ("npc", False), ("environmental_none", False)),
)
def test_roper_only_marks_the_bound_owning_player_kill(
    killer_case: str, writes_player_kill: bool
):
    patch = _script_patch_source("W05_RoperAliasScript")
    assert patch is not None
    assert "player = OwningPlayer.GetActorReference()" in patch
    assert "If player == None || akKiller != player" in patch
    assert "Game.GetPlayer().SetValue" not in patch
    assert (killer_case == "owning_player") is writes_player_kill


def test_quest_toggle_uses_only_live_static_threshold_semantics():
    patch = _script_patch_source("W05_QuestToggleAliasObjectsOnAV")
    assert patch is not None
    assert "currentValue >= EnableStates[i].TargetValue" in patch
    assert "TargetValueLessThanNow" not in patch
    assert "Utility.Now" not in patch


@pytest.mark.parametrize("base_name", RECORD_DEPENDENT_SETTLERS_DAILY_FRAGMENTS)
def test_record_dependent_settlers_daily_fragments_remain_unpatched(base_name: str):
    assert base_name not in PATCH_CASES
    assert _script_patch_source(base_name) is None


def test_foundation_unproved_stage_bodies_remain_absent():
    patch = _script_patch_source(
        "Fragments:Quests:QF_W05_Daily_Foundation01_0054A71A"
    )
    assert patch is not None
    assert _member_names(patch).isdisjoint(
        {
            "fragment_stage_0310_item_00",
            "fragment_stage_0320_item_00",
            "fragment_stage_9900_item_00",
            "fragment_stage_9995_item_00",
        }
    )


def test_r02_unresolved_hunter_reward_stage_bodies_remain_absent():
    patch = _script_patch_source(
        "Fragments:Quests:QF_W05_Daily_R02_Retirement_0054DCF9"
    )
    assert patch is not None
    assert _member_names(patch).isdisjoint(
        {
            "fragment_stage_2000_item_00",
            "fragment_stage_2100_item_00",
            "fragment_stage_2200_item_00",
            "fragment_stage_2300_item_00",
            "fragment_stage_2400_item_00",
            "fragment_stage_2500_item_00",
            "fragment_stage_2510_item_00",
            "fragment_stage_2520_item_00",
            "fragment_stage_2600_item_00",
            "fragment_stage_3000_item_00",
            "fragment_stage_3100_item_00",
            "fragment_stage_3200_item_00",
            "fragment_stage_3300_item_00",
            "fragment_stage_3400_item_00",
            "fragment_stage_3500_item_00",
            "fragment_stage_3510_item_00",
            "fragment_stage_3520_item_00",
            "fragment_stage_3600_item_00",
        }
    )


def test_batchD_patch_count_matches_this_wave():
    # 20 rows authored this wave from the shard's own batch A/D range (#1-24, #45, #65-82).
    assert len(PATCH_CASES) == 20
