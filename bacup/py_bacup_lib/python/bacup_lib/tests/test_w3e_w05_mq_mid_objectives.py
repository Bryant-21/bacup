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

CRANE = "Fragments:Quests:QF_W05_MQ_004P_Crane_0041C976"
MAIN_101P = "Fragments:Quests:QF_W05_MQ_101P_003FBBB2"
RAIDER_101P = "Fragments:Quests:QF_W05_MQ_101P_A_003FBC0D"

OBJECTIVE_CASES: dict[str, tuple[tuple[int, int], ...]] = {
    CRANE: tuple(
        (stage, 0)
        for stage in (100, 110, 111, 300, 399, 500, 700, 750, 760, 800, 1000, 1100, 1200, 1230)
    ),
    MAIN_101P: tuple(
        (stage, 0)
        for stage in (10, 13, 15, 20, 30, 40, 50, 100, 110, 120, 150, 200)
    ),
    RAIDER_101P: (
        (50, 0),
        (100, 0),
        (100, 1),
        (200, 0),
        (300, 0),
        (350, 0),
        (400, 0),
        (500, 0),
        (600, 0),
        (650, 0),
        (700, 0),
        (800, 0),
        (900, 0),
        (950, 0),
        (970, 0),
        (1000, 0),
        (1100, 0),
        (1200, 0),
        (1300, 0),
        (1400, 0),
        (1500, 0),
    ),
}

REPAIR_CASES: dict[str, tuple[tuple[int, int], ...]] = {
    CRANE: tuple(
        (stage, 0)
        for stage in (
            103, 105, 310, 400, 600, 650, 1170, 1180, 1245, 1265, 1300,
            1150, 1235, 1240, 1250, 1260, 8999,
        )
    ),
    MAIN_101P: tuple(
        (stage, 0)
        for stage in (300, 1000, 1400, 1600, 1700, 1800, 1900, 9000, 5, 600, 1300, 1310)
    ),
    RAIDER_101P: tuple(
        (stage, 0)
        for stage in (910, 930, 1110, 1210, 1420, 1450, 1530, 8000, 9000, 10, 1050)
    ),
}

NEGATIVE_CASES: dict[str, tuple[tuple[int, int], ...]] = {
    CRANE: tuple(
        (stage, 0)
        for stage in (
            1, 2, 3, 4, 5, 6, 10, 50, 102, 108, 109, 112, 125, 200, 210,
            301, 401, 495, 701, 702, 703, 704, 710, 765, 775, 820, 830, 1105,
            1220, 1221, 1242, 1243, 1244, 1261, 9000,
        )
    ),
    MAIN_101P: tuple(
        (stage, 0)
        for stage in (
            51, 52, 400, 500, 550, 700, 800, 805, 810, 820, 830, 900,
            1100, 1200, 1290, 1450, 1500, 1510, 1610, 1810, 1820,
            1910, 1920, 2000,
        )
    ),
    RAIDER_101P: tuple(
        (stage, 0)
        for stage in (
            0, 1, 2, 3, 4, 5, 6, 310, 311, 320, 330, 331, 375, 680, 710,
            730, 810, 820, 830, 960, 1415, 1430, 1440,
        )
    ),
}


def _member_name(stage: int, item: int) -> str:
    return f"fragment_stage_{stage:04d}_item_{item:02d}"


PATCH_CASES = {
    base_name: {
        _member_name(stage, item)
        for stage, item in OBJECTIVE_CASES[base_name] + REPAIR_CASES[base_name]
    }
    for base_name in OBJECTIVE_CASES
}

EXPANDED_OBJECTIVE_STAGES = {
    CRANE: {1000, 1200},
    MAIN_101P: {10, 30, 100, 200},
    RAIDER_101P: {1000},
}


def _member_name_list(source: str) -> list[str]:
    return [
        name
        for kind, name, _start, _end in _iter_top_level_papyrus_members(source.splitlines())
        if kind in {"function", "event"}
    ]


def _member_names(source: str) -> set[str]:
    return set(_member_name_list(source))


def _member_body(source: str, member_name: str) -> str:
    start, end = next(
        (start, end)
        for kind, name, start, end in _iter_top_level_papyrus_members(source.splitlines())
        if kind in {"function", "event"} and name == member_name
    )
    return "\n".join(source.splitlines()[start : end + 1])


def _production_skeleton(base_name: str) -> str:
    pex_path = DEPLOYED_SCRIPTS_ROOT / _script_relative_path(base_name, ".pex")
    assert pex_path.is_file(), f"deployed production PEX unavailable: {pex_path}"
    return decompile_pex(pex_path, fo4_api_compat=True)


