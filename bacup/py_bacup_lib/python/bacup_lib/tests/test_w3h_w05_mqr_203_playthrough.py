from __future__ import annotations

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


QF = "Fragments:Quests:QF_W05_MQR_203P_0042F31B"
CONTROLLER = "W05_MQR_203P_QuestScript"
BENCH = "W05_MQR_203P_BenchScript"
BLACKOUT = "W05_MQR_203P_WinnersCupBlackOutScript"
PORTAL = "W05_MQR_203P_DoorPortalScript"
JOHNNY_REGISTRATION_TRAVEL = "Fragments:Packages:PF_W05_MQR_203P_0200_Johnny__00593DC7"
JOHNNY_PLAN_B_APPROACH = "Fragments:Packages:PF_W05_MQR_203P_Johnny_GetCl_005A1F24"
PACKAGE_PRODUCERS = (JOHNNY_REGISTRATION_TRAVEL, JOHNNY_PLAN_B_APPROACH)
SCRIPTS = (
    QF,
    CONTROLLER,
    BENCH,
    BLACKOUT,
    PORTAL,
    *PACKAGE_PRODUCERS,
)

TRACKED_SKELETONS = {
    QF: """Scriptname Fragments:Quests:QF_W05_MQR_203P_0042F31B Extends Quest hidden

ReferenceAlias Property Alias_DisabledForQuestEnableMarker Auto mandatory
ReferenceAlias Property Alias_EnabledForQuestEnableMarker Auto mandatory
ReferenceAlias Property ArenaEntranceMarker Auto mandatory
ReferenceAlias Property Alias_EntranceDoor Auto mandatory
ReferenceAlias Property Alias_currentPlayer Auto mandatory
ActorValue Property pW05_MQR_JohnnyRelationshipValue Auto mandatory
ActorValue Property W05_MQR_203P_DignityValue Auto mandatory
Scene Property W05_MQR_203P_Johnny_002B_RegistrationScene Auto mandatory
Scene Property W05_MQR_203P_Johnny_002C_RegistrationScene Auto mandatory
Scene Property W05_MQR_203P_SargentoPA_002_Round01GhouldenBoy Auto mandatory
Scene Property W05_MQR_203P_SargentoPA_003_Round01CallPlayer Auto mandatory
Scene Property W05_MQR_203P_GuardArena_001_Round01 Auto mandatory
Scene Property W05_MQR_203P_SargentoPA_004_Round01PlayerArena Auto mandatory
Scene Property W05_MQR_203P_SargentoPA_004B_Round01End Auto
Scene Property W05_MQR_203P_Johnny_EnterLockerRoom Auto mandatory
ReferenceAlias Property Alias_ArenaGuardPostMarker Auto mandatory
ReferenceAlias Property Alias_Don Auto mandatory
ReferenceAlias Property Alias_GuardArena Auto mandatory
ReferenceAlias Property Alias_GuardArenaName Auto mandatory
Scene Property W05_MQR_203P_SargentoPA_001_Idle Auto mandatory
Scene Property W05_MQR_203P_SargentoPA_005_Round02CallPlayer Auto mandatory
Scene Property W05_MQR_203P_SargentoPA_006_Round02PlayerArena Auto mandatory
Scene Property W05_MQR_203P_SargentoPA_006B_Round02End Auto mandatory
ActorValue Property W05_MQR_203P_Round2CheatValue Auto mandatory
ActorValue Property W05_MQR_203P_Round3CheatValue Auto mandatory
RefCollectionAlias Property Alias_NeutralTurrets Auto mandatory
RefCollectionAlias Property Alias_AllyTurrets Auto mandatory
Scene Property W05_MQR_203P_SargentoPA_011_Turrets Auto mandatory
ReferenceAlias Property Alias_Klaus Auto mandatory
Scene Property W05_MQR_203P_SargentoPA_007_Round03CallPlayer Auto mandatory
Scene Property W05_MQR_203P_SargentoPA_008_Round03PlayerArena Auto mandatory
Scene Property W05_MQR_203P_SargentoPA_009A_Winner Auto mandatory
ReferenceAlias Property Alias_Maddie Auto mandatory
ActorValue Property W05_MQR_203P_SaleValue Auto mandatory
Scene Property W05_MQR_203P_Johnny_006_EnterArena Auto mandatory
Key Property W05_MQR_203P_HalRoomKey Auto mandatory
Scene Property W05_MQR_203P_JohnnySargento_001_Winner Auto mandatory
Spell Property W05_MQR_203P_WinnersCup_Blackout Auto
Scene Property W05_MQR_203P_HalJohnny_001_Shoot Auto mandatory
Scene Property W05_MQR_203P_Johnny_TravelToHal Auto
ReferenceAlias Property Alias_Hal Auto mandatory
ReferenceAlias Property Alias_JohnnyArena Auto mandatory
ReferenceAlias Property Alias_Sargento Auto mandatory
ActorValue Property W05_MQR_JohnnyCutValue Auto mandatory
Scene Property W05_MQR_203P_JohnnyGuard_001_Distract Auto mandatory
Scene Property W05_MQR_203P_JohnnyMaddie_001_Convince Auto mandatory
Scene Property W05_MQR_203P_SargentoPA_010_PlanB Auto mandatory
Scene Property W05_MQR_203P_SargentoPA_009B_Loser Auto mandatory
ReferenceAlias Property Alias_Player Auto
Armor Property Collar Auto
ActorValue Property Reputation_AV_Crater Auto mandatory
GlobalVariable Property Rep_Mod_Add_MQ Auto mandatory
Keyword Property W05_MQR_Choice_QuestStartKeyword Auto mandatory
""",
    CONTROLLER: """Scriptname W05_MQR_203P_QuestScript Extends Quest
""",
    BENCH: """Scriptname W05_MQR_203P_BenchScript Extends ReferenceAlias

Spell Property W05_MQR_203P_PassTimeSpell Auto mandatory
""",
    BLACKOUT: """Scriptname W05_MQR_203P_WinnersCupBlackOutScript Extends ActiveMagicEffect

ImageSpaceModifier Property FadeToBlack Auto mandatory
Quest Property W05_MQR_203P Auto mandatory
ImageSpaceModifier Property WakeUp Auto mandatory
""",
    PORTAL: """Scriptname W05_MQR_203P_DoorPortalScript Extends ReferenceAlias

ReferenceAlias Property Alias_Destination Auto mandatory
""",
    JOHNNY_REGISTRATION_TRAVEL: """Scriptname Fragments:Packages:PF_W05_MQR_203P_0200_Johnny__00593DC7 Extends Package hidden
""",
    JOHNNY_PLAN_B_APPROACH: """Scriptname Fragments:Packages:PF_W05_MQR_203P_Johnny_GetCl_005A1F24 Extends Package hidden
""",
}

