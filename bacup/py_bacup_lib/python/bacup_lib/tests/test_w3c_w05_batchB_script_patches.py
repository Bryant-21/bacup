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

# contracts/w3c-w05-batchB.md -- the 11 rows the coordinator's ADJUDICATION
# FINAL approved for authoring this wave. All 11 are top-level scripts (no
# Fragments: prefix), so they deploy directly under data/Scripts/, not under
# a fragments/ subfolder. Rows 1, 7, 12, 13, 16, 18 are not patched here
# (evidence-blocked / OPEN / non-defect -- see the contract); row 4 and 19
# are non-defect and ship no patch either.
PATCH_CASES: dict[str, dict[str, set[str] | tuple[str, ...]]] = {
    "W05_001P_Wayward_QuestScript": {
        "members": {"onstageset", "ontimer"},
        "snippets": (
            "auiStageID == 450",
            "StartTimer(BatterFailsafeTimerLength, FailsafeID)",
            "aiTimerID == FailsafeID",
            "SetStage(KillBatterStage)",
        ),
    },
    "W05_002_RadicalCombatantCollScript": {
        "members": {"oncombatstatechanged"},
        "snippets": (
            "aeCombatState != 0",
            "RecentlyAttackedAlias.ForceRefTo(akSenderRef)",
        ),
    },
    "W05_002P_GangerOnHitScript": {
        "members": {"onhit"},
        "snippets": (
            "akAggressor == OwningPlayer.GetReference()",
            "SetStage(StageToSet)",
        ),
    },
    "W05_002P_IntroSceneTriggerScript": {
        "members": {"ontriggerenter"},
        "snippets": (
            "Game.GetPlayer().GetValue(W05_MQ_002P_Radical_HeardRoperWarning) < 1.0",
            "!W05_MQ_002P_Radical.IsCompleted()",
            "Loudspeaker.GetReference().Say(W05_002P_RoperWarning)",
            "Game.GetPlayer().SetValue(W05_MQ_002P_Radical_HeardRoperWarning, 1.0)",
        ),
    },
    "W05_002P_RadicalHostilityTrigger": {
        "members": {"ontriggerenter"},
        "snippets": (
            "Game.GetPlayer().GetValue(W05_MQ_002P_Radical_PlayerKilledRoper) >= 1.0",
            "Game.GetPlayer().GetValue(W05_MQ_002P_Radical_RadicalFriendValue) < 1.0",
            "Game.GetPlayer().AddToFaction(W05_RadicalEnemyFaction)",
        ),
    },
    "W05_002P_TopicInfoRadicalsAttack": {
        "members": {"onend"},
        "snippets": (
            "playerRef.AddToFaction(W05_RadicalEnemyFaction)",
            "playerRef.RemoveFromFaction(W05_RadicalFriendFaction)",
            "playerRef.SetValue(W05_MQ_002P_Radical_RadicalFriendValue, 0.0)",
        ),
    },
    "W05_003P_AddObjectToCollection": {
        "members": {"onlocationchange", "onplayerloadgame", "populatekeycollection"},
        "snippets": (
            "GetOwningQuest().GetStage() >= PreReqStage",
            "KeyCollection.GetCount() == 0",
            "AccessCardSpawn.GetReference().PlaceAtMe(TargetKeys[i])",
            "KeyCollection.AddRef(spawnedKey)",
            "GetOwningQuest().SetStage(CompleteStage)",
        ),
    },
    "W05_003P_LaserGridStateCollScript": {
        "members": {"onload"},
        "snippets": (
            "OwningPlayer.GetReference().GetItemCount(UnlockItem) > 0",
            "akSenderRef.DisableNoWait()",
        ),
    },
    "W05_003P_WakeUpEnemiesRefCollScript": {
        "members": {"ontriggerenter"},
        "snippets": (
            "akActionRef == OwningPlayer.GetReference()",
            "OwningPlayer.GetReference().GetItemCount(TargetKey) == 0",
            "Game.GetPlayer().AddToFaction(EnemyFaction)",
        ),
    },
    "W05_Com_RFC_Defend": {
        "members": {"onstageset"},
        "snippets": (
            "auiStageID == 9000",
            "SQ = W05_Community_RaiderFishCamp_Quest as w05_com_rfc_participants_qi",
            "SQ.CurrentPlayerParticipants.RemoveRef(participant)",
            "SQ.CompletedPlayerParticipants.AddRef(participant)",
        ),
    },
    "W05_Com_RFC_Participants_RC": {
        "members": {"oncombatstatechanged"},
        "snippets": (
            "akTarget != None && CurrentPlayerParticipants != None",
            "CurrentPlayerParticipants.AddRef(akTarget)",
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
def test_batchB_patch_has_expected_members_and_calls(
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
def test_batchB_production_merge_native_compiles_for_fo4(base_name: str):
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


def test_wayward_failsafe_anchors_on_scripted_kill_shot_not_instance_load():
    # Contract row 2, coordinator ADJUDICATION FINAL (ratified over this
    # shard's own first-pass 460 anchor): 450 "Have Mort shoot Batter in the
    # face" is a scripted lethal action -- death is already the imminent
    # expected outcome there, satisfying the decision rule even though 460/520
    # (the hit/combat family) was the provisionally-guessed anchor. 102
    # (bare OnHit-registration note) and 520 (open-ended player-driven fight,
    # where forcing the kill mid-combat risks a worse defect than the one
    # being fixed) are both rejected anchors.
    patch = _script_patch_source("W05_001P_Wayward_QuestScript")
    assert patch is not None
    assert "auiStageID == 450" in patch
    assert "auiStageID == 460" not in patch
    assert "auiStageID == 102" not in patch
    assert "auiStageID == 520" not in patch


def test_radical_hostility_trigger_none_guards_object_typed_property_before_read():
    # Contract row 8: W05_MQ_002P_Radical_PlayerKilledRoper is Object-typed
    # (PEX default None) and left unbound at the Exterior site -- the
    # coordinator's None-check is mandatory, not defensive-only, since
    # Game.GetPlayer().GetValue(None) would error.
    patch = _script_patch_source("W05_002P_RadicalHostilityTrigger")
    assert patch is not None
    assert "W05_MQ_002P_Radical_PlayerKilledRoper != None &&" in patch


def test_addobjecttocollection_defines_shared_populate_helper_for_both_triggers():
    # Contract row 10: OnLocationChange and OnPlayerLoadGame both route into
    # one guarded populate function rather than duplicating the guard/spawn
    # logic in each event body.
    patch = _script_patch_source("W05_003P_AddObjectToCollection")
    assert patch is not None
    members = _member_names(patch)
    assert {"onlocationchange", "onplayerloadgame", "populatekeycollection"} <= members

    onlocationchange_start, onlocationchange_end = next(
        (start, end)
        for kind, name, start, end in _iter_top_level_papyrus_members(patch.splitlines())
        if kind == "event" and name == "onlocationchange"
    )
    onplayerloadgame_start, onplayerloadgame_end = next(
        (start, end)
        for kind, name, start, end in _iter_top_level_papyrus_members(patch.splitlines())
        if kind == "event" and name == "onplayerloadgame"
    )
    lines = patch.splitlines()
    onlocationchange_body = "\n".join(lines[onlocationchange_start:onlocationchange_end])
    onplayerloadgame_body = "\n".join(lines[onplayerloadgame_start:onplayerloadgame_end])
    assert "PopulateKeyCollection()" in onlocationchange_body
    assert "PopulateKeyCollection()" in onplayerloadgame_body


def test_wakeup_enemies_targets_player_not_enemycollection():
    # Contract row 14: the original tracer draft had the AddToFaction target
    # inverted onto EnemyCollection members; the coordinator's ruling and the
    # EnemyFaction PEX docstring ("apply to the player to make the enemies
    # attack them") settle the player as the target. EnemyCollection stays
    # unreferenced (dormant-disclosed) in the shipped body.
    patch = _script_patch_source("W05_003P_WakeUpEnemiesRefCollScript")
    assert patch is not None
    assert "Game.GetPlayer().AddToFaction(EnemyFaction)" in patch
    assert "EnemyCollection" not in patch


def test_batchB_patch_count_matches_adjudication_final():
    # 11 rows approved for authoring (2, 3, 5, 6, 8, 9, 10, 11, 14, 15, 17).
    # Rows 1, 18 are evidence-blocked; 4, 19 are non-defect; 7, 12, 13, 16 are
    # OPEN (split to the controller sub-pass / cross-shard join) -- none of
    # those eight ship a patch this wave.
    assert len(PATCH_CASES) == 11
