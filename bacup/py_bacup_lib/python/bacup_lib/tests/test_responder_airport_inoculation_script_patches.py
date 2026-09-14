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

RS01B = "Fragments:Quests:QF_RS01B_Contact_003C4C23"
RS03 = "Fragments:Quests:QF_RS03_Inoculation_0022730F"
RS03_PLAYER = "RS03_PlayerScript"
GHOUL_PERK = "Fragments:Perks:PRKF_RS03_GhoulBloodSamplePe_004E23F7"
MOLERAT_PERK = "Fragments:Perks:PRKF_RS03_MoleratBloodSample_004E23F6"
WOLF_PERK = "Fragments:Perks:PRKF_RS03_WolfBloodSamplePer_004E23F5"


def _stage_member(stage: int) -> str:
    return f"fragment_stage_{stage:04d}_item_00"


RS01B_STAGES = frozenset({100, 125, 200, 300, 400, 600, 1000})
RS03_STAGES = frozenset(
    {
        10,
        100,
        200,
        205,
        210,
        220,
        230,
        250,
        260,
        305,
        315,
        325,
        400,
        500,
        550,
        575,
        600,
        650,
        700,
        710,
        720,
        730,
        740,
        750,
        760,
        770,
        1000,
        1100,
        2000,
    }
)
RS03_HELPERS = frozenset(
    {
        "restorecanonicalsamplehistory",
        "restorecanonicalfusehistory",
        "preparemodernsamplecollection",
    }
)

PATCH_MEMBERS = {
    RS01B: {_stage_member(stage) for stage in RS01B_STAGES},
    RS03: {_stage_member(stage) for stage in RS03_STAGES} | RS03_HELPERS,
    RS03_PLAYER: {
        "registerautodocactivation",
        "onaliasinit",
        "onplayerloadgame",
        "objectreference.onactivate",
    },
    GHOUL_PERK: {"setrs03ghoulstage"},
    MOLERAT_PERK: {"setrs03moleratstage"},
    WOLF_PERK: {"setrs03wolfstage"},
}


def _member_names(source: str) -> list[str]:
    return [
        name
        for _kind, name, _start, _end in _iter_top_level_papyrus_members(
            source.splitlines()
        )
    ]


def _member_body(source: str, member_name: str) -> str:
    lines = source.splitlines()
    start, end = next(
        (start, end)
        for _kind, name, start, end in _iter_top_level_papyrus_members(lines)
        if name == member_name.lower()
    )
    return "\n".join(lines[start : end + 1])


def _merged_source(script_name: str) -> str:
    source_path = SOURCE_ROOT / _script_relative_path(script_name, ".psc")
    patch = _script_patch_source(script_name)
    assert source_path.is_file(), source_path
    assert patch is not None
    return _merge_script_method_patches(source_path.read_text(encoding="utf-8"), patch)


@pytest.mark.parametrize(("script_name", "members"), PATCH_MEMBERS.items())
def test_responder_airport_inoculation_patches_merge_once_and_are_member_only(
    script_name: str, members: set[str]
):
    patch = _script_patch_source(script_name)
    assert patch is not None
    assert set(_member_names(patch)) == members
    assert not any(
        line.strip().lower().startswith(("scriptname ", "state "))
        for line in patch.splitlines()
    )

    merged = _merged_source(script_name)
    merged_members = _member_names(merged)
    for member in members:
        assert merged_members.count(member) == 1
    assert _merge_script_method_patches(merged, patch) == merged


def test_responder_airport_inoculation_full_merged_sources_compile(tmp_path: Path):
    merged_sources = {
        script_name: _merged_source(script_name) for script_name in PATCH_MEMBERS
    }
    merged_root = tmp_path / "Scripts" / "Source" / "User"
    for script_name, merged in merged_sources.items():
        source_path = merged_root / _script_relative_path(script_name, ".psc")
        source_path.parent.mkdir(parents=True, exist_ok=True)
        source_path.write_text(merged, encoding="utf-8")

    base_source = _fo4_base_source()
    assert base_source is not None, "FO4 base Papyrus sources unavailable"
    for script_name, merged in merged_sources.items():
        result = compile_psc(
            merged,
            imports=[str(merged_root), str(SOURCE_ROOT), str(base_source)],
            game="fo4",
            flags=str(base_source / "Institute_Papyrus_Flags.flg"),
            source_path=str(_script_relative_path(script_name, ".psc")),
        )
        diagnostics = "\n".join(str(item) for item in result.diagnostics)
        assert result.ok, f"{script_name}\n{diagnostics}"
        assert result.pex_bytes is not None


