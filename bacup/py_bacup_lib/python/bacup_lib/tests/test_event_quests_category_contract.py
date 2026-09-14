from __future__ import annotations

import csv
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[5]
CONTRACT = (
    REPO_ROOT
    / "bacup"
    / "docs"
    / "stub_restoration"
    / "contracts"
    / "event-quests-category-audit-2026-09-02.md"
)
LEDGER = REPO_ROOT / "bacup" / "docs" / "stub_restoration" / "quest_verification_ledger.csv"

QUEST_EVIDENCE = {
    "EN05_Basic": {
        "form_id": "08C87F",
        "node": "182075",
        "selector": "182072",
        "route_evidence": "exact source node/selector association",
        "disposition": "repaired and compile-verified at source level",
    },
    "EN07_MQ_FleeBlast": {
        "form_id": "2D0F69",
        "node": "2D1161",
        "selector": "2D115D",
        "route_evidence": "EN07_MQ_Nuke_Master 2D0F67",
        "disposition": "repaired local warning/blast lifecycle",
    },
    "MTR07_Earth": {
        "form_id": "3443FB",
        "node": "345A2A",
        "selector": "345A29",
        "route_evidence": "MTR07_EarthConsole 345A28",
        "disposition": "repaired local lifecycle with a no-crafting core substitute",
    },
    "MILE_MeltdownPointerQuest": {
        "form_id": "78E2CC",
        "node": "78E2D7",
        "selector": "78E2CF",
        "route_evidence": "candidate event route",
        "disposition": "repaired and compile-verified after legitimate start",
    },
    "RD01_GleamingDepths": {
        "form_id": "78DA2A",
        "node": "79AED7",
        "selector": "79AED3",
        "route_evidence": "proves the source node/selector association",
        "disposition": "deterministic component helpers only",
    },
    "GQ_WorkshopClear": {
        "form_id": "1789F9",
        "node": "1789FA",
        "selector": "1789FE",
        "route_evidence": "WorkshopParent 02058E",
        "disposition": "converted route remains structurally stale",
    },
    "MTRZ05_Lucky": {
        "form_id": "019487",
        "node": "2A2691",
        "selector": "2A2690",
        "route_evidence": "three map books `3EB31E`, `42FC98`, and `42FC99`",
        "disposition": "repaired map-to-dig-site local lifecycle",
    },
    "FFZ10_Light": {
        "form_id": "187531",
        "node": "5217E1",
        "selector": "01564F",
        "route_evidence": "inventory association, not a proven direct carrier chain",
        "disposition": "repaired and compile-verified after legitimate start",
    },
}

FRESHNESS_ROWS = {
    "EN05_Basic": ("2026-08-11T23:29:46Z", "unknown"),
    "EN07_MQ_FleeBlast": ("2026-08-11T21:14:51Z", "stale"),
    "MTR07_Earth": ("2026-09-02T17:55:06Z", "stale"),
    "MILE_MeltdownPointerQuest": ("2026-09-02T19:16:32Z", "unknown"),
    "RD01_GleamingDepths": ("2026-08-13T12:37:24Z", "unknown"),
    "GQ_WorkshopClear": ("no category PSC patch", "stale"),
    "MTRZ05_Lucky": ("2026-09-02T17:17:01Z", "stale"),
    "FFZ10_Light": ("2026-09-01T22:08:34Z", "pending-regen"),
}


def _contract() -> str:
    return CONTRACT.read_text(encoding="utf-8")


def _quest_section(editor_id: str) -> str:
    contract = _contract()
    form_id = QUEST_EVIDENCE[editor_id]["form_id"]
    heading = f"## `{form_id}` `{editor_id}`"
    start = contract.index(heading)
    end = contract.find("\n## ", start + len(heading))
    return contract[start:] if end < 0 else contract[start:end]


def _freshness_row(editor_id: str) -> str:
    prefix = f"| `{editor_id}` |"
    return next(line for line in _contract().splitlines() if line.startswith(prefix))


