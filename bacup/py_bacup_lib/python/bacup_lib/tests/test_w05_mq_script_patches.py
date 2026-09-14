from __future__ import annotations

from pathlib import Path

import pytest

from bacup_lib.workflows.unified import (
    _iter_papyrus_states,
    _iter_top_level_papyrus_members,
    _merge_script_method_patches,
    _script_patch_source,
)
from creation_lib.pex import decompile_pex
from creation_lib.pex.native_runtime import compile_psc

from bacup_lib.tests.test_terminal_fragment_script_patches import _fo4_base_source


REPO_ROOT = Path(__file__).resolve().parents[5]
DEPLOYED_SCRIPT_ROOT = REPO_ROOT / "mods" / "SeventySix" / "data" / "Scripts"
DEPLOYED_QUEST_ROOT = DEPLOYED_SCRIPT_ROOT / "fragments" / "quests"
DEPLOYED_TOPICINFO_ROOT = DEPLOYED_SCRIPT_ROOT / "fragments" / "topicinfos"
DEPLOYED_PACKAGE_ROOT = DEPLOYED_SCRIPT_ROOT / "fragments" / "packages"
DEPLOYED_TERMINAL_ROOT = DEPLOYED_SCRIPT_ROOT / "fragments" / "terminals"

SCRIPT_PATCH_CASES: dict[str, tuple[str, ...]] = {
    "AliasSetStageOnItemEquipped": ("onitemequipped",),
    "DefaultCollClearGhostInstOwnerCombat": ("oncombatstatechanged",),
    "W05_001P_BatterAliasScript": (
        "onaliasinit",
        "quest.onstageset",
        "starthitregistration",
        "onhit",
        "onaliasreset",
        "ondeath",
    ),
    "W05_001P_Wayward_QuestScript": (
        "onquestinit",
        "actor.onlocationchange",
        "checkplayerlocation",
        "onstageset",
        "ontimer",
    ),
    "W05_MQ_004P_Crane_QuestScript": (
        "onstageset",
        "ontimer",
        "beginradicalwalkout",
    ),
    "W05_MQ_004p_UpstairDoorAliasScript": (
        "oninit",
        "quest.onstageset",
        "unlockupstairsdoor",
    ),
    "W05_MQ_TheWayward_QuestScript": (
        "refreshduchessangrystate",
        "refreshbullionweek",
        "evaluatewaywardpresence",
        "onquestinit",
        "actor.onlocationchange",
        "onstageset",
    ),
    "W05_MQ_101P_A_RepairTerminalScript": ("onactivate",),
    "W05_MQ_101P_B_AubrieAliasScript": (
        "onaliasinit",
        "prepareforcave",
        "sendhome",
    ),
    "W05_MQ_002P_RemoveTapeScript": ("ongetup",),
    "W05_MQ_003P_RemoveItemTopicInfo": ("onend",),
    "W05_MQ_SkinnerShoutOnOpenScript": ("onopen",),
}


def _fragment_stage_members(*stages: int) -> tuple[str, ...]:
    seen: dict[int, int] = {}
    members: list[str] = []
    for stage in stages:
        item = seen.get(stage, 0)
        members.append(f"fragment_stage_{stage:04d}_item_{item:02d}")
        seen[stage] = item + 1
    return tuple(members)


