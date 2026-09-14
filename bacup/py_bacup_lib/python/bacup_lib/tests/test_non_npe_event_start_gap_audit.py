from __future__ import annotations

import json
from pathlib import Path

from bacup_lib.workflows.unified import _script_patch_source


REPO_ROOT = Path(__file__).resolve().parents[5]
FIXTURE = Path(__file__).resolve().parent / "fixtures" / "non_npe_event_start_gaps.json"
CONTRACT = (
    REPO_ROOT
    / "bacup"
    / "docs"
    / "stub_restoration"
    / "contracts"
    / "non-npe-event-start-gap-audit-2026-09-02.md"
)


def _routes() -> list[dict[str, object]]:
    payload = json.loads(FIXTURE.read_text(encoding="utf-8"))
    assert payload["schema_version"] == 1
    return payload["routes"]


def test_non_npe_event_start_gap_fixture_is_bounded_and_has_local_lifecycles() -> None:
    routes = _routes()

    assert {route["quest_editor_id"] for route in routes} == {
        "FFZ10_Light",
        "MTR07_Earth",
        "MTR05_Mother",
        "SFM04_Organic",
    }
    assert {route["resolution"] for route in routes} == {
        "adapted-pending-regen",
        "carrier-dependency",
        "patched-pending-regen",
    }
    for route in routes:
        assert _script_patch_source(str(route["local_lifecycle_patch"])) is not None
        assert route["selector_keyword"]
        assert route["inventory_issues"]


def test_mtr05_terminal_menu_route_is_no_longer_a_carrier_dependency() -> None:
    route = next(
        route for route in _routes() if route["quest_editor_id"] == "MTR05_Mother"
    )

    assert route["resolution"] == "adapted-pending-regen"
    assert route["sender_script"] == "DefaultSendStoryEventOnMenuItemRun"
    assert route["terminal_chains"] == [
        {
            "submenu_terminal": "0681DF@SeventySix.esm",
            "parent_terminal": "0681C0@SeventySix.esm",
            "placed_reference": "331C8C@SeventySix.esm",
        },
        {
            "submenu_terminal": "0683AB@SeventySix.esm",
            "parent_terminal": "068276@SeventySix.esm",
            "placed_reference": "4331AA@SeventySix.esm",
        },
    ]

    contract = CONTRACT.read_text(encoding="utf-8")
    for terminal_id in ("0681DF", "0683AB", "0681C0", "068276", "331C8C", "4331AA"):
        assert f"`{terminal_id}`" in contract
    assert "This is a classifier repair, not a new Papyrus sender" in " ".join(
        contract.split()
    )


def test_earth_mover_start_repair_is_ordered_before_inventory_consumption() -> None:
    patch = _script_patch_source("MTR07_EarthReactorTriggerScript")
    assert patch is not None

    busy = patch.index('GoToState("busy")')
    send = patch.index("MTR07_EarthQuestStartKeyword.SendStoryEventAndWait(")
    recheck = patch.index("If !MTR07_Earth.IsRunning()", send)
    remove = patch.index("akActionRef.RemoveItem(IgnitionReactorCore01, 1, True)")

    assert busy < send < recheck < remove
    assert "akRef1 = akActionRef, akRef2 = Self" in patch
    assert 'GoToState("ready")' in patch[recheck:remove]


def test_contract_records_each_route_and_forbids_unproven_force_start() -> None:
    contract = CONTRACT.read_text(encoding="utf-8")
    normalized_contract = " ".join(contract.split())

    for route in _routes():
        assert f'`{route["quest_editor_id"]}`' in contract
        assert str(route["source_quest"]).split("@", 1)[0] in contract
        assert str(route["selector_keyword"]).split("@", 1)[0] in contract
    assert (
        "No direct `Quest.Start()` or force-autostart substitute is approved."
        in normalized_contract
    )
