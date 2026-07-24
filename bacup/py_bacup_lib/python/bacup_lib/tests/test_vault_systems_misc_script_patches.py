from __future__ import annotations

import os
import re
from pathlib import Path

import pytest

from bacup_lib.workflows.unified import (
    _iter_papyrus_states,
    _iter_top_level_papyrus_members,
    _merge_script_method_patches,
    _script_patch_source,
    _script_relative_path,
)
from creation_lib.pex.native_runtime import compile_psc


REPO_ROOT = Path(__file__).resolve().parents[5]
SOURCE_ROOT = REPO_ROOT / "mods" / "SeventySix" / "Scripts" / "Source" / "User"
# VaultDefaultMultiStateActivator / VaultDefault2StateActivator are never
# record-bound directly (only descendants like V96_1_CryoPipeScript /
# VaultCircuitBreakerScript are VMAD-bound), so -- same situation as
# RestrictedAccessScript in w1-hand-scanner and VaultDefault1StateActivator in
# w2-vault-systems-activator-enable-disable-toggle-linked-ref -- neither parent was
# ever decompiled to Source/User nor deployed to mods/SeventySix/data/Scripts. Their
# only available compiled form is the raw FO76 client extraction (see the
# cross-cutting finding in contracts/w2-vault-systems-misc.md; escalated to the
# coordinator as a conversion-pipeline gap, not fixed here).
FO76_EXTRACTED_SCRIPTS = REPO_ROOT / "extracted" / "fo76" / "scripts" / "client"

# Every script patched by shard w2-vault-systems-misc. The other 4 rows in this
# shard's runbook resolved to non-defect/evidence-blocked (see
# bacup/docs/stub_restoration/contracts/w2-vault-systems-misc.md) and are
# intentionally not patched or covered here.
PATCH_CASES = (
    "V94_TrapElectricArcSystem",
    "V96_1_CryoPipeScript",
    "V96_TeleportAbilityScript",
    "VaultCircuitBreakerScript",
)

# The two patched scripts whose direct parent only resolves from the raw FO76
# client extraction (see module docstring above).
NEEDS_FO76_EXTRACTED_PARENT = {"V96_1_CryoPipeScript", "VaultCircuitBreakerScript"}


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


def _merged_source(script_name: str) -> str:
    source_path = SOURCE_ROOT / _script_relative_path(script_name, ".psc")
    patch = _script_patch_source(script_name)
    assert source_path.is_file(), source_path
    assert patch is not None
    return _merge_script_method_patches(
        source_path.read_text(encoding="utf-8"), patch
    )


def _state_block(source: str, state_name: str) -> str:
    """Return the text between `State <state_name>` and its matching `EndState`."""
    pattern = re.compile(
        rf"^[ \t]*(?:Auto\s+)?State\s+{re.escape(state_name)}\b[^\n]*\n"
        rf"(?P<body>.*?)"
        rf"^[ \t]*EndState\b",
        re.IGNORECASE | re.MULTILINE | re.DOTALL,
    )
    match = pattern.search(source)
    assert match is not None, f"state {state_name!r} not found in merged source"
    return match.group("body")


def _member_body(source: str, header: str, end_keyword: str) -> str:
    start = source.find(header)
    assert start != -1, f"{header!r} not found"
    end = source.find(end_keyword, start)
    assert end != -1, f"{end_keyword!r} not found after {header!r}"
    return source[start:end]


@pytest.mark.parametrize("script_name", PATCH_CASES)
def test_patch_exists_with_no_scriptname_line(script_name: str):
    patch = _script_patch_source(script_name)
    assert patch is not None
    assert not any(
        line.strip().lower().startswith("scriptname ") for line in patch.splitlines()
    )


@pytest.mark.parametrize("script_name", PATCH_CASES)
def test_patch_declares_no_states_beyond_the_hollow_skeleton(script_name: str):
    # The merger cannot safely introduce new states -- every state named in the
    # patch must already exist (empty) in the generated skeleton.
    source_path = SOURCE_ROOT / _script_relative_path(script_name, ".psc")
    assert source_path.is_file(), source_path
    skeleton_states = {
        name
        for name, _start, _end in _iter_papyrus_states(
            source_path.read_text(encoding="utf-8").splitlines()
        )
    }
    patch_states = {
        name
        for name, _start, _end in _iter_papyrus_states(
            _script_patch_source(script_name).splitlines()
        )
    }
    assert patch_states <= skeleton_states


