from __future__ import annotations

import os
from pathlib import Path

import pytest

from bacup_lib.workflows.unified import (
    _iter_top_level_papyrus_members,
    _merge_script_method_patches,
    _script_patch_source,
    _script_relative_path,
)
from creation_lib.pex.native_runtime import compile_psc


REPO_ROOT = Path(__file__).resolve().parents[5]
SOURCE_ROOT = REPO_ROOT / "mods" / "SeventySix" / "Scripts" / "Source" / "User"

PATCH_CASES = {
    "EN01_QuestScript": {
        "onquestinit",
        "onstageset",
        "initializeplayer",
        "handleimportantnoteread",
        "checkpowerreset",
        "recordcodeclue",
        "displayknowncodes",
    },
    "EN01_SendImportantNoteEvent": {"onread"},
    "EN01_BypassTapeScript": {"onholotapeplay", "trybypass"},
    "EN01_HandscannerAliasScript": {
        "onactivate",
        "ontimer",
        "scanhandprint",
    },
    "EN01_KeypadFurnScript": {"onactivate", "usekeypad"},
    "EN01_SpecialTestRefScript": {"onactivate"},
    "EN01_BunkerQuestScript": {
        "onquestinit",
        "onquestshutdown",
        "beginreset",
        "finishreset",
        "setcollectionenabled",
        "setenablemarkers",
    },
    "EN01_BunkerAccessPadAliasScript": {
        "onactivate",
        "ontimer",
        "openbunkerdoor",
    },
    "EN01_CheckPlayerForCircuitComp": {
        "onaliasinit",
        "oncontainerchanged",
    },
    "DefaultAliasOnPlayerHolotape": {
        "onholotapeplay",
        "tryadvanceholotape",
    },
    "DefaultAliasSetStageOnMenuItemRun": {
        "onaliasinit",
        "onaliasshutdown",
        "terminal.onmenuitemrun",
        "applyvalue",
    },
    "DefaultShutdownQuestAliasOnChangeLoc": {
        "onlocationchange",
        "containslocation",
        "ispreventedstage",
    },
}

FRAGMENT_SCRIPT = "Fragments:Quests:QF_EN01_Sam_000714FE"
FRAGMENT_STAGES = {
    1,
    2,
    3,
    5,
    10,
    15,
    25,
    30,
    40,
    50,
    60,
    70,
    80,
    83,
    86,
    89,
    90,
    95,
    100,
    102,
    105,
    110,
    112,
    114,
    115,
    118,
    119,
    120,
    150,
    200,
    250,
    260,
    300,
    900,
    950,
}
ALL_PATCH_CASES = (*PATCH_CASES, FRAGMENT_SCRIPT)


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


def _members(source: str) -> set[str]:
    return {
        name
        for _kind, name, _start, _end in _iter_top_level_papyrus_members(
            source.splitlines()
        )
    }


def _merged_source(script_name: str) -> str:
    source_path = SOURCE_ROOT / _script_relative_path(script_name, ".psc")
    patch = _script_patch_source(script_name)
    assert source_path.is_file(), source_path
    assert patch is not None
    return _merge_script_method_patches(
        source_path.read_text(encoding="utf-8"), patch
    )


@pytest.fixture(scope="module")
def merged_import_root(tmp_path_factory: pytest.TempPathFactory) -> Path:
    root = tmp_path_factory.mktemp("en01_bunker_merged_sources")
    for script_name in ALL_PATCH_CASES:
        path = root / _script_relative_path(script_name, ".psc")
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(_merged_source(script_name), encoding="utf-8")
    return root


@pytest.mark.parametrize(("script_name", "expected_members"), PATCH_CASES.items())
def test_en01_patch_supplies_bound_script_behavior(
    script_name: str, expected_members: set[str]
):
    patch = _script_patch_source(script_name)
    assert patch is not None
    assert "Scriptname " not in patch
    assert expected_members <= _members(patch)
    assert expected_members <= _members(_merged_source(script_name))


def test_en01_fragment_patch_restores_every_bound_fragment():
    patch = _script_patch_source(FRAGMENT_SCRIPT)
    assert patch is not None
    members = _members(patch)
    expected = {
        f"fragment_stage_{stage:04d}_item_00" for stage in FRAGMENT_STAGES
    }
    expected.add("fragment_stage_0020_item_01")
    assert expected <= members
    assert len({name for name in members if name.startswith("fragment_stage_")}) == 37
    helpers = {
        "finishbunkerquest",
        "starten02mainquest",
    }
    assert helpers <= members
    assert expected | helpers <= _members(_merged_source(FRAGMENT_SCRIPT))


