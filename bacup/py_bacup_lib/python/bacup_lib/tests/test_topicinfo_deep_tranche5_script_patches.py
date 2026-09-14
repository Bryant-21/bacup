from __future__ import annotations

import csv
from pathlib import Path

import pytest

from bacup_lib.tests.test_terminal_fragment_script_patches import _fo4_base_source
from bacup_lib.workflows.unified import (
    _iter_top_level_papyrus_members,
    _merge_script_method_patches,
    _script_patch_source,
)
from creation_lib.pex.native_runtime import compile_psc


REPO_ROOT = Path(__file__).resolve().parents[5]
SOURCE_ROOT = REPO_ROOT / "mods" / "SeventySix" / "Scripts" / "Source" / "User"
LEDGER = (
    REPO_ROOT
    / "bacup"
    / "docs"
    / "stub_restoration"
    / "contracts"
    / "topicinfo-fragment-gap-ledger-2026-08-11.csv"
)

PATCH_CONTRACTS = {
    "Fragments:TopicInfos:TIF_W05_MQ_102P_00401072": (
        "Alias_Projector",
        6,
        "DefensiveTurretSlide",
    ),
    "Fragments:TopicInfos:TIF_W05_MQ_102P_0040107B": (
        "Alias_Projector",
        7,
        "LaserGridSlide",
    ),
    "Fragments:TopicInfos:TIF_W05_MQ_102P_004010A0": (
        "Alias_Projector",
        4,
        "Vault79Slide",
    ),
    "Fragments:TopicInfos:TIF_W05_MQ_102P_004010A6": (
        "Projector",
        3,
        "GoldSlide",
    ),
}
RENTER_SCRIPTS = (
    "Fragments:TopicInfos:TIF_W05_Community_BB_Quest_00548A66",
    "Fragments:TopicInfos:TIF_W05_Community_BB_Quest_0059F592",
)
ALLY_SCRIPTS = (
    "Fragments:TopicInfos:TIF_W05_RE_Scene_JP02_0055F81B",
    "Fragments:TopicInfos:TIF_W05_RE_Scene_JP02_0055F834",
)
TEAM_CONTRACTS = {
    "Fragments:TopicInfos:TIF_TW009_005215CE": (
        "UnionPlayers.RemoveRef(player)",
        "player.RemoveFromFaction(TW009_UnionSoldiers)",
        "ConfederatePlayers.AddRef(player)",
        "player.AddToFaction(TW009_ConfederateSoldiers)",
        "TW009ConfederateJoinMsg.Show()",
    ),
    "Fragments:TopicInfos:TIF_TW009_005215D0": (
        "ConfederatePlayers.RemoveRef(player)",
        "player.RemoveFromFaction(TW009_ConfederateSoldiers)",
        "UnionPlayers.AddRef(player)",
        "player.AddToFaction(TW009_UnionSoldiers)",
        "TW009UnionJoinMsg.Show()",
    ),
}
ALL_SCRIPTS = (
    *PATCH_CONTRACTS,
    *RENTER_SCRIPTS,
    *ALLY_SCRIPTS,
    *TEAM_CONTRACTS,
)


def _merged(script_name: str) -> str:
    base_name = script_name.rsplit(":", 1)[-1]
    source_path = (
        SOURCE_ROOT / "fragments" / "topicinfos" / f"{base_name}.psc"
    )
    skeleton = source_path.read_text(encoding="utf-8-sig")
    patch = _script_patch_source(script_name)
    assert patch is not None
    member_name = "fragment_end" if script_name in ALLY_SCRIPTS else "fragment_begin"
    assert [
        (kind, name)
        for kind, name, _start, _end in _iter_top_level_papyrus_members(
            patch.splitlines()
        )
    ] == [("function", member_name)]
    merged = _merge_script_method_patches(skeleton, patch)
    assert _merge_script_method_patches(merged, patch) == merged
    assert merged.casefold().count(f"function {member_name}(") == 1
    return merged


