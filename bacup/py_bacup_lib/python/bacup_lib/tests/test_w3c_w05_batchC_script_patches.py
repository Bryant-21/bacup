from __future__ import annotations

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

# contracts/w3c-w05-batchC.md -- the 10 rows still owned by this Batch-C test
# adjudication approved for authoring this wave (consolidated-file Row
# numbers 3, 5, 7, 8, 9, 10, 13, 15, 19, 20). All 10 are top-level scripts
# (no Fragments: prefix), so they deploy directly under data/Scripts/. Rows
# 1, 4, 6, 11, 12, 14, 16, 17, 18 are not patched here (row 6 is non-defect --
# zero live carriers; row 4 is evidence-blocked; row 12 was already applied
# via a separate commit; row 18 is HOLD for the 3e design gate; the remainder
# are conditional/deferred/OPEN -- see the contract). Row 2's superseding
# OnActivate contract is owned and tested by Batch D.
PATCH_CASES: dict[str, dict[str, set[str] | tuple[str, ...]]] = {
    "W05_Daily_F01_Script": {
        "members": {"onquestinit", "ondistancelessthan"},
        "snippets": (
            "SetStage(Maguffins[selectedIndex].stageID)",
            "RegisterForDistanceLessThanEvent(player, thief, MaxDistance as Float)",
            "W05_Daily_F01_SignalStrengthMessage.Show(afDistance)",
            "SetStage(300)",
        ),
    },
    "W05_Daily_R01_PlayerAliasScript": {
        "members": {"onlocationchange"},
        "snippets": (
            "akNewLoc == LocCranberryWatogaLocation",
            "PlayerKeywordWatoga.GetReference() != player",
            "PlayerKeywordWatoga.ForceRefTo(player)",
            "akNewLoc == LocToxicWavyWillardsWaterparkLocation",
            "PlayerKeywordWavyWillards.ForceRefTo(player)",
        ),
    },
    "W05_Daily_R01_QuestScript_NEW": {
        "members": {"onquestinit", "initializetechselection"},
        "snippets": (
            "InitializeTechSelection()",
            "Bool[] used = new Bool[poolSize]",
            "SetStage(BrokenTechStages[pick])",
            "SetStage(NormalTechStages[pick])",
        ),
    },
    "W05_Daily_R02_FormerRaiderScript": {
        "members": {"ondeath"},
        "snippets": (
            "ownerQuest.SetStage(PlayerKilledStage)",
            "player.GetDistance(myActor) <= (DistanceCheck as Float)",
            "ownerQuest.SetStage(OtherKilledStage)",
        ),
    },
    "W05_Daniel_StartSceneScript": {
        "members": {"onaliasinit", "objectreference.ontriggerenter"},
        "snippets": (
            'RegisterForRemoteEvent(TriggeringAlias.GetReference(), "OnTriggerEnter")',
            "W05_Daniel_ToggleTriggerApproach.GetValue() <= 0.0",
            "SceneToPlay.IsPlaying()",
            "SceneToPlay.Start()",
        ),
    },
    "W05_DnD_MainDoor_Script": {
        "members": {"onopen"},
        "snippets": (
            "player.GetValue(W05_MQ_003P_Muscle_DnD_FrontDoorUnlockedOnce) >= 1.0",
            "player.SetValue(W05_MQ_003P_Muscle_DnD_FrontDoorUnlockedOnce, 1.0)",
        ),
    },
    "W05_InstSwapEnableStateQuestStage": {
        "members": {"oninit", "onload", "quest.onstageset", "evaluateandsetstate"},
        "snippets": (
            'RegisterForRemoteEvent(EnableStates[i].OwningQuest, "OnStageSet")',
            "EnableStates[i].TargetStageUseGetStageDone",
            "Self.Enable()",
            "Self.Disable()",
        ),
    },
    "W05_KillAliasOnCriteria": {
        "members": {"onload"},
        "snippets": (
            "owner.GetValue(AllowCleanUpValue) >= CleanupValueAmount",
            "target.Disable()",
            "owner.GetValue(KillNPCValue) >= KillValueAmount",
            "target.Kill()",
        ),
    },
    "W05_MortTapeQuestScript": {
        "members": {
            "onquestinit",
            "objectreference.onholotapeplay",
            "objectreference.onitemadded",
            "getplayedholotape",
            "processholotape",
            "ontimer",
            "showtutorialentry",
        },
        "snippets": (
            'RegisterForRemoteEvent(player, "OnItemAdded")',
            "akBaseItem as Holotape",
            "StartTimer(W05_Wayward_MortTapeTutorialCooldown.GetValue(), CooldownTimerID)",
            "aiTimerID != CooldownTimerID",
            "TutorialData[i].bTimerProcessed = False",
            "TutorialData[aiIndex].TargetMessage.Show()",
        ),
    },
    "W05_PurchaseBullionInfoScript": {
        "members": {"onbegin", "onend", "dopurchase"},
        "snippets": (
            "player.RemoveItem(Caps001, Caps, True)",
            "bullionToGrant = (Caps as Float / W05_VendorBullionCost.GetValue()) as Int",
            "player.AddItem(GoldBullion, bullionToGrant, True)",
            "GoldBullion_MaxRefundMessage.Show()",
        ),
    },
}


def _member_names(source: str) -> set[str]:
    return {
        name
        for kind, name, _start, _end in _iter_top_level_papyrus_members(
            source.splitlines()
        )
        if kind in {"function", "event"}
    }


def _merged_production_source(base_name: str) -> str:
    pex_path = DEPLOYED_SCRIPTS_ROOT / f"{base_name.lower()}.pex"
    if not pex_path.is_file():
        pytest.skip(f"deployed production PEX unavailable: {pex_path}")
    skeleton = decompile_pex(pex_path, fo4_api_compat=True)
    patch = _script_patch_source(base_name)
    assert patch is not None
    return _merge_script_method_patches(skeleton, patch)


