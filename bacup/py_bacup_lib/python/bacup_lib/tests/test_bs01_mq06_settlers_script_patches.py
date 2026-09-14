from __future__ import annotations

from collections import Counter
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
FRAGMENT_SCRIPT = "Fragments:Quests:QF_BS01_MQ06B_Settlers_005D1F89"
ON_ENTER_MINE_SCRIPT = "Quests:BS01_MQ06_Settlers:OnEnterMine"
LIVE_LOCATION_TRANSITIONS = (
    (200, 300, "5A6ED2"),
    (450, 625, "05F800"),
)
LIVE_CLUE_BINDINGS = (
    (55, 400, 410, "005D99CA"),
    (56, 400, 411, "005D99CC"),
    (57, 400, 412, "005D99C6"),
)
BOUND_STAGES = (
    100,
    200,
    201,
    202,
    250,
    300,
    310,
    325,
    350,
    400,
    410,
    411,
    412,
    413,
    421,
    422,
    425,
    430,
    440,
    450,
    600,
    625,
    650,
    700,
    710,
    725,
    726,
    749,
    750,
    775,
    799,
    800,
    801,
    802,
    803,
    804,
    826,
    900,
    910,
    1000,
    1001,
    1002,
    1003,
    1030,
    1040,
    1041,
    1050,
    1075,
    1100,
    1110,
    1150,
    1200,
    1201,
    1202,
    1203,
    1204,
    1205,
    1206,
    1207,
    1211,
    1212,
    1213,
    1250,
    1297,
    1300,
    1400,
    9000,
    9999,
)
BOUND_MEMBERS = {
    f"fragment_stage_{stage:04d}_item_00" for stage in BOUND_STAGES
}
QF_RUNTIME_MEMBERS = {
    "onquestinit",
    "onquestshutdown",
    "actor.onplayerloadgame",
    "registerquestactivations",
    "unregisterquestactivations",
    "tryhandleclueactivation",
    "objectreference.onactivate",
    "trycompletesuccessorhandoff",
    "ontimer",
}


def _member_names(source: str) -> list[str]:
    return [
        name
        for kind, name, _start, _end in _iter_top_level_papyrus_members(
            source.splitlines()
        )
        if kind in {"function", "event"}
    ]


def _member_body(source: str, member_name: str) -> str:
    lines = source.splitlines()
    start, end = next(
        (start, end)
        for kind, name, start, end in _iter_top_level_papyrus_members(lines)
        if kind in {"function", "event"} and name == member_name.casefold()
    )
    return "\n".join(lines[start : end + 1])


def _merged_source(script_name: str) -> str:
    source_path = SOURCE_ROOT / _script_relative_path(script_name, ".psc")
    patch = _script_patch_source(script_name)
    assert source_path.is_file(), source_path
    assert patch is not None
    return _merge_script_method_patches(source_path.read_text(encoding="utf-8"), patch)


def test_fragment_patch_contains_all_68_bound_members_once() -> None:
    patch = _script_patch_source(FRAGMENT_SCRIPT)
    assert patch is not None
    members = _member_names(patch)
    fragment_members = [member for member in members if member.startswith("fragment_")]

    assert len(BOUND_STAGES) == 68
    assert set(fragment_members) == BOUND_MEMBERS
    assert Counter(fragment_members) == Counter(
        {member: 1 for member in BOUND_MEMBERS}
    )
    assert set(members) == BOUND_MEMBERS | QF_RUNTIME_MEMBERS
    assert Counter(members) == Counter(
        {member: 1 for member in BOUND_MEMBERS | QF_RUNTIME_MEMBERS}
    )
    assert not any(
        line.strip().casefold().startswith(("scriptname ", "state "))
        for line in patch.splitlines()
    )
    assert not any(
        " property " in f" {line.strip().casefold()} " for line in patch.splitlines()
    )


@pytest.mark.parametrize("script_name", (FRAGMENT_SCRIPT, ON_ENTER_MINE_SCRIPT))
def test_production_merges_are_unique_and_idempotent(script_name: str) -> None:
    patch = _script_patch_source(script_name)
    assert patch is not None
    merged = _merged_source(script_name)
    merged_members = _member_names(merged)

    for member in _member_names(patch):
        assert merged_members.count(member) == 1
        assert _member_body(merged, member) == _member_body(patch, member)
    assert merged.casefold().count("scriptname ") == 1
    assert _merge_script_method_patches(merged, patch) == merged


