from __future__ import annotations

from pathlib import Path

import pytest

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
ROOT_SCRIPT = "MQ_OverseerQuestScript"
FRAGMENT_SCRIPT = "Fragments:Quests:QF_MQ_Overseer_004E49D9"
PLAYER_SCRIPT = "MQ_OverseerPlayerScript"
CACHE_SCRIPT = "MQ_Overseer_CacheRefScript"
PATCHED_SCRIPTS = (ROOT_SCRIPT, FRAGMENT_SCRIPT, PLAYER_SCRIPT, CACHE_SCRIPT)

# Stage numbers declared by the MQ_Overseer (004E49D9) QUST VMAD fragment table.
DECLARED_STAGES = {
    5, 10, 15, 16, 17, 18, 20, 22, 25, 30, 32, 35, 40, 42, 45, 50, 52, 55, 60,
    62, 65, 70, 72, 75, 80, 82, 85, 90, 92, 95, 100, 102, 105, 110, 112, 115,
    120, 122, 125, 500, 502, 505, 510, 512, 515, 520, 522, 525, 530, 532, 535,
    540, 542, 545, 1000, 9000,
}

# Holotape suffix -> (pick-up stage, played stage). Pick-up stages are proven by
# the stage 9000 condition pairs (GetActorValue(<tape>PickedUp) / GetStageDone);
# the split between pick-up and played is proven by the per-stage author notes.
HOLOTAPE_STAGES = {
    "01": (10, 15),
    "01A": (16, 17),
    "02": (22, 25),
    "03": (32, 35),
    "04": (42, 45),
    "05": (52, 55),
    "06": (62, 65),
    "07": (72, 75),
    "08": (82, 85),
    "09": (92, 95),
    "10": (102, 105),
    "11": (112, 115),
    "12": (122, 125),
    "X1": (502, 505),
    "X2": (512, 515),
    "X3": (522, 525),
    "X4": (532, 535),
    "X5": (542, 545),
}


def _member_body(source: str, member_name: str) -> str:
    lines = source.splitlines()
    start, end = next(
        (start, end)
        for _kind, name, start, end in _iter_top_level_papyrus_members(lines)
        if name == member_name.lower()
    )
    return "\n".join(lines[start : end + 1])


def _member_names(source: str) -> list[str]:
    return [
        name
        for _kind, name, _start, _end in _iter_top_level_papyrus_members(
            source.splitlines()
        )
    ]


def _fragment_stage_numbers(source: str) -> set[int]:
    return {
        int(name[len("fragment_stage_") : -len("_item_00")])
        for name in _member_names(source)
        if name.startswith("fragment_stage_") and name.endswith("_item_00")
    }


def _merged_production_source(script_name: str) -> str:
    source_path = SOURCE_ROOT / _script_relative_path(script_name, ".psc")
    patch = _script_patch_source(script_name)
    assert source_path.is_file(), source_path
    assert patch is not None
    return _merge_script_method_patches(
        source_path.read_text(encoding="utf-8"), patch
    )


@pytest.mark.parametrize("script_name", PATCHED_SCRIPTS)
def test_overseer_patch_is_member_only(script_name: str):
    patch = _script_patch_source(script_name)
    assert patch is not None
    assert not any(
        line.strip().lower().startswith(("scriptname ", "state ", "endstate"))
        for line in patch.splitlines()
    )
    assert not any(
        " property " in f" {line.strip().lower()} " for line in patch.splitlines()
    )


def test_onquestinit_sets_stage_five_once():
    patch = _script_patch_source(ROOT_SCRIPT)
    assert patch is not None
    init = _member_body(patch, "onquestinit")
    assert init.count("IsStageDone(5)") == 1
    assert init.count("SetStage(5)") == 1
    assert init.index("!IsStageDone(5)") < init.index("SetStage(5)")


def test_stage_five_starts_quest_and_displays_objective_ten():
    stage = _member_body(_script_patch_source(FRAGMENT_SCRIPT), "fragment_stage_0005_item_00")
    assert stage.count("SetObjectiveDisplayed(10)") == 1
    assert stage.count("SetValue(MQ_OverseerStarted, 1.0)") == 1


def test_fragment_patch_only_implements_declared_stages():
    implemented = _fragment_stage_numbers(_script_patch_source(FRAGMENT_SCRIPT))
    assert implemented <= DECLARED_STAGES
    expected = {5, 1000, 9000}
    for pick_up, played in HOLOTAPE_STAGES.values():
        expected.add(pick_up)
        expected.add(played)
    assert implemented == expected


