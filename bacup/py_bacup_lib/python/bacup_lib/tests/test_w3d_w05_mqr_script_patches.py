from __future__ import annotations

from collections import Counter
from pathlib import Path

import pytest

from bacup_lib.workflows.unified import (
    _iter_top_level_papyrus_members,
    _merge_script_method_patches,
    _script_patch_source,
    _script_relative_path,
)
from creation_lib.pex import decompile_pex
from creation_lib.pex.native_runtime import compile_psc

from bacup_lib.tests.test_terminal_fragment_script_patches import _fo4_base_source


REPO_ROOT = Path(__file__).resolve().parents[5]
DEPLOYED_SCRIPTS_ROOT = REPO_ROOT / "mods" / "SeventySix" / "data" / "Scripts"
GENERATED_SOURCE_ROOT = REPO_ROOT / "mods" / "SeventySix" / "Scripts" / "Source" / "User"


def _fragment_member(stage: int, item: int = 0) -> str:
    return f"fragment_stage_{stage:04d}_item_{item:02d}"


OBJECTIVE_CASES: dict[str, tuple[tuple[int, int], ...]] = {
    "Fragments:Quests:QF_W05_MQR_201P_0040D28D": tuple(
        (stage, 0)
        for stage in (
            1, 2, 3, 4, 100, 200, 210, 211, 220, 300, 400, 410, 500,
            600, 610, 615, 620, 700, 800, 860, 900, 1000, 1100, 1200,
            1300, 1400, 1410, 1420, 1600, 1620, 1700, 1705, 1740, 1745,
            1800, 1801, 1810, 1900, 1901, 1910, 9000, 9999, 10000,
        )
    ),
    "Fragments:Quests:QF_W05_MQR_202P_0041C9E6": tuple(
        (stage, 0)
        for stage in (
            100, 200, 300, 310, 400, 500, 550, 600, 610, 620, 650, 700,
            750, 800, 900, 920, 930, 940, 970, 1000, 1010, 1100, 1150,
            1300, 1400, 1500, 1510, 1520, 1530, 1600, 1610, 1650, 1700,
            1800, 9000,
        )
    ),
    "Fragments:Quests:QF_W05_MQR_203P_0042F31B": (
        (2, 0), (100, 0), (200, 0), (300, 0), (400, 0), (405, 0),
        (499, 0), (500, 0), (550, 0), (600, 0), (605, 0), (700, 0),
        (710, 0), (800, 0), (800, 1), (810, 0), (900, 0), (910, 0),
        (950, 0), (1000, 0), (1100, 0), (1110, 0), (1200, 0),
        (1200, 1), (1210, 0), (1300, 0), (1310, 0), (1350, 0),
        (1400, 0), (1500, 0), (1510, 0), (1600, 0), (1601, 0),
        (1610, 0), (1700, 0), (1705, 0), (1715, 0), (1720, 0),
        (1750, 0), (1800, 0), (1900, 0), (2000, 0), (2100, 0),
        (2110, 0), (2200, 0), (2210, 0), (2220, 0), (2221, 0),
        (2300, 0), (2400, 0), (5000, 0), (5100, 0), (5200, 0),
        (5300, 0), (6000, 0), (7000, 0), (7100, 0), (7200, 0),
        (7300, 0), (8100, 0), (8200, 0), (9000, 0),
    ),
    "Fragments:Quests:QF_W05_MQR_204P_00535E55": tuple(
        (stage, 0)
        for stage in (
            2, 3, 4, 100, 150, 151, 160, 200, 300, 310, 400, 500, 510,
            520, 530, 540, 600, 700, 800, 810, 820, 830, 840, 850, 900,
            970, 1000, 1100, 5000, 5100, 5200, 5300, 9000,
        )
    ),
    "Fragments:Quests:QF_W05_MQR_205P_00548B7A": (
        *tuple(
            (stage, 0)
            for stage in (
                1, 100, 105, 106, 110, 200, 210, 250, 260, 300, 305,
                310, 315, 320, 325, 330, 400, 410, 500, 550, 560, 600,
                610, 615, 620, 700, 710, 800, 810, 900, 905, 906, 910,
                915, 920, 921, 930, 940, 1100, 1105, 1106, 1107, 1110,
                1111, 1120, 1200, 1210, 1220, 1230, 1240, 1250, 9000,
            )
        ),
        (930, 1),
        (940, 1),
    ),
    "Fragments:Quests:QF_W05_MQR_205P_A_005588EF": tuple(
        (stage, 0) for stage in (100, 200, 300, 400, 500, 600, 9000)
    ),
}


