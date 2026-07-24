from __future__ import annotations

from pathlib import Path

import pytest

from bacup_lib.workflows.unified import (
    _iter_papyrus_states,
    _iter_top_level_papyrus_members,
    _merge_script_method_patches,
    _script_patch_source,
    _script_relative_path,
)
from creation_lib.pex import decompile_pex
from creation_lib.pex.native_runtime import compile_psc

from bacup_lib.tests.test_terminal_fragment_script_patches import _fo4_base_source


REPO_ROOT = Path(__file__).resolve().parents[5]
DEPLOYED_SCRIPTS_ROOT = REPO_ROOT / "mods" / "SeventySix" / "data" / "Scripts"

TRADE_SECRETS_OBJECTIVES = (
    100,
    150,
    200,
    300,
    310,
    325,
    400,
    425,
    427,
    428,
    429,
    449,
    450,
    500,
    600,
    700,
    725,
    800,
    850,
    900,
    901,
    925,
    950,
    1000,
    1200,
)
TRADE_SECRETS_NEGATIVE_STAGES = (
    1,
    125,
    311,
    430,
    431,
    432,
    9999,
    10000,
)
MQS_205_NEGATIVE_STAGES = (
    500,
    800,
    950,
    1500,
    1600,
    1800,
    1950,
    10000,
)
MQS_205_OBJECTIVE_STAGES = (10, 30, 40, 50, 100)


def _fragment_member(stage: int) -> str:
    return f"fragment_stage_{stage:04d}_item_00"


PATCH_CASES = {
    "DefaultCollectionAliasOnDeathA": ("ondeath",),
    "W05_Jen205_Script": ("activatestealth", "ontimer"),
    "Fragments:Quests:QF_W05_MQS_205P_0041CB6D": tuple(
        _fragment_member(stage)
        for stage in (
            20, 10, 30, 40, 50, 100, 200, 250, 300, 350, 400, 450, 700,
            900, 1000, 1050, 1100, 1300, 1400, 1900, 2000, 2100, 2200,
            2300, 9000,
        )
    ),
    "Fragments:Quests:QF_W05_MQSettlers_201P_Indus_003F28C3": tuple(
        _fragment_member(stage)
        for stage in (
            10, 100, 150, 175, 200, 225, 250, 300, 310, 325, 400, 425,
            426, 427, 428, 429, 449, 450, 490, 500, 510, 525, 550, 600,
            700, 625, 725, 730, 731, 750, 800, 810, 825, 850, 851, 875,
            900, 901, 902, 925, 950, 951, 952, 953, 975, 1000, 1010,
            1025, 1200, 1225, 9000,
        )
    ),
}

UNPATCHED_JOIN_CASES = (
    "DefaultSetStageOnInstanceLoadQuest",
    "Fragments:Terminals:TERM_W05_MQS_201P_Motherlode_003F514D",
    "W05_MQS_201P_MotherlodeWaveScript",
    "W05_MQS_201P_QuestScript",
    "W05_MQS_202P_QuestScript",
    "W05_MQS_203P_QuestScript",
    "W05_MQS_204P_FakeWallScript",
    "W05_MQS_204P_PlayerScript",
    "W05_MQS_204P_QuestScript",
)