TRACKED_CONTROLLER_IMPORT = """Scriptname W05_MQR_203P_QuestScript Extends Quest

ReferenceAlias Property GraftonCageSequenceActivator Auto mandatory
ReferenceAlias Property Round03GraftonMonster Auto mandatory
ReferenceAlias Property SlaveQuartersMarker Auto mandatory
RefCollectionAlias Property PlanBAttackers Auto mandatory
RefCollectionAlias Property Guards Auto mandatory
"""

TRACKED_WAVE_IMPORT = """Scriptname DefaultQuestEncounterWaveScript Extends Quest

Function StartLocalEncounterWave(Int aiWaveIndex)
EndFunction
"""

LIVE_BOUND_STAGES = {
    2,
    100,
    200,
    300,
    400,
    405,
    410,
    420,
    499,
    500,
    510,
    550,
    600,
    605,
    610,
    700,
    710,
    800,
    810,
    815,
    820,
    822,
    900,
    901,
    910,
    950,
    1000,
    1050,
    1100,
    1110,
    1200,
    1205,
    1210,
    1215,
    1220,
    1222,
    1300,
    1301,
    1310,
    1350,
    1400,
    1450,
    1500,
    1510,
    1600,
    1601,
    1602,
    1605,
    1610,
    1700,
    1703,
    1705,
    1706,
    1710,
    1715,
    1720,
    1750,
    1800,
    1900,
    2000,
    2010,
    2100,
    2110,
    2200,
    2210,
    2220,
    2221,
    2300,
    2400,
    5000,
    5100,
    5200,
    5300,
    6000,
    7000,
    7100,
    7110,
    7120,
    7200,
    7300,
    8000,
    8100,
    8110,
    8200,
    8210,
    8220,
    8230,
    8250,
    9000,
    9001,
    9998,
    9999,
    10000,
}
LIVE_BOUND_MEMBERS = {
    f"fragment_stage_{stage:04d}_item_00" for stage in LIVE_BOUND_STAGES
} | {
    "fragment_stage_0800_item_01",
    "fragment_stage_1200_item_01",
}
EXPECTED_PATCH_STAGES = {
    2,
    100,
    200,
    300,
    400,
    405,
    410,
    420,
    499,
    500,
    510,
    550,
    600,
    605,
    610,
    700,
    710,
    800,
    810,
    815,
    820,
    822,
    900,
    901,
    910,
    950,
    1000,
    1050,
    1100,
    1110,
    1200,
    1205,
    1210,
    1215,
    1220,
    1222,
    1300,
    1301,
    1310,
    1350,
    1400,
    1450,
    1500,
    1510,
    1600,
    1601,
    1602,
    1605,
    1610,
    1700,
    1703,
    1705,
    1706,
    1710,
    1715,
    1720,
    1750,
    1800,
    1900,
    2000,
    2010,
    2100,
    2110,
    2200,
    2210,
    2220,
    2221,
    2300,
    2400,
    5000,
    5100,
    5200,
    5300,
    6000,
    7000,
    7100,
    7110,
    7120,
    7200,
    7300,
    8000,
    8100,
    8110,
    8200,
    8210,
    8220,
    8230,
    8250,
    9000,
    9001,
    9998,
    9999,
    10000,
}
EXPECTED_PATCH_MEMBERS = {
    f"fragment_stage_{stage:04d}_item_00" for stage in EXPECTED_PATCH_STAGES
} | {
    "fragment_stage_0800_item_01",
    "fragment_stage_1200_item_01",
}


