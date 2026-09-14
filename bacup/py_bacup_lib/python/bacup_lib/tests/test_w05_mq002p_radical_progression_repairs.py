"""Regression cover for the W05_MQ_002P_Radical (40F5BE) progression chain.

Each assertion below is tied to a record fact, not to taste:

* stage 110 must put the schematic in the player's inventory, because objective 110
  reads "...from your inventory" while alias SignPlans is created at the
  SchematicSpawn marker, and the alias' OnRead handler is what sets stage 200.
* stage 200 must set PlayerLearnedSignRecipe, because COBJ
  W05_MQ_002P_Radical_workshop_co_CraneRadioTransmitter (40F66C) carries
  ``GetValue(W05_MQ_002P_Radical_PlayerLearnedSignRecipe) == 1.0``. Without it the
  sign can never be built, so W05_MQ_002P_CraneSignScript.OnWorkshopObjectPlaced
  never fires and stages 270/400 are unreachable.
* stage 400 must hand over the broadcast tape, because objective 350 requires it and
  stage 460 already removes it from the player's inventory.
"""

from __future__ import annotations

import json
from pathlib import Path

import pytest

from bacup_lib.workflows.unified import (
    _iter_top_level_papyrus_members,
    _merge_script_method_patches,
    _script_patch_source,
)
from creation_lib.pex import decompile_pex
from creation_lib.pex.native_runtime import compile_psc

from bacup_lib.tests.test_terminal_fragment_script_patches import _fo4_base_source

REPO_ROOT = Path(__file__).resolve().parents[5]
DEPLOYED_SCRIPTS_ROOT = REPO_ROOT / "mods" / "SeventySix" / "data" / "Scripts"

RADICAL_QUEST_FRAGMENT = "Fragments:Quests:QF_W05_MQ_002P_Radical_0040F5BE"
RADICAL_CONTROLLER = "W05_002P_Radical_QuestScript"
STAGE_FLAGS_FIXTURE = (
    REPO_ROOT
    / "bacup"
    / "py_bacup_lib"
    / "python"
    / "bacup_lib"
    / "tests"
    / "fixtures"
    / "w05_mq002_stage_flags.json"
)

# Every stage below is declared by the QUST VMAD fragment table (FragmentCount 67).
PROGRESSION_SNIPPETS = {
    "Fragment_Stage_0110_Item_00": (
        "Alias_SignPlans.GetReference()",
        "playerRef.AddItem(plansRef, 1, True)",
        "W05_MQ_002P_Radical_Recipe_Workshop_CraneRadioTransmitter",
        "SetObjectiveDisplayed(110)",
    ),
    "Fragment_Stage_0125_Item_00": (
        "playerRef.SetValue(W05_MQ_002P_Radical_DirectPlayerToAskAboutCamps, 1.0)",
        "SetObjectiveDisplayed(125)",
    ),
    "Fragment_Stage_0130_Item_00": (
        "GorgeJunkyardMapMarker.AddToMap(True)",
        "SetObjectiveDisplayed(130)",
    ),
    "Fragment_Stage_0200_Item_00": (
        "playerRef.SetValue(W05_MQ_002P_Radical_PlayerLearnedSignRecipe, 1.0)",
        "SetObjectiveCompleted(110)",
        "SetObjectiveDisplayed(200)",
    ),
    "Fragment_Stage_0400_Item_00": (
        "Alias_ConnectionTape.GetReference()",
        "playerRef.AddItem(tapeRef, 1, True)",
        "W05_MQ_002P_Radical_0400_PlayerPowerSignForFirstTime.Start()",
        "SetObjectiveCompleted(125)",
        "SetObjectiveDisplayed(350)",
    ),
    "Fragment_Stage_0450_Item_00": (
        "SetObjectiveCompleted(350)",
        "SetObjectiveCompleted(400)",
    ),
}


def _member_body(source: str, member_name: str) -> str:
    start, end = next(
        (start, end)
        for kind, name, start, end in _iter_top_level_papyrus_members(source.splitlines())
        if kind in {"function", "event"} and name == member_name.lower()
    )
    return "\n".join(source.splitlines()[start : end + 1])


