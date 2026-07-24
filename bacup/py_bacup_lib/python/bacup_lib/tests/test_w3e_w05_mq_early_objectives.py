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


def _fragment_member(stage: int) -> str:
    return f"fragment_stage_{stage:04d}_item_00"


OBJECTIVE_CASES: dict[str, tuple[int, ...]] = {
    "Fragments:Quests:QF_W05_MQ_001P_Wayward_00405E14": (
        200,
        300,
        400,
        600,
    ),
    "Fragments:Quests:QF_W05_MQ_002P_Radical_0040F5BE": (
        100,
        110,
        125,
        130,
        200,
        400,
        475,
        700,
        998,
        1000,
        1020,
        1030,
        1210,
        1310,
        1320,
        1600,
        2000,
    ),
    "Fragments:Quests:QF_W05_MQ_003P_Muscle_0041A39D": (
        100,
        150,
        200,
        300,
        400,
        500,
        600,
        700,
        1000,
        1020,
        1100,
        1150,
        1200,
        1205,
        1300,
        1390,
    ),
}

REPAIR_CASES: dict[str, tuple[int, ...]] = {
    "Fragments:Quests:QF_W05_MQ_001P_Wayward_00405E14": (
        550,
        598,
        610,
        620,
        660,
        680,
        710,
        809,
        820,
        500,
        599,
        805,
        807,
        900,
        905,
        1000,
    ),
    "Fragments:Quests:QF_W05_MQ_002P_Radical_0040F5BE": (
        140,
        505,
        510,
        610,
        709,
        720,
        736,
        1220,
        1240,
        1315,
        1500,
        1700,
        2115,
        450,
        1550,
        8950,
    ),
    "Fragments:Quests:QF_W05_MQ_003P_Muscle_0041A39D": (
        499,
        550,
        999,
        1005,
        1224,
        1225,
        1230,
        1310,
        1311,
        1312,
        1320,
        1500,
        710,
        900,
    ),
}

NEGATIVE_CASES: dict[str, tuple[int, ...]] = {
    "Fragments:Quests:QF_W05_MQ_001P_Wayward_00405E14": (
        10,
        103,
        105,
        301,
        302,
        310,
        445,
        450,
        455,
        460,
        470,
        491,
        510,
        515,
        516,
        520,
        522,
        530,
        705,
        9000,
    ),
    "Fragments:Quests:QF_W05_MQ_002P_Radical_0040F5BE": (
        1,
        2,
        5,
        6,
        10,
        150,
        160,
        270,
        460,
        498,
        501,
        502,
        504,
        511,
        515,
        525,
        707,
        710,
        725,
        735,
        745,
        746,
        760,
        764,
        765,
        766,
        799,
        1100,
        1101,
        1140,
        1290,
        1350,
        1575,
        9000,
    ),
    "Fragments:Quests:QF_W05_MQ_003P_Muscle_0041A39D": (
        1,
        2,
        3,
        4,
        5,
        6,
        7,
        8,
        10,
        103,
        410,
        415,
        450,
        476,
        715,
        725,
        800,
        1015,
        1025,
        1050,
        1226,
        1229,
        1232,
        1240,
        1251,
        1270,
        1275,
        1280,
        1321,
        1325,
        9000,
        10000,
    ),
}

EXPECTED_TOTALS = {
    "Fragments:Quests:QF_W05_MQ_001P_Wayward_00405E14": 40,
    "Fragments:Quests:QF_W05_MQ_002P_Radical_0040F5BE": 67,
    "Fragments:Quests:QF_W05_MQ_003P_Muscle_0041A39D": 62,
}

EXPANDED_OBJECTIVE_STAGES = {
    "Fragments:Quests:QF_W05_MQ_001P_Wayward_00405E14": {400},
    "Fragments:Quests:QF_W05_MQ_003P_Muscle_0041A39D": {100, 400},
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
        for _kind, name, start, end in _iter_top_level_papyrus_members(
            source.splitlines()
        )
        if name == member_name.lower()
    )
    return "\n".join(source.splitlines()[start : end + 1])


def _expected_objective_body(stage: int) -> str:
    return (
        f"Function Fragment_Stage_{stage:04d}_Item_00()\n"
        f"    SetObjectiveDisplayed({stage})\n"
        "EndFunction"
    )


