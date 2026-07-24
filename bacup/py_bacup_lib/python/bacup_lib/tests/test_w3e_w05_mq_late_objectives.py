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
GENERATED_SOURCE_ROOT = REPO_ROOT / "mods" / "SeventySix" / "Scripts" / "Source" / "User"

QF_101P_B = "Fragments:Quests:QF_W05_MQ_101P_B_003FBC10"
QF_102P = "Fragments:Quests:QF_W05_MQ_102P_003FFACF"

POSITIVE_STAGES = {
    QF_101P_B: (10,),
    QF_102P: (20, 200, 300, 400, 530, 540, 550),
}
REPAIR_STAGES = {
    QF_101P_B: (230, 231, 400, 450, 500, 600, 9000),
    QF_102P: (
        10, 15, 30, 580, 584, 585, 586, 590, 680, 684, 685, 686, 690, 730,
        1300, 1400, 1500, 1600, 1700,
    ),
}
NEGATIVE_STAGES = {
    QF_101P_B: (
        100,
        200,
        232,
        240,
        300,
        350,
        590,
        700,
    ),
    QF_102P: (
        450,
        560,
        565,
        582,
        595,
        610,
        615,
        630,
        640,
        665,
        682,
        695,
        700,
        710,
        720,
        740,
        800,
        850,
        900,
        1000,
        1200,
        9000,
        10000,
    ),
}
LIVE_MEMBER_COUNTS = {QF_101P_B: 16, QF_102P: 49}

EXPECTED_STAGES = {
    QF_101P_B: (10, 230, 231, 400, 450, 500, 600, 9000),
    QF_102P: (
        10, 15, 20, 30, 200, 300, 400, 530, 540, 550, 580, 584, 585, 586,
        590, 680, 684, 685, 686, 690, 730, 1300, 1400, 1500, 1600, 1700,
    ),
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
        for kind, name, start, end in _iter_top_level_papyrus_members(
            source.splitlines()
        )
        if kind in {"function", "event"} and name == member_name.lower()
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


@pytest.mark.parametrize("script_name", POSITIVE_STAGES)
def test_patch_has_exact_positive_allowlist_and_complete_negative_absence(
    script_name: str,
):
    patch = _script_patch_source(script_name)
    assert patch is not None
    assert not any(
        line.strip().lower().startswith("scriptname ") for line in patch.splitlines()
    )
    assert _iter_papyrus_states(patch.splitlines()) == []

    positives = [_fragment_member(stage) for stage in POSITIVE_STAGES[script_name]]
    repairs = [_fragment_member(stage) for stage in REPAIR_STAGES[script_name]]
    negatives = {_fragment_member(stage) for stage in NEGATIVE_STAGES[script_name]}
    names = _member_names(patch)

    expected = [_fragment_member(stage) for stage in EXPECTED_STAGES[script_name]]
    assert set(expected) == set(positives + repairs)
    assert names == expected
    assert Counter(names) == Counter({name: 1 for name in expected})
    assert set(names).isdisjoint(negatives)
    assert len(expected) + len(negatives) == LIVE_MEMBER_COUNTS[script_name]


@pytest.mark.parametrize("script_name", POSITIVE_STAGES)
def test_patch_is_objective_display_only(script_name: str):
    patch = _script_patch_source(script_name)
    assert patch is not None

    objective_bodies = []
    for stage in POSITIVE_STAGES[script_name]:
        body = _member_body(patch, _fragment_member(stage))
        objective_bodies.append(body)
        assert body == (
            f"Function Fragment_Stage_{stage:04d}_Item_00()\n"
            f"    SetObjectiveDisplayed({stage})\n"
            "EndFunction"
        )

    objective_source = "\n".join(objective_bodies)
    assert objective_source.count("SetObjectiveDisplayed(") == len(
        POSITIVE_STAGES[script_name]
    )
    for forbidden in (
        "SetObjectiveCompleted(",
        "SetObjectiveFailed(",
        "CompleteQuest(",
        "SetStage(",
        "Stop(",
        ".Start(",
        ".Stop(",
        "AddItem(",
        "RemoveItem(",
        "SetValue(",
        "ModValue(",
        ".Play(",
        ".Enable(",
        ".Disable(",
        ".MoveTo(",
        ".ForceRefTo(",
    ):
        assert forbidden not in objective_source


@pytest.mark.parametrize("script_name", POSITIVE_STAGES)
def test_production_merge_is_exact_and_idempotent(script_name: str):
    skeleton = _production_skeleton(script_name)
    patch = _script_patch_source(script_name)
    assert patch is not None
    merged = _merge_script_method_patches(skeleton, patch)

    skeleton_header = next(
        line for line in skeleton.splitlines() if line.lower().startswith("scriptname ")
    )
    assert skeleton_header in merged
    for line in skeleton.splitlines():
        if " property " in f" {line.lower()} ":
            assert line in merged

    merged_names = _member_names(merged)
    for member_name in _member_names(patch):
        assert merged_names.count(member_name) == 1
        assert _member_body(merged, member_name) == _member_body(patch, member_name)
    assert _merge_script_method_patches(merged, patch) == merged


@pytest.mark.parametrize("script_name", POSITIVE_STAGES)
def test_full_production_merge_native_compiles_for_fo4(script_name: str):
    base_source = _fo4_base_source()
    assert base_source is not None, "FO4 base Papyrus sources unavailable"
    assert GENERATED_SOURCE_ROOT.is_dir(), "generated source root unavailable"

    result = compile_psc(
        _merged_production_source(script_name),
        imports=[str(base_source), str(GENERATED_SOURCE_ROOT)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=f"{script_name.rsplit(':', 1)[-1]}.psc",
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None
