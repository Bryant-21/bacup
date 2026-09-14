from __future__ import annotations

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
SCRIPT_NAME = "Fragments:Quests:QF_ReclamationDay_000D4D34"

# The seven members in the record's own fragment table (QUST 000D4D34, FragmentCount 7).
BOUND_MEMBERS = {
    "fragment_stage_0010_item_00",
    "fragment_stage_0012_item_00",
    "fragment_stage_0013_item_00",
    "fragment_stage_0015_item_00",
    "fragment_stage_0020_item_00",
    "fragment_stage_0100_item_00",
    "fragment_stage_0999_item_00",
}

# Bound properties that carry FO76-only services with no Fallout 4 equivalent, so no
# member may consume them: the Atomic Shop icon tutorial, the character level boost and
# the loadout picker. AldertonSuppliesScene belongs to stage 17, which has no bound
# fragment, so nothing may start it either.
UNSUPPORTED_PROPERTIES = (
    "NPE_ATXIcons_Message",
    "NPE_BoostLevel",
    "NPE_LoadoutsEnabled",
    "PlayerLevel",
    "AldertonSuppliesScene",
)


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


def _merged_source() -> str:
    source_path = SOURCE_ROOT / _script_relative_path(SCRIPT_NAME, ".psc")
    patch = _script_patch_source(SCRIPT_NAME)
    assert source_path.is_file(), source_path
    assert patch is not None
    return _merge_script_method_patches(source_path.read_text(encoding="utf-8"), patch)


def test_patch_supplies_every_bound_member_once_and_declares_nothing() -> None:
    patch = _script_patch_source(SCRIPT_NAME)
    assert patch is not None

    names = _member_names(patch)
    assert set(names) == BOUND_MEMBERS
    for name in BOUND_MEMBERS:
        assert names.count(name) == 1
    assert not any(
        line.strip().casefold().startswith(("scriptname ", "state "))
        for line in patch.splitlines()
    )
    assert not any(
        " property " in f" {line.strip().casefold()} " for line in patch.splitlines()
    )


def test_merge_is_idempotent_and_the_full_source_compiles() -> None:
    patch = _script_patch_source(SCRIPT_NAME)
    assert patch is not None
    merged = _merged_source()

    merged_names = _member_names(merged)
    for name in BOUND_MEMBERS:
        assert merged_names.count(name) == 1
    assert merged.casefold().count("scriptname ") == 1
    assert "scene Property AldertonScene Auto mandatory" in merged
    assert _merge_script_method_patches(merged, patch) == merged

    base_source = _fo4_base_source()
    assert base_source is not None, "FO4 base Papyrus sources unavailable"
    result = compile_psc(
        merged,
        imports=[str(SOURCE_ROOT), str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=str(_script_relative_path(SCRIPT_NAME, ".psc")),
    )
    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None


def test_objective_flow_follows_the_record_stage_notes() -> None:
    patch = _script_patch_source(SCRIPT_NAME)
    assert patch is not None

    # Stage 10 is RunOnStart ("Quest Started"); objective 10 "Leave Vault 76" is the
    # spine and targets alias 4, the breadcrumb ReclamationDay_QTTriggersScript moves.
    assert _member_body(patch, "fragment_stage_0010_item_00").count(
        "SetObjectiveDisplayed(10)"
    ) == 1

    # Stage 13 is "Pop objective to Overseer office"; objective 15 is the only objective
    # that targets alias 12 OverseerOfficeTerminal.
    stage_13 = _member_body(patch, "fragment_stage_0013_item_00")
    assert stage_13.index("!IsObjectiveCompleted(15)") < stage_13.index(
        "SetObjectiveDisplayed(15)"
    )

    # Stage 20 is "Activated Overseer Terminal": objective 15 closes, and objective 20
    # only opens while the holotape has not already been taken or played (stage 12).
    stage_20 = _member_body(patch, "fragment_stage_0020_item_00")
    assert stage_20.index("SetObjectiveCompleted(15)") < stage_20.index(
        "!IsStageDone(12)"
    )
    assert stage_20.count("SetObjectiveDisplayed(20)") == 1

    # Stage 12 is the Checkpoint stage, produced by both DefaultAliasOnPlayerHolotape and
    # DefaultAliasInventoryManagement on alias 0, so it must be safe when objectives 15
    # and 20 were never displayed.
    stage_12 = _member_body(patch, "fragment_stage_0012_item_00")
    assert stage_12.count("IsObjectiveDisplayed(15)") == 1
    assert stage_12.count("IsObjectiveDisplayed(20)") == 1
    assert "!IsObjectiveCompleted(10)" in stage_12


def test_exit_trigger_reaches_the_only_completion_stage_and_hands_off_once() -> None:
    patch = _script_patch_source(SCRIPT_NAME)
    assert patch is not None

    # Alias 15 ExitVaultTrigger is the quest's last producer and stage 999 is its only
    # CompleteQuest stage, with no other producer anywhere in the converted data.
    stage_100 = _member_body(patch, "fragment_stage_0100_item_00")
    assert "W05_MQ_001P_Wayward_QuestStartKeyword.SendStoryEventAndWait(" in stage_100
    assert stage_100.index("SendStoryEventAndWait") < stage_100.index("SetStage(999)")
    assert "!IsStageDone(999)" in stage_100

    stage_999 = _member_body(patch, "fragment_stage_0999_item_00")
    for keyword in (
        "RS01A_Contact_Keyword",
        "W05_MQ_101P_QuestStartKeyword",
        "BS01_MQ00_Breadcrumb_QuestStartKeyword",
    ):
        assert f"{keyword}.SendStoryEvent(None, playerRef, playerRef)" in stage_999
    # Stage 999 shuts the quest down, so no latent story event may run inside it.
    assert "SendStoryEventAndWait" not in stage_999
    assert "CompleteQuest()" not in stage_999
    assert ".Start()" not in stage_999


def test_scene_and_unsupported_service_properties_are_handled_explicitly() -> None:
    patch = _script_patch_source(SCRIPT_NAME)
    assert patch is not None

    # Stage 15 is "Close to Alderton", produced by DefaultAliasOnDistanceLessThan at 500
    # units against alias 17; AldertonScene 0050DE55 exists as a SCEN in the converted
    # plugin, so the scene start is real and must not restart a playing scene.
    stage_15 = _member_body(patch, "fragment_stage_0015_item_00")
    assert stage_15.index("!AldertonScene.IsPlaying()") < stage_15.index(
        "AldertonScene.Start()"
    )

    for property_name in UNSUPPORTED_PROPERTIES:
        assert property_name not in patch
