"""The five Burning Springs controllers.

The native Papyrus compiler accepts unknown identifiers and non-existent member
calls, so each merged PSC is also compiled with stock ``PapyrusCompiler.exe`` and
its symbols resolved through the Papyrus LSP ``ScriptDB``.

``Burn_MQ03_MidQuestChallengeGrants`` carries a declared local substitute:
Fallout 4 has no ``CHAL`` record and no Challenge Menu, so both challenge arrays
arrive null. The two Rust King gates become a cumulative count of completed
Burning Springs bounty hunts against the shipped ``StageCompletionTargets``
globals. The tests pin that the substitute is a real gate, not a bypass.
"""

from __future__ import annotations

import os
import subprocess
from pathlib import Path

import pytest

from bacup_lib.workflows.unified import (
    _augment_fo76_to_fo4_script_skeleton,
    _iter_papyrus_states,
    _iter_top_level_papyrus_members,
    _merge_script_method_patches,
    _script_patch_source,
    _script_relative_path,
)
from creation_lib.papyrus_lsp import ScriptDB


REPO_ROOT = Path(__file__).resolve().parents[5]
SOURCE_ROOT = REPO_ROOT / "mods" / "SeventySix" / "Scripts" / "Source" / "User"

ARENA_CHEER = "Burn_RustKingArenaCheerScript"
SQ01_RETURN = "BurnSQ01QuestReturn"
SET_VALUE_COLLECTION = "SetValueRefCollectionScript"
HOLOTAPE_TRACKING = "Burn_SQ04_HolotapeTrackingScript"
COMBAT_AREA = "Burn:Burn_Bounty:Burn_Bounty_CombatAreaSetLinkedRef"
CHALLENGE_GRANTS = "Burn_MQ03_MidQuestChallengeGrants"
MQ03_FRAGMENT = "Fragments:Quests:QF_Burn_MQ03_RustKingMid_00838038"

CONTROLLERS = (
    ARENA_CHEER,
    SQ01_RETURN,
    SET_VALUE_COLLECTION,
    HOLOTAPE_TRACKING,
    COMBAT_AREA,
    CHALLENGE_GRANTS,
)


def _fo4_root() -> Path | None:
    candidates: list[Path] = []
    configured = os.environ.get("FO4_DIR", "").strip().strip('"')
    if configured:
        candidates.append(Path(configured))
    env_path = REPO_ROOT / ".env"
    if env_path.is_file():
        for line in env_path.read_text(encoding="utf-8").splitlines():
            if line.startswith("FO4_DIR="):
                value = line.split("=", 1)[1].strip().strip('"')
                if value:
                    candidates.append(Path(value))
                break
    for root in candidates:
        if (root / "Papyrus Compiler" / "PapyrusCompiler.exe").is_file() and (
            root / "Data" / "Scripts" / "Source" / "Base"
        ).is_dir():
            return root
    return None


def _merged(script_name: str) -> str:
    source_path = SOURCE_ROOT / _script_relative_path(script_name, ".psc")
    skeleton = _augment_fo76_to_fo4_script_skeleton(
        script_name, source_path.read_text(encoding="utf-8")
    )
    patch = _script_patch_source(script_name)
    assert patch is not None, script_name
    return _merge_script_method_patches(skeleton, patch)


def _member_names(source: str) -> list[str]:
    return [
        name
        for _kind, name, _start, _end in _iter_top_level_papyrus_members(
            source.splitlines()
        )
    ]


def _body(source: str, header: str) -> str:
    assert header in source, header
    tail = source.split(header, 1)[1]
    return tail.split("EndFunction", 1)[0].split("EndEvent", 1)[0]


def _code_lines(source: str) -> str:
    return "\n".join(
        line for line in source.splitlines() if not line.lstrip().startswith(";")
    )


@pytest.mark.parametrize("script_name", CONTROLLERS)
def test_controller_patch_is_member_only(script_name: str) -> None:
    patch = _script_patch_source(script_name)
    assert patch is not None
    for line in patch.splitlines():
        lowered = line.strip().lower()
        assert not lowered.startswith(("scriptname ", "state ", "endstate"))
        assert " property " not in f" {lowered} "
    assert _iter_papyrus_states(patch.splitlines()) == []


