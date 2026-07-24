from __future__ import annotations

import os
import re
from pathlib import Path

import pytest

from bacup_lib.workflows.unified import (
    _iter_papyrus_states,
    _merge_script_method_patches,
    _script_patch_source,
    _script_relative_path,
)
from creation_lib.pex.native_runtime import compile_psc


REPO_ROOT = Path(__file__).resolve().parents[5]
SOURCE_ROOT = REPO_ROOT / "mods" / "SeventySix" / "Scripts" / "Source" / "User"
# VaultDefault1StateActivator is never record-bound directly (only descendants like
# VaultDotMatrixPrinterScript are VMAD-bound), so — same situation as
# RestrictedAccessScript in w1-hand-scanner — it was never decompiled to Source/User
# nor deployed to mods/SeventySix/data/Scripts. Its only available compiled form is
# the raw FO76 client extraction.
FO76_EXTRACTED_SCRIPTS = REPO_ROOT / "extracted" / "fo76" / "scripts" / "client"

# Every script patched by shard
# w2-vault-systems-activator-enable-disable-toggle-linked-ref. VaultDotMatrixPrinterScript's
# member is state-scoped ("waiting"); Vault79RaRaVentSoundScript's three members are
# all top-level (OnLoad/OnUnload/OnTimer). Rows 4/5 (Vault79ReactorSecurityActivateScript,
# Vault79ReactorVentilationScript) resolved to evidence-blocked after re-trace; the
# remaining 5 rows resolved to non-defect. See
# bacup/docs/stub_restoration/contracts/w2-vault-systems-activator-enable-disable-toggle-linked-ref.md
# for full per-row evidence — only the two scripts below are patched/covered here.
PATCH_CASES = (
    "VaultDotMatrixPrinterScript",
    "Vault79RaRaVentSoundScript",
)


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


@pytest.mark.parametrize("script_name", PATCH_CASES)
def test_patch_exists_with_no_scriptname_line(script_name: str):
    patch = _script_patch_source(script_name)
    assert patch is not None
    assert not any(
        line.strip().lower().startswith("scriptname ") for line in patch.splitlines()
    )


@pytest.mark.parametrize("script_name", PATCH_CASES)
def test_patch_declares_no_states_beyond_the_hollow_skeleton(script_name: str):
    # The merger cannot safely introduce new states — every state named in the
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


def test_dot_matrix_printer_waiting_state_shows_inactive_message_on_activate():
    merged = _merged_source("VaultDotMatrixPrinterScript")
    waiting = _state_block(merged, "waiting")
    assert "Event OnActivate(ObjectReference akActionRef)" in waiting
    assert "DotMatrixPrinterMessageNotActive.Show()" in waiting


def test_dot_matrix_printer_active_state_is_untouched_by_the_patch():
    # VaultDefault1StateActivator's "Active" state already plays the printer's
    # animation via inheritance (confirmed by decompile in the contract) — the
    # patch must not add anything there.
    patch = _script_patch_source("VaultDotMatrixPrinterScript")
    assert patch is not None
    assert not re.search(r"State\s+active\b", patch, re.IGNORECASE)


