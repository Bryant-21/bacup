from __future__ import annotations

import os
from pathlib import Path

import pytest

from bacup_lib.workflows.unified import (
    _iter_papyrus_states,
    _iter_top_level_papyrus_members,
    _merge_script_method_patches,
    _script_patch_source,
    _script_relative_path,
)
from creation_lib.pex.native_runtime import compile_psc


QUEST_SCRIPT = "Fragments:Quests:QF_BS02_MQ05_Catalyst_005FA459"
PLAYER_SCRIPT = "Quests:BS02_MQ05_Catalyst:PlayerScript"
MEET_SCIENTISTS_SCRIPT = "Quests:BS02_MQ05_Catalyst:MeetScientistsTrigger"
TERMINAL_SCRIPT = "Fragments:Terminals:TERM_BS02_MQ05_Catalyst_View_00606757"
PACKAGE_SCRIPTS = (
    "Fragments:Packages:PF_BS02_MQ05_Catalyst_Rahman_0060D339",
    "Fragments:Packages:PF_BS02_MQ05_Catalyst_ShinTr_0060D33A",
    "Fragments:Packages:PF_BS02_MQ05_Catalyst_Dorsey_0060D85A",
)

QUEST_MEMBER_NAMES = (
    "fragment_stage_0001_item_00",
    "fragment_stage_0002_item_00",
    "fragment_stage_0003_item_00",
    "fragment_stage_0010_item_00",
    "fragment_stage_0015_item_00",
    "fragment_stage_0020_item_00",
    "fragment_stage_0100_item_00",
    "fragment_stage_0150_item_00",
    "fragment_stage_0200_item_00",
    "fragment_stage_0250_item_00",
    "fragment_stage_0300_item_00",
    "fragment_stage_0400_item_00",
    "fragment_stage_0500_item_00",
    "fragment_stage_0600_item_00",
    "fragment_stage_0700_item_00",
    "fragment_stage_0750_item_00",
    "fragment_stage_0800_item_00",
    "fragment_stage_0900_item_00",
    "fragment_stage_0950_item_00",
    "fragment_stage_0975_item_00",
    "fragment_stage_1000_item_00",
    "fragment_stage_1100_item_00",
    "fragment_stage_1175_item_00",
    "fragment_stage_1210_item_00",
    "fragment_stage_1220_item_00",
    "fragment_stage_1230_item_00",
    "fragment_stage_1240_item_00",
    "fragment_stage_1250_item_00",
    "fragment_stage_1260_item_00",
    "fragment_stage_1300_item_00",
    "fragment_stage_1310_item_00",
    "fragment_stage_1311_item_00",
    "fragment_stage_1313_item_00",
    "fragment_stage_1314_item_00",
    "fragment_stage_1315_item_00",
    "fragment_stage_1316_item_00",
    "fragment_stage_1317_item_00",
    "fragment_stage_1330_item_00",
    "fragment_stage_1340_item_00",
    "fragment_stage_1360_item_00",
    "fragment_stage_1370_item_00",
    "fragment_stage_1401_item_00",
    "fragment_stage_1402_item_00",
    "fragment_stage_1405_item_00",
    "fragment_stage_1410_item_00",
    "fragment_stage_1411_item_00",
    "fragment_stage_1412_item_00",
    "fragment_stage_1413_item_00",
    "fragment_stage_1450_item_00",
    "fragment_stage_1460_item_00",
    "fragment_stage_1475_item_00",
    "fragment_stage_1500_item_00",
    "fragment_stage_1550_item_00",
    "fragment_stage_1600_item_00",
    "fragment_stage_1601_item_00",
    "fragment_stage_1602_item_00",
    "fragment_stage_1650_item_00",
    "fragment_stage_1700_item_00",
    "fragment_stage_1710_item_00",
    "fragment_stage_1720_item_00",
    "fragment_stage_1760_item_00",
    "fragment_stage_9000_item_00",
    "fragment_stage_10000_item_00",
)