@pytest.mark.parametrize(("member", "snippets"), PROGRESSION_SNIPPETS.items())
def test_radical_progression_member_has_evidence_backed_body(
    member: str, snippets: tuple[str, ...]
):
    patch = _script_patch_source(RADICAL_QUEST_FRAGMENT)
    assert patch is not None

    body = _member_body(patch, member)
    for snippet in snippets:
        assert snippet in body, f"{member} lost: {snippet}"


def test_inventory_grants_are_guarded_against_regranting():
    """AddItem must not re-run when the ref already sits in the player's inventory."""
    patch = _script_patch_source(RADICAL_QUEST_FRAGMENT)
    assert patch is not None

    for member in ("Fragment_Stage_0110_Item_00", "Fragment_Stage_0400_Item_00"):
        body = _member_body(patch, member)
        assert "GetContainer() != playerObj" in body, member


def test_radical_fragment_declares_no_new_properties_or_variables():
    """The merger owns declarations; the patch may only contribute member bodies."""
    patch = _script_patch_source(RADICAL_QUEST_FRAGMENT)
    assert patch is not None

    for line in patch.splitlines():
        stripped = line.strip()
        assert not stripped.startswith("Scriptname "), stripped
        assert " Property " not in f" {stripped} ", stripped


def test_final_duchess_stage_completes_radical_without_waiting_for_muscle():
    """Stage 9000 fires the B21 reward listener even when the SM start is deferred."""
    patch = _script_patch_source(RADICAL_QUEST_FRAGMENT)
    assert patch is not None

    body = _member_body(patch, "Fragment_Stage_8950_Item_00")
    assert "If !IsStageDone(9000)" in body
    assert "SetStage(9000)" in body
    assert "SendStoryEventAndWait" not in body


def test_radical_controller_retries_muscle_without_replaying_completion():
    """Only the controller retries the event; stage 9000 remains a one-shot fragment."""
    patch = _script_patch_source(RADICAL_CONTROLLER)
    assert patch is not None

    try_start = _member_body(patch, "TryStartMuscleQuest")
    on_stage_set = _member_body(patch, "OnStageSet")
    on_timer = _member_body(patch, "OnTimer")
    target_state = "muscleQuest.IsRunning() || muscleQuest.IsCompleted()"
    story_send = "startKeyword.SendStoryEventAndWait(None, playerRef, playerRef)"

    for snippet in (
        'Game.GetFormFromFile(0x0041A39D, "SeventySix.esm") as Quest',
        target_state,
        "Actor playerRef = owningPlayer.GetActorReference()",
        "If playerRef == None || playerRef.IsInScene()",
        'Game.GetFormFromFile(0x0041A340, "SeventySix.esm") as Keyword',
        story_send,
        "StartTimer(1.0, 8950)",
    ):
        assert snippet in try_start

    assert try_start.count(target_state) == 2
    assert try_start.index(f"If muscleQuest == None || {target_state}") < try_start.index(
        story_send
    )
    assert try_start.index(story_send) < try_start.index(f"If {target_state}")
    assert f"= {story_send}" not in try_start
    assert f"&& {story_send}" not in try_start

    assert "auiStageID == 8950" in on_stage_set
    assert "TryStartMuscleQuest()" in on_stage_set
    assert "aiTimerID == 8950" in on_timer
    assert "TryStartMuscleQuest()" in on_timer
    assert "SetStage(9000)" not in try_start


def test_radical_controller_waits_until_player_leaves_all_scenes():
    """The Duchess wrap-up scene must finish before Muscle's intro can start."""
    patch = _script_patch_source(RADICAL_CONTROLLER)
    assert patch is not None

    try_start = _member_body(patch, "TryStartMuscleQuest")
    scene_guard = "If playerRef == None || playerRef.IsInScene()"
    story_send = "startKeyword.SendStoryEventAndWait(None, playerRef, playerRef)"

    guard_start = try_start.index(scene_guard)
    guard_end = try_start.index("EndIf", guard_start)
    send_start = try_start.index(story_send)
    guarded_path = try_start[guard_start:guard_end]

    assert guard_start < guard_end < send_start
    assert "StartTimer(1.0, 8950)" in guarded_path
    assert "Return" in guarded_path
    assert "SendStoryEventAndWait" not in guarded_path


