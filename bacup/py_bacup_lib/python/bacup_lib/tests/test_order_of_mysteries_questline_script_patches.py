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
CONTRACT_PATH = (
    REPO_ROOT
    / "bacup"
    / "docs"
    / "stub_restoration"
    / "contracts"
    / "order-of-mysteries-questline.md"
)

PATCH_MEMBERS = {
    "MoMHolotapeScript": {
        "onquestinit",
        "actor.onplayerloadgame",
        "onquestshutdown",
        "clearruntimeregistrations",
        "reconcileruntimeregistrations",
        "objectreference.onholotapeplay",
        "objectreference.onitemadded",
        "processholotape",
        "isholotapeprogresscomplete",
        "getplayedholotape",
    },
    "MoM00CorpseScript": {
        "onload",
        "onunload",
        "ondistancelessthan",
        "onactivate",
        "startdiscoveryquest",
    },
    "MoM00QuestScript": {"onstageset"},
    "MoM01QuestScript": {"onstageset"},
    "MoM02AQuestScript": {"onstageset"},
    "MoM02BQuestScript": {
        "onquestinit",
        "actor.onplayerloadgame",
        "onquestshutdown",
        "clearruntimeregistrations",
        "reconcileruntimeregistrations",
        "actor.onplayermodarmorweapon",
        "actor.onkill",
        "trackdistinctcreaturetype",
        "haskilledactortype",
        "recordkilledactortype",
    },
    "MoM03QuestScript": {
        "onquestinit",
        "actor.onplayerloadgame",
        "onquestshutdown",
        "clearruntimeregistrations",
        "reconcileruntimeregistrations",
        "actor.onlocationchange",
        "objectreference.ontriggerenter",
    },
    "MoM04QuestScript": {
        "onquestinit",
        "actor.onplayerloadgame",
        "onquestshutdown",
        "onstageset",
        "clearruntimeregistrations",
        "reconcileruntimeregistrations",
        "actor.onlocationchange",
        "objectreference.ontriggerenter",
        "populatebattlesitecorpses",
    },
    "MoM02BSwordActivatorScript": {
        "onload",
        "onactivate",
        "requestswordvisibility",
    },
    "MoMParlorEntryTriggerScript": {
        "ontriggerenter",
        "startinitiatequest",
    },
    "Fragments:Quests:QF_MoM00_00345D50": {
        "fragment_stage_0020_item_00",
        "fragment_stage_0021_item_00",
        "fragment_stage_0031_item_00",
        "fragment_stage_0040_item_00",
        "fragment_stage_0050_item_00",
        "fragment_stage_0060_item_00",
        "fragment_stage_0100_item_00",
    },
    "Fragments:Quests:QF_MoM01_00345D52": {
        "fragment_stage_0010_item_00",
        "fragment_stage_0030_item_00",
        "fragment_stage_0040_item_00",
        "fragment_stage_0050_item_00",
        "fragment_stage_0057_item_00",
        "fragment_stage_0058_item_00",
        "fragment_stage_0059_item_00",
        "fragment_stage_0060_item_00",
        "fragment_stage_0062_item_00",
        "fragment_stage_0070_item_00",
        "fragment_stage_0080_item_00",
        "fragment_stage_0085_item_00",
        "fragment_stage_0090_item_00",
        "fragment_stage_0100_item_00",
    },
    "Fragments:Quests:QF_MoM02_003472F4": {
        "fragment_stage_0010_item_00",
        "fragment_stage_0020_item_00",
        "fragment_stage_0021_item_00",
        "fragment_stage_0022_item_00",
        "fragment_stage_0023_item_00",
        "fragment_stage_0090_item_00",
        "fragment_stage_0100_item_00",
    },
    "Fragments:Quests:QF_MoM02A_003472F5": {
        "fragment_stage_0001_item_00",
        "fragment_stage_0020_item_00",
        "fragment_stage_0030_item_00",
        "fragment_stage_0040_item_00",
        "fragment_stage_0041_item_00",
        "fragment_stage_0042_item_00",
        "fragment_stage_0045_item_00",
        "fragment_stage_0046_item_00",
        "fragment_stage_0047_item_00",
        "fragment_stage_0050_item_00",
        "fragment_stage_0100_item_00",
        "fragment_stage_0255_item_00",
    },
    "Fragments:Quests:QF_MoM02B_00357E7B": {
        "fragment_stage_0001_item_00",
        "fragment_stage_0020_item_00",
        "fragment_stage_0030_item_00",
        "fragment_stage_0032_item_00",
        "fragment_stage_0040_item_00",
        "fragment_stage_0045_item_00",
        "fragment_stage_0048_item_00",
        "fragment_stage_0049_item_00",
        "fragment_stage_0050_item_00",
        "fragment_stage_0060_item_00",
        "fragment_stage_0070_item_00",
        "fragment_stage_0090_item_00",
        "fragment_stage_0100_item_00",
        "fragment_stage_0255_item_00",
    },
    "Fragments:Quests:QF_MoM02C_00357E7D": {
        "fragment_stage_0001_item_00",
        "fragment_stage_0020_item_00",
        "fragment_stage_0030_item_00",
        "fragment_stage_0040_item_00",
        "fragment_stage_0050_item_00",
        "fragment_stage_0052_item_00",
        "fragment_stage_0060_item_00",
        "fragment_stage_0070_item_00",
        "fragment_stage_0080_item_00",
        "fragment_stage_0100_item_00",
        "fragment_stage_0255_item_00",
    },
    "Fragments:Quests:QF_MoM03_00357E7E": {
        "fragment_stage_0020_item_00",
        "fragment_stage_0030_item_00",
        "fragment_stage_0040_item_00",
        "fragment_stage_0050_item_00",
        "fragment_stage_0055_item_00",
        "fragment_stage_0060_item_00",
        "fragment_stage_0070_item_00",
        "fragment_stage_0075_item_00",
        "fragment_stage_0080_item_00",
        "fragment_stage_0085_item_00",
        "fragment_stage_0090_item_00",
        "fragment_stage_0091_item_00",
        "fragment_stage_0100_item_00",
    },
    "Fragments:Quests:QF_MoM04_00357E7A": {
        "fragment_stage_0020_item_00",
        "fragment_stage_0030_item_00",
        "fragment_stage_0040_item_00",
        "fragment_stage_0041_item_00",
        "fragment_stage_0042_item_00",
        "fragment_stage_0043_item_00",
        "fragment_stage_0045_item_00",
        "fragment_stage_0050_item_00",
        "fragment_stage_0051_item_00",
        "fragment_stage_0052_item_00",
        "fragment_stage_0060_item_00",
        "fragment_stage_0090_item_00",
        "fragment_stage_0100_item_00",
    },
    "Fragments:Terminals:TERM_MoM_Cryptos_Terminal_000471C5": {
        "setlogin",
        "fragment_terminal_01",
        "fragment_terminal_02",
        "fragment_terminal_03",
        "fragment_terminal_04",
        "fragment_terminal_05",
    },
    "Fragments:Terminals:TERM_MoM_Cryptos_ViewMission_00347309": {
        "startmission",
        "fragment_terminal_01",
        "fragment_terminal_02",
        "fragment_terminal_03",
        "fragment_terminal_04",
    },
    "Fragments:Terminals:TERM_MoM_Cryptos_Administrat_003694FF": {
        "fragment_terminal_03"
    },
}