def test_rara_vent_sound_declares_a_nonzero_fallback_helper_per_cue_family():
    # CRITICAL fix (SHARD_PROTOCOL.md lesson #12, caught before review): wiki,
    # verbatim, "Timers on ObjectReference scripts must have an explicit aiTimerID
    # parameter, the default implicit timer ID 0 will never start." Every sampled
    # live record binds DustDelayTimerId/BugKillDelayTimerId/KnifeDelayTimerId to
    # the Papyrus Int default, 0 — so a raw StartTimer(length, XxxDelayTimerId)
    # call is a silent no-op on every one of the 25 live instances. Each cue family
    # must instead go through a helper that falls back to a distinct nonzero
    # constant when the bound Id reads 0.
    patch = _script_patch_source("Vault79RaRaVentSoundScript")
    assert patch is not None
    assert "Int Function EffectiveDustTimerId()" in patch
    assert "Int Function EffectiveBugKillTimerId()" in patch
    assert "Int Function EffectiveKnifeTimerId()" in patch
    assert re.search(r"DustDelayTimerId\s*>\s*0", patch)
    assert re.search(r"BugKillDelayTimerId\s*>\s*0", patch)
    assert re.search(r"KnifeDelayTimerId\s*>\s*0", patch)
    # The fallback constants must be nonzero and distinct from each other.
    fallbacks = {
        int(m.group(1))
        for m in re.finditer(r"Return DustDelayTimerId|Return (\d+)", patch)
        if m.group(1) is not None
    }
    assert 0 not in fallbacks
    assert len(fallbacks) == 3, f"expected 3 distinct nonzero fallbacks, got {fallbacks}"


def test_rara_vent_sound_no_call_site_passes_a_bare_timer_id_property():
    # Regression guard for the exact bug caught before review: StartTimer,
    # CancelTimer, and the OnTimer id-match must ALL go through the
    # EffectiveXxxTimerId() helpers, never the raw (always-0-on-live-data)
    # DustDelayTimerId/BugKillDelayTimerId/KnifeDelayTimerId property directly —
    # a single missed call site would silently reintroduce the no-op bug at that
    # one site even with the helpers correctly defined elsewhere.
    patch = _script_patch_source("Vault79RaRaVentSoundScript")
    assert patch is not None
    assert "StartTimer(DustDelayTimerLength, DustDelayTimerId)" not in patch
    assert "StartTimer(BugKillDelayTimerLength, BugKillDelayTimerId)" not in patch
    assert "StartTimer(KnifeDelayTimerLength, KnifeDelayTimerId)" not in patch
    assert "CancelTimer(DustDelayTimerId)" not in patch
    assert "CancelTimer(BugKillDelayTimerId)" not in patch
    assert "CancelTimer(KnifeDelayTimerId)" not in patch
    assert "aiTimerID == DustDelayTimerId" not in patch
    assert "aiTimerID == BugKillDelayTimerId" not in patch
    assert "aiTimerID == KnifeDelayTimerId" not in patch
    assert patch.count("StartTimer(") == 6  # OnLoad arms 3, OnTimer reschedules 3
    assert patch.count("CancelTimer(") == 3
    # Each helper's declaration plus its 4 call sites (OnLoad StartTimer, OnUnload
    # CancelTimer, OnTimer id-match, OnTimer re-arm StartTimer) = 5 occurrences.
    assert patch.count("EffectiveDustTimerId()") == 5
    assert patch.count("EffectiveBugKillTimerId()") == 5
    assert patch.count("EffectiveKnifeTimerId()") == 5


def test_rara_vent_sound_onload_arms_a_timer_per_bound_cue():
    patch = _script_patch_source("Vault79RaRaVentSoundScript")
    assert patch is not None
    onload = patch[patch.index("Event OnLoad(") : patch.index("Event OnUnload(")]
    assert "mySound != None" in onload
    assert "StartTimer(DustDelayTimerLength, EffectiveDustTimerId())" in onload
    assert "myBugKillSound != None" in onload
    assert "StartTimer(BugKillDelayTimerLength, EffectiveBugKillTimerId())" in onload
    assert "myKnifeSound != None || myKnifeVSFleshSound != None" in onload
    assert "StartTimer(KnifeDelayTimerLength, EffectiveKnifeTimerId())" in onload


