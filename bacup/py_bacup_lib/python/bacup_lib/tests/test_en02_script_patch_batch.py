from __future__ import annotations

import os
from pathlib import Path

import pytest

from bacup_lib.workflows.unified import (
    _fo76_to_fo4_script_type,
    _iter_top_level_papyrus_members,
    _merge_script_method_patches,
    _script_patch_source,
    _script_relative_path,
)
from creation_lib.pex import decompile_pex
from creation_lib.pex.native_runtime import compile_psc


REPO_ROOT = Path(__file__).resolve().parents[5]
OLD_PEX_ROOT = REPO_ROOT / "mods" / "SeventySixOld" / "data" / "Scripts"
OLD_SOURCE_ROOT = (
    REPO_ROOT / "mods" / "SeventySixOld" / "Scripts" / "Source" / "User"
)

PATCH_CASES = {
    "EN02_DeleteCollOnUnload": {"onunload"},
    "EN02_DisableAfterWait": {"onactivate", "ontimer", "finishdisable"},
    "EN02_ExamHandlerScript": {"onactivate", "updateexamscore", "ontimer"},
    "EN02_ExamPlayerScript": {
        "recordanswer",
        "resetanswers",
        "recountcorrectanswers",
    },
    "EN02_ExamQuestionScript": {"onmenuitemrun", "resolveanswervalue"},
    "EN02_ExamRoomAVTriggerScript": {"ontriggerenter", "ontriggerleave"},
    "EN02_FillAliasOnTriggerEnter": {"ontriggerenter"},
    "EN02_FoodDispenserScript": {"onactivate"},
    "EN02_ImmunoboosterEffectScript": {"oneffectstart", "oneffectfinish"},
    "EN02_IntroTerminalStateScript": {"onactivate"},
    "EN02_DecalParentScript": {"onunload"},
    "EN02_ExamWrapupScript": {"onmenuitemrun", "finishexam"},
    "EN02_Misc_QuestScript": {"onquestinit", "onstageset"},
    "EN02_ModuleHandlerScript": {
        "onquestinit",
        "ontimer",
        "beginmodulesequence",
        "triggermoduleblast",
        "finishmodulesequence",
    },
    "EN02_MQ_QuestScript": {
        "onquestinit",
        "onstageset",
        "ontimer",
        "finishdecontamination",
        "completeexam",
        "beginmodulesequence",
        "spawnorbitaldrop",
        "checkorbitalrewards",
        "updatecheckpoint",
    },
    "EN02_OrbitalDropCrateScript": {
        "onload",
        "onitemremoved",
        "getmainquest",
    },
    "EN02_OrbitalStrikeMarkerScript": {
        "onload",
        "ontimer",
        "playwarningbeep",
        "firestrike",
    },
    "EN02_OrbitalStrikeShooterScript": {"onload"},
    "EN02_RefCollRemoveOnActivate": {"onactivate"},
    "EN02_RefreshLaserGridCollOnStageSet": {
        "onaliasinit",
        "quest.onstageset",
        "refreshlasergrids",
    },
    "EN02_RefreshLaserGridOnStageSet": {"onaliasinit", "quest.onstageset"},
    "EN02_QuestScript": {
        "onquestinit",
        "onstageset",
        "registerplayer",
        "completeexam",
        "markacquiredfev",
        "markwestteklogread",
        "markorbitalplatformstarted",
    },
    "Fragments:Quests:QF_EN02_Misc_000293A4": {
        "fragment_stage_0010_item_00",
        "fragment_stage_0100_item_00",
    },
    "Fragments:TopicInfos:TIF_EN02_MQ_Us_0027DB1A": {
        "fragment_begin",
        "fragment_end",
    },
    "Fragments:TopicInfos:TIF_EN02_MQ_Us_004DF64B_1": {"fragment_end"},
    "Fragments:TopicInfos:TIF_EN02_MQ_Us_0052F516": {"fragment_begin"},
}

MAIN_QUEST_SCRIPT = "Fragments:Quests:QF_EN02_MQ_Us_000293A3"
MAIN_QUEST_SCENE_STAGES = {
    1,
    7,
    35,
    40,
    45,
    50,
    70,
    110,
    120,
    140,
    160,
    200,
    260,
    270,
    280,
    300,
    310,
    330,
    400,
}
MAIN_QUEST_MVP_STAGES = {
    5,
    15,
    30,
    47,
    90,
    170,
    190,
    240,
    290,
    315,
    320,
    340,
    350,
    360,
    405,
    407,
}
EN01_START_SCRIPT = "Fragments:Quests:QF_EN01_Sam_000714FE"

ALL_PATCH_CASES = (*PATCH_CASES, MAIN_QUEST_SCRIPT, EN01_START_SCRIPT)


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
    pex_path = OLD_PEX_ROOT / _script_relative_path(script_name, ".pex")
    assert pex_path.is_file(), pex_path
    skeleton = decompile_pex(
        pex_path,
        type_adapter=_fo76_to_fo4_script_type,
        drop_script_const=True,
        skip_internal_functions=True,
        fo4_api_compat=True,
    )
    patch = _script_patch_source(script_name)
    assert patch is not None
    return _merge_script_method_patches(skeleton, patch)


@pytest.fixture(scope="module")
def merged_import_root(tmp_path_factory: pytest.TempPathFactory) -> Path:
    root = tmp_path_factory.mktemp("en02_merged_sources")
    for script_name in ALL_PATCH_CASES:
        path = root / _script_relative_path(script_name, ".psc")
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(_merged_source(script_name), encoding="utf-8")
    return root


@pytest.mark.parametrize(("script_name", "expected_members"), PATCH_CASES.items())
def test_en02_patch_supplies_confirmed_members(
    script_name: str, expected_members: set[str]
):
    patch = _script_patch_source(script_name)
    assert patch is not None
    members = {
        name
        for _kind, name, _start, _end in _iter_top_level_papyrus_members(
            patch.splitlines()
        )
    }
    assert expected_members <= members


def test_en02_main_quest_patch_restores_scene_and_membership_stages():
    patch = _script_patch_source(MAIN_QUEST_SCRIPT)
    assert patch is not None
    members = {
        name
        for kind, name, _start, _end in _iter_top_level_papyrus_members(
            patch.splitlines()
        )
        if kind == "function"
    }
    assert {
        f"fragment_stage_{stage:04d}_item_00"
        for stage in MAIN_QUEST_SCENE_STAGES | MAIN_QUEST_MVP_STAGES
    } <= members
    assert "playerRef.SetValue(EN02_JoinedEnclaveValue, 1.0)" in patch
    assert "playerRef.AddToFaction(EnclaveFaction)" in patch


def test_en01_completion_directly_starts_en02_when_story_nodes_are_absent():
    patch = _script_patch_source(EN01_START_SCRIPT)
    assert patch is not None
    assert "EN02_QuestStartKeyword.SendStoryEvent" in patch
    assert 'Game.GetFormFromFile(0x000293A3, "SeventySix.esm")' in patch
    assert "en02MainQuest.Start()" in patch
    assert "en02MainQuest.SetStage(5)" in patch


@pytest.mark.parametrize("script_name", ALL_PATCH_CASES)
def test_en02_merged_patch_native_compiles_for_fo4(
    script_name: str, merged_import_root: Path
):
    base_source = _fo4_base_source()
    if base_source is None:
        pytest.skip("FO4 base Papyrus sources unavailable")

    result = compile_psc(
        _merged_source(script_name),
        imports=[
            str(merged_import_root),
            str(OLD_SOURCE_ROOT),
            str(base_source),
        ],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=str(_script_relative_path(script_name, ".psc")),
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None