def test_en01_note_reads_use_bound_note_data_idempotently():
    quest = _script_patch_source("EN01_QuestScript")
    note = _script_patch_source("EN01_SendImportantNoteEvent")
    assert quest is not None
    assert note is not None

    assert "If clue.TargetBook == akBook" in quest
    assert "If !IsStageDone(clue.iStageToSetOnRead)" in quest
    assert "SetStage(clue.iStageToSetOnRead)" in quest
    assert "RecordCodeClue(clue.iStageToSetOnRead)" in quest
    assert "auiStageID == 115 || auiStageID == 117 || auiStageID == 119" in quest
    assert "playerRef.SetValue(clue.myActorValue, 1.0)" in quest
    assert "SetObjectiveDisplayed(clue.iObjectiveIndex, True)" in quest
    assert 'Game.GetFormFromFile(0x000714FE, "SeventySix.esm")' in note
    assert "bunkerScript.HandleImportantNoteRead(GetBaseObject() as Book)" in note
    assert "EN01_Bunker != None && !EN01_Bunker.IsRunning()" in quest
    assert "EN01_Bunker.Start()" in quest


def test_en01_stage_60_reaches_shared_midquest_reward_stage():
    patch = _script_patch_source(FRAGMENT_SCRIPT)
    assert patch is not None
    stage_60 = patch.split("Function Fragment_Stage_0060_Item_00()", 1)[1].split(
        "EndFunction", 1
    )[0]
    stage_110 = patch.split("Function Fragment_Stage_0110_Item_00()", 1)[1].split(
        "EndFunction", 1
    )[0]
    assert "SetStage(61)" in stage_60
    assert "GrantMidQuestReward" not in patch
    assert "Game.RewardPlayerXP" not in patch
    assert "EN01_MidQuestRewardOnce" not in stage_60
    assert "EN01_MidQuestRewardOnce" not in stage_110


def test_en01_completion_defers_branch_rewards_and_preserves_en02_handoff():
    patch = _script_patch_source(FRAGMENT_SCRIPT)
    assert patch is not None
    assert "GiveCompletionRewards" not in patch
    assert 'Game.GetFormFromFile(0x003CC4A4, "SeventySix.esm")' not in patch
    assert 'Game.GetFormFromFile(0x003CA66D, "SeventySix.esm")' not in patch
    assert 'Game.GetFormFromFile(0x00188A97, "SeventySix.esm")' not in patch
    assert 'Game.GetFormFromFile(0x004EABEF, "SeventySix.esm")' not in patch
    assert "playerRef.SetValue(EN01_CompletedValue, 1.0)" in patch
    assert "FinishBunkerQuest(True)" in patch
    assert "FinishBunkerQuest(False)" in patch
    assert "EN02_QuestStartKeyword.SendStoryEvent" in patch
    assert 'Game.GetFormFromFile(0x000293A3, "SeventySix.esm")' not in patch
    assert "en02MainQuest.Start()" not in patch
    assert "en02MainQuest.SetStage" not in patch

    stage_250 = patch.split("Function Fragment_Stage_0250_Item_00()", 1)[1].split(
        "EndFunction", 1
    )[0]
    stage_260 = patch.split("Function Fragment_Stage_0260_Item_00()", 1)[1].split(
        "EndFunction", 1
    )[0]
    assert "FinishBunkerQuest(True)" in stage_250
    assert "FinishBunkerQuest(False)" in stage_260


def test_en01_stage_300_owns_completion_not_branch_rewards():
    patch = _script_patch_source(FRAGMENT_SCRIPT)
    assert patch is not None
    stage_300 = patch.split("Function Fragment_Stage_0300_Item_00()", 1)[1].split(
        "EndFunction", 1
    )[0]
    assert "CompleteQuest()" in stage_300
    assert "GiveCompletionRewards" not in stage_300


def test_en01_repair_preserves_documented_quest_gates():
    fragment = _script_patch_source(FRAGMENT_SCRIPT)
    holotape = _script_patch_source("DefaultAliasOnPlayerHolotape")
    bypass = _script_patch_source("EN01_BypassTapeScript")
    keypad = _script_patch_source("EN01_KeypadFurnScript")
    assert fragment is not None
    assert holotape is not None
    assert bypass is not None
    assert keypad is not None

    assert "Event OnItemAdded" not in holotape
    assert "Event OnHolotapePlay" in holotape
    assert "playerRef.GetDistance(triggerRef) > 1024.0" in bypass
    assert "IsStageDone(83) && IsStageDone(86) && IsStageDone(89)" in fragment
    assert "If bunkerQuest.IsStageDone(112)" in keypad
    for stage in (122, 124, 126, 128):
        assert f"bunkerQuest.SetStage({stage})" in keypad
    assert "GiveAliasItem(Alias_BlackwellID)" in fragment
    assert "GiveAliasItem(Alias_WhitespringHolotape)" in fragment


def test_en01_access_pad_has_self_plugin_door_fallback():
    patch = _script_patch_source("EN01_BunkerAccessPadAliasScript")
    assert patch is not None
    assert 'Game.GetFormFromFile(0x001AE92D, "SeventySix.esm")' in patch


@pytest.mark.parametrize("script_name", ALL_PATCH_CASES)
def test_en01_merged_patch_native_compiles_for_fo4(
    script_name: str, merged_import_root: Path
):
    base_source = _fo4_base_source()
    if base_source is None:
        pytest.skip("FO4 base Papyrus sources unavailable")

    result = compile_psc(
        _merged_source(script_name),
        imports=[
            str(merged_import_root),
            str(SOURCE_ROOT),
            str(base_source),
        ],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=str(_script_relative_path(script_name, ".psc")),
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None
