from __future__ import annotations

import csv
import re
from collections import Counter
from datetime import date
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[5]
LEDGER = REPO_ROOT / "bacup" / "docs" / "stub_restoration" / "quest_verification_ledger.csv"
LEDGER_DOC = REPO_ROOT / "bacup" / "docs" / "stub_restoration" / "quest_verification_ledger.md"
CENSUS = REPO_ROOT / "bacup" / "docs" / "stub_restoration" / "fallout76_wiki_quest_census.csv"

REQUIRED_COLUMNS = {
    "quest_form_id",
    "editor_id",
    "quest_or_line",
    "wiki_category",
    "scope_scripts",
    "disposition",
    "evidence_level",
    "automated_verification_tests",
    "generated_freshness",
    "in_game_verification",
    "remaining_blocker",
    "evidence_contract_path",
    "last_audited_date",
}
DISPOSITIONS = {
    "audit-in-progress",
    "audited-no-change",
    "conversion-adapted",
    "deferred-npe",
    "excluded-crafting",
    "excluded-cut-content",
    "non-defect",
    "not-an-independent-quest",
    "partially-repaired",
    "record-dependency",
    "source-repaired",
    "unsupported-online",
}
EVIDENCE_LEVELS = {
    "assigned-only",
    "contract-and-tests",
    "contract-only",
    "status-and-contract",
    "tests-only",
}
FRESHNESS_STATES = {"fresh", "not-applicable", "pending-regen", "stale", "unknown"}
IN_GAME_STATES = {
    "not-applicable",
    "pending",
    "reported-broken",
    "reported-working",
    "unknown",
    "verified",
}
WIKI_CATEGORY_COUNTS = {
    "Expeditions: Atlantic City / America's Playground / Expeditions": 1,
    "Expeditions: Atlantic City / America's Playground / Main": 1,
    "Expeditions: Atlantic City / America's Playground / Side": 2,
    "Expeditions: The Pitt / Main": 1,
    "Ghoul Within / Main": 1,
    "Gleaming Depths / Main": 1,
    "Gone Fission / Daily": 1,
    "Gone Fission / Side": 1,
    "Main quests / Brotherhood of Steel": 3,
    "Main quests / Enclave": 1,
    "Main quests / Free States": 3,
    "Main quests / Raiders": 3,
    "Main quests / Responders": 6,
    "Main quests / Vault 76": 2,
    "Milepost Zero / Side": 2,
    "Not a listed wiki quest / Ally radiant framework": 4,
    "Not a listed wiki quest / Ambient dialogue": 2,
    "Not a listed wiki quest / Order of Mysteries support": 1,
    "Not a listed wiki quest / Public event": 1,
    "Not a listed wiki quest / Random encounter": 4,
    "One Wasteland For All / Daily Ops": 1,
    "One Wasteland For All / Side": 1,
    "Side quests / Faction quests / Order of Mysteries": 5,
    "Side quests / Miscellaneous quests": 13,
    "Side quests / Secondary quests": 18,
    "Steel Dawn / Main": 1,
    "Steel Dawn / Side": 1,
    "The Legendary Run / Miscellaneous quests": 1,
    "The Slasher / Main": 4,
    "The Slasher / Side": 2,
    "Wastelanders / Ally / Beckett": 19,
    "Wastelanders / Ally / Commander Daguerre": 36,
    "Wastelanders / Ally / Settler forager": 3,
    "Wastelanders / Cut": 5,
    "Wastelanders / Daily": 1,
    "Wastelanders / Main": 4,
    "Wastelanders / Main / Raiders": 1,
    "Wastelanders / Side quests / Miscellaneous quests": 1,
    "Wastelanders / Unmarked": 2,
    "Wild Appalachia / Cut": 5,
    "Wild Appalachia / Side": 14,
}
WIKI_CATEGORIES = set(WIKI_CATEGORY_COUNTS)
RECLASSIFIED_QUEST_CATEGORIES = {
    "00405E14": "Wastelanders / Main",
    "0040F5BE": "Wastelanders / Main",
    "0042F31B": "Wastelanders / Main / Raiders",
    "0054EDB9": "Wastelanders / Main",
    "003B32DC": "Main quests / Responders",
    "003B32DA": "Main quests / Responders",
    "003C4C23": "Main quests / Responders",
    "0022730F": "Main quests / Responders",
    "0003363B": "Main quests / Responders",
    "003A5FA0": "Main quests / Responders",
    "00031163": "Main quests / Raiders",
    "0009732E": "Main quests / Raiders",
    "00045A40": "Main quests / Raiders",
    "00002315": "Main quests / Free States",
    "004E0719": "Main quests / Free States",
    "00012566": "Main quests / Free States",
    "00052C77": "Main quests / Brotherhood of Steel",
    "0004E89C": "Main quests / Brotherhood of Steel",
    "000183CA": "Main quests / Brotherhood of Steel",
    "0008C87F": "Main quests / Enclave",
    "0076B107": "Milepost Zero / Side",
    "0078E2CC": "Milepost Zero / Side",
    "0078DA2A": "Gleaming Depths / Main",
    "0073BAFC": "Expeditions: Atlantic City / America's Playground / Side",
    "005C70CC": "Steel Dawn / Main",
    "0062F5D6": "Expeditions: The Pitt / Main",
    "006C1F50": "Expeditions: Atlantic City / America's Playground / Main",
    "00187531": "Not a listed wiki quest / Public event",
    "00562BA5": "Not a listed wiki quest / Random encounter",
    "0056385E": "Not a listed wiki quest / Random encounter",
    "00568E56": "Not a listed wiki quest / Random encounter",
    "0056F038": "Not a listed wiki quest / Random encounter",
    "0056E7E4": "Not a listed wiki quest / Ambient dialogue",
    "00402CA0": "Not a listed wiki quest / Ambient dialogue",
    "0054DCF9": "Wastelanders / Daily",
    "003F2DC7": "Wastelanders / Cut",
    "00403436": "Wastelanders / Cut",
    "0041B725": "Wastelanders / Cut",
    "003F2DC9": "Wastelanders / Cut",
    "005A05DE": "Wastelanders / Ally / Beckett",
    "005637BC": "Wastelanders / Cut",
    "0054F1A4": "Not a listed wiki quest / Ally radiant framework",
    "0056FB76": "Not a listed wiki quest / Ally radiant framework",
    "005727AD": "Not a listed wiki quest / Ally radiant framework",
    "0055FD53": "Not a listed wiki quest / Ally radiant framework",
    "00345D51": "Not a listed wiki quest / Order of Mysteries support",
    "0041C976": "Wastelanders / Main",
    "005698E4": "Wastelanders / Unmarked",
    "006FCF87": "Expeditions: Atlantic City / America's Playground / Side",
}
SECONDARY_QUESTS = {
    "0010AE02": "SFM04_Organic",
    "0007A2D8": "TW005",
    "001789F9": "GQ_WorkshopClear",
    "002C460C": "TW007_ColdCase",
    "002D0F69": "EN07_MQ_FleeBlast",
    "003443FB": "MTR07_Earth",
    "0010E201": "TW002",
    "0010D89F": "BoSZ01",
    "003B25E3": "GQ_HunterHuntedQuest",
    "00019487": "MTRZ05_Lucky",
    "002A93F5": "CB04_Mayor",
    "0033C3DE": "MTR02_Miner",
    "0033C4DE": "MTR02_Miner",
    "004E49D9": "MQ_Overseer",
    "0026AA34": "OverseerPersonal",
    "00006F3C": "GQ_DropGovtIntro",
    "0006A379": "MTR05_Mother",
    "00131033": "SFL02_Track",
}
ORDER_OF_MYSTERIES = {
    "00345D50": "MoM00",
    "00345D52": "MoM01",
    "003472F4": "MoM02",
    "00357E7E": "MoM03",
    "00357E7A": "MoM04",
}
CURRENT_WAVE = {
    "RSVP01_Quest": ("source-repaired", "stale"),
    "RSVP02_Quest": ("source-repaired", "stale"),
    "RS01B_Contact": ("source-repaired", "unknown"),
    "RS03_Inoculation": ("source-repaired", "stale"),
    "XPD_Hub_Responders": ("source-repaired", "pending-regen"),
    "FS01_MQ_Warn": ("source-repaired", "stale"),
    "FS02_MQ_Reassembly": ("source-repaired", "stale"),
    "FS03_MQ_Fruition": ("source-repaired", "stale"),
    "BoS01": ("source-repaired", "stale"),
    "BoS02": ("source-repaired", "stale"),
    "BoS03": ("source-repaired", "stale"),
    "BS01_NewBrothers": ("source-repaired", "stale"),
    "EN05_Basic": ("source-repaired", "unknown"),
    "EN07_MQ_FleeBlast": ("source-repaired", "stale"),
    "MTR07_Earth": ("source-repaired", "stale"),
    "MILE_CaravanIntro": ("record-dependency", "unknown"),
    "MILE_MeltdownPointerQuest": ("partially-repaired", "unknown"),
    "RD01_GleamingDepths": ("unsupported-online", "unknown"),
    "GQ_WorkshopClear": ("record-dependency", "stale"),
    "MTRZ05_Lucky": ("source-repaired", "stale"),
    "FFZ10_Light": ("partially-repaired", "pending-regen"),
    "TW005": ("source-repaired", "stale"),
    "COMP_RQ_Fetch": ("partially-repaired", "stale"),
    "COMP_RQ_Kill": ("partially-repaired", "stale"),
    "COMP_RQ_Rescue": ("partially-repaired", "stale"),
    "COMP_Visitor": ("source-repaired", "stale"),
    "MoM00": ("partially-repaired", "fresh"),
    "MoM01": ("partially-repaired", "fresh"),
    "MoM02": ("partially-repaired", "fresh"),
    "MoM03": ("partially-repaired", "fresh"),
    "MoM04": ("partially-repaired", "fresh"),
}
GAP_WAVE_CONTRACTS = {
    "bacup/docs/stub_restoration/contracts/legacy-empty-misc-gap-repair-2026-09-03.md": 15,
    "bacup/docs/stub_restoration/contracts/recent-empty-quests-gap-repair-2026-09-03.md": 9,
    "bacup/docs/stub_restoration/contracts/wastelanders-ally-empty-gap-repair-2026-09-03.md": 43,
    "bacup/docs/stub_restoration/contracts/wild-ac-empty-quests-gap-repair-2026-09-03.md": 18,
}
FINAL_GAP_WAVE_CONTRACTS = {
    "bacup/docs/stub_restoration/contracts/remaining-standalone-quest-gaps-2026-09-03.md": 6,
    "bacup/docs/stub_restoration/contracts/beckett-first-half-gap-repair-2026-09-03.md": 7,
    "bacup/docs/stub_restoration/contracts/beckett-second-half-finale-repair-2026-09-03.md": 8,
}


