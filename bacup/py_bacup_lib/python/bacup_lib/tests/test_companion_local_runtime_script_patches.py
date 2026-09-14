from __future__ import annotations

import os
from pathlib import Path

import pytest

from bacup_lib.workflows.unified import (
    _augment_fo76_to_fo4_script_skeleton,
    _iter_top_level_papyrus_members,
    _merge_script_method_patches,
    _script_patch_source,
    _script_relative_path,
)
from creation_lib.pex.native_runtime import compile_psc


REPO_ROOT = Path(__file__).resolve().parents[5]
SOURCE_ROOT = REPO_ROOT / "mods" / "SeventySix" / "Scripts" / "Source" / "User"

PATCH_CASES = {
    "CompanionRQStageHandlerScript": {
        "oninit",
        "quest.onstageset",
        "applyradiantqueststagedatum",
    },
    "DefaultQuestTriggerRespawnVIPScript": {
        "onquestinit",
        "onquestshutdown",
        "updateplayervipstatus",
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


def _member_name_list(source: str) -> list[str]:
    return [
        name
        for _kind, name, _start, _end in _iter_top_level_papyrus_members(
            source.splitlines()
        )
    ]


def _merged_source(script_name: str) -> str:
    source_path = SOURCE_ROOT / _script_relative_path(script_name, ".psc")
    patch = _script_patch_source(script_name)
    assert source_path.is_file(), source_path
    assert patch is not None
    skeleton = _augment_fo76_to_fo4_script_skeleton(
        script_name, source_path.read_text(encoding="utf-8")
    )
    return _merge_script_method_patches(skeleton, patch)


@pytest.mark.parametrize(("script_name", "expected_members"), PATCH_CASES.items())
def test_companion_local_runtime_patch_merges_once(
    script_name: str, expected_members: set[str]
):
    patch = _script_patch_source(script_name)
    assert patch is not None
    assert not any(
        line.strip().lower().startswith("scriptname ") for line in patch.splitlines()
    )
    assert expected_members == _member_names(patch)

    merged = _merged_source(script_name)
    assert expected_members <= _member_names(merged)
    assert merged.lower().count("scriptname ") == 1
    for expected_member in expected_members:
        assert _member_name_list(merged).count(expected_member) == 1


def test_stage_handler_keeps_single_player_threshold_and_ignore_guards():
    merged = _merged_source("CompanionRQStageHandlerScript")

    unregister = 'UnregisterForRemoteEvent(RQData[index].BaseQuest, "OnStageSet")'
    register = 'RegisterForRemoteEvent(RQData[index].BaseQuest, "OnStageSet")'
    assert unregister in merged
    assert register in merged
    assert merged.find(unregister) < merged.find(register)
    assert "currentDatum.BaseQuest == akSender" in merged
    assert "currentDatum.Stage == auiStageID" in merged
    assert "GetValue(COMP_QuestCount) < currentDatum.RequiredQuestCount" in merged
    assert "player.GetValue(currentDatum.IgnoreOnPlayerActorValue)" in merged
    assert (
        "player.SetValue(currentDatum.PlayerActorValueToSet, currentDatum.ValueToSet)"
        in merged
    )


def test_vip_bridge_adds_and_removes_the_single_player():
    merged = _merged_source("DefaultQuestTriggerRespawnVIPScript")

    assert "UpdatePlayerVIPStatus(True)" in merged
    assert "UpdatePlayerVIPStatus(False)" in merged
    assert merged.splitlines().count("ObjectReference[] PlayerVIPTriggerCache") == 1
    assert "PlayerVIPTriggerCache = new ObjectReference[0]" in merged
    assert "PlayerVIPTriggerCache.Add(triggerReference)" in merged
    assert "PlayerVIPTriggerCache[index] as DefaultTriggerRespawnActorGroup" in merged
    assert "trigger.AddPlayerAsVIP(player)" in merged
    assert "trigger.RemovePlayerAsVIP(player)" in merged
    assert "PlayerVIPTriggerCache = None" in merged
    assert "QuestTarget.Start()" not in merged


def test_vip_cache_declaration_is_exact_keyed_and_idempotent():
    script_name = "DefaultQuestTriggerRespawnVIPScript"
    source_path = SOURCE_ROOT / _script_relative_path(script_name, ".psc")
    raw = source_path.read_text(encoding="utf-8")

    augmented = _augment_fo76_to_fo4_script_skeleton(script_name, raw)
    assert augmented.splitlines().count("ObjectReference[] PlayerVIPTriggerCache") == 1
    assert _augment_fo76_to_fo4_script_skeleton(script_name, augmented) == augmented
    assert _augment_fo76_to_fo4_script_skeleton("UnrelatedScript", raw) == raw


def test_vip_shutdown_uses_cached_references_after_aliases_clear():
    patch = _script_patch_source("DefaultQuestTriggerRespawnVIPScript")
    assert patch is not None

    shutdown_body = patch.split("Event OnQuestShutdown()", 1)[1].split("EndEvent", 1)[0]
    remove_body = patch.split("ElseIf PlayerVIPTriggerCache != None", 1)[1].split(
        "\nEndFunction", 1
    )[0]

    assert "ActorGroupTriggers" not in shutdown_body
    assert "ActorGroupTriggers" not in remove_body
    assert "PlayerVIPTriggerCache[index]" in remove_body
    assert shutdown_body.find("UpdatePlayerVIPStatus(False)") < shutdown_body.find(
        "PlayerVIPTriggerCache = None"
    )


@pytest.mark.parametrize("script_name", PATCH_CASES)
def test_companion_local_runtime_patch_native_compiles_for_fo4(script_name: str):
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
