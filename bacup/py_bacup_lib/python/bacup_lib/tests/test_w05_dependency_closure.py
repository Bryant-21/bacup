from __future__ import annotations

import csv
from pathlib import Path

from bacup_lib.workflows.unified import _script_patch_source


REPO_ROOT = Path(__file__).resolve().parents[5]
DOCS = REPO_ROOT / "bacup" / "docs" / "stub_restoration"
SOURCE_ROOT = REPO_ROOT / "mods" / "SeventySix" / "Scripts" / "Source" / "User"
TALES_ROOT = REPO_ROOT / "mods" / "B21_TalesFromAppalachia"


def _status(relative_path: str) -> dict[str, str]:
    with (DOCS / "status.csv").open(encoding="utf-8", newline="") as stream:
        rows = csv.DictReader(stream)
        return next(row for row in rows if row["relative_path"] == relative_path)


def test_w05_onconnect_has_pcon_producer_and_patched_disposition() -> None:
    relative_path = "fragments/Quests/QF_W05_MQ_101P_OnConnect_003FBBB4.psc"
    row = _status(relative_path)
    assert row["terminal_state"] == "patched"
    assert row["evidence"] == "contracts/w05-mq-101p-onconnect-pcon-closure.md"

    patch = _script_patch_source(
        "Fragments:Quests:QF_W05_MQ_101P_OnConnect_003FBBB4"
    )
    assert patch is not None
    assert "Alias_currentPlayer.GetReference()" in patch
    assert (
        "W05_MQ_101P_QuestStartKeyword.SendStoryEventAndWait(None, playerRef, playerRef)"
        in patch
    )
    assert patch.index("SendStoryEventAndWait") < patch.index("Stop()")
    assert ".Start()" not in patch

    coordinator = (
        TALES_ROOT
        / "Scripts"
        / "Source"
        / "User"
        / "B21"
        / "B21_TFA_PostReclamationBridge.psc"
    ).read_text(encoding="utf-8")
    assert "Bool Function DispatchPlayerConnectEvent(Actor akPlayer)" in coordinator
    assert 'Game.GetFormFromFile(0x10D8E3, "SeventySix.esm") as Keyword' in coordinator
    assert "eventKeyword.SendStoryEventAndWait(None, akPlayer, akPlayer)" in coordinator

    bridge_yaml = (
        TALES_ROOT
        / "yaml"
        / "records"
        / "QUST"
        / "B21_TFA_qust_PostReclamationBridge - FFF005_B21_TalesFromAppalachia.esp.yaml"
    ).read_text(encoding="utf-8")
    assert "propertyName: PlayerConnectKeyword" in bridge_yaml
    assert 'object_id: "10D8E3"' in bridge_yaml

    producer_yaml = (
        TALES_ROOT
        / "yaml"
        / "records"
        / "QUST"
        / "B21_TFA_qust_PlayerConnectBridge - FFF008_B21_TalesFromAppalachia.esp.yaml"
    ).read_text(encoding="utf-8")
    assert "- StartGameEnabled" in producer_yaml
    assert "- StartsEnabled" in producer_yaml
    assert 'object_id: "FFF005"' in producer_yaml


def test_give_item_alias_preserves_exclusivity_and_patched_disposition() -> None:
    row = _status("DefaultAliasOnActivateGiveItem.psc")
    assert row["terminal_state"] == "patched"
    assert row["evidence"] == (
        "contracts/default-alias-on-activate-give-item-closure.md"
    )

    generated = (
        SOURCE_ROOT / "DefaultAliasOnActivateGiveItem.psc"
    ).read_text(encoding="utf-8")
    assert "ActorValue Property RequiredActorValue Auto" in generated
    assert "Int Property RequiredActorValueValue Auto" in generated

    patch = _script_patch_source("DefaultAliasOnActivateGiveItem")
    assert patch is not None
    gate = "playerRef.GetValue(RequiredActorValue) != RequiredActorValueValue"
    assert gate in patch
    assert patch.index(gate) < patch.index("playerRef.AddItem(")
    assert patch.index("playerRef.AddItem(") < patch.index("TryToSetStage(")
    for bound_filter in (
        "PlayerActivateOnly",
        "ActivatedByReferences",
        "ActivatedByAliases",
        "ActivatedByFactions",
    ):
        assert bound_filter in patch

