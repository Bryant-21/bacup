from __future__ import annotations

import os
from pathlib import Path

import pytest

from bacup_lib.workflows.unified import (
    _iter_top_level_papyrus_members,
    _merge_script_method_patches,
    _script_patch_source,
    _script_relative_path,
)
from creation_lib.pex.native_runtime import compile_psc


REPO_ROOT = Path(__file__).resolve().parents[5]
SOURCE_ROOT = REPO_ROOT / "mods" / "SeventySix" / "Scripts" / "Source" / "User"

# Every script patched by shard w2-workshop-camp-misc, mapped to the top-level
# member(s) its patch must supply. Remote-relayed events keep their dotted
# "<SourceScriptType>.<EventName>" name (matches how _iter_top_level_papyrus_members
# captures the member-start regex's optional dotted group).
PATCH_CASES = {
    "WorkshopArtillerySmokeScript": {"oninit"},
    "WorkshopVertibirdGrenadeScript": {"oninit"},
    "WorkshopCampScript": {"oninit"},
    "WorkshopRadioScript": {"ondestructionstagechanged"},
    "WorkshopInitActorValueScript": {"oninit"},
    "WorkshopSpotlightTurretScript": {"oninit", "onload"},
    "WorkshopRefillingContainerScript": {
        "oninit",
        "onitemremoved",
        "mayberefill",
        "objectreference.ondestructionstagechanged",
    },
    "WorkshopCreatedActorScript": {
        "oninit",
        "spawncreatedactor",
        "actor.ondeath",
        "ontimer",
        "removecreatedactor",
    },
}


def _fo4_base_source() -> Path | None:
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
    for game_root in candidates:
        source_root = game_root / "Data" / "Scripts" / "Source" / "Base"
        if source_root.is_dir():
            return source_root
    return None


def _member_names(source: str) -> set[str]:
    return {
        name
        for _kind, name, _start, _end in _iter_top_level_papyrus_members(
            source.splitlines()
        )
    }


def _merged_source(script_name: str) -> str:
    source_path = SOURCE_ROOT / _script_relative_path(script_name, ".psc")
    patch = _script_patch_source(script_name)
    assert source_path.is_file(), source_path
    assert patch is not None
    return _merge_script_method_patches(
        source_path.read_text(encoding="utf-8"), patch
    )


@pytest.mark.parametrize(("script_name", "expected_members"), PATCH_CASES.items())
def test_patch_supplies_confirmed_members(
    script_name: str, expected_members: set[str]
):
    patch = _script_patch_source(script_name)
    assert patch is not None
    assert not any(
        line.strip().lower().startswith("scriptname ") for line in patch.splitlines()
    )
    assert expected_members <= _member_names(patch)


@pytest.mark.parametrize("script_name", PATCH_CASES)
def test_merged_source_has_single_scriptname_line(script_name: str):
    merged = _merged_source(script_name)
    assert merged.lower().count("scriptname ") == 1


def test_artillery_smoke_guards_and_self_tags():
    patch = _script_patch_source("WorkshopArtillerySmokeScript")
    assert patch is not None
    assert "If WorkshopArtilleryKW != None" in patch
    assert "Self.AddKeyword(WorkshopArtilleryKW)" in patch


def test_vertibird_grenade_self_tags_and_leaves_fail_message_unconsumed():
    # Coordinator-accepted scope: SQ_WorkshopVertibirdFailMessage has no evidenced
    # fail condition on this local marker and must stay unconsumed, not guessed.
    patch = _script_patch_source("WorkshopVertibirdGrenadeScript")
    assert patch is not None
    assert "Self.AddKeyword(WorkshopVertibirdGrenadeKW)" in patch
    assert "SQ_WorkshopVertibirdFailMessage" not in patch