@pytest.mark.parametrize("suffix", sorted(HOLOTAPE_STAGES))
def test_each_holotape_stage_records_its_own_actor_value(suffix: str):
    patch = _script_patch_source(FRAGMENT_SCRIPT)
    pick_up, played = HOLOTAPE_STAGES[suffix]

    pick_up_body = _member_body(patch, f"fragment_stage_{pick_up:04d}_item_00")
    assert (
        f"RecordHolotapePickedUp(MQ_OverseerHolotape{suffix}PickedUp)" in pick_up_body
    )
    assert "Played" not in pick_up_body

    played_body = _member_body(patch, f"fragment_stage_{played:04d}_item_00")
    assert f"RecordHolotapePlayed(MQ_OverseerHolotape{suffix}Played)" in played_body
    assert "PickedUp" not in played_body


def test_all_pick_up_actor_values_gate_stage_1000():
    body = _member_body(_script_patch_source(FRAGMENT_SCRIPT), "haseveryholotape")
    for suffix in HOLOTAPE_STAGES:
        assert f"MQ_OverseerHolotape{suffix}PickedUp" in body
    guard = _member_body(_script_patch_source(FRAGMENT_SCRIPT), "recordholotapepickedup")
    assert "!IsStageDone(1000)" in guard
    assert "SetStage(1000)" in guard


def test_stage_1000_and_9000_advance_the_objectives():
    patch = _script_patch_source(FRAGMENT_SCRIPT)
    stage_1000 = _member_body(patch, "fragment_stage_1000_item_00")
    assert "SetObjectiveCompleted(10)" in stage_1000
    assert "SetObjectiveDisplayed(1000)" in stage_1000
    assert "SetValue(W05_MQ_Overseer_HasAllHolotapes, 1.0)" in stage_1000
    assert "SetObjectiveCompleted(1000)" in _member_body(
        patch, "fragment_stage_9000_item_00"
    )


@pytest.mark.parametrize("suffix", sorted(HOLOTAPE_STAGES))
def test_player_alias_fires_both_stages_for_each_holotape(suffix: str):
    patch = _script_patch_source(PLAYER_SCRIPT)
    pick_up, played = HOLOTAPE_STAGES[suffix]
    added = _member_body(patch, "onitemadded")
    played_map = _member_body(patch, "playedstagefor")
    sync = _member_body(patch, "syncholotapesalreadyheld")

    assert added.count(f", {pick_up})") == 1
    assert sync.count(f", {pick_up})") == 1
    assert played_map.count(f"Return {played}\n") == 1


def test_player_alias_stage_setter_is_idempotent():
    body = _member_body(_script_patch_source(PLAYER_SCRIPT), "setholotapestage")
    assert "!myQuest.IsStageDone(aiStage)" in body
    assert "myQuest.SetStage(aiStage)" in body


def test_cache_ref_requests_start_through_the_story_manager():
    body = _member_body(_script_patch_source(CACHE_SCRIPT), "requestoverseerqueststart")
    # MQ_Overseer keeps its ENAM, so Start() would be rejected: only the story
    # event may start it.
    assert "overseerQuest.Start()" not in body
    assert "0x004E49E5" in body
    assert "SendStoryEventAndWait()" in body
    assert "IsRunning()" in body


@pytest.mark.parametrize("script_name", PATCHED_SCRIPTS)
def test_production_merge_is_unique_and_idempotent(script_name: str):
    patch = _script_patch_source(script_name)
    assert patch is not None
    merged = _merged_production_source(script_name)
    names = _member_names(merged)

    for member_name in _member_names(patch):
        assert names.count(member_name) == 1
        assert _member_body(merged, member_name) == _member_body(patch, member_name)
    assert _merge_script_method_patches(merged, patch) == merged


@pytest.mark.parametrize("script_name", PATCHED_SCRIPTS)
def test_full_production_merge_native_compiles_for_fo4(script_name: str):
    base_source = _fo4_base_source()
    assert base_source is not None, "FO4 base Papyrus sources unavailable"

    result = compile_psc(
        _merged_production_source(script_name),
        imports=[str(SOURCE_ROOT), str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=str(_script_relative_path(script_name, ".psc")),
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None
