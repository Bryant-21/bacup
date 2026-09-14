from __future__ import annotations

import csv
from collections import Counter
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[5]
DOCS = REPO_ROOT / "bacup" / "docs" / "stub_restoration"
PATCH_ROOT = (
    REPO_ROOT / "bacup" / "py_bacup_lib" / "python" / "bacup_lib" / "script_patches"
)
SOURCE_ROOT = REPO_ROOT / "mods" / "SeventySix" / "Scripts" / "Source" / "User"

# Floors from the 2026-09-09 snapshot: 715 patched / 536 non-defect /
# 11 record-dependency / 47 unsupported-online over 1309 rows.
#
# Only the row count and `patched` are monotonic: rows are never removed, and a
# patched row stays patched. `record-dependency` and `unsupported-online` shrink
# as blockers clear, so they aren't pinned. The suite defends that no row is lost,
# no repair is undone, every row is terminal, and each path and script name
# appears exactly once.
MIN_STATUS_ROWS = 1309
MIN_PATCHED_ROWS = 715
TERMINAL_STATES = {
    "patched",
    "non-defect",
    "record-dependency",
    "unsupported-online",
}
REVIEWED_SECOND_WAVE_PATCHED = {
    "comp_rq_masterscript.psc",
    "comp_rq_restore_fetch_script.psc",
    "comp_rq_script.psc",
    "comp_rq_specificaliasesscript.psc",
    "companionconversationscript.psc",
    "companionscript.psc",
    "companionvisitorscript.psc",
    "fragments/quests/qf_comp_visitor_0055fd53.psc",
    "fragments/quests/qf_tw005_0007a2d8.psc",
}
SUPERSEDED_REVIEWED_SECOND_WAVE = {"companionscript.psc"}
RETIRED_SPECULATIVE_PATCHES = {
    "comp_idletimerscript.psc",
    "w05_mqr_vault79keypadaliasscript.psc",
    "w05_vaut79entrancekeypadscript.psc",
}
REVIEWED_WAVE_STATUS = {
    "fragments/quests/qf_rsvp01_water_003b32dc.psc",
    "fragments/quests/qf_rsvp02_hunger_003b32da.psc",
    "fragments/quests/qf_rs03_inoculation_0022730f.psc",
    "rsvp00_onitemcraftedsetstage.psc",
    "rsvp01_playercollectwater.psc",
    "fragments/quests/qf_fs01_mq_warn_00002315.psc",
    "fs01_mq_warn_playeraliasscript.psc",
    "fragments/quests/qf_fs02_mq_reassembly_004e0719.psc",
    "fragments/quests/qf_fs03_mq_fruition_00012566.psc",
    "fragments/quests/qf_bos01_00052c77.psc",
    "fragments/quests/qf_bos02_0004e89c.psc",
    "fragments/quests/qf_bos03_000183ca.psc",
    "fragments/quests/qf_bs01_newbrothers_005c70cc.psc",
}
# Floor from the 2026-09-09 snapshot.
MIN_DURABLE_PATCHES = 2339
# `(generated, patched)` — the first is exact, the second a floor.
FRAGMENT_COUNTS = {
    "Quests": (1483, 687),
    "TopicInfos": (1140, 430),
    "Terminals": (397, 136),
    "Scenes": (357, 29),
    "Packages": (205, 65),
    "Perks": (20, 16),
}
RESOLVED_VIA_PARENT = {
    "defaultaliasinventorymanagementg.psc",
    "defaultaliasinventorymanagementh.psc",
    "defaultaliasinventorymanagementi.psc",
    "w05_inventoryscriptj.psc",
    "w05_inventory_scriptk.psc",
}
UNREGISTERED_PATCHES = {
    "CharGenPerkBoardActivatorScript.psc",
    "CharGenPipBoyPickupScript.psc",
    "FlipCardSignCounterScript.psc",
    "LC101ControlTerminalScript.psc",
    "PlayerTutorialScript.psc",
    "Raids/RD01/Enc02/QuestScript.psc",
    "Storm_LightningShakeCameraScript.psc",
    "V94_1_StranglerHeartScript.psc",
    "Vatul79VentilationSetStageScript.psc",
    "Vault79GearDoorExplosivesScript.psc",
    "Vault79GoldBotSetStageScript.psc",
    "Vault79KillGhoulsScript.psc",
    "Vault79MotherlodeTunnelScript.psc",
    "Vault79MotherlodeVaultWallScrip.psc",
    "Vault79ReactorDoorOpenScript.psc",
    "Vault79_SentryBotPodScript.psc",
    "Vault79SentryBotSetStageScript.psc",
    "VaultHandScannerScript.psc",
    "VaultIDCardReaderScript.psc",
    "WL038MotherlodeDrillScript.psc",
    "p76_dlc01/DLC01_AtomicShopAdvertisement.psc",
}
CLOSURE_CONTRACT = "contracts/evidence-blocked-closure-2026-09-01.md"
SUPERSEDED_CLOSURE_EVIDENCE = {
    "fragments/Quests/QF_W05_RE_SceneZW01_0056368E.psc": (
        "contracts/w05-re-scenezw01-car-fail.md"
    ),
    "w05_vaut79entrancekeypadscript.psc": (
        "contracts/wastelanders-keypad-package-source-closure-2026-09-03.md"
    ),
    # FO4 has no native keypad menu; the numeric-entry adapter (built on the
    # Vitale per-digit puzzle) moved this row to its own contract.
    "DefaultKeypadScript.psc": (
        "contracts/fo4-keypad-numeric-entry-adapter-2026-09-09.md"
    ),
}