def _rows() -> list[dict[str, str]]:
    with LEDGER.open(encoding="utf-8-sig", newline="") as stream:
        return list(csv.DictReader(stream))


def _references(row: dict[str, str], column: str) -> list[Path]:
    return [REPO_ROOT / value for value in row[column].split(";") if value]


def test_quest_verification_ledger_schema_enums_and_uniqueness() -> None:
    rows = _rows()
    required_values = REQUIRED_COLUMNS - {
        "automated_verification_tests",
        "evidence_contract_path",
    }

    assert len(rows) == 179
    assert REQUIRED_COLUMNS <= rows[0].keys()
    assert all(all(row[column] for column in required_values) for row in rows)
    assert all(re.fullmatch(r"[0-9A-F]{8}", row["quest_form_id"]) for row in rows)
    assert len({row["quest_form_id"] for row in rows}) == len(rows)
    duplicate_editor_ids = {
        row["editor_id"].casefold()
        for row in rows
        if sum(other["editor_id"].casefold() == row["editor_id"].casefold() for other in rows) > 1
    }
    assert duplicate_editor_ids <= {"mtr02_miner"}
    assert {row["disposition"] for row in rows} <= DISPOSITIONS
    assert {row["evidence_level"] for row in rows} <= EVIDENCE_LEVELS
    assert {row["generated_freshness"] for row in rows} <= FRESHNESS_STATES
    assert {row["in_game_verification"] for row in rows} <= IN_GAME_STATES
    assert {row["wiki_category"] for row in rows} == WIKI_CATEGORIES
    assert Counter(row["wiki_category"] for row in rows) == WIKI_CATEGORY_COUNTS
    assert all(row["wiki_category"] != "Uncategorized" for row in rows)
    assert all(date.fromisoformat(row["last_audited_date"]) for row in rows)
    assert all(row["in_game_verification"] != "verified" for row in rows)