REPO_ROOT = Path(__file__).resolve().parents[5]
SOURCE_ROOT = REPO_ROOT / "mods" / "SeventySix" / "Scripts" / "Source" / "User"
DEPLOYED_SCRIPTS_DIR = REPO_ROOT / "mods" / "SeventySix" / "data" / "Scripts"
QUEST_SKELETON = (
    SOURCE_ROOT
    / "Fragments"
    / "Quests"
    / "QF_BS02_MQ05_Catalyst_005FA459.psc"
).read_text(encoding="utf-8")
PLAYER_SKELETON = (
    SOURCE_ROOT / "Quests" / "BS02_MQ05_Catalyst" / "PlayerScript.psc"
).read_text(encoding="utf-8")
MEET_SCIENTISTS_SKELETON = (
    SOURCE_ROOT / "Quests" / "BS02_MQ05_Catalyst" / "MeetScientistsTrigger.psc"
).read_text(encoding="utf-8")


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
    return _merge_script_method_patches(source_path.read_text(encoding="utf-8"), patch)


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


def test_quest_patch_supplies_all_63_exact_vmad_members_once():
    patch = _script_patch_source(QUEST_SCRIPT)
    assert patch is not None

    member_names = _member_names(patch)
    assert len(QUEST_MEMBER_NAMES) == 63
    # One local helper is declared ahead of the fragments: Alias_EnableMarkers_AmbientSMs
    # is a RefCollectionAlias, which has no TryToEnable/TryToDisable, so the enable and
    # disable sites iterate the collection instead.
    assert member_names[0] == "setambientstorymanagermarkersenabled"
    assert member_names[1:] == list(QUEST_MEMBER_NAMES)
    assert len(member_names) == len(set(member_names))
    assert _iter_papyrus_states(patch.splitlines()) == []
    assert not any(
        line.strip().lower().startswith(("scriptname ", "state "))
        for line in patch.splitlines()
    )
    assert not any(" property " in f" {line.strip().lower()} " for line in patch.splitlines())


def test_quest_patch_merges_uniquely_and_idempotently_into_the_107_property_skeleton():
    patch = _script_patch_source(QUEST_SCRIPT)
    assert patch is not None

    merged = _merge_script_method_patches(QUEST_SKELETON, patch)
    merged_names = _member_names(merged)

    assert QUEST_SKELETON.lower().count(" property ") == 107
    assert merged_names == ["setambientstorymanagermarkersenabled"] + list(
        QUEST_MEMBER_NAMES
    )
    assert all(merged_names.count(name) == 1 for name in QUEST_MEMBER_NAMES)
    assert "referencealias Property Alias_Player Auto mandatory" in merged
    assert "actorvalue Property BS02_AV_BrotherhoodLeaderFinal Auto" in merged
    assert "actorvalue Property BS02_AV_BrotherhoodLeaderDead Auto" in merged
    assert _merge_script_method_patches(merged, patch) == merged


def test_leader_choice_death_and_banishment_preserve_persistent_av_meanings():
    patch = _script_patch_source(QUEST_SCRIPT)
    assert patch is not None

    rahmani_choice = _member_body(patch, "fragment_stage_1330_item_00")
    shin_choice = _member_body(patch, "fragment_stage_1360_item_00")
    leader_death = _member_body(patch, "fragment_stage_1450_item_00")
    rahmani_banished = _member_body(patch, "fragment_stage_1401_item_00")
    shin_banished = _member_body(patch, "fragment_stage_1402_item_00")

    assert "SetValue(BS02_AV_BrotherhoodLeaderFinal, 1.0)" in rahmani_choice
    assert "SetValue(BS02_AV_BrotherhoodLeaderFinal, 2.0)" in shin_choice
    assert "SetObjectiveDisplayed(129)" in rahmani_choice
    assert "SetObjectiveDisplayed(128)" in shin_choice
    assert "SetStage(1400)" in rahmani_choice
    assert "SetStage(1400)" in shin_choice
    assert "IsStageDone(1340)" in leader_death
    assert "SetValue(BS02_AV_BrotherhoodLeaderDead, 2.0)" in leader_death
    assert "IsStageDone(1370)" in leader_death
    assert "SetValue(BS02_AV_BrotherhoodLeaderDead, 1.0)" in leader_death
    assert "SetValue(BS01_RahmaniAwayValue, 1.0)" in rahmani_banished
    assert "SetValue(BS01_ShinAwayValue, 1.0)" in shin_banished
    assert "SetObjectiveCompleted(128)" in rahmani_banished
    assert "SetObjectiveCompleted(129)" in shin_banished
    assert "SetStage(1450)" not in rahmani_banished
    assert "SetStage(1450)" not in shin_banished


