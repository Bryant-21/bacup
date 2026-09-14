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
QUEST_SCRIPT = "Fragments:Quests:QF_BoS02_0004E89C"

QUEST_STAGE_IDS = {
    1,
    2,
    100,
    200,
    250,
    300,
    400,
    500,
    510,
    520,
    530,
    540,
    551,
    552,
    553,
    554,
    561,
    562,
    563,
    564,
    600,
    800,
    900,
    1000,
    1100,
    1200,
    1400,
    1450,
    1500,
    1600,
    1700,
    1750,
    1800,
    1900,
}

SCRIPT_MEMBERS = {
    QUEST_SCRIPT: {
        f"fragment_stage_{stage_id:04d}_item_00" for stage_id in QUEST_STAGE_IDS
    }
    | {"bos02_trystartbos03", "ontimer"},
    "BoS02TriggerScript": {"ontriggerenter"},
    "BoS02PCSoldierInventoryScript": {"onaliasinit", "onitemadded"},
    "BoS02_DMV_SupportScript": {"onquestinit", "onquestshutdown"},
    "Fragments:Terminals:TERM_BoS02DMVMainTerminalSub_00274582": {
        "fragment_terminal_01",
        "fragment_terminal_02",
        "fragment_terminal_03",
        "fragment_terminal_04",
    },
    "Fragments:Terminals:TERM_BoS02DMVMainTerminalSub_00274586": {
        "fragment_terminal_01",
        "fragment_terminal_02",
        "fragment_terminal_03",
        "fragment_terminal_04",
    },
    "Fragments:Terminals:TERM_BoS02DMVMainTerminalSub_0027459E": {
        "fragment_terminal_01",
        "fragment_terminal_02",
        "fragment_terminal_03",
        "fragment_terminal_04",
    },
    "Fragments:Terminals:TERM_BoS02DMVMainTerminalSub_002A6DE0": {
        "fragment_terminal_02"
    },
    "Fragments:Terminals:TERM_BoS02DMVNumberTerminal_0027E575": {
        "fragment_terminal_01",
        "fragment_terminal_02",
    },
    "Fragments:Terminals:TERM_BoS02RegistrationTermin_004E4588": {
        "fragment_terminal_01"
    },
    "Fragments:Terminals:TERM_BoS02RegistrationTermin_004E458A": {
        "fragment_terminal_01",
        "fragment_terminal_02",
        "fragment_terminal_03",
        "fragment_terminal_04",
    },
    "Fragments:TopicInfos:TIF_BoS02_DMV_Support_001570C9": {"fragment_end"},
    "Fragments:TopicInfos:TIF_BoS02_DMV_Support_001869EB": {
        "fragment_begin",
        "fragment_end",
    },
    "Fragments:TopicInfos:TIF_BoS02_DMV_Support_00186AFD": {"fragment_end"},
    "Fragments:TopicInfos:TIF_BoS02_DMV_Support_00186B01": {"fragment_end"},
    "Fragments:TopicInfos:TIF_BoS02_DMV_Support_00186B05": {"fragment_end"},
    "Fragments:TopicInfos:TIF_BoS02_DMV_Support_001D8392": {"fragment_begin"},
    "Fragments:Scenes:SF_BoS02_DMV_Support_100_Fir_0027E576": {
        "fragment_begin",
        "fragment_end",
    },
}


def _patch(script_name: str) -> str:
    patch = _script_patch_source(script_name)
    assert patch is not None, script_name
    return patch


def test_bos02_quest_restores_exact_source_bound_stage_members():
    patch = _patch(QUEST_SCRIPT)
    members = {
        name
        for kind, name, _start, _end in _iter_top_level_papyrus_members(
            patch.splitlines()
        )
        if kind in {"function", "event"}
    }

    assert members == SCRIPT_MEMBERS[QUEST_SCRIPT]
    for omitted_stage in (650, 700, 1300):
        assert f"fragment_stage_{omitted_stage:04d}_item_00" not in members


def test_bos02_basic_training_gate_and_certificate_return_are_windowed():
    quest_patch = _patch(QUEST_SCRIPT)
    alias_patch = _patch("BoS02PCSoldierInventoryScript")
    stage_400 = quest_patch.split("Function Fragment_Stage_0400_Item_00()", 1)[1].split(
        "EndFunction", 1
    )[0]

    assert "pEN05_Basic.IsCompleted()" in stage_400
    assert "playerRef.GetItemCount(pBoS02SoldierCertificate) > 0" in stage_400
    assert 'Game.GetFormFromFile(0x00182072, "SeventySix.esm") as Keyword' in stage_400
    assert "startKeyword.SendStoryEventAndWait(None, playerRef, playerRef)" in stage_400
    assert "pEN05_Basic.Start()" not in stage_400
    assert "SetStage(500)" in stage_400

    assert alias_patch.count("owningQuest.GetStage() < 400") == 2
    assert alias_patch.count("owningQuest.GetStage() >= 500") == 2
    assert "Event OnAliasInit()" in alias_patch
    assert "AddInventoryEventFilter(pBoS02SoldierCertificate)" in alias_patch
    assert alias_patch.index("AddInventoryEventFilter") < alias_patch.index(
        "owningQuest.GetStage() < 400"
    )
    assert "Event OnItemAdded(Form akBaseItem" in alias_patch
    assert "akBaseItem == pBoS02SoldierCertificate" in alias_patch
    assert alias_patch.count("owningQuest.SetStage(500)") == 2


