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

QF_205P = "Fragments:Quests:QF_W05_MQR_205P_00548B7A"
QF_205P_A = "Fragments:Quests:QF_W05_MQR_205P_A_005588EF"
RARA_COMBAT = "W05_MQR_205P_RaRaCombatScript"
RARA_COWER = "W05_MQR_205P_RaRaCowerTriggerScript"
SCANNER = "W05_MQR_205P_ScannerFurnitureScript"
SECURITY = "W05_MQR_205P_SecurityTriggerScript"
TURRETS = "W05_MQR_205P_TurretsOffScript"
VENT = "W05_MQR_205P_VentSequenceScript"

SCRIPTS = (
    QF_205P,
    QF_205P_A,
    RARA_COMBAT,
    RARA_COWER,
    SCANNER,
    SECURITY,
    TURRETS,
    VENT,
)

LIVE_205P_MEMBERS = {
    *(
        f"fragment_stage_{stage:04d}_item_00"
        for stage in (
            1,
            100,
            105,
            106,
            110,
            200,
            210,
            250,
            260,
            300,
            305,
            310,
            315,
            320,
            325,
            330,
            400,
            410,
            500,
            550,
            560,
            600,
            610,
            615,
            620,
            700,
            710,
            800,
            810,
            900,
            905,
            906,
            910,
            915,
            920,
            921,
            930,
            940,
            1100,
            1105,
            1106,
            1107,
            1110,
            1111,
            1120,
            1200,
            1210,
            1220,
            1230,
            1240,
            1250,
            9000,
        )
    ),
    "fragment_stage_0930_item_01",
    "fragment_stage_0940_item_01",
}

EXPECTED_MEMBERS = {
    QF_205P: LIVE_205P_MEMBERS,
    QF_205P_A: {
        f"fragment_stage_{stage:04d}_item_00"
        for stage in (100, 200, 300, 400, 500, 600, 9000)
    },
    RARA_COMBAT: {"oncombatstatechanged"},
    RARA_COWER: {"ontriggerenter"},
    SCANNER: {"onactivate"},
    SECURITY: {"ontriggerenter"},
    TURRETS: {"ontriggerenter"},
    VENT: {"onactivate"},
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


def _stage_body(source: str, stage: int, item: int = 0) -> str:
    return _member_body(source, f"fragment_stage_{stage:04d}_item_{item:02d}")


def _production_skeleton(script_name: str) -> str:
    pex_path = DEPLOYED_SCRIPTS_ROOT / _script_relative_path(script_name, ".pex")
    assert pex_path.is_file(), f"deployed production PEX unavailable: {pex_path}"
    return decompile_pex(pex_path, fo4_api_compat=True)


def _merged_production_source(script_name: str) -> str:
    return _merge_script_method_patches(_production_skeleton(script_name), _patch(script_name))


@pytest.mark.parametrize("script_name", SCRIPTS)
def test_patch_uses_only_the_exact_live_member_contract(script_name: str):
    patch = _patch(script_name)
    assert not any(
        line.strip().lower().startswith("scriptname ") for line in patch.splitlines()
    )
    assert Counter(_member_names(patch)) == Counter(EXPECTED_MEMBERS[script_name])


def test_preserved_scenes_keep_native_scqs_completion_ownership():
    patch = _patch(QF_205P)
    scene_receivers = {
        110: ("W05_MQR_205P_001_IntroScene.Start()", (200,)),
        210: ("W05_MQR_205P_002_Lou_Door02.Start()", (250,)),
        260: ("W05_MQR_205P_003_DoorBlownUp.Start()", (300,)),
        315: ("W05_MQR_205P_004A_JohnnyDoor.Start()", (320,)),
        400: ("W05_MQR_205P_005_SecurityRoom.Start()", (410, 500)),
        920: ("W05_MQR_205P_015_RaRa_OverseerRoom.Start()", (921,)),
        1210: (
            "W05_MQR_205P_017_RaRa_LastVent02.Start()",
            (1230, 1240, 9000),
        ),
    }
    for stage, (scene_start, native_stages) in scene_receivers.items():
        body = _stage_body(patch, stage)
        assert scene_start in body
        for native_stage in native_stages:
            assert f"SetStage({native_stage})" not in body


def test_removed_encounter_wave_has_a_narrow_single_player_receiver():
    patch = _patch(QF_205P)
    atrium_bypass = _stage_body(patch, 915)
    assert atrium_bypass.index("Alias_AtriumRobotsWave01.DisableAll()") < atrium_bypass.index(
        "SetStage(920)"
    )
    for stage in (930, 940):
        assert "SetObjectiveDisplayed(1000)" in _stage_body(patch, stage)
        assert "SetStage(" not in _stage_body(patch, stage, 1)


def test_terminal_handoff_uses_story_manager_without_reward_or_reputation_calls():
    patch = _patch(QF_205P)
    terminal = _stage_body(patch, 9000)
    assert "W05_MQA_206P_QuestStart_Keyword.SendStoryEvent" in terminal
    assert ".Start()" not in terminal

    combined = "\n".join(_patch(script_name) for script_name in SCRIPTS)
    for forbidden in (
        "Rep_Mod_",
        "Reputation_AV_",
        ".AddItem(",
        ".RemoveItem(",
        "CompleteQuest(",
        "GoldBullion",
        "EWS",
    ):
        assert forbidden not in combined


def test_optional_lou_epilogue_converges_without_reputation_fidelity():
    patch = _patch(QF_205P_A)
    for stage in (200, 300, 400, 600):
        assert "SetStage(9000)" in _stage_body(patch, stage)
    assert "SendStoryEvent" not in _stage_body(patch, 9000)


def test_helper_guards_match_live_alias_bindings():
    combat = _patch(RARA_COMBAT)
    cower = _patch(RARA_COWER)
    scanner = _patch(SCANNER)
    security = _patch(SECURITY)
    turrets = _patch(TURRETS)
    vent = _patch(VENT)

    assert combat.index("raRaRef == None") < combat.index("ChangeAnimArchetype")
    assert cower.index("akActionRef != Game.GetPlayer()") < cower.index(
        "RaRaCowerIdleMarker.GetReference()"
    )
    assert scanner.index("akActionRef != Gail.GetReference()") < scanner.index(
        "owningQuest.SetStage(610)"
    )
    assert security.index("akActionRef != Game.GetPlayer()") < security.index(
        "collisionRef.Disable()"
    )
    assert turrets.index("akActionRef != Game.GetPlayer()") < turrets.index(
        "SecurityRoomTurrets.DisableAll()"
    )
    assert vent.index("akActionRef != owningQuestScript.RaRa.GetReference()") < vent.index(
        "ventButtonRef.Activate(akActionRef)"
    )


def test_plain_todo_markers_only_remain_on_partially_bounded_patches():
    expected_markers = {
        QF_205P: 1,
        QF_205P_A: 0,
        RARA_COMBAT: 1,
        RARA_COWER: 1,
        SCANNER: 1,
        SECURITY: 0,
        TURRETS: 0,
        VENT: 1,
    }
    for script_name, count in expected_markers.items():
        assert _patch(script_name).splitlines().count("; TODO") == count


@pytest.mark.parametrize("script_name", SCRIPTS)
def test_production_merge_is_exact_unique_and_idempotent(script_name: str):
    patch = _patch(script_name)
    merged = _merged_production_source(script_name)
    assert Counter(_member_names(merged)) == Counter(EXPECTED_MEMBERS[script_name])
    for member_name in EXPECTED_MEMBERS[script_name]:
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