def test_quest_verification_ledger_references_exist_and_match_evidence_level() -> None:
    for row in _rows():
        tests = _references(row, "automated_verification_tests")
        evidence = _references(row, "evidence_contract_path")

        assert all(path.is_file() for path in tests), row["editor_id"]
        assert all(path.is_file() for path in evidence), row["editor_id"]
        assert all(path.suffix in {".py", ".rs"} for path in tests), row["editor_id"]
        if row["evidence_level"] == "assigned-only":
            assert not tests and not evidence
        elif row["evidence_level"] == "tests-only":
            assert tests
        elif row["evidence_level"] == "contract-only":
            assert evidence
        elif row["evidence_level"] == "contract-and-tests":
            assert tests and evidence
        elif row["evidence_level"] == "status-and-contract":
            assert evidence


def test_compile_or_contract_evidence_never_claims_in_game_verification() -> None:
    for row in _rows():
        if row["in_game_verification"] != "verified":
            continue
        assert row["generated_freshness"] == "fresh", row["editor_id"]
        assert row["evidence_level"] in {"contract-and-tests", "status-and-contract"}
        assert _references(row, "evidence_contract_path")
        assert _references(row, "automated_verification_tests")


def test_assigned_wiki_categories_preserve_supplied_formids() -> None:
    rows = _rows()
    secondary_rows = [
        row for row in rows if row["wiki_category"] == "Side quests / Secondary quests"
    ]
    mystery_rows = [
        row
        for row in rows
        if row["wiki_category"]
        == "Side quests / Faction quests / Order of Mysteries"
    ]
    secondary = {
        row["quest_form_id"]: row["editor_id"]
        for row in secondary_rows
    }
    mysteries = {
        row["quest_form_id"]: row["editor_id"]
        for row in mystery_rows
    }

    assert secondary == SECONDARY_QUESTS
    assert mysteries == ORDER_OF_MYSTERIES
    assert Counter(row["disposition"] for row in secondary_rows) == {
        "source-repaired": 13,
        "partially-repaired": 1,
        "conversion-adapted": 1,
        "unsupported-online": 1,
        "record-dependency": 1,
        "audit-in-progress": 1,
    }
    assert Counter(row["disposition"] for row in mystery_rows) == {
        "partially-repaired": 5,
    }
    assert {row["generated_freshness"] for row in mystery_rows} == {"fresh"}
    for row in rows:
        if row["wiki_category"] in {
            "Side quests / Secondary quests",
            "Side quests / Faction quests / Order of Mysteries",
        }:
            assert row["in_game_verification"] != "verified"


