from __future__ import annotations

import csv
import os
from pathlib import Path

import pytest

from bacup_lib.workflows.unified import (
    _iter_top_level_papyrus_members,
    _merge_script_method_patches,
    _script_patch_source,
)
from creation_lib.pex.native_runtime import compile_psc


REPO_ROOT = Path(__file__).resolve().parents[5]
GENERATED_TOPICINFO_ROOT = (
    REPO_ROOT
    / "mods"
    / "SeventySix"
    / "Scripts"
    / "Source"
    / "User"
    / "fragments"
    / "topicinfos"
)
LIVE_BINDING_LEDGER = (
    REPO_ROOT
    / "bacup"
    / "docs"
    / "stub_restoration"
    / "contracts"
    / "topicinfo-fragment-gap-ledger-2026-08-11.csv"
)

PATCH_MEMBERS = {
    "TIF_BS02_MQ01_Penance_005F5DDF": "fragment_end",
    "TIF_COMP_Quest_Camp_Full_Bec_00590942": "fragment_end",
    "TIF_NWOT_Del_Dialogue_00674BAE": "fragment_begin",
    "TIF_NWOT_Del_Dialogue_00674BB1": "fragment_begin",
    "TIF_NWOT_Del_Dialogue_00674BB2": "fragment_begin",
    "TIF_NWOT_Pat_Dialogue_00678C0E": "fragment_end",
    "TIF_NWOT_Pat_Dialogue_00678C0F": "fragment_end",
    "TIF_Storm_Gumley_Dialogue_0075DD85": "fragment_end",
    "TIF_Storm_Gumley_Dialogue_0075DD94": "fragment_end",
    "TIF_W05_RE_Camp_JP09_Cryptid_00571B60": "fragment_end",
}


def _fo4_base_source() -> Path | None:
    candidates: list[Path] = []
    configured = os.environ.get("FO4_DIR", "").strip().strip('"')
    if configured:
        candidates.append(Path(configured))

    env_path = REPO_ROOT / ".env"
    if env_path.is_file():
        for line in env_path.read_text(encoding="utf-8").splitlines():
            if line.startswith("FO4_DIR="):
                value = line.split("=", 1)[1].strip().strip('"')
                if value:
                    candidates.append(Path(value))
                break

    for game_root in candidates:
        source_root = game_root / "Data" / "Scripts" / "Source" / "Base"
        if source_root.is_dir():
            return source_root
    return None


@pytest.mark.parametrize("base_name,member_name", PATCH_MEMBERS.items())
def test_topicinfo_deep_tranche3_exact_member_merges_and_compiles(
    base_name: str, member_name: str
):
    source_path = next(
        path
        for path in GENERATED_TOPICINFO_ROOT.glob("*.psc")
        if path.stem.casefold() == base_name.casefold()
    )
    source = source_path.read_text(encoding="utf-8-sig")
    patch = _script_patch_source(f"Fragments:TopicInfos:{base_name}")

    assert patch is not None
    assert [
        (kind, name)
        for kind, name, _start, _end in _iter_top_level_papyrus_members(
            patch.splitlines()
        )
    ] == [("function", member_name)]

    merged = _merge_script_method_patches(source, patch)
    assert merged.casefold().count(f"function {member_name}(") == 1

    base_source = _fo4_base_source()
    if base_source is None:
        pytest.skip("FO4 base Papyrus sources unavailable")

    result = compile_psc(
        merged,
        imports=[str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=f"Fragments/TopicInfos/{base_name}.psc",
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None


def test_topicinfo_deep_tranche3_manifest_matches_live_vmad_ledger():
    with LIVE_BINDING_LEDGER.open(encoding="utf-8", newline="") as stream:
        tranche_rows = {
            row["script"]: row
            for row in csv.DictReader(stream)
            if row["disposition"] == "patched-deterministic-tranche3"
        }

    assert tranche_rows.keys() == PATCH_MEMBERS.keys()
    for script_name, member_name in PATCH_MEMBERS.items():
        row = tranche_rows[script_name]
        assert row["exact_live_binding"] == "true"
        assert row["fragments"].casefold() == member_name
        assert row["patch_members"].casefold() == member_name


def test_topicinfo_deep_tranche3_exact_av_constants_and_ownership():
    values = {
        "TIF_NWOT_Pat_Dialogue_00678C0E": ("pNWOT_Pat_AfterBoss_AV", "1.0"),
        "TIF_NWOT_Pat_Dialogue_00678C0F": ("pNWOT_Pat_AfterBoss_AV", "1.0"),
        "TIF_Storm_Gumley_Dialogue_0075DD85": ("GumleyAV", "1.0"),
        "TIF_Storm_Gumley_Dialogue_0075DD94": ("GumleyAV", "2.0"),
    }
    for script_name, (actor_value, value) in values.items():
        patch = _script_patch_source(f"Fragments:TopicInfos:{script_name}")
        assert patch is not None
        assert f"Game.GetPlayer().SetValue({actor_value}, {value})" in patch
        assert f"akSpeakerRef.SetValue({actor_value}" not in patch


@pytest.mark.parametrize(
    "script_name,item_name,price",
    (
        ("TIF_NWOT_Del_Dialogue_00674BAE", "Potion_Bufftats", 80),
        ("TIF_NWOT_Del_Dialogue_00674BB1", "Potion_XCell", 100),
        ("TIF_NWOT_Del_Dialogue_00674BB2", "Potion_Fury", 50),
    ),
)
def test_topicinfo_deep_tranche3_chem_transaction_order(
    script_name: str, item_name: str, price: int
):
    patch = _script_patch_source(f"Fragments:TopicInfos:{script_name}")
    assert patch is not None
    guard = f"player.GetItemCount(Currency_Caps) >= {price}"
    payment = f"player.RemoveItem(Currency_Caps, {price}, True)"
    grant = f"player.AddItem({item_name}, 1, False)"
    state = "player.SetValue(ChemPurchases, 1.0)"
    assert guard in patch
    assert patch.index(guard) < patch.index(payment) < patch.index(grant) < patch.index(state)
    assert f"player.RemoveItem(Currency_Caps, {price}, True, akSpeakerRef)" not in patch


def test_topicinfo_deep_tranche3_scene_actions():
    penance = _script_patch_source(
        "Fragments:TopicInfos:TIF_BS02_MQ01_Penance_005F5DDF"
    )
    assert penance is not None
    assert "hewsen.MoveTo(hewsenMarker)" in penance
    assert "norland.MoveTo(norlandMarker)" in penance

    beckett = _script_patch_source(
        "Fragments:TopicInfos:TIF_COMP_Quest_Camp_Full_Bec_00590942"
    )
    assert beckett is not None
    assert "player.SetValue(AV_Beer, 0.0)" in beckett
    assert "player.SetValue(AV_Liquor, 0.0)" in beckett

    cryptid = _script_patch_source(
        "Fragments:TopicInfos:TIF_W05_RE_Camp_JP09_Cryptid_00571B60"
    )
    assert cryptid is not None
    assert "SGRefColProp.EvaluateAll()" in cryptid
    assert "StartCombatAll" not in cryptid


def test_topicinfo_deep_tranche3_patch_count():
    assert len(PATCH_MEMBERS) == 10
