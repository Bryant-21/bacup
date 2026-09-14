from __future__ import annotations

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

SFL02_PATCH_MEMBERS = {
    "SFL02_Track_QuestScript": {
        "onquestinit",
        "reconcileruntimeregistrations",
        "objectreference.onitemadded",
        "actor.onitemequipped",
        "objectreference.onactivate",
        "objectreference.ontriggerenter",
        "terminal.onmenuitemrun",
        "onquestshutdown",
        "registeraliasevent",
        "registerterminalevent",
        "unregisteraliasevent",
        "unregisterterminalevent",
        "setstageforaliasform",
        "isaliasreference",
    },
    "Fragments:Quests:QF_SFL02_Track_00131033": {
        "fragment_stage_0010_item_00",
        "fragment_stage_0025_item_00",
        "fragment_stage_0050_item_00",
        "fragment_stage_0100_item_00",
        "fragment_stage_0150_item_00",
        "fragment_stage_0200_item_00",
        "fragment_stage_0200_item_01",
        "fragment_stage_0250_item_00",
        "fragment_stage_0275_item_00",
        "fragment_stage_0300_item_00",
        "fragment_stage_0350_item_00",
        "fragment_stage_0400_item_00",
        "fragment_stage_0450_item_00",
        "fragment_stage_0460_item_00",
        "fragment_stage_0500_item_00",
        "fragment_stage_0550_item_00",
        "fragment_stage_0600_item_00",
        "fragment_stage_0800_item_00",
        "fragment_stage_0900_item_00",
        "fragment_stage_0910_item_00",
        "fragment_stage_0920_item_00",
        "fragment_stage_0930_item_00",
        "fragment_stage_1000_item_00",
        "fragment_stage_1010_item_00",
        "fragment_stage_1020_item_00",
        "fragment_stage_1030_item_00",
        "fragment_stage_1100_item_00",
        "fragment_stage_1200_item_00",
        "fragment_stage_1300_item_00",
        "fragment_stage_2000_item_00",
    },
    "SFL02_Track_PerkScript": {"onentryrun"},
    "Fragments:Quests:QF_SFL02_Track_VertibotQuest_0032BB59": {
        "fragment_stage_0100_item_00",
        "fragment_stage_0200_item_00",
        "fragment_stage_0300_item_00",
        "fragment_stage_1000_item_00",
    },
    "Fragments:Quests:QF_SFL02_Track_RadioQuest_00018ECF": {
        "fragment_stage_1000_item_00"
    },
}


@pytest.mark.parametrize(
    ("script_name", "expected_members"), SFL02_PATCH_MEMBERS.items()
)
def test_sfl02_tracking_patches_merge_once_and_compile(
    script_name: str, expected_members: set[str]
):
    source_path = SOURCE_ROOT / _script_relative_path(script_name, ".psc")
    skeleton = source_path.read_text(encoding="utf-8")
    patch = _script_patch_source(script_name)

    assert patch is not None
    assert "Scriptname" not in patch
    assert "Property" not in patch

    merged = _merge_script_method_patches(skeleton, patch)
    members = [
        name
        for _kind, name, _start, _end in _iter_top_level_papyrus_members(
            merged.splitlines()
        )
    ]
    for member in expected_members:
        assert members.count(member) == 1
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
    assert result.ok, diagnostics
    assert result.pex_bytes is not None


def test_sfl02_tracking_patch_preserves_the_local_progression_contract():
    root_patch = _script_patch_source("SFL02_Track_QuestScript")
    quest_patch = _script_patch_source("Fragments:Quests:QF_SFL02_Track_00131033")
    perk_patch = _script_patch_source("SFL02_Track_PerkScript")
    vertibot_patch = _script_patch_source(
        "Fragments:Quests:QF_SFL02_Track_VertibotQuest_0032BB59"
    )

    assert root_patch is not None
    assert quest_patch is not None
    assert perk_patch is not None
    assert vertibot_patch is not None
    assert "SFL02_Track_Vertibot_QuestStartKeyword.SendStoryEvent()" in quest_patch
    assert "SFL02_Track_Radio_QuestStartKeyword.SendStoryEvent()" in quest_patch
    assert "SFL02_Track_VertibotQuest.Start()" not in quest_patch
    assert "SFL02_Track_RadioQuest.Start()" not in quest_patch
    assert "SFL02_Track_VertibotTerminalCode" in quest_patch
    assert "SFL02_Track_VertibotQuest.SetStage(200)" in perk_patch
    assert "SFL02_Track.SetStage(500)" in vertibot_patch
    assert "SFL02_Track.SetStage(800)" in vertibot_patch
    assert root_patch.count("SetStageForAliasForm(akBaseItem") == 8
    assert root_patch.count("SetStageForAliasForm(akBaseObject") == 3
    assert "SetStage(150)" in root_patch
    assert "SetStage(450)" in root_patch
    assert "SetStage(1200)" in root_patch
    assert quest_patch.count("SetStage(1100)") == 2


