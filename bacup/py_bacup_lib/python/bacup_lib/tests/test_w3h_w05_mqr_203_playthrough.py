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
from creation_lib.pex import decompile_pex
from creation_lib.pex.native_runtime import compile_psc


REPO_ROOT = Path(__file__).resolve().parents[5]
DEPLOYED_SCRIPTS_ROOT = REPO_ROOT / "mods" / "SeventySix" / "data" / "Scripts"
GENERATED_SOURCE_ROOT = (
    REPO_ROOT / "mods" / "SeventySix" / "Scripts" / "Source" / "User"
)

QF = "Fragments:Quests:QF_W05_MQR_203P_0042F31B"
BENCH = "W05_MQR_203P_BenchScript"
BLACKOUT = "W05_MQR_203P_WinnersCupBlackOutScript"
SCRIPTS = (QF, BENCH, BLACKOUT)

LIVE_BOUND_STAGES = {
    2,
    100,
    200,
    300,
    400,
    405,
    410,
    420,
    499,
    500,
    510,
    550,
    600,
    605,
    610,
    700,
    710,
    800,
    810,
    815,
    820,
    822,
    900,
    901,
    910,
    950,
    1000,
    1050,
    1100,
    1110,
    1200,
    1205,
    1210,
    1215,
    1220,
    1222,
    1300,
    1301,
    1310,
    1350,
    1400,
    1450,
    1500,
    1510,
    1600,
    1601,
    1602,
    1605,
    1610,
    1700,
    1703,
    1705,
    1706,
    1710,
    1715,
    1720,
    1750,
    1800,
    1900,
    2000,
    2010,
    2100,
    2110,
    2200,
    2210,
    2220,
    2221,
    2300,
    2400,
    5000,
    5100,
    5200,
    5300,
    6000,
    7000,
    7100,
    7110,
    7120,
    7200,
    7300,
    8000,
    8100,
    8110,
    8200,
    8210,
    8220,
    8230,
    8250,
    9000,
    9001,
    9998,
    9999,
    10000,
}


def _patch(script_name: str) -> str:
    patch = _script_patch_source(script_name)
    assert patch is not None
    return patch


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
        for _kind, name, start, end in _iter_top_level_papyrus_members(
            source.splitlines()
        )
        if name == member_name.lower()
    )
    return "\n".join(source.splitlines()[start : end + 1])


def _stage_body(stage: int, item: int = 0) -> str:
    return _member_body(_patch(QF), f"fragment_stage_{stage:04d}_item_{item:02d}")


def _production_skeleton(script_name: str) -> str:
    pex_path = DEPLOYED_SCRIPTS_ROOT / _script_relative_path(script_name, ".pex")
    assert pex_path.is_file(), f"deployed production PEX unavailable: {pex_path}"
    return decompile_pex(pex_path, fo4_api_compat=True)


def _merged_production_source(script_name: str) -> str:
    return _merge_script_method_patches(
        _production_skeleton(script_name), _patch(script_name)
    )


def test_qf_members_are_live_vmad_bound_and_todo_is_minimal():
    qf = _patch(QF)
    assert qf.splitlines().count("; TODO") == 1
    assert _patch(BENCH).splitlines().count("; TODO") == 0
    assert _patch(BLACKOUT).splitlines().count("; TODO") == 0

    for member_name in _member_names(qf):
        prefix = "fragment_stage_"
        assert member_name.startswith(prefix)
        assert int(member_name[len(prefix) : len(prefix) + 4]) in LIVE_BOUND_STAGES


def test_instance_registration_and_round_producers_are_connected():
    init = _stage_body(2)
    assert init.index("disabledMarker != None") < init.index("SetStage(300)")
    assert init.index("entranceDoor != None") < init.index("SetStage(300)")

    assert "SetStage(405)" in _stage_body(400)
    assert "W05_MQR_203P_Johnny_002B_RegistrationScene.Start()" in _stage_body(405)
    assert "W05_MQR_203P_Johnny_002C_RegistrationScene.Start()" in _stage_body(499)
    assert "W05_MQR_203P_SargentoPA_002_Round01GhouldenBoy.Start()" in _stage_body(550)
    assert "W05_MQR_203P_SargentoPA_003_Round01CallPlayer.Start()" in _stage_body(605)

    bench = _member_body(_patch(BENCH), "onactivate")
    for current_stage, next_stage in ((600, 605), (1050, 1100), (1450, 1500)):
        assert f"currentStage == {current_stage}" in bench
        assert f"owningQuest.SetStage({next_stage})" in bench

    assert "SetStage(810)" in _stage_body(800, 1)
    assert "W05_MQR_203P_SargentoPA_004B_Round01End.Start()" in _stage_body(810)
    assert "W05_MQR_203P_SargentoPA_005_Round02CallPlayer.Start()" in _stage_body(1100)
    assert "SetStage(1210)" in _stage_body(1200, 1)
    assert "W05_MQR_203P_SargentoPA_006B_Round02End.Start()" in _stage_body(1210)
    assert "W05_MQR_203P_SargentoPA_007_Round03CallPlayer.Start()" in _stage_body(1500)


