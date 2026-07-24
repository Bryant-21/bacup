from __future__ import annotations

import csv
from pathlib import Path

from bacup_lib.workflows.unified import _iter_top_level_papyrus_members


REPO_ROOT = Path(__file__).resolve().parents[5]
TODO_PATH = REPO_ROOT / "bacup" / "docs" / "stub_restoration" / "TODO.md"
STATUS_PATH = REPO_ROOT / "bacup" / "docs" / "stub_restoration" / "status.csv"

QF_PATCHES = {
    "5698E4": "QF_W05_MQ_000P_005698E4.psc",
    "405E14": "QF_W05_MQ_001P_Wayward_00405E14.psc",
    "40F5BE": "QF_W05_MQ_002P_Radical_0040F5BE.psc",
    "41A39D": "QF_W05_MQ_003P_Muscle_0041A39D.psc",
    "41C976": "QF_W05_MQ_004P_Crane_0041C976.psc",
    "3FBBB2": "QF_W05_MQ_101P_003FBBB2.psc",
    "3FBC0D": "QF_W05_MQ_101P_A_003FBC0D.psc",
    "3FBC10": "QF_W05_MQ_101P_B_003FBC10.psc",
    "3FFACF": "QF_W05_MQ_102P_003FFACF.psc",
}
PATCH_ROOT = (
    REPO_ROOT
    / "bacup"
    / "py_bacup_lib"
    / "python"
    / "bacup_lib"
    / "script_patches"
    / "Fragments"
    / "Quests"
)

EXPECTED_CALLABLE_COUNTS = {
    "5698E4": 3,
    "405E14": 20,
    "40F5BE": 33,
    "41A39D": 30,
    "41C976": 31,
    "3FBBB2": 24,
    "3FBC0D": 32,
    "3FBC10": 8,
    "3FFACF": 26,
}

EXPECTED_GROUPS = {
    ("5698E4", "EB"): "0100:00,0150:00,0200:00,0250:00,0300:00,0350:00,0400:00,0450:00,0500:00,0550:00,0999:00,1100:00,1150:00,1200:00,1250:00,1300:00,1350:00,1400:00,1450:00,1500:00,1550:00,1600:00,1650:00,1999:00",
    ("405E14", "EB"): "0103:00,0105:00,0301:00,0302:00,0310:00,0445:00,0450:00,0455:00,0460:00,0470:00,0491:00,0510:00,0515:00,0516:00,0520:00,0522:00,0530:00,0705:00",
    ("40F5BE", "EB"): "0150:00,0160:00,0270:00,0460:00,0498:00,0501:00,0502:00,0504:00,0511:00,0515:00,0525:00,0707:00,0710:00,0725:00,0735:00,0745:00,0746:00,0760:00,0764:00,0765:00,0766:00,0799:00,1100:00,1101:00,1140:00,1290:00,1350:00,1575:00",
    ("41A39D", "EB"): "0103:00,0410:00,0415:00,0450:00,0476:00,1015:00,1025:00,1050:00,1229:00,1232:00,1240:00,1270:00,1275:00,1280:00,1325:00,10000:00",
    ("41C976", "EB"): "0108:00,0109:00,0112:00,0125:00,0200:00,0210:00,0301:00,0401:00,0495:00,0701:00,0703:00,0704:00,0710:00,0775:00,0820:00,0830:00,1105:00,1220:00,1221:00,1243:00",
    ("3FBBB2", "EB"): "0051:00,0052:00,0400:00,0500:00,0550:00,0700:00,0800:00,0805:00,0810:00,0820:00,0830:00,0900:00,1100:00,1290:00,1450:00,1500:00,1510:00,1610:00,1810:00,1820:00,1910:00,1920:00,2000:00",
    ("3FBBB2", "RD"): "1200:00",
    ("3FBC0D", "EB"): "0310:00,0311:00,0320:00,0330:00,0331:00,0375:00,0680:00,0710:00,0730:00,0810:00,0820:00,0830:00,0960:00,1415:00,1430:00,1440:00",
    ("3FBC10", "EB"): "0100:00,0200:00,0232:00,0240:00,0300:00,0350:00,0590:00,0700:00",
    ("3FFACF", "EB"): "0450:00,0560:00,0565:00,0582:00,0595:00,0610:00,0615:00,0630:00,0640:00,0665:00,0682:00,0695:00,0700:00,0710:00,0720:00,0740:00,0800:00,0850:00,0900:00,1000:00,1200:00,10000:00",
}

