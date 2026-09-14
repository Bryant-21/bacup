from __future__ import annotations

import json
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
FIXTURE_ROOT = Path(__file__).resolve().parent / "fixtures"

PATCH_MEMBERS = {
    "SFM04_Organic_PlayerAliasScript": {
        "onaliasinit",
        "onplayerloadgame",
        "onitemadded",
        "reconcileradshield",
    },
    "Fragments:Quests:QF_SFM04_Organic_0010AE02": {
        "tryadvancetochemicaldeposit",
        "fragment_stage_0340_item_00",
        "fragment_stage_0350_item_00",
        "fragment_stage_0360_item_00",
        "fragment_stage_0370_item_00",
        "fragment_stage_0700_item_00",
    },
    "Fragments:Quests:QF_MTR07_Earth_003443FB": {
        "tryadvanceaftercoreinstalled",
        "fragment_stage_0030_item_00",
        "fragment_stage_0040_item_00",
        "fragment_stage_0050_item_00",
        "fragment_stage_0060_item_00",
        "fragment_stage_0070_item_00",
    },
    "MTR07_EarthQuestScript": {
        "onstageset",
        "onquestinit",
        "onquestshutdown",
        "actor.onplayerloadgame",
        "ontimer",
        "registerforplayerloadreconciliation",
        "ensureattacktimer",
    },
    "Fragments:Quests:QF_BoSZ01_0010D89F": {"fragment_stage_0001_item_00"},
    "MTRZ05_MapScript": {"onequipped"},
    "SFL02_Track_QuestScript": {
        "onquestinit",
        "reconcileruntimeregistrations",
        "onquestshutdown",
    },
    "Fragments:Quests:QF_MTR05_Mother_0006A379": {
        "fragment_stage_0004_item_00",
        "fragment_stage_0050_item_00",
        "fragment_stage_0100_item_00",
        "fragment_stage_0150_item_00",
        "fragment_stage_0200_item_00",
        "fragment_stage_0300_item_00",
        "fragment_stage_0308_item_00",
        "fragment_stage_0500_item_00",
        "fragment_stage_0510_item_00",
        "fragment_stage_0999_item_00",
    },
}


def _member_body(source: str, member_name: str) -> str:
    lines = source.splitlines()
    start, end = next(
        (start, end)
        for _kind, name, start, end in _iter_top_level_papyrus_members(lines)
        if name == member_name.casefold()
    )
    return "\n".join(lines[start : end + 1])


@pytest.mark.parametrize(("script_name", "expected_members"), PATCH_MEMBERS.items())
def test_misc_quest_patches_merge_once_and_compile(
    script_name: str, expected_members: set[str]
):
    source_path = SOURCE_ROOT / _script_relative_path(script_name, ".psc")
    skeleton = source_path.read_text(encoding="utf-8")
    patch = _script_patch_source(script_name)

    assert patch is not None
    assert "Scriptname" not in patch
    merged = _merge_script_method_patches(skeleton, patch)
    members = [
        name
        for _kind, name, _start, _end in _iter_top_level_papyrus_members(
            merged.splitlines()
        )
    ]
    for member in expected_members:
        assert members.count(member) == 1
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


def test_sfm04_organic_progression_contract():
    quest = _script_patch_source("Fragments:Quests:QF_SFM04_Organic_0010AE02")
    player = _script_patch_source("SFM04_Organic_PlayerAliasScript")
    assert quest is not None and player is not None
    assert "IsStageDone(340) && IsStageDone(350)" in quest
    assert "IsStageDone(360) && IsStageDone(370)" in quest
    assert quest.count("TryAdvanceToChemicalDeposit()") == 5
    assert "AddInventoryEventFilter(SFM04_Organic_RadShield)" in player
    assert "owningQuest.SetStage(QuestCompleteStage)" in player
    assert player.count("ReconcileRadShield()") == 4
    reconciliation = _member_body(player, "reconcileradshield")
    assert "owningQuest.IsRunning()" in reconciliation
    assert "owningQuest.IsStageDone(CraftRadshieldStage)" in reconciliation
    assert "!owningQuest.IsStageDone(QuestCompleteStage)" in reconciliation
    assert "playerRef.GetItemCount(SFM04_Organic_RadShield) > 0" in reconciliation
    stage700 = _member_body(quest, "fragment_stage_0700_item_00")
    assert "Alias_SFM04Player.GetActorReference()" in stage700
    assert "playerRef.GetItemCount(SFM04_Organic_RadShield) > 0" in stage700
    assert "!IsStageDone(1000)" in stage700
    stage435 = _member_body(quest, "fragment_stage_0435_item_00")
    stage440 = _member_body(quest, "fragment_stage_0440_item_00")
    assert "If IsStageDone(440)" in stage435
    assert "If IsStageDone(435)" in stage440
    assert "SetStage(450)" in stage435
    assert "SetStage(450)" in stage440

    flags = json.loads(
        (FIXTURE_ROOT / "events_misc_completion_stage_flags.json").read_text(
            encoding="utf-8"
        )
    )["SFM04_Organic"]
    assert flags["source"] == flags["live"] == ["CompleteQuest"]
    assert flags["source_index_flags"] == flags["live_index_flags"] == []
    stage1000 = _member_body(quest, flags["fragment"])
    assert "CompleteQuest()" not in stage1000
    assert "Stop()" in stage1000


