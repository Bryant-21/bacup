from __future__ import annotations

from collections import Counter
from pathlib import Path

import pytest

from bacup_lib.tests.test_terminal_fragment_script_patches import _fo4_base_source
from bacup_lib.workflows.unified import (
    _iter_papyrus_states,
    _iter_top_level_papyrus_members,
    _merge_script_method_patches,
    _script_patch_source,
    _script_relative_path,
)
from creation_lib.pex import decompile_pex
from creation_lib.pex.native_runtime import compile_psc


REPO_ROOT = Path(__file__).resolve().parents[5]
DEPLOYED_SCRIPTS_ROOT = REPO_ROOT / "mods" / "SeventySix" / "data" / "Scripts"

SCRIPTS = (
    "Fragments:Quests:QF_W05_MQSettlers_201P_Indus_003F28C3",
    "Fragments:Quests:QF_W05_MQS_202P_Acrobat_003F28C7",
    "Fragments:Quests:QF_W05_MQS_203P_0040571C",
    "Fragments:Quests:QF_W05_MQS_Choice_00592500",
    "Fragments:Quests:QF_W05_MQS_204P_0040C458",
    "Fragments:Quests:QF_W05_MQS_205P_0041CB6D",
)

NEW_STAGES = {
    SCRIPTS[0]: (
        10,
        175,
        225,
        250,
        490,
        510,
        525,
        550,
        625,
        426,
        730,
        731,
        750,
        810,
        825,
        851,
        875,
        902,
        951,
        952,
        953,
        975,
        1010,
        1025,
        1225,
        9000,
    ),
    SCRIPTS[1]: (
        10,
        75,
        150,
        201,
        202,
        210,
        211,
        275,
        721,
        726,
        750,
        751,
        752,
        899,
        999,
        9000,
    ),
    SCRIPTS[2]: (
        300,
        400,
        500,
        700,
        710,
        720,
        730,
        900,
        950,
        1002,
        1003,
        1004,
        1200,
        1300,
        1400,
        1510,
        1520,
        1530,
        1800,
        9000,
    ),
    SCRIPTS[3]: (9000,),
    SCRIPTS[4]: (
        200,
        250,
        300,
        360,
        370,
        390,
        525,
        550,
        600,
        700,
        800,
        1000,
        9000,
    ),
    SCRIPTS[5]: (
        200,
        250,
        300,
        350,
        400,
        450,
        700,
        900,
        1000,
        1050,
        1100,
        1300,
        1400,
        1900,
        2000,
        2100,
        2200,
        2300,
        9000,
    ),
}

HANDOFFS = {
    SCRIPTS[0]: (9000, "W05_MQS_202P_QuestStartKeyword.SendStoryEvent"),
    SCRIPTS[1]: (9000, "W05_MQS_203P_QuestStartKeyword.SendStoryEvent"),
    SCRIPTS[2]: (9000, "W05_MQS_Choice_QuestStartKeyword.SendStoryEvent"),
    SCRIPTS[3]: (9000, "W05_MQS_204P_QuestStartKeyword.SendStoryEvent"),
    SCRIPTS[4]: (9000, "W05_MQS_205P_QuestStartKeyword.SendStoryEvent"),
    SCRIPTS[5]: (9000, "W05_MQA_206P_QuestStart_Keyword.SendStoryEvent"),
}


def _fragment_member(stage: int) -> str:
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
        for _kind, name, start, end in _iter_top_level_papyrus_members(
            source.splitlines()
        )
        if name == member_name.lower()
    )
    return "\n".join(source.splitlines()[start : end + 1])


def _production_skeleton(script_name: str) -> str:
    pex_path = DEPLOYED_SCRIPTS_ROOT / _script_relative_path(script_name, ".pex")
    assert pex_path.is_file(), f"deployed production PEX unavailable: {pex_path}"
    return decompile_pex(pex_path, fo4_api_compat=True)


def _merged_production_source(script_name: str) -> str:
    patch = _script_patch_source(script_name)
    assert patch is not None
    return _merge_script_method_patches(_production_skeleton(script_name), patch)