UNRESOLVED_CASES: dict[str, tuple[tuple[int, int], ...]] = {
    "Fragments:Quests:QF_W05_MQR_201P_0040D28D": tuple(
        (stage, 0)
        for stage in (1430, 1440, 1500, 1510, 1520, 1530)
    ),
    "Fragments:Quests:QF_W05_MQR_202P_0041C9E6": tuple(
        (stage, 0)
        for stage in (
            1, 2, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22,
            23, 24, 25, 26, 27, 28, 301, 605, 630, 910, 1017, 1020,
            1210, 1220, 1511, 1810, 1811, 9999,
        )
    ),
    "Fragments:Quests:QF_W05_MQR_203P_0042F31B": tuple(
        (stage, 0)
        for stage in (
            410, 420, 510, 610, 815, 820, 822, 901, 1050, 1205,
            1215, 1220, 1222, 1301, 1450, 1602, 1605, 1703, 1706,
            1710, 2010, 7110, 7120, 8000, 8110, 8210, 8220, 8230,
            8250, 9001, 9998, 9999, 10000,
        )
    ),
    "Fragments:Quests:QF_W05_MQR_204P_00535E55": tuple(
        (stage, 0)
        for stage in (
            511, 512, 513, 521, 522, 531, 532, 533, 541, 550, 560,
            950, 960, 1110, 1120, 5210, 5310, 10000,
        )
    ),
    "Fragments:Quests:QF_W05_MQR_205P_00548B7A": (),
    "Fragments:Quests:QF_W05_MQR_205P_A_005588EF": (),
}


PATCH_CASES: dict[str, set[str]] = {
    base_name: {_fragment_member(stage, item) for stage, item in members}
    for base_name, members in OBJECTIVE_CASES.items()
}
PATCH_CASES.update(
    {
        "Fragments:Quests:QF_W05_MQR_201P_Track_RadioQ_0040D28C": {
            "fragment_stage_1000_item_00"
        },
        "Fragments:Quests:QF_W05_MQR_Choice_005930B2": {
            "fragment_stage_0100_item_00",
            "fragment_stage_0200_item_00",
            "fragment_stage_9000_item_00",
        },
        "W05_MQR_201P_ExplosiveBreakerScript": {"onclose"},
        "W05_MQR_201P_IntercomTriggerScript": {"ontriggerenter"},
        "W05_MQR_202P_IDCardReaderScript": {"onactivate"},
        "W05_MQR_202P_PlayerScript": {"onlocationchange"},
        "W05_MQR_202P_RaRaItemPickedUpScript": {"oncontainerchanged"},
        "W05_MQR_202P_VentMarkerScript": {"onactivate"},
        "W05_MQR_203P_BenchScript": {"onactivate"},
        "W05_MQR_203P_DoorPortalScript": {"onactivate"},
        "W05_MQR_203P_WinnersCupBlackOutScript": {
            "oneffectstart",
            "oneffectfinish",
        },
        "W05_MQR_205P_RaRaCombatScript": {"oncombatstatechanged"},
        "W05_MQR_205P_RaRaCowerTriggerScript": {"ontriggerenter"},
        "W05_MQR_205P_ScannerFurnitureScript": {"onactivate"},
        "W05_MQR_205P_SecurityTriggerScript": {"ontriggerenter"},
        "W05_MQR_205P_TurretsOffScript": {"ontriggerenter"},
        "W05_MQR_205P_VentSequenceScript": {"onactivate"},
        "W05_MQR_PlayerVault79KeypadObjective": {"onlocationchange"},
        "WL019_BookshelfScript": {"onactivate"},
    }
)