def _merged_production_source(base_name: str) -> str:
    patch = _script_patch_source(base_name)
    assert patch is not None
    return _merge_script_method_patches(_production_skeleton(base_name), patch)


@pytest.mark.parametrize(("base_name", "expected_members"), PATCH_CASES.items())
def test_patch_has_exact_objective_member_allowlist(
    base_name: str, expected_members: set[str]
):
    patch = _script_patch_source(base_name)
    assert patch is not None
    assert not any(
        line.strip().lower().startswith(("scriptname ", "property "))
        for line in patch.splitlines()
    )
    assert _iter_papyrus_states(patch.splitlines()) == []
    assert _member_names(patch) == expected_members
    assert Counter(_member_name_list(patch)) == Counter(
        {name: 1 for name in expected_members}
    )


@pytest.mark.parametrize(("base_name", "members"), OBJECTIVE_CASES.items())
def test_objective_members_have_exact_signatures_and_bodies(
    base_name: str, members: tuple[tuple[int, int], ...]
):
    patch = _script_patch_source(base_name)
    assert patch is not None
    for stage, item in members:
        member_name = _member_name(stage, item)
        expected_body = (
            f"Function Fragment_Stage_{stage:04d}_Item_{item:02d}()\n"
            f"    SetObjectiveDisplayed({stage})\n"
            "EndFunction"
        )
        body = _member_body(patch, member_name)
        if stage in EXPANDED_OBJECTIVE_STAGES[base_name]:
            assert body.startswith(f"Function Fragment_Stage_{stage:04d}_Item_{item:02d}()\n")
            assert body.count(f"SetObjectiveDisplayed({stage})") == 1
        else:
            assert body == expected_body


@pytest.mark.parametrize(("base_name", "members"), NEGATIVE_CASES.items())
def test_all_negative_ledger_members_remain_absent(
    base_name: str, members: tuple[tuple[int, int], ...]
):
    patch = _script_patch_source(base_name)
    assert patch is not None
    negative_names = {_member_name(stage, item) for stage, item in members}
    assert _member_names(patch).isdisjoint(negative_names)


def test_patch_counts_match_the_reviewed_ledger():
    assert {name: len(members) for name, members in OBJECTIVE_CASES.items()} == {
        CRANE: 14,
        MAIN_101P: 12,
        RAIDER_101P: 21,
    }
    assert {name: len(members) for name, members in REPAIR_CASES.items()} == {
        CRANE: 17,
        MAIN_101P: 12,
        RAIDER_101P: 11,
    }
    assert {name: len(members) for name, members in NEGATIVE_CASES.items()} == {
        CRANE: 35,
        MAIN_101P: 24,
        RAIDER_101P: 23,
    }


def test_objective_display_surface_remains_exact_after_behavior_repairs():
    combined = "\n".join(_script_patch_source(name) or "" for name in PATCH_CASES)
    assert combined.count("SetObjectiveDisplayed(") == 47
    assert "SetObjectiveCompleted(" not in combined
    assert "SetObjectiveFailed(" not in combined
    assert "CompleteQuest(" not in combined


@pytest.mark.parametrize("base_name", PATCH_CASES)
def test_production_merge_preserves_skeleton_and_is_idempotent(base_name: str):
    skeleton = _production_skeleton(base_name)
    patch = _script_patch_source(base_name)
    assert patch is not None
    merged = _merge_script_method_patches(skeleton, patch)

    skeleton_header = next(
        line for line in skeleton.splitlines() if line.lower().startswith("scriptname ")
    )
    assert skeleton_header in merged
    for line in skeleton.splitlines():
        if " property " in f" {line.lower()} ":
            assert line in merged

    merged_counts = Counter(_member_name_list(merged))
    for member in PATCH_CASES[base_name]:
        assert merged_counts[member] == 1
    assert _merge_script_method_patches(merged, patch) == merged


@pytest.mark.parametrize("base_name", PATCH_CASES)
def test_production_merge_native_compiles_for_fo4(base_name: str):
    base_source = _fo4_base_source()
    assert base_source is not None, "FO4 base Papyrus sources unavailable"
    assert GENERATED_SOURCE_ROOT.is_dir(), "generated source root unavailable"

    result = compile_psc(
        _merged_production_source(base_name),
        imports=[str(base_source), str(GENERATED_SOURCE_ROOT)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=f"{base_name.rsplit(':', 1)[-1]}.psc",
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None
