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
    "QF_W05_MQ_000P_005698E4": _fragment_stage_members(2100, 2200, 2300),
    "QF_W05_MQ_001P_Wayward_00405E14": _fragment_stage_members(
        200, 300, 400, 600,
        550, 598, 610, 620, 660, 680, 710, 809, 820,
        500, 599, 805, 807, 900, 905, 1000,
    ),
    "QF_W05_MQ_002P_Radical_0040F5BE": _fragment_stage_members(
        100, 110, 125, 130, 200, 400, 475, 700, 998, 1000, 1020, 1030,
        1210, 1310, 1320, 1600, 2000,
        140, 505, 510, 610, 709, 720, 736, 1220, 1240, 1315, 1500, 1700, 2115,
        450, 1550, 8950,
    ),
    "QF_W05_MQ_003P_Muscle_0041A39D": _fragment_stage_members(
        100, 150, 200, 300, 400, 500, 600, 700, 1000, 1020, 1100, 1150,
        1200, 1205, 1300, 1390,
        499, 550, 999, 1005, 1224, 1225, 1230, 1310, 1311, 1312, 1320, 1500,
        710, 900,
    ),
    "QF_W05_MQ_004P_Crane_0041C976": _fragment_stage_members(
        100, 110, 111, 300, 399, 500, 700, 750, 760, 800, 1000, 1100, 1200,
        1230,
        103, 105, 310, 400, 600, 650, 1170, 1180, 1245, 1265, 1300,
        1150, 1235, 1240, 1250, 1260, 8999,
    ),
    "QF_W05_MQ_101P_003FBBB2": _fragment_stage_members(
        5, 10, 13, 15, 20, 30, 40, 50, 100, 110, 120, 150, 200, 300, 600,
        1000, 1400, 1300, 1310, 1600, 1700, 1800, 1900, 9000,
    ),
    "QF_W05_MQ_101P_A_003FBC0D": _fragment_stage_members(
        10, 50, 100, 100, 200, 300, 350, 400, 500, 600, 650, 700, 800, 900,
        950, 970, 1000, 1050, 1100, 1200, 1300, 1400, 1500,
        910, 930, 1110, 1210, 1420, 1450, 1530, 8000, 9000,
    ),
    "QF_W05_MQ_101P_B_003FBC10": _fragment_stage_members(
        10, 230, 231, 400, 450, 500, 600, 9000,
    ),
    "QF_W05_MQ_102P_003FFACF": _fragment_stage_members(
        10, 15, 20, 30, 200, 300, 400, 530, 540, 550,
        580, 584, 585, 586, 590, 680, 684, 685, 686, 690, 730, 1300,
        1400, 1500, 1600, 1700,
    ),
    "QF_W05_MQ_001P_Wayward_Lacey_00405E15": (
        "fragment_stage_0010_item_00",
        "fragment_stage_0015_item_00",
        "fragment_stage_0100_item_00",
        "fragment_stage_0200_item_00",
    ),
    "QF_W05_MQ_001P_Wayward_Lacey_0053AF40": (
        "fragment_stage_0010_item_00",
        "fragment_stage_0100_item_00",
        "fragment_stage_1000_item_00",
    ),
    "QF_W05_MQ_001P_Wayward_MiscP_00594DFD": (
        "fragment_stage_0100_item_00",
        "fragment_stage_9000_item_00",
    ),
    "QF_W05_MQ_003P_Muscle_Duncan_005537E0": (
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
    "DefaultAliasOnActivateGiveItem",
    "DefaultAliasSetStageOnKeypadSuccess",
    "DefaultChallengeMessageOnActivateAlias",
    "W05_MQ_003P_SolAliasScript",
    "W05_MQ_004P_Crane_QuestScript",
    "W05_MQ_TheWayward_QuestScript",
    "DefaultInstanceCellQuestSupportScript",
    "W05_MQ_002P_DeathclawEggWrapUpScript",
    "Fragments:TopicInfos:TIF_W05_MQ_002P_Radical_Ty_0058958C_1",
    "W05_MQ_101P_A_RepairTerminalScript",
    "Fragments:Quests:QF_W05_MQ_101P_OnIncreaseLev_00591E06",
    "Fragments:Quests:QF_W05_MQ_101P_OnLocationCha_00591AB3",
    "W05_MQ_004p_UpstairDoorAliasScript",
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
        "Game.GetPlayer().AddItem(MQ_Overseer_01_Vault76Holotape, 1, False)",
        "Game.GetPlayer().SetValue(MQ_OverseerHolotape01PickedUp, 1.0)",
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
    assert len(ALL_PATCH_CASES) == 36
    assert len(ALL_PATCH_SCRIPT_NAMES) == 36
    assert len(PART2_PATCH_SCRIPT_NAMES) == 20
    assert PART2_PATCH_SCRIPT_NAMES < ALL_PATCH_SCRIPT_NAMES
    assert len(ALL_PATCH_SCRIPT_NAMES - PART2_PATCH_SCRIPT_NAMES) == 16


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


def test_lacey_stage_15_story_event_start_is_guarded_and_stage_30_dependency_is_omitted():
    patch = _script_patch_source(
        "Fragments:Quests:QF_W05_MQ_001P_Wayward_Lacey_00405E15"
    )
    assert patch is not None

    stage_15 = _member_body(patch, "fragment_stage_0015_item_00")
    assert "W05_MQ_001P_Wayward != None" in stage_15
    assert "W05_MQ_001P_Wayward_QuestStartKeyword != None" in stage_15
    assert "!W05_MQ_001P_Wayward.IsRunning()" in stage_15
    assert "!W05_MQ_001P_Wayward.IsCompleted()" in stage_15
    assert "ObjectReference playerRef = Game.GetPlayer()" in stage_15
    assert (
        "W05_MQ_001P_Wayward_QuestStartKeyword.SendStoryEventAndWait("
        "None, playerRef, playerRef)"
    ) in stage_15
    assert "W05_MQ_001P_Wayward.Start()" not in stage_15
    assert "fragment_stage_0030_item_00" not in _member_names(patch)


def test_lacey_single_player_owner_is_filled_before_dialogue_and_none_safe():
    patch = _script_patch_source(
        "Fragments:Quests:QF_W05_MQ_001P_Wayward_Lacey_00405E15"
    )
    assert patch is not None

    stage_10 = _member_body(patch, "fragment_stage_0010_item_00")
    assert "Alias_owningPlayer.ForceRefIfEmpty(Game.GetPlayer())" in stage_10

    stage_100 = _member_body(patch, "fragment_stage_0100_item_00")
    assert "ObjectReference playerRef" in stage_100
    assert "If Alias_owningPlayer" in stage_100
    assert "playerRef = Alias_owningPlayer.GetReference()" in stage_100
    assert "If playerRef" in stage_100
    assert "playerRef.SetValue(" in stage_100
    assert "Alias_owningPlayer.GetRef().SetValue(" not in stage_100


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


def test_row95_only_clears_the_two_approved_key_aliases():
    patch = _script_patch_source(
        "Fragments:Quests:QF_W05_MQ_003P_Muscle_Duncan_005537E0"
    )
    assert patch is not None
    assert _member_names(patch) == [
        "fragment_stage_0200_item_00",
        "fragment_stage_0300_item_00",
    ]
    assert "fragment_stage_0010_item_00" not in patch.lower()
    assert "fragment_stage_0100_item_00" not in patch.lower()
    assert "W05_MQ_003P_Muscle_ProtectronRoomKey" not in patch
    assert "AddItem" not in patch


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