STATUS_EVIDENCE = {
    **{
        f"Fragments:Quests:{QF_PATCHES[record][:-4]}": "contracts/w3g-w05-main-quest-rough-playthrough.md"
        for record in ("5698E4", "405E14", "40F5BE", "41A39D", "41C976")
    },
    **{
        f"Fragments:Quests:{QF_PATCHES[record][:-4]}": "contracts/w3g-w05-main-quest-rough-playthrough.md"
        for record in ("3FBBB2", "3FBC0D", "3FBC10", "3FFACF")
    },
}


def _entries(prefix: str) -> list[dict[str, str]]:
    entries: list[dict[str, str]] = []
    for line in TODO_PATH.read_text(encoding="utf-8").splitlines():
        if not line.startswith(prefix):
            continue
        fields = line.split("|")[1:]
        entries.append(dict(field.split("=", 1) for field in fields))
    return entries


def _member_count(value: str) -> int:
    return len(value.split(","))


def test_mq_registry_has_exact_groups_links_and_accounting():
    entries = [entry for entry in _entries("W05-TODO|") if entry["chain"] == "MQ"]
    actual = {(entry["record"], entry["category"]): entry["members"] for entry in entries}

    assert actual == EXPECTED_GROUPS
    assert len(entries) == len(EXPECTED_GROUPS) == 10
    assert all(entry["blocker"] for entry in entries)
    assert all(entry["contract"] == "contracts/w3g-w05-main-quest-rough-playthrough.md" for entry in entries)
    assert all(entry["evidence"] == "contracts/w3g-w05-main-quest-rough-playthrough-evidence.md" for entry in entries)
    assert all(entry["status"] in {"deferred", "record-dependent"} for entry in entries)

    early = [entry for entry in entries if entry["record"] in {"5698E4", "405E14", "40F5BE", "41A39D", "41C976"}]
    late = [entry for entry in entries if entry["record"] in {"3FBBB2", "3FBC0D", "3FBC10", "3FFACF"}]
    assert sum(_member_count(entry["members"]) for entry in early) == 106
    assert sum(_member_count(entry["members"]) for entry in late) == 70


def test_rewards_online_and_reputation_are_excluded_from_blockers():
    entries = _entries("W05-TODO|")
    assert all("REWARD" not in entry["category"] for entry in entries)
    assert all("EWS" not in entry["category"] for entry in entries)
    assert all("REPUTATION" not in entry["category"] for entry in entries)


def test_each_mq_patch_has_one_plain_marker_and_unchanged_callable_count():
    for record, patch_name in QF_PATCHES.items():
        source = (PATCH_ROOT / patch_name).read_text(encoding="utf-8")
        todo_lines = [line for line in source.splitlines() if "TODO" in line]
        members = [
            (kind, name)
            for kind, name, _start, _end in _iter_top_level_papyrus_members(source.splitlines())
            if kind in {"function", "event"}
        ]
        assert todo_lines == ["; TODO"]
        assert "TODO(" not in source
        assert len(members) == EXPECTED_CALLABLE_COUNTS[record]


def test_exact_nine_qf_status_rows_are_bounded_partial_and_pending_runtime():
    with STATUS_PATH.open(encoding="utf-8", newline="") as status_file:
        rows = {
            row["script_name"]: row
            for row in csv.DictReader(status_file)
            if row["script_name"] in STATUS_EVIDENCE
        }

    assert set(rows) == set(STATUS_EVIDENCE)
    for script_name, evidence in STATUS_EVIDENCE.items():
        row = rows[script_name]
        assert row["terminal_state"] == "patched"
        assert evidence in row["evidence"]
        assert "source-ready" in row["notes"]
        assert "user regeneration and runtime verification pending" in row["notes"]
