from __future__ import annotations

from pathlib import Path

from bacup_lib.workflows.unified import (
    _iter_papyrus_states,
    _iter_top_level_papyrus_members,
    _merge_script_method_patches,
    _script_patch_source,
    _script_relative_path,
)


REPO_ROOT = Path(__file__).resolve().parents[5]
SOURCE_ROOT = REPO_ROOT / "mods" / "SeventySix" / "Scripts" / "Source" / "User"
QUEST_SCRIPT = "Fragments:Quests:QF_BS02_MQ04_Conscience_005F3CF5"
ACTIVATOR_SCRIPT = "Quests:BS02_MQ04_Conscience:ActivatorStartScene"
FURNITURE_SCRIPT = "Quests:BS02_MQ04_Conscience:SetStageOnExitFurniture"

QUEST_MEMBER_NAMES = (
    "fragment_stage_0010_item_00",
    "fragment_stage_0050_item_00",
    "fragment_stage_0100_item_00",
    "fragment_stage_0150_item_00",
    "fragment_stage_0160_item_00",
    "fragment_stage_0200_item_00",
    "fragment_stage_0300_item_00",
    "fragment_stage_0400_item_00",
    "fragment_stage_0450_item_00",
    "fragment_stage_0500_item_00",
    "fragment_stage_0600_item_00",
    "fragment_stage_0700_item_00",
    "fragment_stage_0710_item_00",
    "fragment_stage_0750_item_00",
    "fragment_stage_0800_item_00",
    "fragment_stage_0900_item_00",
    "fragment_stage_0930_item_00",
    "fragment_stage_0950_item_00",
    "fragment_stage_0954_item_00",
    "fragment_stage_0955_item_00",
    "fragment_stage_0960_item_00",
    "fragment_stage_0965_item_00",
    "fragment_stage_0981_item_00",
    "fragment_stage_0982_item_00",
    "fragment_stage_0983_item_00",
    "fragment_stage_1000_item_00",
    "fragment_stage_1050_item_00",
    "fragment_stage_1100_item_00",
    "fragment_stage_1110_item_00",
    "fragment_stage_1150_item_00",
    "fragment_stage_1151_item_00",
    "fragment_stage_1200_item_00",
    "fragment_stage_1240_item_00",
    "fragment_stage_1250_item_00",
    "fragment_stage_1300_item_00",
    "fragment_stage_1400_item_00",
    "fragment_stage_1500_item_00",
    "fragment_stage_1550_item_00",
    "fragment_stage_1560_item_00",
    "fragment_stage_1600_item_00",
    "fragment_stage_1600_item_01",
    "fragment_stage_1605_item_00",
    "fragment_stage_1620_item_00",
    "fragment_stage_1650_item_00",
    "fragment_stage_1660_item_00",
    "fragment_stage_1670_item_00",
    "fragment_stage_1700_item_00",
    "fragment_stage_1800_item_00",
    "fragment_stage_1850_item_00",
    "fragment_stage_1855_item_00",
    "fragment_stage_1860_item_00",
    "fragment_stage_1870_item_00",
    "fragment_stage_1875_item_00",
    "fragment_stage_1900_item_00",
    "fragment_stage_1900_item_01",
    "fragment_stage_1900_item_02",
    "fragment_stage_1900_item_03",
    "fragment_stage_1900_item_04",
    "fragment_stage_1940_item_00",
    "fragment_stage_1940_item_01",
    "fragment_stage_1950_item_00",
    "fragment_stage_1950_item_01",
    "fragment_stage_1952_item_00",
    "fragment_stage_1960_item_00",
    "fragment_stage_1960_item_01",
    "fragment_stage_1970_item_00",
    "fragment_stage_1980_item_00",
    "fragment_stage_1990_item_00",
    "fragment_stage_1995_item_00",
    "fragment_stage_2000_item_00",
    "fragment_stage_2010_item_00",
    "fragment_stage_2100_item_00",
    "fragment_stage_2200_item_00",
    "fragment_stage_2210_item_00",
    "fragment_stage_2300_item_00",
    "fragment_stage_2301_item_00",
    "fragment_stage_2302_item_00",
    "fragment_stage_2303_item_00",
    "fragment_stage_2304_item_00",
    "fragment_stage_2310_item_00",
    "fragment_stage_2400_item_00",
    "fragment_stage_2470_item_00",
    "fragment_stage_3000_item_00",
    "fragment_stage_9000_item_00",
)