@pytest.mark.parametrize("script_name", PATCH_CASES)
def test_merged_source_has_single_scriptname_line(script_name: str):
    merged = _merged_source(script_name)
    assert merged.lower().count("scriptname ") == 1


# --- V94_TrapElectricArcSystem ----------------------------------------------


def test_trap_electric_arc_system_patch_supplies_three_members():
    patch = _script_patch_source("V94_TrapElectricArcSystem")
    assert patch is not None
    members = {
        (kind, name)
        for kind, name, _start, _end in _iter_top_level_papyrus_members(
            patch.splitlines()
        )
    }
    assert ("event", "oninit") in members
    assert ("function", "localfiretrap") in members
    assert ("function", "localontimer") in members


def test_trap_electric_arc_system_bridges_target_distance_guarded():
    # TrapSystemTargetDistance is unbound (0.0) on V94_TrapElectricArcSource, the
    # one fully-authored record -- an unguarded assignment would zero out the
    # parent's sensible 256.0 default for it.
    patch = _script_patch_source("V94_TrapElectricArcSystem")
    assert "TrapSystemTargetDistance > 0.0" in patch
    assert "TargetDistance = TrapSystemTargetDistance" in patch


def test_trap_electric_arc_system_local_fire_trap_delegates_to_parent():
    merged = _merged_source("V94_TrapElectricArcSystem")
    fire_body = _member_body(merged, "Function LocalFireTrap()", "EndFunction")
    assert "StartTimer(ActiveTime, CONST_ActiveTimeTimerID)" in fire_body
    assert "parent.LocalFireTrap()" in fire_body


def test_trap_electric_arc_system_local_on_timer_only_clears_its_own_id():
    merged = _merged_source("V94_TrapElectricArcSystem")
    timer_body = _member_body(merged, "Function LocalOnTimer(", "EndFunction")
    assert "aiTimerID == CONST_ActiveTimeTimerID" in timer_body
    assert "IsActive = False" in timer_body


# --- V96_1_CryoPipeScript ----------------------------------------------------


def test_cryo_pipe_patch_supplies_three_members():
    patch = _script_patch_source("V96_1_CryoPipeScript")
    assert patch is not None
    members = {
        (kind, name)
        for kind, name, _start, _end in _iter_top_level_papyrus_members(
            patch.splitlines()
        )
    }
    assert ("function", "setanimationstatecryopipe") in members
    assert ("event", "oninit") in members
    assert ("event", "onactivate") in members


def test_cryo_pipe_set_animation_state_is_mutex_guarded():
    # Mirrors the parent's own lock_UpdateNetworkState idiom (see contract).
    patch = _script_patch_source("V96_1_CryoPipeScript")
    assert "While lock_SetAnimationStateCryoPipe" in patch
    assert "lock_SetAnimationStateCryoPipe = True" in patch
    assert "lock_SetAnimationStateCryoPipe = False" in patch


def test_cryo_pipe_on_init_starts_at_intact_index_zero():
    merged = _merged_source("V96_1_CryoPipeScript")
    init_body = _member_body(merged, "Event OnInit()", "EndEvent")
    assert "SetAnimationStateCryoPipe(0)" in init_body


def test_cryo_pipe_on_activate_is_terminal_and_destroys():
    merged = _merged_source("V96_1_CryoPipeScript")
    activate_body = _member_body(merged, "Event OnActivate(", "EndEvent")
    assert "If CryoPipeIsDestroyed" in activate_body
    assert "CryoPipeIsDestroyed = True" in activate_body
    assert "SetDestroyed(True)" in activate_body


# --- V96_TeleportAbilityScript -----------------------------------------------


def test_teleport_ability_patch_supplies_oneffectstart():
    patch = _script_patch_source("V96_TeleportAbilityScript")
    assert patch is not None
    members = {
        (kind, name)
        for kind, name, _start, _end in _iter_top_level_papyrus_members(
            patch.splitlines()
        )
    }
    assert ("event", "oneffectstart") in members


def test_teleport_ability_checks_suppression_before_blinking():
    patch = _script_patch_source("V96_TeleportAbilityScript")
    assert patch is not None
    assert "HasMagicEffect(V96_1_SuppressionEffect_ScriptSpell)" in patch
    assert "HasMagicEffect(V96_1_SuppressionEffect_Weapon)" in patch