ADJACENT_RECORD_DEPENDENCIES = {
    "MoMParlorLaserGridManagerScript": {
        "Bool lock_UpdateLaserGrids",
        "Bool laserGridsNeedUpdate",
        "quest Property MoMMaster",
    },
    "MoMParlorSecretEntranceScript": {
        "keyword Property LinkDoor",
        "keyword Property LinkTrigger",
        "quest Property MoMMaster",
        "actorvalue Property MoMRank",
    },
    "MoM00RECorpseAliasScript": {
        "Int CONST_CleanupTimerDelay",
        "Int CONST_CleanupTimerID",
        "keyword Property MoM00ActiveCorpseKeyword",
    },
}

DECLARATION_ONLY_DATA_CONTAINERS = {
    "MoMMasterQuestScript": {
        "MoMQuestData[] Property MoMQuestList",
        "refcollectionalias Property RiversideManorLaserGrids",
        "actorvalue Property MoMRank",
    },
}

SERVER_SIDE_CHECKPOINT_CARRIERS = {
    "MoMParentQuestScript": {
        "Bool Property InitializingCheckpoints",
        "referencealias Property MoMHoldingContainer",
        "referencealias[] Property ReferenceAliasesToCleanup",
    },
    "MoM02QuestScript": {
        "quest Property MoM02A",
        "actorvalue Property MoM02B_CheckpointValue",
        "quest Property MoM02C",
    },
    "MoM02CQuestScript": {
        "actorvalue Property MoM02CTerminalValue",
        "Int Property CONST_MoM02CValue_ReadyForExfiltration",
        "Int Property CONST_MoM02CValue_ReadyForFabrication",
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
        if name == member_name.lower()
    )
    return "\n".join(lines[start : end + 1])


def _contract_section(contract: str, heading: str) -> str:
    marker = f"## {heading}\n"
    start = contract.index(marker) + len(marker)
    end = contract.find("\n## ", start)
    return contract[start:] if end == -1 else contract[start:end]


def _contract_row(section: str, script_name: str) -> str:
    marker = f"| `{script_name}` |"
    return next(line for line in section.splitlines() if line.startswith(marker))


def _merged_production_source(script_name: str) -> str:
    source_path = SOURCE_ROOT / _script_relative_path(script_name, ".psc")
    patch = _script_patch_source(script_name)
    assert source_path.is_file(), source_path
    assert patch is not None
    return _merge_script_method_patches(
        source_path.read_text(encoding="utf-8"), patch
    )


@pytest.mark.parametrize(
    ("script_name", "declarations"), ADJACENT_RECORD_DEPENDENCIES.items()
)
def test_evidence_blocked_order_of_mysteries_helpers_are_not_speculatively_patched(
    script_name: str, declarations: set[str]
):
    source_path = SOURCE_ROOT / _script_relative_path(script_name, ".psc")
    source = source_path.read_text(encoding="utf-8")
    assert _script_patch_source(script_name) is None
    assert not _member_names(source)
    for declaration in declarations:
        assert declaration in source


@pytest.mark.parametrize(
    ("script_name", "declarations"), DECLARATION_ONLY_DATA_CONTAINERS.items()
)
def test_order_of_mysteries_data_containers_need_no_invented_members(
    script_name: str, declarations: set[str]
):
    source_path = SOURCE_ROOT / _script_relative_path(script_name, ".psc")
    source = source_path.read_text(encoding="utf-8")
    assert _script_patch_source(script_name) is None
    assert not _member_names(source)
    for declaration in declarations:
        assert declaration in source


@pytest.mark.parametrize(
    ("script_name", "declarations"), SERVER_SIDE_CHECKPOINT_CARRIERS.items()
)
def test_server_side_checkpoint_carriers_need_no_invented_client_members(
    script_name: str, declarations: set[str]
):
    source_path = SOURCE_ROOT / _script_relative_path(script_name, ".psc")
    source = source_path.read_text(encoding="utf-8")
    assert _script_patch_source(script_name) is None
    assert not _member_names(source)
    for declaration in declarations:
        assert declaration in source


@pytest.mark.parametrize(("script_name", "members"), PATCH_MEMBERS.items())
def test_order_of_mysteries_patches_are_member_fragments(
    script_name: str, members: set[str]
):
    patch = _script_patch_source(script_name)
    assert patch is not None
    assert set(_member_names(patch)) == members
    assert not any(
        line.strip().lower().startswith(("scriptname ", "state "))
        for line in patch.splitlines()
    )
    assert not any(
        " property " in f" {line.strip().lower()} "
        for line in patch.splitlines()
    )


@pytest.mark.parametrize(("script_name", "members"), PATCH_MEMBERS.items())
def test_order_of_mysteries_patches_merge_once_and_compile(
    script_name: str, members: set[str]
):
    patch = _script_patch_source(script_name)
    assert patch is not None
    merged = _merged_production_source(script_name)
    names = _member_names(merged)
    for member in members:
        assert names.count(member) == 1
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


def test_holotape_rows_target_named_quests_and_preserve_story_start():
    played = _member_body(
        _merged_production_source("MoMHolotapeScript"),
        "objectreference.onholotapeplay",
    )
    acquired = _member_body(
        _merged_production_source("MoMHolotapeScript"),
        "objectreference.onitemadded",
    )
    registrations = _member_body(
        _merged_production_source("MoMHolotapeScript"),
        "reconcileruntimeregistrations",
    )
    body = _member_body(
        _merged_production_source("MoMHolotapeScript"), "processholotape"
    )
    completion = _member_body(
        _merged_production_source("MoMHolotapeScript"),
        "isholotapeprogresscomplete",
    )
    assert "ProcessHolotape" in played
    assert "ProcessHolotape" not in acquired
    assert 'RegisterForRemoteEvent(akItemReference, "OnHolotapePlay")' in acquired
    assert 'RegisterForRemoteEvent(player, "OnHolotapePlay")' not in registrations
    assert ".MoMQuestName == MoMHolotapeData[index].MoMQuestName" in body
    assert ".MoMQuestKeyword" in body
    assert "SendStoryEventAndWait(None, Game.GetPlayer())" in body
    assert "targetQuest.SetStage(" in body
    assert "MoMMaster.SetStage(" not in body
    assert "targetQuest.IsCompleted() || targetQuest.IsStageDone(" in completion


def test_holotape_playback_registration_is_bounded_by_quest_lifecycle():
    merged = _merged_production_source("MoMHolotapeScript")
    played = _member_body(merged, "objectreference.onholotapeplay")
    acquired = _member_body(merged, "objectreference.onitemadded")

    assert "If !IsRunning() || IsCompleted()" in played
    assert played.count('UnregisterForRemoteEvent(akSender, "OnHolotapePlay")') == 2
    assert "If IsHolotapeProgressComplete(playedTape)" in played
    guard_index = played.index("If !IsRunning() || IsCompleted()")
    stopped_unregister_index = played.index(
        'UnregisterForRemoteEvent(akSender, "OnHolotapePlay")', guard_index
    )
    process_index = played.index("ProcessHolotape(playedTape)")
    completed_check_index = played.index(
        "If IsHolotapeProgressComplete(playedTape)"
    )
    completed_unregister_index = played.index(
        'UnregisterForRemoteEvent(akSender, "OnHolotapePlay")', process_index
    )
    assert guard_index < stopped_unregister_index < process_index
    assert process_index < completed_check_index < completed_unregister_index
    assert (
        "If IsRunning() && !IsCompleted() && akSender == Game.GetPlayer()"
        in acquired
    )
    assert acquired.count(
        'RegisterForRemoteEvent(akItemReference, "OnHolotapePlay")'
    ) == 1


def test_each_story_handoff_uses_the_live_master_list_keyword():
    handoffs = {
        "MoMParlorEntryTriggerScript": ("startinitiatequest", "MoMQuestList[2]", "SetStage(10)"),
        "Fragments:Quests:QF_MoM01_00345D52": ("fragment_stage_0100_item_00", "MoMQuestList[3]", "SetStage(10)"),
        "Fragments:Quests:QF_MoM02_003472F4": ("fragment_stage_0100_item_00", "MoMQuestList[7]", "SetStage(20)"),
        "Fragments:Quests:QF_MoM03_00357E7E": ("fragment_stage_0100_item_00", "MoMQuestList[8]", "SetStage(20)"),
    }
    for script_name, (member, list_index, stage_call) in handoffs.items():
        body = _member_body(_merged_production_source(script_name), member)
        assert list_index in body
        assert ".MoMQuestKeyword" in body
        assert "SendStoryEventAndWait" in body
        assert stage_call in body


def test_event_scoped_quests_never_use_direct_start_or_an_invented_grid_manager_abi():
    patch_text = "\n".join(
        _script_patch_source(script_name) or "" for script_name in PATCH_MEMBERS
    )
    assert ".Start(" not in patch_text
    assert "MoMParlorLaserGridManagerScript" not in patch_text
    assert "RequestReevaluateConditions" not in patch_text


def test_adjacent_dependency_contract_captures_exact_live_topology_and_limits():
    contract = CONTRACT_PATH.read_text(encoding="utf-8")
    bounded = _contract_section(
        contract, "Bounded adjacent-carrier dispositions"
    )
    laser_grid = _contract_section(contract, "Laser-grid evidence boundary")
    core = contract[: contract.index("## Bounded adjacent-carrier dispositions")]

    chain = "002C68CB -> 00008D0D -> 00002A26 -> 002F16EA -> 002F16F1"
    assert chain in laser_grid
    assert contract.count(chain) == 1
    assert chain not in bounded
    assert chain not in core
    assert "the last grid has" in laser_grid
    assert "no successor" in laser_grid

    entrance = _contract_row(bounded, "MoMParlorSecretEntranceScript")
    reciprocal_sets = (
        "`0011BC3D` / `0025435B` uses triggers `003B3729` / `003B41F6` and teleport doors `00357EBE` / `00357EBF`",
        "`0011BC3C` / `0025435C` uses `003B3727` / `003B41F7` and `00357EBB` / `00357EC1`",
        "`0011BC3B` / `0025435D` uses `003B3728` / `003B41F5` and `00357EBD` / `00357EC0`",
    )
    for reciprocal_set in reciprocal_sets:
        assert entrance.count(reciprocal_set) == 1
        assert contract.count(reciprocal_set) == 1
    assert entrance.count("`0011BC") == 3
    assert "Each teleport-door pair retains reciprocal `XTEL`" in entrance
    assert "The exact callback remains unproven" in entrance
    assert "do not invent occupancy, rank, animation, failure-topic, or linked-door sequencing" in entrance
    assert "record-dependency" in entrance
    assert all(reciprocal_set not in laser_grid for reciprocal_set in reciprocal_sets)

    corpse = _contract_row(bounded, "MoM00RECorpseAliasScript")
    for corpse_topology in (
        "temporary alias 14 `MoMCorpse`",
        "60-second cleanup delay, timer ID 1",
        "encounter allows three instances",
        "only stop stage 1000",
        "repaired stop fragment re-arms the encounter trigger",
    ):
        assert corpse_topology in corpse
        assert contract.count(corpse_topology) == 1
    for unresolved_contract in (
        "Nothing establishes `OnAliasInit` versus `OnLoad`",
        "cancellation on alias clear",
        "save/load reconciliation",
    ):
        assert unresolved_contract in corpse
    assert "record-dependency" in corpse
    assert "stop stage 1000" not in laser_grid

    for unresolved_contract in (
        "does not invent a manager update callback",
        "occupancy",
        "access evaluation",
        "equipment-to-",
        "load/equip reconciliation",
        "The manager remains record-dependency/evidence-blocked",
    ):
        assert unresolved_contract in laser_grid
    assert "record-dependency" in laser_grid


def test_novice_parent_waits_for_all_three_child_reports():
    parent = _script_patch_source("Fragments:Quests:QF_MoM02_003472F4") or ""
    assert "IsStageDone(22) && IsStageDone(23)" in _member_body(
        parent, "fragment_stage_0021_item_00"
    )
    assert "IsStageDone(21) && IsStageDone(23)" in _member_body(
        parent, "fragment_stage_0022_item_00"
    )
    assert "IsStageDone(21) && IsStageDone(22)" in _member_body(
        parent, "fragment_stage_0023_item_00"
    )

    child_expectations = {
        "Fragments:Quests:QF_MoM02A_003472F5": "CONST_MoM02_CompletedMoM02A",
        "Fragments:Quests:QF_MoM02B_00357E7B": "CONST_MoM02_CompletedMoM02B",
        "Fragments:Quests:QF_MoM02C_00357E7D": "CONST_MoM02_CompletedMoM02C",
    }
    for script_name, stage_constant in child_expectations.items():
        body = _member_body(
            _merged_production_source(script_name), "fragment_stage_0100_item_00"
        )
        assert "MoMQuestList[3].MoMQuest" in body
        assert stage_constant in body


def test_completed_mom_quests_stop_and_release_their_quest_aliases():
    quest_fragments = (
        "Fragments:Quests:QF_MoM00_00345D50",
        "Fragments:Quests:QF_MoM01_00345D52",
        "Fragments:Quests:QF_MoM02_003472F4",
        "Fragments:Quests:QF_MoM02A_003472F5",
        "Fragments:Quests:QF_MoM02B_00357E7B",
        "Fragments:Quests:QF_MoM02C_00357E7D",
        "Fragments:Quests:QF_MoM03_00357E7E",
        "Fragments:Quests:QF_MoM04_00357E7A",
    )
    for script_name in quest_fragments:
        body = _member_body(
            _merged_production_source(script_name), "fragment_stage_0100_item_00"
        )
        assert "Stop()" in body


def test_mom02c_terminal_flow_uses_local_quest_stages_not_server_actor_values():
    terminal_scripts = (
        "Fragments:Terminals:TERM_MoM02C_AdvancedResearch_00366711",
        "Fragments:Terminals:TERM_MoM02C_AdvancedResearch_00366716",
        "Fragments:Terminals:TERM_MoM02C_AdvancedResearch_00366712",
        "Fragments:Terminals:TERM_MoM02C_AdvancedResearch_00366717",
        "Fragments:Terminals:TERM_MoM02C_SigIntAnalysisTe_00366715",
        "Fragments:Terminals:TERM_MoM02C_DataExfiltration_00357FB5",
    )
    patch_text = "\n".join(
        _script_patch_source(script_name) or "" for script_name in terminal_scripts
    )
    assert "MoM02CTerminalValue" not in patch_text
    assert "targetQuest.SetStage" in patch_text
    for stage_constant in (
        "CONST_MoM02C_SearchedForTarget",
        "CONST_MoM02C_LearnedAboutSiphon",
        "CONST_MoM02C_GotExfiltrationHolotape",
        "CONST_MoM02C_SuccessfullyExtractedData",
        "CONST_MoM02C_UploadedData",
    ):
        assert stage_constant in patch_text


def test_forging_a_legend_restores_exact_mod_and_distinct_kill_contracts():
    quest = _merged_production_source("MoM02BQuestScript")
    modified = _member_body(quest, "actor.onplayermodarmorweapon")
    killed = _member_body(quest, "actor.onkill")
    tracker = _member_body(quest, "trackdistinctcreaturetype")
    assert "akBaseObject == MoM02BHistoricSword" in modified
    assert "akModBaseObject == MoM02BSwingAnalyzerMod" in modified
    assert "akSender.GetEquippedWeapon() != MoM02BHistoricSword" in killed
    assert "!HasKilledActorType(actorTypeName)" in tracker
    assert "killObjectiveCurrent >= CONST_KillObjectiveTotal" in tracker
    assert "SetStage(CONST_CHECKPOINT_MoM02B_FabricateBlade)" in tracker


def test_mistress_battle_stage_keeps_travel_before_body_search():
    quest = _script_patch_source("Fragments:Quests:QF_MoM04_00357E7A") or ""
    clues = _member_body(quest, "fragment_stage_0045_item_00")
    arrived = _member_body(quest, "fragment_stage_0050_item_00")
    assert "SetObjectiveCompleted(40)" not in clues
    assert "SetObjectiveDisplayed(40)" in clues
    assert "SetObjectiveCompleted(40)" in arrived
    assert "SetObjectiveDisplayed(50)" in arrived


def test_initiate_authorization_targets_mom01_not_mom02():
    body = _member_body(
        _merged_production_source(
            "Fragments:Terminals:TERM_MoM_Cryptos_Administrat_003694FF"
        ),
        "fragment_terminal_03",
    )
    assert "MoMQuestList[2].MoMQuest" in body
    assert "CONST_MoM01_AuthorizedPromotion" in body
    assert "CONST_MoM02_AuthorizedPromotion" not in body