QUEST_PATCH_CASES: dict[str, tuple[str, ...]] = {
    "QF_ReclamationDay_000D4D34": _fragment_stage_members(100),
    "QF_W05_MQ_000P_005698E4": _fragment_stage_members(
        1100,
        1150,
        1200,
        1250,
        1300,
        1350,
        1400,
        1450,
        1500,
        1550,
        1600,
        1650,
        1999,
        2100,
        2200,
        2300,
    ),
    "QF_W05_MQ_001P_Wayward_00405E14": _fragment_stage_members(
        10, 200, 300, 400, 600, 103, 105, 301, 302, 310, 445, 450, 455, 460,
        470, 491, 510, 515, 516, 520, 522, 530, 550, 598, 610, 620, 660, 680,
        705, 710, 809, 820, 500, 599, 805, 807, 900, 905, 9000, 1000,
    ) + ("attemptradicalhandoff", "ontimer"),
    "QF_W05_MQ_002P_Radical_0040F5BE": _fragment_stage_members(
        100, 110, 125, 130, 200, 400, 475, 700, 998, 1000, 1020, 1030, 1210,
        1310, 1320, 1600, 2000, 140, 505, 510, 610, 709, 720, 736, 1220,
        1240, 1315, 1500, 1700, 2115, 450, 1550, 8950, 1, 2, 5, 6, 10, 150,
        160, 270, 460, 498, 501, 502, 504, 511, 515, 525, 707, 710, 725, 735,
        745, 746, 760, 764, 765, 766, 799, 1100, 1101, 1140, 1290, 1350, 1575,
        9000,
    ),
    "QF_W05_MQ_003P_Muscle_0041A39D": _fragment_stage_members(
        1, 2, 3, 4, 5, 6, 7, 8, 10, 100, 103, 150, 200, 300, 400, 500, 600,
        700, 1000, 1020, 1100, 1150, 1200, 1205, 1300, 1390, 410, 415, 450,
        476, 1050, 499, 550, 710, 715, 725, 800, 900, 999, 1005, 1015, 1025,
        1224, 1225, 1226, 1229, 1230, 1232, 1240, 1251, 1270, 1275, 1280,
        1310, 1311, 1312, 1320, 1325, 1321, 1500, 9000, 10000,
    ),
    "QF_W05_MQ_004P_Crane_0041C976": _fragment_stage_members(
        1, 2, 3, 4, 5, 6, 10, 50, 100, 102, 103, 105, 108, 109, 110, 111, 112,
        125, 200, 210, 300, 301, 310, 399, 400, 401, 495, 500, 600, 650, 700,
        701, 702, 703, 704, 710, 750, 760, 765, 775, 800, 820, 830, 1000, 1100,
        1105, 1150, 1170, 1180, 1200, 1220, 1221, 1230, 1235, 1240, 1242,
        1243, 1244, 1245, 1250, 1260, 1261, 1265, 1300, 8999, 9000,
    ),
    "QF_W05_MQ_101P_003FBBB2": _fragment_stage_members(
        5, 10, 13, 15, 20, 30, 40, 50, 51, 52, 100, 110, 120, 150, 200, 300,
        400, 500, 550, 600, 700, 800, 805, 810, 820, 830, 900, 1000, 1100,
        1200, 1290, 1300, 1310, 1400, 1450, 1500, 1510, 1600, 1610, 1700,
        1800, 1810, 1820, 1900, 1910, 1920, 2000, 9000,
    ),
    "QF_W05_MQ_101P_A_003FBC0D": _fragment_stage_members(
        0, 1, 2, 3, 4, 5, 6, 10, 50, 100, 100, 200, 300, 310, 311, 320, 330,
        331, 350, 375, 400, 500, 600, 650, 680, 700, 710, 730, 800, 810, 820,
        830, 900, 910, 930, 950, 960, 970, 1000, 1050, 1100, 1110, 1200, 1210,
        1300, 1400, 1415, 1420, 1430, 1440, 1450, 1500, 1530, 8000, 9000,
    ),
    "QF_W05_MQ_101P_B_003FBC10": _fragment_stage_members(
        10, 100, 200, 230, 231, 232, 240, 300, 350, 400, 450, 500, 590, 600,
        700, 9000,
    ),
    "QF_W05_MQ_102P_003FFACF": _fragment_stage_members(
        10, 15, 20, 30, 200, 300, 400, 450, 530, 540, 550, 560, 565, 580, 582,
        584, 585, 586, 590, 595, 610, 615, 630, 640, 665, 680, 682, 684, 685,
        686, 690, 695, 700, 710, 720, 730, 740, 800, 850, 900, 1000, 1200,
        1300, 1400, 1500, 1600, 1700, 9000, 10000,
    ),
    "QF_W05_MQ_001P_Wayward_Lacey_00405E15": (
        "fragment_stage_0015_item_00",
        "dispatchwaywardstartevent",
        "setlaceyiselacheckpoint",
        "fragment_stage_0030_item_00",
        "fragment_stage_0100_item_00",
        "fragment_stage_0200_item_00",
    ),
    "QF_W05_MQ_001P_Wayward_Lacey_0053AF40": (
        "fragment_stage_0010_item_00",
        "dispatchwaywardstartevent",
        "fragment_stage_0100_item_00",
        "fragment_stage_1000_item_00",
    ),
    "QF_W05_MQ_001P_Wayward_MiscP_00594DFD": (
        "fragment_stage_0100_item_00",
        "fragment_stage_9000_item_00",
    ),
    "QF_W05_MQ_003P_Muscle_Duncan_005537E0": (
        "fragment_stage_0010_item_00",
        "fragment_stage_0100_item_00",
        "fragment_stage_0200_item_00",
        "fragment_stage_0300_item_00",
    ),
    "QF_W05_MQ_003P_Radio_0041A325": ("fragment_stage_9000_item_00",),
    "QF_W05_MQ_101P_Radio_003FBBB3": ("fragment_stage_0020_item_00",),
    "QF_W05_MQ_102P_A_003FFC02": (
        "fragment_stage_0010_item_00",
        "fragment_stage_0100_item_00",
        "fragment_stage_0200_item_00",
        "fragment_stage_0400_item_00",
        "fragment_stage_0300_item_00",
        "fragment_stage_9000_item_00",
        "fragment_stage_9500_item_00",
    ),
    "QF_W05_MQ_102P_B_003FFC00": (
        "fragment_stage_0010_item_00",
        "fragment_stage_0100_item_00",
        "fragment_stage_8000_item_00",
        "fragment_stage_9000_item_00",
    ),
}

PACKAGE_PATCH_CASES: dict[str, tuple[str, ...]] = {
    "PF_W05_MQ_001P_Wayward_Batte_0040BD22": ("fragment_end",),
    "PF_W05_MQ_101P_B_AubriePacka_0059F653": ("fragment_end",),
}