def _patch(script_name: str) -> str:
    patch = _script_patch_source(script_name)
    assert patch is not None
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


def _stage_body(stage: int, item: int = 0) -> str:
    return _member_body(_patch(QF), f"fragment_stage_{stage:04d}_item_{item:02d}")


def _merged_tracked_source(script_name: str) -> str:
    return _merge_script_method_patches(
        TRACKED_SKELETONS[script_name], _patch(script_name)
    )


def test_qf_members_cover_every_live_vmad_binding():
    qf = _patch(QF)
    assert qf.splitlines().count("; TODO") == 0
    assert _patch(BENCH).splitlines().count("; TODO") == 0
    assert _patch(BLACKOUT).splitlines().count("; TODO") == 0
    assert _patch(PORTAL).splitlines().count("; TODO") == 0

    members = set(_member_names(qf))
    assert len(LIVE_BOUND_MEMBERS) == 95
    assert len(EXPECTED_PATCH_MEMBERS) == 95
    assert members == EXPECTED_PATCH_MEMBERS
    assert EXPECTED_PATCH_MEMBERS == LIVE_BOUND_MEMBERS


def test_instance_registration_and_round_producers_are_connected():
    quest_init = _member_body(_patch(CONTROLLER), "onquestinit")
    assert "IsStageDone(2)" in quest_init
    assert "SetStage(2)" in quest_init
    assert "IsStageDone(100)" in quest_init
    assert "SetStage(100)" in quest_init
    assert quest_init.index("SetStage(2)") < quest_init.index("SetStage(100)")

    init = _stage_body(2)
    for effect in (
        "disabledMarker.Disable()",
        "enabledMarker.Enable()",
        "entranceMarker.Enable()",
        "entranceDoor.Enable()",
    ):
        assert effect in init
    assert "SetStage(300)" not in init

    assert "SetStage(405)" in _stage_body(400)
    assert "W05_MQR_203P_Johnny_002B_RegistrationScene.Start()" in _stage_body(405)
    assert "W05_MQR_203P_Johnny_002C_RegistrationScene.Start()" in _stage_body(499)
    assert "W05_MQR_203P_SargentoPA_002_Round01GhouldenBoy.Start()" in _stage_body(550)
    assert "W05_MQR_203P_SargentoPA_003_Round01CallPlayer.Start()" in _stage_body(605)

    bench = _member_body(_patch(BENCH), "onactivate")
    assert "W05_MQR_203P_PassTimeSpell.Cast" in bench
    for current_stage, next_stage in ((600, 605), (1050, 1100), (1450, 1500)):
        assert f"currentStage == {current_stage}" in bench
        assert f"owningQuest.SetStage({next_stage})" in bench
        assert bench.index("W05_MQR_203P_PassTimeSpell.Cast") < bench.index(
            f"owningQuest.SetStage({next_stage})"
        )

    portal = _member_body(_patch(PORTAL), "onactivate")
    assert portal.index("Alias_Destination == None") < portal.index(
        "Alias_Destination.GetReference()"
    )
    assert portal.index("destinationRef != None") < portal.index(
        "akActionRef.MoveTo(destinationRef)"
    )

    round_one_wave = _stage_body(800, 1)
    assert "StartLocalEncounterWave(0)" in round_one_wave
    assert "SetStage(810)" not in round_one_wave
    assert "W05_MQR_203P_SargentoPA_004B_Round01End.Start()" in _stage_body(810)
    assert "W05_MQR_203P_SargentoPA_005_Round02CallPlayer.Start()" in _stage_body(1100)
    round_two_wave = _stage_body(1200, 1)
    assert "StartLocalEncounterWave(1)" in round_two_wave
    assert "SetStage(1210)" not in round_two_wave
    assert "W05_MQR_203P_SargentoPA_006B_Round02End.Start()" in _stage_body(1210)
    assert "W05_MQR_203P_SargentoPA_007_Round03CallPlayer.Start()" in _stage_body(1500)