def test_on_enter_mine_signature_and_bound_exit_contract() -> None:
    patch = _script_patch_source(ON_ENTER_MINE_SCRIPT)
    assert patch is not None
    assert set(_member_names(patch)) == {
        "onaliasinit",
        "onlocationchange",
        "onplayerloadgame",
        "reconcilequestlocation",
    }
    assert "Event OnLocationChange(Location akOldLoc, Location akNewLoc)" in patch
    assert "Event OnPlayerLoadGame()" in patch
    assert "ReconcileQuestLocation(playerRef.GetCurrentLocation())" in patch
    assert "LocationAlias supplyRoomAlias = Quest_M06.GetAlias(44) as LocationAlias" in patch
    assert "Quest_M06.IsStageDone(200)" in patch
    assert "!Quest_M06.IsStageDone(300)" in patch
    assert patch.count("Quest_M06.SetStage(300)") == 1
    assert "Quest_M06.IsStageDone(450)" in patch
    assert "!Quest_M06.IsStageDone(625)" in patch
    assert patch.count("Quest_M06.SetStage(625)") == 1
    assert "akNewLoc == Loc_Mine" in patch
    assert "playerRef.SetValue(AV_BreadCrumb, 0.0)" in patch
    assert "akOldLoc == Loc_Mine" in patch
    assert "Quest_M06.IsStageDone(Stage_MikeIsDead)" in patch
    assert "!Quest_M06.IsStageDone(Stage_PlayerHasKey)" in patch
    assert "!Quest_M06.IsStageDone(StageToSet_LeftEarly)" in patch
    assert patch.count("Quest_M06.SetStage(StageToSet_LeftEarly)") == 1
    assert "playerRef != Game.GetPlayer()" in patch
    assert not any(
        " property " in f" {line.strip().casefold()} " for line in patch.splitlines()
    )
    assert LIVE_LOCATION_TRANSITIONS == (
        (200, 300, "5A6ED2"),
        (450, 625, "05F800"),
    )


def test_clue_activations_are_quest_scoped_exact_and_save_safe() -> None:
    patch = _script_patch_source(FRAGMENT_SCRIPT)
    assert patch is not None
    quest_init = _member_body(patch, "onquestinit")
    player_load = _member_body(patch, "actor.onplayerloadgame")
    registration = _member_body(patch, "registerquestactivations")
    unregistration = _member_body(patch, "unregisterquestactivations")
    handler = _member_body(patch, "tryhandleclueactivation")
    activation = _member_body(patch, "objectreference.onactivate")
    stage_400 = _member_body(patch, "fragment_stage_0400_item_00")

    assert LIVE_CLUE_BINDINGS == (
        (55, 400, 410, "005D99CA"),
        (56, 400, 411, "005D99CC"),
        (57, 400, 412, "005D99C6"),
    )
    assert "RegisterQuestActivations()" in quest_init
    assert "RegisterQuestActivations()" in player_load
    assert "RegisterQuestActivations()" in stage_400
    assert "UnregisterQuestActivations()" in registration
    terminal_guard = (
        "!IsStageDone(400) || IsStageDone(430) || "
        "IsStageDone(440) || IsStageDone(450)"
    )
    assert terminal_guard in registration
    assert terminal_guard in handler
    assert "playerRef != Game.GetPlayer()" in activation
    assert "akActionRef != playerRef" in activation
    assert "TryHandleClueActivation(akSender)" in activation

    alias_contracts = {
        55: ("RefCollectionAlias bodies", "bodyRef", "bodies.GetAt(index) == akSender"),
        56: ("RefCollectionAlias crates", "crateRef", "crates.GetAt(index) == akSender"),
        # Named explosionAlias, not explosion: stock PapyrusCompiler.exe rejects a
        # local named after the known FO4 script type Explosion.
        57: (
            "ReferenceAlias explosionAlias",
            "explosionAlias.GetReference()",
            "explosionAlias.GetReference() == akSender",
        ),
    }
    for alias_id, prereq_stage, stage_to_set, message_form in LIVE_CLUE_BINDINGS:
        assert prereq_stage == 400
        declaration, registered_ref, sender_match = alias_contracts[alias_id]
        assert f"{declaration} = GetAlias({alias_id})" in registration
        assert f"!IsStageDone({stage_to_set})" in registration
        registered = f'RegisterForRemoteEvent({registered_ref}, "OnActivate")'
        unregistered = f'UnregisterForRemoteEvent({registered_ref}, "OnActivate")'
        assert registered in registration
        assert unregistered in unregistration
        assert sender_match in handler
        message_lookup = (
            f'Game.GetFormFromFile(0x{message_form}, "SeventySix.esm") as Message'
        )
        assert message_lookup in handler
        assert handler.count(f"SetStage({stage_to_set})") == 1

    for terminal_stage in (430, 440, 450):
        body = _member_body(patch, f"fragment_stage_{terminal_stage:04d}_item_00")
        assert "UnregisterQuestActivations()" in body