def test_bos02_trigger_is_player_only_and_starts_through_story_manager():
    trigger_patch = _patch("BoS02TriggerScript")

    assert trigger_patch.splitlines()[0] == (
        "Event OnTriggerEnter(ObjectReference akActionRef)"
    )
    assert "Actor playerRef = Game.GetPlayer()" in trigger_patch
    assert "akActionRef != playerRef" in trigger_patch
    assert (
        'Game.GetFormFromFile(0x004E458E, "SeventySix.esm") as Keyword' in trigger_patch
    )
    assert (
        "startKeyword.SendStoryEventAndWait(None, playerRef, playerRef)"
        in trigger_patch
    )
    assert "pBoS02.Start()" not in trigger_patch


def test_bos02_dmv_choices_calls_and_registration_are_idempotent():
    name_patch = _patch("Fragments:Terminals:TERM_BoS02DMVMainTerminalSub_00274582")
    address_patch = _patch("Fragments:Terminals:TERM_BoS02DMVMainTerminalSub_0027459E")
    number_patch = _patch("Fragments:Terminals:TERM_BoS02DMVNumberTerminal_0027E575")
    registration_patch = _patch(
        "Fragments:Terminals:TERM_BoS02RegistrationTermin_004E458A"
    )
    support_patch = _patch("BoS02_DMV_SupportScript")

    for stage_id in (510, 520, 530, 540):
        assert f"pBoS02.SetStage({stage_id})" in name_patch
    assert name_patch.count("pBoS02.GetStage() < 600") == 4

    for stage_id in (561, 562, 563, 564):
        assert f"pBoS02.SetStage({stage_id})" in address_patch
    assert address_patch.count("pBoS02.GetStage() >= 600") == 4
    assert (
        address_patch.count("playerRef.GetItemCount(pBoS02_ApplicationForm) == 0") == 4
    )
    assert address_patch.count("pBoS02.SetStage(600)") == 4

    assert "pBoS02.SetStage(650)" in number_patch
    assert "pBoS02.SetStage(1300)" in number_patch
    assert number_patch.count("!pBoS02_DMV_Support.IsRunning()") == 2
    assert support_patch.count(".Start()") == 2
    assert "pBoS02_DMVNumber_42.GetValue() <= 0.0" in support_patch

    assert registration_patch.count("pBoS02.SetStage(1750)") == 4
    assert registration_patch.count("pBoS02.GetStage() < 1750") == 4


def test_bos02_dmv_topic_and_scene_progress_are_repeat_safe_assignments():
    office_close = _patch("Fragments:TopicInfos:TIF_BoS02_DMV_Support_001570C9")
    c42 = _patch("Fragments:TopicInfos:TIF_BoS02_DMV_Support_001869EB")
    a2 = _patch("Fragments:TopicInfos:TIF_BoS02_DMV_Support_00186B01")
    scene = _patch("Fragments:Scenes:SF_BoS02_DMV_Support_100_Fir_0027E576")

    assert "owningQuest.Stop()" in office_close
    assert c42.count("pBoS02_DMVNumber_42.SetValue(1.0)") == 2
    assert "pBoS02_DMVNumber_A3.SetValue(1.0)" in a2
    assert "pBoS02_DeptCCooldown.SetValue(1.0)" in a2
    assert "pBoS02_DeptBCooldown.SetValue(0.0)" in scene
    assert "pBoS02_DMVNumber_42.SetValue(1.0)" in scene
    assert ".Mod(" not in c42 + a2 + scene


def test_bos02_completion_rewards_checkpoint_and_handoff_are_guarded():
    patch = _patch(QUEST_SCRIPT)

    completion = patch.split("Function Fragment_Stage_1900_Item_00()", 1)[1].split(
        "EndFunction", 1
    )[0]
    helper = patch.split("Bool Function BoS02_TryStartBoS03()", 1)[1].split(
        "EndFunction", 1
    )[0]
    retry = patch.split("Event OnTimer(Int aiTimerID)", 1)[1].split(
        "EndEvent", 1
    )[0]
    assert "SetObjectiveCompleted(1800, True)" in completion
    assert "CompleteAllObjectives()" in completion
    assert "playerRef.AddItem(pBoSTechnicalDocument, 1, False)" in completion
    assert (
        "pBoSTechnicalDocument != None && "
        "playerRef.GetItemCount(pBoSTechnicalDocument) == 0"
        in completion
    )
    assert "playerRef.GetValue(pBoS02_CheckpointValue) < 1.0" in completion
    assert "CompleteQuest()" in completion
    assert "BoS02_TryStartBoS03()" in completion
    assert "StartTimer(5.0, 1900)" in completion
    assert "pBoS03.IsRunning() || pBoS03.IsCompleted()" in helper
    assert "pBoS03_QuestStartKeyword.SendStoryEventAndWait" in helper
    assert "accepted ||" in helper
    assert "playerRef.SetValue(pBoS03StartedAV, 1.0)" in helper
    assert "aiTimerID != 1900" in retry
    assert "BoS02_TryStartBoS03()" in retry
    assert "StartTimer(5.0, 1900)" in retry
    assert "pBoS03.Start()" not in completion
    assert "pBoS03.Start()" not in helper


@pytest.mark.parametrize(("script_name", "expected_members"), SCRIPT_MEMBERS.items())
def test_all_bos02_patches_merge_once_and_compile_full_production_source(
    script_name: str, expected_members: set[str]
):
    source_path = SOURCE_ROOT / _script_relative_path(script_name, ".psc")
    assert source_path.is_file(), source_path
    skeleton = source_path.read_text(encoding="utf-8")
    patch = _patch(script_name)

    assert not any(
        line.strip().lower().startswith(("scriptname ", "property "))
        for line in patch.splitlines()
    )
    merged = _merge_script_method_patches(skeleton, patch)
    members = [
        name
        for _kind, name, _start, _end in _iter_top_level_papyrus_members(
            merged.splitlines()
        )
    ]
    for expected_member in expected_members:
        assert members.count(expected_member) == 1
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