def test_final_round_blackout_escape_and_choice_handoff_are_connected():
    final_round = _stage_body(1600)
    assert "W05_MQR_203P_SargentoPA_008_Round03PlayerArena.Start()" in final_round

    cage = _stage_body(1601)
    assert cage.index("cageActivator.Activate(playerRef)") < cage.index("SetStage(1602)")
    assert "SetStage(1700)" in _stage_body(1610)
    assert "W05_MQR_203P_SargentoPA_009A_Winner.Start()" in _stage_body(1700)
    assert "SetStage(1715)" in _stage_body(1705)
    assert "W05_MQR_203P_Johnny_006_EnterArena.Start()" in _stage_body(1715)

    key = _stage_body(1720)
    assert key.index("AddItem(W05_MQR_203P_HalRoomKey") < key.index("SetStage(1750)")
    assert "W05_MQR_203P_JohnnySargento_001_Winner.Start()" in _stage_body(1750)
    assert "W05_MQR_203P_WinnersCup_Blackout.Cast" in _stage_body(1800)

    blackout = _member_body(_patch(BLACKOUT), "oneffectfinish")
    assert blackout.index("SlaveQuartersMarker.GetReference()") < blackout.index(
        "akTarget.MoveTo(destinationRef)"
    )
    assert blackout.index("akTarget.MoveTo(destinationRef)") < blackout.index(
        "W05_MQR_203P.SetStage(1900)"
    )

    assert "W05_MQR_203P_HalJohnny_001_Shoot.Start()" in _stage_body(2100)
    assert "halRef.Kill(johnnyRef)" in _stage_body(2110)
    for stage in (2210, 2220, 2221):
        assert "SetStage(9000)" in _stage_body(stage)

    handoff = _stage_body(9000)
    assert "W05_MQR_Choice_QuestStartKeyword.SendStoryEvent" in handoff
    assert ".Start()" not in handoff


def test_optional_cheat_routes_rejoin_without_online_or_reward_fidelity():
    assert "SetStage(5100)" in _stage_body(5000)
    for stage in (5200, 5300, 6000):
        assert "SetStage(910)" in _stage_body(stage)
    assert "SetStage(7100)" in _stage_body(7000)
    for stage in (7200, 7300):
        assert "SetStage(1310)" in _stage_body(stage)

    combined = "\n".join(_patch(script_name) for script_name in SCRIPTS)
    for forbidden in (
        "defaultquestencounterwavescript",
        "EncounterWaves",
        "Reputation_AV_",
        "Rep_Mod_",
        "CompleteQuest(",
        "W05_MQR_204P_QuestStart_Keyword.SendStoryEvent",
    ):
        assert forbidden not in combined


@pytest.mark.parametrize("script_name", SCRIPTS)
def test_production_merge_is_exact_unique_and_idempotent(script_name: str):
    patch = _patch(script_name)
    merged = _merged_production_source(script_name)
    assert Counter(_member_names(merged)) == Counter(_member_names(patch))
    for member_name in _member_names(patch):
        assert _member_body(merged, member_name) == _member_body(patch, member_name)
    assert _merge_script_method_patches(merged, patch) == merged


@pytest.mark.parametrize("script_name", SCRIPTS)
def test_full_production_merge_native_compiles_for_fo4(script_name: str):
    base_source = _fo4_base_source()
    assert base_source is not None, "FO4 base Papyrus sources unavailable"
    assert GENERATED_SOURCE_ROOT.is_dir(), "generated source root unavailable"

    result = compile_psc(
        _merged_production_source(script_name),
        imports=[str(base_source), str(GENERATED_SOURCE_ROOT)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=f"{script_name.replace(':', '/')}.psc",
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None