QUEST_PROPERTY_TYPES = {
    "Alias_Player": "referencealias",
    "Alias_Door_TestChamber_01": "referencealias",
    "Alias_Actor_Woods": "referencealias",
    "Alias_Marker_Valdez_TeleportOffice": "referencealias",
    "ValdezAmbient_Reactor": "scene",
    "Alias_Activator_TempPuzzle": "referencealias",
    "Alias_Terminal_ReactorControlDoor": "referencealias",
    "PuzzleSolution": "actorvalue",
    "Alias_Door_TestChamber_02": "referencealias",
    "Alias_Loc_Vault96Exterior": "locationalias",
    "Alias_Holo_BlackburnLogs_Research_05": "referencealias",
    "Alias_Actors_TallysBloodEagles": "refcollectionalias",
    "Alias_EM_Blackburn": "referencealias",
    "PlayerAlly": "faction",
    "Alias_Door_TestChamber_02_Utility": "referencealias",
    "BlackburnAmbient_ResearchLab": "scene",
    "Alias_Cardreader_Overseer": "referencealias",
    "Alias_Marker_Rahmani_Intro": "referencealias",
    "Tally_IntercomScene": "scene",
    "Alias_Door_TestChamber_01_Utility": "referencealias",
    "Alias_Door_TestChamber_03": "referencealias",
    "Alias_Door_Overseer": "referencealias",
    "BS01_RahmaniAwayValue": "actorvalue",
    "Alias_Marker_Valdez_Office": "referencealias",
    "Alias_Marker_Valdez_Intercom": "referencealias",
    "Valdez_PostFight_Scene": "scene",
    "Alias_Marker_Valdez_Cassie": "referencealias",
    "Alias_Marker_Valdez_CassieWindow": "referencealias",
    "Alias_Actor_ValdezFollower": "referencealias",
    "Valdez_PuzzleApproach": "scene",
    "Vault96": "location",
    "Alias_Loc_Vault96Interior": "locationalias",
    "Alias_Furniture_WallSit": "referencealias",
    "ValdezAwayValue": "actorvalue",
    "Alias_Enemies_NonQuestEncounters": "refcollectionalias",
    "Alias_EM_ResearchEnemies": "referencealias",
    "Alias_EM_CryoEnemies": "referencealias",
    "Alias_EM_ReactorEnemies": "referencealias",
    "Alias_EM_MainframeEnemies": "referencealias",
    "Alias_Terminal_EntranceTerminal02": "referencealias",
    "Alias_Dummy_EntranceTerminal": "referencealias",
    "Alias_Holo_BlackburnLogs_Research_02": "referencealias",
    "Alias_Terminal_EntranceDoor": "referencealias",
    "Alias_EM_Atrium_BloodEagles": "referencealias",
    "Alias_EM_Atrium_NonQuestCombat": "referencealias",
    "Alias_Furniture_StairsSit": "referencealias",
    "Alias_Actor_Blackburn": "referencealias",
    "BlackburnOffice_Scene": "scene",
    "Alias_EM_HellcatCorpses": "referencealias",
    "Alias_Marker_Hydraulics_FX": "referencealias",
    "WaterExplosion": "explosion",
    "Alias_Door_TestChamber_04_Utility": "referencealias",
    "Alias_Door_TestChamber_04": "referencealias",
    "Alias_Door_TestChamber_03_Utility": "referencealias",
    "Alias_Enemy_AtriumBloodEagle": "referencealias",
    "Alias_EM_ValdezDungeon": "referencealias",
    "ValdezDungeon_AV": "actorvalue",
    "Valdez_EntranceDoorScene": "scene",
    "WoodsPresent_AV": "actorvalue",
    "Alias_Actor_Rahmani": "referencealias",
    "Alias_Door_ReactorControl": "referencealias",
    "Alias_EM_VaultDoors": "referencealias",
    "Alias_Door_CryoBay": "referencealias",
    "Alias_EM_TallyLang": "referencealias",
    "Alias_Actor_TallyLang": "referencealias",
    "Alias_Enemies_Cryobots": "refcollectionalias",
    "Alias_Key_SecurityKeycard": "referencealias",
    "SecurityKeycard": "key",
    "TallyGaveKeycard": "actorvalue",
    "Alias_Doors_TestChamber": "refcollectionalias",
    "CassieGreetScene": "scene",
    "DiseaseCureAntibiotics": "potion",
    "Alias_Actor_Cassie": "referencealias",
    "CassieShoutScene": "scene",
    "DiseaseCureHerbal": "potion",
    "ValdezPostTallyScene": "scene",
    "TallyPreFightScene": "scene",
    "pBS02_MQ05_Catalyst": "quest",
    "Catalyst_StartKeyword": "keyword",
    "BlackburnAmbient_Mainframe": "scene",
    "Valdez_PuzzleCommentary": "scene",
    "BlackburnAmbient_Atrium": "scene",
    "BlackburnAmbient_Reactor": "scene",
    "BlackburnAmbient_Cryo": "scene",
    "PlayerEnemy": "faction",
    "BlackburnAmbient_Research": "scene",
    "BlackburnAmbient_Overseer": "scene",
    "ValdezAmbient_Atrium": "scene",
    "ValdezAmbient_Mainframe": "scene",
    "ValdezAmbient_Cryo": "scene",
    "ValdezAmbient_Research": "scene",
    "ValdezAmbient_ResearchLab": "scene",
    "Alias_Door_Entrance": "referencealias",
    "Alias_Holo_BlackburnLogs_Research_07": "referencealias",
    "Alias_Marker_ReactorTerminal": "referencealias",
    "ElectricalExplosion": "explosion",
    "Intro_Ambient_Scene": "scene",
}

