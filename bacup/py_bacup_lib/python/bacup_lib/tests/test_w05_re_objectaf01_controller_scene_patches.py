from __future__ import annotations

import json
from pathlib import Path

import pytest

from bacup_lib.tests.test_terminal_fragment_script_patches import _fo4_base_source
from bacup_lib.workflows.unified import (
    _iter_top_level_papyrus_members,
    _merge_script_method_patches,
    _script_patch_source,
    _script_relative_path,
)
from creation_lib.pex.native_runtime import compile_psc


REPO_ROOT = Path(__file__).resolve().parents[5]
SOURCE_ROOT = REPO_ROOT / "mods" / "SeventySix" / "Scripts" / "Source" / "User"
FIXTURE = Path(__file__).parent / "fixtures" / "fo76" / "w05_re_objectaf01_source_live.json"
CONTROLLER = "W05_RE_ObjectAF01_Quest_Script"
ATTACK = "Fragments:Scenes:SF_W05_RE_ObjectAF01_Attack_0056BC6B"
EXPLOSION = "Fragments:Scenes:SF_W05_RE_ObjectAF01_Explosi_0056A252"
FIX_ROBOT = "Fragments:Scenes:SF_W05_RE_ObjectAF01_FixRobo_0056A251"
SANDBOX = "Fragments:Scenes:SF_W05_RE_ObjectAF01_Sandbox_0056A24C"
QUEST_FRAGMENT = "Fragments:Quests:QF_W05_RE_ObjectAF01_0056A1D1"

PATCH_MEMBERS = {
    CONTROLLER: {"onquestinit", "objectreference.onactivate"},
    ATTACK: {"fragment_phase_02_begin", "fragment_phase_02_end"},
    EXPLOSION: {"fragment_phase_02_end", "fragment_phase_05_end"},
    FIX_ROBOT: {"fragment_phase_03_end"},
    SANDBOX: {"fragment_phase_01_begin"},
    QUEST_FRAGMENT: {
        "fragment_stage_0010_item_00",
        "fragment_stage_0023_item_00",
        "fragment_stage_0025_item_00",
        "fragment_stage_0030_item_00",
        "fragment_stage_1000_item_00",
    },
}


def _member_body(source: str, member_name: str) -> str:
    lines = source.splitlines()
    start, end = next(
        (start, end)
        for kind, name, start, end in _iter_top_level_papyrus_members(lines)
        if kind in {"function", "event"} and name == member_name.lower()
    )
    return "\n".join(lines[start : end + 1])


def _merged(script_name: str) -> tuple[str, str]:
    skeleton_path = SOURCE_ROOT / _script_relative_path(script_name, ".psc")
    patch = _script_patch_source(script_name)
    assert patch is not None
    merged = _merge_script_method_patches(
        skeleton_path.read_text(encoding="utf-8"), patch
    )
    return patch, merged