def test_stripped_package_end_producers_connect_registration_and_plan_b():
    registration = _member_body(_patch(JOHNNY_REGISTRATION_TRAVEL), "fragment_end")
    assert "owningQuest.IsStageDone(200)" in registration
    assert "!owningQuest.IsStageDone(300)" in registration
    assert registration.count("owningQuest.SetStage(300)") == 1

    approach = _member_body(_patch(JOHNNY_PLAN_B_APPROACH), "fragment_end")
    assert "owningQuest.IsStageDone(8210)" in approach
    assert "!owningQuest.IsStageDone(8220)" in approach
    assert approach.count("owningQuest.SetStage(8220)") == 1

    for script_name in PACKAGE_PRODUCERS:
        assert _member_names(_patch(script_name)) == ["fragment_end"]


def test_final_round_local_effects_and_bounded_blackout_substitute():
    final_round = _stage_body(1600)
    assert "W05_MQR_203P_SargentoPA_008_Round03PlayerArena.Start()" in final_round

    cage = _stage_body(1601)
    # Papyrus rejects a direct sibling-script cast; the hop through the shared
    # Quest ancestor is what the stock FO4 compiler accepts.
    assert "(Self as Quest) as W05_MQR_203P_QuestScript" in cage
    assert "questScript.GraftonCageSequenceActivator.GetReference()" in cage
    assert "cageActivator.Activate(playerRef)" in cage
    assert cage.index("cageActivator.Activate(playerRef)") < cage.index(
        "SetStage(1602)"
    )
    assert "graftonRef.StartCombat(playerRef, True)" in _stage_body(1602)
    assert "SetStage(1700)" in _stage_body(1610)
    assert "W05_MQR_203P_SargentoPA_009A_Winner.Start()" in _stage_body(1700)
    assert "maddieRef.Kill()" in _stage_body(1705)
    assert "SetStage(1715)" in _stage_body(1705)
    assert "W05_MQR_203P_Johnny_006_EnterArena.Start()" in _stage_body(1715)

    key = _stage_body(1720)
    assert "AddItem(W05_MQR_203P_HalRoomKey" in key
    assert "SetStage(1750)" in key
    assert "W05_MQR_203P_JohnnySargento_001_Winner.Start()" in _stage_body(1750)
    assert "W05_MQR_203P_WinnersCup_Blackout.Cast" in _stage_body(1800)

    blackout = _member_body(_patch(BLACKOUT), "oneffectfinish")
    assert "WakeUp.Apply(1.0)" in blackout
    assert blackout.index("akTarget.MoveTo(destinationRef)") < blackout.index(
        "WakeUp.Apply(1.0)"
    )
    assert blackout.index("WakeUp.Apply(1.0)") < blackout.index(
        "W05_MQR_203P.SetStage(1900)"
    )

    assert "W05_MQR_203P_HalJohnny_001_Shoot.Start()" in _stage_body(2100)
    assert "halRef.Kill(johnnyRef)" in _stage_body(2110)
    assert "SetStage(2200)" in _stage_body(2110)
    for stage in (2210, 2220, 2221):
        assert "SetStage(9000)" in _stage_body(stage)

    handoff = _stage_body(9000)
    assert "W05_MQR_Choice_QuestStartKeyword.SendStoryEvent" in handoff
    assert ".Start()" not in handoff
    assert "Stop()" in handoff