@pytest.mark.parametrize("script_name", SCRIPTS)
def test_playthrough_patch_contract_and_production_merge(script_name: str):
    patch = _script_patch_source(script_name)
    assert patch is not None
    assert patch.count("; TODO") == 1
    assert _iter_papyrus_states(patch.splitlines()) == []
    assert not any(
        line.strip().lower().startswith("scriptname ") for line in patch.splitlines()
    )

    members = _member_names(patch)
    for stage in NEW_STAGES[script_name]:
        assert members.count(_fragment_member(stage)) == 1

    skeleton = _production_skeleton(script_name)
    merged = _merge_script_method_patches(skeleton, patch)
    assert Counter(_member_names(merged)) == Counter(members)
    for member_name in members:
        assert _member_body(merged, member_name) == _member_body(patch, member_name)
    assert _merge_script_method_patches(merged, patch) == merged


@pytest.mark.parametrize("script_name", SCRIPTS)
def test_terminal_handoff_uses_surviving_story_manager_keyword(script_name: str):
    patch = _script_patch_source(script_name)
    assert patch is not None
    stage, snippet = HANDOFFS[script_name]
    body = _member_body(patch, _fragment_member(stage))
    assert snippet in body
    assert ".Start()" not in body


def test_local_single_player_substitutes_cover_removed_producers():
    trade_secrets = _script_patch_source(SCRIPTS[0])
    acrobat = _script_patch_source(SCRIPTS[1])
    robobrain = _script_patch_source(SCRIPTS[2])
    cave = _script_patch_source(SCRIPTS[4])
    vault = _script_patch_source(SCRIPTS[5])
    assert all((trade_secrets, acrobat, robobrain, cave, vault))

    assert "RemoveItem(W05_MQS_201P_MiscItem_PipboyKit" in trade_secrets
    assert "RemoveItem(W05_MQS_201P_MiscItem_EyebotPart" in trade_secrets
    assert "AddItem(W05_MQS_202_MiscItem_DeactivatedLiberator" in acrobat
    assert "SetValue(W05_MQS_202P_CanPlaceLiberator, 1.0)" in acrobat
    assert "SetValue(W05_MQS_203P_HasPreppedDiasBrain, 1.0)" in robobrain
    assert "SetValue(W05_MQS_203P_HasPreppedGregBrain, 1.0)" in robobrain
    assert "SetValue(W05_MQS_203P_HasPreppedGinaBrain, 1.0)" in robobrain

    cave_bypass = _member_body(cave, _fragment_member(600))
    assert cave_bypass.count(".Disable()") == 10
    assert "SetStage(700)" in cave_bypass
    assert "AddItem(W05_MQS_204P_IntelligenceModule" in cave
    assert "SetValue(W05_MQS_205P_LaserGridState, 1.0)" in vault
    assert "Alias_AtriumExit.GetReference()" in vault


