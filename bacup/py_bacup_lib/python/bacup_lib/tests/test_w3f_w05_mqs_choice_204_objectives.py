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
    "Fragments:Quests:QF_W05_MQS_Choice_00592500": (100, 9000),
    "Fragments:Quests:QF_W05_MQS_204P_0040C458": (
        10, 15, 90, 100, 200, 250, 300, 360, 370, 390, 525, 550, 600, 700,
        800, 1000, 9000,
    ),
}

NEGATIVE_CASES: dict[str, tuple[int, ...]] = {
    "Fragments:Quests:QF_W05_MQS_Choice_00592500": (9999,),
    "Fragments:Quests:QF_W05_MQS_204P_0040C458": (
        210,
        275,
        375,
        380,
        400,
        500,
        610,
        615,
        620,
        625,
        630,
        635,
        640,
        645,
        650,
        655,
        900,
        1100,
        10000,
    ),
}

EXPECTED_TOTALS = {
    "Fragments:Quests:QF_W05_MQS_Choice_00592500": 3,
    "Fragments:Quests:QF_W05_MQS_204P_0040C458": 36,
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


def _production_skeleton(script_name: str) -> str:
    pex_path = DEPLOYED_SCRIPTS_ROOT / _script_relative_path(script_name, ".pex")
    assert pex_path.is_file(), f"deployed production PEX unavailable: {pex_path}"
    return decompile_pex(pex_path, fo4_api_compat=True)


def _merged_production_source(script_name: str) -> str:
    patch = _script_patch_source(script_name)
    assert patch is not None
    return _merge_script_method_patches(_production_skeleton(script_name), patch)


@pytest.mark.parametrize(("script_name", "positive_stages"), OBJECTIVE_CASES.items())
def test_patch_has_the_exact_reconciled_allowlist(
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
    ]
    for forbidden in (
        "CompleteQuest(",
        "ModValue(",
        "Reputation_AV_",
        "Community",
        "Bounty",
        "defaultquestencounterwavescript",
    ):
        assert forbidden not in patch


@pytest.mark.parametrize("script_name", OBJECTIVE_CASES)
def test_positive_and_full_negative_lists_cover_the_production_members(
    script_name: str,
):
    patch = _script_patch_source(script_name)
    assert patch is not None
    positive_members = [
        _fragment_member(stage) for stage in OBJECTIVE_CASES[script_name]
    ]
    negative_members = [
        _fragment_member(stage) for stage in NEGATIVE_CASES[script_name]
    ]

    assert set(positive_members).isdisjoint(negative_members)
    assert len(positive_members) + len(negative_members) == EXPECTED_TOTALS[script_name]
    assert len(set(positive_members + negative_members)) == EXPECTED_TOTALS[script_name]
    assert set(_member_names(patch)).isdisjoint(negative_members)


@pytest.mark.parametrize("script_name", OBJECTIVE_CASES)
def test_production_merge_is_exact_and_idempotent(script_name: str):
    patch = _script_patch_source(script_name)
    assert patch is not None
    skeleton = _production_skeleton(script_name)
    merged = _merge_script_method_patches(skeleton, patch)

    expected_members = [
        _fragment_member(stage) for stage in OBJECTIVE_CASES[script_name]
    ]
    assert Counter(_member_names(merged)) == Counter(expected_members)
    for stage in OBJECTIVE_CASES[script_name]:
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
