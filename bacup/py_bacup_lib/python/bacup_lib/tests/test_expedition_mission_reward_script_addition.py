from __future__ import annotations

from pathlib import Path

import pytest

from bacup_lib.tests.test_terminal_fragment_script_patches import _fo4_base_source
from bacup_lib.workflows import unified
from creation_lib.pex.native_runtime import compile_psc


SCRIPT_NAME = "B21:ExpeditionMissionRewards"
SCRIPT_PATH = (
    unified._SCRIPT_ADDITION_DIR / "fo76_fo4" / "B21" / "ExpeditionMissionRewards.psc"
)


def _source() -> str:
    return SCRIPT_PATH.read_text(encoding="utf-8")


def _body(source: str, start: str, end: str) -> str:
    return source.split(start, 1)[1].split(end, 1)[0]


def test_expedition_reward_addition_is_pair_scoped_and_has_exact_contract() -> None:
    additions = unified._script_addition_sources("fo76", "fo4")
    key = unified._script_key(SCRIPT_NAME)

    assert additions[key] == (SCRIPT_NAME, SCRIPT_PATH)
    assert key not in unified._script_addition_sources("skyrimse", "fo4")

    source = _source()
    lines = [line.strip() for line in source.splitlines() if line.strip()]
    assert lines[0] == f"Scriptname {SCRIPT_NAME} Extends Quest"
    assert [line for line in lines if " Property " in line] == [
        "Int[] Property OptionalStages Auto Const",
        "GlobalVariable[] Property TierXP Auto",
        "Int[] Property TierStampCounts Auto Const",
        "MiscObject Property StampItem Auto",
    ]


def test_expedition_reward_resets_and_grants_exactly_once_per_run() -> None:
    source = _source()
    init = _body(source, "Event OnQuestInit()", "EndEvent")
    stage_set = _body(
        source,
        "Event OnStageSet(Int auiStageID, Int auiItemID)",
        "EndEvent",
    )
    grant = _body(source, "Function GrantCompletionReward()", "EndFunction")

    assert "RewardGrantedThisRun = False" in init
    assert "auiStageID == 0" in stage_set
    assert "RewardGrantedThisRun = False" in stage_set
    assert "auiStageID == 9000" in stage_set
    assert stage_set.count("GrantCompletionReward()") == 1
    assert "RewardGrantedThisRun || !RewardContractIsValid()" in grant
    assert grant.index("RewardGrantedThisRun = True") < grant.index(
        "Game.RewardPlayerXP"
    )
    assert grant.index("Game.RewardPlayerXP") < grant.index(
        "playerRef.AddItem(StampItem, stampCount, True)"
    )
    assert source.count("Game.RewardPlayerXP") == 1
    assert source.count("playerRef.AddItem") == 1


def test_expedition_reward_selects_tier_only_from_optional_success_stages() -> None:
    source = _source()
    count = _body(
        source,
        "Int Function CompletedOptionalCount()",
        "EndFunction",
    )
    validation = _body(
        source,
        "Bool Function RewardContractIsValid()",
        "EndFunction",
    )

    assert "While index < OptionalStages.Length" in count
    assert "IsStageDone(OptionalStages[index])" in count
    assert "completed += 1" in count
    assert "OptionalStages.Length == 3" in validation
    assert "TierXP.Length == 4" in validation
    assert "TierStampCounts.Length == 4" in validation
    for excluded in (
        "Daily",
        "Weekly",
        "Legendary",
        "Treasury",
        "Team",
        "Random",
    ):
        assert excluded not in source


def test_expedition_reward_addition_compiles_with_fo4_imports() -> None:
    base_source = _fo4_base_source()
    if base_source is None:
        pytest.skip("FO4 base Papyrus sources unavailable")

    result = compile_psc(
        _source(),
        imports=[str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=str(Path("B21") / "ExpeditionMissionRewards.psc"),
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None
