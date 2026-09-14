from __future__ import annotations

from collections import Counter
from pathlib import Path

import pytest

from bacup_lib.tests.test_terminal_fragment_script_patches import _fo4_base_source
from bacup_lib.workflows.unified import (
    _iter_top_level_papyrus_members,
    _merge_script_method_patches,
    _script_patch_source,
)
from creation_lib.pex.native_runtime import compile_psc


CONTROLLER = "W05_MQR_204P_QuestScript"
CODE_NOTE = "W05_MQR_Vault79CodeNoteScript"
KEYPAD_ALIAS = "W05_MQR_Vault79KeypadAliasScript"
KEYPAD_CONTROLLER = "W05_Vaut79EntranceKeypadScript"

SKELETON = """Scriptname W05_MQR_204P_QuestScript Extends Quest

referencealias Property currentPlayer Auto mandatory
Int Property TalkToLouStage = 300 Auto
Int Property FreeLouStage = 200 Auto
perk Property W05_MQR_204P_FreeLou_Perk Auto mandatory
actorvalue Property W05_MQ00_CodeAV Auto mandatory
book Property W05_MQR_Vault79CodeBackupNote Auto mandatory
book Property W05_MQR_Vault79CodeNote Auto mandatory
"""

CODE_NOTE_SKELETON = """Scriptname W05_MQR_Vault79CodeNoteScript Extends ObjectReference

actorvalue Property W05_MQ00_CodeAV Auto mandatory
"""

def _member_names(source: str) -> list[str]:
    return [
        name
        for kind, name, _start, _end in _iter_top_level_papyrus_members(
            source.splitlines()
        )
        if kind in {"function", "event"}
    ]


def _member_body(source: str, member_name: str) -> str:
    start, end = next(
        (start, end)
        for _kind, name, start, end in _iter_top_level_papyrus_members(
            source.splitlines()
        )
        if name == member_name.lower()
    )
    return "\n".join(source.splitlines()[start : end + 1])


def _merged_source() -> str:
    patch = _script_patch_source(CONTROLLER)
    assert patch is not None
    return _merge_script_method_patches(SKELETON, patch)


def _merged_code_note_source() -> str:
    patch = _script_patch_source(CODE_NOTE)
    assert patch is not None
    return _merge_script_method_patches(CODE_NOTE_SKELETON, patch)


def test_lou_perk_controller_lifecycle_merges_once_and_is_idempotent():
    patch = _script_patch_source(CONTROLLER)
    assert patch is not None
    assert not any(
        line.strip().lower().startswith("scriptname ") for line in patch.splitlines()
    )

    merged = _merged_source()
    members = _member_names(merged)
    assert Counter(members) == Counter(
        {
            "getquestplayer": 1,
            "addfreelouperk": 1,
            "removefreelouperk": 1,
            "syncfreelouperk": 1,
            "ensurevault79codenote": 1,
            "registerforplayerload": 1,
            "unregisterforplayerload": 1,
            "onquestinit": 1,
            "onstageset": 1,
            "actor.onplayerloadgame": 1,
            "onquestshutdown": 1,
            "onreset": 1,
        }
    )
    assert _merge_script_method_patches(merged, patch) == merged


