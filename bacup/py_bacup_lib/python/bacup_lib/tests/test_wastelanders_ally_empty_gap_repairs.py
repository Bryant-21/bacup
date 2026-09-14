from __future__ import annotations

import os
from pathlib import Path

import pytest

from bacup_lib.workflows.unified import (
    _augment_fo76_to_fo4_script_skeleton,
    _iter_top_level_papyrus_members,
    _merge_script_method_patches,
    _script_patch_source,
    _script_relative_path,
)
from creation_lib.pex.native_runtime import compile_psc


REPO_ROOT = Path(__file__).resolve().parents[5]
SOURCE_ROOT = REPO_ROOT / "mods" / "SeventySix" / "Scripts" / "Source" / "User"
CONTRACT = (
    REPO_ROOT
    / "bacup"
    / "docs"
    / "stub_restoration"
    / "contracts"
    / "wastelanders-ally-empty-gap-repair-2026-09-03.md"
)

SKELETONS = {
    "COMP_PlayerAcceptsQuestInfoScript": """Scriptname COMP_PlayerAcceptsQuestInfoScript Extends TopicInfo
""",
    "CompanionRQStageHandlerScript": """Scriptname CompanionRQStageHandlerScript Extends Actor
Struct RQDatum
    Float RequiredQuestCount
    Float IgnoreOnValue = -1.0
    ActorValue IgnoreOnPlayerActorValue
    Float ValueToSet = 1.0
    ActorValue PlayerActorValueToSet
    Int Stage
    Quest BaseQuest
EndStruct
ActorValue Property COMP_QuestCount Auto Mandatory
RQDatum[] Property RQData Auto Mandatory
""",
    "COMP_RQ_DialogueVariable_QuestScript": """Scriptname COMP_RQ_DialogueVariable_QuestScript Extends Quest
ReferenceAlias Property Alias_Ally Auto Mandatory
ActorValue Property COMP_RQ_FlavorVariable_IntelSource Auto Mandatory
ActorValue Property COMP_RQ_FlavorVariable_Enemies Auto Mandatory
Int Property max_Target = 3 Auto
Int Property min_Target = 1 Auto
ActorValue Property COMP_RQ_FlavorVariable_Target Auto Mandatory
Int Property max_QuestType = 3 Auto
Int Property min_QuestType = 1 Auto
ActorValue Property COMP_RQ_FlavorVariable_QuestType Auto Mandatory
Int Property max_Location = 3 Auto
Int Property min_Location = 1 Auto
Int Property min_AcceptQuest = 1 Auto
ActorValue Property COMP_RQ_FlavorVariable_Location Auto Mandatory
Int Property max_IntelSource = 3 Auto
Int Property min_IntelSource = 1 Auto
ActorValue Property COMP_RQ_FlavorVariable_AcceptQuest Auto Mandatory
Int Property max_AcceptQuest = 3 Auto
Int Property min_Enemies = 1 Auto
Int Property max_Enemies = 3 Auto
""",
    "Fragments:Quests:QF_COMP_Quest_Camp_Full_Astronaut": """Scriptname Fragments:Quests:QF_COMP_Quest_Camp_Full_Astronaut Extends Quest Hidden
Potion Property SuperStimpak Auto Mandatory
ReferenceAlias Property Alias_Player Auto Mandatory
Potion Property DilutedStimpak Auto Mandatory
ActorValue Property AV_QuestCount Auto
Quest Property pCOMP_Quest_Outtro_Full_Astronaut Auto Mandatory
ActorValue Property AV_StrengthEnduranceAgility Auto Mandatory
ActorValue Property AV_Charisma Auto Mandatory
ActorValue Property AV_Luck Auto Mandatory
ActorValue Property AV_PerceptionIntelligence Auto Mandatory
Potion Property Stimpak Auto Mandatory
ActorValue Property AV_PlayerKnows_WhalesongIsFake Auto Mandatory
ActorValue Property AV_PlayerKnows_GutsyHolotape Auto Mandatory
ActorValue Property AV_PlayerGave_Stimpak Auto Mandatory
""",
    "Fragments:Quests:QF_COMP_Astronaut_Quest_Outt_0056BF31": """Scriptname Fragments:Quests:QF_COMP_Astronaut_Quest_Outt_0056BF31 Extends Quest Hidden
ReferenceAlias Property Alias_Player Auto Mandatory
ReferenceAlias Property Alias_Astronaut_Instanced Auto Mandatory
ReferenceAlias Property Alias_XMarker_ATHENAroom_Entrance Auto Mandatory
ActorValue Property COMP_AV_Astronaut_Outtro_MadeATHENAChoice Auto Mandatory
ActorValue Property COMP_AV_Astronaut_FinaleComplete Auto Mandatory
ReferenceAlias Property Alias_AssaultronMarker Auto Mandatory
ActorValue Property COMP_AV_Astronaut_PlayerChoice_SavedAthena Auto Mandatory
ReferenceAlias Property Alias_LightMarker Auto Mandatory
Scene Property COMP_Astronaut_Quest_Outtro_TransferScene Auto Mandatory
ReferenceAlias Property Alias_Athena Auto Mandatory
Sound Property QSTMassFusionPowerDown Auto Mandatory
Scene Property COMP_Astronaut_Quest_Outtro_ATHENAShutdown Auto Mandatory
ReferenceAlias Property Alias_MapMarker Auto Mandatory
ReferenceAlias Property Alias_TransferSoundMarker Auto Mandatory
ReferenceAlias Property Alias_Astronaut Auto Mandatory
Scene Property COMP_Astronaut_Quest_Outtro_CHOICE Auto Mandatory
ReferenceAlias Property Alias_Emerson_Instanced Auto Mandatory
ReferenceAlias Property Alias_Assaultron_Instanced Auto Mandatory
ActorValue Property AV_PlayerKnows_BlueSunset Auto Mandatory
Key Property COMP_Astronaut_Outtro_Keycard_ATHENA Auto Mandatory
""",
}


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


