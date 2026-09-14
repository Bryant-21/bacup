from __future__ import annotations

import csv
from pathlib import Path

from bacup_lib.tests.test_order_of_mysteries_questline_script_patches import (
    PATCH_MEMBERS,
)
from bacup_lib.workflows.unified import _script_patch_source


REPO_ROOT = Path(__file__).resolve().parents[5]
LEDGER = (
    REPO_ROOT
    / "bacup"
    / "docs"
    / "stub_restoration"
    / "quest_verification_ledger.csv"
)
ORDER_CONTRACT = (
    REPO_ROOT
    / "bacup"
    / "docs"
    / "stub_restoration"
    / "contracts"
    / "order-of-mysteries-questline.md"
)
REVIEW_CONTRACT = (
    REPO_ROOT
    / "bacup"
    / "docs"
    / "stub_restoration"
    / "contracts"
    / "order-secondary-independent-review-2026-09-02.md"
)

SECONDARY_IDENTITIES = {
    ("0010AE02", "SFM04_Organic"),
    ("0007A2D8", "TW005"),
    ("001789F9", "GQ_WorkshopClear"),
    ("002C460C", "TW007_ColdCase"),
    ("002D0F69", "EN07_MQ_FleeBlast"),
    ("003443FB", "MTR07_Earth"),
    ("0010E201", "TW002"),
    ("0010D89F", "BoSZ01"),
    ("003B25E3", "GQ_HunterHuntedQuest"),
    ("00019487", "MTRZ05_Lucky"),
    ("002A93F5", "CB04_Mayor"),
    ("0033C3DE", "MTR02_Miner"),
    ("0033C4DE", "MTR02_Miner"),
    ("004E49D9", "MQ_Overseer"),
    ("0026AA34", "OverseerPersonal"),
    ("00006F3C", "GQ_DropGovtIntro"),
    ("0006A379", "MTR05_Mother"),
    ("00131033", "SFL02_Track"),
}
ORDER_IDENTITIES = {
    ("00345D50", "MoM00"),
    ("00345D52", "MoM01"),
    ("003472F4", "MoM02"),
    ("00357E7E", "MoM03"),
    ("00357E7A", "MoM04"),
}


def _ledger_rows() -> list[dict[str, str]]:
    with LEDGER.open(encoding="utf-8-sig", newline="") as stream:
        return list(csv.DictReader(stream))


def test_reviewed_wiki_categories_have_exact_identities_and_no_runtime_claims():
    rows = _ledger_rows()
    secondary = [
        row
        for row in rows
        if row["wiki_category"] == "Side quests / Secondary quests"
    ]
    order = [
        row
        for row in rows
        if row["wiki_category"]
        == "Side quests / Faction quests / Order of Mysteries"
    ]

    assert {(row["quest_form_id"], row["editor_id"]) for row in secondary} == (
        SECONDARY_IDENTITIES
    )
    assert {(row["quest_form_id"], row["editor_id"]) for row in order} == (
        ORDER_IDENTITIES
    )
    assert all(row["in_game_verification"] != "verified" for row in secondary + order)


def test_order_handoffs_do_not_bypass_story_manager_with_direct_quest_start():
    patch_text = "\n".join(
        _script_patch_source(script_name) or "" for script_name in PATCH_MEMBERS
    )
    assert ".Start()" not in patch_text
    assert "SendStoryEventAndWait" in patch_text


def test_mom_holotape_registration_stops_after_shutdown_or_completed_progress():
    patch = _script_patch_source("MoMHolotapeScript")
    assert patch is not None
    played = patch.split(
        "Event ObjectReference.OnHolotapePlay", 1
    )[1].split("EndEvent", 1)[0]
    acquired = patch.split(
        "Event ObjectReference.OnItemAdded", 1
    )[1].split("EndEvent", 1)[0]

    assert "If !IsRunning() || IsCompleted()" in played
    assert played.count('UnregisterForRemoteEvent(akSender, "OnHolotapePlay")') == 2
    assert "ProcessHolotape(playedTape)" in played
    assert "If IsHolotapeProgressComplete(playedTape)" in played
    assert "If IsRunning() && !IsCompleted()" in acquired
    assert acquired.count(
        'RegisterForRemoteEvent(akItemReference, "OnHolotapePlay")'
    ) == 1
    assert "targetQuest.IsCompleted() || targetQuest.IsStageDone(" in patch


def test_review_contract_keeps_rewards_freshness_and_runtime_claims_bounded():
    order_contract = ORDER_CONTRACT.read_text(encoding="utf-8")
    review_contract = REVIEW_CONTRACT.read_text(encoding="utf-8")

    assert "generated MoM PSC/PEX pairs and `SeventySix.esm`" in order_contract
    assert "are stale" in order_contract
    assert "does not independently" in review_contract
    assert "exact GMRW" in review_contract
    assert "No reviewed quest is in-game verified" in review_contract
