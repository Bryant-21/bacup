from __future__ import annotations

import re
from pathlib import Path

from bacup_lib.tests.test_terminal_fragment_script_patches import _fo4_base_source
from bacup_lib.workflows.unified import (
    _iter_top_level_papyrus_members,
    _merge_script_method_patches,
    _script_patch_source,
    _script_relative_path,
)
from creation_lib.pex.native_runtime import compile_psc


REPO_ROOT = Path(__file__).resolve().parents[5]
SCRIPT_NAME = "Fragments:Quests:QF_BS01_Arms_005CB547"
SOURCE_ROOT = REPO_ROOT / "mods" / "SeventySix" / "Scripts" / "Source" / "User"

EXPECTED_MEMBERS = {
    "fragment_stage_0010_item_00",
    "fragment_stage_0100_item_00",
    "fragment_stage_0200_item_00",
    "fragment_stage_0300_item_00",
    "fragment_stage_0310_item_00",
    "fragment_stage_0320_item_00",
    "fragment_stage_0330_item_00",
    "fragment_stage_0350_item_00",
    "fragment_stage_0375_item_00",
    "fragment_stage_0400_item_00",
    "fragment_stage_0450_item_00",
    "fragment_stage_0500_item_00",
    "fragment_stage_0515_item_00",
    "fragment_stage_0520_item_00",
    "fragment_stage_0525_item_00",
    "fragment_stage_0550_item_00",
    "fragment_stage_0575_item_00",
    "fragment_stage_0600_item_00",
    "fragment_stage_0610_item_00",
    "fragment_stage_0625_item_00",
    "fragment_stage_0640_item_00",
    "fragment_stage_0650_item_00",
    "fragment_stage_0700_item_00",
    "fragment_stage_0800_item_00",
    "fragment_stage_0850_item_00",
    "fragment_stage_0860_item_00",
    "fragment_stage_0870_item_00",
    "fragment_stage_0875_item_00",
    "fragment_stage_0900_item_00",
    "fragment_stage_0910_item_00",
    "fragment_stage_0920_item_00",
    "fragment_stage_0940_item_00",
    "fragment_stage_0945_item_00",
    "fragment_stage_0950_item_00",
    "fragment_stage_0955_item_00",
    "fragment_stage_0960_item_00",
    "fragment_stage_9000_item_00",
}


def _member_body(patch: str, member: str) -> str:
    match = re.search(
        rf"Function {re.escape(member)}\(\)(.*?)EndFunction",
        patch,
        flags=re.IGNORECASE | re.DOTALL,
    )
    assert match is not None, member
    return match.group(1)


def _event_body(patch: str, event: str) -> str:
    match = re.search(
        rf"Event {re.escape(event)}\(Int aiTimerID\)(.*?)EndEvent",
        patch,
        flags=re.IGNORECASE | re.DOTALL,
    )
    assert match is not None, event
    return match.group(1)


