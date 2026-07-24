from __future__ import annotations

from collections import Counter
from pathlib import Path

import pytest

from bacup_lib.tests.test_terminal_fragment_script_patches import _fo4_base_source
from bacup_lib.workflows.unified import (
    _iter_papyrus_states,
    _iter_top_level_papyrus_members,
    _merge_script_method_patches,
    _script_patch_source,
    _script_relative_path,
)
from creation_lib.pex import decompile_pex
from creation_lib.pex.native_runtime import compile_psc


REPO_ROOT = Path(__file__).resolve().parents[5]
DEPLOYED_SCRIPTS_ROOT = REPO_ROOT / "mods" / "SeventySix" / "data" / "Scripts"
GENERATED_SOURCE_ROOT = REPO_ROOT / "mods" / "SeventySix" / "Scripts" / "Source" / "User"

CHASE_SCRIPT = "W05_MQA_206_ChaseDistanceLessThan"
TRIGGER_SCRIPT = "W05_MQA_206P_SSTalkTriggerBoxScript"
QUEST_FRAGMENT = "Fragments:Quests:QF_W05_MQA_206P_0054EDB9"

OBJECTIVE_STAGES = (
    100,
    105,
    200,
    250,
    300,
    400,
    450,
    500,
    525,
    550,
    575,
    600,
    700,
    800,
    5200,
    5220,
)
PROGRESSION_STAGES = (30, 33, 50, 150, 585, 590, 5000, 5050)
EXPANDED_OBJECTIVE_STAGES = {200, 700, 800}
UNRESOLVED_STAGES = (
    4,
    5,
    6,
    20,
    21,
    22,
    35,
    60,
    70,
    75,
    80,
    81,
    82,
    83,
    85,
    90,
    91,
    92,
    93,
    94,
    95,
    96,
    97,
    98,
    460,
    471,
    472,
    473,
    474,
    475,
    476,
    477,
    478,
    479,
    481,
    5010,
    5011,
    5012,
    5013,
    5205,
    5210,
    5230,
    5240,
    5250,
    5275,
    5300,
    5400,
    5500,
    9000,
)

PATCH_CASES = {
    CHASE_SCRIPT: {"onaliasinit", "quest.onstageset", "ondistancelessthan"},
    TRIGGER_SCRIPT: {"ontriggerenter"},
    QUEST_FRAGMENT: {
        *(f"fragment_stage_{stage:04d}_item_00" for stage in OBJECTIVE_STAGES),
        *(f"fragment_stage_{stage:04d}_item_00" for stage in PROGRESSION_STAGES),
        "fragment_stage_9999_item_00",
    },
}


def _member_name_list(source: str) -> list[str]:
    return [
        name
        for kind, name, _start, _end in _iter_top_level_papyrus_members(
            source.splitlines()
        )
        if kind in {"function", "event"}
    ]


def _member_names(source: str) -> set[str]:
    return set(_member_name_list(source))


def _member_body(source: str, member_name: str) -> str:
    start, end = next(
        (start, end)
        for kind, name, start, end in _iter_top_level_papyrus_members(
            source.splitlines()
        )
        if kind in {"function", "event"} and name == member_name.lower()
    )
    return "\n".join(source.splitlines()[start : end + 1])


def _production_skeleton(base_name: str) -> str:
    pex_path = DEPLOYED_SCRIPTS_ROOT / _script_relative_path(base_name, ".pex")
    assert pex_path.is_file(), f"deployed production PEX unavailable: {pex_path}"
    return decompile_pex(pex_path, fo4_api_compat=True)


def _merged_production_source(base_name: str) -> str:
    patch = _script_patch_source(base_name)
    assert patch is not None
    return _merge_script_method_patches(_production_skeleton(base_name), patch)