def test_reclassified_quests_have_stable_wiki_taxonomy_assignments() -> None:
    rows = _rows()
    reclassified = {
        row["quest_form_id"]: row["wiki_category"]
        for row in rows
        if row["quest_form_id"] not in SECONDARY_QUESTS
        and row["quest_form_id"] not in ORDER_OF_MYSTERIES
    }

    assert RECLASSIFIED_QUEST_CATEGORIES.items() <= reclassified.items()


def test_markdown_category_snapshot_matches_csv_taxonomy_counts() -> None:
    document = LEDGER_DOC.read_text(encoding="utf-8")

    assert f"The ledger contains {len(_rows())} rows" in document
    for category, count in WIKI_CATEGORY_COUNTS.items():
        assert f"| `{category}` | {count} |" in document


def test_markdown_disposition_snapshot_matches_csv_counts() -> None:
    document = LEDGER_DOC.read_text(encoding="utf-8")
    rows = _rows()
    counts = Counter(row["disposition"] for row in rows)

    for disposition, count in sorted(counts.items()):
        assert f"| `{disposition}` | {count} |" in document
    for freshness, count in Counter(
        row["generated_freshness"] for row in rows
    ).items():
        assert f"| `{freshness}` | {count} |" in document
    for runtime, count in Counter(row["in_game_verification"] for row in rows).items():
        assert f"| `{runtime}` | {count} |" in document


def test_reviewed_current_wave_has_exact_bounded_outcomes() -> None:
    rows = {row["editor_id"]: row for row in _rows()}

    assert CURRENT_WAVE.keys() <= rows.keys()
    for editor_id, (disposition, freshness) in CURRENT_WAVE.items():
        row = rows[editor_id]
        assert row["disposition"] == disposition
        assert row["generated_freshness"] == freshness
        assert row["in_game_verification"] == "pending"
        assert row["evidence_level"] == "contract-and-tests"

    assert "no-crafting" in rows["RSVP01_Quest"]["scope_scripts"]
    assert "no-crafting" in rows["RSVP02_Quest"]["scope_scripts"]
    assert "no-crafting" in rows["MTR07_Earth"]["scope_scripts"]
    assert "no-crafting" in rows["MILE_MeltdownPointerQuest"]["scope_scripts"]
    assert "Recipe-bearing" in rows["GQ_WorkshopClear"]["remaining_blocker"]


