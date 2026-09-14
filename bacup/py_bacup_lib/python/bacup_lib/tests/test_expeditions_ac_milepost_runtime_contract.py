from __future__ import annotations

from pathlib import Path
import re

REPO_ROOT = Path(__file__).resolve().parents[5]
CONTRACT = (
    REPO_ROOT
    / "bacup"
    / "docs"
    / "stub_restoration"
    / "contracts"
    / "expeditions-atlantic-city-milepost-runtime-2026-09-03.md"
)


def _contract_section(form_id: str) -> str:
    text = CONTRACT.read_text(encoding="utf-8")
    match = re.search(
        rf"(?ms)^## {re.escape(form_id)} .+?(?=^## [0-9A-F]{{8}} |\Z)",
        text,
    )
    assert match is not None, form_id
    return match.group(0)


def test_each_quest_has_section_scoped_disposition_and_freshness() -> None:
    expected = {
        "0076B107": "evidence_blocked_online_handoff",
        "006C1F50": "repaired_local_behavior_runtime_pending",
        "0078DA2A": "unsupported_online_raid_orchestration",
        "0078E2CC": "repaired_local_breadcrumb_start_evidence_blocked",
        "0062F5D6": "repaired_local_hub_progression_runtime_pending",
    }

    for form_id, disposition in expected.items():
        section = _contract_section(form_id)
        assert f"Disposition: `{disposition}`" in section
        assert "### Behavior graph" in section
        assert "### Freshness and runtime" in section
        assert "- Durable patch:" in section
        assert "- Generated PSC:" in section
        assert "- Deployed PEX:" in section
        assert "- ESM:" in section
        assert "- Runtime:" in section


def test_each_quest_explicitly_forbids_direct_start_and_synthetic_outcomes() -> None:
    required = "No direct `Quest.Start()`, synthetic completion, or synthetic reward is authorized."
    for form_id in ("0076B107", "006C1F50", "0078DA2A", "0078E2CC", "0062F5D6"):
        assert required in " ".join(_contract_section(form_id).split())


def test_freshness_and_runtime_language_does_not_overclaim_verification() -> None:
    expected = {
        "0076B107": "freshness unknown per the committed verification ledger",
        "006C1F50": "stale per the committed verification ledger",
        "0078DA2A": "freshness unknown per the committed verification ledger",
        "0078E2CC": "freshness unknown per the committed verification ledger",
        "0062F5D6": "pending authorized regeneration per the committed verification ledger",
    }
    for form_id, marker in expected.items():
        section = _contract_section(form_id)
        normalized = " ".join(section.split())
        assert marker in normalized
        assert "no fresh live-plugin verification" in normalized
        assert re.search(r"(?im)^- Runtime:.*\b(?:pending|blocked)\b", section)


def test_mile_caravan_intro_records_local_listeners_but_no_guessed_members() -> None:
    section = _contract_section("0076B107")
    for evidence in (
        "alias 2 within 2048 of player -> stage 3000",
        "alias 7 holotape enters player container -> stage 4050",
        "player listens to NOTE 7915FB -> stage 4100",
        "alias 11 cargo enters player container -> stage 4500",
        "player enters location alias 13 -> stage 5000",
        "76B13B event selects 761066 MILE_CaravanEscort",
        "approximately thirty source `QMDL` encounter modules",
        "No direct `Quest.Start()`, synthetic completion, or synthetic reward is",
    ):
        assert evidence in section



def test_ac_opportunity_contract_preserves_local_ambush_boundary() -> None:
    section = _contract_section("006C1F50")
    for evidence in (
        "aliases 28 and 13 die -> stages 670 and 680",
        "essential leader alias 11 bleeds out -> stage 690",
        "all three checkpoints -> stage 700",
        "Runtime: pending; authorized regeneration is required",
    ):
        assert evidence in section



def test_rd01_disposition_is_bound_to_exact_missing_module_topology() -> None:
    section = _contract_section("0078DA2A")
    for evidence in (
        "`78DA2B -> 772A47` (Enc01)",
        "`78F7A7 -> 78F7A1` (Enc02)",
        "`78DA2C -> 78B59E` (Enc04)",
        "`78DA2D -> 788127` (Enc05)",
        "`78DA2E -> 786D41` (Enc06)",
        "alias 1/index 0/stage 175",
        "alias 8/index 4/stage 650",
        "converted QuestModule topology is unresolved as noted above",
        "The main QUST has no `QuestReward` binding",
        "Runtime: blocked for a full raid",
    ):
        assert evidence in section



def test_meltdown_pointer_keeps_local_reward_separate_from_unknown_sender() -> None:
    section = _contract_section("0078E2CC")
    assert "unknown 78E2CF script-event sender" in section
    assert "Source node `78E2D7`" in section
    assert "does not prove that the intro quest\n  starts the pointer" in section
    assert "Start acquisition remains evidence-blocked" in section



def test_responders_handoff_ends_at_local_hub_completion() -> None:
    section = _contract_section("0062F5D6")
    for evidence in (
        "`63BED4 -> 400`, `63D5BD -> 500`, and `621FB7 -> 600`",
        "The QF contains an `AC_MQ02_Stage 6F0128` property",
        "no bound Responders\n  fragment proves a direct Atlantic City quest start",
        "Runtime: pending; authorized regeneration is required",
    ):
        assert evidence in section
