"""MQR 201P/202P quest-controller repairs and the sibling-cast contract.

The FO76 `W05_MQR_201P_QuestScript` and `W05_MQR_202P_QuestScript` PEX files ship
server-stripped: properties and docstrings survive, every function body is gone.
These are the stateful controllers behind the Lou stealth approach, the third
deathtrap callback, and the two Grafton Steel turret waves, so a merge or
state-machine regression here would still compile while silently doing nothing.
"""

from __future__ import annotations

import csv
from collections import Counter
from pathlib import Path

import pytest

from bacup_lib.tests.test_terminal_fragment_script_patches import _fo4_base_source
from bacup_lib.workflows.unified import (
    _iter_top_level_papyrus_members,
    _merge_script_method_patches,
    _script_patch_source,
)
from creation_lib.pex.native_runtime import compile_psc


CONTROLLER_201 = "W05_MQR_201P_QuestScript"
CONTROLLER_202 = "W05_MQR_202P_QuestScript"
QF_202 = "Fragments:Quests:QF_W05_MQR_202P_0041C9E6"
QF_203 = "Fragments:Quests:QF_W05_MQR_203P_0042F31B"

CONTROLLERS = (CONTROLLER_201, CONTROLLER_202)
REPO_ROOT = Path(__file__).resolve().parents[5]
STATUS = REPO_ROOT / "bacup" / "docs" / "stub_restoration" / "status.csv"
CLOSURE_CONTRACT = "contracts/w05-mqr-202-alias-producer-closure.md"

# Declarations copied from the generated skeletons; the merger keeps them and may
# not introduce new properties or script variables.
TRACKED_SKELETONS = {
    CONTROLLER_201: """Scriptname W05_MQR_201P_QuestScript Extends Quest

Bool bSignalActive
Float fDoorToWeaselDistance
Float DetectionTimerLength = 0.5
Int DetectionTimerID = 3
Int SignalTimerID = 2
Int WeaselDeathtrapSayTimerID = 1
Float WeaselDeathtrapSayTimerLength = 5.0

Int Property Deathtrap03BeginStage = 1510 Auto
actorvalue Property Luck Auto mandatory
topic Property W05_MQR_201P_LouSaysTopic_SneakFail Auto mandatory
topic Property W05_MQR_201P_LouSaysTopic_ShootSucceed Auto mandatory
topic Property W05_MQR_201P_LouSaysTopic_ShootFail Auto mandatory
topic Property W05_MQR_201P_Weasel_Deathtrap03_Comment02 Auto mandatory
referencealias Property LouRoomMarker01 Auto mandatory
referencealias Property LouMineEntranceDoorInterior Auto mandatory
Int Property LouNoticedStage = 1740 Auto
referencealias Property FisherRadioMarker Auto
referencealias Property DeathTrap03BTrigger Auto mandatory
referencealias Property Weasel Auto mandatory
referencealias Property Lou Auto mandatory
referencealias Property currentPlayer Auto mandatory
message Property W05_MQR_201P_SignalStrengthMessage Auto mandatory
locationalias Property LousMineInteriorLoc Auto mandatory
location Property LocMountainsLousMineInteriorLocation Auto mandatory
Float Property SignalTimerLength = 8.0 Auto
Float Property RadioStationFreq = 92.5 Auto
Float Property LouDistance = 800.0 Auto
Int Property PlayerTalkedToLouStage = 1801 Auto
Int Property AllBreakersOffStage = 1745 Auto
Int Property PlayerShotLouSuccessStage = 1721 Auto
Int Property PlayerShotLouFailStage = 1720 Auto
actorvalue Property Perception Auto mandatory
Int Property StopDetonationStage = 1700 Auto
Int Property Deathtrap03WeaselStage = 1515 Auto
actorvalue[] Property AwayValuesStart Auto mandatory
actorvalue Property W05_MQR_LouAwayValue Auto mandatory
actorvalue Property W05_MQR_RaRaAwayValue Auto mandatory
actorvalue Property W05_MQR_GailAwayValue Auto mandatory
globalvariable Property SPECIAL_Challenge03_Hard Auto mandatory
""",
    CONTROLLER_202: """Scriptname W05_MQR_202P_QuestScript Extends Quest conditional

Struct BossVentDatum
	referencealias Vent
	referencealias MarkerC
	referencealias MarkerB
	referencealias MarkerA
EndStruct

Int VentIndex
Float RaRaItemDropTimerOutTimerLength = 30.0
Int RaRaItemDropTimeOutTimerID = 2
Int RaRaBossVentTimerID = 1
Int RaRaDropCount = 0

referencealias[] Property Turrets02Targets Auto
referencealias[] Property Turrets01Targets Auto
referencealias[] Property Turrets02Aliases Auto
referencealias[] Property Turrets01Aliases Auto
refcollectionalias Property Turrets02 Auto mandatory
refcollectionalias Property Turrets01 Auto mandatory
Int Property RaRaDropCountMax = 20 Auto
Int Property VentTimerInit = 8 Auto
Int Property VentTimerMax = 24 Auto
Int Property VentTimerMin = 12 Auto
BossVentDatum[] Property BossVentData Auto mandatory
Bool Property RaRaBossVentReadyToPeek = False Auto conditional
referencealias Property RaRaBossCurrentVent Auto mandatory
scene Property W05_MQR_202P_RaRaVent_1600_BossPeekSequence Auto mandatory
Int Property DefeatedBossStage = 1650 Auto
Int Property ItemDroppedObjective = 1610 Auto
Int Property DefeatRobotsNearEndStage = 1520 Auto
referencealias Property RaRaItemToDrop Auto mandatory
referencealias Property RaRa Auto mandatory
leveleditem Property W05_MQR_202P_LL_RaRaDropItemList Auto mandatory
""",
}