def _member_names(source: str) -> list[str]:
    return [
        name
        for _kind, name, _start, _end in _iter_top_level_papyrus_members(
            source.splitlines()
        )
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


@pytest.mark.parametrize(("script_name", "expected_members"), PATCH_CASES.items())
def test_patch_has_exact_allowlisted_members(
    script_name: str, expected_members: tuple[str, ...]
):
    patch = _script_patch_source(script_name)
    assert patch is not None
    assert not any(
        line.strip().lower().startswith("scriptname ") for line in patch.splitlines()
    )
    assert _iter_papyrus_states(patch.splitlines()) == []
    assert _member_names(patch) == list(expected_members)


@pytest.mark.parametrize(("script_name", "expected_members"), PATCH_CASES.items())
def test_production_merge_is_exact_and_idempotent(
    script_name: str, expected_members: tuple[str, ...]
):
    patch = _script_patch_source(script_name)
    assert patch is not None
    merged = _merged_production_source(script_name)

    merged_names = _member_names(merged)
    for member_name in expected_members:
        assert merged_names.count(member_name) == 1
        assert _member_body(merged, member_name) == _member_body(patch, member_name)
    assert _merge_script_method_patches(merged, patch) == merged


def test_on_death_patch_only_forwards_to_the_inherited_helper():
    patch = _script_patch_source("DefaultCollectionAliasOnDeathA")
    assert patch == (
        "Event OnDeath(ObjectReference akSenderRef, Actor akKiller)\n"
        "    TryToSetStage(TriggeredRef = akSenderRef, "
        "setStageOnSingleTrigger = setStageWhenAnyRefDies)\n"
        "EndEvent\n"
    )


def test_jen_patch_has_one_spell_timer_scene_chain():
    patch = _script_patch_source("W05_Jen205_Script")
    assert patch is not None
    activate = _member_body(patch, "activatestealth")
    timer = _member_body(patch, "ontimer")

    assert activate.count("W05_MQS_205P_JenStealthSpell.Cast(jenRef, jenRef)") == 1
    assert activate.count("CancelTimer(1)") == 1
    assert activate.count("StartTimer(SpellDuration, 1)") == 1
    assert timer.count("W05_MQS_205P_JenStealthFieldOff.Start()") == 1
    assert "aiTimerID == 1" in timer
    assert "!W05_MQS_205P_JenStealthFieldOff.IsPlaying()" in timer
    for forbidden in ("SetStage(", "AddSpell(", "RemoveSpell("):
        assert forbidden not in patch


def test_mqs_205_manifest_preserves_jen_alias_and_progression():
    script_name = "Fragments:Quests:QF_W05_MQS_205P_0041CB6D"
    patch = _script_patch_source(script_name)
    assert patch is not None
    expected_members = list(PATCH_CASES[script_name])
    patch_members = _member_names(patch)

    assert patch_members == expected_members
    stage_20 = _member_body(patch, "fragment_stage_0020_item_00")
    assert stage_20.count("Alias_Jen as W05_Jen205_Script") == 1
    assert stage_20.count("jenScript.ActivateStealth()") == 1
    assert "SetObjectiveDisplayed(" not in stage_20
    assert "playerRef.SetValue(W05_MQS_205P_Started, 1.0)" in _member_body(
        patch, _fragment_member(10)
    )
    assert len(MQS_205_NEGATIVE_STAGES) == 8
    assert all(
        _fragment_member(stage) not in patch_members
        for stage in MQS_205_NEGATIVE_STAGES
    )
    for forbidden in ("Community", "Bounty", "Reputation_AV_", "defaultquestencounterwavescript"):
        assert forbidden not in patch


def test_trade_secrets_manifest_is_exact_and_excludes_online_surfaces():
    script_name = "Fragments:Quests:QF_W05_MQSettlers_201P_Indus_003F28C3"
    patch = _script_patch_source(script_name)
    assert patch is not None

    assert _member_names(patch) == list(PATCH_CASES[script_name])
    assert all(
        _fragment_member(stage) not in _member_names(patch)
        for stage in TRADE_SECRETS_NEGATIVE_STAGES
    )
    assert patch.count("SetObjectiveDisplayed(") == len(TRADE_SECRETS_OBJECTIVES)
    for forbidden in ("Community", "Bounty", "Reputation_AV_", "defaultquestencounterwavescript"):
        assert forbidden not in patch


@pytest.mark.parametrize("script_name", UNPATCHED_JOIN_CASES)
def test_unsupported_nondefect_and_open_join_rows_remain_unpatched(
    script_name: str,
):
    assert _script_patch_source(script_name) is None


@pytest.mark.parametrize("script_name", PATCH_CASES)
def test_full_production_merge_compiles_for_fo4(
    script_name: str, tmp_path: Path
):
    base_source = _fo4_base_source()
    assert base_source is not None, "FO4 base Papyrus sources unavailable"

    imports = [str(base_source)]
    if script_name == "Fragments:Quests:QF_W05_MQS_205P_0041CB6D":
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
