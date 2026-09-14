from __future__ import annotations

import csv
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
STATUS_PATH = REPO_ROOT / "bacup" / "docs" / "stub_restoration" / "status.csv"
PACKAGE_CLOSURE_CONTRACT = "contracts/w05-mqa-206-package-ctda-closure.md"

CHASE_SCRIPT = "W05_MQA_206_ChaseDistanceLessThan"
TRIGGER_SCRIPT = "W05_MQA_206P_SSTalkTriggerBoxScript"
QUEST_FRAGMENT = "Fragments:Quests:QF_W05_MQA_206P_0054EDB9"
EXIT_OPERATIONS_PACKAGE = (
    "Fragments:Packages:PF_W05_MQA_206P_ExitOperatio_00583506"
)
GAIL_ENTRANCE_PACKAGE = (
    "Fragments:Packages:PF_W05_MQA_206P_Gail_Entranc_00558978"
)
LEAVE_VAULT_PACKAGE = "Fragments:Packages:PF_W05_MQA_206P_LeaveVault_005674A3"
RARA_ENTRANCE_PACKAGE = (
    "Fragments:Packages:PF_W05_MQA_206P_RaRa_Entranc_00558977"
)

OBJECTIVE_STAGES = (
    100,
    105,
    200,
    250,
    300,
    400,
    500,
    525,
    550,
    575,
    600,
    700,
    5200,
    5220,
)
QF_STAGES = (
    4,
    5,
    6,
    20,
    21,
    22,
    30,
    33,
    35,
    50,
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
    100,
    105,
    150,
    200,
    250,
    300,
    400,
    450,
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
    500,
    525,
    550,
    575,
    585,
    590,
    600,
    700,
    800,
    5000,
    5010,
    5011,
    5012,
    5013,
    5050,
    5200,
    5205,
    5210,
    5220,
    5230,
    5240,
    5250,
    5275,
    5300,
    5400,
    5500,
    9000,
    9999,
)

