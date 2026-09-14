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

EXPECTED_MEMBERS = {
    "Fragments:Quests:QF_DailyOps_VernonDodge_Misc_005CF0C7": {
        "fragment_stage_0100_item_00",
        "fragment_stage_0200_item_00",
        "fragment_stage_0300_item_00",
        "fragment_stage_0400_item_00",
        "fragment_stage_0450_item_00",
        "fragment_stage_0500_item_00",
        "fragment_stage_0600_item_00",
        "fragment_stage_9000_item_00",
    },
    "Fragments:Quests:QF_E06_PocketWatch_Misc_0058912C": {
        "fragment_stage_0100_item_00",
        "fragment_stage_9000_item_00",
    },
    "Fragments:Quests:QF_LC178_WatogaCivicCenterMi_0050DE51": {
        "fragment_stage_0010_item_00",
        "fragment_stage_0100_item_00",
    },
    "Fragments:Quests:QF_LC172_WatogaEmergencyMisc_0050F989": {
        "fragment_stage_0010_item_00",
        "fragment_stage_0100_item_00",
    },
    "Fragments:Quests:QF_LC139_SummersvilleMisc_004F900F": {
        "fragment_stage_0010_item_00",
        "fragment_stage_0020_item_00",
        "fragment_stage_0030_item_00",
        "fragment_stage_0040_item_00",
        "fragment_stage_0100_item_00",
    },
    "Fragments:Quests:QF_LC154_TylerCountyFairgrou_004F8A93": {
        "fragment_stage_0010_item_00",
        "fragment_stage_0100_item_00",
    },
    "Fragments:Quests:QF_LC013_BigBendMisc_0050F877": {
        "fragment_stage_0010_item_00",
        "fragment_stage_0020_item_00",
        "fragment_stage_0030_item_00",
        "fragment_stage_0040_item_00",
        "fragment_stage_0100_item_00",
    },
    "Fragments:Quests:QF_WS01_0050E611": {
        "fragment_stage_0100_item_00",
        "fragment_stage_1000_item_00",
    },
    "WS01script": {"onquestinit", "actor.onkill", "onquestshutdown"},
    "Fragments:Quests:QF_WS02_0050E616": {
        "fragment_stage_0100_item_00",
        "fragment_stage_0200_item_00",
        "fragment_stage_0300_item_00",
        "fragment_stage_0400_item_00",
        "fragment_stage_0500_item_00",
        "fragment_stage_0600_item_00",
        "fragment_stage_0700_item_00",
        "fragment_stage_0800_item_00",
        "markcollected",
        "fragment_stage_1000_item_00",
    },
    "WS02script": {"onquestinit"},
    "VS_MiscTempScript": {"onquestinit", "ontimer", "onquestshutdown"},
    "Fragments:Quests:QF_VS_MiscTemp_0001564E": {
        "fragment_stage_0020_item_00",
        "fragment_stage_0100_item_00",
    },
    "HouseOfScaresScript": {"onquestinit", "actor.onkill", "onquestshutdown"},
    "Fragments:Quests:QF_HouseOfScares_004F82B8": {
        "fragment_stage_0100_item_00",
        "fragment_stage_0200_item_00",
        "fragment_stage_1000_item_00",
    },
}


@pytest.mark.parametrize(("script_name", "expected"), EXPECTED_MEMBERS.items())
def test_legacy_empty_misc_patches_merge_once_and_compile(
    script_name: str, expected: set[str]
):
    source_path = SOURCE_ROOT / _script_relative_path(script_name, ".psc")
    skeleton = source_path.read_text(encoding="utf-8")
    patch = _script_patch_source(script_name)

    assert patch is not None
    assert "Scriptname" not in patch
    assert " Property " not in patch

    merged = _merge_script_method_patches(skeleton, patch)
    members = [
        name
        for _kind, name, _start, _end in _iter_top_level_papyrus_members(
            merged.splitlines()
        )
    ]
    for member in expected:
        assert members.count(member) == 1
    assert set(members) == expected
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


def test_local_progression_contracts_are_explicit():
    dodge = _script_patch_source(
        "Fragments:Quests:QF_DailyOps_VernonDodge_Misc_005CF0C7"
    )
    ws01 = _script_patch_source("WS01script")
    ws02 = _script_patch_source("Fragments:Quests:QF_WS02_0050E616")
    house = _script_patch_source("HouseOfScaresScript")
    vs_misc = _script_patch_source("VS_MiscTempScript")

    assert dodge is not None
    assert "SetObjectiveDisplayed(35, True)" in dodge
    assert "SetValue(AV_ReadyToTurnIn, 1.0)" in dodge
    assert "SetStage(9000)" in dodge
    assert ws01 is not None
    assert 'RegisterForRemoteEvent(playerRef, "OnKill")' in ws01
    assert "akVictim.WornHasKeyword(FeralGhoulGolfClothes)" in ws01
    assert "victimLocation.HasKeyword(LocationKeyword)" in ws01
    assert "WS01MaxKills.GetValue()" in ws01
    assert ws02 is not None
    assert ws02.count("MarkCollected(WS02Holotape") == 7
    assert "SetStage(1000)" in ws02
    assert house is not None
    assert "akVictim.GetRace() == WendigoRace" in house
    assert "akSender.WornHasKeyword(HouseOfScaresOutfit)" in house
    assert vs_misc is not None
    assert "distance <= MaxDistanceToStart" in vs_misc
    assert "distance <= DistanceToCompleteObjective" in vs_misc


def test_online_and_dropped_loot_services_are_not_faked():
    assert _script_patch_source(
        "Fragments:Quests:QF_DailyOps_Mode01_Quest_005A77D4_1"
    ) is None
    assert _script_patch_source("Tutorial_PlayerDeathScript") is None