def test_each_quest_section_contains_its_own_route_provenance_and_disposition() -> None:
    for editor_id, expected in QUEST_EVIDENCE.items():
        section = " ".join(_quest_section(editor_id).split())

        assert "**Route provenance:**" in section
        assert f"SMQN {expected['node']}" in section
        assert f"`{expected['selector']}`" in section
        assert str(expected["route_evidence"]) in section
        assert "**Disposition:**" in section
        assert str(expected["disposition"]) in section
        assert "committed runtime status remains `pending`" in section


def test_candidate_route_associations_are_demoted_inside_their_quest_sections() -> None:
    mile = " ".join(_quest_section("MILE_MeltdownPointerQuest").split())
    light = " ".join(_quest_section("FFZ10_Light").split())

    assert "has zero node conditions" in mile
    assert "not by a node-condition operand" in mile
    assert "candidate event route, not a proven Theodore-to-node chain" in mile
    assert "no Theodore INFO, scene, activator, or helper is proven" in mile

    assert "associated with the quest by quest event metadata" in light
    assert "not a local event sender" in light
    assert "inventory association, not a proven direct carrier chain" in light
    assert "no source references or placements" in light


def test_exact_route_claims_include_section_scoped_source_and_reverse_reference_evidence() -> None:
    en05 = " ".join(_quest_section("EN05_Basic").split())
    rd01 = " ".join(_quest_section("RD01_GleamingDepths").split())
    lucky = " ".join(_quest_section("MTRZ05_Lucky").split())

    assert "names `QUST 08C87F` as its child" in en05
    assert "selector-condition operand is `182072`" in en05
    assert "EN02 stage-400 fragment as a local producer" in en05

    assert "names `QUST 78DA2A` as its child" in rd01
    assert "selector-condition operand is `79AED3`" in rd01
    assert (
        "reverse references to the selector are the player record and `QUST 78DA2A`"
        in rd01
    )

    assert "names `QUST 019487` as its child" in lucky
    assert "selector-condition operand is `2A2690`" in lucky
    assert "durable `MTRZ05_MapScript` patch sends that same selector" in lucky


def test_each_quest_has_a_committed_ledger_row_separate_from_timestamp_evidence() -> None:
    contract = " ".join(_contract().split())
    assert (
        "The filesystem timestamp check and the committed ledger answer different questions"
        in contract
    )
    assert (
        "generated PSCs were dated `2026-09-03T05:43:47Z` through `05:43:50Z`"
        in contract
    )
    assert (
        "generated PEX files `2026-09-03T05:43:51Z` through `05:43:53Z`"
        in contract
    )
    assert "generated ESM `2026-09-03T06:03:16Z`" in contract
    assert "It does not supersede the committed ledger" in contract

    ledger_rows = {}
    with LEDGER.open(encoding="utf-8", newline="") as stream:
        for row in csv.DictReader(stream):
            editor_id = row.get("editor_id")
            if editor_id in QUEST_EVIDENCE:
                ledger_rows[editor_id] = row

    assert set(ledger_rows) == set(QUEST_EVIDENCE)
    for editor_id, (patch_timestamp, generated_state) in FRESHNESS_ROWS.items():
        row = _freshness_row(editor_id)
        assert patch_timestamp in row
        assert f"`{generated_state}`" in row
        assert "`pending`" in row
        ledger_row = ledger_rows[editor_id]
        assert int(ledger_row["quest_form_id"], 16) == int(
            QUEST_EVIDENCE[editor_id]["form_id"], 16
        )
        assert ledger_row["generated_freshness"] == generated_state
        assert ledger_row["in_game_verification"] == "pending"


def test_unsafe_direct_start_substitutes_are_forbidden_in_the_relevant_sections() -> None:
    sections = {
        editor_id: " ".join(_quest_section(editor_id).split())
        for editor_id in (
            "EN07_MQ_FleeBlast",
            "MILE_MeltdownPointerQuest",
            "GQ_WorkshopClear",
            "FFZ10_Light",
        )
    }

    assert "Direct `Start()` is forbidden" in sections["EN07_MQ_FleeBlast"]
    assert "must not be force-started globally" in sections[
        "MILE_MeltdownPointerQuest"
    ]
    assert "No Papyrus direct-start workaround is approved" in sections[
        "GQ_WorkshopClear"
    ]
    assert "Direct global autostart is not approved" in sections["FFZ10_Light"]