def _merged(script_name: str) -> str:
    patch = _script_patch_source(script_name)
    assert patch is not None
    if script_name in SKELETONS:
        skeleton = SKELETONS[script_name]
    else:
        source_path = SOURCE_ROOT / _script_relative_path(script_name, ".psc")
        skeleton = _augment_fo76_to_fo4_script_skeleton(
            script_name, source_path.read_text(encoding="utf-8")
        )
    return _merge_script_method_patches(skeleton, patch)


def _member_names(source: str) -> list[str]:
    return [
        name
        for _kind, name, _start, _end in _iter_top_level_papyrus_members(
            source.splitlines()
        )
    ]


@pytest.mark.parametrize(
    "script_name",
    [
        "CompanionScript",
        *SKELETONS,
    ],
)
def test_ally_gap_patches_merge_idempotently(script_name: str) -> None:
    patch = _script_patch_source(script_name)
    assert patch is not None
    assert "scriptname " not in patch.lower()
    merged = _merged(script_name)
    assert merged.lower().count("scriptname ") == 1
    for member in _member_names(patch):
        assert _member_names(merged).count(member) == 1
    assert _merge_script_method_patches(merged, patch) == merged


def test_all_equal_threshold_rows_are_considered_without_first_match_bias() -> None:
    companion = _merged("CompanionScript")
    selection = companion.split("Int Function FindRadiantQuestDataIndex", 1)[1].split(
        "EndFunction", 1
    )[0]

    assert "Int eligibleCount = 0" in selection
    assert "eligibleCount += 1" in selection
    assert "Utility.RandomInt(1, eligibleCount) == 1" in selection
    assert "selectedIndex = index" in selection
    assert "Return index" not in selection
    assert selection.rstrip().endswith("Return selectedIndex")


def test_proven_acceptance_binding_and_companion_owned_milestone_count() -> None:
    accept = _merged("COMP_PlayerAcceptsQuestInfoScript")
    companion = _merged("CompanionScript")
    milestones = _merged("CompanionRQStageHandlerScript")
    contract = CONTRACT.read_text(encoding="utf-8")

    assert "Event OnEnd(ObjectReference akSpeakerRef, Bool abHasBeenSaid)" in accept
    assert "If akSpeakerRef == Game.GetPlayer()" in accept
    assert "(akSpeakerRef as Actor).GetDialogueTarget()" in accept
    assert "companionActor = akSpeakerRef as Actor" in accept
    assert "companionActor as CompanionScript" in accept
    assert "companion.SetRQAcceptanceStage()" in accept
    acceptance = companion.split("Function SetRQAcceptanceStage()", 1)[1].split(
        "EndFunction", 1
    )[0]
    assert "QuestStageAcceptQuest" in acceptance
    assert "QuestStageReturnToQuestGiver" not in acceptance
    assert 'RegisterForRemoteEvent(RQData[index].BaseQuest, "OnStageSet")' in milestones
    assert "currentDatum.BaseQuest == akSender" in milestones
    assert "GetValue(COMP_QuestCount) < currentDatum.RequiredQuestCount" in milestones
    assert "player.GetValue(COMP_QuestCount)" not in milestones
    assert "player.SetValue(currentDatum.PlayerActorValueToSet" in milestones
    assert "**122 live INFO bindings**" in contract
    assert "**175 live INFO bindings**" in contract
    for form_id in ("5825EF", "582AE3", "582ADB", "582ADF"):
        assert form_id in contract


