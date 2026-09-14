"""Stage 550 of W05_MQR_205P ("Kill Johnny") must actually kill Johnny.

Johnny's quest alias carries DefaultAliasOnDeath(preReqStage=550, StageToSet=560),
so the branch where the player pushes Johnny to keep hacking only closes out if
something applies lethal damage at stage 550. FO76 drove that from the (hollow)
W05_MQR_205P_QuestScript using its bound timer and damage values.
"""

from __future__ import annotations

from collections import Counter
from pathlib import Path

from bacup_lib.tests.test_terminal_fragment_script_patches import _fo4_base_source
from bacup_lib.workflows.unified import (
    _iter_top_level_papyrus_members,
    _merge_script_method_patches,
    _script_patch_source,
)
from creation_lib.pex.native_runtime import compile_psc


QF_205P = "Fragments:Quests:QF_W05_MQR_205P_00548B7A"
CONTROLLER = "W05_MQR_205P_QuestScript"

# Mirrors the real generated skeleton's declarations for the members under test.
CONTROLLER_SKELETON = """Scriptname W05_MQR_205P_QuestScript Extends Quest

Float HealthDamageAmount = 99999.0

Float Property JohnnyDeathTimerLength = 2.0 Auto
ActorValue Property Health Auto mandatory
Int Property JohnnyDeathTimerId = 1 Auto
ReferenceAlias Property Johnny Auto mandatory
"""

FRAGMENT_SKELETON = """Scriptname Fragments:Quests:QF_W05_MQR_205P_00548B7A Extends Quest hidden

ActorValue Property W05_MQR_JohnnyDeadValue Auto mandatory
ReferenceAlias Property Alias_currentPlayer Auto mandatory

Function Fragment_Stage_0550_Item_00()
EndFunction

Function Fragment_Stage_0560_Item_00()
EndFunction
"""


def _patch(script_name: str) -> str:
    source = _script_patch_source(script_name)
    assert source is not None, f"missing durable patch for {script_name}"
    return source


def _member_names(source: str) -> list[str]:
    return [
        name
        for _kind, name, _start, _end in _iter_top_level_papyrus_members(source.splitlines())
    ]


def _member_body(source: str, member_name: str) -> str:
    lines = source.splitlines()
    for _kind, name, start, end in _iter_top_level_papyrus_members(lines):
        if name == member_name:
            return "\n".join(lines[start : end + 1])
    raise AssertionError(f"{member_name} not found")


def test_controller_patch_declares_only_the_death_members():
    assert Counter(_member_names(_patch(CONTROLLER))) == Counter(
        ["killjohnnyinsecurityroom", "ontimer"]
    )


def test_death_sequence_uses_bound_timer_and_damage_values():
    body = _patch(CONTROLLER)

    # Arming uses the bound timer length and id, never a literal delay.
    start = _member_body(body, "killjohnnyinsecurityroom")
    assert "StartTimer(JohnnyDeathTimerLength, JohnnyDeathTimerId)" in start
    assert "Johnny == None" in start
    assert "johnnyRef == None || johnnyRef.IsDead()" in start

    # Expiry damages the bound Health actor value by the bound amount.
    fired = _member_body(body, "ontimer")
    assert "aiTimerID != JohnnyDeathTimerId" in fired
    assert "johnnyRef.DamageValue(Health, HealthDamageAmount)" in fired
    # Re-entry must not double-damage a corpse.
    assert "johnnyRef == None || johnnyRef.IsDead()" in fired


def test_stage_550_arms_the_kill_and_560_records_it():
    fragment = _patch(QF_205P)

    armed = _member_body(fragment, "fragment_stage_0550_item_00")
    assert "KillJohnnyInSecurityRoom()" in armed
    # Sibling script types cannot cast directly; go through the shared Quest base
    # so the stock Fallout 4 compiler accepts the merged source.
    assert "(Self as Quest) as W05_MQR_205P_QuestScript" in armed

    closed = _member_body(fragment, "fragment_stage_0560_item_00")
    assert "SetValue(W05_MQR_JohnnyDeadValue, 1.0)" in closed


def test_no_fragment_uses_a_direct_sibling_script_cast():
    for script_name in (QF_205P, "Fragments:Quests:QF_W05_MQR_204P_00535E55"):
        source = _patch(script_name)
        for suffix in (
            "Self as W05_MQR_205P_QuestScript",
            "Self as W05_MQR_204P_QuestScript",
            "Self as defaultquestencounterwavescript",
        ):
            assert f"= {suffix}" not in source


def test_controller_merge_is_exact_unique_and_idempotent():
    patch = _patch(CONTROLLER)
    merged = _merge_script_method_patches(CONTROLLER_SKELETON, patch)

    assert Counter(_member_names(merged)) == Counter(
        ["killjohnnyinsecurityroom", "ontimer"]
    )
    for member_name in ("killjohnnyinsecurityroom", "ontimer"):
        assert _member_body(merged, member_name) == _member_body(patch, member_name)
    # Declarations still come exclusively from the skeleton.
    assert "Float HealthDamageAmount = 99999.0" in merged
    assert merged.count("Scriptname W05_MQR_205P_QuestScript") == 1
    assert _merge_script_method_patches(merged, patch) == merged


def test_merged_controller_compiles_for_fo4(tmp_path: Path):
    base_source = _fo4_base_source()
    assert base_source is not None, "FO4 base Papyrus sources unavailable"

    result = compile_psc(
        _merge_script_method_patches(CONTROLLER_SKELETON, _patch(CONTROLLER)),
        imports=[str(tmp_path), str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path="W05_MQR_205P_QuestScript.psc",
    )

    assert result.ok, "\n".join(str(item) for item in result.diagnostics)
    assert result.pex_bytes is not None