def test_rs01b_progresses_airport_objectives_and_uses_story_handoffs():
    patch = _script_patch_source(RS01B)
    assert patch is not None
    for stage, completed, displayed in (
        (200, 100, 200),
        (300, 200, 300),
        (400, 300, 400),
    ):
        body = _member_body(patch, _stage_member(stage))
        assert f"SetObjectiveCompleted({completed}, True)" in body
        assert f"SetObjectiveDisplayed({displayed}, True)" in body
    completion = _member_body(patch, _stage_member(1000))
    assert "SetValue(RS01B_Contact_Completed, 1.0)" in completion
    assert "RS03_Inoculation_Keyword.SendStoryEvent" in completion
    assert "RS06_Manual_Stims_Keyword.SendStoryEvent" in completion
    assert ".Start(" not in completion

    holotape_collected = _member_body(patch, _stage_member(300))
    assert "GetValue(RS01B_CheckpointValue) < 10.0" in holotape_collected
    assert "SetValue(RS01B_CheckpointValue, 10.0)" in holotape_collected


def test_rs03_sample_perks_set_only_the_evidenced_stages():
    expected = {
        GHOUL_PERK: 315,
        MOLERAT_PERK: 305,
        WOLF_PERK: 325,
    }
    for script_name, stage in expected.items():
        patch = _script_patch_source(script_name)
        assert patch is not None
        assert f"RS03_Inoculation.SetStage({stage})" in patch
        for other_stage in {305, 315, 325} - {stage}:
            assert f"RS03_Inoculation.SetStage({other_stage})" not in patch


def test_rs03_sample_fuse_and_analysis_transactions_converge():
    patch = _script_patch_source(RS03)
    assert patch is not None
    for stage, objective, item, perk in (
        (305, 25, "RS03_Inoculation_MoleratBloodSample", "RS03_MoleratBloodSamplePerk"),
        (315, 35, "RS03_Inoculation_GhoulBloodSample", "RS03_GhoulBloodSamplePerk"),
        (325, 45, "RS03_Inoculation_WolfBloodSample", "RS03_WolfBloodSamplePerk"),
    ):
        body = _member_body(patch, _stage_member(stage))
        assert f"SetObjectiveCompleted({objective}, True)" in body
        assert f"GetItemCount({item}) == 0" in body
        assert f"AddItem({item}, 1, False)" in body
        assert f"RemovePerk({perk})" in body
        assert (
            "GetValue(RS03_Inoculation_CheckPointBlood) < checkpointValue"
            in body
        )

    modern = _member_body(patch, "preparemodernsamplecollection")
    assert "SetObjectiveDisplayed(25, False)" in modern
    assert "SetObjectiveDisplayed(35, True)" in modern
    assert "SetObjectiveDisplayed(45, False)" in modern
    assert "RemovePerk(RS03_MoleratBloodSamplePerk)" in modern
    assert "AddPerk(RS03_GhoulBloodSamplePerk)" in modern
    assert "RemovePerk(RS03_WolfBloodSamplePerk)" in modern

    ghoul = _member_body(patch, _stage_member(315))
    assert "IsStageDone(260)" in ghoul
    assert "SetStage(400)" in ghoul
    for deprecated_stage in (305, 325):
        assert "SetStage(400)" not in _member_body(
            patch, _stage_member(deprecated_stage)
        )

    fuse = _member_body(patch, _stage_member(260))
    assert "RemoveItem(RS03_Inoculation_TypeTFuse, 1, True)" in fuse
    assert (
        "SetValue(RS03_Inoculation_CheckPointFuse, CPFuse_FuseInstalled as Float)"
        in fuse
    )
    collected_fuse = _member_body(patch, _stage_member(250))
    assert (
        "GetValue(RS03_Inoculation_CheckPointFuse) < "
        "CPFuse_FuseCollectedNotInstalled"
        in collected_fuse
    )
    assert "IsStageDone(315)" in fuse
    assert "IsStageDone(305)" not in fuse
    assert "IsStageDone(325)" not in fuse
    centrifuge = _member_body(patch, _stage_member(500))
    for item in (
        "RS03_Inoculation_MoleratBloodSample",
        "RS03_Inoculation_GhoulBloodSample",
        "RS03_Inoculation_WolfBloodSample",
    ):
        assert f"RemoveItem({item}, 1, True)" in centrifuge
    assert "OBJBeakerInsertOneshot.Play" in centrifuge
    assert "OBJMixerMachineOneshotComp.Play" in _member_body(patch, _stage_member(550))


MODERN_BLOOD_SAMPLE_STAGES = {
    0: frozenset(),
    10: frozenset(),
    20: frozenset(),
    30: frozenset({315}),
    40: frozenset(),
    50: frozenset({315}),
    60: frozenset({315}),
    70: frozenset(),
    80: frozenset({315}),
    90: frozenset({315}),
}