def _member_name_list(source: str) -> list[str]:
    return [
        name
        for kind, name, _start, _end in _iter_top_level_papyrus_members(source.splitlines())
        if kind in {"function", "event"}
    ]


def _member_names(source: str) -> set[str]:
    return set(_member_name_list(source))


def _production_skeleton(base_name: str) -> str:
    pex_path = DEPLOYED_SCRIPTS_ROOT / _script_relative_path(base_name, ".pex")
    assert pex_path.is_file(), f"deployed production PEX unavailable: {pex_path}"
    return decompile_pex(pex_path, fo4_api_compat=True)


def _merged_production_source(base_name: str) -> str:
    patch = _script_patch_source(base_name)
    assert patch is not None
    return _merge_script_method_patches(_production_skeleton(base_name), patch)


@pytest.mark.parametrize(("base_name", "expected_members"), PATCH_CASES.items())
def test_patch_case_has_exact_default_state_members(base_name: str, expected_members: set[str]):
    patch = _script_patch_source(base_name)
    assert patch is not None
    assert not any(line.strip().lower().startswith("scriptname ") for line in patch.splitlines())
    assert not any(line.strip().lower().startswith(("state ", "auto state ")) for line in patch.splitlines())
    assert _member_names(patch) == expected_members
    assert Counter(_member_name_list(patch)) == Counter({name: 1 for name in expected_members})


@pytest.mark.parametrize(("base_name", "members"), OBJECTIVE_CASES.items())
def test_qf_manifest_uses_the_exact_live_fragment_indices(
    base_name: str, members: tuple[tuple[int, int], ...]
):
    patch = _script_patch_source(base_name)
    assert patch is not None
    assert _member_names(patch) == {
        _fragment_member(stage, item) for stage, item in members
    }


@pytest.mark.parametrize("base_name", OBJECTIVE_CASES)
def test_unresolved_mega_fragment_members_remain_absent(base_name: str):
    patch = _script_patch_source(base_name)
    assert patch is not None
    unresolved = {
        _fragment_member(stage, item)
        for stage, item in UNRESOLVED_CASES[base_name]
    }
    assert _member_names(patch).isdisjoint(unresolved)


def test_choice_guards_all_calls_and_leaves_failure_stage_absent():
    patch = _script_patch_source("Fragments:Quests:QF_W05_MQR_Choice_005930B2")
    assert patch is not None
    assert "fragment_stage_9999_item_00" not in _member_names(patch)
    assert "If W05_MQR_204P_WarningMSG != None" in patch
    for quest_name in ("W05_MQS_201P", "W05_MQS_202P", "W05_MQS_203P", "W05_MQS_Choice"):
        guard = f"If {quest_name} != None"
        call = f"{quest_name}.Stop()"
        assert patch.index(guard) < patch.index(call)


def test_repair_and_alias_bodies_have_approved_guards_and_reachable_calls():
    breaker = _script_patch_source("W05_MQR_201P_ExplosiveBreakerScript")
    assert breaker is not None
    assert "Event OnClose(ObjectReference akSenderRef, ObjectReference akActionRef)" in breaker
    assert "(Self as RefCollectionAlias).GetCount()" in breaker
    assert "(Self as RefCollectionAlias).GetAt(breakerIndex)" in breaker
    assert "breakerCount == 0" in breaker
    assert "breakerRef == None || breakerRef.GetOpenState() != 3" in breaker
    assert breaker.index("While breakerIndex < breakerCount") < breaker.index("owningQuest.SetStage(1745)")

    reader = _script_patch_source("W05_MQR_202P_IDCardReaderScript")
    assert reader is not None
    assert reader.index("akActionRef != Game.GetPlayer()") < reader.index("owningQuest.SetStage(550)")

    player = _script_patch_source("W05_MQR_202P_PlayerScript")
    assert player is not None
    assert player.index("LocToxicGraftonSteelUndergroundLocation == None") < player.index("owningQuest.SetStage(310)")

    item = _script_patch_source("W05_MQR_202P_RaRaItemPickedUpScript")
    assert item is not None
    assert item.index("akNewContainer != Game.GetPlayer()") < item.index("owningQuest.SetObjectiveCompleted(1610)")

    vent = _script_patch_source("W05_MQR_202P_VentMarkerScript")
    assert vent is not None
    assert vent.index("SceneToPlay != None") < vent.index("SceneToPlay.Start()")

    bench = _script_patch_source("W05_MQR_203P_BenchScript")
    assert bench is not None
    assert "akActionRef == Game.GetPlayer() && W05_MQR_203P_PassTimeSpell != None" in bench

    portal = _script_patch_source("W05_MQR_203P_DoorPortalScript")
    assert portal is not None
    assert portal.index("Alias_Destination == None") < portal.index("Alias_Destination.GetReference()")
    assert portal.index("destinationRef != None") < portal.index("akActionRef.MoveTo(destinationRef)")

    turrets = _script_patch_source("W05_MQR_205P_TurretsOffScript")
    assert turrets is not None
    assert turrets.index("akActionRef != Game.GetPlayer()") < turrets.index("SecurityRoomTurrets.DisableAll()")
    assert "owningQuestScript != None && owningQuestScript.SecurityRoomTurrets != None" in turrets