@pytest.mark.parametrize("script_name", ALL_SCRIPTS)
def test_topicinfo_deep_tranche5_merges_idempotently_and_compiles_full_source(
    script_name: str,
):
    base_source = _fo4_base_source()
    assert base_source is not None, "FO4 base Papyrus sources unavailable"
    merged = _merged(script_name)
    result = compile_psc(
        merged,
        imports=[str(SOURCE_ROOT), str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=f"Fragments/TopicInfos/{script_name.rsplit(':', 1)[-1]}.psc",
    )
    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None


@pytest.mark.parametrize("script_name,contract", PATCH_CONTRACTS.items())
def test_topicinfo_deep_tranche5_uses_exact_projector_alias_and_live_state_index(
    script_name: str, contract: tuple[str, int, str]
):
    alias_property, state_index, _state_name = contract
    patch = _script_patch_source(script_name)
    assert patch is not None
    assert (
        f"{alias_property}.GetReference() as DefaultMultiStateActivator" in patch
    )
    assert f"projectorRef.SetLocalState({state_index})" in patch
    assert "Game.GetForm" not in patch
    assert "PlayAnimation" not in patch


@pytest.mark.parametrize("script_name", RENTER_SCRIPTS)
def test_topicinfo_deep_tranche5_consumes_exact_room_price_before_alias_assignment(
    script_name: str,
):
    patch = _script_patch_source(script_name)
    assert patch is not None
    assert "player.GetItemCount(Caps001) >= 5" in patch
    removal = "player.RemoveItem(Caps001, 5, True)"
    assignment = "CurRenterAlias.ForceRefTo(player)"
    assert removal in patch
    assert assignment in patch
    assert patch.index(removal) < patch.index(assignment)
    assert "akSpeakerRef" not in patch[patch.index(removal) : patch.index(assignment)]


@pytest.mark.parametrize("script_name", ALLY_SCRIPTS)
def test_topicinfo_deep_tranche5_adds_local_player_to_settler_allies(
    script_name: str,
):
    patch = _script_patch_source(script_name)
    assert patch is not None
    assert "If SettlerAllies != None" in patch
    assert "SettlerAllies.AddRef(Game.GetPlayer())" in patch
    assert "RemoveRef" not in patch
    assert "StartCombat" not in patch


@pytest.mark.parametrize("script_name,operations", TEAM_CONTRACTS.items())
def test_topicinfo_deep_tranche5_switches_local_civil_war_side_before_feedback(
    script_name: str, operations: tuple[str, str, str, str, str]
):
    patch = _script_patch_source(script_name)
    assert patch is not None
    offsets = [patch.index(operation) for operation in operations]
    assert offsets == sorted(offsets)
    assert "UnionPlayers != None" in patch
    assert "ConfederatePlayers != None" in patch


def test_topicinfo_deep_tranche5_manifest_matches_exact_live_ledger_contracts():
    with LEDGER.open(encoding="utf-8", newline="") as stream:
        rows = {
            row["script"].casefold(): row
            for row in csv.DictReader(stream)
            if row["disposition"] == "patched-deterministic-tranche5"
        }

    assert rows.keys() == {
        script_name.rsplit(":", 1)[-1].casefold()
        for script_name in ALL_SCRIPTS
    }
    for script_name, (_alias, _index, state_name) in PATCH_CONTRACTS.items():
        row = rows[script_name.rsplit(":", 1)[-1].casefold()]
        assert row["exact_live_binding"] == "true"
        assert row["fragments"].casefold() == "fragment_begin"
        assert row["patch_members"].casefold() == "fragment_begin"
        assert state_name.casefold() in row["evidence"].casefold()
    for script_name in RENTER_SCRIPTS:
        row = rows[script_name.rsplit(":", 1)[-1].casefold()]
        assert row["exact_live_binding"] == "true"
        assert row["fragments"].casefold() == "fragment_begin"
        assert row["patch_members"].casefold() == "fragment_begin"
        assert "5" in row["evidence"] and "cap" in row["evidence"].casefold()
    for script_name in ALLY_SCRIPTS:
        row = rows[script_name.rsplit(":", 1)[-1].casefold()]
        assert row["exact_live_binding"] == "true"
        assert row["fragments"].casefold() == "fragment_end"
        assert row["patch_members"].casefold() == "fragment_end"
        assert "playersalliedtosettler" in row["evidence"].casefold()
    for script_name in TEAM_CONTRACTS:
        row = rows[script_name.rsplit(":", 1)[-1].casefold()]
        assert row["exact_live_binding"] == "true"
        assert row["fragments"].casefold() == "fragment_begin"
        assert row["patch_members"].casefold() == "fragment_begin"
        assert "removes" in row["evidence"].casefold()
        assert "before adding" in row["evidence"].casefold()


def test_topicinfo_deep_tranche5_exhaustive_remainder_count():
    with LEDGER.open(encoding="utf-8", newline="") as stream:
        deferred_rows = [
            row
            for row in csv.DictReader(stream)
            if row["disposition"] == "evidence-deferred"
        ]
    assert len(ALL_SCRIPTS) == 10
    assert len(deferred_rows) == 8