@pytest.mark.parametrize("script_name", CONTROLLERS)
def test_controller_merge_is_unique_and_idempotent(script_name: str) -> None:
    patch = _script_patch_source(script_name)
    assert patch is not None
    merged = _merged(script_name)
    assert merged.lower().count("scriptname ") == 1
    merged_names = _member_names(merged)
    for name in _member_names(patch):
        assert merged_names.count(name) == 1
    assert _merge_script_method_patches(merged, patch) == merged


def test_challenge_grants_never_read_the_null_chal_arrays() -> None:
    code = _code_lines(_script_patch_source(CHALLENGE_GRANTS))
    for null_property in ("ChallengeSetOne", "ChallengeSetTwo"):
        assert null_property not in code
    # The substitute reads the counter the family already maintains: the
    # repaired Grunt Hunt stage 9000 does ModValue on this actor value.
    assert 'Game.GetFormFromFile(0x00829BB6, "SeventySix.esm") as ActorValue' in code


def test_challenge_gate_is_cumulative_and_uses_the_shipped_globals() -> None:
    patch = _script_patch_source(CHALLENGE_GRANTS)
    cumulative = _body(patch, "Int Function CumulativeTargetFor(Int aiGroup)")
    assert "StageCompletionTargets[index].GetValue() as Int" in cumulative
    assert "While index <= aiGroup" in cumulative

    is_open = _body(patch, "Bool Function GroupIsOpen(Int aiGroup)")
    assert "IsStageDone(StageValues[aiGroup])" in is_open
    assert "!IsStageDone(CompletedStageValues[aiGroup])" in is_open

    evaluate = _body(patch, "Function EvaluateChallengeProgress()")
    assert "earned >= CumulativeTargetFor(groupIndex)" in evaluate
    assert "UnlockGroup(groupIndex)" in evaluate

    unlock = _body(patch, "Function UnlockGroup(Int aiGroup)")
    # The unlock actor value must be set before the stage that reads it.
    assert unlock.index("SetValue(StageUnlocked[aiGroup], 1.0)") < unlock.index(
        "SetStage(CompletedStageValues[aiGroup])"
    )


def test_challenge_substitute_is_a_gate_and_not_a_reward_or_a_bypass() -> None:
    patch = _script_patch_source(CHALLENGE_GRANTS)
    for forbidden in (
        "CompleteQuest",
        "CompleteAllObjectives",
        "AddItem",
        "RewardPlayerXP",
        "SetStage(400)",
        "SetStage(600)",
        "SendStoryEvent",
        ".Start()",
    ):
        assert forbidden not in patch
    # Every advance goes through the guarded group evaluation.
    assert patch.count("SetStage(") == 1

    fragment = _script_patch_source(MQ03_FRAGMENT)
    for stage in ("0300", "0500"):
        body = _body(fragment, f"Function Fragment_Stage_{stage}_Item_00()")
        assert "controller.EvaluateChallengeProgress()" in body
    assert "SetStage(400)" not in fragment
    assert "SetStage(600)" not in fragment


def test_arena_cheer_is_window_scoped_chance_gated_ambience() -> None:
    patch = _script_patch_source(ARENA_CHEER)
    active = _body(patch, "Bool Function CheersAreActive()")
    assert "!IsStageDone(StageToBeginCheers)" in active
    assert "!IsStageDone(StageToEndCheers)" in active

    on_kill = _body(patch, "Event Actor.OnKill(Actor akSender, Actor akVictim)")
    assert "akSender != ArenaPlayerReference()" in on_kill
    assert "!CheersAreActive() || !KillIsInTheArena(akSender, akVictim)" in on_kill
    assert "Utility.RandomFloat(0.0, 1.0) > ChanceToCheerOnKill" in on_kill

    # The two consumers configure this differently: BURN_SQ01 binds no
    # ArenaLocation and relies on CheerSFXRadius, BURN_SQ02_OutroP2 binds one.
    in_arena = _body(patch, "Bool Function KillIsInTheArena(Actor akCrowd, Actor akVictim)")
    assert "If ArenaLocation != None" in in_arena
    assert "akCrowd.IsInLocation(ArenaLocation)" in in_arena
    assert "akVictim.GetDistance(akCrowd) <= CheerSFXRadius" in in_arena

    # Ambience only: it must not touch quest state.
    for forbidden in ("SetStage", "SetObjective", "CompleteQuest", "AddItem"):
        assert forbidden not in patch