# QUST VMAD fragment table for W05_MQR_202P: the two turret-wave activation stages
# whose fragments call EnableAll() on the matching collection alias.
TURRET_WAVE_STAGES = {1210: "Turrets01", 1220: "Turrets02"}


def _patch(script_name: str) -> str:
    patch = _script_patch_source(script_name)
    assert patch is not None, script_name
    return patch


def _member_names(source: str) -> list[str]:
    return [
        name
        for kind, name, _start, _end in _iter_top_level_papyrus_members(
            source.splitlines()
        )
        if kind in {"function", "event"}
    ]


def _member_body(source: str, member_name: str) -> str:
    start, end = next(
        (start, end)
        for _kind, name, start, end in _iter_top_level_papyrus_members(
            source.splitlines()
        )
        if name == member_name.lower()
    )
    return "\n".join(source.splitlines()[start : end + 1])


def _merged(script_name: str) -> str:
    return _merge_script_method_patches(
        TRACKED_SKELETONS[script_name], _patch(script_name)
    )


def test_201_controller_owns_exactly_the_three_recovered_entrypoints():
    assert set(_member_names(_patch(CONTROLLER_201))) == {
        "onstageset",
        "onquestshutdown",
        "ontimer",
    }


def test_201_stage_driver_only_uses_bound_stage_properties():
    driver = _member_body(_patch(CONTROLLER_201), "onstageset")

    assert "auiStageID == Deathtrap03BeginStage" in driver
    assert "StartTimer(WeaselDeathtrapSayTimerLength, WeaselDeathtrapSayTimerID)" in driver
    assert "auiStageID == StopDetonationStage" in driver
    assert "StartTimer(DetectionTimerLength, DetectionTimerID)" in driver
    for terminal in ("LouNoticedStage", "AllBreakersOffStage", "PlayerTalkedToLouStage"):
        assert f"auiStageID == {terminal}" in driver
    assert "CancelTimer(DetectionTimerID)" in driver

    # No unbound stage literals may leak into the driver.
    for literal in ("1510", "1700", "1740", "1745", "1801"):
        assert literal not in driver


def test_201_deathtrap_callback_says_then_advances_to_its_bound_stage():
    timer = _member_body(_patch(CONTROLLER_201), "ontimer")
    say = "weaselActor.Say(W05_MQR_201P_Weasel_Deathtrap03_Comment02"

    assert say in timer
    assert "!IsStageDone(Deathtrap03WeaselStage)" in timer
    assert timer.index(say) < timer.index("SetStage(Deathtrap03WeaselStage)")


def test_201_detection_poll_is_guarded_reentrant_and_terminates():
    timer = _member_body(_patch(CONTROLLER_201), "ontimer")

    # Every terminal stage stops the poll before any detection work happens.
    for terminal in ("LouNoticedStage", "AllBreakersOffStage", "PlayerTalkedToLouStage"):
        assert f"IsStageDone({terminal})" in timer
    assert "louActor == None || playerActor == None || louActor.IsDead()" in timer

    detect = "playerActor.GetDistance(louActor) <= LouDistance && playerActor.IsDetectedBy(louActor)"
    assert detect in timer
    assert "louActor.Say(W05_MQR_201P_LouSaysTopic_SneakFail" in timer
    assert timer.index(detect) < timer.index("SetStage(LouNoticedStage)")

    # The success branch returns instead of re-arming, so the poll cannot fail twice.
    fired = timer.index("SetStage(LouNoticedStage)")
    rearm = timer.index("StartTimer(DetectionTimerLength, DetectionTimerID)")
    assert fired < rearm
    assert timer[fired:rearm].count("Return") == 1


