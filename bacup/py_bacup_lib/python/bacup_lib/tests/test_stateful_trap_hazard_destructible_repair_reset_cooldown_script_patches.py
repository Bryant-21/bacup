from __future__ import annotations

import os
from pathlib import Path

import pytest

from bacup_lib.workflows.unified import (
    _iter_papyrus_states,
    _iter_papyrus_members_in_state,
    _iter_top_level_papyrus_members,
    _merge_script_method_patches,
    _script_patch_source,
)
from creation_lib.pex import decompile_pex
from creation_lib.pex.native_runtime import compile_psc


REPO_ROOT = Path(__file__).resolve().parents[5]
DEPLOYED_SCRIPTS_DIR = REPO_ROOT / "mods" / "SeventySix" / "data" / "Scripts"
PARENT_SOURCE_DIR = REPO_ROOT / "mods" / "SeventySix" / "Scripts" / "Source" / "User"

# script_name -> (deployed pex filename, expected new/repaired top-level member names)
PATCH_CASES = {
    "EditKeywordsOnCombatChangeScript": (
        "editkeywordsoncombatchangescript.pex",
        {
            ("event", "oncombatstatechanged"),
            ("function", "addkeywords"),
            ("function", "removekeywords"),
        },
    ),
    "EpicCreatureRestoreHealthEffectScript": (
        "epiccreaturerestorehealtheffectscript.pex",
        {("event", "oneffectstart")},
    ),
    "ExplodeOnDeathFX": (
        "explodeondeathfx.pex",
        {("event", "oneffectfinish")},
    ),
    "FirecrackerWhiskey_BurnAttackerScript": (
        "firecrackerwhiskey_burnattackerscript.pex",
        {
            ("event", "oneffectstart"),
            ("event", "oneffectfinish"),
            ("event", "onhit"),
            ("event", "ontimer"),
        },
    ),
    "MeatSweatsScript": (
        "meatsweatsscript.pex",
        {
            ("event", "oneffectstart"),
            ("event", "oneffectfinish"),
            ("event", "ontimer"),
        },
    ),
    "MN2_TreatBowlCooldownScript": (
        "mn2_treatbowlcooldownscript.pex",
        {("event", "onactivate")},
    ),
    "ModActorValueOnSpellTargetScript": (
        "modactorvalueonspelltargetscript.pex",
        {("event", "oneffectstart"), ("event", "oneffectfinish")},
    ),
    "MoM_PhantomEffectScript": (
        "mom_phantomeffectscript.pex",
        {("event", "oneffectstart"), ("event", "oneffectfinish")},
    ),
    "MOON_Ambush_SoundSpellCooldown": (
        "moon_ambush_soundspellcooldown.pex",
        {("event", "oneffectstart"), ("event", "oneffectfinish")},
    ),
    "MTNM01_BaitMineScript": (
        "mtnm01_baitminescript.pex",
        {("event", "onload"), ("event", "onunload"), ("event", "ontimer")},
    ),
    "MTNM01_DeathclawFriendPerkScript": (
        "mtnm01_deathclawfriendperkscript.pex",
        {("event", "onentryrun")},
    ),
    "MutationTriggerMeleeExplosionScript": (
        "mutationtriggermeleeexplosionscript.pex",
        {
            ("event", "oneffectstart"),
            ("event", "oneffectfinish"),
            ("event", "onhit"),
            ("event", "ontimer"),
        },
    ),
    "NWOT_Fortune_CooldownEffectScript": (
        "nwot_fortune_cooldowneffectscript.pex",
        {("event", "oneffectstart"), ("event", "oneffectfinish")},
    ),
    "OnHitByMeleeCastSpell": (
        "onhitbymeleecastspell.pex",
        {
            ("event", "oneffectstart"),
            ("event", "oneffectfinish"),
            ("event", "onhit"),
            ("event", "ontimer"),
        },
    ),
    "PerkPacifyScript": (
        "perkpacifyscript.pex",
        {("event", "onentryrun")},
    ),
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


def _production_skeleton(pex_filename: str) -> str:
    pex_path = DEPLOYED_SCRIPTS_DIR / pex_filename
    if not pex_path.is_file():
        pytest.skip(f"production PEX unavailable in deployed dir: {pex_filename}")
    return decompile_pex(pex_path, fo4_api_compat=True)


def _patch_source(script_name: str) -> str:
    patch = _script_patch_source(script_name)
    assert patch is not None
    return patch


def _merged_production_source(script_name: str, pex_filename: str) -> str:
    return _merge_script_method_patches(
        _production_skeleton(pex_filename), _patch_source(script_name)
    )


@pytest.mark.parametrize("script_name", PATCH_CASES)
def test_patch_exists_with_no_scriptname_line(script_name: str):
    patch = _patch_source(script_name)
    assert not any(
        line.strip().lower().startswith("scriptname ") for line in patch.splitlines()
    )


@pytest.mark.parametrize("script_name", PATCH_CASES)
def test_patch_declares_no_states_beyond_the_hollow_skeleton(script_name: str):
    # The merger cannot safely introduce new states — every state named in the
    # patch must already exist in the production skeleton. None of this shard's
    # patches declare a named state; all repairs live in the default state.
    pex_filename = PATCH_CASES[script_name][0]
    skeleton_states = {
        name for name, _start, _end in _iter_papyrus_states(_production_skeleton(pex_filename).splitlines())
    }
    patch_states = {
        name for name, _start, _end in _iter_papyrus_states(_patch_source(script_name).splitlines())
    }
    assert patch_states <= skeleton_states


@pytest.mark.parametrize(("script_name", "case"), PATCH_CASES.items())
def test_patch_supplies_every_repaired_member(script_name: str, case: tuple[str, set[tuple[str, str]]]):
    _pex_filename, expected_members = case
    patch_lines = _patch_source(script_name).splitlines()

    found: set[tuple[str, str]] = {
        (kind, name)
        for kind, name, _start, _end in _iter_top_level_papyrus_members(patch_lines)
    }
    for state_name, start, end in _iter_papyrus_states(patch_lines):
        found |= {
            (kind, name)
            for kind, name, _start, _end in _iter_papyrus_members_in_state(patch_lines, start, end)
        }

    assert expected_members <= found


def test_bait_mine_script_polling_loop_terminates_on_unload():
    """Regression guard: the coordinator required MTNM01_BaitMineScript's recheck
    loop to terminate cleanly rather than orphan itself. FO4 has no
    RegisterForSingleUpdate/OnUpdate, so the recheck runs on StartTimer/OnTimer
    (timer ID 1); OnUnload must CancelTimer(1) or the timer keeps firing after the
    reference unloads."""
    patch = _patch_source("MTNM01_BaitMineScript")
    assert "Event OnUnload()" in patch
    assert "CancelTimer(1)" in patch
    assert "RegisterForSingleUpdate(" not in patch
    assert "Event OnUpdate(" not in patch


def test_deathclaw_friend_perk_script_consumes_quest_gate_property():
    """Regression guard: the declared MTNM01_Mayhem quest property must actually
    gate the perk entry (IsRunning()), not just the companion keyword — otherwise
    the mandatory property is left silently unread, and the perk could fire while
    the timed event quest isn't running."""
    patch = _patch_source("MTNM01_DeathclawFriendPerkScript")
    assert "MTNM01_Mayhem.IsRunning()" in patch


def _on_effect_finish_body(patch: str) -> str:
    lines = patch.splitlines()
    start = next(i for i, line in enumerate(lines) if "Event OnEffectFinish" in line)
    end = next(i for i in range(start, len(lines)) if lines[i].strip() == "EndEvent")
    return "\n".join(lines[start : end + 1])


@pytest.mark.parametrize(
    ("script_name", "timer_id"),
    [("FirecrackerWhiskey_BurnAttackerScript", "1"), ("OnHitByMeleeCastSpell", "TimerID")],
)
def test_hit_cooldown_scripts_cancel_timer_on_effect_finish(script_name: str, timer_id: str):
    """Regression guard: rows 4 and 14 StartTimer a cooldown from OnHit, same as
    sibling rows 5 and 12 (MeatSweatsScript, MutationTriggerMeleeExplosionScript).
    Both siblings CancelTimer in OnEffectFinish; these two originally didn't,
    leaving a pending timer uncancelled on early effect removal (reviewer finding)."""
    finish_body = _on_effect_finish_body(_patch_source(script_name))
    assert f"CancelTimer({timer_id})" in finish_body


@pytest.mark.parametrize(("script_name", "case"), PATCH_CASES.items())
def test_production_merge_native_compiles_for_fo4(script_name: str, case: tuple[str, set[tuple[str, str]]]):
    base_source = _fo4_base_source()
    if base_source is None:
        pytest.skip("FO4 base Papyrus sources unavailable")
    if not PARENT_SOURCE_DIR.is_dir() or not DEPLOYED_SCRIPTS_DIR.is_dir():
        pytest.skip("SeventySix generated/deployed script directories unavailable")

    pex_filename, _expected_members = case
    result = compile_psc(
        _merged_production_source(script_name, pex_filename),
        imports=[str(base_source), str(PARENT_SOURCE_DIR), str(DEPLOYED_SCRIPTS_DIR)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=f"{script_name}.psc",
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None