def test_beckett_outro_has_bounded_local_finale_repair() -> None:
    row = next(row for row in _rows() if row["quest_form_id"] == "005A05DE")

    assert row["editor_id"] == "COMP_Quest_Outro_Full_Beckett"
    assert row["disposition"] == "source-repaired"
    assert row["generated_freshness"] == "stale"
    assert row["in_game_verification"] == "pending"
    assert "camp alias" in row["remaining_blocker"]
    assert row["evidence_contract_path"].endswith(
        "beckett-second-half-finale-repair-2026-09-03.md"
    )


def test_true_empty_missing_wave_has_exact_bounded_dispositions() -> None:
    wave_rows = [
        row
        for row in _rows()
        if row["evidence_contract_path"] in GAP_WAVE_CONTRACTS
    ]

    assert Counter(row["evidence_contract_path"] for row in wave_rows) == (
        GAP_WAVE_CONTRACTS
    )
    assert Counter(row["disposition"] for row in wave_rows) == {
        "source-repaired": 22,
        "partially-repaired": 6,
        "unsupported-online": 3,
        "record-dependency": 6,
        "not-an-independent-quest": 41,
        "excluded-cut-content": 5,
        "excluded-crafting": 2,
    }
    assert all(row["in_game_verification"] != "verified" for row in wave_rows)
    assert all(
        row["generated_freshness"] == "stale"
        for row in wave_rows
        if row["disposition"] in {"source-repaired", "partially-repaired"}
    )

    deferred = [row for row in _rows() if row["disposition"] == "deferred-npe"]
    assert {row["quest_form_id"] for row in deferred} == {"000D4D34", "003C4C22"}
    assert {
        row["evidence_contract_path"] for row in deferred
    } == {
        "bacup/docs/stub_restoration/fallout76_wiki_quest_census-2026-09-02.md"
    }
    assert all(
        row["generated_freshness"] == "not-applicable"
        for row in wave_rows
        if row["disposition"]
        in {
            "deferred-npe",
            "excluded-crafting",
            "excluded-cut-content",
            "not-an-independent-quest",
            "record-dependency",
            "unsupported-online",
        }
    )


def test_census_separates_patch_coverage_from_remaining_candidate_gaps() -> None:
    with CENSUS.open(encoding="utf-8-sig", newline="") as stream:
        rows = list(csv.DictReader(stream))

    assert len(rows) == 333
    assert Counter(row["accounting-status"] for row in rows) == {
        "audit-in-progress": 1,
        "conversion-adapted": 1,
        "deferred-npe": 2,
        "excluded-crafting": 2,
        "excluded-cut-content": 5,
        "identity-blocked": 12,
        "non-defect": 6,
        "not-an-independent-quest": 53,
        "partially-repaired": 17,
        "patch-covered-not-ledgered": 147,
        "record-dependency": 13,
        "source-repaired": 67,
        "unsupported-online": 7,
    }
    assert not [
        row for row in rows if row["accounting-status"] == "candidate-gap"
    ]

    skyline = [row for row in rows if row["section"] == "Skyline Valley"]
    assert len(skyline) == 12
    assert Counter(row["accounting-status"] for row in skyline) == {
        "patch-covered-not-ledgered": 11,
        "identity-blocked": 1,
    }


def test_final_gap_wave_closes_all_20_candidates_and_tracks_dependency_controller() -> None:
    rows = [
        row
        for row in _rows()
        if row["evidence_contract_path"] in FINAL_GAP_WAVE_CONTRACTS
    ]

    assert Counter(row["evidence_contract_path"] for row in rows) == (
        FINAL_GAP_WAVE_CONTRACTS
    )
    assert Counter(row["disposition"] for row in rows) == {
        "source-repaired": 2,
        "partially-repaired": 2,
        "record-dependency": 2,
        "unsupported-online": 2,
        "non-defect": 1,
        "not-an-independent-quest": 12,
    }
    assert Counter(row["generated_freshness"] for row in rows) == {
        "fresh": 1,
        "stale": 3,
        "not-applicable": 17,
    }
    assert Counter(row["in_game_verification"] for row in rows) == {
        "pending": 4,
        "not-applicable": 17,
    }

    by_id = {row["quest_form_id"]: row for row in rows}
    assert by_id["005C390B"]["generated_freshness"] == "fresh"
    assert by_id["004845B0"]["generated_freshness"] == "stale"
    assert by_id["00574625"]["generated_freshness"] == "stale"
    assert by_id["005A05DE"]["generated_freshness"] == "stale"
    assert "materialize_fo76_local_encounter_waves.rs" in by_id["00574625"][
        "automated_verification_tests"
    ]
    assert "crafting" in by_id["0047A44D"]["remaining_blocker"]
