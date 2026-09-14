from __future__ import annotations

from collections import Counter
from pathlib import Path

import pytest

from bacup_lib.tests.test_script_patch_conventions import (
    has_unregistered_inventory_handler,
    sibling_script_casts,
)
from bacup_lib.tests.test_terminal_fragment_script_patches import _fo4_base_source
from bacup_lib.workflows.unified import (
    _iter_top_level_papyrus_members,
    _merge_script_method_patches,
    _script_patch_source,
    _script_relative_path,
)
from creation_lib.pex.native_runtime import compile_psc


REPO_ROOT = Path(__file__).resolve().parents[5]
SOURCE_ROOT = REPO_ROOT / "mods" / "SeventySix" / "Scripts" / "Source" / "User"

SQ01_QF = "Fragments:Quests:QF_AC_SQ01_HellsEagles_0073BAFC"
SQ01_ROOT = "Quests:AC_SQ01_HellsEagles:QuestScript"
SQ03_QF = "Fragments:Quests:QF_AC_SQ03_Custodial_00729CBB"
SQ03_MESSES = "Quests:AC_SQ03_Custodial:MessActivatorScript"
SQ03_STORAGE = "Quests:AC_SQ03_Custodial:PropStorageActivatorScript"
SQ03_PERK = "Fragments:Perks:PRKF_AC_SQ03_Custodial_Capti_00746186"
SQ05_QF = "Fragments:Quests:QF_AC_SQ05_Regent_006FCF87"
SQ05_ROOT = "AC_SQ05_Regent_QuestScript"

PATCHED_SCRIPTS = (
    SQ01_QF,
    SQ01_ROOT,
    SQ03_QF,
    SQ03_MESSES,
    SQ03_STORAGE,
    SQ05_QF,
    SQ05_ROOT,
)

BOUND_FRAGMENT_STAGES = {
    SQ01_QF: (
        1,
        2,
        50,
        75,
        80,
        100,
        200,
        300,
        400,
        450,
        460,
        470,
        480,
        500,
        600,
        700,
        750,
        800,
        900,
        950,
        1000,
        1100,
        1200,
        1240,
        1260,
        1300,
        1400,
        1450,
        1500,
        1525,
        1550,
        1600,
        1700,
        9000,
        9001,
        10000,
    ),
    SQ03_QF: (
        10,
        20,
        30,
        100,
        110,
        140,
        200,
        250,
        300,
        400,
        499,
        500,
        505,
        510,
        520,
        540,
        550,
        554,
        558,
        580,
        590,
        600,
        700,
        750,
        800,
        900,
        959,
        960,
        1000,
        1010,
        1020,
        1050,
        1055,
        1060,
        1100,
        1200,
        1250,
        1260,
        1270,
        1300,
        1399,
        1400,
        1500,
        1501,
        1550,
        1560,
        9000,
        9100,
        10000,
    ),
    SQ05_QF: (
        100,
        105,
        110,
        115,
        116,
        117,
        119,
        125,
        127,
        129,
        130,
        135,
        140,
        141,
        145,
        152,
        155,
        161,
        162,
        165,
        170,
        171,
        172,
        175,
        180,
        185,
        250,
        252,
        255,
        260,
        269,
        270,
        272,
        274,
        276,
        277,
        278,
        279,
        280,
        281,
        282,
        284,
        286,
        288,
        290,
        9000,
    ),
}


def _patch(script_name: str) -> str:
    patch = _script_patch_source(script_name)
    assert patch is not None
    return patch


def _members(source: str) -> list[tuple[str, int, int]]:
    return [
        (name, start, end)
        for kind, name, start, end in _iter_top_level_papyrus_members(
            source.splitlines()
        )
        if kind in {"function", "event"}
    ]


def _member_body(source: str, member_name: str) -> str:
    lines = source.splitlines()
    start, end = next(
        (start, end)
        for name, start, end in _members(source)
        if name == member_name.casefold()
    )
    return "\n".join(lines[start : end + 1])


def _merged(script_name: str) -> str:
    source_path = SOURCE_ROOT / _script_relative_path(script_name, ".psc")
    assert source_path.is_file(), source_path
    return _merge_script_method_patches(
        source_path.read_text(encoding="utf-8"), _patch(script_name)
    )


@pytest.mark.parametrize("script_name", PATCHED_SCRIPTS)
def test_side_quest_patch_is_member_only_unique_and_idempotent(
    script_name: str,
) -> None:
    patch = _patch(script_name)
    assert not any(
        line.strip().casefold().startswith(("scriptname ", "state ", "auto state "))
        for line in patch.splitlines()
    )
    assert not any(
        " property " in f" {line.strip().casefold()} " for line in patch.splitlines()
    )
    assert not has_unregistered_inventory_handler(patch)
    assert not sibling_script_casts(patch)

    names = [name for name, _start, _end in _members(patch)]
    assert Counter(names) == Counter(set(names))
    merged = _merged(script_name)
    merged_names = [name for name, _start, _end in _members(merged)]
    for member_name in names:
        assert merged_names.count(member_name) == 1
        assert _member_body(merged, member_name) == _member_body(patch, member_name)
    assert _merge_script_method_patches(merged, patch) == merged


@pytest.mark.parametrize(("script_name", "stages"), BOUND_FRAGMENT_STAGES.items())
def test_side_quest_qf_matches_exact_live_vmad_fragment_set(
    script_name: str, stages: tuple[int, ...]
) -> None:
    fragments = {
        name
        for name, _start, _end in _members(_patch(script_name))
        if name.startswith("fragment_stage_")
    }
    assert fragments == {f"fragment_stage_{stage:04d}_item_00" for stage in stages}