def _production_skeleton(script_name: str) -> str:
    pex_path = DEPLOYED_SCRIPTS_ROOT / _script_relative_path(script_name, ".pex")
    assert pex_path.is_file(), f"deployed production PEX unavailable: {pex_path}"
    return decompile_pex(pex_path, fo4_api_compat=True)


def _merged_production_source(script_name: str) -> str:
    patch = _script_patch_source(script_name)
    assert patch is not None
    return _merge_script_method_patches(_production_skeleton(script_name), patch)


@pytest.mark.parametrize(("script_name", "positive_stages"), OBJECTIVE_CASES.items())
def test_patch_preserves_the_exact_objective_bodies_and_reviewed_order(
    script_name: str, positive_stages: tuple[int, ...]
):
    patch = _script_patch_source(script_name)
    assert patch is not None
    assert not any(
        line.strip().lower().startswith("scriptname ") for line in patch.splitlines()
    )
    assert _iter_papyrus_states(patch.splitlines()) == []
    assert _member_names(patch) == [
        _fragment_member(stage) for stage in positive_stages
    ] + [
        _fragment_member(stage) for stage in REPAIR_CASES[script_name]
    ]

    expanded = EXPANDED_OBJECTIVE_STAGES.get(script_name, set())
    objective_source = "\n".join(
        _member_body(patch, _fragment_member(stage))
        for stage in positive_stages
        if stage not in expanded
    )
    for stage in positive_stages:
        body = _member_body(patch, _fragment_member(stage))
        if stage in expanded:
            assert body.startswith(f"Function Fragment_Stage_{stage:04d}_Item_00()\n")
            assert body.count(f"SetObjectiveDisplayed({stage})") == 1
        else:
            assert body == _expected_objective_body(stage)

    for forbidden in (
        "Fragment_Stage_9000",
        "Fragment_Stage_10000",
        "SetObjectiveCompleted(",
        "CompleteQuest(",
        "SetStage(",
        "Stop()",
        ".Start()",
        "AddItem(",
        "RemoveItem(",
        "StartTimer(",
        "CancelTimer(",
    ):
        assert forbidden not in objective_source


@pytest.mark.parametrize("script_name", OBJECTIVE_CASES)
def test_positive_and_full_negative_lists_cover_the_production_members(
    script_name: str,
):
    patch = _script_patch_source(script_name)
    assert patch is not None
    positive_members = [
        _fragment_member(stage) for stage in OBJECTIVE_CASES[script_name]
    ] + [
        _fragment_member(stage) for stage in REPAIR_CASES[script_name]
    ]
    negative_members = [
        _fragment_member(stage) for stage in NEGATIVE_CASES[script_name]
    ]

    assert set(positive_members).isdisjoint(negative_members)
    assert len(positive_members) + len(negative_members) == EXPECTED_TOTALS[script_name]
    assert len(set(positive_members + negative_members)) == EXPECTED_TOTALS[script_name]
    assert _member_names(patch) == positive_members
    assert set(positive_members).isdisjoint(negative_members)


@pytest.mark.parametrize("script_name", OBJECTIVE_CASES)
def test_production_merge_is_exact_and_idempotent(script_name: str):
    patch = _script_patch_source(script_name)
    assert patch is not None
    skeleton = _production_skeleton(script_name)
    merged = _merge_script_method_patches(skeleton, patch)

    expected_members = [
        _fragment_member(stage) for stage in OBJECTIVE_CASES[script_name]
    ] + [
        _fragment_member(stage) for stage in REPAIR_CASES[script_name]
    ]
    assert Counter(_member_names(merged)) == Counter(expected_members)
    for stage in OBJECTIVE_CASES[script_name] + REPAIR_CASES[script_name]:
        member_name = _fragment_member(stage)
        assert _member_body(merged, member_name) == _member_body(patch, member_name)
    for stage in NEGATIVE_CASES[script_name]:
        assert _fragment_member(stage) not in _member_names(merged)
    assert _merge_script_method_patches(merged, patch) == merged


@pytest.mark.parametrize("script_name", OBJECTIVE_CASES)
def test_full_production_merge_native_compiles_for_fo4(script_name: str):
    base_source = _fo4_base_source()
    assert base_source is not None, "FO4 base Papyrus sources unavailable"

    result = compile_psc(
        _merged_production_source(script_name),
        imports=[str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=f"{script_name.replace(':', '/')}.psc",
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None