def test_opposing_leader_objectives_complete_on_fight_or_banishment_branches():
    patch = _script_patch_source(QUEST_SCRIPT)
    assert patch is not None

    shin_fight = _member_body(patch, "fragment_stage_1340_item_00")
    rahmani_fight = _member_body(patch, "fragment_stage_1370_item_00")
    assert "SetObjectiveCompleted(129)" in shin_fight
    assert "SetObjectiveCompleted(128)" in rahmani_fight


def test_scientist_spare_and_execution_paths_converge_on_distinct_fort_states():
    patch = _script_patch_source(QUEST_SCRIPT)
    assert patch is not None

    execution_start = _member_body(patch, "fragment_stage_1405_item_00")
    scientists_vulnerable = _member_body(patch, "fragment_stage_1410_item_00")
    execution_done = _member_body(patch, "fragment_stage_1460_item_00")
    fort_setup = _member_body(patch, "fragment_stage_1500_item_00")

    assert "BS02_MQ05_Catalyst_KillingScientists.Start()" in execution_start
    assert scientists_vulnerable.count("SetProtected(False)") == 3
    for stage, alias_name in (
        (1411, "Alias_Actor_Farha_WestTek"),
        (1412, "Alias_Actor_Anthony_WestTek"),
        (1413, "Alias_Actor_Nellie_WestTek"),
    ):
        body = _member_body(patch, f"fragment_stage_{stage:04d}_item_00")
        assert f"{alias_name}.TryToKill()" in body
    assert "BS02_MQ05_Catalyst_PostKilledScientists.Start()" in execution_done

    assert fort_setup.index("If finalLeader == 1.0") < fort_setup.index(
        "Alias_Actor_Farha_FortAtlas.TryToEnable()"
    )
    assert fort_setup.index("ElseIf finalLeader == 2.0") < fort_setup.index(
        "Alias_Actor_Farha_FortAtlas.TryToDisable()"
    )


def test_terminal_fallback_completion_address_and_cleanup_have_terminal_effects():
    patch = _script_patch_source(QUEST_SCRIPT)
    assert patch is not None

    failed_persuasion = _member_body(patch, "fragment_stage_1240_item_00")
    terminal_unlocked = _member_body(patch, "fragment_stage_1250_item_00")
    address = _member_body(patch, "fragment_stage_1550_item_00")
    complete = _member_body(patch, "fragment_stage_9000_item_00")
    cleanup = _member_body(patch, "fragment_stage_10000_item_00")

    assert "Alias_Terminal_ViewingRoom.TryToEnable()" in failed_persuasion
    assert "SetObjectiveDisplayed(111)" in failed_persuasion
    assert "SetObjectiveDisplayed(112)" in failed_persuasion
    assert "viewingDoor.Lock(False)" in terminal_unlocked
    assert "viewingDoor.SetOpen()" in terminal_unlocked
    assert "SetObjectiveCompleted(111)" in terminal_unlocked
    assert "SetStage(1300)" in terminal_unlocked
    assert "BS02_MQ05_Catalyst_FortAtlasAddress.Start()" in address
    assert "IsObjectiveDisplayed(170)" in complete
    assert "SetObjectiveCompleted(170)" in complete
    assert "IsObjectiveDisplayed(171)" in complete
    assert "SetObjectiveCompleted(171)" in complete
    assert complete.rstrip().endswith("CompleteQuest()\nEndFunction")
    assert cleanup.count(".Stop()") >= 12
    assert cleanup.rstrip().endswith("Stop()\nEndFunction")