def test_lou_perk_controller_stage_and_load_contract_is_bounded():
    merged = _merged_source()
    stage_set = _member_body(merged, "onstageset")
    load = _member_body(merged, "actor.onplayerloadgame")
    init = _member_body(merged, "onquestinit")
    shutdown = _member_body(merged, "onquestshutdown")
    reset = _member_body(merged, "onreset")
    sync = _member_body(merged, "syncfreelouperk")
    add = _member_body(merged, "addfreelouperk")
    remove = _member_body(merged, "removefreelouperk")

    assert "auiStageID == FreeLouStage" in stage_set
    assert "auiStageID >= TalkToLouStage" in stage_set
    assert stage_set.count("AddFreeLouPerk()") == 1
    assert stage_set.count("RemoveFreeLouPerk()") == 1
    assert "RegisterForRemoteEvent(playerRef, \"OnPlayerLoadGame\")" in merged
    assert "Event Actor.OnPlayerLoadGame(Actor akSender)" in load
    assert "akSender == Game.GetPlayer()" in load
    assert "SyncFreeLouPerk()" in load
    assert "EnsureVault79CodeNote()" in load
    assert "RegisterForPlayerLoad()" in init
    assert "EnsureVault79CodeNote()" in init
    assert "SyncFreeLouPerk()" in init
    assert "RemoveFreeLouPerk()" in shutdown
    assert "UnregisterForPlayerLoad()" in shutdown
    assert "RemoveFreeLouPerk()" in reset
    assert "UnregisterForPlayerLoad()" in reset
    assert "IsStageDone(FreeLouStage) && !IsStageDone(TalkToLouStage)" in sync
    assert "!playerRef.HasPerk(W05_MQR_204P_FreeLou_Perk)" in add
    assert "playerRef.HasPerk(W05_MQR_204P_FreeLou_Perk)" in remove
    assert "Lev" not in merged


def test_vault79_code_and_primary_note_delivery_is_source_bounded_and_idempotent():
    merged = _merged_source()
    ensure = _member_body(merged, "ensurevault79codenote")
    stage_set = _member_body(merged, "onstageset")

    sentinel = "playerRef.GetValue(W05_MQ00_CodeAV) < 0.0"
    generate = "Utility.RandomInt(100000, 999999)"
    inventory_guard = (
        "playerRef.GetItemCount(W05_MQR_Vault79CodeNote) == 0"
    )
    deliver = "playerRef.AddItem(W05_MQR_Vault79CodeNote, 1, False)"

    assert sentinel in ensure
    assert generate in ensure
    assert inventory_guard in ensure
    assert deliver in ensure
    assert ensure.index(sentinel) < ensure.index(generate) < ensure.index(deliver)
    assert ensure.count(generate) == 1
    assert ensure.count(deliver) == 1
    assert "AddItem(W05_MQR_Vault79CodeBackupNote" not in ensure
    assert "auiStageID == 100" in stage_set
    assert stage_set.count("EnsureVault79CodeNote()") == 1


def test_code_note_read_uses_explicit_fo4_display_substitution_without_mutation():
    patch = _script_patch_source(CODE_NOTE)
    assert patch is not None
    assert not any(
        line.strip().lower().startswith("scriptname ") for line in patch.splitlines()
    )

    merged = _merged_code_note_source()
    assert Counter(_member_names(merged)) == Counter({"onread": 1})
    body = _member_body(merged, "onread")
    assert "Event OnRead()" in body
    assert "playerRef.GetValue(W05_MQ00_CodeAV) as Int" in body
    assert "vault79Code >= 100000 && vault79Code <= 999999" in body
    assert 'Debug.Notification("Vault 79 keypad code: " + vault79Code)' in body
    assert "SetValue(" not in body
    assert "RandomInt(" not in body
    assert _merge_script_method_patches(merged, patch) == merged


def test_keypad_activation_and_note_possession_fallbacks_are_retired():
    assert _script_patch_source(KEYPAD_ALIAS) is None
    assert _script_patch_source(KEYPAD_CONTROLLER) is None


def test_lou_perk_controller_merged_full_source_native_compiles_for_fo4(
    tmp_path: Path,
):
    base_source = _fo4_base_source()
    if base_source is None:
        pytest.skip("FO4 base Papyrus sources unavailable")

    result = compile_psc(
        _merged_source(),
        imports=[str(base_source), str(tmp_path)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=f"{CONTROLLER}.psc",
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None

def test_code_note_merged_full_source_native_compiles_for_fo4(tmp_path: Path):
    base_source = _fo4_base_source()
    if base_source is None:
        pytest.skip("FO4 base Papyrus sources unavailable")

    result = compile_psc(
        _merged_code_note_source(),
        imports=[str(base_source), str(tmp_path)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=f"{CODE_NOTE}.psc",
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None
