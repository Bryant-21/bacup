from __future__ import annotations

from collections import Counter
from pathlib import Path

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
FRAGMENT_SCRIPT = "Fragments:Quests:QF_FS01_MQ_Warn_00002315"
PLAYER_SCRIPT = "FS01_MQ_Warn_PlayerAliasScript"

FRAGMENT_MEMBERS = {
    "fragment_stage_0000_item_00",
    "fragment_stage_0001_item_00",
    "fragment_stage_0010_item_00",
    "fragment_stage_0025_item_00",
    "fragment_stage_0050_item_00",
    "fragment_stage_0060_item_00",
    "fragment_stage_0100_item_00",
    "fragment_stage_0150_item_00",
    "fragment_stage_0200_item_00",
    "fragment_stage_0220_item_00",
    "fragment_stage_0250_item_00",
    "fragment_stage_0300_item_00",
    "fragment_stage_0400_item_00",
    "fragment_stage_0405_item_00",
    "fragment_stage_0450_item_00",
    "fragment_stage_0455_item_00",
    "fragment_stage_0500_item_00",
    "fragment_stage_0500_item_01",
    "fragment_stage_0510_item_00",
    "fragment_stage_0520_item_00",
    "fragment_stage_0530_item_00",
    "fragment_stage_0540_item_00",
    "fragment_stage_0550_item_00",
    "fragment_stage_0560_item_00",
    "fragment_stage_0600_item_00",
    "fragment_stage_0700_item_00",
    "fragment_stage_0800_item_00",
    "fragment_stage_1000_item_00",
}

HELPER_MEMBERS = {
    "fs01_getplayer",
    "fs01_setplayervalue",
    "fs01_giveitem",
    "fs01_startscene",
    "fs01_runonstart",
    "fs01_giveraleighpassword",
    "fs01_setmajorcheckpoint",
    "fs01_enteramplifiersearch",
    "fs01_updateabbieheatingcoils",
    "fs01_checkamplifierscomplete",
}


def _members(source: str) -> list[str]:
    return [
        name
        for _kind, name, _start, _end in _iter_top_level_papyrus_members(
            source.splitlines()
        )
    ]


def _member(source: str, member_name: str) -> str:
    lines = source.splitlines()
    start, end = next(
        (start, end)
        for _kind, name, start, end in _iter_top_level_papyrus_members(lines)
        if name == member_name.casefold()
    )
    return "\n".join(lines[start : end + 1])


def _skeleton() -> str:
    return (
        SOURCE_ROOT / _script_relative_path(FRAGMENT_SCRIPT, ".psc")
    ).read_text(encoding="utf-8")


def _patch() -> str:
    patch = _script_patch_source(FRAGMENT_SCRIPT)
    assert patch is not None
    return patch


def _merged() -> str:
    return _merge_script_method_patches(_skeleton(), _patch())


def _merged_script(script_name: str) -> str:
    source_path = SOURCE_ROOT / _script_relative_path(script_name, ".psc")
    patch = _script_patch_source(script_name)
    assert source_path.is_file(), source_path
    assert patch is not None
    return _merge_script_method_patches(
        source_path.read_text(encoding="utf-8"), patch
    )


def test_fs01_patch_is_member_only_and_covers_exact_source_fragments() -> None:
    patch = _patch()
    members = _members(patch)

    assert Counter(members) == Counter(FRAGMENT_MEMBERS | HELPER_MEMBERS)
    assert not any(
        line.strip().casefold().startswith(
            ("scriptname ", "extends ", "state ", "auto state ")
        )
        for line in patch.splitlines()
    )
    assert not any(
        " property " in f" {line.strip().casefold()} " for line in patch.splitlines()
    )


def test_fs01_merge_is_unique_and_idempotent() -> None:
    patch = _patch()
    merged = _merged()
    merged_members = _members(merged)

    for member in FRAGMENT_MEMBERS | HELPER_MEMBERS:
        assert merged_members.count(member) == 1
        assert _member(merged, member) == _member(patch, member)
    assert _merge_script_method_patches(merged, patch) == merged