def test_teleport_ability_resolves_group_index_before_moving():
    patch = _script_patch_source("V96_TeleportAbilityScript")
    assert patch is not None
    group_index = patch.find("teleportTriggerGroupIndex = 0")
    move_index = patch.find("akTarget.MoveTo(")
    assert group_index != -1
    assert move_index != -1
    assert group_index < move_index


def test_teleport_ability_guards_none_manager_and_none_linked_trigger():
    patch = _script_patch_source("V96_TeleportAbilityScript")
    assert patch is not None
    assert "TeleportManagerScript == None" in patch
    assert "linkedTrigger == None" in patch


# --- VaultCircuitBreakerScript ------------------------------------------------


def test_circuit_breaker_patch_supplies_onactivate_and_four_states():
    patch = _script_patch_source("VaultCircuitBreakerScript")
    assert patch is not None
    top_level = {
        (kind, name)
        for kind, name, _start, _end in _iter_top_level_papyrus_members(
            patch.splitlines()
        )
    }
    assert ("event", "onactivate") in top_level
    patch_states = {
        name for name, _start, _end in _iter_papyrus_states(patch.splitlines())
    }
    assert patch_states == {"opening", "open", "closed", "startsclosed"}


def test_circuit_breaker_deny_on_position_checked_before_lock_to_on_position():
    merged = _merged_source("VaultCircuitBreakerScript")
    activate_body = _member_body(merged, "Event OnActivate(", "EndEvent")
    deny_index = activate_body.find("turningOn && DenyOnPosition")
    lock_index = activate_body.find("!turningOn && LockToOnPosition")
    assert deny_index != -1
    assert lock_index != -1
    assert deny_index < lock_index


def test_circuit_breaker_opening_and_closed_collision_polarity_are_inverse():
    merged = _merged_source("VaultCircuitBreakerScript")
    opening = _state_block(merged, "opening")
    closed = _state_block(merged, "closed")
    assert "EnableLinkChain(TwoStateCollisionKeyword)" in opening
    assert "DisableLinkChain(TwoStateCollisionKeyword)" in opening
    assert "DisableLinkChain(TwoStateCollisionKeyword)" in closed
    assert "EnableLinkChain(TwoStateCollisionKeyword)" in closed
    # opening reaches "open" via InvertCollision?Enable:Disable; closed is the
    # exact inverse -- lock the branch order, not just presence of both calls.
    assert opening.find("If InvertCollision") < opening.find("EnableLinkChain")
    assert closed.find("If InvertCollision") < closed.find("DisableLinkChain")


def test_circuit_breaker_startsclosed_reconciles_without_animation():
    merged = _merged_source("VaultCircuitBreakerScript")
    starts_closed = _state_block(merged, "StartsClosed")
    assert "Event OnLoad()" in starts_closed
    assert 'GoToState("closed")' in starts_closed
    assert "PlayAnimation" not in starts_closed


def test_circuit_breaker_animation_calls_require_loaded_3d():
    merged = _merged_source("VaultCircuitBreakerScript")
    for state_name, animation_name in (("opening", "OpenAnim"), ("closed", "CloseAnim")):
        state = _state_block(merged, state_name)
        guard = f'If {animation_name} != "" && Is3DLoaded()'
        assert guard in state
        assert state.find(guard) < state.find(f"PlayAnimation({animation_name})")


def test_circuit_breaker_no_closing_state_is_authored():
    # The skeleton itself has no `closing` counterpart to `opening` -- respected
    # as given rather than invented (see contract).
    patch = _script_patch_source("VaultCircuitBreakerScript")
    assert patch is not None
    assert not re.search(r"State\s+closing\b", patch, re.IGNORECASE)


# --- compile verification -----------------------------------------------------


@pytest.mark.parametrize("script_name", PATCH_CASES)
def test_merged_patch_native_compiles_for_fo4(script_name: str):
    base_source = _fo4_base_source()
    if base_source is None:
        pytest.skip("FO4 base Papyrus sources unavailable")
    if (
        script_name in NEEDS_FO76_EXTRACTED_PARENT
        and not FO76_EXTRACTED_SCRIPTS.is_dir()
    ):
        pytest.skip(
            f"FO76 extracted client scripts unavailable ({script_name}'s parent source)"
        )

    imports = [str(base_source), str(SOURCE_ROOT)]
    if script_name in NEEDS_FO76_EXTRACTED_PARENT:
        imports.append(str(FO76_EXTRACTED_SCRIPTS))

    merged = _merged_source(script_name)
    result = compile_psc(
        merged,
        imports=imports,
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=str(_script_relative_path(script_name, ".psc")),
    )
    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None