def test_camp_script_self_tags_and_caches_player_only():
    # Coordinator-corrected scope: the attack-scheduling properties are left
    # unimplemented because no attack-trigger consumer or rating-comparison target
    # is evidenced (not because of a timer mechanism gap) — regression guard that
    # this patch stays narrow rather than growing speculative timer/rating logic.
    patch = _script_patch_source("WorkshopCampScript")
    assert patch is not None
    assert "Self.AddKeyword(SQ_CampAttackKeyword)" in patch
    assert "myPlayer = Game.GetPlayer()" in patch
    for unconsumed in (
        "AttackTimerMinDays",
        "NextAttackMinDays",
        "NextAttackMaxDays",
        "MinLevelForAttack",
        "WorkshopRatingNextAttackAllowed",
        "StartTimerGameTime",
    ):
        assert unconsumed not in patch


def test_radio_script_guards_unset_sentinel_before_toggling():
    patch = _script_patch_source("WorkshopRadioScript")
    assert patch is not None
    guard_index = patch.find("If DestroyedStage <= 0")
    off_index = patch.find("SetRadioOn(false)")
    on_index = patch.find("SetRadioOn(true)")
    assert -1 not in (guard_index, off_index, on_index)
    assert guard_index < off_index < on_index


def test_init_actor_value_script_writes_starting_value_onto_self():
    patch = _script_patch_source("WorkshopInitActorValueScript")
    assert patch is not None
    assert "Self.SetValue(InitialActorValue, StartingValue)" in patch


def test_spotlight_turret_caches_workshop_link_on_init_and_load():
    patch = _script_patch_source("WorkshopSpotlightTurretScript")
    assert patch is not None
    assert "myWorkshop = Self.GetLinkedRef()" in patch
    onload = patch[patch.index("Event OnLoad(") :]
    assert "If myWorkshop == None" in onload


def test_refilling_container_refill_gated_by_stock_and_cooldown_in_order():
    merged = _merged_source("WorkshopRefillingContainerScript")
    body = merged[merged.index("Function MaybeRefill(") :]
    guard_index = body.find(
        "WorkshopRefillingGrenade == None || Variable01 == None || MinGrenadeCount <= 0"
    )
    stock_index = body.find("Self.GetItemCount(WorkshopRefillingGrenade)")
    cooldown_index = body.find("Utility.GetCurrentGameTime() - Self.GetValue(Variable01)")
    add_index = body.find("Self.AddItem(WorkshopRefillingGrenade, MinGrenadeCount, true)")
    assert -1 not in (guard_index, stock_index, cooldown_index, add_index)
    assert guard_index < stock_index < cooldown_index < add_index


def test_refilling_container_only_registers_linked_ref_destruction_when_flagged():
    patch = _script_patch_source("WorkshopRefillingContainerScript")
    assert patch is not None
    oninit = patch[patch.index("Event OnInit(") : patch.index("Event OnItemRemoved(")]
    assert "If DestroyWithLinkedRef" in oninit
    assert 'RegisterForRemoteEvent(linkedRef, "OnDestructionStageChanged")' in oninit


def test_refilling_container_registers_an_inventory_event_filter_before_onitemremoved_can_fire():
    # Regression guard for program-wide lesson #11: OnItemAdded/OnItemRemoved never
    # dispatch to a script that hasn't called AddInventoryEventFilter. Without this
    # call, MaybeRefill()'s OnItemRemoved trigger is a silent no-op.
    patch = _script_patch_source("WorkshopRefillingContainerScript")
    assert patch is not None
    oninit = patch[patch.index("Event OnInit(") : patch.index("Event OnItemRemoved(")]
    assert "If WorkshopRefillingGrenade != None" in oninit
    assert "AddInventoryEventFilter(WorkshopRefillingGrenade)" in oninit


def test_refilling_container_destruction_handler_checks_sender_identity():
    # Review Minor: the remote OnDestructionStageChanged handler verifies its sender
    # against the linked ref (Row 4's Actor.OnDeath does the same for its own remote
    # registration) rather than trusting any relayed event unconditionally.
    patch = _script_patch_source("WorkshopRefillingContainerScript")
    assert patch is not None
    handler = patch[patch.index("Event ObjectReference.OnDestructionStageChanged(") :]
    guard_index = handler.find("If akSender != Self.GetLinkedRef()")
    delete_index = handler.find("Self.Delete()")
    assert -1 not in (guard_index, delete_index)
    assert guard_index < delete_index