def test_fs01_run_on_start_uses_inventory_and_story_manager_routing() -> None:
    body = _member(_patch(), "FS01_RunOnStart")

    assert "FS01_MQ_Warn_StartedValue" in body
    assert "playerRef.GetItemCount(FS01_Warn_UplinkMiscItem) > 0" in body
    assert "SetStage(200)" in body
    assert "playerRef.GetItemCount(FS01_MQ_Warn_BrokenUplinkMiscItem) > 0" in body
    assert "SetStage(150)" in body
    assert 'Game.GetFormFromFile(0x003A5FA0, "SeventySix.esm") as Quest' in body
    assert "!missingLink.IsRunning()" in body
    assert "!missingLink.GetStageDone(500)" in body
    assert "MTN_MQ_Missing_Quest_Keyword.SendStoryEventAndWait(" in body
    assert ".Start()" not in body

    stage_10 = _member(_patch(), "Fragment_Stage_0010_Item_00")
    assert "FS01_RunOnStart()" in stage_10


def test_fs01_major_checkpoints_and_objectives_follow_source_stages() -> None:
    patch = _patch()
    for stage, checkpoint in (
        (25, 25),
        (50, 25),
        (100, 100),
        (150, 150),
        (200, 200),
        (250, 250),
        (600, 600),
        (700, 700),
    ):
        body = _member(patch, f"Fragment_Stage_{stage:04d}_Item_00")
        assert f"FS01_SetMajorCheckpoint({checkpoint})" in body

    assert "FS01_MQ_Warn_AbbieIntroScene" in _member(
        patch, "Fragment_Stage_0050_Item_00"
    )
    assert "FS01_Warn_AbbieNextStepScene" in _member(
        patch, "Fragment_Stage_0220_Item_00"
    )
    assert "FS01_Hope_AbbieRaleighsBunkerScene" in _member(
        patch, "Fragment_Stage_0300_Item_00"
    )

    password = _member(patch, "FS01_GiveRaleighPassword")
    assert "FS01_Hope_RaleighPassword" in password
    assert "FS01_CheckpointRaleighPassword" in password


def test_fs01_repairs_and_preserves_the_uplink_for_fs02() -> None:
    patch = _patch()
    repair = _member(patch, "Fragment_Stage_0150_Item_00")
    assert "FS01_MQ_Warn_BrokenUplinkMiscItem" in repair
    assert "SetObjectiveDisplayed(150" in repair
    assert "!GetStageDone(200)" in repair
    assert repair.count("SetStage(200)") == 1

    repaired = _member(patch, "Fragment_Stage_0200_Item_00")
    assert "RemoveItem(FS01_MQ_Warn_BrokenUplinkMiscItem, 1, True)" in repaired
    assert "FS01_GiveItem(FS01_Warn_UplinkMiscItem, 1)" in repaired

    completion = _member(patch, "Fragment_Stage_1000_Item_00")
    assert "RemoveItem(FS01_Warn_UplinkMiscItem" not in completion


def test_fs01_transmitter_and_amplifier_progress_is_idempotent() -> None:
    patch = _patch()

    transmitters = _member(patch, "Fragment_Stage_0405_Item_00")
    assert "SetObjectiveCompleted(400, True)" in transmitters
    assert "!GetStageDone(500)" in transmitters
    assert "SetStage(500)" in transmitters

    with_plan = _member(patch, "Fragment_Stage_0500_Item_00")
    without_plan = _member(patch, "Fragment_Stage_0500_Item_01")
    assert "FS01_EnterAmplifierSearch(False)" in with_plan
    assert "FS01_EnterAmplifierSearch(True)" in without_plan

    for stage, checkpoint, objective in (
        (530, "FS01_CheckpointEllaAmplifier03", 501),
        (540, "FS01_CheckpointRaleighAmplifier04", 500),
        (550, "FS01_CheckpointRelayAmplifier05", 502),
    ):
        body = _member(patch, f"Fragment_Stage_{stage:04d}_Item_00")
        assert checkpoint in body
        assert f"SetObjectiveCompleted({objective}, True)" in body
        assert "FS01_CheckAmplifiersComplete()" in body

    abbie_1 = _member(patch, "Fragment_Stage_0510_Item_00")
    abbie_2 = _member(patch, "Fragment_Stage_0520_Item_00")
    assert "FS01_CheckpointAbbieAmplifier01" in abbie_1
    assert "FS01_CheckpointAbbieAmplifier02" in abbie_2
    assert "FS01_UpdateAbbieHeatingCoils()" in abbie_1
    assert "FS01_UpdateAbbieHeatingCoils()" in abbie_2

    all_amplifiers = _member(patch, "FS01_CheckAmplifiersComplete")
    for stage in (510, 520, 530, 540, 550):
        assert f"GetStageDone({stage})" in all_amplifiers
    assert "!GetStageDone(560)" in all_amplifiers


