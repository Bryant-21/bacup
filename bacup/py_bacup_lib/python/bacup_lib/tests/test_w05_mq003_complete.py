from __future__ import annotations

from collections import Counter
from pathlib import Path

from bacup_lib.tests.test_terminal_fragment_script_patches import _fo4_base_source
from bacup_lib.workflows.unified import (
    _iter_top_level_papyrus_members,
    _merge_script_method_patches,
    _script_patch_source,
    _script_relative_path,
)
from creation_lib.pex import decompile_pex
from creation_lib.pex.native_runtime import compile_psc


REPO_ROOT = Path(__file__).resolve().parents[5]
DEPLOYED_SCRIPTS_ROOT = REPO_ROOT / "mods" / "SeventySix" / "data" / "Scripts"
QF_MQ003 = "Fragments:Quests:QF_W05_MQ_003P_Muscle_0041A39D"

LIVE_FRAGMENT_STAGES = (
    1,
    2,
    3,
    4,
    5,
    6,
    7,
    8,
    10,
    100,
    103,
    150,
    200,
    300,
    400,
    410,
    415,
    450,
    476,
    499,
    500,
    550,
    600,
    700,
    710,
    715,
    725,
    800,
    900,
    999,
    1000,
    1005,
    1015,
    1020,
    1025,
    1050,
    1100,
    1150,
    1200,
    1205,
    1224,
    1225,
    1226,
    1229,
    1230,
    1232,
    1240,
    1251,
    1270,
    1275,
    1280,
    1300,
    1310,
    1311,
    1312,
    1320,
    1321,
    1325,
    1390,
    1500,
    9000,
    10000,
)


def _member_name(stage: int) -> str:
    return f"fragment_stage_{stage:04d}_item_00"


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
        for kind, name, start, end in _iter_top_level_papyrus_members(
            source.splitlines()
        )
        if kind in {"function", "event"} and name == member_name
    )
    return "\n".join(source.splitlines()[start : end + 1])


def _qf_patch() -> str:
    patch = _script_patch_source(QF_MQ003)
    assert patch is not None
    return patch


def _stage_body(stage: int) -> str:
    return _member_body(_qf_patch(), _member_name(stage))


def _production_skeleton() -> str:
    pex_path = DEPLOYED_SCRIPTS_ROOT / _script_relative_path(QF_MQ003, ".pex")
    assert pex_path.is_file(), f"deployed production PEX unavailable: {pex_path}"
    return decompile_pex(pex_path, fo4_api_compat=True)


def test_mq003_authors_every_live_vmad_fragment_once():
    patch = _qf_patch()
    expected = [_member_name(stage) for stage in LIVE_FRAGMENT_STAGES]
    names = _member_names(patch)

    assert len(expected) == 62
    assert Counter(names) == Counter(expected)
    assert len(names) == len(set(names)) == 62
    assert "; TODO" not in patch

    for name in expected:
        body = _member_body(patch, name)
        executable = [
            line.strip()
            for line in body.splitlines()[1:-1]
            if line.strip() and not line.lstrip().startswith(";")
        ]
        assert executable, f"behaviorless live fragment: {name}"


def test_gauley_rescue_and_local_encounter_route_cannot_deadlock():
    assert "solRef.RemoveFromFaction(CaptiveFaction)" in _stage_body(410)
    assert "solRef.AddToFaction(W05_CrimeTheWayward)" in _stage_body(410)
    assert "solRef.ResetHealthAndLimbs()" in _stage_body(415)
    assert "solRef.EvaluatePackage()" in _stage_body(450)

    assert "SetStage(710)" in _stage_body(700)
    assert "StartLocalEncounterWave(0)" in _stage_body(710)
    assert "GetAlias(17) as RefCollectionAlias" in _stage_body(715)
    assert "SetStage(725)" in _stage_body(715)
    assert "StartLocalEncounterWave(1)" in _stage_body(725)
    assert "SetStage(900)" in _stage_body(800)
    assert "SetStage(900)" in _stage_body(500)
    assert "SetStage(1000)" in _stage_body(900)

    shared = _script_patch_source("defaultquestencounterwavescript")
    assert shared is not None
    assert "Function StartLocalEncounterWave(Int aiWaveIndex)" in shared
    assert "SetLocalEncounterStageIfPending(waveData.StageToSetAtEnd)" in shared


