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

FRAGMENT_SCRIPT = "Fragments:Quests:QF_MTNS01_Intro_00031163"
QUEST_SCRIPT = "MTNS01QuestScript"
PLAYER_SCRIPT = "mtns01_playerscript"
MASTER_SCRIPT = "MTNS01MasterQuestScript"
CHECKPOINT_SCRIPT = "DefaultCheckpointingScript"

FRAGMENT_STAGES = (
    50,
    100,
    200,
    201,
    205,
    206,
    210,
    211,
    213,
    220,
    225,
    300,
    400,
    401,
    410,
    420,
    500,
    600,
)

PATCH_MEMBERS = {
    FRAGMENT_SCRIPT: {
        *(f"fragment_stage_{stage:04d}_item_00" for stage in FRAGMENT_STAGES),
        "mtns01_getplayer",
        "mtns01_setcheckpoint",
        "mtns01_getcontroller",
    },
    QUEST_SCRIPT: {
        "mtns01_getplayer",
        "mtns01_movequestitem",
        "mtns01_trackradiorepeater",
        "mtns01_reconcileprogress",
        "onquestinit",
        "onquestshutdown",
    },
    PLAYER_SCRIPT: {
        "onaliasinit",
        "onitemadded",
        "onplayerloadgame",
        "onaliasshutdown",
    },
    MASTER_SCRIPT: {
        "mtns01_sendquestevent",
        "mtns01_startintro",
        "mtns01_startmayhem",
        "mtns01_startraiders",
        "mtns01_startmissinglink",
    },
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
        if name == member_name.casefold()
    )
    return "\n".join(lines[start : end + 1])


def _merged_source(script_name: str) -> str:
    source_path = SOURCE_ROOT / _script_relative_path(script_name, ".psc")
    patch = _script_patch_source(script_name)
    assert source_path.is_file(), source_path
    assert patch is not None, script_name
    return _merge_script_method_patches(source_path.read_text(encoding="utf-8"), patch)


@pytest.mark.parametrize(("script_name", "members"), PATCH_MEMBERS.items())
def test_mtns01_patches_are_member_only_and_exact(
    script_name: str, members: set[str]
) -> None:
    patch = _script_patch_source(script_name)

    assert patch is not None
    names = _member_names(patch)
    assert Counter(names) == Counter(members)
    assert not any(
        line.strip().casefold().startswith(("scriptname ", "state ", "extends "))
        for line in patch.splitlines()
    )
    assert not any(
        " property " in f" {line.strip().casefold()} " for line in patch.splitlines()
    )


def test_mtns01_fragment_covers_exact_qust_vmad_stage_members() -> None:
    patch = _script_patch_source(FRAGMENT_SCRIPT)
    assert patch is not None

    stage_members = {
        name
        for kind, name, _start, _end in _iter_top_level_papyrus_members(
            patch.splitlines()
        )
        if kind == "function" and name.startswith("fragment_stage_")
    }
    expected = {f"fragment_stage_{stage:04d}_item_00" for stage in FRAGMENT_STAGES}

    assert stage_members == expected
    assert "fragment_stage_0700_item_00" not in stage_members


@pytest.mark.parametrize(("script_name", "members"), PATCH_MEMBERS.items())
def test_mtns01_production_merges_are_unique_and_idempotent(
    script_name: str, members: set[str]
) -> None:
    patch = _script_patch_source(script_name)
    assert patch is not None
    merged = _merged_source(script_name)
    merged_members = _member_names(merged)

    for member in members:
        assert merged_members.count(member) == 1
        assert _member_body(merged, member) == _member_body(patch, member)
    assert _merge_script_method_patches(merged, patch) == merged


def test_mtns01_fragments_restore_items_objectives_and_checkpoints() -> None:
    patch = _script_patch_source(FRAGMENT_SCRIPT)
    assert patch is not None

    schematic = _member_body(patch, "fragment_stage_0200_item_00")
    assert "SetObjectiveDisplayed(200)" in schematic
    assert "MTNS01_MoveQuestItem(Schematics, Alias_Corpse)" in schematic
    assert "MTNS01_SetCheckpoint(1)" in schematic

    note = _member_body(patch, "fragment_stage_0205_item_00")
    assert "MTNS01_MoveQuestItem(Alias_RespondersNote, Alias_Corpse)" in note

    parts = _member_body(patch, "fragment_stage_0210_item_00")
    assert "MTNS01_MoveQuestItem(Alias_RadioParts01, Alias_Container01)" in parts
    assert "MTNS01_MoveQuestItem(Alias_RadioParts02, Alias_Container02)" in parts
    assert "SetObjectiveDisplayed(211)" in parts
    assert "SetObjectiveDisplayed(212)" in parts
    assert "SetObjectiveDisplayed(213)" in parts

    for stage, counterparts in ((211, (213, 225)), (213, (211, 220))):
        body = _member_body(patch, f"fragment_stage_{stage:04d}_item_00")
        for counterpart in counterparts:
            assert f"IsStageDone({counterpart})" in body
        assert "SetStage(300)" in body

    restored_part01 = _member_body(patch, "fragment_stage_0225_item_00")
    restored_part02 = _member_body(patch, "fragment_stage_0220_item_00")
    assert "MTNS01_MoveQuestItem(Alias_RadioParts01, Alias_Container01)" in restored_part01
    assert "MTNS01_MoveQuestItem(Alias_RadioParts02, Alias_Container02)" in restored_part02

    checkpoints = {
        200: 1,
        210: 2,
        220: 3,
        225: 4,
        300: 5,
        400: 6,
        500: 7,
    }
    for stage, checkpoint in checkpoints.items():
        body = _member_body(patch, f"fragment_stage_{stage:04d}_item_00")
        assert f"MTNS01_SetCheckpoint({checkpoint})" in body


