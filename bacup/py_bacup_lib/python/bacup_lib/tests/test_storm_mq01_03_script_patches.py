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
from creation_lib.pex.native_runtime import compile_psc


REPO_ROOT = Path(__file__).resolve().parents[5]
SOURCE_ROOT = REPO_ROOT / "mods" / "SeventySix" / "Scripts" / "Source" / "User"
MQ01 = "Fragments:Quests:QF_Storm_MQ01_Breadcrumb_0072A2A8"
MQ02 = "Fragments:Quests:QF_Storm_MQ02_IntroPt1_00734C6B"
MQ03 = "Fragments:Quests:QF_Storm_MQ03_IntroPt2_00733833"
MQ02_MESSAGE = "StormMQ02MessageBoxScript"
MQ03_HILDA_EXIT = "Fragments:Packages:PF_Storm_MQ03_HildaExit_pack_007734E2"
PATCHED_SCRIPTS = (MQ01, MQ02, MQ03, MQ02_MESSAGE, MQ03_HILDA_EXIT)

DECLARED_STAGES = {
    MQ01: {100, 200, 300, 400, 500, 9000, 9900},
    MQ02: {
        100,
        150,
        160,
        170,
        200,
        250,
        260,
        290,
        300,
        400,
        410,
        450,
        500,
        550,
        600,
        610,
        620,
        625,
        630,
        700,
        800,
        9000,
        9500,
    },
    MQ03: {
        100,
        200,
        300,
        310,
        320,
        330,
        340,
        350,
        400,
        500,
        550,
        600,
        700,
        800,
        900,
        950,
        1000,
        1050,
        1060,
        1100,
        1130,
        1200,
        1300,
        9000,
        9500,
    },
}


def _patch(script_name: str) -> str:
    patch = _script_patch_source(script_name)
    assert patch is not None
    return patch


def _members(source: str) -> list[str]:
    return [
        name
        for kind, name, _start, _end in _iter_top_level_papyrus_members(
            source.splitlines()
        )
        if kind in {"function", "event"}
    ]


def _member_body(source: str, member_name: str) -> str:
    lines = source.splitlines()
    start, end = next(
        (start, end)
        for _kind, name, start, end in _iter_top_level_papyrus_members(lines)
        if name == member_name.lower()
    )
    return "\n".join(lines[start : end + 1])


def _stage(script_name: str, stage: int, item: int = 0) -> str:
    return _member_body(
        _patch(script_name), f"fragment_stage_{stage:04d}_item_{item:02d}"
    )


def _merged(script_name: str) -> str:
    source_path = SOURCE_ROOT / _script_relative_path(script_name, ".psc")
    assert source_path.is_file(), source_path
    return _merge_script_method_patches(
        source_path.read_text(encoding="utf-8"), _patch(script_name)
    )


@pytest.mark.parametrize("script_name", PATCHED_SCRIPTS)
def test_storm_mq01_03_patches_are_member_only(script_name: str):
    patch = _patch(script_name)
    assert not any(
        line.strip().lower().startswith(("scriptname ", "state ", "endstate"))
        for line in patch.splitlines()
    )
    assert not any(
        " property " in f" {line.strip().lower()} " for line in patch.splitlines()
    )


@pytest.mark.parametrize("script_name", PATCHED_SCRIPTS)
def test_storm_mq01_03_production_merge_is_unique_and_idempotent(script_name: str):
    patch = _patch(script_name)
    merged = _merged(script_name)
    patch_members = _members(patch)
    merged_members = Counter(_members(merged))

    assert Counter(patch_members) == Counter({name: 1 for name in patch_members})
    for member_name in patch_members:
        assert merged_members[member_name] == 1
        assert _member_body(merged, member_name) == _member_body(patch, member_name)
    assert _merge_script_method_patches(merged, patch) == merged


@pytest.mark.parametrize("script_name, declared", DECLARED_STAGES.items())
def test_storm_mq01_03_fragments_only_implement_vmad_declared_stages(
    script_name: str, declared: set[int]
):
    implemented = {
        int(name[len("fragment_stage_") : len("fragment_stage_") + 4])
        for name in _members(_patch(script_name))
        if name.startswith("fragment_stage_")
    }
    assert implemented <= declared