def test_mike_dispositions_and_hellstorm_cache_handoff_are_distinct() -> None:
    patch = _script_patch_source(FRAGMENT_SCRIPT)
    assert patch is not None
    keeps_weapons = _member_body(patch, "fragment_stage_1000_item_00")
    yields_weapons = _member_body(patch, "fragment_stage_1001_item_00")
    flees = _member_body(patch, "fragment_stage_1002_item_00")
    killed = _member_body(patch, "fragment_stage_1003_item_00")
    cache_ready = _member_body(patch, "fragment_stage_1030_item_00")
    cache_taken = _member_body(patch, "fragment_stage_1050_item_00")
    ready_to_return = _member_body(patch, "fragment_stage_1075_item_00")

    assert "SetValue(AV_MikeDead, 2.0)" in keeps_weapons
    assert "SetStage(1075)" in keeps_weapons
    assert "SetStage(1030)" not in keeps_weapons
    assert "SetValue(AV_MikeDead, 2.0)" in yields_weapons
    assert "SetStage(1030)" in yields_weapons
    assert "SetValue(AV_MikeDead, 1.0)" in flees
    assert "SetStage(1030)" in flees
    assert "SetValue(AV_MikeDead, 1.0)" in killed
    assert "WepKeyAlias.GetReference()" in killed
    assert "WepKeyAlias.ForceRefTo(weaponKeyRef)" in killed
    assert "mikeRef.GetItemCount(WepKeyBaseItem) == 0" in killed
    assert killed.count("mikeRef.AddItem(WepKeyBaseItem, 1, True)") == 1
    assert "SetObjectiveCompleted(115)" in killed

    assert "barredDoorRef.Disable()" in cache_ready
    assert "CacheAlias.GetReference()" in cache_ready
    assert "CacheAlias.ForceRefTo(cacheRef)" in cache_ready
    assert "cacheCrateRef.GetItemCount(CacheBaseItem) == 0" in cache_ready
    assert cache_ready.count("cacheCrateRef.AddItem(CacheBaseItem, 1, True)") == 1
    assert "playerRef.GetItemCount(CacheBaseItem) == 0" in cache_taken
    assert "CacheAlias.GetReference()" in cache_taken
    assert cache_taken.count("playerRef.AddItem(CacheBaseItem, 1, True)") == 1
    assert "SetStage(1075)" in cache_taken
    assert "SetValue(AV_Ready, 1.0)" in ready_to_return
    assert "SetObjectiveDisplayed(130)" in ready_to_return
    assert "SetStage(9999)" in ready_to_return


def test_local_player_and_quest_item_aliases_are_preserved() -> None:
    patch = _script_patch_source(FRAGMENT_SCRIPT)
    assert patch is not None
    start = _member_body(patch, "fragment_stage_0100_item_00")

    assert "Actor playerRef = Game.GetPlayer()" in start
    assert "Alias_Player.ForceRefIfEmpty(playerRef)" in start
    alias_contracts = {
        421: "HolotapeQOAlias",
        430: "PasswordAlias",
        749: "DiveSuit_Suit",
        799: "KeyAlias",
        801: "Valuables01Alias",
        802: "Valuables02Alias",
        803: "Valuables03Alias",
        1003: "WepKeyAlias",
        1030: "CacheAlias",
    }
    for stage, alias in alias_contracts.items():
        body = _member_body(patch, f"fragment_stage_{stage:04d}_item_00")
        assert f"{alias}.GetReference()" in body
        assert f"{alias}.ForceRefTo(" in body