def _simulate_rs03_checkpoint_restore(
    blood_checkpoint: int, fuse_checkpoint: int
) -> tuple[list[int], Counter[int]]:
    stage_order: list[int] = []
    stage_counts: Counter[int] = Counter()
    completed: set[int] = set()

    def set_stage(stage: int) -> None:
        if stage in completed:
            return
        completed.add(stage)
        stage_order.append(stage)
        stage_counts[stage] += 1
        if stage in {260, 315}:
            if 260 in completed and 315 in completed:
                set_stage(400)

    for sample_stage in (315,):
        if sample_stage in MODERN_BLOOD_SAMPLE_STAGES[blood_checkpoint]:
            set_stage(sample_stage)

    set_stage(100 if blood_checkpoint <= 0 and fuse_checkpoint <= 0 else 200)

    if fuse_checkpoint >= 20:
        set_stage(250)
    if fuse_checkpoint >= 30:
        set_stage(260)
    elif fuse_checkpoint >= 20:
        set_stage(760)
    else:
        set_stage(210)

    if blood_checkpoint >= 90:
        set_stage(500)
    return stage_order, stage_counts


@pytest.mark.parametrize("blood_checkpoint", sorted(MODERN_BLOOD_SAMPLE_STAGES))
@pytest.mark.parametrize("fuse_checkpoint", (0, 10, 20, 30))
def test_rs03_checkpoint_restore_state_order_matrix(
    blood_checkpoint: int, fuse_checkpoint: int
):
    stage_order, stage_counts = _simulate_rs03_checkpoint_restore(
        blood_checkpoint, fuse_checkpoint
    )
    completed = set(stage_order)

    assert completed & {305, 315, 325} == MODERN_BLOOD_SAMPLE_STAGES[blood_checkpoint]
    assert all(count == 1 for count in stage_counts.values())
    assert (400 in completed) == (
        315 in MODERN_BLOOD_SAMPLE_STAGES[blood_checkpoint]
        and fuse_checkpoint >= 30
    )
    if 400 in completed:
        assert stage_order.index(400) > stage_order.index(260)
        assert stage_order.index(400) > stage_order.index(315)
    if fuse_checkpoint >= 20:
        assert 250 in completed
    if fuse_checkpoint == 20:
        assert stage_order.index(250) < stage_order.index(760)
    if fuse_checkpoint >= 30:
        assert stage_order.index(250) < stage_order.index(260)


def test_rs03_stage10_reconciles_samples_before_fuse_and_terminal_converges_objectives():
    patch = _script_patch_source(RS03)
    assert patch is not None
    startup = _member_body(patch, _stage_member(10))
    assert startup.index(
        "RestoreCanonicalSampleHistory(bloodCheckpoint)"
    ) < startup.index("RestoreCanonicalFuseHistory(fuseCheckpoint)")

    assert (
        "IsStageDone(305) && IsStageDone(315) && IsStageDone(325)"
        not in patch
    )
    for checkpoint_stage in (650, 700, 720, 750):
        assert "PrepareModernSampleCollection()" in _member_body(
            patch, _stage_member(checkpoint_stage)
        )
    for checkpoint_stage in (710, 730, 740, 770):
        assert "SetStage(315)" in _member_body(
            patch, _stage_member(checkpoint_stage)
        )

    completion = _member_body(patch, _stage_member(600))
    for objective in (70, 72, 75):
        assert f"SetObjectiveCompleted({objective}, True)" in completion
    assert "RS03_Inoculation_CentrifugeSoundMarker.Disable()" in completion


def test_rs03_autodoc_completion_rewards_cleans_up_and_hands_off_once():
    player_patch = _script_patch_source(RS03_PLAYER)
    quest_patch = _script_patch_source(RS03)
    assert player_patch is not None
    assert quest_patch is not None
    activation = _member_body(player_patch, "objectreference.onactivate")
    assert "owningQuest.IsStageDone(600)" in activation
    assert "!owningQuest.IsStageDone(1000)" in activation
    assert "owningQuest.SetStage(1000)" in activation

    completion = _member_body(quest_patch, _stage_member(1000))
    assert "AddSpell(RS03_Inoculation_InoculationSpell, False)" in completion
    assert "SetValue(RS03_Inoculation_Completed, 1.0)" in completion
    assert "RS03_Inoculation_Message.Show()" in completion
    assert "MTR06_QuestStartKeyword.SendStoryEvent" in completion
    assert "W05_MQ_101P_QuestStartKeyword.SendStoryEvent" in completion
    assert ".Start(" not in completion
    assert "SetStage(1100)" in completion
    assert "Stop()" in _member_body(quest_patch, _stage_member(1100))
    shutdown = _member_body(quest_patch, _stage_member(2000))
    assert "RemoveItem(RS03_Inoculation_TypeTFuse" in shutdown
    assert "RemovePerk(RS03_MoleratBloodSamplePerk)" in shutdown
    assert "Stop()" in shutdown