TOPICINFO_PATCH_CASES: dict[str, tuple[str, ...]] = {
    "TIF_W05_MQ_001P_Wayward_Lace_0056A164": ("fragment_end",),
    "TIF_W05_MQ_001P_Wayward_Lace_0056A173": ("fragment_end",),
    "TIF_W05_MQ_001P_Wayward_Lace_0056A174": ("fragment_end",),
    "TIF_W05_MQ_001P_Wayward_Penn_005852B7": ("fragment_end",),
    "TIF_W05_MQ_002P_Radical_Anch_00589599": ("fragment_end",),
    "TIF_W05_MQ_002P_Radical_Anch_0058959A": ("fragment_end",),
    "TIF_W05_MQ_002P_Radical_Tyle_00589564": ("fragment_end",),
    "TIF_W05_MQ_002P_Radical_Tyle_00589570": ("fragment_end",),
    "TIF_W05_MQ_002P_Radical_Tyle_005895DE": ("fragment_end",),
    "TIF_W05_MQ_102P_004010A8": ("fragment_end",),
}

TERMINAL_PATCH_CASES: dict[str, tuple[str, ...]] = {
    "TERM_W05_MQ_101P_NukaTermina_003FE452": (
        "fragment_terminal_01",
        "fragment_terminal_02",
        "fragment_terminal_03",
    ),
    "TERM_W05_MQ_102P_SecurityTer_00544D59": ("fragment_terminal_01",),
}

PATCH_GROUPS = (
    ("", DEPLOYED_SCRIPT_ROOT, SCRIPT_PATCH_CASES),
    ("Fragments:Quests:", DEPLOYED_QUEST_ROOT, QUEST_PATCH_CASES),
    ("Fragments:Packages:", DEPLOYED_PACKAGE_ROOT, PACKAGE_PATCH_CASES),
    ("Fragments:TopicInfos:", DEPLOYED_TOPICINFO_ROOT, TOPICINFO_PATCH_CASES),
    ("Fragments:Terminals:", DEPLOYED_TERMINAL_ROOT, TERMINAL_PATCH_CASES),
)

ALL_PATCH_CASES = tuple(
    (f"{namespace}{base_name}", root / f"{base_name.lower()}.pex", expected_members)
    for namespace, root, cases in PATCH_GROUPS
    for base_name, expected_members in cases.items()
)
ALL_PATCH_SCRIPT_NAMES = frozenset(script_name for script_name, _path, _members in ALL_PATCH_CASES)

PART2_PATCH_SCRIPT_NAMES = frozenset(
    {
        "AliasSetStageOnItemEquipped",
        "W05_MQ_003P_RemoveItemTopicInfo",
        "DefaultCollClearGhostInstOwnerCombat",
        "W05_MQ_002P_RemoveTapeScript",
        "Fragments:TopicInfos:TIF_W05_MQ_002P_Radical_Tyle_00589564",
        "Fragments:TopicInfos:TIF_W05_MQ_002P_Radical_Tyle_00589570",
        "Fragments:TopicInfos:TIF_W05_MQ_002P_Radical_Tyle_005895DE",
        "Fragments:TopicInfos:TIF_W05_MQ_002P_Radical_Anch_00589599",
        "Fragments:TopicInfos:TIF_W05_MQ_002P_Radical_Anch_0058959A",
        "Fragments:Quests:QF_W05_MQ_101P_Radio_003FBBB3",
        "Fragments:Packages:PF_W05_MQ_101P_B_AubriePacka_0059F653",
        "Fragments:Terminals:TERM_W05_MQ_101P_NukaTermina_003FE452",
        "Fragments:Quests:QF_W05_MQ_102P_A_003FFC02",
        "Fragments:Quests:QF_W05_MQ_102P_B_003FFC00",
        "Fragments:Terminals:TERM_W05_MQ_102P_SecurityTer_00544D59",
        "Fragments:TopicInfos:TIF_W05_MQ_102P_004010A8",
        "Fragments:Quests:QF_W05_MQ_003P_Radio_0041A325",
        "Fragments:Quests:QF_W05_MQ_003P_Muscle_Duncan_005537E0",
        "Fragments:TopicInfos:TIF_W05_MQ_001P_Wayward_Penn_005852B7",
        "W05_MQ_SkinnerShoutOnOpenScript",
    }
)

OPEN_JOIN_SCRIPT_NAMES = (
    "DefaultInstanceCellQuestSupportScript",
)