def test_foundation_trade_decisions_reactions_and_caps_are_preserved() -> None:
    patch = _script_patch_source(FRAGMENT_SCRIPT)
    assert patch is not None

    decision_codes = {
        1201: 6,
        1202: 1,
        1203: 4,
        1204: 3,
        1205: 5,
        1206: 2,
        1207: 7,
    }
    for stage, code in decision_codes.items():
        body = _member_body(patch, f"fragment_stage_{stage:04d}_item_00")
        assert body.count(f"SetValue(FoundationDecision, {code}.0)") == 1

    paid = _member_body(patch, "fragment_stage_1201_item_00")
    paid_and_training = _member_body(patch, "fragment_stage_1297_item_00")
    assert paid.count("RemoveItem(Caps001, 1000, True)") == 1
    assert "!IsStageDone(1298)" in paid_and_training
    assert paid_and_training.count("RemoveItem(Caps001, 2500, True)") == 1
    assert paid_and_training.count("SetStage(1298)") == 1

    reactions = {
        1211: (1, "Worried"),
        1212: (2, "Friendly"),
        1213: (3, "Confident"),
    }
    for stage, (code, archetype) in reactions.items():
        body = _member_body(patch, f"fragment_stage_{stage:04d}_item_00")
        assert body.count(f"SetValue(AV_Foundation, {code}.0)") == 1
        assert body.count(f"ChangeAnimFaceArchetype(Face{archetype})") == 2
        assert body.count(f"ChangeAnimArchetype(Anim{archetype})") == 2


def test_items_are_restored_idempotently_without_duplicate_grants() -> None:
    patch = _script_patch_source(FRAGMENT_SCRIPT)
    assert patch is not None
    restore_contracts = {
        421: ("HolotapeBaseItem", "playerRef"),
        430: ("PasswordBaseItem", "playerRef"),
        749: ("DiveSuit_Suit_BaseItem", "crateRef"),
        750: ("DiveSuit_Suit_BaseItem", "playerRef"),
        799: ("KeyBaseItem", "keyCrateRef"),
        800: ("KeyBaseItem", "playerRef"),
        801: ("Valuables01BaseItem", "playerRef"),
        802: ("Valuables02BaseItem", "playerRef"),
        803: ("Valuables03BaseItem", "playerRef"),
        1030: ("CacheBaseItem", "cacheCrateRef"),
        1040: ("WepKeyBaseItem", "playerRef"),
        1050: ("CacheBaseItem", "playerRef"),
    }
    for stage, (item, owner) in restore_contracts.items():
        body = _member_body(patch, f"fragment_stage_{stage:04d}_item_00")
        assert f"{owner}.GetItemCount({item}) == 0" in body
        assert body.count(f"{owner}.AddItem({item}, 1, True)") == 1

    holotape = _member_body(patch, "fragment_stage_0421_item_00")
    assert "EvidenceAlias.GetReference()" in holotape
    assert "playerRef.GetItemCount(EvidenceBaseItem) == 0" in holotape
    assert "EvidenceAlias.ForceRefTo(evidenceRef)" in holotape
    assert holotape.count("playerRef.AddItem(EvidenceBaseItem, 1, True)") == 1

    valuables = "\n".join(
        _member_body(patch, f"fragment_stage_{stage:04d}_item_00")
        for stage in (801, 802, 803)
    )
    assert valuables.count("ModValue(AV_ValuablesCurrent, 1.0)") == 3
    assert valuables.count("!IsStageDone(804)") == 3


