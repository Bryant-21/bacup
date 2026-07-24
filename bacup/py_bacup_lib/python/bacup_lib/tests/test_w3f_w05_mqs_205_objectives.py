from __future__ import annotations

from collections import Counter
from pathlib import Path

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
SCRIPT_NAME = "Fragments:Quests:QF_W05_MQS_205P_0041CB6D"
POSITIVE_STAGES = (
    20, 10, 30, 40, 50, 100, 200, 250, 300, 350, 400, 450, 700, 900,
    1000, 1050, 1100, 1300, 1400, 1900, 2000, 2100, 2200, 2300, 9000,
)
NEGATIVE_STAGES = (
    500,
    800,
    950,
    1500,
    1600,
    1800,
    1950,
    10000,
)
STAGE_20_MEMBER = """Function Fragment_Stage_0020_Item_00()
    W05_Jen205_Script jenScript = Alias_Jen as W05_Jen205_Script
    If jenScript != None
        jenScript.ActivateStealth()
    EndIf
EndFunction"""


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


def test_patch_preserves_stage_20_and_exact_reconciled_manifest():
    patch = _script_patch_source(SCRIPT_NAME)
    assert patch is not None
    expected_members = [_fragment_member(stage) for stage in POSITIVE_STAGES]

    assert _iter_papyrus_states(patch.splitlines()) == []
    assert not any(
        line.strip().lower().startswith("scriptname ") for line in patch.splitlines()
    )
    assert _member_names(patch) == expected_members
    assert _member_body(patch, "fragment_stage_0020_item_00") == STAGE_20_MEMBER
    assert "SetObjectiveDisplayed(" not in STAGE_20_MEMBER

    assert patch.count("jenScript.ActivateStealth()") == 1


def test_all_eight_negative_members_remain_absent():
    patch = _script_patch_source(SCRIPT_NAME)
    assert patch is not None
    patch_members = set(_member_names(patch))
    positive_members = {_fragment_member(stage) for stage in POSITIVE_STAGES}
    negative_members = {_fragment_member(stage) for stage in NEGATIVE_STAGES}

    assert len(NEGATIVE_STAGES) == 8
    assert positive_members.isdisjoint(negative_members)
    assert len(positive_members) + len(negative_members) == 33
    assert patch_members.isdisjoint(negative_members)


def test_progression_members_exclude_online_reward_and_reputation_surfaces():
    patch = _script_patch_source(SCRIPT_NAME)
    assert patch is not None
    for forbidden in (
        "CompleteQuest(",
        "ModValue(",
        "Reputation_AV_",
        "Community",
        "Bounty",
        "defaultquestencounterwavescript",
    ):
        assert forbidden not in patch


def test_production_merge_is_exact_and_idempotent():
    patch = _script_patch_source(SCRIPT_NAME)
    assert patch is not None
    skeleton = _production_skeleton(SCRIPT_NAME)
    merged = _merge_script_method_patches(skeleton, patch)
    expected_members = [_fragment_member(stage) for stage in POSITIVE_STAGES]

    assert Counter(_member_names(merged)) == Counter(expected_members)
    for member_name in expected_members:
        assert _member_body(merged, member_name) == _member_body(patch, member_name)
    for stage in NEGATIVE_STAGES:
        assert _fragment_member(stage) not in _member_names(merged)
    assert _merge_script_method_patches(merged, patch) == merged


def test_full_production_merge_native_compiles_for_fo4(tmp_path: Path):
    base_source = _fo4_base_source()
    assert base_source is not None, "FO4 base Papyrus sources unavailable"

    dependency = tmp_path / "W05_Jen205_Script.psc"
    dependency.write_text(
        _merged_production_source("W05_Jen205_Script"), encoding="utf-8"
    )
    result = compile_psc(
        _merged_production_source(SCRIPT_NAME),
        imports=[str(tmp_path), str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=f"{SCRIPT_NAME.replace(':', '/')}.psc",
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None