def test_fort_resident_departures_converge_regardless_of_event_order():
    patch = _script_patch_source(QUEST_SCRIPT)
    assert patch is not None

    dorsey_departure = _member_body(patch, "fragment_stage_1710_item_00")
    hewsen_departure = _member_body(patch, "fragment_stage_1720_item_00")
    assert "IsStageDone(1720)" in dorsey_departure
    assert "SetObjectiveCompleted(171)" in dorsey_departure
    assert "IsStageDone(1710)" in hewsen_departure
    assert "SetObjectiveCompleted(171)" in hewsen_departure


def test_player_helper_sets_cleanup_once_only_after_leaving_completed_fort_atlas():
    patch = _script_patch_source(PLAYER_SCRIPT)
    assert patch is not None
    assert _script_patch_source("Quests:_Default:SetStageOnEnterInstancedLoc") is not None

    assert _member_names(patch) == [
        "onaliasinit",
        "onplayerloadgame",
        "actor.onlocationchange",
        "onaliasshutdown",
        "initializeplayerlocationtracking",
        "reconcilefortatlasentry",
        "reconcilecleanup",
    ]
    on_location_change = _member_body(patch, "actor.onlocationchange")
    assert on_location_change.splitlines()[0] == (
        "Event Actor.OnLocationChange(Actor akSender, Location akOldLoc, Location akNewLoc)"
    )
    assert "akSender != Game.GetPlayer()" in on_location_change
    assert "ReconcileFortAtlasEntry(akNewLoc)" in on_location_change
    assert "ReconcileCleanup(akOldLoc, akNewLoc)" in on_location_change

    init = _member_body(patch, "initializeplayerlocationtracking")
    fort_entry = _member_body(patch, "reconcilefortatlasentry")
    cleanup = _member_body(patch, "reconcilecleanup")
    shutdown = _member_body(patch, "onaliasshutdown")
    assert "InitializePlayerLocationTracking()" in _member_body(patch, "onaliasinit")
    assert "InitializePlayerLocationTracking()" in _member_body(
        patch, "onplayerloadgame"
    )
    assert "playerRef != Game.GetPlayer()" in init
    assert 'UnregisterForRemoteEvent(playerRef, "OnLocationChange")' in init
    assert 'RegisterForRemoteEvent(playerRef, "OnLocationChange")' in init
    assert "ReconcileFortAtlasEntry(playerRef.GetCurrentLocation())" in init
    assert "Loc_FortAtlas.GetLocation()" in fort_entry
    assert "currentLocation == fortAtlas" in fort_entry
    assert "owningQuest.IsStageDone(1475)" in fort_entry
    assert "!owningQuest.IsStageDone(20)" in fort_entry
    assert fort_entry.count("owningQuest.SetStage(20)") == 1
    assert "oldLocation == fortAtlas" in cleanup
    assert "newLocation != fortAtlas" in cleanup
    assert "owningQuest.IsStageDone(CompletionStage)" in cleanup
    assert "!owningQuest.IsStageDone(CleanupStage)" in cleanup
    assert cleanup.count("owningQuest.SetStage(CleanupStage)") == 1
    assert 'UnregisterForRemoteEvent(playerRef, "OnLocationChange")' in shutdown

    merged = _merge_script_method_patches(PLAYER_SKELETON, patch)
    assert _member_names(PLAYER_SKELETON) == []
    assert _member_names(merged) == _member_names(patch)
    assert all(_member_names(merged).count(name) == 1 for name in _member_names(patch))
    assert _merge_script_method_patches(merged, patch) == merged


