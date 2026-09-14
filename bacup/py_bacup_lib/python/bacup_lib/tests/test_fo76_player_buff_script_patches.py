from __future__ import annotations

import re
from pathlib import Path

import pytest

from bacup_lib.tests.test_terminal_fragment_script_patches import _fo4_base_source
from bacup_lib.workflows.unified import _merge_script_method_patches, _script_patch_source
from creation_lib.pex.native_runtime import compile_psc


SKELETONS = {
    "OnActivateCastSpell": """Scriptname OnActivateCastSpell extends ObjectReference
Bool Property PlayerOnly = True Auto
Bool Property SelfCast = True Auto
Spell Property SpellToCast Auto
""",
    "ATX_FortuneTellerMachineScript": """Scriptname ATX_FortuneTellerMachineScript extends ObjectReference
Int FortuneGrantedTimerID = 0
Actor activatingPlayer
LeveledItem Property pFortuneBooks Auto
Float Property TimeToDispense Auto
Spell Property SpellToCast Auto
Auto State FortuneStopped
EndState
State fortunestarted
    Event OnActivate(ObjectReference akActionRef)
    EndEvent
    Event OnTimer(Int aiTimerID)
    EndEvent
EndState
""",
    "ATX_BloodTransfusionPumpScript": """Scriptname ATX_BloodTransfusionPumpScript extends ObjectReference
Int SoundID = 0
Actor interactingPlayer
Message Property RechargingMessage Auto
Sound Property ActivateSound Auto
Spell Property BuffSpell Auto
Keyword Property CooldownKeyword Auto
Spell Property CooldownSpell Auto
Auto State Ready
    Event OnActivate(ObjectReference akActionRef)
    EndEvent
EndState
State processing
EndState
""",
    "Objects:XPD_AC_SlotMachineScript_WestVirginia": """Scriptname Objects:XPD_AC_SlotMachineScript_WestVirginia extends Objects:XPD_AC_SlotMachine
Form Property PrizeLoss Auto
Spell Property ATX_BuffLuck Auto
Keyword Property ATX_DispellFortifyLuck Auto
Form Property PrizeJackpot Auto
""",
}


def merged_source(name: str) -> str:
    patch = _script_patch_source(name)
    assert patch is not None
    return _merge_script_method_patches(SKELETONS[name], patch)


@pytest.mark.parametrize("name", SKELETONS)
def test_player_buff_patch_compiles_after_production_merge(name: str, tmp_path: Path):
    base_source = _fo4_base_source()
    if base_source is None:
        pytest.skip("FO4 Papyrus imports unavailable")
    namespace = tmp_path / "Objects"
    namespace.mkdir()
    (namespace / "XPD_AC_CasinoGame.psc").write_text(
        "Scriptname Objects:XPD_AC_CasinoGame extends ObjectReference\n"
        "Message Property MessageNoPower Auto\n"
    )
    (namespace / "XPD_AC_SlotMachine.psc").write_text(
        "Scriptname Objects:XPD_AC_SlotMachine extends Objects:XPD_AC_CasinoGame\n"
    )
    result = compile_psc(
        merged_source(name),
        imports=[str(tmp_path), str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=str(Path(*name.split(":")).with_suffix(".psc")),
    )
    assert result.ok, "\n".join(str(item) for item in result.diagnostics)
    assert result.pex_bytes


def state_source(source: str, name: str) -> str:
    match = re.search(rf"(?ims)^\s*(?:Auto\s+)?State\s+{name}\s*$.*?^\s*EndState\s*$", source)
    assert match
    return match.group()


def test_fortune_callback_returns_to_ready_from_both_saved_states():
    source = merged_source("ATX_FortuneTellerMachineScript")
    busy = state_source(source, "fortunestarted")
    assert "FinishFortune(aiTimerID)" in busy
    assert source.count("FinishFortune(aiTimerID)") == 2
    assert source.count("Function FinishFortune(") == 1
    assert 'GoToState("FortuneStopped")' in source
    assert "SpellToCast.Cast(activatingPlayer, activatingPlayer)" in source
    assert "activatingPlayer = None" in source
    assert "SpellToCast.Cast(Self" not in source


def test_transfusion_overrides_ready_state_and_suppresses_processing_activation():
    source = merged_source("ATX_BloodTransfusionPumpScript")
    ready = state_source(source, "Ready")
    busy = state_source(source, "processing")
    assert ready.count("TryTransfusion(akActionRef)") == 1
    assert "Event OnActivate(ObjectReference akActionRef)" in busy
    assert "TryTransfusion" not in busy
    assert "HasMagicEffectWithKeyword(CooldownKeyword)" in source
    assert "BuffSpell.Cast(interactingPlayer, interactingPlayer)" in source
    assert "CooldownSpell.Cast(interactingPlayer, interactingPlayer)" in source
    assert "Cast(Self" not in source


def test_activation_self_cast_uses_activator_and_preserves_aimed_spell_branch():
    source = merged_source("OnActivateCastSpell")
    assert "SpellToCast.Cast(akActionRef, akActionRef)" in source
    assert "SpellToCast.Cast(Self, akActionRef)" in source
    assert "If SelfCast" in source
    assert "PlayerOnly && akActionRef != Game.GetPlayer()" in source