def test_fs01_intermediate_rewards_are_mapper_owned_stage_triggers() -> None:
    patch = _patch()

    assert "RewardPlayerXP" not in patch
    assert "capsItem" not in patch
    assert "0x0039F140" not in patch
    assert "0x00051616" not in patch

    stage_501 = _member(patch, "FS01_EnterAmplifierSearch")
    assert "!GetStageDone(501)" in stage_501
    assert "SetStage(501)" in stage_501

    stage_601 = _member(patch, "Fragment_Stage_0600_Item_00")
    assert "!GetStageDone(601)" in stage_601
    assert "SetStage(601)" in stage_601


def test_fs01_crafting_gates_have_bounded_single_player_substitutes() -> None:
    patch = _patch()

    repair_uplink = _member(patch, "Fragment_Stage_0150_Item_00")
    upgraded_motors = _member(patch, "Fragment_Stage_0600_Item_00")

    assert repair_uplink.index("SetObjectiveDisplayed(150") < repair_uplink.index(
        "SetStage(200)"
    )
    assert upgraded_motors.index("SetStage(601)") < upgraded_motors.index(
        "SetStage(700)"
    )
    assert "!GetStageDone(700)" in upgraded_motors
    assert upgraded_motors.count("SetStage(700)") == 1
    assert "LearnRecipe" not in patch
    assert "TeachRecipe" not in patch


def test_fs01_player_alias_reconciles_existing_saves_at_crafting_gates() -> None:
    patch = _script_patch_source(PLAYER_SCRIPT)
    assert patch is not None
    members = _members(patch)

    assert Counter(members) == Counter(
        {"applynocraftingprogress", "onaliasinit", "onplayerloadgame"}
    )
    reconcile = _member(patch, "ApplyNoCraftingProgress")
    assert "GetOwningQuest()" in reconcile
    assert "currentStage == 150" in reconcile
    assert reconcile.count("owningQuest.SetStage(200)") == 1
    assert "currentStage >= 600 && currentStage < 700" in reconcile
    assert reconcile.count("owningQuest.SetStage(700)") == 1
    assert "owningQuest.GetStageDone(1000)" in reconcile
    assert "ApplyNoCraftingProgress()" in _member(patch, "OnAliasInit")
    assert "ApplyNoCraftingProgress()" in _member(patch, "OnPlayerLoadGame")


def test_fs01_player_alias_full_production_merge_compiles(tmp_path: Path) -> None:
    base_source = _fo4_base_source()
    assert base_source is not None, "FO4 base Papyrus sources unavailable"

    merged = _merged_script(PLAYER_SCRIPT)
    patch = _script_patch_source(PLAYER_SCRIPT)
    assert patch is not None
    assert _merge_script_method_patches(merged, patch) == merged

    result = compile_psc(
        merged,
        imports=[str(tmp_path), str(SOURCE_ROOT), str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=str(_script_relative_path(PLAYER_SCRIPT, ".psc")),
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None


def test_fs01_handoff_waits_for_fs02_to_confirm_start() -> None:
    patch = _patch()
    handoff = _member(patch, "Fragment_Stage_0800_Item_00")

    assert 'Game.GetFormFromFile(0x004E0719, "SeventySix.esm") as Quest' in handoff
    assert "FS02_MQ_Overdue_QuestStartKeyword.SendStoryEventAndWait(" in handoff
    assert "SetStage(1000)" not in handoff
    assert "CompleteQuest()" not in handoff

    completion = _member(patch, "Fragment_Stage_1000_Item_00")
    assert "FS01_Warn_QuestCompletedValue" in completion
    assert completion.index("CompleteAllObjectives()") < completion.index(
        "CompleteQuest()"
    )
    assert completion.index("CompleteQuest()") < completion.index("Stop()")
    assert "FS02_MQ_Overdue_QuestStartKeyword" not in completion


def test_fs01_full_production_merge_compiles(tmp_path: Path) -> None:
    base_source = _fo4_base_source()
    assert base_source is not None, "FO4 base Papyrus sources unavailable"

    result = compile_psc(
        _merged(),
        imports=[str(tmp_path), str(SOURCE_ROOT), str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=str(_script_relative_path(FRAGMENT_SCRIPT, ".psc")),
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None