def test_radical_controller_trusts_muscle_state_not_shared_selector_result():
    """A shared selector may accept the event without selecting Muscle."""
    patch = _script_patch_source(RADICAL_CONTROLLER)
    assert patch is not None

    try_start = _member_body(patch, "TryStartMuscleQuest")
    target_state = "muscleQuest.IsRunning() || muscleQuest.IsCompleted()"
    story_send = "startKeyword.SendStoryEventAndWait(None, playerRef, playerRef)"

    assert try_start.count(target_state) == 2
    assert try_start.index(target_state) < try_start.index(story_send)
    assert try_start.index(story_send) < try_start.rindex(target_state)
    assert try_start.rindex(target_state) < try_start.rindex("StartTimer(1.0, 8950)")
    assert "If startKeyword != None &&" not in try_start


def test_radical_completion_keeps_the_successor_retry_timer_alive():
    evidence = json.loads(STAGE_FLAGS_FIXTURE.read_text(encoding="utf-8"))
    expected_flags = {
        "8950": [],
        "9000": ["CompleteQuest"],
        "10000": [],
    }
    assert evidence["source"]["editor_id"] == "W05_MQ_002P_Radical"
    assert evidence["live"]["editor_id"] == "W05_MQ_002P_Radical"
    assert evidence["source"]["stages"] == expected_flags
    assert evidence["live"]["stages"] == expected_flags
    assert all(
        "ShutDown" not in flags
        for record in evidence.values()
        for flags in record["stages"].values()
    )

    quest_patch = _script_patch_source(RADICAL_QUEST_FRAGMENT)
    controller_patch = _script_patch_source(RADICAL_CONTROLLER)
    assert quest_patch is not None
    assert controller_patch is not None
    retry_members = [
        _member_body(quest_patch, "Fragment_Stage_8950_Item_00"),
        _member_body(quest_patch, "Fragment_Stage_9000_Item_00"),
        _member_body(controller_patch, "TryStartMuscleQuest"),
        _member_body(controller_patch, "OnTimer"),
    ]
    assert "SetStage(9000)" in retry_members[0]
    assert "CompleteQuest()" in retry_members[1]
    assert "StartTimer(1.0, 8950)" in retry_members[2]
    assert "aiTimerID == 8950" in retry_members[3]
    assert "TryStartMuscleQuest()" in retry_members[3]
    assert all("Stop()" not in member for member in retry_members)


def test_radical_fragment_merged_patch_compiles_for_fo4():
    base_source = _fo4_base_source()
    if base_source is None:
        pytest.skip("FO4 base Papyrus sources unavailable")

    pex_path = DEPLOYED_SCRIPTS_ROOT / "fragments" / "quests" / (
        "qf_w05_mq_002p_radical_0040f5be.pex"
    )
    if not pex_path.is_file():
        pytest.skip(f"deployed production PEX unavailable: {pex_path}")

    skeleton = decompile_pex(pex_path, fo4_api_compat=True)
    patch = _script_patch_source(RADICAL_QUEST_FRAGMENT)
    assert patch is not None

    result = compile_psc(
        _merge_script_method_patches(skeleton, patch),
        imports=[str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path="QF_W05_MQ_002P_Radical_0040F5BE.psc",
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None


def test_radical_controller_merged_patch_compiles_for_fo4():
    base_source = _fo4_base_source()
    if base_source is None:
        pytest.skip("FO4 base Papyrus sources unavailable")

    pex_path = DEPLOYED_SCRIPTS_ROOT / "W05_002P_Radical_QuestScript.pex"
    if not pex_path.is_file():
        pytest.skip(f"deployed production PEX unavailable: {pex_path}")

    skeleton = decompile_pex(pex_path, fo4_api_compat=True)
    patch = _script_patch_source(RADICAL_CONTROLLER)
    assert patch is not None

    merged = _merge_script_method_patches(skeleton, patch)
    assert sum(
        1
        for kind, name, *_ in _iter_top_level_papyrus_members(merged.splitlines())
        if kind == "function" and name == "trystartmusclequest"
    ) == 1

    result = compile_psc(
        merged,
        imports=[str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path="W05_002P_Radical_QuestScript.psc",
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None