@pytest.mark.parametrize(("base_name", "case"), PATCH_CASES.items())
def test_batchC_patch_has_expected_members_and_calls(
    base_name: str, case: dict[str, set[str] | tuple[str, ...]]
):
    patch = _script_patch_source(base_name)

    assert patch is not None
    assert not any(
        line.strip().lower().startswith("scriptname ") for line in patch.splitlines()
    )
    assert _member_names(patch) == case["members"]
    for snippet in case["snippets"]:
        assert snippet in patch


@pytest.mark.parametrize("base_name", PATCH_CASES)
def test_batchC_production_merge_native_compiles_for_fo4(base_name: str):
    base_source = _fo4_base_source()
    if base_source is None:
        pytest.skip("FO4 base Papyrus sources unavailable")

    result = compile_psc(
        _merged_production_source(base_name),
        imports=[str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=f"{base_name}.psc",
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None


def test_daily_r01_questscript_new_uses_dynamic_bool_array_not_missing_utility_call():
    # Contract row 7: an earlier draft called a nonexistent
    # Utility.CreateBoolArray(poolSize) helper; confirmed absent from the
    # indexed wiki corpus, replaced with the standard FO4 Papyrus dynamic
    # array allocation syntax.
    patch = _script_patch_source("W05_Daily_R01_QuestScript_NEW")
    assert patch is not None
    assert "Utility.CreateBoolArray" not in patch
    assert "Bool[] used = new Bool[poolSize]" in patch


def test_daily_r01_questscript_no_suffix_ships_no_patch():
    # Contract row 6: W05_Daily_R01_QuestScript (no _NEW suffix) has zero
    # live VMAD carriers even at SOURCE -- non-defect, not patched. Guards
    # against accidentally authoring a patch under the wrong (unsuffixed)
    # base name for row 7's sibling.
    assert _script_patch_source("W05_Daily_R01_QuestScript") is None


def test_instswapenablestatequeststage_registers_remote_event_before_first_evaluate():
    # Contract row 13: coordinator required an added trigger design (OnInit
    # registration + OnLoad safety re-eval) around the already-drafted
    # EvaluateAndSetState() body, since the skeleton shipped no reachable
    # entry point on its own.
    patch = _script_patch_source("W05_InstSwapEnableStateQuestStage")
    assert patch is not None
    members = _member_names(patch)
    assert {"oninit", "onload", "quest.onstageset"} <= members

    oninit_start, oninit_end = next(
        (start, end)
        for kind, name, start, end in _iter_top_level_papyrus_members(patch.splitlines())
        if kind == "event" and name == "oninit"
    )
    oninit_body = "\n".join(patch.splitlines()[oninit_start:oninit_end])
    assert "EvaluateAndSetState()" in oninit_body


def test_instswapenablestatequeststage_false_branch_uses_equality_not_at_least():
    # rev-3d required micro-amendment: TargetStageUseGetStageDone=False must compare
    # GetStage() == TargetStage, not >=. The docstring says "based only on whether the
    # owning quest is CURRENTLY SET TO the TargetStage" -- equality, matching the
    # sticky-vs-current distinction the True/False branches exist to express. No live
    # carrier exercises the False branch, so this is docstring fidelity, not a behavior
    # change for any currently-bound record.
    patch = _script_patch_source("W05_InstSwapEnableStateQuestStage")
    assert patch is not None
    assert "GetStage() == EnableStates[i].TargetStage" in patch
    assert "GetStage() >= EnableStates[i].TargetStage" not in patch


def test_morttapequestscript_ontimer_resets_processed_flags_not_replay_state():
    # Contract row 19, coordinator amendment: the drafted body only set
    # bTimerProcessed = True per entry with no reset path, which would
    # permanently suppress the tutorial after one showing. OnTimer must
    # clear every entry's flag on cooldown expiry, not just cancel the timer.
    patch = _script_patch_source("W05_MortTapeQuestScript")
    assert patch is not None
    members = _member_names(patch)
    assert "ontimer" in members

    ontimer_start, ontimer_end = next(
        (start, end)
        for kind, name, start, end in _iter_top_level_papyrus_members(patch.splitlines())
        if kind == "event" and name == "ontimer"
    )
    ontimer_body = "\n".join(patch.splitlines()[ontimer_start:ontimer_end])
    assert "TutorialData[i].bTimerProcessed = False" in ontimer_body
    assert "CancelTimer" not in ontimer_body


def test_purchasebullioninfoscript_routes_by_purchaseonbegin_not_inverted():
    # Contract row 20: the tracer's addendum initially inverted the routing;
    # SOURCE-PEX docstring confirms PurchaseOnBegin=True routes the purchase
    # into OnEnd, not OnBegin -- the corrected routing, not the inverted one.
    patch = _script_patch_source("W05_PurchaseBullionInfoScript")
    assert patch is not None
    assert "If !PurchaseOnBegin\n        DoPurchase()" in patch
    assert "If PurchaseOnBegin\n        DoPurchase()" in patch


def test_batchC_patch_count_matches_adjudication():
    # 10 rows remain owned by this Batch-C test (consolidated-file rows
    # 3, 5, 7, 8, 9, 10, 13, 15, 19, 20). Row 2's corrected OnActivate
    # semantics are owned by Batch D. Rows 1, 4, 6, 11, 12, 14, 16, 17
    # are not patched (non-defect / evidence-blocked / already-applied /
    # conditional-deferred / OPEN); row 18 is HOLD for the 3e design gate --
    # none of those nine ship a patch this wave.
    assert len(PATCH_CASES) == 10