def test_meet_scientists_helper_uses_exact_non_overlapping_stage_windows():
    patch = _script_patch_source(MEET_SCIENTISTS_SCRIPT)
    assert patch is not None

    assert _member_names(patch) == ["oninit", "ontriggerenter"]
    trigger = _member_body(patch, "ontriggerenter")
    assert trigger.splitlines()[0] == "Event OnTriggerEnter(ObjectReference akActionRef)"
    assert "akActionRef != myPlayerRef" in trigger
    assert "myPlayerRef != Game.GetPlayer()" in trigger
    assert "currentStage >= MeetScientistsTurnOnStage" in trigger
    assert "currentStage < MeetScientistsTurnOffStage" in trigger
    assert "currentStage >= BlackburnBetrayalTurnOnStage" in trigger
    assert "currentStage < BlackburnBetrayalTurnOffStage" in trigger
    assert trigger.count("BS02_MQ05_Catalyst_MeetScientists.Start()") == 1
    assert trigger.count("BS02_MQ05_Catalyst_BlackburnBetrayal.Start()") == 1

    assert "Int Property MeetScientistsTurnOnStage Auto" in MEET_SCIENTISTS_SKELETON
    assert "Int Property MeetScientistsTurnOffStage Auto" in MEET_SCIENTISTS_SKELETON
    assert "Int Property BlackburnBetrayalTurnOnStage Auto" in MEET_SCIENTISTS_SKELETON
    assert "Int Property BlackburnBetrayalTurnOffStage Auto" in MEET_SCIENTISTS_SKELETON

    merged = _merge_script_method_patches(MEET_SCIENTISTS_SKELETON, patch)
    assert _member_names(MEET_SCIENTISTS_SKELETON) == []
    assert _member_names(merged) == ["oninit", "ontriggerenter"]
    assert all(_member_names(merged).count(name) == 1 for name in ("oninit", "ontriggerenter"))
    assert _merge_script_method_patches(merged, patch) == merged


@pytest.mark.parametrize("script_name", PACKAGE_SCRIPTS)
def test_catalyst_exit_packages_disable_the_actor_at_the_exact_end_fragment(
    script_name: str,
):
    patch = _script_patch_source(script_name)
    assert patch is not None

    assert _member_names(patch) == ["fragment_end"]
    fragment = _member_body(patch, "fragment_end")
    assert fragment.splitlines()[0] == "Function Fragment_End(Actor akActor)"
    assert fragment.count("akActor.Disable()") == 1

    merged = _merged_source(script_name)
    assert _member_names(merged) == ["fragment_end"]
    assert _merge_script_method_patches(merged, patch) == merged


def test_viewing_room_terminal_unlocks_the_bound_terminal_for_stage_1250_alias_event():
    patch = _script_patch_source(TERMINAL_SCRIPT)
    assert patch is not None

    assert _member_names(patch) == ["fragment_terminal_01"]
    fragment = _member_body(patch, "fragment_terminal_01")
    assert fragment.splitlines()[0] == (
        "Function Fragment_Terminal_01(ObjectReference akTerminalRef)"
    )
    assert fragment.count("akTerminalRef.Unlock()") == 1
    assert "SetStage(" not in fragment

    merged = _merged_source(TERMINAL_SCRIPT)
    assert _member_names(merged) == ["fragment_terminal_01"]
    assert _merge_script_method_patches(merged, patch) == merged


def test_mercenary_stage_400_activates_the_bound_enable_marker():
    patch = _script_patch_source(QUEST_SCRIPT)
    assert patch is not None

    encounter_start = _member_body(patch, "fragment_stage_0400_item_00")
    encounter_end = _member_body(patch, "fragment_stage_0500_item_00")
    assert "Alias_EnableMarker_Mercs.TryToEnable()" in encounter_start
    assert "Alias_EnableMarker_Mercs.TryToDisable()" in encounter_end
    assert "GenericEWS" not in encounter_start


@pytest.mark.parametrize(
    "script_name",
    (
        QUEST_SCRIPT,
        PLAYER_SCRIPT,
        MEET_SCIENTISTS_SCRIPT,
        TERMINAL_SCRIPT,
        *PACKAGE_SCRIPTS,
    ),
)
def test_catalyst_helper_and_record_fragment_production_merges_compile(
    script_name: str,
):
    base_source = _fo4_base_source()
    if base_source is None or not DEPLOYED_SCRIPTS_DIR.is_dir():
        pytest.skip("FO4 Papyrus compile dependencies unavailable")

    result = compile_psc(
        _merged_source(script_name),
        imports=[str(base_source), str(SOURCE_ROOT), str(DEPLOYED_SCRIPTS_DIR)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=str(_script_relative_path(script_name, ".psc")),
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None