@pytest.mark.parametrize(("script_name", "expected_members"), PATCH_MEMBERS.items())
def test_objectaf01_patches_merge_once_and_native_compile(
    script_name: str, expected_members: set[str]
) -> None:
    patch, merged = _merged(script_name)
    members = [
        name
        for kind, name, _start, _end in _iter_top_level_papyrus_members(
            merged.splitlines()
        )
        if kind in {"function", "event"}
    ]

    assert "Scriptname" not in patch
    for member_name in expected_members:
        assert members.count(member_name) == 1
    assert _merge_script_method_patches(merged, patch) == merged

    base_source = _fo4_base_source()
    assert base_source is not None, "FO4 base Papyrus sources unavailable"
    result = compile_psc(
        merged,
        imports=[str(SOURCE_ROOT), str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=str(_script_relative_path(script_name, ".psc")),
    )
    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, f"{script_name}:\n{diagnostics}"
    assert result.pex_bytes is not None


def test_controller_owns_only_the_pre_stage_20_protectron_message() -> None:
    patch, _merged_source = _merged(CONTROLLER)
    init = _member_body(patch, "onquestinit")
    activate = _member_body(patch, "objectreference.onactivate")

    assert 'RegisterForRemoteEvent(protectronRef, "OnActivate")' in init
    assert "akSender == protectronRef" in activate
    assert "akActionRef == Game.GetPlayer()" in activate
    assert "!IsStageDone(20)" in activate
    assert "W05_RE_ObjectAF01_Protectron_InteractMSG.Show()" in activate
    assert "Scavenger" not in patch
    assert "RepairFurniture" not in patch
    assert "TestObject" not in patch


def test_attack_callback_frees_furniture_then_changes_relation_then_replans() -> None:
    patch, _merged_source = _merged(ATTACK)
    quest_patch, _merged_qf = _merged(QUEST_FRAGMENT)
    begin = _member_body(patch, "fragment_phase_02_begin")
    end = _member_body(patch, "fragment_phase_02_end")
    shutdown = _member_body(quest_patch, "fragment_stage_1000_item_00")

    assert begin.index("repairFurniture.Disable()") < begin.index(".SetEnemy(")
    assert begin.index(".SetEnemy(") < begin.index("protectronRef.EvaluatePackage()")
    assert ".SetAlly(" in end
    assert ".SetAlly(" in shutdown
    assert ".SetEnemy(" not in end
    assert "StartCombat" not in patch
    assert "ScavengerRef" not in patch


def test_explosion_callback_is_the_only_self_destruct_cast_owner() -> None:
    explosion_patch, _merged_source = _merged(EXPLOSION)
    quest_patch, _merged_qf = _merged(QUEST_FRAGMENT)
    phase_2 = _member_body(explosion_patch, "fragment_phase_02_end")
    phase_5 = _member_body(explosion_patch, "fragment_phase_05_end")
    stage_25 = _member_body(quest_patch, "fragment_stage_0025_item_00")

    assert ".Cast(protectronRef, protectronRef)" in phase_2
    assert "repairFurniture.Disable()" in phase_5
    assert "W05_RE_ObjectAF01_Explosion.Start()" in stage_25
    assert ".Cast(" not in stage_25
    assert (explosion_patch + quest_patch).count(".Cast(") == 1
    assert "ScavengerRef" not in explosion_patch


def test_fix_and_sandbox_callbacks_use_only_their_bound_operands() -> None:
    fix_patch, _merged_fix = _merged(FIX_ROBOT)
    sandbox_patch, _merged_sandbox = _merged(SANDBOX)
    fix = _member_body(fix_patch, "fragment_phase_03_end")

    assert fix.index("repairFurniture.Disable()") < fix.index(
        "scavengerActor.EvaluatePackage()"
    )
    assert "scavengerActor.SnapIntoInteraction(repairFurniture)" in sandbox_patch
    assert "SetStage(" not in fix_patch + sandbox_patch
    assert ".Cast(" not in fix_patch + sandbox_patch


def test_source_live_carrier_contract_is_frozen() -> None:
    evidence = json.loads(FIXTURE.read_text(encoding="utf-8"))

    quest = evidence["quest"]
    assert quest["form_id"] == "0056A1D1"
    assert set(quest["source_root_bindings"]) - set(quest["live_root_bindings"]) == {
        "Aggression"
    }
    assert quest["aliases"] == {
        "test_object": 14,
        "dis_protectron": 18,
        "scavenger": 19,
        "repair_furniture": 20,
        "player_helper": 23,
    }
    assert quest["stages"] == [10, 20, 23, 25, 27, 28, 30, 1000]

    scenes = evidence["scenes"]
    for scene in scenes.values():
        assert scene["source_bindings"] == scene["live_bindings"]
    assert scenes["0056BC6B"]["declared_but_unbound"] == ["ScavengerRef"]
    assert scenes["0056A252"]["declared_but_unbound"] == ["ScavengerRef"]
    assert scenes["0056A251"]["declared_but_unbound"] == []
    assert scenes["0056A24C"]["declared_but_unbound"] == []

    assert evidence["native_topology"] == {
        "robot_faction": "0056BC69",
        "human_faction": "0056BC6A",
        "initial_relationship": "Ally",
        "repair_furniture": "000D96C3",
        "scavenger_link_keyword": "LinkCustom03",
        "repair_package": "SitLinkCustom03",
        "broken_robot_keyword": "007AC618",
        "interact_message": "0059E0F7",
        "self_destruct_spell": "0056A24F",
    }
    assert all(
        pex["members"] == [] for pex in evidence["source_client_pex"].values()
    )
