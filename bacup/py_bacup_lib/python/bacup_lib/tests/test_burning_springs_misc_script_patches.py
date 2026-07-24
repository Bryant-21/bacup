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
    "Burn_ArmoredDeathclawFrenzy": (
        "burn_armoreddeathclawfrenzy.pex",
        {
            ("event", "oneffectstart"),
            ("event", "oneffectfinish"),
            ("event", "ontimer"),
        },
    ),
    "Burn_Bounty_CustomRadBurst": (
        "burn_bounty_customradburst.pex",
        {
            ("event", "oneffectstart"),
            ("event", "oneffectfinish"),
            ("event", "onhit"),
            ("event", "ontimer"),
        },
    ),
    "Burn_HandlerCreateDeathclawScript": (
        "burn_handlercreatedeathclawscript.pex",
        {
            ("event", "oncombatstatechanged"),
            ("function", "spawndeathclaws"),
        },
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
def test_patch_declares_no_new_states(script_name: str):
    # None of these three scripts have any named states in their skeleton
    # (all-default-state stubs) — the merger cannot safely introduce a new
    # state, so the patch must declare none either.
    patch_states = {
        name for name, _start, _end in _iter_papyrus_states(_patch_source(script_name).splitlines())
    }
    assert patch_states == set()


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


def test_frenzy_cancels_both_armed_timers_on_effect_finish():
    """Regression guard (coordinator condition 1b): a periodic StartTimer/OnTimer
    poll loop that survives OnEffectFinish is a trap-ungrouped-Critical repeat —
    both the id-1 poll timer and the id-2 post-roar finalize timer must be
    cancelled when the effect is removed."""
    patch = _patch_source("Burn_ArmoredDeathclawFrenzy")
    finish_start = patch.index("Event OnEffectFinish")
    finish_end = patch.index("EndEvent", finish_start)
    body = patch[finish_start:finish_end]

    assert "CancelTimer(1)" in body
    assert "CancelTimer(2)" in body


def test_frenzy_uses_explicit_nonzero_timer_ids():
    """Regression guard (coordinator condition 1a / lesson #12): every
    StartTimer/OnTimer/CancelTimer call must use an explicit non-zero id."""
    patch = _patch_source("Burn_ArmoredDeathclawFrenzy")
    assert "StartTimer(5.0, 1)" in patch
    assert "StartTimer(idleLength, 2)" in patch
    assert "CancelTimer(0)" not in patch
    assert "StartTimer(5.0, 0)" not in patch


def test_frenzy_aggression_flip_is_none_guarded():
    """Regression guard: Aggression resolves FormID:null on the live record
    (translation-map gap), so the SetValue call must be guarded rather than
    assuming a live value."""
    patch = _patch_source("Burn_ArmoredDeathclawFrenzy")
    guard_index = patch.find("If Aggression")
    setvalue_index = patch.find("SetValue(Aggression, Frenzied)")
    assert guard_index != -1
    assert setvalue_index != -1
    assert guard_index < setvalue_index


def test_rad_burst_reregisters_only_from_timer_not_from_hit():
    """Regression guard (coordinator condition 2a): the cooldown lock only
    locks if RegisterForHitEvent is called from OnTimer (post-cooldown), never
    from inside OnHit itself — otherwise the cooldown gate doesn't actually
    gate anything."""
    patch = _patch_source("Burn_Bounty_CustomRadBurst")
    hit_start = patch.index("Event OnHit")
    hit_end = patch.index("EndEvent", hit_start)
    hit_body = patch[hit_start:hit_end]
    timer_start = patch.index("Event OnTimer")
    timer_end = patch.index("EndEvent", timer_start)
    timer_body = patch[timer_start:timer_end]

    assert "RegisterForHitEvent" not in hit_body
    assert "RegisterForHitEvent" in timer_body


def test_rad_burst_explosion_form_is_none_guarded():
    """Regression guard (coordinator condition 2c)."""
    patch = _patch_source("Burn_Bounty_CustomRadBurst")
    guard_index = patch.find("If ExplosionForm")
    place_index = patch.find("PlaceAtMe(ExplosionForm)")
    assert guard_index != -1
    assert place_index != -1
    assert guard_index < place_index


def test_rad_burst_cancels_cooldown_timer_on_effect_finish():
    """Regression guard (coordinator condition 2b)."""
    patch = _patch_source("Burn_Bounty_CustomRadBurst")
    finish_start = patch.index("Event OnEffectFinish")
    finish_end = patch.index("EndEvent", finish_start)
    body = patch[finish_start:finish_end]
    assert "CancelTimer(iCooldownTimerID)" in body


def test_handler_create_deathclaw_spawn_guard_latches_via_linked_array():
    """Regression guard (coordinator condition 4a): the one-shot re-entry guard
    on OnCombatStateChanged only works if SpawnDeathclaws actually appends the
    spawned actor to LinkedDeathclaws — otherwise combat re-entry can respawn
    the pack indefinitely."""
    patch = _patch_source("Burn_HandlerCreateDeathclawScript")
    assert "LinkedDeathclaws.Length == 0" in patch
    assert "LinkedDeathclaws.Add(newDeathclaw)" in patch


def test_handler_create_deathclaw_none_guards_present():
    """Regression guard (coordinator condition 4b): None-guards on
    Burn_LvlDeathclaw_Armored, Burn_RustRaiderFaction, NoStaggerAll, and
    Deathclaw_Roar."""
    patch = _patch_source("Burn_HandlerCreateDeathclawScript")
    assert "If !Burn_LvlDeathclaw_Armored" in patch
    assert "If Burn_RustRaiderFaction" in patch
    assert "If NoStaggerAll" in patch
    assert "If Deathclaw_Roar" in patch


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