EXPECTED_SNIPPETS: dict[str, tuple[str, ...]] = {
    "Fragments:Packages:PF_W05_MQ_001P_Wayward_Batte_0040BD22": (
        "GetOwningQuest().IsStageDone(600)",
        "GetOwningQuest().SetStage(600)",
    ),
    "Fragments:TopicInfos:TIF_W05_MQ_001P_Wayward_Lace_0056A164": (
        "Game.GetPlayer().AddItem(LL_Weapon_Ranged_PipeGun, 1, False)",
        "Game.GetPlayer().AddItem(Ammo38Caliber, 1, False)",
    ),
    "Fragments:TopicInfos:TIF_W05_MQ_001P_Wayward_Lace_0056A173": (
        "Game.GetPlayer().AddItem(CharGenLL_Weapon_Simple_Melee_Machete_FullHealth, 1, False)",
        "Game.GetPlayer().SetValue(W05_MQ_001P_Wayward_LaceyIsela_PlayerGotGun, 1.0)",
    ),
    "Fragments:TopicInfos:TIF_W05_MQ_001P_Wayward_Lace_0056A174": (
        "Game.GetPlayer().AddItem(CharGenLL_Weapon_Simple_Melee_Machete_FullHealth, 1, False)",
        "Game.GetPlayer().SetValue(W05_MQ_001P_Wayward_LaceyIsela_PlayerGotGun, 1.0)",
    ),
    "Fragments:TopicInfos:TIF_W05_MQ_002P_Radical_Tyle_00589570": (
        "Game.GetPlayer().AddItem(W05_Wayward_002P_LL_TylerCounty_Reward, 1, False)",
    ),
    "Fragments:TopicInfos:TIF_W05_MQ_002P_Radical_Tyle_005895DE": (
        "Game.GetPlayer().AddItem(StealthBoy, 1, False)",
        "Game.GetPlayer().SetValue(W05_MQ_002P_Radical_TylerCounty_PlayerGotFreebeeStealthBoy, 1.0)",
    ),
    "Fragments:TopicInfos:TIF_W05_MQ_002P_Radical_Anch_00589599": (
        "Game.GetPlayer().AddItem(W05_Wayward_002P_LL_AnchorFarm_Reward, 1, False)",
    ),
    "Fragments:TopicInfos:TIF_W05_MQ_002P_Radical_Anch_0058959A": (
        "Game.GetPlayer().AddItem(W05_Wayward_002P_LL_AnchorFarm_Reward, 1, False)",
    ),
    "Fragments:TopicInfos:TIF_W05_MQ_001P_Wayward_Penn_005852B7": (
        "playerRef.GetValue(MQ_OverseerHolotape01PickedUp) < 1.0",
        "playerRef.GetItemCount(MQ_Overseer_01_Vault76Holotape) == 0",
        "playerRef.AddItem(MQ_Overseer_01_Vault76Holotape, 1, False)",
        "playerRef.SetValue(MQ_OverseerHolotape01PickedUp, 1.0)",
    ),
}


def _member_names(source: str) -> list[str]:
    return [
        name
        for _kind, name, _start, _end in _iter_top_level_papyrus_members(
            source.splitlines()
        )
    ]


def _merged_production_source(script_name: str, pex_path: Path) -> str:
    assert pex_path.is_file(), f"deployed production PEX unavailable: {pex_path}"
    skeleton = decompile_pex(pex_path, fo4_api_compat=True)
    patch = _script_patch_source(script_name)
    assert patch is not None
    return _merge_script_method_patches(skeleton, patch)


def _member_body(source: str, member_name: str) -> str:
    start, end = next(
        (start, end)
        for _kind, name, start, end in _iter_top_level_papyrus_members(
            source.splitlines()
        )
        if name == member_name.lower()
    )
    return "\n".join(source.splitlines()[start : end + 1])


def test_w05_mq_manifest_enumerates_part1_and_exact_part2_gate():
    newly_restored_route_helpers = {
        "W05_001P_BatterAliasScript",
        "W05_001P_Wayward_QuestScript",
        "W05_MQ_101P_A_RepairTerminalScript",
        "W05_MQ_101P_B_AubrieAliasScript",
    }
    assert newly_restored_route_helpers <= ALL_PATCH_SCRIPT_NAMES
    assert len(ALL_PATCH_CASES) == 44
    assert len(ALL_PATCH_SCRIPT_NAMES) == 44
    assert len(PART2_PATCH_SCRIPT_NAMES) == 20
    assert PART2_PATCH_SCRIPT_NAMES < ALL_PATCH_SCRIPT_NAMES
    assert len(ALL_PATCH_SCRIPT_NAMES - PART2_PATCH_SCRIPT_NAMES) == 24


@pytest.mark.parametrize(("script_name", "_pex_path", "expected_members"), ALL_PATCH_CASES)
def test_patch_members_are_unique_top_level_and_state_free(
    script_name: str, _pex_path: Path, expected_members: tuple[str, ...]
):
    patch = _script_patch_source(script_name)

    assert patch is not None
    assert not any(
        line.strip().lower().startswith("scriptname ") for line in patch.splitlines()
    )
    assert _iter_papyrus_states(patch.splitlines()) == []
    member_names = _member_names(patch)
    assert member_names == list(expected_members)
    assert len(member_names) == len(set(member_names))