def test_201_shutdown_cancels_every_declared_timer():
    shutdown = _member_body(_patch(CONTROLLER_201), "onquestshutdown")
    for timer_id in ("DetectionTimerID", "WeaselDeathtrapSayTimerID", "SignalTimerID"):
        assert f"CancelTimer({timer_id})" in shutdown


def test_202_controller_binds_each_turret_wave_to_its_paired_target_array():
    patch = _patch(CONTROLLER_202)
    assert set(_member_names(patch)) == {
        "onstageset",
        "ontimer",
        "onquestshutdown",
        "startbossventcycle",
        "beginbosspeek",
        "dropbossventitem",
        "endbosspeek",
        "finishbosspeekcycle",
        "stopbossventcycle",
        "selectnextbossvent",
        "forcecurrentbossmarker",
        "cleardroppeditemalias",
        "clearbossventaliases",
        "clearbossmarker",
        "assignturretwavetargets",
    }

    driver = _member_body(patch, "onstageset")
    assert "auiStageID == 1210" in driver
    assert "AssignTurretWaveTargets(Turrets01Aliases, Turrets01Targets)" in driver
    assert "auiStageID == 1220" in driver
    assert "AssignTurretWaveTargets(Turrets02Aliases, Turrets02Targets)" in driver
    assert set(TURRET_WAVE_STAGES) == {1210, 1220}


def test_202_paired_arrays_are_walked_in_lockstep_and_optional_safe():
    helper = _member_body(_patch(CONTROLLER_202), "assignturretwavetargets")

    assert "akTurretAliases == None || akTargetAliases == None" in helper
    # Lockstep: the shorter array bounds the walk, so a ragged pair cannot
    # index past the end or cross-wire a turret onto the wrong target.
    assert (
        "pairIndex < akTurretAliases.Length && pairIndex < akTargetAliases.Length"
        in helper
    )
    assert helper.count("[pairIndex]") == 4
    assert "turretActor.StartCombat(targetActor, True)" in helper
    assert "!turretActor.IsDead() && !targetActor.IsDead()" in helper
    assert "pairIndex += 1" in helper


def test_202_boss_cycle_uses_bound_timers_and_survives_reentry():
    patch = _patch(CONTROLLER_202)
    start = _member_body(patch, "startbossventcycle")
    timer = _member_body(patch, "ontimer")

    assert start.index("StopBossVentCycle()") < start.index("RaRaDropCount = 0")
    assert "StartTimer(VentTimerInit, RaRaBossVentTimerID)" in start
    assert "IsStageDone(DefeatedBossStage)" in start
    assert "aiTimerID == RaRaBossVentTimerID" in timer
    assert timer.index("IsStageDone(DefeatedBossStage)") < timer.index(
        "SelectNextBossVent()"
    )
    assert "aiTimerID == RaRaItemDropTimeOutTimerID" in timer
    assert timer.index("BossPeekSequence.Stop()") < timer.index(
        "FinishBossPeekCycle()"
    )


def test_202_boss_vent_selection_forces_the_live_dynamic_alias_set():
    patch = _patch(CONTROLLER_202)
    select = _member_body(patch, "selectnextbossvent")
    clear = _member_body(patch, "clearbossventaliases")

    assert "BossVentData[VentIndex]" in select
    assert "VentIndex += 1" in select
    assert "RaRaBossCurrentVent.ForceRefTo(ventRef)" in select
    assert "ForceCurrentBossMarker(83, selectedVent.MarkerA)" in select
    assert "ForceCurrentBossMarker(84, selectedVent.MarkerB)" in select
    assert "ForceCurrentBossMarker(85, selectedVent.MarkerC)" in select
    assert select.index("RaRaBossVentReadyToPeek = True") < select.index(
        "BossPeekSequence.Start()"
    )
    for alias_id in (83, 84, 85):
        assert f"ClearBossMarker({alias_id})" in clear
    assert "RaRaBossCurrentVent.Clear()" in clear