def test_rara_vent_sound_onunload_cancels_every_timer_onload_could_have_armed():
    # Coordinator-directed lifecycle fix: up to 24 simultaneously-loaded placed
    # instances each arming up to 3 perpetual OnTimer loops means dozens of timers
    # that must not survive a cell unload. OnUnload must mirror OnLoad's exact
    # three guards (and the same Effective*TimerId() calls) so arm/cancel stay
    # symmetric across repeated load/unload cycles.
    patch = _script_patch_source("Vault79RaRaVentSoundScript")
    assert patch is not None
    onunload = patch[patch.index("Event OnUnload(") : patch.index("Event OnTimer(")]
    assert "mySound != None" in onunload
    assert "CancelTimer(EffectiveDustTimerId())" in onunload
    assert "myBugKillSound != None" in onunload
    assert "CancelTimer(EffectiveBugKillTimerId())" in onunload
    assert "myKnifeSound != None || myKnifeVSFleshSound != None" in onunload
    assert "CancelTimer(EffectiveKnifeTimerId())" in onunload


def test_rara_vent_sound_ontimer_dispatches_and_reschedules_each_cue():
    patch = _script_patch_source("Vault79RaRaVentSoundScript")
    assert patch is not None
    ontimer = patch[patch.index("Event OnTimer(") :]
    assert "aiTimerID == EffectiveDustTimerId() && mySound != None" in ontimer
    assert "mySound.Play(Self)" in ontimer
    assert "aiTimerID == EffectiveBugKillTimerId() && myBugKillSound != None" in ontimer
    assert "myBugKillSound.Play(myBugKillMarker)" in ontimer
    assert "myBugKillSound.Play(Self)" in ontimer
    assert "aiTimerID == EffectiveKnifeTimerId()" in ontimer
    assert "myKnifeSound.Play(Self)" in ontimer
    assert "myKnifeVSFleshSound.Play(Self)" in ontimer
    # Loop: every branch re-arms its own timer rather than firing once.
    assert ontimer.count("StartTimer(") == 3


def test_rara_vent_sound_ontimer_reguards_bound_state_not_just_id_match():
    # Residual collision mitigation (now secondary to the 0-never-starts fix, but
    # still load-bearing): a CK author on an unsampled record could still set two
    # cues' bound Id properties to the same positive value, or to a value that
    # collides with another cue's 1/2/3 fallback constant. Each OnTimer branch
    # must re-check its own Sound-bound guard (not just the id) so a collision
    # degrades to "whichever bound cue matches first plays" instead of a
    # None-reference call.
    patch = _script_patch_source("Vault79RaRaVentSoundScript")
    assert patch is not None
    assert "aiTimerID == EffectiveDustTimerId() && mySound != None" in patch
    assert "aiTimerID == EffectiveBugKillTimerId() && myBugKillSound != None" in patch
    assert (
        "aiTimerID == EffectiveKnifeTimerId() && (myKnifeSound != None || "
        "myKnifeVSFleshSound != None)" in patch
    )


@pytest.mark.parametrize("script_name", PATCH_CASES)
def test_merged_source_has_single_scriptname_line(script_name: str):
    merged = _merged_source(script_name)
    assert merged.lower().count("scriptname ") == 1


@pytest.mark.parametrize("script_name", PATCH_CASES)
def test_merged_patch_native_compiles_for_fo4(script_name: str):
    base_source = _fo4_base_source()
    if base_source is None:
        pytest.skip("FO4 base Papyrus sources unavailable")
    if not FO76_EXTRACTED_SCRIPTS.is_dir():
        pytest.skip(
            "FO76 extracted client scripts unavailable (VaultDefault1StateActivator source)"
        )

    merged = _merged_source(script_name)
    result = compile_psc(
        merged,
        # FO4 base first, then the mod's own generated custom-parent source, then
        # the raw FO76 client extraction so VaultDefault1StateActivator resolves
        # from its compiled bytecode (never record-bound directly, so it was never
        # decompiled to Source/User or deployed — same situation as
        # RestrictedAccessScript in w1-hand-scanner).
        imports=[str(base_source), str(SOURCE_ROOT), str(FO76_EXTRACTED_SCRIPTS)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=str(_script_relative_path(script_name, ".psc")),
    )
    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None