def test_mtns01_install_boost_completion_and_reward_stage_flow() -> None:
    patch = _script_patch_source(FRAGMENT_SCRIPT)
    assert patch is not None

    install = _member_body(patch, "fragment_stage_0410_item_00")
    assert "PlayerRepeaterInstalled.ForceRefTo(playerRef)" in install
    assert "QSTMTNS01RepeaterInstall.Play(soundMarker)" in install
    assert "SetObjectiveDisplayed(420)" in install

    boost = _member_body(patch, "fragment_stage_0420_item_00")
    assert "PlayerSignalBoosted.ForceRefTo(playerRef)" in boost
    assert "SetStage(500)" in boost

    for reward_stage in (210, 400, 500, 600):
        assert f"fragment_stage_{reward_stage:04d}_item_00" in _member_names(patch)

    complete = _member_body(patch, "fragment_stage_0600_item_00")
    assert "CompleteAllObjectives()" in complete
    assert (
        "MTNM01_Mayhem_Quest_Keyword.SendStoryEventAndWait(None, playerRef, playerRef)"
        in complete
    )
    assert complete.index("CompleteAllObjectives()") < complete.index(
        "SendStoryEventAndWait"
    )
    assert complete.index("SendStoryEventAndWait") < complete.index("Stop()")


def test_mtns01_controller_and_player_reconcile_crafted_repeater() -> None:
    controller = _script_patch_source(QUEST_SCRIPT)
    player = _script_patch_source(PLAYER_SCRIPT)
    assert controller is not None
    assert player is not None

    move = _member_body(controller, "mtns01_movequestitem")
    assert "MoveQuestItemSpinLock" in move
    assert "itemRef.MoveTo(destinationRef)" in move
    assert move.index("MoveQuestItemSpinLock = True") < move.index("itemRef.MoveTo")
    assert move.index("itemRef.MoveTo") < move.index("MoveQuestItemSpinLock = False")

    reconcile = _member_body(controller, "mtns01_reconcileprogress")
    assert "playerRef.GetItemCount(MTNS01_RadioRepeater) > 0" in reconcile
    assert "SetStage(ArrayStage)" in reconcile
    assert "playerRef.GetItemCount(part01.GetBaseObject()) > 0" in reconcile
    assert "playerRef.GetItemCount(part02.GetBaseObject()) > 0" in reconcile
    assert "SetStage(CraftStage)" in reconcile

    init = _member_body(controller, "onquestinit")
    assert init.index("Parent.OnQuestInit()") < init.index("MTNS01_ReconcileProgress()")

    alias_init = _member_body(player, "onaliasinit")
    assert "AddInventoryEventFilter(MTNS01_RadioRepeater_Keyword)" in alias_init
    item_added = _member_body(player, "onitemadded")
    assert "akBaseItem.HasKeyword(MTNS01_RadioRepeater_Keyword)" in item_added
    assert "RadioRepeater.ForceRefTo(akItemReference)" in item_added
    assert "controller.MTNS01_TrackRadioRepeater(akItemReference)" in item_added


def test_mtns01_master_routes_each_source_start_keyword() -> None:
    patch = _script_patch_source(MASTER_SCRIPT)
    assert patch is not None

    sender = _member_body(patch, "mtns01_sendquestevent")
    assert "akStartKeyword.SendStoryEventAndWait(None, playerRef, playerRef)" in sender

    for member, keyword in (
        ("mtns01_startintro", "MTNS01_Intro_Quest_Keyword"),
        ("mtns01_startmayhem", "MTNM01_Mayhem_Quest_Keyword"),
        ("mtns01_startraiders", "MTNL01_Raiders_Quest_Keyword"),
        ("mtns01_startmissinglink", "MTN_MQ_Missing_Quest_Keyword"),
    ):
        assert f"MTNS01_SendQuestEvent({keyword})" in _member_body(patch, member)


@pytest.mark.parametrize("script_name", PATCH_MEMBERS)
def test_mtns01_production_merges_compile(script_name: str, tmp_path: Path) -> None:
    base_source = _fo4_base_source()
    assert base_source is not None, "FO4 base Papyrus sources unavailable"

    dependencies = (*PATCH_MEMBERS, CHECKPOINT_SCRIPT)
    for dependency_name in dependencies:
        dependency_path = tmp_path / _script_relative_path(dependency_name, ".psc")
        dependency_path.parent.mkdir(parents=True, exist_ok=True)
        dependency_path.write_text(_merged_source(dependency_name), encoding="utf-8")

    merged = _merged_source(script_name)
    result = compile_psc(
        merged,
        imports=[str(tmp_path), str(SOURCE_ROOT), str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=str(_script_relative_path(script_name, ".psc")),
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None