PATCH_CASES = {
    CHASE_SCRIPT: {"onaliasinit", "quest.onstageset", "ondistancelessthan"},
    TRIGGER_SCRIPT: {"ontriggerenter"},
    QUEST_FRAGMENT: {
        *(f"fragment_stage_{stage:04d}_item_00" for stage in QF_STAGES),
    },
    EXIT_OPERATIONS_PACKAGE: {"fragment_end", "fragment_change"},
    GAIL_ENTRANCE_PACKAGE: {"fragment_end"},
    LEAVE_VAULT_PACKAGE: {"fragment_end", "fragment_change"},
    RARA_ENTRANCE_PACKAGE: {"fragment_end"},
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


def _clean_qf_skeleton() -> str:
    members = "\n\n".join(
        f"Function Fragment_Stage_{stage:04d}_Item_00()\nEndFunction"
        for stage in QF_STAGES
    )
    return (
        "Scriptname Fragments:Quests:QF_W05_MQA_206P_0054EDB9 "
        f"Extends Quest hidden\n\n{members}\n"
    )


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
    assert "; TODO" not in patch

    for member_name in expected_members:
        body_lines = _member_body(patch, member_name).splitlines()[1:-1]
        meaningful_lines = [line.strip() for line in body_lines if line.strip()]
        assert meaningful_lines, f"hollow member: {base_name}:{member_name}"
        if all(line.startswith(";") for line in meaningful_lines):
            explanation = " ".join(meaningful_lines).lower()
            assert "online-only" in explanation or "server-only" in explanation


def test_mqa_fragment_preserves_verified_objective_displays_and_stop():
    patch = _script_patch_source(QUEST_FRAGMENT)
    assert patch is not None

    for stage in OBJECTIVE_STAGES:
        member_name = f"fragment_stage_{stage:04d}_item_00"
        body = _member_body(patch, member_name)
        assert body.startswith(f"Function Fragment_Stage_{stage:04d}_Item_00()\n")
        assert body.count(f"SetObjectiveDisplayed({stage})") == 1

    assert _member_body(patch, "fragment_stage_9999_item_00") == (
        "Function Fragment_Stage_9999_Item_00()\n"
        "    Stop()\n"
        "EndFunction"
    )


def test_mqa_fragment_has_all_74_live_members_and_no_hollow_bodies():
    patch = _script_patch_source(QUEST_FRAGMENT)
    assert patch is not None
    expected_members = {
        f"fragment_stage_{stage:04d}_item_00" for stage in QF_STAGES
    }

    assert len(QF_STAGES) == 74
    assert len(expected_members) == 74
    assert _member_names(patch) == expected_members


def test_mqa_fragment_replaces_every_clean_live_skeleton_member_once():
    patch = _script_patch_source(QUEST_FRAGMENT)
    assert patch is not None

    merged = _merge_script_method_patches(_clean_qf_skeleton(), patch)
    member_counts = Counter(_member_name_list(merged))

    assert len(member_counts) == 74
    assert set(member_counts) == PATCH_CASES[QUEST_FRAGMENT]
    assert set(member_counts.values()) == {1}
    assert _merge_script_method_patches(merged, patch) == merged


@pytest.mark.parametrize(
    ("stage", "scene_property"),
    (
        (30, "W05_MQA_206P_SeeChase"),
        (33, "W05_MQA_206P_Ghoul"),
        (50, "W05_MQA_206P_GoldRoom"),
        (150, "W05_MQA_206P_Greet"),
        (585, "W05_MQA_206P_LiveOrDie"),
        (700, "W05_MQA_206P_OperationsScene"),
        (5000, "W05_MQA_206P_Raiders_Leaving"),
        (5050, "W05_MQA_206P_Johnny_001_Gold"),
    ),
)
def test_mqa_dialogue_scene_stages_start_the_exact_bound_scene(
    stage: int, scene_property: str
):
    patch = _script_patch_source(QUEST_FRAGMENT)
    assert patch is not None
    body = _member_body(patch, f"fragment_stage_{stage:04d}_item_00")

    assert f"{scene_property} && !{scene_property}.IsPlaying()" in body
    assert body.count(f"{scene_property}.Start()") == 1


def test_mqa_stage_200_opens_lous_door_without_skipping_the_power_search():
    patch = _script_patch_source(QUEST_FRAGMENT)
    assert patch is not None
    body = _member_body(patch, "fragment_stage_0200_item_00")

    assert "Alias_LouDoor.GetReference() as Default2StateActivator" in body
    # FO76 SetActivatorOpen has no FO4 counterpart; FO4 Default2StateActivator.SetOpen does.
    assert "louDoor.SetOpen(True)" in body
    assert "SetStage(250)" not in body
    assert "SetObjectiveDisplayed(200)" in body


def test_mqa_main_route_preserves_objective_and_epilogue_order():
    patch = _script_patch_source(QUEST_FRAGMENT)
    assert patch is not None

    entered_operations = _member_body(patch, "fragment_stage_0080_item_00")
    assert "SetObjectiveCompleted(600)" in entered_operations
    assert "SetObjectiveDisplayed(700)" in entered_operations
    assert "SetStage(75)" in entered_operations

    ventilated = _member_body(patch, "fragment_stage_0450_item_00")
    assert "SetObjectiveCompleted(400)" in ventilated
    assert "SetObjectiveDisplayed(440)" in ventilated
    found_gold = _member_body(patch, "fragment_stage_0460_item_00")
    assert "SetObjectiveCompleted(440)" in found_gold
    assert "SetObjectiveDisplayed(450)" in found_gold

    for stage in (81, 82, 83):
        choice = _member_body(patch, f"fragment_stage_{stage:04d}_item_00")
        assert "SetObjectiveCompleted(700)" in choice
    epilogue = _member_body(patch, "fragment_stage_0800_item_00")
    assert "SetObjectiveDisplayed(800)" not in epilogue
    assert epilogue.index("SetObjectiveCompleted(800)") < epilogue.index(
        "SetStage(9000)"
    )


def test_mqa_raider_departure_waits_for_both_distance_confirmations():
    patch = _script_patch_source(QUEST_FRAGMENT)
    assert patch is not None

    departure = _member_body(patch, "fragment_stage_5010_item_00")
    assert departure.index("Alias_Gail.GetActorReference()") < departure.index(
        "gailRef.EvaluatePackage()"
    )
    assert departure.index("Alias_RaRa.GetActorReference()") < departure.index(
        "raRaRef.EvaluatePackage()"
    )

    gail_distance = _member_body(patch, "fragment_stage_5011_item_00")
    assert "IsStageDone(5012) && !IsStageDone(5013)" in gail_distance
    assert gail_distance.count("SetStage(5013)") == 1

    ra_ra_distance = _member_body(patch, "fragment_stage_5012_item_00")
    assert "IsStageDone(5011) && !IsStageDone(5013)" in ra_ra_distance
    assert ra_ra_distance.count("SetStage(5013)") == 1

    cleanup = _member_body(patch, "fragment_stage_5013_item_00")
    assert cleanup.index("gailRef.Disable()") < cleanup.index("raRaRef.Disable()")


def test_mqa_departure_package_fragments_produce_the_two_confirmations_once():
    for script_name, stage in (
        (GAIL_ENTRANCE_PACKAGE, 5011),
        (RARA_ENTRANCE_PACKAGE, 5012),
    ):
        patch = _script_patch_source(script_name)
        assert patch is not None
        body = _member_body(patch, "fragment_end")

        assert "Quest owningQuest = GetOwningQuest()" in body
        assert f"!owningQuest.IsStageDone({stage})" in body
        assert body.count(f"owningQuest.SetStage({stage})") == 1
        for forbidden in ("AddItem(", "RemoveItem(", "MoveTo(", ".Start()"):
            assert forbidden not in body


def test_mqa_leave_vault_package_only_disables_the_callback_actor():
    patch = _script_patch_source(LEAVE_VAULT_PACKAGE)
    assert patch is not None

    for member_name in ("fragment_end", "fragment_change"):
        body = _member_body(patch, member_name)
        assert "If akActor" in body
        assert body.count("akActor.Disable()") == 1
        for forbidden in ("SetStage(", "AddItem(", "RemoveItem(", "MoveTo("):
            assert forbidden not in body


def test_mqa_package_record_dependencies_are_closed_as_patched():
    with STATUS_PATH.open(encoding="utf-8", newline="") as status_file:
        rows = {row["script_name"]: row for row in csv.DictReader(status_file)}

    for script_name in (
        GAIL_ENTRANCE_PACKAGE,
        LEAVE_VAULT_PACKAGE,
        RARA_ENTRANCE_PACKAGE,
    ):
        row = rows[script_name]
        assert row["terminal_state"] == "patched"
        assert row["evidence"] == PACKAGE_CLOSURE_CONTRACT


def test_mqa_stage_5400_reevaluates_johnnys_proven_follow_package():
    patch = _script_patch_source(QUEST_FRAGMENT)
    assert patch is not None
    stage_5400 = _member_body(patch, "fragment_stage_5400_item_00")

    assert stage_5400.index("Alias_Johnny.GetActorReference()") < stage_5400.index(
        "johnnyRef.EvaluatePackage()"
    )
    assert stage_5400.count("EvaluatePackage()") == 1


def test_mqa_restart_setup_converges_both_faction_paths_to_entered_vault_stage():
    patch = _script_patch_source(QUEST_FRAGMENT)
    assert patch is not None

    crater = _member_body(patch, "fragment_stage_0005_item_00")
    foundation = _member_body(patch, "fragment_stage_0006_item_00")
    assert "Alias_Johnny.GetActorReference()" in crater
    assert "Alias_Gail.GetActorReference()" in crater
    assert "Alias_RaRa.GetActorReference()" in crater
    assert crater.count("SetStage(4)") == 1
    assert "Alias_Jen.GetActorReference()" in foundation
    assert "Alias_Penny.GetActorReference()" in foundation
    assert "Alias_Radcliff.GetActorReference()" in foundation
    assert foundation.count("SetStage(4)") == 1


@pytest.mark.parametrize(
    ("stage", "alias_property"),
    tuple((stage, f"Alias_GoldActivator{stage - 470:02d}") for stage in range(471, 480)),
)
def test_mqa_gold_activators_grant_local_physical_gold_once_and_disable(
    stage: int, alias_property: str
):
    patch = _script_patch_source(QUEST_FRAGMENT)
    assert patch is not None
    body = _member_body(patch, f"fragment_stage_{stage:04d}_item_00")

    assert f"{alias_property}.GetReference()" in body
    assert body.count("playerRef.AddItem(Gold_Bullion, 100, True)") == 1
    assert body.count("playerRef.ModValue(W05_MQA_206P_CollectedBullion, 100.0)") == 1
    assert body.count("goldRef.Disable()") == 1


def test_mqa_chest_completes_the_local_thousand_gold_collection_route():
    patch = _script_patch_source(QUEST_FRAGMENT)
    assert patch is not None
    body = _member_body(patch, "fragment_stage_0481_item_00")

    assert "playerRef.AddItem(Gold_Bullion, 100, True)" in body
    assert "playerRef.GetItemCount(Gold_Bullion) >= GoldToRemove" in body
    assert body.count("SetStage(500)") == 1
    assert body.count("goldChest.Disable()") == 1


def test_mqa_currency_stage_preserves_gold_as_local_inventory_before_online_noop():
    patch = _script_patch_source(QUEST_FRAGMENT)
    assert patch is not None
    body = _member_body(patch, "fragment_stage_0085_item_00")

    assert body.index("playerRef.RemoveItem(Gold_Bullion, rawGold, True)") < body.index(
        "playerRef.AddItem(GoldBar, rawGold, True)"
    )
    assert "account bullion deposits" in body
    assert "physical gold is retained locally" in body


def test_mqa_johnny_share_choices_use_only_bound_local_transaction_amounts():
    patch = _script_patch_source(QUEST_FRAGMENT)
    assert patch is not None

    half = _member_body(patch, "fragment_stage_5205_item_00")
    most = _member_body(patch, "fragment_stage_5210_item_00")
    refund = _member_body(patch, "fragment_stage_5250_item_00")
    assert "Int johnnyShare = GoldToRemoveHalfToJohnny" in half
    assert "Int johnnyShare = GoldToRemoveMostToJohnny" in most
    assert "playerRef.AddItem(Gold_Bullion, GoldToRemoveExtraJohnnyShare, True)" in refund
    for body in (half, most):
        assert "If johnnyShare > availableGold" in body
        assert body.count("playerRef.RemoveItem(Gold_Bullion, johnnyShare, True)") == 1


def test_mqa_completion_keeps_local_reward_and_world_state_before_server_noops():
    patch = _script_patch_source(QUEST_FRAGMENT)
    assert patch is not None
    body = _member_body(patch, "fragment_stage_9000_item_00")

    assert "playerRef.AddItem(W05_Vault79Operations_KeyCard, 1, True)" in body
    assert "mapMarker.Enable()" in body
    assert body.index("SetObjectiveCompleted(800)") < body.index("CompleteQuest()")
    assert "account" in body
    assert "server-only" in body


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


def test_remote_event_patch_replaces_only_matching_decompiler_pseudo_member():
    skeleton = """Scriptname W05_MQA_206_ChaseDistanceLessThan Extends ReferenceAlias

Function ::remote_Quest_OnStageSet(Quest akSender, Int auiStageID, Int auiItemID)
EndFunction

Function ::remote_Quest_OnReset(Quest akSender)
EndFunction

Function KeepUnrelatedFunction()
EndFunction
"""
    patch = """Event Quest.OnStageSet(Quest akSender, Int auiStageID, Int auiItemID)
EndEvent
"""

    merged = _merge_script_method_patches(skeleton, patch)

    assert "::remote_Quest_OnStageSet" not in merged
    assert "::remote_Quest_OnReset" in merged
    assert "Function KeepUnrelatedFunction()" in merged
    assert merged.count("Event Quest.OnStageSet(") == 1
    assert _merge_script_method_patches(merged, patch) == merged


def test_remote_event_pseudo_member_matches_underscores_by_exact_derived_name():
    skeleton = """Scriptname RemoteEventUnderscoreTest Extends ReferenceAlias

Function ::remote_Quest_With_Underscore_On_Stage_Set(Quest akSender)
EndFunction

Function ::remote_Quest_With_Underscore_On_Stage_Set_Extra(Quest akSender)
EndFunction
"""
    patch = """Event Quest_With_Underscore.On_Stage_Set(Quest akSender)
EndEvent
"""

    merged = _merge_script_method_patches(skeleton, patch)

    assert "::remote_Quest_With_Underscore_On_Stage_Set(" not in merged
    assert "::remote_Quest_With_Underscore_On_Stage_Set_Extra(" in merged
    assert merged.count("Event Quest_With_Underscore.On_Stage_Set(") == 1


def test_named_state_remote_event_patch_replaces_only_matching_pseudo_member():
    skeleton = """Scriptname NamedStateRemoteEventTest Extends ReferenceAlias

State Listening
    Function ::remote_Quest_With_Underscore_On_Stage_Set(Quest akSender)
    EndFunction

    Function ::remote_Quest_With_Underscore_On_Other_Event(Quest akSender)
    EndFunction
EndState
"""
    patch = """State Listening
    Event Quest_With_Underscore.On_Stage_Set(Quest akSender)
    EndEvent
EndState
"""

    merged = _merge_script_method_patches(skeleton, patch)

    assert "::remote_Quest_With_Underscore_On_Stage_Set(" not in merged
    assert "::remote_Quest_With_Underscore_On_Other_Event(" in merged
    assert merged.count("Event Quest_With_Underscore.On_Stage_Set(") == 1
    assert _merge_script_method_patches(merged, patch) == merged


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
    assert len(PATCH_CASES) == 7