def test_sq01_return_teleports_only_the_player_to_a_resolved_destination() -> None:
    patch = _script_patch_source(SQ01_RETURN)
    activate = _body(
        patch,
        "Event ObjectReference.OnActivate(ObjectReference akSender, ObjectReference akActionRef)",
    )
    assert "akActionRef != player" in activate
    assert "activatorAlias.GetReference() == akSender" in activate
    assert "If destination != None" in activate
    assert activate.index("player.MoveTo(destination)") < activate.index(
        "AdvanceStageForIndex(stageIndex)"
    )

    # StagesProperties is unbound on the only live consumer (007DF76B), so the
    # gate must default open and the stage set must be a no-op rather than a
    # None dereference.
    gate = _body(patch, "Bool Function StageGateIsOpen(Int aiStageIndex)")
    assert "If StagesProperties == None" in gate
    assert "Return True" in gate
    advance = _body(patch, "Function AdvanceStageForIndex(Int aiStageIndex)")
    assert "If StagesProperties == None" in advance
    assert "!IsStageDone(stageToSet)" in advance


def test_set_value_collection_skips_unresolvable_actor_values() -> None:
    # The only live entry (alias#79 of 007DF76B) points at an actor value that
    # exists in neither plugin, so the loop must skip rather than crash.
    patch = _script_patch_source(SET_VALUE_COLLECTION)
    apply_one = _body(patch, "Function ApplyActorValuesTo(ObjectReference akRef)")
    assert "If akRef == None || ActorValues == None" in apply_one
    assert "If valueToSet != None" in apply_one
    assert "akRef.SetValue(valueToSet, ActorValues[index].ValueToSet)" in apply_one
    assert "If UseOnRefAddedTiming" in _body(
        patch, "Event OnLoad(ObjectReference akSenderRef)"
    )


def test_holotape_tracking_is_the_sole_producer_of_stages_one_to_fifteen() -> None:
    patch = _script_patch_source(HOLOTAPE_TRACKING)
    setter = _body(patch, "Function SetStageForHolotape(Form akHolotape)")
    assert "HolotapeStageTracking[index].DirtyLaundryHolotapes == akHolotape" in setter
    assert "!owner.IsStageDone(stageToSet)" in setter
    assert setter.count("owner.SetStage(stageToSet)") == 1
    # SetOnEnd selects acquisition vs. playing; the shipped value is False.
    assert "If !SetOnEnd" in _body(
        patch,
        "Event OnItemAdded(Form akBaseItem, Int aiItemCount, ObjectReference akItemReference, ObjectReference akSourceContainer)",
    )
    assert "If SetOnEnd" in _body(
        patch, "Event OnItemEquipped(Form akBaseObject, ObjectReference akReference)"
    )


def test_holotape_tracking_filters_inventory_events_to_the_tracked_holotapes() -> None:
    """SHARD_PROTOCOL.md #11: without a filter registered on an init path,
    OnItemAdded is never dispatched and stages 1-15 never set."""
    patch = _script_patch_source(HOLOTAPE_TRACKING)
    assert "RegisterHolotapeFilters()" in _body(patch, "Event OnAliasInit()")
    register = _body(patch, "Function RegisterHolotapeFilters()")
    assert "RemoveAllInventoryEventFilters()" in register
    assert "HolotapeStageTracking[index].DirtyLaundryHolotapes" in register
    assert "AddInventoryEventFilter(tracked)" in register
    # The blanket None filter would admit every item the player picks up.
    assert "AddInventoryEventFilter(None)" not in _code_lines(patch)


def test_combat_area_links_every_bound_dmp_trigger_by_its_own_keyword() -> None:
    patch = _script_patch_source(COMBAT_AREA)
    link = _body(patch, "Function LinkCombatAreaFor(ObjectReference akRef)")
    for trigger, keyword in (
        ("refSandboxTrigger", "SandboxKeyword"),
        ("refHoldUntilEngagedTrigger", "HoldUntilEngagedKeyword"),
        ("refHoldPositionTrigger", "HoldPositionKeyword"),
        ("refHoldPreferredPositionTrigger", "HoldPreferredPositionKeyword"),
    ):
        assert f"{trigger} != None && {keyword} != None" in link
        assert f"akRef.SetLinkedRef({trigger}, {keyword})" in link
    # Melee targets hold their own position instead of a ranged standoff point.
    assert "akRef.HasKeyword(BountyIsMelee)" in link
    cache = _body(patch, "Function CacheCombatAreaTriggers()")
    assert "If bTriggersInitialized" in cache