def test_bs01_mq04_arms_patch_merges_all_vmad_members_once_and_compiles():
    source_path = SOURCE_ROOT / _script_relative_path(SCRIPT_NAME, ".psc")
    skeleton = source_path.read_text(encoding="utf-8")
    patch = _script_patch_source(SCRIPT_NAME)

    assert patch is not None
    assert "Scriptname" not in patch
    assert " Property " not in patch
    assert patch.count("Function Fragment_Stage_") == len(EXPECTED_MEMBERS)

    merged = _merge_script_method_patches(skeleton, patch)
    members = [
        (kind, name)
        for kind, name, _start, _end in _iter_top_level_papyrus_members(
            merged.splitlines()
        )
    ]
    for member in EXPECTED_MEMBERS:
        assert members.count(("function", member)) == 1
    assert members.count(("event", "ontimer")) == 1
    assert len(members) == len(EXPECTED_MEMBERS) + 1
    assert _merge_script_method_patches(merged, patch) == merged

    declarations = [line for line in skeleton.splitlines() if " Property " in line]
    assert len(declarations) == 46
    for declaration in declarations:
        assert merged.count(declaration) == 1

    base_source = _fo4_base_source()
    assert base_source is not None, "FO4 base Papyrus sources unavailable"
    result = compile_psc(
        merged,
        imports=[str(SOURCE_ROOT), str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=str(_script_relative_path(SCRIPT_NAME, ".psc")),
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None


def test_bs01_mq04_arms_patch_preserves_dagger_and_launcher_choices():
    patch = _script_patch_source(SCRIPT_NAME)

    assert patch is not None
    bribe = _member_body(patch, "Fragment_Stage_0640_Item_00")
    bribed = _member_body(patch, "Fragment_Stage_0650_Item_00")
    peaceful = _member_body(patch, "Fragment_Stage_0700_Item_00")
    violent = _member_body(patch, "Fragment_Stage_0800_Item_00")
    keep_weapons = _member_body(patch, "Fragment_Stage_0920_Item_00")
    donate_weapons = _member_body(patch, "Fragment_Stage_0940_Item_00")

    assert "player.GetItemCount(Caps001) >= 300" in bribe
    assert "player.RemoveItem(Caps001, 300, True)" in bribe
    assert "player.SetValue(BS01_MQ04_Arms_DaggerResolution, 3.0)" in bribed
    assert "player.SetValue(BS01_MQ04_Arms_DaggerResolution, 2.0)" in peaceful
    assert "player.SetValue(BS01_MQ04_Arms_DaggerResolution, 1.0)" in violent
    assert "BS01_MQ04_Arms_LieutenantFaction.SetEnemy(PlayerFaction)" in violent
    assert "Alias_Lieutenants.StartCombatAll(player)" in violent

    assert "player.SetValue(BS01_MQ04_Arms_WeaponChoice, 1.0)" in keep_weapons
    assert "BS01_MQ04_Arms_WeaponCache" not in keep_weapons
    assert "player.SetValue(BS01_MQ04_Arms_WeaponChoice, 2.0)" in donate_weapons
    assert "player.RemoveItem(BS01_MQ04_Arms_WeaponCache, 1, True)" in donate_weapons
    assert "player.AddKeyword(BS01_MQ04_Arms_GaveWeaponsKeyword)" in donate_weapons


def test_bs01_mq04_arms_patch_restores_progression_and_terminal_handoff():
    patch = _script_patch_source(SCRIPT_NAME)

    assert patch is not None
    villagers = _member_body(patch, "Fragment_Stage_0300_Item_00")
    lieutenant = _member_body(patch, "Fragment_Stage_0525_Item_00")
    chests = _member_body(patch, "Fragment_Stage_0610_Item_00")
    return_scene = _member_body(patch, "Fragment_Stage_0950_Item_00")
    final_scene = _member_body(patch, "Fragment_Stage_0955_Item_00")
    terminal = _member_body(patch, "Fragment_Stage_9000_Item_00")
    retry = _event_body(patch, "OnTimer")

    assert "Alias_Villager01.ForceRefIfEmpty(Alias_Villagers.GetAt(0))" in villagers
    assert "Alias_Villager03.ForceRefIfEmpty(Alias_Villagers.GetAt(2))" in villagers
    assert "keyGuardRef.AddItem(BS01_Arms_ThroneRoomKey, 1, True)" in lieutenant
    assert "BS01_MQ04_Arms_KeyGuardScene.Start()" in lieutenant
    assert "Alias_SupplyCrate.GetReference().Enable()" in chests
    assert "Alias_WeaponsCache.GetReference().Enable()" in chests
    assert "rahmaniRef.MoveTo(Alias_XMarker_Rahmani.GetReference())" in return_scene
    assert "shinRef.MoveTo(Alias_XMarker_Shin.GetReference())" in return_scene
    assert "BS01_MQ04_Arms_FinalSceneConversation.Start()" in return_scene
    assert "BS01_MQ04_Arms_FinalSceneConversation.Stop()" in final_scene
    assert "BS01_MQ04_Arms_FinalScene.Start()" in final_scene

    assert "Actor player = Game.GetPlayer()" in terminal
    handoff = (
        "BS01_MQ05_Raiders_StartKeyword.SendStoryEventAndWait(None, player, player)"
    )
    accepted_cleanup = "If accepted\n        Stop()\n    Else\n        StartTimer(5.0, 9000)"
    for attempt in (terminal, retry):
        assert "BS01_MQ05_Raiders.IsRunning()" in attempt
        assert "BS01_MQ05_Raiders.IsCompleted()" in attempt
        assert f"accepted = {handoff}" in attempt
        assert accepted_cleanup in attempt
        assert attempt.count("Stop()") == 1
        assert attempt.count("StartTimer(5.0, 9000)") == 1
        assert attempt.index(handoff) < attempt.index(accepted_cleanup)
        assert "BS01_MQ05_Raiders.Start()" not in attempt
        assert "BS01_MQ05_Raiders_StartKeyword.SendStoryEvent(" not in attempt

    assert "aiTimerID != 9000 || !IsStageDone(9000)" in retry