def _csv(path: Path) -> list[dict[str, str]]:
    with path.open(encoding="utf-8", newline="") as stream:
        return list(csv.DictReader(stream))


def test_status_registry_is_unique_and_fully_terminalized() -> None:
    rows = _csv(DOCS / "status.csv")
    states = Counter(row["terminal_state"] for row in rows)

    assert len(rows) >= MIN_STATUS_ROWS
    assert len({row["relative_path"].lower() for row in rows}) == len(rows)
    assert len({row["script_name"].lower() for row in rows}) == len(rows)
    assert set(states) <= TERMINAL_STATES
    assert states["patched"] >= MIN_PATCHED_ROWS
    assert all(row["terminal_state"] != "evidence-blocked" for row in rows)


def test_reviewed_wave_concrete_script_repairs_are_registered() -> None:
    rows = {
        row["relative_path"].replace("\\", "/").lower(): row
        for row in _csv(DOCS / "status.csv")
    }

    assert REVIEWED_WAVE_STATUS <= rows.keys()
    for relative_path in REVIEWED_WAVE_STATUS:
        row = rows[relative_path]
        assert row["terminal_state"] == "patched"
        assert row["wave"] == "6"
        assert row["evidence"] in {
            "contracts/responders-questline-audit-2026-09-02.md",
            "contracts/free-states-original-brotherhood-main-questline-2026-09-02.md",
        }
        assert "compile verified" in row["notes"]
        assert "generated delivery is stale" in row["notes"]


def test_reviewed_second_wave_repairs_and_retirements_are_accounted_once() -> None:
    rows = {
        row["relative_path"].replace("\\", "/").lower(): row
        for row in _csv(DOCS / "status.csv")
    }

    assert REVIEWED_SECOND_WAVE_PATCHED <= rows.keys()
    for relative_path in REVIEWED_SECOND_WAVE_PATCHED:
        assert rows[relative_path]["terminal_state"] == "patched"
        if relative_path in SUPERSEDED_REVIEWED_SECOND_WAVE:
            assert rows[relative_path]["wave"] == "7"
            assert rows[relative_path]["evidence"] == (
                "contracts/wastelanders-ally-empty-gap-repair-2026-09-03.md"
            )
        else:
            assert rows[relative_path]["wave"] == "6"

    for relative_path in RETIRED_SPECULATIVE_PATCHES:
        assert rows[relative_path]["terminal_state"] == "record-dependency"
        assert not (PATCH_ROOT / relative_path).is_file()


def test_evidence_blocked_closure_is_exhaustive_and_traceable() -> None:
    rows = _csv(DOCS / "status.csv")
    closure_rows = [
        row
        for row in rows
        if row["evidence"] == CLOSURE_CONTRACT
        or row["relative_path"] in SUPERSEDED_CLOSURE_EVIDENCE
    ]

    # The cohort is fixed at 48 rows; only their dispositions move, and only
    # toward `patched` as each blocker is actually cleared.
    assert Counter(row["terminal_state"] for row in closure_rows) == {
        "patched": 24,
        "non-defect": 8,
        "record-dependency": 1,
        "unsupported-online": 15,
    }
    assert len(closure_rows) == 48

    for row in closure_rows:
        superseding = SUPERSEDED_CLOSURE_EVIDENCE.get(row["relative_path"])
        if superseding is not None:
            assert row["evidence"] == superseding
            continue
        assert "Prior evidence:" in row["notes"]

    contract = (DOCS / CLOSURE_CONTRACT).read_text(encoding="utf-8").lower()
    assert all(row["relative_path"].lower() in contract for row in closure_rows)


