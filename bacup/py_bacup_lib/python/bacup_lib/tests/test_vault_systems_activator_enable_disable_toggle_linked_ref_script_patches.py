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

# Stateful Vault-system repairs covered by this focused merger/compiler suite.
PATCH_CASES = (
    "V94_3_GearRoomIDCardCaseScript",
    "VaultDotMatrixPrinterScript",
    "Vault79ReactorVentilationScript",
    "Vault79RaRaVentSoundScript",
    "Vault79SentryBotDeathScript",
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
    merged = _merge_script_method_patches(
        source_path.read_text(encoding="utf-8"), patch
    )
    assert _merge_script_method_patches(merged, patch) == merged
    return merged


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


def test_reactor_ventilation_restores_the_bound_local_sequence_and_stage_handoff():
    patch = _script_patch_source("Vault79ReactorVentilationScript")
    assert patch is not None
    assert "IsActivationBlocked()" in patch
    assert "BlockActivation(True, True)" in patch
    assert "myKlaxonDummy.Activate(akActionRef)" in patch
    assert patch.count("myDoorDummy.Activate(akActionRef)") == 2
    assert "Utility.Wait(4.0)" in patch
    assert "myFanOffEnableTrigger.Disable()" in patch
    assert "myFanOnEnableTrigger.Enable()" in patch
    assert "myKillTrigger.Enable()" in patch
    assert "myRadDisableTrigger.Disable()" in patch
    assert "myVentilationSoundRef.Enable()" in patch
    assert "myMachineHumRef.Enable()" in patch
    assert "myFanOnSound.Play(myFanOnSoundRef)" in patch
    assert "myFanOnSound2.Play(myFanOnSoundRef)" in patch
    assert "mySecurityDummy.Activate(akActionRef)" in patch
    assert 'Game.GetFormFromFile(0x0054EDB9, "SeventySix.esm") as Quest' in patch
    assert "secretsRevealed.IsStageDone(400)" in patch
    assert "!secretsRevealed.IsStageDone(450)" in patch
    assert "secretsRevealed.SetStage(450)" in patch


def test_reactor_ventilation_is_player_only_and_one_shot():
    patch = _script_patch_source("Vault79ReactorVentilationScript")
    assert patch is not None
    assert "akActionRef != Game.GetPlayer() || IsActivationBlocked()" in patch
    assert "BlockActivation(True, True)" in patch
    assert "BlockActivation(False)" not in patch


def test_reactor_ventilation_warns_before_enabling_the_lethal_transition():
    patch = _script_patch_source("Vault79ReactorVentilationScript")
    assert patch is not None
    wait = patch.index("Utility.Wait(4.0)")
    assert patch.index("myKlaxonDummy.Activate(akActionRef)") < wait
    assert patch.index("myDoorDummy.Activate(akActionRef)") < wait
    assert wait < patch.index("myKillTrigger.Enable()")
    assert wait < patch.index("mySecurityDummy.Activate(akActionRef)")
    stage = patch.index("secretsRevealed.SetStage(450)")
    assert patch.index("myKillTrigger.Enable()") < stage
    assert patch.index("mySecurityDummy.Activate(akActionRef)") < stage
    assert stage < patch.rindex("myDoorDummy.Activate(akActionRef)")
    assert stage < patch.rindex("myKlaxonDummy.Activate(akActionRef)")


def test_sentry_bot_death_sets_both_supported_single_player_signal_carriers():
    merged = _merged_source("Vault79SentryBotDeathScript")
    assert merged.count("Event OnDeath(Actor akKiller)") == 1
    assert "If myActorValue == None" in merged
    actor_write = merged.index("SetValue(myActorValue, 1.0)")
    player_guard = merged.index("playerRef != None && playerRef != Self")
    player_write = merged.index("playerRef.SetValue(myActorValue, 1.0)")
    assert actor_write < player_guard < player_write


def test_v94_card_case_grants_the_bound_card_before_advancing_the_bound_stage():
    patch = _script_patch_source("V94_3_GearRoomIDCardCaseScript")
    assert patch is not None
    on_activate = patch[: patch.index("Function InitializeCardCase()")]
    grant = on_activate.index(
        "playerRef.AddItem(myV94_3QI.V94_3_SecurityIDCard, 1, True)"
    )
    handoff = on_activate.index("CompleteSecurityIdPickup()", grant)
    assert grant < handoff
    assert "ITMKeycardPickup.Play(Self)" in on_activate[grant:handoff]
    assert "V94_3_GearRoomIDCardCaseMessageHasCard.Show()" in on_activate

    complete = patch[
        patch.index("Function CompleteSecurityIdPickup()") :
        patch.index("Function FinishActivation()")
    ]
    stage = complete.index("myV94_3QI.SetStage(completionStage)")
    objective_completed = complete.index("myV94_3QI.SetObjectiveCompleted(20)")
    objective_displayed = complete.index("myV94_3QI.SetObjectiveDisplayed(21)")
    assert stage < objective_completed < objective_displayed


def test_v94_card_case_uses_the_existing_busy_state_and_unlocks_every_exit():
    patch = _script_patch_source("V94_3_GearRoomIDCardCaseScript")
    assert patch is not None
    on_activate = patch[: patch.index("Function InitializeCardCase()")]
    block = on_activate.index("BlockActivation(True, True)")
    busy = on_activate.index('GoToState("processingactivation")')
    assert block < busy
    assert on_activate.count("FinishActivation()") == 3
    assert on_activate.count("Return") == 3
    processing = _state_block(patch, "processingactivation")
    assert "Event OnActivate(ObjectReference akActionRef)" in processing
    finish = patch[patch.index("Function FinishActivation()") :]
    ready = finish.index('GoToState("waitingforactivation")')
    unblock = finish.index("BlockActivation(False)")
    assert ready < unblock


def test_v94_card_case_initializes_from_the_exact_live_quest_and_link_chain():
    patch = _script_patch_source("V94_3_GearRoomIDCardCaseScript")
    assert patch is not None
    assert (
        'Game.GetFormFromFile(0x0046F0DD, "SeventySix.esm") as '
        "V94_3_VaultMissionQuestScript_Access"
    ) in patch
    assert "myFauxIDCards = GetLinkedRefChain()" in patch
    assert "myFauxIDCards[myFauxIDCardIndex].Disable()" in patch


def test_rara_vent_sound_declares_a_nonzero_fallback_helper_per_cue_family():
    # Per the CK wiki, an ObjectReference timer with the implicit ID 0 never
    # starts. Every sampled live record binds DustDelayTimerId/BugKillDelayTimerId/
    # KnifeDelayTimerId to the Papyrus Int default 0, so a raw
    # StartTimer(length, XxxDelayTimerId) is a silent no-op on all 25 live
    # instances. Each cue family goes through a helper that falls back to a
    # distinct nonzero constant when the bound Id reads 0.
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
    # StartTimer, CancelTimer, and the OnTimer id-match must all use the
    # EffectiveXxxTimerId() helpers, never the raw (0 on live data)
    # DustDelayTimerId/BugKillDelayTimerId/KnifeDelayTimerId properties; one missed
    # call site is a no-op timer there.
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
    # Up to 24 loaded instances each arm up to 3 perpetual OnTimer loops, and none
    # may survive a cell unload. OnUnload mirrors OnLoad's three guards and
    # Effective*TimerId() calls so arm/cancel stay symmetric across load/unload
    # cycles.
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
    # Two cues can share an Id; each OnTimer branch re-checks its own Sound guard so a
    # collision plays the first matching cue instead of calling on None.
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