def test_request_start_carrier_is_not_fabricated() -> None:
    companion = _merged("CompanionScript")
    daily = companion.split("Function StartDailyQuest()", 1)[1].split(
        "EndFunction", 1
    )[0]
    contract = CONTRACT.read_text(encoding="utf-8")

    assert "OutroQuestStartKeyword.SendStoryEventAndWait" not in daily
    assert "no client-side caller" in contract
    # The return producer is a separate edge and is now implemented; it must not
    # be turned into a request/start carrier to compensate for the missing one.
    ret = _script_patch_source("COMP_PlayerReturnToQuestGiverInfo")
    assert ret is not None
    assert ".Start()" not in ret
    assert "StartDailyQuest" not in ret
    assert "StartRadiantQuestByIndex" not in ret


def test_astronaut_controllers_cover_start_choice_wrapup_and_completion() -> None:
    camp = _merged("Fragments:Quests:QF_COMP_Quest_Camp_Full_Astronaut")
    outro = _merged("Fragments:Quests:QF_COMP_Astronaut_Quest_Outt_0056BF31")

    assert "pCOMP_Quest_Outtro_Full_Astronaut.SetStage(9100)" in camp
    assert "SetStage(9999)" in camp
    assert "COMP_Astronaut_Outtro_Keycard_ATHENA" in outro
    assert "COMP_Astronaut_Quest_Outtro_CHOICE.Start()" in outro
    assert "COMP_Astronaut_Quest_Outtro_ATHENAShutdown.Start()" in outro
    assert "COMP_Astronaut_Quest_Outtro_TransferScene.Start()" in outro
    stage_3190 = outro.split("Function Fragment_Stage_3190_Item_00()", 1)[1].split(
        "EndFunction", 1
    )[0]
    stage_3520 = outro.split("Function Fragment_Stage_3520_Item_00()", 1)[1].split(
        "EndFunction", 1
    )[0]
    assert "SetStage(5000)" not in stage_3190
    assert "SetStage(5000)" not in stage_3520
    assert "astronaut.CampQuest.SetStage(astronaut.CampQuestCompletionStage)" in outro
    assert "CompleteQuest()" in outro
    assert "SetStage(9999)" in outro
    assert "Stop()" in outro


def test_all_43_rows_are_mapped_without_per_title_script_patches() -> None:
    contract = CONTRACT.read_text(encoding="utf-8")

    assert "43-row identity map" in contract
    assert contract.count("| LCTN |") == 41
    assert contract.count("| QUST |") == 2
    assert "0059C300" in contract
    assert "0059C303" in contract
    assert "005A2715" in contract
    assert "005A2714" in contract
    assert "005A2713" in contract


@pytest.mark.parametrize(
    "script_name",
    [
        "CompanionRQStageHandlerScript",
        "COMP_RQ_DialogueVariable_QuestScript",
        "COMP_PlayerAcceptsQuestInfoScript",
        "Fragments:Quests:QF_COMP_Quest_Camp_Full_Astronaut",
        "Fragments:Quests:QF_COMP_Astronaut_Quest_Outt_0056BF31",
        "CompanionScript",
    ],
)
def test_ally_gap_full_merged_sources_compile_for_fo4(
    script_name: str, tmp_path: Path
) -> None:
    base_source = _fo4_base_source()
    if base_source is None:
        pytest.skip("FO4 base Papyrus sources unavailable")

    merged_root = tmp_path / "merged"
    merged_root.mkdir(exist_ok=True)
    for dependency in ("CompanionScript", "CompanionRQStageHandlerScript"):
        (merged_root / f"{dependency}.psc").write_text(
            _merged(dependency), encoding="utf-8"
        )

    result = compile_psc(
        _merged(script_name),
        imports=[str(merged_root), str(SOURCE_ROOT), str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=str(_script_relative_path(script_name, ".psc")),
    )
    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None