def test_sfl02_startup_and_world_callbacks_preserve_runtime_order():
    root_patch = _script_patch_source("SFL02_Track_QuestScript")
    quest_patch = _script_patch_source("Fragments:Quests:QF_SFL02_Track_00131033")
    assert root_patch is not None
    assert quest_patch is not None

    init = next(
        "\n".join(root_patch.splitlines()[start : end + 1])
        for _kind, name, start, end in _iter_top_level_papyrus_members(
            root_patch.splitlines()
        )
        if name == "onquestinit"
    )
    assert init.index("ReconcileRuntimeRegistrations()") < init.index("SetStage(10)")

    reconciliation = next(
        "\n".join(root_patch.splitlines()[start : end + 1])
        for _kind, name, start, end in _iter_top_level_papyrus_members(
            root_patch.splitlines()
        )
        if name == "reconcileruntimeregistrations"
    )
    assert reconciliation.index(
        "If !IsRunning() || IsCompleted()"
    ) < reconciliation.index('UnregisterAliasEvent(53, "OnTriggerEnter")')
    for alias_id in (53, 54, 55):
        unregister = f'UnregisterAliasEvent({alias_id}, "OnTriggerEnter")'
        register = f'RegisterAliasEvent({alias_id}, "OnTriggerEnter")'
        assert reconciliation.count(unregister) == 1
        assert reconciliation.count(register) == 1
        assert reconciliation.index(unregister) < reconciliation.index(register)

    stage_ten = next(
        "\n".join(quest_patch.splitlines()[start : end + 1])
        for _kind, name, start, end in _iter_top_level_papyrus_members(
            quest_patch.splitlines()
        )
        if name == "fragment_stage_0010_item_00"
    )
    assert stage_ten.count("SetStage(25)") == 1
    assert "SetStage(50)" not in stage_ten
    assert "SetStage(100)" not in stage_ten
    assert "Alias_SFL02Player.GetReference() == None" in stage_ten

    trigger = next(
        "\n".join(root_patch.splitlines()[start : end + 1])
        for _kind, name, start, end in _iter_top_level_papyrus_members(
            root_patch.splitlines()
        )
        if name == "objectreference.ontriggerenter"
    )
    assert all(
        f"IsAliasReference(akSender, {alias_id})" in trigger
        for alias_id in (53, 54, 55)
    )
    assert trigger.index("SetStage(50)") < trigger.index("SetStage(150)")

    shutdown = next(
        "\n".join(root_patch.splitlines()[start : end + 1])
        for _kind, name, start, end in _iter_top_level_papyrus_members(
            root_patch.splitlines()
        )
        if name == "onquestshutdown"
    )
    assert all(
        f'UnregisterAliasEvent({alias_id}, "OnTriggerEnter")' in shutdown
        for alias_id in (53, 54, 55)
    )


def test_sfl02_pickup_callbacks_honor_source_prerequisites():
    root_patch = _script_patch_source("SFL02_Track_QuestScript")
    assert root_patch is not None
    added = next(
        "\n".join(root_patch.splitlines()[start : end + 1])
        for _kind, name, start, end in _iter_top_level_papyrus_members(
            root_patch.splitlines()
        )
        if name == "objectreference.onitemadded"
    )

    assert added.index("IsStageDone(300)") < added.index(
        "SetStageForAliasForm(akBaseItem, SignalBooster01, 400)"
    )
    assert added.index("IsStageDone(1200)") < added.index(
        "SetStageForAliasForm(akBaseItem, HolotapeLucy, 1300)"
    )