def test_mtr07_and_early_misc_start_contracts():
    earth = _script_patch_source("Fragments:Quests:QF_MTR07_Earth_003443FB")
    earth_root = _script_patch_source("MTR07_EarthQuestScript")
    bos = _script_patch_source("Fragments:Quests:QF_BoSZ01_0010D89F")
    lucky = _script_patch_source("MTRZ05_MapScript")
    assert earth is not None and earth_root is not None
    assert bos is not None and lucky is not None
    assert "IsStageDone(30) && IsStageDone(40)" in earth
    assert "IsStageDone(50) && IsStageDone(60)" in earth
    assert earth.count("TryAdvanceAfterCoreInstalled()") == 5
    assert 'Game.GetFormFromFile(0x003DA814, "SeventySix.esm")' in earth_root
    on_stage = _member_body(earth_root, "onstageset")
    on_load = _member_body(earth_root, "actor.onplayerloadgame")
    on_timer = _member_body(earth_root, "ontimer")
    ensure_timer = _member_body(earth_root, "ensureattacktimer")
    assert "If auiStageID == 200" in on_stage
    assert "RegisterForPlayerLoadReconciliation()" in on_stage
    assert "EnsureAttackTimer()" in on_stage
    assert "ElseIf auiStageID == 255" in on_stage
    assert "CancelTimer(AttackTimerID)" in on_stage
    assert "akSender == Game.GetPlayer()" in on_load
    assert "EnsureAttackTimer()" in on_load
    assert "!earthQuest.IsRunning()" in ensure_timer
    assert "!earthQuest.IsStageDone(200)" in ensure_timer
    assert "earthQuest.IsStageDone(255)" in ensure_timer
    assert "Float duration = 1800.0" in ensure_timer
    assert "wheelTime != None && wheelTime.GetValue() > 0.0" in ensure_timer
    assert ensure_timer.index("CancelTimer(AttackTimerID)") < ensure_timer.index(
        "StartTimer(duration, AttackTimerID)"
    )
    assert "aiTimerID == AttackTimerID" in on_timer
    assert "earthQuest.IsRunning()" in on_timer
    assert "earthQuest.IsStageDone(200)" in on_timer
    assert "!earthQuest.IsStageDone(255)" in on_timer
    assert "earthQuest.SetStage(255)" in on_timer
    flags = json.loads(
        (FIXTURE_ROOT / "events_misc_completion_stage_flags.json").read_text(
            encoding="utf-8"
        )
    )["MTR07_Earth"]
    assert flags["source"] == flags["live"] == ["CompleteQuest"]
    assert flags["source_index_flags"] == ["TimerEnd"]
    assert flags["live_index_flags"] == []
    stage255 = _member_body(earth, flags["fragment"])
    assert "SetObjectiveCompleted(40, True)" in stage255
    assert "CompleteQuest()" not in stage255
    assert "SetStage(100)" in bos
    assert "MTRZ05MapKeyword.SendStoryEventAndWait(" in lucky
    assert "= MTRZ05MapKeyword.SendStoryEventAndWait" not in lucky
    send_index = lucky.index("MTRZ05MapKeyword.SendStoryEventAndWait(")
    first_state_check = lucky.index("MTRZ05_Lucky.IsRunning() || MTRZ05_Lucky.IsCompleted()")
    second_state_check = lucky.index(
        "MTRZ05_Lucky.IsRunning() || MTRZ05_Lucky.IsCompleted()",
        send_index,
    )
    assert first_state_check < send_index < second_state_check
    fire_indices = [
        index
        for index in range(len(lucky))
        if lucky.startswith("FireOnce = 1", index)
    ]
    assert len(fire_indices) == 2
    assert first_state_check < fire_indices[0] < send_index
    assert second_state_check < fire_indices[1]
    assert "akRef1 = akActor" in lucky
    assert "aiValue1 = iRewardValue" in lucky


def test_mtrz05_busy_state_suppresses_reentrant_equip():
    script_name = "MTRZ05_MapScript"
    skeleton = (SOURCE_ROOT / _script_relative_path(script_name, ".psc")).read_text(
        encoding="utf-8"
    )
    patch = _script_patch_source(script_name)
    assert patch is not None
    merged = _merge_script_method_patches(skeleton, patch)
    assert merged.count("Event OnEquipped(Actor akActor)") == 2
    ready = merged.split("State ready", 1)[1].split("EndState", 1)[0]
    busy = merged.split("State busy", 1)[1].split("EndState", 1)[0]
    assert "OnEquipped" not in ready
    assert "Event OnEquipped(Actor akActor)\n    EndEvent" in busy
    assert _merge_script_method_patches(merged, patch) == merged