def test_choice_cheat_plan_b_and_cleanup_routes_converge():
    assert "SetStage(1000)" in _stage_body(950)
    assert "SetStage(1050)" in _stage_body(1000)
    assert "SetStage(1400)" in _stage_body(1350)
    assert "SetStage(1450)" in _stage_body(1400)

    assert "SetStage(5100)" in _stage_body(5000)
    assert "W05_MQR_203P_JohnnyGuard_001_Distract.Start()" in _stage_body(5000)
    for stage in (5200, 5300):
        body = _stage_body(stage)
        assert "SetStage(910)" in body
        assert "SetStage(1310)" in body
    turret_choice = _stage_body(6000)
    assert "W05_MQR_203P_Round2CheatValue" in turret_choice
    assert "W05_MQR_203P_Round3CheatValue" in turret_choice
    assert "SetStage(7100)" in _stage_body(7000)
    assert "W05_MQR_203P_JohnnyMaddie_001_Convince.Start()" in _stage_body(7110)
    assert "Alias_GuardArenaName.ForceRefTo(guardRef)" in _stage_body(7120)
    assert "SetStage(7200)" not in _stage_body(7120)
    for stage in (7200, 7300):
        assert "SetStage(1310)" in _stage_body(stage)

    plan_b_start = _stage_body(8000)
    assert "W05_MQR_203P_SargentoPA_010_PlanB.Start()" in plan_b_start
    assert "SetStage(8100)" not in plan_b_start
    plan_b_combat = _stage_body(8100)
    assert "questScript.Guards.GetAt" in plan_b_combat
    assert "questScript.PlanBAttackers.AddRef(guardRef)" in plan_b_combat
    assert "attackerRef.StartCombat(playerRef, True)" in plan_b_combat

    first_plan_b_arrival = _stage_body(8200)
    second_plan_b_arrival = _stage_body(8230)
    assert "IsStageDone(8230)" in first_plan_b_arrival
    assert "SetStage(8240)" in first_plan_b_arrival
    assert "SetStage(8210)" not in first_plan_b_arrival
    assert "johnnyRef.EvaluatePackage()" in _stage_body(8210)
    assert "SetStage(8220)" not in _stage_body(8210)
    assert "W05_MQR_203P_Johnny_TravelToHal.Start()" in _stage_body(8220)
    assert "SetStage(8230)" not in _stage_body(8220)
    assert "IsStageDone(8200)" in second_plan_b_arrival
    assert "SetStage(8240)" in second_plan_b_arrival
    assert "W05_MQR_203P_HalJohnny_001_Shoot.Start()" in _stage_body(8250)

    assert "SetStage(8000)" in _stage_body(9001)

    cleanup = _stage_body(9998)
    assert "Alias_Player.GetActorReference()" in cleanup
    assert "playerRef.UnequipItem(Collar, False, True)" in cleanup

    final_cleanup = _stage_body(10000)
    assert "playerRef.RemoveItem(Collar" in final_cleanup
    assert "disabledMarker.Enable()" in final_cleanup
    assert "enabledMarker.Disable()" in final_cleanup
    assert "SetPlayerTeammate(False, False, False)" in final_cleanup
    assert "SetStage(10000)" not in _stage_body(9999)
    assert "Stop()" in _stage_body(9999)

    combined = "\n".join(_patch(script_name) for script_name in SCRIPTS)
    for forbidden in (
        "CompleteQuest(",
        "W05_MQR_204P_QuestStart_Keyword.SendStoryEvent",
    ):
        assert forbidden not in combined


@pytest.mark.parametrize("script_name", SCRIPTS)
def test_tracked_zero_member_merge_is_exact_unique_and_idempotent(script_name: str):
    skeleton = TRACKED_SKELETONS[script_name]
    patch = _patch(script_name)
    merged = _merged_tracked_source(script_name)
    assert _member_names(skeleton) == []
    assert Counter(_member_names(merged)) == Counter(_member_names(patch))
    for member_name in _member_names(patch):
        assert _member_body(merged, member_name) == _member_body(patch, member_name)
    for declaration in (line for line in skeleton.splitlines() if " Property " in line):
        assert declaration in merged
    assert _merge_script_method_patches(merged, patch) == merged


@pytest.mark.parametrize("script_name", SCRIPTS)
def test_full_tracked_merge_native_compiles_for_fo4(script_name: str, tmp_path: Path):
    base_source = _fo4_base_source()
    assert base_source is not None, "FO4 base Papyrus sources unavailable"

    import_root = tmp_path / "imports"
    import_root.mkdir()
    (import_root / "W05_MQR_203P_QuestScript.psc").write_text(
        TRACKED_CONTROLLER_IMPORT, encoding="utf-8"
    )
    (import_root / "DefaultQuestEncounterWaveScript.psc").write_text(
        TRACKED_WAVE_IMPORT, encoding="utf-8"
    )

    result = compile_psc(
        _merged_tracked_source(script_name),
        imports=[str(base_source), str(import_root)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=f"{script_name.replace(':', '/')}.psc",
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None