def test_keypad_and_bookshelf_cache_and_guard_resolved_references():
    keypad = _script_patch_source("W05_MQR_PlayerVault79KeypadObjective")
    assert keypad is not None
    assert "owningQuest == None || InstancedLocationAlias == None" in keypad
    assert "targetLocation == None || akNewLoc != targetLocation" in keypad
    assert "currentStage >= PreReqStage && currentStage < EndOnStage" in keypad
    assert "!owningQuest.IsObjectiveDisplayed(KeypadObjective)" in keypad

    bookshelf = _script_patch_source("WL019_BookshelfScript")
    assert bookshelf is not None
    assert bookshelf.count("GetLinkedRef(bookshelfKeyword)") == 1
    assert bookshelf.count("GetLinkedRef(navcutKeyword)") == 1
    assert bookshelf.index("bookshelfRef != None") < bookshelf.index("bookshelfRef.DisableNoWait()")
    assert bookshelf.index("navcutRef != None") < bookshelf.index("navcutRef.DisableNoWait()")


def test_unapproved_systems_and_rows_remain_unpatched():
    combined = "\n".join(_script_patch_source(name) or "" for name in PATCH_CASES)
    for forbidden in (
        "ForceRefTo",
        "defaultquestencounterwavescript",
        "encounterwaveparentscript",
        "Reputation_AV_",
        "Rep_Mod_",
    ):
        assert forbidden.lower() not in combined.lower()

    for unpatched in (
        "W05_MQR_203P_TurretScript",
        "W05_MQR_204P_LevScript",
    ):
        assert _script_patch_source(unpatched) is None


@pytest.mark.parametrize("base_name", PATCH_CASES)
def test_production_merge_preserves_skeleton_and_is_idempotent(base_name: str):
    skeleton = _production_skeleton(base_name)
    patch = _script_patch_source(base_name)
    assert patch is not None
    merged = _merge_script_method_patches(skeleton, patch)

    skeleton_header = next(line for line in skeleton.splitlines() if line.lower().startswith("scriptname "))
    assert skeleton_header in merged
    for line in skeleton.splitlines():
        if " property " in f" {line.lower()} ":
            assert line in merged

    merged_counts = Counter(_member_name_list(merged))
    for member in PATCH_CASES[base_name]:
        assert merged_counts[member] == 1
    assert _merge_script_method_patches(merged, patch) == merged


@pytest.mark.parametrize("base_name", PATCH_CASES)
def test_production_merge_native_compiles_for_fo4_without_skips(base_name: str):
    base_source = _fo4_base_source()
    assert base_source is not None, "FO4 base Papyrus sources unavailable"
    assert GENERATED_SOURCE_ROOT.is_dir(), f"generated source root unavailable: {GENERATED_SOURCE_ROOT}"

    result = compile_psc(
        _merged_production_source(base_name),
        imports=[str(base_source), str(GENERATED_SOURCE_ROOT)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=f"{base_name.rsplit(':', 1)[-1]}.psc",
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None


def test_patch_case_count_matches_approved_contract_rows():
    assert len(PATCH_CASES) == 25