def test_mike_bag_branch_is_quest_scoped_and_exact() -> None:
    patch = _script_patch_source(FRAGMENT_SCRIPT)
    assert patch is not None
    quest_init = _member_body(patch, "onquestinit")
    player_load = _member_body(patch, "actor.onplayerloadgame")
    activation = _member_body(patch, "objectreference.onactivate")
    stage_400 = _member_body(patch, "fragment_stage_0400_item_00")

    assert "RegisterQuestActivations()" in quest_init
    assert "RegisterQuestActivations()" in player_load
    assert "RegisterQuestActivations()" in stage_400
    unregister_player = 'UnregisterForRemoteEvent(playerRef, "OnPlayerLoadGame")'
    register_player = 'RegisterForRemoteEvent(playerRef, "OnPlayerLoadGame")'
    assert quest_init.index(unregister_player) < quest_init.index(register_player)
    assert player_load.index(
        'UnregisterForRemoteEvent(akSender, "OnPlayerLoadGame")'
    ) < player_load.index('RegisterForRemoteEvent(akSender, "OnPlayerLoadGame")')
    assert "GetAlias(82) as ReferenceAlias" in patch
    assert "playerRef != Game.GetPlayer()" in activation
    assert "akActionRef != playerRef" in activation
    assert "!IsStageDone(400)" in activation
    assert "IsStageDone(430)" in activation
    assert "IsStageDone(440)" in activation
    assert "IsStageDone(450)" in activation
    assert 'Game.GetFormFromFile(0x005DB4F3, "SeventySix.esm") as Message' in activation
    assert 'Game.GetFormFromFile(0x005DB4F2, "SeventySix.esm") as Message' in activation
    assert 'Game.GetFormFromFile(0x005DB4F4, "SeventySix.esm") as Message' in activation
    assert "playerRef.GetItemCount(EvidenceBaseItem) > 0" in activation
    assert activation.count("SetStage(430)") == 1
    assert activation.count("SetStage(413)") == 1
    assert "UnregisterQuestActivations()" in activation


def test_supply_room_scene_and_over_and_out_handoff_are_guarded() -> None:
    patch = _script_patch_source(FRAGMENT_SCRIPT)
    assert patch is not None
    supply_room = _member_body(patch, "fragment_stage_1110_item_00")
    completion = _member_body(patch, "fragment_stage_9000_item_00")
    handoff = _member_body(patch, "trycompletesuccessorhandoff")
    timer = _member_body(patch, "ontimer")
    player_load = _member_body(patch, "actor.onplayerloadgame")

    assert "playerRef.GetValue(AV_MikeDead) == 2.0" in supply_room
    assert "scene_MikeTalk != None" in supply_room
    assert "!scene_MikeTalk.IsPlaying()" in supply_room
    assert supply_room.count("scene_MikeTalk.Start()") == 1

    story_event = "BS01_MQ07_Over_StartKeyword.SendStoryEventAndWait(None, playerRef, playerRef)"
    assert "CompleteQuest()" in completion
    assert completion.count("TryCompleteSuccessorHandoff()") == 1
    assert "Stop()" not in completion
    assert "SendStoryEvent(" not in completion

    assert "BS01_MQ07_Over != None" in handoff
    assert "BS01_MQ07_Over.IsRunning()" in handoff
    assert "BS01_MQ07_Over.IsCompleted()" in handoff
    assert handoff.count(story_event) == 1
    assert "If accepted" in handoff
    assert handoff.count("Stop()") == 1
    assert handoff.count("StartTimer(5.0, 9000)") == 1
    assert ".Start()" not in handoff

    assert "aiTimerID == 9000" in timer
    assert "IsStageDone(9000)" in timer
    assert timer.count("TryCompleteSuccessorHandoff()") == 1
    assert "IsStageDone(9000)" in player_load
    assert player_load.count("TryCompleteSuccessorHandoff()") == 1


@pytest.mark.parametrize("script_name", (FRAGMENT_SCRIPT, ON_ENTER_MINE_SCRIPT))
def test_full_production_merge_native_compiles_for_fo4(script_name: str) -> None:
    base_source = _fo4_base_source()
    assert base_source is not None, "FO4 base Papyrus sources unavailable"

    result = compile_psc(
        _merged_source(script_name),
        imports=[str(SOURCE_ROOT), str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=str(_script_relative_path(script_name, ".psc")),
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None
