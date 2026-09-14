from __future__ import annotations

from collections import Counter

import pytest

from bacup_lib.workflows.unified import (
    _iter_top_level_papyrus_members,
    _script_patch_source,
)


def _fragment_member(stage: int, item: int = 0) -> str:
    return f"fragment_stage_{stage:04d}_item_{item:02d}"


OBJECTIVE_CASES: dict[str, tuple[tuple[int, int], ...]] = {
    "Fragments:Quests:QF_W05_MQR_201P_0040D28D": tuple(
        (stage, 0)
        for stage in (
            100,
            200,
            300,
            400,
            500,
            600,
            610,
            700,
            800,
            900,
            1000,
            1100,
            1200,
            1300,
            1400,
            1700,
            1705,
            1800,
            1900,
            9000,
            10000,
        )
    ),
    "Fragments:Quests:QF_W05_MQR_202P_0041C9E6": tuple(
        (stage, 0)
        for stage in (
            100,
            200,
            300,
            310,
            400,
            500,
            600,
            620,
            700,
            800,
            900,
            920,
            940,
            970,
            1000,
            1100,
            1300,
            1400,
            1500,
            1510,
            1600,
            1610,
            1700,
            1800,
            9000,
        )
    ),
    "Fragments:Quests:QF_W05_MQR_203P_0042F31B": (
        *tuple(
            (stage, 0)
            for stage in (
                2,
                100,
                200,
                300,
                400,
                405,
                499,
                500,
                550,
                600,
                605,
                700,
                710,
                800,
                810,
                900,
                950,
                1000,
                1100,
                1110,
                1200,
                1210,
                1300,
                1350,
                1400,
                1500,
                1510,
                1600,
                1601,
                1700,
                1715,
                1720,
                1750,
                1800,
                1900,
                2000,
                2100,
                2110,
                2200,
                2300,
                2400,
                5100,
                7100,
                8100,
                8110,
                8200,
                8230,
                8250,
                9000,
                9998,
            )
        ),
    ),
    "Fragments:Quests:QF_W05_MQR_204P_00535E55": tuple(
        (stage, 0)
        for stage in (
            2,
            3,
            4,
            100,
            150,
            151,
            160,
            200,
            300,
            310,
            400,
            500,
            510,
            520,
            530,
            540,
            600,
            700,
            800,
            810,
            820,
            830,
            840,
            850,
            900,
            1000,
            1100,
            5000,
            5100,
            5200,
            5300,
            9000,
        )
    ),
    "Fragments:Quests:QF_W05_MQR_205P_00548B7A": (
        *tuple(
            (stage, 0)
            for stage in (
                100,
                105,
                110,
                200,
                210,
                250,
                260,
                300,
                310,
                320,
                325,
                330,
                400,
                410,
                500,
                600,
                700,
                800,
                810,
                900,
                920,
                921,
                1100,
                1200,
                1210,
                1230,
                9000,
            )
        ),
    ),
    "Fragments:Quests:QF_W05_MQR_205P_A_005588EF": tuple(
        (stage, 0) for stage in (100, 200, 300, 400, 500, 600, 9000)
    ),
}