@pytest.mark.parametrize("script_name", CONTROLLERS + (MQ03_FRAGMENT,))
def test_controller_merged_source_compiles_under_stock_papyrus_compiler(
    script_name: str, tmp_path: Path
) -> None:
    fo4_root = _fo4_root()
    if fo4_root is None:
        pytest.skip("FO4 PapyrusCompiler.exe / Base scripts unavailable")
    compiler = fo4_root / "Papyrus Compiler" / "PapyrusCompiler.exe"
    base = fo4_root / "Data" / "Scripts" / "Source" / "Base"

    user_root = tmp_path / "Source" / "User"
    out_root = tmp_path / "out"
    out_root.mkdir(parents=True, exist_ok=True)
    # The MQ03 fragment calls into the merged challenge controller, so every
    # merged source has to be on the import path — the generated tree under
    # mods/SeventySix is one conversion run behind these patches.
    for dependency in CONTROLLERS + (MQ03_FRAGMENT,):
        dependency_path = user_root / _script_relative_path(dependency, ".psc")
        dependency_path.parent.mkdir(parents=True, exist_ok=True)
        dependency_path.write_text(_merged(dependency), encoding="utf-8")
    relative = _script_relative_path(script_name, ".psc")

    proc = subprocess.run(
        [
            str(compiler),
            str(Path(relative).with_suffix("")),
            f"-import={user_root};{SOURCE_ROOT};{base}",
            f"-output={out_root}",
            f"-flags={base / 'Institute_Papyrus_Flags.flg'}",
        ],
        cwd=str(user_root),
        capture_output=True,
        text=True,
        check=False,
    )
    output = (proc.stdout or "") + (proc.stderr or "")
    assert "Compilation succeeded." in output, output
    assert ", 0 failed." in output, output


def test_controller_symbols_resolve_through_the_papyrus_lsp(tmp_path: Path) -> None:
    merged_root = tmp_path / "merged"
    merged_root.mkdir()
    for script_name in CONTROLLERS + (MQ03_FRAGMENT,):
        destination = merged_root / _script_relative_path(script_name, ".psc")
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_text(_merged(script_name), encoding="utf-8")

    db = ScriptDB(str(tmp_path / "lsp.db"), source_dirs=[str(merged_root)])
    try:
        assert db.has_function(CHALLENGE_GRANTS, "EvaluateChallengeProgress")
        assert db.has_function(CHALLENGE_GRANTS, "CumulativeTargetFor")
        assert db.get_function_return_type(CHALLENGE_GRANTS, "CumulativeTargetFor") == "Int"
        assert db.has_property(CHALLENGE_GRANTS, "StageCompletionTargets")
        assert db.has_property(CHALLENGE_GRANTS, "StageUnlocked")
        assert db.has_function(MQ03_FRAGMENT, "ChallengeController")
        assert db.has_function(ARENA_CHEER, "PlayRandomCheer")
        assert db.has_property(ARENA_CHEER, "CheerSoundEffects")
        assert db.has_function(SQ01_RETURN, "WatchReturnActivators")
        assert db.has_property(SQ01_RETURN, "ActivatorsAndTeleport")
        assert db.has_function(SET_VALUE_COLLECTION, "ApplyActorValuesTo")
        assert db.has_function(HOLOTAPE_TRACKING, "SetStageForHolotape")
        assert db.has_function(HOLOTAPE_TRACKING, "RegisterHolotapeFilters")
        assert db.has_event(HOLOTAPE_TRACKING, "OnItemAdded")
        assert db.has_function(COMBAT_AREA, "LinkCombatAreaFor")
        assert db.has_property(COMBAT_AREA, "BountyIsMelee")
        # Negative control: names are not simply being accepted.
        assert not db.has_function(CHALLENGE_GRANTS, "EvaluateChallengeProgressZ")
        assert not db.has_property(ARENA_CHEER, "CheerSoundEffectsZ")
    finally:
        db.close()