@pytest.mark.parametrize(("base_name", "expected_members"), PATCH_CASES.items())
def test_mqa_patch_has_exact_member_allowlist(
    base_name: str, expected_members: set[str]
):
    patch = _script_patch_source(base_name)
    assert patch is not None
    assert not any(
        line.strip().lower().startswith("scriptname ") for line in patch.splitlines()
    )
    assert _iter_papyrus_states(patch.splitlines()) == []
    assert _member_names(patch) == expected_members
    assert Counter(_member_name_list(patch)) == Counter(
        {name: 1 for name in expected_members}
    )


def test_mqa_fragment_preserves_verified_objective_displays_and_stop():
    patch = _script_patch_source(QUEST_FRAGMENT)
    assert patch is not None

    for stage in OBJECTIVE_STAGES:
        member_name = f"fragment_stage_{stage:04d}_item_00"
        body = _member_body(patch, member_name)
        if stage in EXPANDED_OBJECTIVE_STAGES:
            assert body.startswith(f"Function Fragment_Stage_{stage:04d}_Item_00()\n")
            assert body.count(f"SetObjectiveDisplayed({stage})") == 1
        else:
            assert body == (
                f"Function Fragment_Stage_{stage:04d}_Item_00()\n"
                f"    SetObjectiveDisplayed({stage})\n"
                "EndFunction"
            )

    assert _member_body(patch, "fragment_stage_9999_item_00") == (
        "Function Fragment_Stage_9999_Item_00()\n"
        "    Stop()\n"
        "EndFunction"
    )


def test_mqa_fragment_keeps_all_unresolved_members_bodyless():
    patch = _script_patch_source(QUEST_FRAGMENT)
    assert patch is not None
    unresolved_members = {
        f"fragment_stage_{stage:04d}_item_00" for stage in UNRESOLVED_STAGES
    }
    assert len(unresolved_members) == 49
    assert _member_names(patch).isdisjoint(unresolved_members)


def test_mqsettlers_quest_is_excluded_from_the_mqa_patch_manifest():
    assert "Fragments:Quests:QF_W05_MQSettlers_201P_Indus_003F28C3" not in PATCH_CASES


def test_chase_registers_only_the_verified_pair_and_stage_gate():
    patch = _script_patch_source(CHASE_SCRIPT)
    assert patch is not None
    on_init = _member_body(patch, "onaliasinit")
    on_stage = _member_body(patch, "quest.onstageset")
    on_distance = _member_body(patch, "ondistancelessthan")

    assert "StageToRegister < 0 || owningQuest.IsStageDone(StageToRegister)" in on_init
    assert "RegisterForRemoteEvent(owningQuest, \"OnStageSet\")" in on_init
    assert "akSender != owningQuest || auiStageID != StageToRegister" in on_stage
    assert "RegisterForDistanceLessThanEvent(distanceRef, targetRef, fTargetDistance)" in on_init
    assert "RegisterForDistanceLessThanEvent(distanceRef, targetRef, fTargetDistance)" in on_stage
    assert "akObj1 != distanceRef || akObj2 != CachedDistanceRef" in on_distance
    assert on_distance.index("!owningQuest.IsStageDone(StageToSet)") < on_distance.index(
        "owningQuest.SetStage(StageToSet)"
    )


def test_chase_uses_ratified_actor_value_polarity_without_extra_effects():
    patch = _script_patch_source(CHASE_SCRIPT)
    assert patch is not None
    on_distance = _member_body(patch, "ondistancelessthan")
    polarity = (
        "targetActor.GetValue(W05_MQA_206P_MustDealWithJohnnyAV) == 0.0"
    )
    assert polarity in on_distance
    assert on_distance.index(polarity) < on_distance.index(
        "owningQuest.SetStage(StageToSet)"
    )
    assert patch.count("owningQuest.SetStage(StageToSet)") == 1
    for forbidden_effect in (
        ".AddItem(",
        ".RemoveItem(",
        ".SetValue(",
        ".ModValue(",
        ".MoveTo(",
        ".Enable(",
        ".Disable(",
        ".ForceRefTo(",
    ):
        assert forbidden_effect not in patch