UNRESOLVED_CASES: dict[str, tuple[tuple[int, int], ...]] = {
    "Fragments:Quests:QF_W05_MQR_201P_0040D28D": tuple(
        (stage, 0)
        for stage in (
            1,
            2,
            3,
            4,
            210,
            211,
            220,
            410,
            615,
            620,
            860,
            1410,
            1420,
            1430,
            1440,
            1500,
            1510,
            1520,
            1530,
            1600,
            1620,
            1740,
            1745,
            1801,
            1810,
            1901,
            1910,
            9999,
        )
    ),
    "Fragments:Quests:QF_W05_MQR_202P_0041C9E6": tuple(
        (stage, 0)
        for stage in (
            1,
            2,
            11,
            12,
            13,
            14,
            15,
            16,
            17,
            18,
            19,
            20,
            21,
            22,
            23,
            24,
            25,
            26,
            27,
            28,
            301,
            605,
            630,
            910,
            1017,
            1020,
            1210,
            1220,
            1511,
            1810,
            1811,
            9999,
            550,
            610,
            650,
            750,
            930,
            1010,
            1150,
            1520,
            1530,
            1650,
        )
    ),
    "Fragments:Quests:QF_W05_MQR_203P_0042F31B": (
        *tuple(
            (stage, 0)
            for stage in (
                410,
                420,
                510,
                610,
                815,
                820,
                822,
                901,
                910,
                1050,
                1205,
                1215,
                1220,
                1222,
                1301,
                1310,
                1450,
                1602,
                1605,
                1610,
                1703,
                1705,
                1706,
                1710,
                2010,
                2210,
                2220,
                2221,
                5000,
                5200,
                5300,
                6000,
                7000,
                7110,
                7120,
                7200,
                7300,
                8000,
                8210,
                8220,
                9001,
                9999,
                10000,
            )
        ),
        (800, 1),
        (1200, 1),
    ),
    "Fragments:Quests:QF_W05_MQR_204P_00535E55": tuple(
        (stage, 0)
        for stage in (
            511,
            512,
            513,
            521,
            522,
            531,
            532,
            533,
            541,
            550,
            560,
            950,
            960,
            970,
            1110,
            1120,
            5210,
            5310,
            10000,
        )
    ),
    "Fragments:Quests:QF_W05_MQR_205P_00548B7A": (
        *tuple(
            (stage, 0)
            for stage in (
                1,
                106,
                305,
                315,
                550,
                560,
                610,
                615,
                620,
                710,
                905,
                906,
                910,
                915,
                930,
                940,
                1105,
                1106,
                1107,
                1110,
                1111,
                1120,
                1220,
                1240,
                1250,
            )
        ),
        (930, 1),
        (940, 1),
    ),
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
        "W05_MQR_205P_TurretsOffScript": {"ontriggerenter"},
        "W05_MQR_PlayerVault79KeypadObjective": {"onlocationchange"},
        "WL019_BookshelfScript": {"onactivate"},
    }
)


def _member_name_list(source: str) -> list[str]:
    return [
        name
        for kind, name, _start, _end in _iter_top_level_papyrus_members(
            source.splitlines()
        )
        if kind in {"function", "event"}
    ]


def _member_names(source: str) -> set[str]:
    return set(_member_name_list(source))


@pytest.mark.parametrize(("base_name", "expected_members"), PATCH_CASES.items())
def test_legacy_patch_case_members_remain_present_and_unique(
    base_name: str, expected_members: set[str]
):
    patch = _script_patch_source(base_name)
    assert patch is not None
    assert not any(
        line.strip().lower().startswith("scriptname ") for line in patch.splitlines()
    )
    assert not any(
        line.strip().lower().startswith(("state ", "auto state "))
        for line in patch.splitlines()
    )
    assert expected_members.issubset(_member_names(patch))
    assert Counter(_member_name_list(patch)) == Counter(
        {name: 1 for name in _member_names(patch)}
    )


@pytest.mark.parametrize(("base_name", "members"), OBJECTIVE_CASES.items())
def test_legacy_qf_manifest_members_remain_present(
    base_name: str, members: tuple[tuple[int, int], ...]
):
    patch = _script_patch_source(base_name)
    assert patch is not None
    assert {_fragment_member(stage, item) for stage, item in members}.issubset(
        _member_names(patch)
    )


@pytest.mark.parametrize("base_name", OBJECTIVE_CASES)
def test_completed_mega_fragment_patches_have_no_todo_marker(base_name: str):
    patch = _script_patch_source(base_name)
    assert patch is not None
    assert "; TODO" not in patch


def test_choice_guards_all_calls_and_authors_failure_stage():
    patch = _script_patch_source("Fragments:Quests:QF_W05_MQR_Choice_005930B2")
    assert patch is not None
    assert "fragment_stage_9999_item_00" in _member_names(patch)
    assert "If W05_MQR_204P_WarningMSG != None" in patch
    for quest_name in (
        "W05_MQS_201P",
        "W05_MQS_202P",
        "W05_MQS_203P",
        "W05_MQS_Choice",
    ):
        guard = f"If {quest_name} != None"
        call = f"{quest_name}.Stop()"
        assert patch.index(guard) < patch.index(call)


