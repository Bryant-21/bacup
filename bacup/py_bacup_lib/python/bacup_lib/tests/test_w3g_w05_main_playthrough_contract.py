from __future__ import annotations

from pathlib import Path

from bacup_lib.workflows.unified import (
    _iter_top_level_papyrus_members,
    _script_patch_source,
)


REPO_ROOT = Path(__file__).resolve().parents[5]
DOC_ROOT = REPO_ROOT / "bacup" / "docs" / "stub_restoration"
TODO_PATH = DOC_ROOT / "TODO.md"

CORE_COUNTS = {
    "Fragments:Quests:QF_W05_MQ_000P_005698E4": 26,
    "Fragments:Quests:QF_W05_MQ_001P_Wayward_00405E14": 40,
    "Fragments:Quests:QF_W05_MQ_002P_Radical_0040F5BE": 67,
    "Fragments:Quests:QF_W05_MQ_003P_Muscle_0041A39D": 62,
    "Fragments:Quests:QF_W05_MQ_004P_Crane_0041C976": 66,
    "Fragments:Quests:QF_W05_MQ_101P_003FBBB2": 48,
    "Fragments:Quests:QF_W05_MQ_101P_A_003FBC0D": 55,
    "Fragments:Quests:QF_W05_MQ_101P_B_003FBC10": 16,
    "Fragments:Quests:QF_W05_MQ_102P_003FFACF": 49,
}

FOUNDATION_COUNTS = {
    "Fragments:Quests:QF_W05_MQSettlers_201P_Indus_003F28C3": 59,
    "Fragments:Quests:QF_W05_MQS_202P_Acrobat_003F28C7": 40,
    "Fragments:Quests:QF_W05_MQS_203P_0040571C": 48,
    "Fragments:Quests:QF_W05_MQS_Choice_00592500": 3,
    "Fragments:Quests:QF_W05_MQS_204P_0040C458": 36,
    "Fragments:Quests:QF_W05_MQS_205P_0041CB6D": 33,
}

RAIDER_COUNTS = {
    "Fragments:Quests:QF_W05_MQR_201P_0040D28D": 49,
    "Fragments:Quests:QF_W05_MQR_201P_Track_RadioQ_0040D28C": 1,
    "W05_MQR_201P_ExplosiveBreakerScript": 1,
    "Fragments:Quests:QF_W05_MQR_202P_0041C9E6": 67,
    "W05_MQR_202P_IDCardReaderScript": 1,
    "W05_MQR_202P_PlayerScript": 1,
    "W05_MQR_202P_RaRaItemPickedUpScript": 1,
    "W05_MQR_202P_VentMarkerScript": 1,
    "Fragments:Quests:QF_W05_MQR_203P_0042F31B": 95,
    "W05_MQR_203P_BenchScript": 1,
    "W05_MQR_203P_DoorPortalScript": 1,
    "W05_MQR_203P_WinnersCupBlackOutScript": 2,
    "Fragments:Quests:QF_W05_MQR_Choice_005930B2": 4,
    "Fragments:Quests:QF_W05_MQR_204P_00535E55": 51,
    "Fragments:Quests:QF_W05_MQR_205P_00548B7A": 54,
    "Fragments:Quests:QF_W05_MQR_205P_A_005588EF": 7,
    "W05_MQR_205P_TurretsOffScript": 1,
}

RESTORED_RAIDER_HELPERS = {
    "W05_MQR_201P_IntercomTriggerScript": 3,
    "W05_MQR_205P_RaRaCombatScript": 1,
    "W05_MQR_205P_RaRaCowerTriggerScript": 1,
    "W05_MQR_205P_ScannerFurnitureScript": 1,
    "W05_MQR_205P_SecurityTriggerScript": 1,
    "W05_MQR_205P_VentSequenceScript": 1,
}

ZERO_MARKER_SCRIPTS = {
    "Fragments:Quests:QF_W05_MQ_101P_Radio_003FBBB3",
    "Fragments:Quests:QF_W05_MQ_102P_A_003FFC02",
    "Fragments:Quests:QF_W05_MQ_102P_B_003FFC00",
    "Fragments:Quests:QF_W05_MQR_205P_00548B7A",
}

def _member_names(source: str) -> list[str]:
    return [
        name
        for kind, name, _start, _end in _iter_top_level_papyrus_members(
            source.splitlines()
        )
        if kind in {"function", "event"}
    ]


def _entries(prefix: str) -> list[dict[str, str]]:
    entries: list[dict[str, str]] = []
    for line in TODO_PATH.read_text(encoding="utf-8").splitlines():
        if line.startswith(prefix):
            entries.append(dict(field.split("=", 1) for field in line.split("|")[1:]))
    return entries


def test_callable_counts_match_live_bound_manifests():
    for script_name, expected_count in (
        CORE_COUNTS | FOUNDATION_COUNTS | RAIDER_COUNTS | RESTORED_RAIDER_HELPERS
    ).items():
        patch = _script_patch_source(script_name)
        assert patch is not None
        member_names = _member_names(patch)
        if script_name == "Fragments:Quests:QF_W05_MQ_001P_Wayward_00405E14":
            fragment_names = [
                name for name in member_names if name.startswith("fragment_stage_")
            ]
            assert len(fragment_names) == expected_count == 40
            assert member_names == [
                *fragment_names,
                "attemptradicalhandoff",
                "ontimer",
            ]
        else:
            assert len(member_names) == expected_count, script_name


def test_marker_counts_follow_real_gap_registry():
    todo_entries = [
        entry
        for prefix in (
            "W05-TODO|",
            "W05-HELPER-TODO|",
            "W05-SEMANTIC-TODO|",
        )
        for entry in _entries(prefix)
    ]
    todo_scripts = {
        entry["script"] for entry in todo_entries if entry["patch"] != "none"
    }
    route_scripts = set(CORE_COUNTS) | set(FOUNDATION_COUNTS) | set(RAIDER_COUNTS)
    for script_name in todo_scripts | ZERO_MARKER_SCRIPTS | route_scripts:
        patch = _script_patch_source(script_name)
        assert patch is not None
        expected = 1 if script_name in todo_scripts - ZERO_MARKER_SCRIPTS else 0
        assert patch.splitlines().count("; TODO") == expected