@pytest.mark.parametrize(
    ("actor_value", "stage_done", "pair_matches", "sets_stage_525"),
    (
        (0.0, False, True, True),
        (1.0, False, True, False),
        (0.0, True, True, False),
        (0.0, False, False, False),
    ),
)
def test_chase_stage_525_behavior_matrix(
    actor_value: float,
    stage_done: bool,
    pair_matches: bool,
    sets_stage_525: bool,
):
    should_set_stage = pair_matches and not stage_done and actor_value == 0.0
    assert should_set_stage is sets_stage_525


def test_secret_service_trigger_has_player_combat_and_stage_gates():
    patch = _script_patch_source(TRIGGER_SCRIPT)
    assert patch is not None
    on_trigger = _member_body(patch, "ontriggerenter")

    player_guard = "akActionRef != playerRef"
    stage_guard = (
        "owningQuest.IsStageDone(StageToSet) || "
        "owningQuest.IsStageDone(TurnOffStage)"
    )
    combat_guard = "playerRef.IsInCombat()"
    set_stage = "owningQuest.SetStage(StageToSet)"
    assert on_trigger.index(player_guard) < on_trigger.index(set_stage)
    assert on_trigger.index(stage_guard) < on_trigger.index(set_stage)
    assert on_trigger.index(combat_guard) < on_trigger.index(set_stage)
    assert patch.count(set_stage) == 1


@pytest.mark.parametrize(
    (
        "is_player",
        "in_combat",
        "start_stage_done",
        "turnoff_stage_done",
        "sets_stage_150",
    ),
    (
        (True, False, False, False, True),
        (False, False, False, False, False),
        (True, True, False, False, False),
        (True, False, True, False, False),
        (True, False, False, True, False),
    ),
)
def test_secret_service_trigger_behavior_matrix(
    is_player: bool,
    in_combat: bool,
    start_stage_done: bool,
    turnoff_stage_done: bool,
    sets_stage_150: bool,
):
    should_set_stage = (
        is_player
        and not in_combat
        and not start_stage_done
        and not turnoff_stage_done
    )
    assert should_set_stage is sets_stage_150


def test_mqa_patches_have_no_unapproved_quest_or_world_effects():
    combined = "\n".join(_script_patch_source(name) or "" for name in PATCH_CASES)
    for forbidden_effect in (
        "SetObjectiveCompleted(",
        "SetObjectiveFailed(",
        "CompleteQuest(",
        ".AddItem(",
        ".RemoveItem(",
        ".ModValue(",
        ".ForceRefTo(",
        "Reputation_AV_",
        "Rep_Mod_",
        "defaultquestencounterwavescript",
        "Community",
        "Bounty",
    ):
        assert forbidden_effect not in combined


@pytest.mark.parametrize("base_name", PATCH_CASES)
def test_mqa_production_merge_preserves_skeleton_and_is_idempotent(base_name: str):
    skeleton = _production_skeleton(base_name)
    patch = _script_patch_source(base_name)
    assert patch is not None
    merged = _merge_script_method_patches(skeleton, patch)

    skeleton_header = next(
        line for line in skeleton.splitlines() if line.lower().startswith("scriptname ")
    )
    assert skeleton_header in merged
    for line in skeleton.splitlines():
        if " property " in f" {line.lower()} ":
            assert line in merged

    merged_counts = Counter(_member_name_list(merged))
    for member in PATCH_CASES[base_name]:
        assert merged_counts[member] == 1
    assert _merge_script_method_patches(merged, patch) == merged


@pytest.mark.parametrize("base_name", PATCH_CASES)
def test_mqa_production_merge_native_compiles_for_fo4(base_name: str):
    base_source = _fo4_base_source()
    assert base_source is not None, "FO4 base Papyrus sources unavailable"
    assert GENERATED_SOURCE_ROOT.is_dir(), "generated source root unavailable"

    result = compile_psc(
        _merged_production_source(base_name),
        imports=[str(base_source), str(GENERATED_SOURCE_ROOT)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=f"{base_name.rsplit(':', 1)[-1]}.psc",
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None


def test_mqa_patch_count_matches_authorized_shard():
    assert len(PATCH_CASES) == 3