def test_reviewed_stage_edges_are_guarded_and_ordered():
    trade_secrets = _script_patch_source(SCRIPTS[0])
    acrobat = _script_patch_source(SCRIPTS[1])
    robobrain = _script_patch_source(SCRIPTS[2])
    cave = _script_patch_source(SCRIPTS[4])
    vault = _script_patch_source(SCRIPTS[5])
    assert trade_secrets is not None
    assert acrobat is not None
    assert robobrain is not None
    assert cave is not None
    assert vault is not None

    assert _fragment_member(430) not in _member_names(trade_secrets)
    assert _fragment_member(1050) not in _member_names(trade_secrets)

    scene_four = _member_body(trade_secrets, _fragment_member(1025))
    assert scene_four.index("W05_MQS_201P_Scene4.Start()") < scene_four.index(
        "W05_MQS_201P_Scene4.IsPlaying()"
    )
    assert scene_four.index("IsStageDone(1050)") < scene_four.index("SetStage(1200)")

    plane_bypass = _member_body(trade_secrets, _fragment_member(902))
    assert plane_bypass.index("PlayerBypassVertibotPart") < plane_bypass.index(
        "SetStage(951)"
    )

    penny_handoff = _member_body(trade_secrets, _fragment_member(625))
    assert "!IsStageDone(700)" in penny_handoff
    assert "SetStage(700)" in penny_handoff
    assert "!IsStageDone(725)" in penny_handoff
    assert "SetStage(725)" in penny_handoff

    pipboy_photo = _member_body(trade_secrets, _fragment_member(731))
    assert pipboy_photo.index("PlayerFoundPipboyPhoto") < pipboy_photo.index(
        "SetStage(750)"
    )

    robot_parts = _member_body(trade_secrets, _fragment_member(875))
    for stage in (900, 925, 950):
        assert f"!IsStageDone({stage})" in robot_parts
        assert f"SetStage({stage})" in robot_parts

    liberator_conversion = _member_body(acrobat, _fragment_member(210))
    assert liberator_conversion.index(
        "AddItem(W05_MQS_202P_MiscItem_RecalibratedLiberator"
    ) < liberator_conversion.index("SetStage(211)")

    corpse_cleanup = _member_body(acrobat, _fragment_member(211))
    assert corpse_cleanup.index(
        "Alias_CollectedLiberator.GetReference()"
    ) < corpse_cleanup.index("corpseRef.Disable()")
    assert corpse_cleanup.index("corpseRef.Disable()") < corpse_cleanup.index(
        "SetStage(225)"
    )

    instance_edge = _member_body(acrobat, _fragment_member(700))
    assert "Alias_TL_Instance_Actor_Jen.GetReference()" in instance_edge
    assert "Alias_TL_Instance_Actor_Spy.GetReference()" in instance_edge
    assert instance_edge.index("jenRef != None && spyRef != None") < instance_edge.index(
        "SetStage(720)"
    )

    spy_died = _member_body(acrobat, _fragment_member(751))
    spy_lived = _member_body(acrobat, _fragment_member(752))
    assert "SetValue(W05_MQS_202P_SpyIsAlive, 0.0)" in spy_died
    assert "SetValue(W05_MQS_202P_SpyIsAlive, 1.0)" in spy_lived
    assert "SetStage(800)" not in spy_died
    assert "SetStage(800)" not in spy_lived

    robobrain_members = _member_names(robobrain)
    for unsupported_stage in (200, 600, 1600):
        assert _fragment_member(unsupported_stage) not in robobrain_members

    dome_pickup = _member_body(robobrain, _fragment_member(1200))
    assert "SetStage(1300)" not in dome_pickup

    brain_assembly = _member_body(robobrain, _fragment_member(1300))
    assert "Bool assembledChoice = False" in brain_assembly
    for choice in ("Dias", "Greg", "Gina"):
        assert f"GetValue(W05_MQS_203P_Chose{choice})" in brain_assembly
        assert f"SetValue(W05_MQS_203P_Chose{choice}" not in brain_assembly
    assert "If assembledChoice && GetStage() < 1400" in brain_assembly

    tool_choice = _member_body(robobrain, _fragment_member(1400))
    assert "ElseIf playerRef.GetValue(W05_MQS_203P_ChoseGina)" in tool_choice
    assert "\n        Else\n" not in tool_choice

    vault_entry = _member_body(cave, _fragment_member(15))
    assert "Alias_EnableMarkerVault79.GetReference()" in vault_entry
    assert "Alias_currentPlayer.GetReference()" in vault_entry
    assert vault_entry.index("W05_MQS_204P_ActorEnableMarker.Enable()") < vault_entry.index(
        "SetStage(90)"
    )
    assert _fragment_member(1100) not in _member_names(cave)

    assert _fragment_member(600) not in _member_names(vault)
    motherlode_scene = _member_body(vault, _fragment_member(200))
    assert "W05_MQS_205P_005_MotherlodeScene.Start()" in motherlode_scene
    assert "W05_MQS_205P_006_MotherlodeSpeaks.Start()" not in vault

    atrium_scene = _member_body(vault, _fragment_member(2100))
    assert "W05_MQS_205P_017_AtriumScene.Start()" in atrium_scene
    assert "SetStage(2200)" not in atrium_scene


def test_excluded_online_reward_reputation_and_raider_surfaces_are_not_recreated():
    patches = "\n".join(_script_patch_source(name) or "" for name in SCRIPTS)
    for forbidden in (
        "EWS",
        "GoldBullion",
        "Rep_Mod_",
        "Reputation_AV_",
        "W05_MQR_",
        "W05_MQS_204P_WarningMSG.Show",
    ):
        assert forbidden not in patches


@pytest.mark.parametrize("script_name", SCRIPTS)
def test_full_production_merge_native_compiles_for_fo4(
    script_name: str, tmp_path: Path
):
    base_source = _fo4_base_source()
    assert base_source is not None, "FO4 base Papyrus sources unavailable"

    imports = [str(base_source)]
    if script_name == SCRIPTS[5]:
        dependency = tmp_path / "W05_Jen205_Script.psc"
        dependency.write_text(
            _merged_production_source("W05_Jen205_Script"), encoding="utf-8"
        )
        imports.insert(0, str(tmp_path))

    result = compile_psc(
        _merged_production_source(script_name),
        imports=imports,
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=f"{script_name.replace(':', '/')}.psc",
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None