def test_mq01_objectives_radio_and_story_handoff_are_restored():
    assert "SetObjectiveDisplayed(10)" in _stage(MQ01, 100)
    assert "Storm_MQ01_Breadcrumb_Radio_QuestStartKeyword.SendStoryEventAndWait" in _stage(
        MQ01, 100
    )
    for completed, displayed, stage in ((10, 20, 200), (20, 30, 300), (30, 40, 400)):
        body = _stage(MQ01, stage)
        assert f"SetObjectiveCompleted({completed})" in body
        assert f"SetObjectiveDisplayed({displayed})" in body
    handoff = _stage(MQ01, 500)
    assert "Storm_MQ02_IntroPt1_QuestStartKeyword.SendStoryEventAndWait" in handoff
    assert "Storm_MQ02_IntroPt1.Start()" not in handoff
    assert "SetStage(9000)" in handoff


def test_mq02_skips_only_online_waves_and_preserves_scene_and_optional_flares():
    for stage in (150, 160, 170, 200, 250, 260):
        assert "SetStage(290)" in _stage(MQ02, stage)
    assert "Scene_Hilda_Callout" in _stage(MQ02, 290)
    assert "SetStage(700)" in _stage(MQ02, 400)
    assert "Scene_Craig_Callouts" in _stage(MQ02, 410)
    assert "PlaceFlareAt(Alias_Flare01)" in _stage(MQ02, 610)
    assert "PlaceFlareAt(Alias_Flare02)" in _stage(MQ02, 625)
    assert "PlaceFlareAt(Alias_Flare03)" in _stage(MQ02, 700)
    completion = _stage(MQ02, 9000)
    assert "Storm_MQ03_IntroPt2_StartKeyword.SendStoryEventAndWait" in completion
    assert "SetStage(9500)" in completion
    assert "EncounterWaves" not in _patch(MQ02)


def test_mq02_skill_messages_choose_highest_eligible_and_set_bound_stage():
    activate = _member_body(_patch(MQ02_MESSAGE), "onactivate")
    assert "akActionRef != playerRef" in activate
    assert "Messages[index].RequiredAmount > highestRequirement" in activate
    assert "messageToShow = FallbackMessage" in activate
    assert "ButtonStagesToSet[selectedButton]" in activate
    assert "owningQuest.SetStage(stageToSet)" in activate


def test_mq03_online_combat_substitute_converges_and_story_path_survives():
    for stage in (200, 300, 310, 320, 330, 340, 350):
        assert "AdvancePastOnlineEncounter()" in _stage(MQ03, stage)
    assert "Scene_Hilda_Intercom" in _stage(MQ03, 500)
    assert "Key_LostHand" in _stage(MQ03, 600)
    assert "Alias_DHM_WallDoor" in _stage(MQ03, 800)
    assert "Scene_Hilda_Meeting" in _stage(MQ03, 950)
    assert "Scene_Hugo_Intro" in _stage(MQ03, 1060)
    assert "Holotape_OberlinReport" in _stage(MQ03, 1130)
    completion = _stage(MQ03, 9000)
    assert "Storm_MQ04_HugoPt1_StartKeyword.SendStoryEventAndWait" in completion
    assert "Storm_MQ07_OberlinPt1_StartKeyword.SendStoryEventAndWait" in completion
    assert "SetStage(9500)" in completion


def test_mq03_hilda_package_end_advances_cleanup_once():
    fragment = _member_body(_patch(MQ03_HILDA_EXIT), "fragment_end")
    assert "owningQuest.GetStage() >= 1000" in fragment
    assert "!owningQuest.IsStageDone(1050)" in fragment
    assert "owningQuest.SetStage(1050)" in fragment


def test_existing_reward_helper_remains_the_only_reward_authority():
    combined = "\n".join(_patch(script_name) for script_name in (MQ01, MQ02, MQ03))
    for forbidden in ("B21:QuestRewards", "Caps001", "AddCaps", "Reward"):
        assert forbidden not in combined


@pytest.mark.parametrize("script_name", PATCHED_SCRIPTS)
def test_storm_mq01_03_full_production_merge_native_compiles_for_fo4(
    script_name: str,
):
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
    assert result.ok, diagnostics
    assert result.pex_bytes is not None
