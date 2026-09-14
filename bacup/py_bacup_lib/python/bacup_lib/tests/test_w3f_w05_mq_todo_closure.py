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
    "5698E4": 16,
    "405E14": 40,
    "40F5BE": 67,
    "41A39D": 62,
    "41C976": 66,
    "3FBBB2": 48,
    "3FBC0D": 55,
    "3FBC10": 16,
    "3FFACF": 49,
}

EXPECTED_TODO_MARKERS = {record: int(record == "5698E4") for record in QF_PATCHES}

EXPECTED_GROUPS = {
    ("5698E4", "EB"): "0100:00,0150:00,0200:00,0250:00,0300:00,0350:00,0400:00,0450:00,0500:00,0550:00,0999:00",
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
    assert len(entries) == len(EXPECTED_GROUPS) == 1
    assert all(entry["blocker"] for entry in entries)
    assert all(entry["contract"] == "contracts/w3g-w05-main-quest-rough-playthrough.md" for entry in entries)
    assert all(
        entry["evidence"]
        == "contracts/w3g-w05-main-quest-rough-playthrough-evidence.md"
        for entry in entries
    )
    assert all((TODO_PATH.parent / entry["evidence"]).is_file() for entry in entries)
    assert all(entry["status"] in {"deferred", "record-dependent"} for entry in entries)
    assert sum(_member_count(entry["members"]) for entry in entries) == 11


def test_rewards_online_and_reputation_are_excluded_from_blockers():
    entries = _entries("W05-TODO|")
    assert all("REWARD" not in entry["category"] for entry in entries)
    assert all("EWS" not in entry["category"] for entry in entries)
    assert all("REPUTATION" not in entry["category"] for entry in entries)


def test_each_mq_patch_has_reconciled_marker_and_callable_count():
    for record, patch_name in QF_PATCHES.items():
        source = (PATCH_ROOT / patch_name).read_text(encoding="utf-8")
        todo_lines = [line for line in source.splitlines() if "TODO" in line]
        members = [
            (kind, name)
            for kind, name, _start, _end in _iter_top_level_papyrus_members(source.splitlines())
            if kind in {"function", "event"}
        ]
        assert todo_lines == ["; TODO"] * EXPECTED_TODO_MARKERS[record]
        assert "TODO(" not in source
        if record == "405E14":
            fragment_members = [
                member
                for member in members
                if member[1].startswith("fragment_stage_")
            ]
            assert len(fragment_members) == EXPECTED_CALLABLE_COUNTS[record] == 40
            assert members == [
                *fragment_members,
                ("function", "attemptradicalhandoff"),
                ("event", "ontimer"),
            ]
        else:
            assert len(members) == EXPECTED_CALLABLE_COUNTS[record]


def test_exact_nine_qf_status_rows_are_reconciled_and_pending_runtime():
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
        assert "user regeneration and runtime verification pending" in row["notes"]
        if script_name.endswith("QF_W05_MQ_000P_005698E4"):
            assert "bounded partial source repair" in row["notes"]
        else:
            assert "source-ready" in row["notes"]