def test_polly_body_skinner_reward_and_crane_handoff_are_complete():
    assert "playerRef.EquipItem(W05_MQ_003P_Muscle_PollyAssaultronHead" in _stage_body(1015)
    assert "playerRef.DrawWeapon()" in _stage_body(1015)
    assert "SetStage(1050)" in _stage_body(1025)
    assert "playerRef.RemoveItem(W05_MQ_003P_Muscle_PollyAssaultronHead" in _stage_body(1150)
    assert "W05_MQ_003P_Muscle_1150_PollyScene.Start()" in _stage_body(1150)
    assert "playerRef.AddItem(W05_MQ_003P_Muscle_DnDAccessCard" in _stage_body(1200)
    assert "W05_MQ_003P_Muscle_DuncanNDuncanQuest_Interior.Start()" in _stage_body(1200)

    for stage, choice in ((1270, "1.0"), (1275, "2.0"), (1280, "3.0")):
        body = _stage_body(stage)
        assert f"W05_MQ_003P_Muscle_PollyBodyChoiceIndex, {choice}" in body
        assert f"W05_MQ_003P_Muscle_DnDBodyChoiceIndex, {choice}" in body
        assert "SetStage(1300)" in body

    assert "SetObjectiveDisplayed(1315)" in _stage_body(1325)
    assert "SetStage(9000)" in _stage_body(1500)
    assert "W05_MQ_004P_Crane_QuestStartKeyword.SendStoryEvent()" in _stage_body(1500)
    assert "Game.GetFormFromFile(0x00572D17, \"SeventySix.esm\")" in _stage_body(9000)
    assert "playerRef.AddItem(duchessDram, 2, False)" in _stage_body(9000)
    assert "CompleteAllObjectives()" in _stage_body(9000)


def test_quest_specific_producers_are_no_longer_hollow():
    expected_members = {
        "W05_003P_Muscle_QuestScript": {
            "onquestinit",
            "onquestshutdown",
            "ontimer",
            "startlocalcombatmusic",
            "stoplocalcombatmusic",
        },
        "W05_003P_MusicOverrideTriggerScript": {
            "ontriggerenter",
            "ontriggerleave",
        },
        "W05_MQ_003P_SolAliasScript": {"oncripple"},
        "W05_MQ_003P_PollyHeadEffectScript": {
            "oneffectstart",
            "oneffectfinish",
        },
    }
    for script_name, members in expected_members.items():
        patch = _script_patch_source(script_name)
        assert patch is not None
        assert set(_member_names(patch)) == members

    sol = _script_patch_source("W05_MQ_003P_SolAliasScript")
    polly = _script_patch_source("W05_MQ_003P_PollyHeadEffectScript")
    assert sol is not None
    assert polly is not None
    assert "GetOwningQuest().SetStage(TriggerStage)" not in sol
    assert "owningQuest.SetStage(TriggerStage)" in sol
    assert "OwningInstance.SetStage(1050)" in polly


def test_mq003_production_merge_is_exact_idempotent_and_native_compiles(tmp_path: Path):
    patch = _qf_patch()
    skeleton = _production_skeleton()
    merged = _merge_script_method_patches(skeleton, patch)

    expected = Counter(_member_name(stage) for stage in LIVE_FRAGMENT_STAGES)
    assert Counter(_member_names(merged)) == expected
    assert _merge_script_method_patches(merged, patch) == merged

    (tmp_path / "DefaultQuestEncounterWaveScript.psc").write_text(
        """Scriptname DefaultQuestEncounterWaveScript Extends Quest

Function StartLocalEncounterWave(Int aiWaveIndex)
EndFunction
""",
        encoding="utf-8",
    )
    base_source = _fo4_base_source()
    assert base_source is not None, "FO4 base Papyrus sources unavailable"
    result = compile_psc(
        merged,
        imports=[str(base_source), str(tmp_path)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=f"{QF_MQ003.replace(':', '/')}.psc",
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None
