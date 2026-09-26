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
        "0076B107": ("evidence_blocked_online_handoff", "freshness unknown"),
        "006C1F50": ("repaired_local_behavior_runtime_pending", "stale"),
        "0078DA2A": ("unsupported_online_raid_orchestration", "freshness unknown"),
        "0078E2CC": ("repaired_local_breadcrumb_start_evidence_blocked", "freshness unknown"),
        "0062F5D6": ("repaired_local_hub_progression_runtime_pending", "pending authorized regeneration"),
    }
    forbidden = "No direct `Quest.Start()`, synthetic completion, or synthetic reward is authorized."

    for form_id, (disposition, freshness) in expected.items():
        section = _contract_section(form_id)
        normalized = " ".join(section.split())
        assert f"Disposition: `{disposition}`" in section
        for heading in (
            "### Behavior graph",
            "### Freshness and runtime",
            "- Durable patch:",
            "- Generated PSC:",
            "- Deployed PEX:",
            "- ESM:",
            "- Runtime:",
        ):
            assert heading in section
        assert forbidden in normalized
        assert f"{freshness} per the committed verification ledger" in normalized
        assert "no fresh live-plugin verification" in normalized
        assert re.search(r"(?im)^- Runtime:.*\b(?:pending|blocked)\b", section)