def test_exhaustive_accepted_rows_override_older_status_dispositions() -> None:
    status = {row["relative_path"].lower(): row for row in _csv(DOCS / "status.csv")}
    quest_rows = _csv(
        DOCS / "contracts" / "quest-fragment-production-tranche-2026-08-11.csv"
    )
    topic_rows = _csv(
        DOCS / "contracts" / "topicinfo-fragment-gap-ledger-2026-08-11.csv"
    )

    for row in quest_rows:
        if not row["disposition"].startswith(("patched-", "partial-")):
            continue
        path = f"fragments/quests/{row['generated_file']}".lower()
        if path in status:
            assert status[path]["terminal_state"] == "patched"

    for row in topic_rows:
        if not row["disposition"].startswith("patched-"):
            continue
        path = f"fragments/topicinfos/{row['script']}.psc".lower()
        if path in status:
            assert status[path]["terminal_state"] == "patched"


def test_parent_resolved_children_and_default_challenge_have_semantic_dispositions() -> None:
    status = {row["relative_path"].lower(): row for row in _csv(DOCS / "status.csv")}
    for path in RESOLVED_VIA_PARENT:
        row = status[path]
        assert row["terminal_state"] == "non-defect"
        assert row["evidence"] == "contracts/w3c-infra-daim.md"
        assert "resolved via inherited repaired parent behavior" in row["notes"]

    challenge = status["defaultchallengemessageonactivatealias.psc"]
    assert challenge["terminal_state"] == "patched"
    assert challenge["evidence"] == (
        "contracts/root-nonfragment-gap-closure-2026-08-11.md"
    )


def test_current_patch_and_generated_fragment_inventory_matches_audit() -> None:
    """Generated counts are fixed; authored counts are floors.

    The generated corpus changes only when the conversion changes, so it stays
    an equality — a move there is a real signal. The durable patch set grows
    every time a repair lands, so only a *drop* means something was lost.
    """
    assert len(list(PATCH_ROOT.rglob("*.psc"))) >= MIN_DURABLE_PATCHES
    assert len(list(SOURCE_ROOT.rglob("*.psc"))) == 6636
    for family, (generated, patched) in FRAGMENT_COUNTS.items():
        assert len(list((SOURCE_ROOT / "Fragments" / family).rglob("*.psc"))) == generated
        assert len(list((PATCH_ROOT / "Fragments" / family).rglob("*.psc"))) >= patched


def test_newer_patch_registration_gap_is_explicit_and_bounded() -> None:
    assert len(UNREGISTERED_PATCHES) == 21
    status_paths = {
        row["relative_path"].replace("\\", "/").lower()
        for row in _csv(DOCS / "status.csv")
    }
    for relative_path in UNREGISTERED_PATCHES:
        assert (PATCH_ROOT / relative_path).is_file()
        assert relative_path.lower() not in status_paths


def test_true_empty_missing_wave_status_rows_are_registered_and_stale() -> None:
    rows = [
        row
        for row in _csv(DOCS / "status.csv")
        if row["shard"] == "w7-true-empty-missing-quest-closure"
    ]

    assert len(rows) == 47
    assert {row["terminal_state"] for row in rows} == {"patched"}
    assert {row["wave"] for row in rows} == {"7"}
    assert {
        row["evidence"] for row in rows
    } == {
        "contracts/legacy-empty-misc-gap-repair-2026-09-03.md",
        "contracts/recent-empty-quests-gap-repair-2026-09-03.md",
        "contracts/wastelanders-ally-empty-gap-repair-2026-09-03.md",
        "contracts/wild-ac-empty-quests-gap-repair-2026-09-03.md",
    }
    assert all((PATCH_ROOT / row["relative_path"]).is_file() for row in rows)
    assert all(
        "compile" in row["notes"] and "stale" in row["notes"] for row in rows
    )


def test_final_gap_wave_status_distinguishes_fresh_and_stale_delivery() -> None:
    rows = {
        row["relative_path"].replace("\\", "/"): row
        for row in _csv(DOCS / "status.csv")
        if row["shard"] == "w8-final-quest-gap-closure"
    }

    assert rows.keys() == {
        "fragments/topicinfos/TIF_W05_DialogueDavenport_0056F021.psc",
        "Fragments/Quests/QF_SHELS01_OpenHouse_005C390B.psc",
        "Fragments/Quests/QF_COMP_Quest_Full_Beckett_I_00574625.psc",
        "Fragments/Quests/QF_COMP_Quest_Outro_Full_Bec_005A05DE.psc",
    }
    assert all(row["terminal_state"] == "patched" for row in rows.values())
    assert "current generated PSC/PEX" in rows[
        "Fragments/Quests/QF_SHELS01_OpenHouse_005C390B.psc"
    ]["notes"]
    for path in (
        "fragments/topicinfos/TIF_W05_DialogueDavenport_0056F021.psc",
        "Fragments/Quests/QF_COMP_Quest_Full_Beckett_I_00574625.psc",
        "Fragments/Quests/QF_COMP_Quest_Outro_Full_Bec_005A05DE.psc",
    ):
        assert "stale" in rows[path]["notes"]
