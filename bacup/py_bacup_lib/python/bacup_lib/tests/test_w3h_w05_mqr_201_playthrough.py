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

QF_201 = "Fragments:Quests:QF_W05_MQR_201P_0040D28D"
TRACK_RADIO = "Fragments:Quests:QF_W05_MQR_201P_Track_RadioQ_0040D28C"
INTERCOM = "W05_MQR_201P_IntercomTriggerScript"
EXPLOSIVE_BREAKERS = "W05_MQR_201P_ExplosiveBreakerScript"

COMPILE_CASES = (QF_201, TRACK_RADIO, INTERCOM, EXPLOSIVE_BREAKERS)
LIVE_BOUND_QF_STAGES = {
    1,
    2,
    3,
    4,
    100,
    200,
    210,
    211,
    220,
    300,
    400,
    410,
    500,
    600,
    610,
    615,
    620,
    700,
    800,
    860,
    900,
    1000,
    1100,
    1200,
    1300,
    1400,
    1410,
    1420,
    1430,
    1440,
    1500,
    1510,
    1520,
    1530,
    1600,
    1620,
    1700,
    1705,
    1740,
    1745,
    1800,
    1801,
    1810,
    1900,
    1901,
    1910,
    9000,
    9999,
    10000,
}


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


def _stage_member(stage: int) -> str:
    return f"fragment_stage_{stage:04d}_item_00"


def _production_skeleton(script_name: str) -> str:
    pex_path = DEPLOYED_SCRIPTS_ROOT / _script_relative_path(script_name, ".pex")
    assert pex_path.is_file(), f"deployed production PEX unavailable: {pex_path}"
    return decompile_pex(pex_path, fo4_api_compat=True)


def _merged_production_source(script_name: str) -> str:
    patch = _script_patch_source(script_name)
    assert patch is not None
    return _merge_script_method_patches(_production_skeleton(script_name), patch)


def test_every_authored_qf_member_is_bound_by_the_live_vmad_census():
    patch = _script_patch_source(QF_201)
    assert patch is not None
    authored = set(_member_names(patch))
    bound = {_stage_member(stage) for stage in LIVE_BOUND_QF_STAGES}

    assert authored <= bound
    assert Counter(_member_names(patch)) == Counter({name: 1 for name in authored})
    for unbound_stage in (201, 801, 1001, 1210, 1515, 1610, 1710, 1721, 1750):
        assert _stage_member(unbound_stage) not in authored


CRITICAL_ROUTE: dict[int, tuple[str, ...]] = {
    100: ("SetStage(1)", "SetStage(200)"),
    210: ("SetStage(500)",),
    211: ("SetStage(300)", "SetStage(400)"),
    220: ("SetStage(500)",),
    410: ("SetStage(500)",),
    500: ("SetStage(2)", "SetStage(600)"),
    615: ("SetStage(700)",),
    620: ("SetStage(700)",),
    800: (
        "W05_MQR_201P_Track_RadioQuestStartKeyword.SendStoryEvent",
        "SetStage(860)",
    ),
    860: ("SetStage(900)",),
    900: ("SetStage(3)", "SetStage(1000)"),
    1000: ("W05_MQR_201P_Weasel_000_StandAndFacePlayer.Start()",),
    1300: ("W05_MQR_201P_Weasel_004_GoToWall01.Start()",),
    1410: ("W05_MQR_201P_Weasel_006_BlowUpWall01.Start()",),
    1420: ("Alias_WallActivator01.GetReference()", "Activate(weaselRef)", "SetStage(1600)"),
    1600: ("W05_MQR_201P_Weasel_007_BlowUpWall02.Start()",),
    1620: ("Alias_WallActivator02.GetReference()", "Activate(weaselRef)", "SetStage(1700)"),
    1705: ("SetStage(1740)",),
    1740: ("SetStage(1800)",),
    1810: ("SetStage(1900)",),
    1901: ("SetStage(1910)",),
    1910: ("SetStage(9000)",),
    9000: ("W05_MQR_202P_QuestStart_Keyword.SendStoryEvent",),
    10000: ("W05_MQR_201P_Track_RadioQuest.SetStage(1000)", "Stop()"),
}


@pytest.mark.parametrize(("stage", "snippets"), CRITICAL_ROUTE.items())
def test_rough_single_player_route_has_the_required_receiving_edges(
    stage: int, snippets: tuple[str, ...]
):
    patch = _script_patch_source(QF_201)
    assert patch is not None
    body = _member_body(patch, _stage_member(stage))
    for snippet in snippets:
        assert snippet in body


def test_instance_substitutes_resolve_only_proven_aliases_and_fail_closed():
    patch = _script_patch_source(QF_201)
    assert patch is not None

    for stage, alias_name in ((1, "Alias_LouNote"), (2, "Alias_Kogan"), (3, "Alias_Weasel"), (4, "Alias_Gail")):
        body = _member_body(patch, _stage_member(stage))
        assert alias_name in body
        assert "!= None" in body


def test_intercom_substitute_is_player_only_guarded_and_converges_at_1300():
    patch = _script_patch_source(INTERCOM)
    assert patch is not None
    body = _member_body(patch, "ontriggerenter")

    for snippet in (
        "Event OnTriggerEnter(ObjectReference akActionRef)",
        "akActionRef != playerRef",
        "intercomRef == None",
        "louRef == None",
        "W05_MQR_201P_LouSaysTopic_IntercomGreeting == None",
        "owningQuest.SetStage(1200)",
        "intercomRef.Say(W05_MQR_201P_LouSaysTopic_IntercomGreeting",
        "owningQuest.SetStage(1210)",
        "owningQuest.SetStage(1300)",
    ):
        assert snippet in body
    assert body.index("owningQuest.SetStage(1200)") < body.index("intercomRef.Say(")
    assert body.index("intercomRef.Say(") < body.index("owningQuest.SetStage(1210)")


def test_cut_lou_room_helper_remains_unpatched_and_excluded_systems_are_absent():
    assert _script_patch_source("W05_MQR_201P_LouRoomTriggerScript") is None

    combined = "\n".join(_script_patch_source(name) or "" for name in COMPILE_CASES)
    for excluded in (
        "Community",
        "EWS",
        "Bounty",
        "Reputation_AV_",
        "Rep_Mod_",
        "AddItem(",
        "RemoveItem(",
        "ModValue(",
    ):
        assert excluded not in combined


@pytest.mark.parametrize("script_name", COMPILE_CASES)
def test_owned_production_merge_is_idempotent_and_native_compiles(script_name: str):
    patch = _script_patch_source(script_name)
    assert patch is not None
    skeleton = _production_skeleton(script_name)
    merged = _merge_script_method_patches(skeleton, patch)

    assert _merge_script_method_patches(merged, patch) == merged
    for member_name in _member_names(patch):
        assert Counter(_member_names(merged))[member_name] == 1

    base_source = _fo4_base_source()
    assert base_source is not None, "FO4 base Papyrus sources unavailable"
    result = compile_psc(
        merged,
        imports=[str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=f"{script_name.replace(':', '/')}.psc",
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None


@pytest.mark.parametrize("script_name", (QF_201, INTERCOM))
def test_partial_patches_keep_one_plain_todo(script_name: str):
    patch = _script_patch_source(script_name)
    assert patch is not None
    assert [line for line in patch.splitlines() if line.strip() == "; TODO"] == [
        "; TODO"
    ]
    assert [line for line in patch.splitlines() if line.lstrip().startswith(";")] == [
        "; TODO"
    ]