def test_sq01_restores_local_search_interview_and_cleanup_chain() -> None:
    patch = _patch(SQ01_QF)
    assert (
        "GiveItemIfMissing(AC_SQ01_HellsEagles_Investigation01_Holotape)"
        in _member_body(patch, "fragment_stage_0200_item_00")
    )
    assert "AC_SQ01_HellsEagles_Surly_ChemFailure_Scene" in _member_body(
        patch, "fragment_stage_0460_item_00"
    )
    assert "EnableCollection(Alias_Actors_Guards)" in _member_body(
        patch, "fragment_stage_0900_item_00"
    )
    assert "AC_SQ01_CompletedInterview_AV" in _member_body(
        patch, "fragment_stage_1400_item_00"
    )
    assert "GiveItemIfMissing(AC_SQ01_HellsEagles_ToOscarNote)" in _member_body(
        patch, "fragment_stage_1450_item_00"
    )
    assert "SetObjectiveCompleted(150)" in _member_body(
        patch, "fragment_stage_9000_item_00"
    )
    assert "Stop()" in _member_body(patch, "fragment_stage_10000_item_00")

    root = _patch(SQ01_ROOT)
    assert "player.SetValue(AC_SQ01_LittleRob_Anger_AV, 0.0)" in root
    assert "surly.RemoveFromFaction(BloodEagleFaction)" in root


def test_sq03_restores_collection_helpers_and_all_required_convergence() -> None:
    messes = _patch(SQ03_MESSES)
    assert "MessesCleanedReq = GetCount()" in messes
    assert "Find(akSenderRef) < 0" in messes
    assert "RemoveRef(akSenderRef)" in messes
    assert "OwningQuest.SetStage(Stage_AllMessesCleaned)" in messes

    storage = _patch(SQ03_STORAGE)
    assert "OwningQuest.IsStageDone(currentProp.FoundStage)" in storage
    assert "currentProp.PropToEnable.GetReference().Enable()" in storage
    assert "OwningQuest.SetStage(currentProp.PlacedStage)" in storage

    qf = _patch(SQ03_QF)
    prop_gate = _member_body(qf, "tryfinishpropcollection")
    assert all(f"IsStageDone({stage})" in prop_gate for stage in (550, 554, 558))
    cleaning_gate = _member_body(qf, "tryfinishcleaning")
    assert all(f"IsStageDone({stage})" in cleaning_gate for stage in (510, 520, 590))
    assert "AC_SQ03_Custodial_CaptiveActivatePerk" in _member_body(
        qf, "fragment_stage_1400_item_00"
    )
    assert "FreeVictim()" in _member_body(qf, "fragment_stage_1560_item_00")
    assert "SetObjectiveCompleted(150)" in _member_body(
        qf, "fragment_stage_9000_item_00"
    )


def test_sq03_bound_captive_perk_fragment_is_already_behavioral() -> None:
    source_path = SOURCE_ROOT / _script_relative_path(SQ03_PERK, ".psc")
    source = source_path.read_text(encoding="utf-8")
    entry = _member_body(source, "fragment_entry_00")

    assert "akActor.RemovePerk(AC_SQ03_Custodial_CaptiveActivatePerk)" in entry
    assert "target.RemoveKeyword(AC_SQ03_Custodial_BoundCaptiveKeyword)" in entry
    assert "target.EvaluatePackage(False)" in entry


def test_sq05_restores_clue_counter_and_all_decision_branches() -> None:
    root = _patch(SQ05_ROOT)
    refresh = _member_body(root, "refreshclueprogress")
    assert "clueStage = minClueStage" in refresh
    assert "clueStage <= maxClueStage" in refresh
    assert "clueCounter >= totalCluesNeeded" in refresh
    assert "SetStage(allCluesFoundStage)" in refresh

    qf = _patch(SQ05_QF)
    assert "GiveItemIfMissing(BookKey)" in _member_body(
        qf, "fragment_stage_0161_item_00"
    )
    assert "GiveItemIfMissing(DevilsBlood)" in _member_body(
        qf, "fragment_stage_0162_item_00"
    )
    assert "SetObjectiveDisplayed(55)" in _member_body(
        qf, "fragment_stage_0165_item_00"
    )
    for stage in (272, 274, 276, 277, 280, 284, 286, 288):
        assert _member_body(qf, f"fragment_stage_{stage:04d}_item_00")
    assert "SetObjectiveDisplayed(110)" in _member_body(
        qf, "fragment_stage_0290_item_00"
    )
    assert "SetObjectiveCompleted(110)" in _member_body(
        qf, "fragment_stage_9000_item_00"
    )


def test_side_quest_starts_and_stage_9000_rewards_remain_pipeline_owned() -> None:
    combined = "\n".join(_patch(script_name) for script_name in PATCHED_SCRIPTS)
    assert "SendStoryEvent" not in combined
    assert "Game.GetFormFromFile" not in combined
    assert "QuestReward" not in combined
    assert "RewardCaps" not in combined

    for qf in (SQ01_QF, SQ03_QF, SQ05_QF):
        terminal = _member_body(_patch(qf), "fragment_stage_9000_item_00")
        assert "Caps001" not in terminal
        assert "Reward" not in terminal


@pytest.mark.parametrize("script_name", PATCHED_SCRIPTS)
def test_side_quest_full_production_merge_native_compiles_for_fo4(
    script_name: str,
) -> None:
    base_source = _fo4_base_source()
    assert base_source is not None, "FO4 base Papyrus sources unavailable"

    result = compile_psc(
        _merged(script_name),
        imports=[str(SOURCE_ROOT), str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=str(_script_relative_path(script_name, ".psc")),
    )
    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, f"{script_name}: {diagnostics}"
    assert result.pex_bytes is not None