def test_202_drop_uses_the_bound_leveled_list_and_dynamic_alias_lifecycle():
    drop = _member_body(_patch(CONTROLLER_202), "dropbossventitem")

    spawn = "raRaRef.PlaceAtMe(W05_MQR_202P_LL_RaRaDropItemList, 1, False, True, False)"
    timeout = "StartTimer(RaRaItemDropTimerOutTimerLength, RaRaItemDropTimeOutTimerID)"
    assert drop.index(timeout) < drop.index("RaRaDropCount >= RaRaDropCountMax")
    assert "RaRaDropCount >= RaRaDropCountMax" in drop
    assert "RaRaItemToDrop.GetReference() != None" in drop
    assert spawn in drop
    assert drop.index(spawn) < drop.index("RaRaItemToDrop.ForceRefTo(droppedItem)")
    assert drop.index("RaRaItemToDrop.ForceRefTo(droppedItem)") < drop.index(
        "droppedItem.Enable()"
    )
    assert "RaRaDropCount += 1" in drop
    assert "SetStage(ItemDroppedObjective)" in drop
    assert timeout in drop
    assert ".AddItem(" not in drop
    assert "SetObjectiveCompleted" not in drop


def test_202_finish_defeat_and_shutdown_clean_state_without_deleting_drops():
    patch = _patch(CONTROLLER_202)
    finish = _member_body(patch, "finishbosspeekcycle")
    stop = _member_body(patch, "stopbossventcycle")
    stage = _member_body(patch, "onstageset")
    shutdown = _member_body(patch, "onquestshutdown")

    assert "CancelTimer(RaRaItemDropTimeOutTimerID)" in finish
    assert finish.index("ClearDroppedItemAlias()") < finish.index(
        "StartTimer(Utility.RandomInt(VentTimerMin, VentTimerMax), RaRaBossVentTimerID)"
    )
    for timer_id in ("RaRaBossVentTimerID", "RaRaItemDropTimeOutTimerID"):
        assert f"CancelTimer({timer_id})" in stop
    assert "auiStageID == DefeatedBossStage" in stage
    assert "StopBossVentCycle()" in shutdown
    lifecycle = finish + stop + _member_body(patch, "cleardroppeditemalias")
    for destructive_call in (".Delete(", ".Disable(", ".RemoveItem("):
        assert destructive_call not in lifecycle


def test_202_saved_cycle_state_reenters_through_persistent_quest_data():
    patch = _patch(CONTROLLER_202)
    select = _member_body(patch, "selectnextbossvent")
    drop = _member_body(patch, "dropbossventitem")
    timer = _member_body(patch, "ontimer")

    assert "VentIndex" in select
    assert "RaRaBossCurrentVent.ForceRefTo" in select
    assert "RaRaItemToDrop.GetReference() != None" in drop
    assert "RaRaDropCount" in drop
    assert "IsStageDone(DefeatedBossStage)" in timer


def test_202_controller_and_dynamic_alias_receivers_are_registered_as_patched():
    with STATUS.open(encoding="utf-8", newline="") as status_file:
        rows = {row["script_name"]: row for row in csv.DictReader(status_file)}

    for script_name in (
        CONTROLLER_202,
        "W05_MQR_202P_RaRaItemPickedUpScript",
        "W05_MQR_202P_VentMarkerScript",
    ):
        assert rows[script_name]["terminal_state"] == "patched"
        assert rows[script_name]["evidence"] == CLOSURE_CONTRACT


@pytest.mark.parametrize("script_name", CONTROLLERS)
def test_controller_merge_into_the_hollow_skeleton_is_exact_and_idempotent(
    script_name: str,
):
    skeleton = TRACKED_SKELETONS[script_name]
    patch = _patch(script_name)
    merged = _merged(script_name)

    assert _member_names(skeleton) == []
    assert Counter(_member_names(merged)) == Counter(_member_names(patch))
    for member_name in _member_names(patch):
        assert _member_body(merged, member_name) == _member_body(patch, member_name)
    for declaration in (line for line in skeleton.splitlines() if " Property " in line):
        assert declaration in merged
    assert _merge_script_method_patches(merged, patch) == merged


@pytest.mark.parametrize("qf_name", (QF_202, QF_203))
def test_sibling_script_casts_hop_through_the_shared_quest_ancestor(qf_name: str):
    # Papyrus rejects `Self as <sibling script>`; only the upcast-then-downcast
    # form compiles. A regression here silently kills every wave and controller
    # call in the fragment.
    patch = _patch(qf_name)
    for sibling in ("DefaultQuestEncounterWaveScript", "W05_MQR_203P_QuestScript"):
        assert f"= Self as {sibling}" not in patch
    assert "= (Self as Quest) as DefaultQuestEncounterWaveScript" in patch


@pytest.mark.parametrize("script_name", CONTROLLERS)
def test_merged_controller_native_compiles_for_fo4(script_name: str, tmp_path: Path):
    base_source = _fo4_base_source()
    assert base_source is not None, "FO4 base Papyrus sources unavailable"

    result = compile_psc(
        _merged(script_name),
        imports=[str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=f"{script_name}.psc",
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None