@pytest.mark.parametrize(("script_name", "pex_path", "expected_members"), ALL_PATCH_CASES)
def test_production_merge_preserves_top_level_placement_and_is_idempotent(
    script_name: str, pex_path: Path, expected_members: tuple[str, ...]
):
    patch = _script_patch_source(script_name)
    assert patch is not None
    merged = _merged_production_source(script_name, pex_path)

    merged_member_names = _member_names(merged)
    for expected_member in expected_members:
        assert merged_member_names.count(expected_member) == 1
        assert _member_body(merged, expected_member) == _member_body(
            patch, expected_member
        )
    assert sum(line.strip().lower().startswith("scriptname ") for line in merged.splitlines()) == 1
    assert _merge_script_method_patches(merged, patch) == merged


@pytest.mark.parametrize(("script_name", "pex_path", "_expected_members"), ALL_PATCH_CASES)
def test_full_production_merge_native_compiles_for_fo4_without_skips(
    script_name: str, pex_path: Path, _expected_members: tuple[str, ...]
):
    base_source = _fo4_base_source()
    assert base_source is not None, "FO4 base Papyrus sources unavailable"

    result = compile_psc(
        _merged_production_source(script_name, pex_path),
        imports=[str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=f"{script_name.replace(':', '/')}.psc",
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None


@pytest.mark.parametrize("script_name", OPEN_JOIN_SCRIPT_NAMES)
def test_open_controller_and_deep_pass_join_has_no_patch(script_name: str):
    assert _script_patch_source(script_name) is None


def test_shared_distance_parent_join_has_single_player_patch():
    patch = _script_patch_source("DefaultAliasOnDistanceLessThan")
    assert patch is not None
    assert "RegisterForDistanceLessThanEvent(Self, TargetAlias, fTargetDistance)" in patch
    assert "OwningQuest.SetStage(StageToSet)" in patch


@pytest.mark.parametrize(("script_name", "expected_snippets"), EXPECTED_SNIPPETS.items())
def test_deterministic_result_patch_contains_expected_calls(
    script_name: str, expected_snippets: tuple[str, ...]
):
    patch = _script_patch_source(script_name)
    assert patch is not None
    for snippet in expected_snippets:
        assert snippet in patch


def test_lacey_stage_15_uses_guarded_story_event_and_stage_30_starts_checkpoint():
    patch = _script_patch_source(
        "Fragments:Quests:QF_W05_MQ_001P_Wayward_Lacey_00405E15"
    )
    assert patch is not None

    stage_15 = _member_body(patch, "fragment_stage_0015_item_00")
    assert "DispatchWaywardStartEvent()" in stage_15

    dispatch = _member_body(patch, "dispatchwaywardstartevent")
    assert "W05_MQ_001P_Wayward != None" in dispatch
    assert "W05_MQ_001P_Wayward_QuestStartKeyword != None" in dispatch
    assert "!W05_MQ_001P_Wayward.IsRunning()" in dispatch
    assert "!W05_MQ_001P_Wayward.IsCompleted()" in dispatch
    assert "ObjectReference playerRef = Game.GetPlayer()" in dispatch
    assert (
        "W05_MQ_001P_Wayward_QuestStartKeyword.SendStoryEventAndWait("
        "None, playerRef, playerRef)"
    ) in dispatch
    assert "W05_MQ_001P_Wayward.Start()" not in dispatch

    stage_30 = _member_body(patch, "fragment_stage_0030_item_00")
    assert "SetLaceyIselaCheckpoint(1.0)" in stage_30
    assert "W05_MQ_001P_Wayward_LaceyIselaScene_010.Stop()" in stage_30
    assert "W05_MQ_001P_Wayward_LaceyIselaScene_020.Stop()" in stage_30


def test_lacey_has_no_invented_stage_10_member_and_stage_100_is_none_safe():
    patch = _script_patch_source(
        "Fragments:Quests:QF_W05_MQ_001P_Wayward_Lacey_00405E15"
    )
    assert patch is not None

    assert "fragment_stage_0010_item_00" not in _member_names(patch)

    checkpoint = _member_body(patch, "setlaceyiselacheckpoint")
    assert "playerRef = Game.GetPlayer()" in checkpoint
    assert (
        "playerRef.SetValue(W05_MQ_001P_Wayward_LaceyIsela_Checkpoint, "
        "checkpointValue)"
    ) in checkpoint

    stage_100 = _member_body(patch, "fragment_stage_0100_item_00")
    assert "W05_MQ_001P_Wayward.IsRunning()" in stage_100
    assert "ObjectReference playerRef" in stage_100
    assert "If Alias_owningPlayer" in stage_100
    assert "playerRef = Alias_owningPlayer.GetReference()" in stage_100
    assert "If playerRef" in stage_100
    assert "playerRef.SetValue(" in stage_100
    assert "Alias_owningPlayer.GetRef().SetValue(" not in stage_100
    assert "SetLaceyIselaCheckpoint(10.0)" in stage_100

    stage_200 = _member_body(patch, "fragment_stage_0200_item_00")
    assert stage_200.index("SetLaceyIselaCheckpoint(10.0)") < stage_200.index(
        "Stop()"
    )


def test_wayward_instance_entry_produces_stage_102_from_the_bound_location():
    patch = _script_patch_source("W05_001P_Wayward_QuestScript")
    assert patch is not None

    quest_init = _member_body(patch, "onquestinit")
    assert 'RegisterForRemoteEvent(playerRef, "OnLocationChange")' in quest_init
    assert "CheckPlayerLocation(playerRef.GetCurrentLocation())" in quest_init

    location_change = _member_body(patch, "actor.onlocationchange")
    assert "akSender == Game.GetPlayer()" in location_change
    assert "CheckPlayerLocation(akNewLoc)" in location_change

    check_location = _member_body(patch, "checkplayerlocation")
    assert "playerLocation == LocForestTheWaywardLocationInterior" in check_location
    assert "!IsStageDone(102)" in check_location
    assert "SetStage(102)" in check_location


def test_batter_hit_registration_tracks_only_player_and_mort_without_guessing_kill_credit():
    patch = _script_patch_source("W05_001P_BatterAliasScript")
    assert patch is not None

    alias_init = _member_body(patch, "onaliasinit")
    assert 'RegisterForRemoteEvent(owningQuest, "OnStageSet")' in alias_init
    assert "owningQuest.IsStageDone(RegistrationStage)" in alias_init

    stage_set = _member_body(patch, "quest.onstageset")
    assert "auiStageID == RegistrationStage" in stage_set
    assert "StartHitRegistration()" in stage_set

    register = _member_body(patch, "starthitregistration")
    assert "UnregisterForAllHitEvents()" in register
    assert "RegisterForHitEvent(Self)" in register

    hit = _member_body(patch, "onhit")
    assert "akAggressor == playerRef" in hit
    assert "PlayerHitBatter = True" in hit
    assert "akAggressor == Mort.GetActorReference()" in hit
    assert "MortHitBatter = True" in hit
    assert "RegisterForHitEvent(Self)" in hit

    death = _member_body(patch, "ondeath")
    assert death.index("owningQuest.SetStage(470)") < death.index(
        "owningQuest.SetStage(599)"
    )
    assert "akKiller == playerRef" in death
    assert "akKiller == mortRef" in death
    assert "PlayerHitBatter" not in death
    assert "MortHitBatter" not in death


def test_reclamation_day_stage_100_uses_story_manager_without_direct_start():
    patch = _script_patch_source(
        "Fragments:Quests:QF_ReclamationDay_000D4D34"
    )
    assert patch is not None

    stage_100 = _member_body(patch, "fragment_stage_0100_item_00")
    assert "ObjectReference playerRef = Game.GetPlayer()" in stage_100
    assert (
        "W05_MQ_001P_Wayward_QuestStartKeyword.SendStoryEventAndWait("
        "None, playerRef, playerRef)"
    ) in stage_100
    assert ".Start()" not in stage_100


def test_row15_uses_compiled_minus_one_prerequisite_sentinel():
    patch = _script_patch_source("AliasSetStageOnItemEquipped")
    assert patch is not None
    assert "iPrereqStage == -1" in patch
    assert "iPrereqStage <= 0" not in patch
    assert patch.index("akBaseObject == ItemToCheck") < patch.index("SetStage(iStageToSet)")


def test_row25_item_guard_reaches_remove_and_flag_only_when_item_exists():
    patch = _script_patch_source("W05_MQ_003P_RemoveItemTopicInfo")
    assert patch is not None
    count_guard = "playerRef.GetItemCount(ItemToRemove) > 0"
    remove_call = "playerRef.RemoveItem(ItemToRemove, 1, True)"
    flag_call = "playerRef.SetValue(W05_MQ_003P_Muscle_PlayerGaveSolItem, 1.0)"
    assert patch.index("If OwningPlayer") < patch.index(count_guard)
    assert patch.index(count_guard) < patch.index(remove_call) < patch.index(flag_call)


def test_row36_uses_lesson21_refcollection_parent_casts():
    patch = _script_patch_source("DefaultCollClearGhostInstOwnerCombat")
    assert patch is not None
    assert "(Self as RefCollectionAlias).GetCount()" in patch
    assert "(Self as RefCollectionAlias).GetAt(i)" in patch
    assert "GetActorAt(" not in patch


def test_rows43_to45_and52_to53_use_exact_one_argument_fragment_end_signature():
    script_names = (
        "Fragments:TopicInfos:TIF_W05_MQ_002P_Radical_Tyle_00589564",
        "Fragments:TopicInfos:TIF_W05_MQ_002P_Radical_Tyle_00589570",
        "Fragments:TopicInfos:TIF_W05_MQ_002P_Radical_Tyle_005895DE",
        "Fragments:TopicInfos:TIF_W05_MQ_002P_Radical_Anch_00589599",
        "Fragments:TopicInfos:TIF_W05_MQ_002P_Radical_Anch_0058959A",
    )
    expected_signature = "Function Fragment_End(ObjectReference akSpeakerRef)"

    for script_name in script_names:
        patch = _script_patch_source(script_name)
        assert patch is not None
        assert _member_body(patch, "fragment_end").splitlines()[0] == expected_signature
        assert "bool abHasBeenSaid" not in patch

    row43 = _script_patch_source(script_names[0])
    assert row43 is not None
    assert "If DeathclawIslandMapMarker" in row43
    assert "DeathclawIslandMapMarker.AddToMap()" in row43
    assert "AddToMap(True)" not in row43
    assert "W05_MQ_002P_Radical_QuestActiveKeyword" not in row43
    assert "AddKeyword" not in row43


def test_pennington_holotape_reward_is_idempotent():
    patch = _script_patch_source(
        "Fragments:TopicInfos:TIF_W05_MQ_001P_Wayward_Penn_005852B7"
    )
    assert patch is not None
    fragment = _member_body(patch, "fragment_end")

    value_guard = "playerRef.GetValue(MQ_OverseerHolotape01PickedUp) < 1.0"
    inventory_guard = (
        "playerRef.GetItemCount(MQ_Overseer_01_Vault76Holotape) == 0"
    )
    add_item = "playerRef.AddItem(MQ_Overseer_01_Vault76Holotape, 1, False)"
    set_value = "playerRef.SetValue(MQ_OverseerHolotape01PickedUp, 1.0)"

    assert fragment.count(add_item) == 1
    assert fragment.index(value_guard) < fragment.index(inventory_guard)
    assert fragment.index(inventory_guard) < fragment.index(add_item)
    assert fragment.index(add_item) < fragment.index(set_value)


def test_rows74_and75_use_bound_branch_convergence_bodies():
    row74 = _script_patch_source("Fragments:Quests:QF_W05_MQ_102P_A_003FFC02")
    row75 = _script_patch_source("Fragments:Quests:QF_W05_MQ_102P_B_003FFC00")
    assert row74 is not None
    assert row75 is not None

    for source in (row74, row75):
        for parent_literal in ("1200", "1300", "1400"):
            assert parent_literal not in source
    assert _member_names(row74)[-1] == "fragment_stage_9500_item_00"
    row74_completion = _member_body(row74, "fragment_stage_9500_item_00")
    assert "W05_MQR_201P_QuestStart_Keyword.SendStoryEvent" in row74_completion
    assert "W05_MQ_102P.SetStage(1600)" in row74_completion
    assert row74_completion.endswith("    Stop()\nEndFunction")
    assert _member_body(row74, "fragment_stage_0300_item_00") == (
        "Function Fragment_Stage_0300_Item_00()\n"
        "    If W05_MQ_102P_A_MegIntroScene02 && !W05_MQ_102P_A_MegIntroScene02.IsPlaying()\n"
        "        W05_MQ_102P_A_MegIntroScene02.Start()\n"
        "    EndIf\n"
        "EndFunction"
    )
    for omitted_row74_member in (
        "fragment_stage_9999_item_00",
        "fragment_stage_10000_item_00",
    ):
        assert omitted_row74_member not in _member_names(row74)
    row75_completion = _member_body(row75, "fragment_stage_9000_item_00")
    assert "W05_MQS_201P_QuestStartKeyword.SendStoryEvent" in row75_completion
    assert "W05_MQ_102P.SetStage(1700)" in row75_completion
    assert "fragment_stage_9999_item_00" not in _member_names(row75)


def test_row77_duplicate_guard_precedes_holotape_grant():
    patch = _script_patch_source(
        "Fragments:Terminals:TERM_W05_MQ_102P_SecurityTer_00544D59"
    )
    assert patch is not None
    guard = "Game.GetPlayer().GetItemCount(W05_MQ_102P_VTec_Holotape01) == 0"
    grant = "Game.GetPlayer().AddItem(W05_MQ_102P_VTec_Holotape01, 1, False)"
    assert patch.index(guard) < patch.index(grant)


def test_row82_optional_alias_and_owning_quest_guards_reach_stage_1400_once():
    patch = _script_patch_source("Fragments:TopicInfos:TIF_W05_MQ_102P_004010A8")
    assert patch is not None
    assert _member_names(patch) == ["fragment_end"]
    assert "fragment_begin" not in patch.lower()
    assert patch.index("If Alias_Projector") < patch.index(
        "Alias_Projector.GetOwningQuest()"
    )
    assert patch.index("Alias_Projector.GetOwningQuest()") < patch.index(
        "owningQuest && !owningQuest.IsStageDone(1400)"
    )
    assert patch.index("!owningQuest.IsStageDone(1400)") < patch.index(
        "owningQuest.SetStage(1400)"
    )


def test_crane_walkout_replays_from_collection_state_without_duplicate_release():
    patch = _script_patch_source("W05_MQ_004P_Crane_QuestScript")
    assert patch is not None

    stage_set = _member_body(patch, "onstageset")
    assert "auiStageID == 1260" in stage_set
    assert stage_set.index("BeginRadicalWalkOut()") < stage_set.index(
        "StartTimer(Utility.RandomFloat(WaitMin, WaitMax), iEVPPrepTimerID)"
    )

    timer = _member_body(patch, "ontimer")
    assert "aiTimerID == iWalkOutTimerID" in timer
    assert timer.count("BeginRadicalWalkOut()") == 1
    assert "aiTimerID == iEVPPrepTimerID" in timer
    assert "iEVPStage > 0 && !IsStageDone(iEVPStage)" in timer

    walkout = _member_body(patch, "beginradicalwalkout")
    assert "Radicals == None || RadicalsTravelToExit == None" in walkout
    assert "RadicalsTravelToExit.Find(radicalRef) < 0" in walkout
    assert walkout.index("RadicalsTravelToExit.AddRef(radicalRef)") < walkout.index(
        "radicalActor.EvaluatePackage()"
    )
    assert "radicalIndex + 1 < radicalCount" in walkout
    assert (
        "StartTimer(Utility.RandomFloat(WalkoutUpdateTimeMin, "
        "WalkoutUpdateTimeMax), iWalkOutTimerID)"
    ) in walkout
    assert walkout.count("Return") == 2


def test_upstairs_door_unlocks_on_existing_or_future_prerequisite_stage():
    patch = _script_patch_source("W05_MQ_004p_UpstairDoorAliasScript")
    assert patch is not None

    alias_init = _member_body(patch, "oninit")
    assert 'RegisterForRemoteEvent(owningQuest, "OnStageSet")' in alias_init
    assert "owningQuest.GetStage() >= PreReqStage" in alias_init
    assert alias_init.count("UnlockUpstairsDoor()") == 1

    stage_set = _member_body(patch, "quest.onstageset")
    assert "auiStageID >= PreReqStage" in stage_set
    assert stage_set.count("UnlockUpstairsDoor()") == 1

    unlock = _member_body(patch, "unlockupstairsdoor")
    assert "doorRef != None && doorRef.IsLocked()" in unlock
    assert unlock.count("doorRef.Unlock()") == 1


def test_row95_reissues_only_the_earned_protectron_key_and_clears_key_aliases():
    patch = _script_patch_source(
        "Fragments:Quests:QF_W05_MQ_003P_Muscle_Duncan_005537E0"
    )
    assert patch is not None
    assert _member_names(patch) == [
        "fragment_stage_0010_item_00",
        "fragment_stage_0100_item_00",
        "fragment_stage_0200_item_00",
        "fragment_stage_0300_item_00",
    ]

    instance_load = _member_body(patch, "fragment_stage_0010_item_00")
    acquired = "W05_MQ_003P_Muscle_PlayerAcquiredProtectronKey"
    key = "W05_MQ_003P_Muscle_ProtectronRoomKey"
    assert f"{acquired} && {key}" in instance_load
    assert f"playerRef.GetValue({acquired}) >= 1.0" in instance_load
    assert f"playerRef.GetItemCount({key} as Form) == 0" in instance_load
    assert instance_load.count(f"playerRef.AddItem({key} as Form, 1, True)") == 1

    protectron = _member_body(patch, "fragment_stage_0100_item_00")
    assert protectron.index("Alias_ProtectronKey.Clear()") < protectron.index(
        f"playerRef.SetValue({acquired}, 1.0)"
    )
    assert "AddItem" not in protectron

    assert _member_body(patch, "fragment_stage_0200_item_00").count(
        "Alias_AssaultronKey.Clear()"
    ) == 1
    assert _member_body(patch, "fragment_stage_0300_item_00").count(
        "Alias_HandyKey.Clear()"
    ) == 1
    assert patch.count("AddItem") == 1


def test_row97_source_encodes_three_credential_or_without_dialogue_push():
    patch = _script_patch_source("W05_MQ_SkinnerShoutOnOpenScript")
    assert patch is not None
    assert "playerRef.GetItemCount(KeyObject) > 0" in patch
    assert "playerRef.GetItemCount(AccessCard) > 0" in patch
    assert "playerRef.GetItemCount(AccessCard01) > 0" in patch
    assert patch.count("||") == 2
    assert patch.index("playerRef.GetValue(W05_MQ_003P_Muscle_SkinnerAcknowledgesBreakIn) == 0.0") < patch.index(
        "Bool hasCredential"
    )
    assert patch.index("If !hasCredential") < patch.index(
        "playerRef.SetValue(W05_MQ_003P_Muscle_SkinnerAcknowledgesBreakIn, 1.0)"
    )
    assert "2.0" not in patch
    assert ".Say(" not in patch
    assert "W05_003_SkinnerShoutRobbery" not in patch


@pytest.mark.parametrize(
    ("has_key", "has_access_card", "has_access_card01", "sets_break_in"),
    (
        (False, False, False, True),
        (True, False, False, False),
        (False, True, False, False),
        (False, False, True, False),
        (True, True, False, False),
        (True, False, True, False),
        (False, True, True, False),
        (True, True, True, False),
    ),
)
def test_row97_credential_matrix(
    has_key: bool,
    has_access_card: bool,
    has_access_card01: bool,
    sets_break_in: bool,
):
    has_authorizing_credential = has_key or has_access_card or has_access_card01
    assert (not has_authorizing_credential) is sets_break_in