def test_created_actor_spawn_guards_reentry_and_links_back_to_workshop():
    patch = _script_patch_source("WorkshopCreatedActorScript")
    assert patch is not None
    spawn = patch[
        patch.index("Function SpawnCreatedActor(") : patch.index("Event Actor.OnDeath(")
    ]
    assert "If createdActorRef != None && !createdActorRef.IsDead()" in spawn
    assert "createdActorRef = Self.PlaceActorAtMe(CreatedActorBase)" in spawn
    assert (
        "createdActorRef.SetLinkedRef(workshopRef, WorkshopLinkCreatedActorTarget)"
        in spawn
    )
    assert "If StartsDestroyed" in spawn
    assert 'RegisterForRemoteEvent(createdActorRef, "OnDeath")' in spawn


def test_created_actor_spawn_guards_its_mandatory_reference_properties():
    # Review Minor: mandatory reference-type properties get the same defensive
    # None-safety convention Rows 1/2/3/10 apply to their own mandatory properties.
    patch = _script_patch_source("WorkshopCreatedActorScript")
    assert patch is not None
    spawn = patch[
        patch.index("Function SpawnCreatedActor(") : patch.index("Event Actor.OnDeath(")
    ]
    guard_index = spawn.find("If CreatedActorBase == None")
    place_index = spawn.find("Self.PlaceActorAtMe(CreatedActorBase)")
    assert -1 not in (guard_index, place_index)
    assert guard_index < place_index

    assert "If WorkshopItemKeyword != None" in spawn
    assert (
        "If workshopRef != None && WorkshopLinkCreatedActorTarget != None" in spawn
    )


def test_created_actor_ondeath_ignores_stale_senders_and_arms_cooldown_timers():
    patch = _script_patch_source("WorkshopCreatedActorScript")
    assert patch is not None
    ondeath = patch[
        patch.index("Event Actor.OnDeath(") : patch.index("Event OnTimer(")
    ]
    assert "If akSender != createdActorRef" in ondeath
    assert 'UnregisterForRemoteEvent(createdActorRef, "OnDeath")' in ondeath
    assert "StartTimer(DestroyAfterDeathSeconds, DestroyAfterDeathTimerID)" in ondeath


def test_created_actor_remove_respawns_only_when_delete_flag_is_set():
    patch = _script_patch_source("WorkshopCreatedActorScript")
    assert patch is not None
    remove = patch[patch.index("Function RemoveCreatedActor(") :]
    assert "If DeleteActorWhenDestroyed" in remove
    delete_branch, _, no_delete_branch = remove.partition("Else")
    assert "createdActorRef.Delete()" in delete_branch
    assert "SpawnCreatedActor()" in delete_branch
    assert "SpawnCreatedActor()" not in no_delete_branch


def test_created_actor_remove_clears_the_bound_quest_alias():
    patch = _script_patch_source("WorkshopCreatedActorScript")
    assert patch is not None
    remove = patch[patch.index("Function RemoveCreatedActor(") :]
    assert "If ActorAliasID >= 0 && QuestToRemoveFrom != None" in remove
    assert (
        "QuestToRemoveFrom.GetAlias(ActorAliasID) as ReferenceAlias" in remove
    )
    assert "targetAlias.Clear()" in remove


@pytest.mark.parametrize("script_name", PATCH_CASES)
def test_merged_patch_native_compiles_for_fo4(script_name: str):
    base_source = _fo4_base_source()
    if base_source is None:
        pytest.skip("FO4 base Papyrus sources unavailable")

    merged = _merged_source(script_name)
    result = compile_psc(
        merged,
        imports=[str(SOURCE_ROOT), str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=str(_script_relative_path(script_name, ".psc")),
    )
    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None