def test_sfl02_runtime_registration_reconciliation_is_idempotent_and_guarded():
    patch = _script_patch_source("SFL02_Track_QuestScript")
    assert patch is not None

    on_init = _member_body(patch, "onquestinit")
    reconciliation = _member_body(patch, "reconcileruntimeregistrations")
    assert "ReconcileRuntimeRegistrations()" in on_init
    assert "If !IsRunning() || IsCompleted()" in reconciliation
    assert "SetStage(" not in reconciliation
    assert "Start()" not in reconciliation
    assert "Stop()" not in reconciliation

    for event_name in ("OnItemAdded", "OnItemEquipped"):
        unregister = (
            f'UnregisterForRemoteEvent(playerRef, "{event_name}")'
        )
        register = f'RegisterForRemoteEvent(playerRef, "{event_name}")'
        assert reconciliation.count(unregister) == 1
        assert reconciliation.count(register) == 1
        assert reconciliation.index(unregister) < reconciliation.index(register)

    alias_events = (
        (1, "OnActivate"),
        (2, "OnTriggerEnter"),
        (19, "OnTriggerEnter"),
        (20, "OnActivate"),
        (53, "OnTriggerEnter"),
        (54, "OnTriggerEnter"),
        (55, "OnTriggerEnter"),
    )
    for alias_id, event_name in alias_events:
        unregister = f'UnregisterAliasEvent({alias_id}, "{event_name}")'
        register = f'RegisterAliasEvent({alias_id}, "{event_name}")'
        assert reconciliation.count(unregister) == 1
        assert reconciliation.count(register) == 1
        assert reconciliation.index(unregister) < reconciliation.index(register)

    for alias_id in (21, 47, 48, 49):
        unregister = f"UnregisterTerminalEvent({alias_id})"
        register = f"RegisterTerminalEvent({alias_id})"
        assert reconciliation.count(unregister) == 1
        assert reconciliation.count(register) == 1
        assert reconciliation.index(unregister) < reconciliation.index(register)


def test_mtr05_mother_marker_and_shutdown_contract():
    patch = _script_patch_source("Fragments:Quests:QF_MTR05_Mother_0006A379")
    assert patch is not None
    expected = {
        4: ("SetStage(5)",),
        30: ("SetObjectiveCompleted(25, True)", "SetObjectiveDisplayed(30, True)"),
        50: (
            "SetObjectiveCompleted(30, True)",
            "SetObjectiveDisplayed(50, True)",
            "SetObjectiveDisplayed(55, True)",
            "SetObjectiveDisplayed(57, True)",
        ),
        55: ("SetObjectiveCompleted(57, True)", "SetObjectiveDisplayed(59, True)"),
        57: ("SetObjectiveCompleted(57, True)", "SetObjectiveCompleted(59, True)"),
        60: (
            "SetObjectiveCompleted(50, True)",
            "SetObjectiveCompleted(53, True)",
            "SetObjectiveDisplayed(60, True)",
        ),
        70: (
            "SetObjectiveCompleted(55, True)",
            "SetObjectiveCompleted(57, True)",
            "SetObjectiveCompleted(59, True)",
            "SetObjectiveDisplayed(70, True)",
        ),
        90: ("SetObjectiveCompleted(60, True)",),
        100: (
            "SetObjectiveCompleted(60, True)",
            "SetObjectiveCompleted(70, True)",
            "SetObjectiveDisplayed(100, True)",
        ),
        150: ("SetObjectiveCompleted(100, True)", "SetObjectiveDisplayed(150, True)"),
        200: ("SetObjectiveCompleted(150, True)", "SetObjectiveDisplayed(200, True)"),
        210: ("SetObjectiveCompleted(200, True)", "SetObjectiveDisplayed(210, True)"),
        240: ("SetObjectiveCompleted(210, True)", "SetObjectiveDisplayed(240, True)"),
        250: ("SetObjectiveCompleted(240, True)", "SetObjectiveDisplayed(250, True)"),
        300: ("SetObjectiveCompleted(250, True)", "MTR05_Mother_0300_BeaconDeposited.Start()"),
        308: ("SetStage(310)",),
        310: ("SetObjectiveDisplayed(310, True)",),
        500: ("SetObjectiveCompleted(310, True)", "SetStage(510)"),
        510: ("Stop()",),
    }
    for stage, statements in expected.items():
        body = _member_body(patch, f"fragment_stage_{stage:04d}_item_00")
        for statement in statements:
            assert statement in body

    flags = json.loads(
        (FIXTURE_ROOT / "events_misc_mtr05_stage_flags.json").read_text(
            encoding="utf-8"
        )
    )
    assert flags["source"] == flags["live"]
    assert flags["live"] == {
        "500": ["CompleteQuest"],
        "510": [],
        "999": ["RunOnStop"],
    }
    stage500 = _member_body(patch, "fragment_stage_0500_item_00")
    stage510 = _member_body(patch, "fragment_stage_0510_item_00")
    stage999 = _member_body(patch, "fragment_stage_0999_item_00")
    assert "CompleteQuest()" not in stage500
    assert "CompleteQuest()" not in stage510
    assert "Stop()" in stage510
    assert "Stop()" not in stage999
