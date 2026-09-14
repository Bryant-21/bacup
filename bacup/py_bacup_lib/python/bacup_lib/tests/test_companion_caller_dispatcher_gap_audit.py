from __future__ import annotations

import re
from pathlib import Path

from bacup_lib.workflows.unified import _script_patch_source


REPO_ROOT = Path(__file__).resolve().parents[5]
CONTRACT = (
    REPO_ROOT
    / "bacup"
    / "docs"
    / "stub_restoration"
    / "contracts"
    / "companion-caller-dispatcher-gap-audit-2026-09-02.md"
)
TOPIC_PATCHES = (
    REPO_ROOT
    / "bacup"
    / "py_bacup_lib"
    / "python"
    / "bacup_lib"
    / "script_patches"
    / "Fragments"
    / "TopicInfos"
)
DIRECT_COMPANION_CALL = re.compile(
    r"(?<![A-Za-z0-9_])(?:StartDailyQuest|SetRQAcceptanceStage)\s*\("
)


def test_acceptance_endpoint_is_pinned_without_inventing_a_stage_producer() -> None:
    contract = CONTRACT.read_text(encoding="utf-8")

    assert (
        "Player accepts quest; set by CompanionScript.SetRQAcceptanceStage()"
        in contract
    )
    assert "`54F1A4 COMP_RQ_Fetch`" in contract
    assert "`Fragments:Quests:QF_COMP_RQ_Fetch_0054F1A4`" in contract
    assert (
        _script_patch_source("Fragments:Quests:QF_COMP_RQ_Fetch_0054F1A4")
        is None
    )


def test_restore_contract_pins_custom_restore_before_saved_stage_restore() -> None:
    contract = CONTRACT.read_text(encoding="utf-8")
    normalized = " ".join(contract.split())
    restore_call = "`RestoreQuestCustom(Int restoreQuestStage)`"
    stage_restore = "restore the quest to the current value of `COMP_QuestStage`"

    exact_order = normalized.index("Exact proven order:")
    restore_index = normalized.index(restore_call, exact_order)
    stage_index = normalized.index(stage_restore, restore_index)

    assert exact_order < restore_index < stage_index
    assert _script_patch_source("COMP_RQ_RestoreScript") is None


def test_no_topic_fragment_claims_an_unidentified_companion_caller() -> None:
    unexpected_callers = []
    for patch_path in TOPIC_PATCHES.rglob("*.psc"):
        source = patch_path.read_text(encoding="utf-8")
        if DIRECT_COMPANION_CALL.search(source):
            unexpected_callers.append(patch_path.relative_to(TOPIC_PATCHES).as_posix())

    assert unexpected_callers == []

    contract = CONTRACT.read_text(encoding="utf-8")
    assert "`StartDailyQuest` is not a recovered SOURCE ABI" in contract
    assert "no caller fragment patch" in contract
    assert "no `COMP_RQ_RestoreScript` patch" in contract

