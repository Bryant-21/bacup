from __future__ import annotations

import csv
from pathlib import Path

from bacup_lib.workflows.unified import (
    _iter_top_level_papyrus_members,
    _script_patch_source,
)


REPO_ROOT = Path(__file__).resolve().parents[5]
DOC_ROOT = REPO_ROOT / "bacup" / "docs" / "stub_restoration"
TODO_PATH = DOC_ROOT / "TODO.md"
STATUS_PATH = DOC_ROOT / "status.csv"
CONTRACT_PATH = DOC_ROOT / "contracts" / "w3g-w05-main-quest-rough-playthrough.md"
EVIDENCE_PATH = (
    DOC_ROOT / "contracts" / "w3g-w05-main-quest-rough-playthrough-evidence.md"
)
TEST_PLAN_PATH = DOC_ROOT / "w05-main-quest-test-plan.md"

CORE_COUNTS = {
    "Fragments:Quests:QF_W05_MQ_000P_005698E4": 16,
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

RAIDER_QFS = {
    "Fragments:Quests:QF_W05_MQR_201P_0040D28D",
    "Fragments:Quests:QF_W05_MQR_202P_0041C9E6",
    "Fragments:Quests:QF_W05_MQR_203P_0042F31B",
    "Fragments:Quests:QF_W05_MQR_Choice_005930B2",
    "Fragments:Quests:QF_W05_MQR_204P_00535E55",
    "Fragments:Quests:QF_W05_MQR_205P_00548B7A",
    "Fragments:Quests:QF_W05_MQR_205P_A_005588EF",
}

ORIGINAL_MAIN_SCOPE = {
    *CORE_COUNTS,
    *FOUNDATION_COUNTS,
    "Fragments:Quests:QF_W05_MQ_001P_Wayward_Lacey_00405E15",
    "Fragments:Quests:QF_W05_MQ_001P_Wayward_Lacey_0053AF40",
    "Fragments:Quests:QF_W05_MQ_001P_Wayward_MiscP_00594DFD",
    "Fragments:Quests:QF_W05_MQ_003P_Muscle_Duncan_005537E0",
    "Fragments:Quests:QF_W05_MQ_003P_Radio_0041A325",
    "Fragments:Quests:QF_W05_MQ_101P_Radio_003FBBB3",
    "Fragments:Quests:QF_W05_MQ_102P_A_003FFC02",
    "Fragments:Quests:QF_W05_MQ_102P_B_003FFC00",
    "Fragments:Quests:QF_W05_MQA_206P_0054EDB9",
}

ZERO_MARKER_SCRIPTS = {
    "Fragments:Quests:QF_W05_MQ_101P_Radio_003FBBB3",
    "Fragments:Quests:QF_W05_MQ_102P_A_003FFC02",
    "Fragments:Quests:QF_W05_MQ_102P_B_003FFC00",
    "Fragments:Quests:QF_W05_MQR_205P_00548B7A",
}

LACEY_SCRIPT = "Fragments:Quests:QF_W05_MQ_001P_Wayward_Lacey_00405E15"
RECOVERED_MQR_202_PRODUCERS = {
    "W05_MQR_202P_RaRaItemPickedUpScript",
    "W05_MQR_202P_VentMarkerScript",
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


def test_contract_evidence_and_user_plan_exist_and_agree_on_scope():
    contract = CONTRACT_PATH.read_text(encoding="utf-8")
    evidence = EVIDENCE_PATH.read_text(encoding="utf-8")
    test_plan = TEST_PLAN_PATH.read_text(encoding="utf-8")

    for text in (contract, evidence, test_plan):
        assert "Community" in text
        assert "EWS" in text
        assert "reward" in text.lower()
        assert "reputation" in text.lower()
    assert "user owns regeneration" in contract.lower()
    assert "Use this plan after you regenerate" in test_plan
    assert "MQR_201P" in contract and "MQSettlers_201P" in contract


def test_original_24_main_scope_rows_report_bounded_partial_and_regen_state():
    with STATUS_PATH.open(encoding="utf-8", newline="") as status_file:
        rows = {
            row["script_name"]: row
            for row in csv.DictReader(status_file)
            if row["script_name"] in ORIGINAL_MAIN_SCOPE
        }

    assert len(ORIGINAL_MAIN_SCOPE) == 24
    assert set(rows) == ORIGINAL_MAIN_SCOPE
    for script_name, row in rows.items():
        if script_name == LACEY_SCRIPT:
            assert row["evidence"] == (
                "contracts/wayward-lacey-stale-scene-fragment.md"
            )
        else:
            assert "contracts/w3g-w05-main-quest-rough-playthrough.md" in row["evidence"]
            assert (
                "contracts/w3g-w05-main-quest-rough-playthrough-evidence.md"
                in row["evidence"]
            )
        assert "user regeneration and runtime verification pending" in row["notes"]
        assert "rough playthrough" not in row["notes"]
    assert rows[LACEY_SCRIPT]["terminal_state"] == "patched"
    assert "source scene targets are absent" in rows[LACEY_SCRIPT]["notes"]
    for script_name in ORIGINAL_MAIN_SCOPE:
        assert rows[script_name]["terminal_state"] == "patched"
        assert "generated output stale" in rows[script_name]["notes"]
        if script_name in CORE_COUNTS and not script_name.endswith("005698E4"):
            assert "source-ready exact live-bound member repair" in rows[script_name]["notes"]


def test_foundation_status_rows_report_source_ready_and_stale_output():
    with STATUS_PATH.open(encoding="utf-8", newline="") as status_file:
        rows = {
            row["script_name"]: row
            for row in csv.DictReader(status_file)
            if row["script_name"] in FOUNDATION_COUNTS
        }

    assert set(rows) == set(FOUNDATION_COUNTS)
    for row in rows.values():
        assert "source-ready exact live-bound member repair" in row["notes"]
        assert "generated output stale" in row["notes"]
        assert "rough playthrough" not in row["notes"]


def test_raider_route_status_rows_are_reconciled_to_the_two_branch_contract():
    with STATUS_PATH.open(encoding="utf-8", newline="") as status_file:
        rows = {
            row["script_name"]: row
            for row in csv.DictReader(status_file)
            if row["script_name"] in RAIDER_COUNTS
        }

    assert set(rows) == set(RAIDER_COUNTS)
    for script_name, row in rows.items():
        if script_name in RECOVERED_MQR_202_PRODUCERS:
            assert row["evidence"] == "contracts/w05-mqr-202-alias-producer-closure.md"
            assert "current generated PSC/PEX contain the repair" in row["notes"]
        else:
            assert "contracts/w3d-w05-mqr.md" in row["evidence"]
            assert "generated output stale" in row["notes"]
        assert "rough playthrough" not in row["notes"]
        assert "playthrough remains pending" in row["notes"] or (
            "user regeneration and runtime verification pending" in row["notes"]
        )
        if script_name in RAIDER_QFS:
            assert row["terminal_state"] == "patched"
            assert "source-ready exact live-bound member repair" in row["notes"]


def test_mqr204_producer_and_restored_callback_are_synchronized():
    contract = CONTRACT_PATH.read_text(encoding="utf-8")
    evidence = EVIDENCE_PATH.read_text(encoding="utf-8")
    test_plan = TEST_PLAN_PATH.read_text(encoding="utf-8")

    for text in (contract, evidence, test_plan):
        assert "DefaultAliasOnDeath" in text
        assert "FreeLou" in text
        assert "Untie/PERK" in text

    assert "preReqStage=160" in contract
    assert "StageToSet=200" in contract
    assert "UseOnDyingInstead=True" in contract
    assert "advance 200→300" in contract

    entries = [
        entry for entry in _entries("W05-TODO|") if entry.get("record") == "535E55"
    ]
    assert entries == []

    with STATUS_PATH.open(encoding="utf-8", newline="") as status_file:
        rows = {
            row["script_name"]: row
            for row in csv.DictReader(status_file)
            if row["script_name"]
            in {
                "Fragments:Quests:QF_W05_MQR_204P_00535E55",
                "W05_MQR_204P_FreeLouPerkScript",
            }
        }
    assert set(rows) == {
        "Fragments:Quests:QF_W05_MQR_204P_00535E55",
        "W05_MQR_204P_FreeLouPerkScript",
    }
    assert rows["W05_MQR_204P_FreeLouPerkScript"]["terminal_state"] == "patched"
    for row in rows.values():
        assert "DefaultAliasOnDeath" in row["notes"] or "FreeLou" in row["notes"]
        assert "stage 200 to 300" in row["notes"]
        assert "user regeneration and runtime verification pending" in row["notes"]


def test_core_and_foundation_callable_counts_match_live_bound_manifests():
    for script_name, expected_count in (
        CORE_COUNTS | FOUNDATION_COUNTS | RAIDER_COUNTS
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
            assert len(member_names) == expected_count


def test_raider_registry_has_exact_deferred_qf_accounting():
    entries = [entry for entry in _entries("W05-TODO|") if entry.get("chain") == "MQR"]
    actual_counts = {
        entry["record"]: len(entry["members"].split(",")) for entry in entries
    }
    assert actual_counts == {}
    assert sum(actual_counts.values()) == 0
    child = "Fragments:Quests:QF_W05_MQR_205P_A_005588EF"
    assert (
        sum(
            RAIDER_COUNTS[script_name]
            for script_name in RAIDER_QFS
            if script_name != child
        )
        == 320
    )
    assert RAIDER_COUNTS[child] == 7

    for path in (CONTRACT_PATH, EVIDENCE_PATH):
        text = path.read_text(encoding="utf-8")
        assert "320" in text
        assert "0 deferred" in text
        assert "7/7" in text

    semantic_scripts = {
        entry["script"]
        for prefix in ("W05-HELPER-TODO|", "W05-SEMANTIC-TODO|")
        for entry in _entries(prefix)
        if entry.get("chain") == "MQR"
    }
    assert semantic_scripts == set()


def test_restored_raider_helpers_have_exact_persistent_members():
    for script_name, expected_count in RESTORED_RAIDER_HELPERS.items():
        patch = _script_patch_source(script_name)
        assert patch is not None
        assert len(_member_names(patch)) == expected_count


def test_every_registry_member_is_absent_from_its_persistent_patch():
    for entry in _entries("W05-TODO|"):
        patch = _script_patch_source(entry["script"])
        assert patch is not None, entry["script"]
        names = set(_member_names(patch))
        for token in entry["members"].split(","):
            stage_item = token.split("@", 1)[0]
            stage, item = stage_item.split(":", 1)
            member = f"fragment_stage_{int(stage):04d}_item_{int(item):02d}"
            assert member not in names, (entry["script"], member)


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


def test_every_w05_plain_marker_has_one_canonical_registry_path():
    patch_root = (
        REPO_ROOT / "bacup" / "py_bacup_lib" / "python" / "bacup_lib" / "script_patches"
    )
    marked_paths = {
        path.relative_to(REPO_ROOT).as_posix().lower()
        for path in patch_root.rglob("*.psc")
        if "w05" in path.name.lower()
        and path.read_text(encoding="utf-8").splitlines().count("; TODO") == 1
    }
    registry_paths = {
        entry["patch"].replace("\\", "/").lower()
        for prefix in (
            "W05-TODO|",
            "W05-HELPER-TODO|",
            "W05-SEMANTIC-TODO|",
            "W05-AUDIT-TODO|",
        )
        for entry in _entries(prefix)
        if entry["patch"] != "none" and entry["script"] not in ZERO_MARKER_SCRIPTS
    }

    assert marked_paths == registry_paths


def test_excluded_surfaces_never_appear_as_todo_categories():
    categories = {
        entry["category"].lower()
        for prefix in (
            "W05-TODO|",
            "W05-HELPER-TODO|",
            "W05-SEMANTIC-TODO|",
        )
        for entry in _entries(prefix)
    }
    for excluded in (
        "community",
        "online",
        "ews",
        "event",
        "bounty",
        "reward",
        "reputation",
    ):
        assert all(excluded not in category for category in categories)

    exclusions = TODO_PATH.read_text(encoding="utf-8")
    assert "W05-EXCLUDED|chain=ALL" in exclusions