def test_repair_and_alias_bodies_have_approved_guards_and_reachable_calls():
    breaker = _script_patch_source("W05_MQR_201P_ExplosiveBreakerScript")
    assert breaker is not None
    assert (
        "Event OnClose(ObjectReference akSenderRef, ObjectReference akActionRef)"
        in breaker
    )
    assert "(Self as RefCollectionAlias).GetCount()" in breaker
    assert "(Self as RefCollectionAlias).GetAt(breakerIndex)" in breaker
    assert "breakerCount == 0" in breaker
    assert "breakerRef == None || breakerRef.GetOpenState() != 3" in breaker
    assert breaker.index("While breakerIndex < breakerCount") < breaker.index(
        "owningQuest.SetStage(1745)"
    )

    reader = _script_patch_source("W05_MQR_202P_IDCardReaderScript")
    assert reader is not None
    assert reader.index("akActionRef != Game.GetPlayer()") < reader.index(
        "owningQuest.SetStage(550)"
    )

    player = _script_patch_source("W05_MQR_202P_PlayerScript")
    assert player is not None
    location_guard = "LocToxicGraftonSteelUndergroundLocation == None"
    prerequisite_guard = "owningQuest.IsStageDone(200)"
    idempotent_guard = "!owningQuest.IsStageDone(300)"
    stage_producer = "owningQuest.SetStage(300)"
    assert player.index(location_guard) < player.index(prerequisite_guard)
    assert player.index(prerequisite_guard) < player.index(idempotent_guard)
    assert player.index(idempotent_guard) < player.index(stage_producer)
    assert "owningQuest.SetStage(310)" not in player

    item = _script_patch_source("W05_MQR_202P_RaRaItemPickedUpScript")
    assert item is not None
    assert item.index("akNewContainer != Game.GetPlayer()") < item.index(
        "owningQuest.SetObjectiveCompleted(1610)"
    )

    vent = _script_patch_source("W05_MQR_202P_VentMarkerScript")
    assert vent is not None
    assert vent.index("SceneToPlay != None") < vent.index("SceneToPlay.Start()")

    bench = _script_patch_source("W05_MQR_203P_BenchScript")
    assert bench is not None
    assert (
        "akActionRef == Game.GetPlayer() && W05_MQR_203P_PassTimeSpell != None" in bench
    )

    portal = _script_patch_source("W05_MQR_203P_DoorPortalScript")
    assert portal is not None
    assert portal.index("Alias_Destination == None") < portal.index(
        "Alias_Destination.GetReference()"
    )
    assert portal.index("destinationRef != None") < portal.index(
        "akActionRef.MoveTo(destinationRef)"
    )

    turrets = _script_patch_source("W05_MQR_205P_TurretsOffScript")
    assert turrets is not None
    assert turrets.index("akActionRef != Game.GetPlayer()") < turrets.index(
        "SecurityRoomTurrets.DisableAll()"
    )
    assert (
        "owningQuestScript != None && owningQuestScript.SecurityRoomTurrets != None"
        in turrets
    )


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
    assert bookshelf.index("bookshelfRef != None") < bookshelf.index(
        "bookshelfRef.DisableNoWait()"
    )
    assert bookshelf.index("navcutRef != None") < bookshelf.index(
        "navcutRef.DisableNoWait()"
    )


def test_still_evidence_blocked_rows_remain_unpatched():
    combined = "\n".join(_script_patch_source(name) or "" for name in PATCH_CASES)
    for forbidden in (
        "CompleteQuest(",
        "ServerGrantReward(",
    ):
        assert forbidden.lower() not in combined.lower()

    for unpatched in (
        "W05_MQR_203P_TurretScript",
        "W05_MQR_204P_LevScript",
    ):
        assert _script_patch_source(unpatched) is None


def test_patch_case_count_matches_approved_contract_rows():
    assert len(PATCH_CASES) == 19