ACTIVATOR_PROPERTY_TYPES = {
    "SceneToPlay": "scene",
    "Alias_Player": "referencealias",
    "ShutoffStage": "int",
    "PrereqStage": "int",
}

FURNITURE_PROPERTY_TYPES = {
    "StageToSet": "int",
    "Alias_Player": "referencealias",
    "ShutoffStage": "int",
    "PrereqStage": "int",
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


def _generated_source(script_name: str) -> str:
    source_path = SOURCE_ROOT / _script_relative_path(script_name, ".psc")
    assert source_path.is_file(), source_path
    return source_path.read_text(encoding="utf-8")


def _property_types(source: str) -> dict[str, str]:
    properties: dict[str, str] = {}
    for line in source.splitlines():
        parts = line.split()
        if len(parts) >= 4 and parts[1].casefold() == "property":
            property_type, property_name = parts[0].casefold(), parts[2]
            assert property_name not in properties
            properties[property_name] = property_type
    return properties


def test_quest_patch_supplies_all_84_exact_vmad_members_and_runtime_events():
    patch = _script_patch_source(QUEST_SCRIPT)
    assert patch is not None

    member_names = _member_names(patch)
    assert len(QUEST_MEMBER_NAMES) == 84
    assert member_names == [*QUEST_MEMBER_NAMES, "actor.ondeath", "ontimer"]
    assert len(member_names) == len(set(member_names))
    assert _iter_papyrus_states(patch.splitlines()) == []
    assert not any(
        line.strip().lower().startswith(("scriptname ", "state "))
        for line in patch.splitlines()
    )
    assert not any(" property " in f" {line.strip().lower()} " for line in patch.splitlines())


def test_quest_patch_merges_into_real_generated_property_contract_idempotently():
    patch = _script_patch_source(QUEST_SCRIPT)
    assert patch is not None

    generated = _generated_source(QUEST_SCRIPT)
    assert _property_types(generated) == QUEST_PROPERTY_TYPES
    assert len(QUEST_PROPERTY_TYPES) == 97

    merged = _merge_script_method_patches(generated, patch)
    merged_names = _member_names(merged)

    assert merged_names == [*QUEST_MEMBER_NAMES, "actor.ondeath", "ontimer"]
    assert all(merged_names.count(member_name) == 1 for member_name in QUEST_MEMBER_NAMES)
    assert merged_names.count("actor.ondeath") == 1
    assert merged_names.count("ontimer") == 1
    assert _property_types(merged) == QUEST_PROPERTY_TYPES
    assert _merge_script_method_patches(merged, patch) == merged


def test_four_clue_families_converge_once_on_their_bound_summary_stages():
    patch = _script_patch_source(QUEST_SCRIPT)
    assert patch is not None

    convergence_contracts = (
        ((981, 982, 983), 1000),
        ((1150, 1151), 1200),
        ((1650, 1660, 1670), 1700),
        ((1850, 1855, 1860), 1900),
    )
    for clue_stages, summary_stage in convergence_contracts:
        for clue_stage in clue_stages:
            body = _member_body(
                patch, f"fragment_stage_{clue_stage:04d}_item_00"
            )
            for sibling_stage in clue_stages:
                if sibling_stage != clue_stage:
                    assert f"IsStageDone({sibling_stage})" in body
            assert f"!IsStageDone({summary_stage})" in body
            assert body.count(f"SetStage({summary_stage})") == 1


def test_tally_deal_and_fight_branches_preserve_keycard_progression_safely():
    patch = _script_patch_source(QUEST_SCRIPT)
    assert patch is not None

    killed_tally = _member_body(patch, "fragment_stage_1560_item_00")
    assert killed_tally.index("GetItemCount(SecurityKeycard) == 0") < killed_tally.index(
        "AddItem(SecurityKeycard, 1, False)"
    )

    deal_branch = _member_body(patch, "fragment_stage_1600_item_00")
    fight_branch = _member_body(patch, "fragment_stage_1600_item_01")
    for body in (deal_branch, fight_branch):
        assert body.count("AddItem(SecurityKeycard, 1, False)") == 1
        assert body.index("GetItemCount(SecurityKeycard) == 0") < body.index(
            "AddItem(SecurityKeycard, 1, False)"
        )

    assert "SetValue(TallyGaveKeycard, 1.0)" in deal_branch
    assert "AddToFaction(PlayerAlly)" in deal_branch
    assert "ValdezPostTallyScene.Start()" in deal_branch
    assert "SetValue(TallyGaveKeycard, 1.0)" not in fight_branch
    assert "Alias_EM_TallyLang.GetReference()" in fight_branch
    assert "Valdez_PostFight_Scene.Start()" in fight_branch


def test_cryo_wave_spawns_exactly_two_and_completes_only_after_both_deaths():
    patch = _script_patch_source(QUEST_SCRIPT)
    assert patch is not None

    start_wave = _member_body(patch, "fragment_stage_1400_item_00")
    assert "GetAlias(97) as ReferenceAlias" in start_wave
    assert (
        'Game.GetFormFromFile(0x00450123, "SeventySix.esm") as ActorBase'
        in start_wave
    )
    assert "Alias_Enemies_Cryobots.GetCount() == 0" in start_wave
    assert start_wave.count("PlaceActorAtMe(cryobotBase, 2)") == 2
    assert start_wave.count("Alias_Enemies_Cryobots.AddRef(") == 2
    assert start_wave.count('RegisterForRemoteEvent(cryobot') == 2
    assert start_wave.count("StartCombat(playerRef)") == 2
    assert start_wave.count("Delete()") == 2
    assert "SetStage(1500)" not in start_wave

    death = _member_body(patch, "actor.ondeath")
    assert death.splitlines()[0] == (
        "Event Actor.OnDeath(Actor akSender, Actor akKiller)"
    )
    assert "GetCurrentStageID() == 1400" in death
    assert "Alias_Enemies_Cryobots.GetCount() == 2" in death
    assert "Alias_Enemies_Cryobots.GetAt(0) as Actor" in death
    assert "Alias_Enemies_Cryobots.GetAt(1) as Actor" in death
    assert "akSender == cryobotOne || akSender == cryobotTwo" in death
    assert "cryobotOne.IsDead() && cryobotTwo.IsDead()" in death
    assert death.count('UnregisterForRemoteEvent(cryobot') == 2
    assert death.count("SetStage(1500)") == 1
    assert death.index("cryobotOne.IsDead()") < death.index("SetStage(1500)")
    assert death.index("cryobotTwo.IsDead()") < death.index("SetStage(1500)")
    assert patch.count("SetStage(1500)") == 1

    cleanup = _member_body(patch, "fragment_stage_9000_item_00")
    assert "Alias_Enemies_Cryobots.GetCount() - 1" in cleanup
    # OnDeath is an Actor event: stock PapyrusCompiler.exe rejects an
    # ObjectReference receiver, so the cleanup unregisters the Actor cast.
    assert "Actor cryobotActor = cryobotRef as Actor" in cleanup
    assert 'UnregisterForRemoteEvent(cryobotActor, "OnDeath")' in cleanup
    assert "Alias_Enemies_Cryobots.RemoveRef(cryobotRef)" in cleanup
    assert cleanup.index("cryobotRef.Disable()") < cleanup.index(
        "cryobotRef.Delete()"
    )


def test_duplicate_research_stage_ordinals_keep_distinct_branch_effects():
    patch = _script_patch_source(QUEST_SCRIPT)
    assert patch is not None

    research_bodies = [
        _member_body(patch, f"fragment_stage_1900_item_{item:02d}")
        for item in range(5)
    ]
    assert len(set(research_bodies)) == 5
    assert "SetObjectiveDisplayed(1840)" in research_bodies[0]
    assert "SetObjectiveDisplayed(1860)" in research_bodies[1]
    assert "SetObjectiveDisplayed(1900)" in research_bodies[2]
    assert "SetObjectiveDisplayed(1850)" in research_bodies[3]
    assert "SetStage(2100)" in research_bodies[4]

    for stage, count in ((1940, 2), (1950, 2), (1960, 2)):
        bodies = [
            _member_body(patch, f"fragment_stage_{stage:04d}_item_{item:02d}")
            for item in range(count)
        ]
        assert len(set(bodies)) == count


def test_scenes_doors_and_local_encounters_cover_the_dungeon_progression():
    patch = _script_patch_source(QUEST_SCRIPT)
    assert patch is not None

    assert "Alias_EM_ValdezDungeon.GetReference()" in _member_body(
        patch, "fragment_stage_0050_item_00"
    )
    assert "Valdez_EntranceDoorScene.Start()" in _member_body(
        patch, "fragment_stage_0710_item_00"
    )
    assert "Alias_EM_Atrium_BloodEagles.GetReference()" in _member_body(
        patch, "fragment_stage_0800_item_00"
    )
    assert "Alias_EM_MainframeEnemies.GetReference()" in _member_body(
        patch, "fragment_stage_0955_item_00"
    )
    assert "Alias_EM_ReactorEnemies.GetReference()" in _member_body(
        patch, "fragment_stage_1100_item_00"
    )
    assert "Alias_EM_CryoEnemies.GetReference()" in _member_body(
        patch, "fragment_stage_1400_item_00"
    )
    assert "Alias_EM_ResearchEnemies.GetReference()" in _member_body(
        patch, "fragment_stage_1800_item_00"
    )

    short_circuit = _member_body(patch, "fragment_stage_1050_item_00")
    assert short_circuit.index("PlaceAtMe(ElectricalExplosion)") < short_circuit.index(
        "SetStage(1100)"
    )
    assert "Alias_Door_TestChamber_02.GetReference()" in _member_body(
        patch, "fragment_stage_1990_item_00"
    )
    hydraulics = _member_body(patch, "fragment_stage_2400_item_00")
    assert hydraulics.index("overseerDoor.Unlock()") < hydraulics.index(
        "overseerDoor.SetOpen(True)"
    )
    assert "Valdez_PuzzleCommentary.Start()" in hydraulics
    assert "BlackburnOffice_Scene.Start()" in _member_body(
        patch, "fragment_stage_2470_item_00"
    )


def test_catalyst_handoff_retries_without_bypassing_story_manager_conditions():
    patch = _script_patch_source(QUEST_SCRIPT)
    assert patch is not None

    handoff = _member_body(patch, "fragment_stage_3000_item_00")
    retry = _member_body(patch, "ontimer")
    story_event = (
        "Catalyst_StartKeyword.SendStoryEventAndWait(Vault96, playerRef, playerRef)"
    )
    accepted_handoff = (
        "pBS02_MQ05_Catalyst.IsRunning() || "
        "pBS02_MQ05_Catalyst.IsCompleted()"
    )
    completion = "SetStage(9000)"
    rejected_handoff = "ElseIf GetCurrentStageID() == 3000"
    retry_timer = "StartTimer(5.0, 3000)"

    assert retry.splitlines()[0] == "Event OnTimer(Int aiTimerID)"
    assert "aiTimerID == 3000 && GetCurrentStageID() == 3000" in retry
    for attempt in (handoff, retry):
        assert attempt.count(story_event) == 1
        assert ".Start()" not in attempt
        assert "Stop()" not in attempt
        assert attempt.count(accepted_handoff) == 1
        assert attempt.count(completion) == 1
        assert attempt.count(rejected_handoff) == 1
        assert attempt.count(retry_timer) == 1
        assert attempt.index(story_event) < attempt.index(accepted_handoff)
        assert attempt.index(accepted_handoff) < attempt.index(completion)
        assert attempt.index(completion) < attempt.index(rejected_handoff)
        assert attempt.index(rejected_handoff) < attempt.index(retry_timer)

    shutdown = _member_body(patch, "fragment_stage_9000_item_00")
    assert shutdown.count("Stop()") == 1


def test_activator_helper_uses_exact_activation_signature_and_local_player_window():
    patch = _script_patch_source(ACTIVATOR_SCRIPT)
    assert patch is not None

    assert _member_names(patch) == ["oninit", "onactivate"]
    assert len(set(_member_names(patch))) == 2
    activate = _member_body(patch, "onactivate")
    assert activate.splitlines()[0] == "Event OnActivate(ObjectReference akActionRef)"
    assert "Actor localPlayer = Game.GetPlayer()" in activate
    assert "akActionRef == localPlayer" in activate
    assert "myPlayerRef == localPlayer" in activate
    assert "myQI.IsStageDone(PrereqStage)" in activate
    assert "!myQI.IsStageDone(ShutoffStage)" in activate
    assert "SceneToPlay.Start()" in activate

    generated = _generated_source(ACTIVATOR_SCRIPT)
    assert _property_types(generated) == ACTIVATOR_PROPERTY_TYPES
    merged = _merge_script_method_patches(generated, patch)
    assert _member_names(merged) == ["oninit", "onactivate"]
    assert _property_types(merged) == ACTIVATOR_PROPERTY_TYPES
    assert _merge_script_method_patches(merged, patch) == merged


def test_furniture_helper_uses_exact_exit_signature_and_bounded_stage_transition():
    patch = _script_patch_source(FURNITURE_SCRIPT)
    assert patch is not None

    assert _member_names(patch) == ["oninit", "onexitfurniture"]
    assert len(set(_member_names(patch))) == 2
    on_exit = _member_body(patch, "onexitfurniture")
    assert on_exit.splitlines()[0] == (
        "Event OnExitFurniture(ObjectReference akActionRef)"
    )
    assert "Actor localPlayer = Game.GetPlayer()" in on_exit
    assert "akActionRef == localPlayer" in on_exit
    assert "myPlayerRef == localPlayer" in on_exit
    assert "myQI.IsStageDone(PrereqStage)" in on_exit
    assert "!myQI.IsStageDone(StageToSet)" in on_exit
    assert "!myQI.IsStageDone(ShutoffStage)" in on_exit
    assert on_exit.count("myQI.SetStage(StageToSet)") == 1

    generated = _generated_source(FURNITURE_SCRIPT)
    assert _property_types(generated) == FURNITURE_PROPERTY_TYPES
    merged = _merge_script_method_patches(generated, patch)
    assert _member_names(merged) == ["oninit", "onexitfurniture"]
    assert _property_types(merged) == FURNITURE_PROPERTY_TYPES
    assert _merge_script_method_patches(merged, patch) == merged
