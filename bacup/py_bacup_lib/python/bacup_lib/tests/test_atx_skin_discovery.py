"""Tests for discover_atx_skins — category-scoped FO76 ATX paint discovery.

Covers both WEAP (``materials/atx/weapons/``) and ARMO
(``materials/atx/armor/``) categories. The ATX BGSMs are written as raw
bytes; ``_read_bgsm_texture_refs`` fails gracefully on unparseable content,
so the texture side is not asserted here — only slug collection, asset
discovery, category-correct output paths, and MSWP synthesis.
"""
from __future__ import annotations

from pathlib import Path

from bacup_lib.atx_walker import discover_atx_skins
from bacup_lib.models import AssetRef, DependencyGraph, RecordNode


def _graph(editor_id: str, record_type: str, material_src: str) -> DependencyGraph:
    root = RecordNode(form_key="54A165:SeventySix.esm", editor_id=editor_id, record_type=record_type)
    base = AssetRef(asset_type="material", source_path=material_src)
    return DependencyGraph(root=root, all_records=[root], all_assets=[base])


def _touch(path: Path) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(b"\x00" * 16)


def test_discovers_armor_atx_skins(tmp_path: Path):
    atx = tmp_path / "materials" / "atx" / "armor" / "combatarmor"
    _touch(atx / "atx_combatarmor_enclave.bgsm")
    _touch(atx / "atx_combatarmor_blackrider.bgsm")

    graph = _graph("CombatArmor", "ARMO", "materials/armor/combatarmor/combatarmor.bgsm")
    result = discover_atx_skins(graph, extracted_dir=str(tmp_path), mod_name="TestMod")

    mats = {a.source_path.replace("\\", "/").lower() for a in result.new_assets if a.asset_type == "material"}
    assert "materials/atx/armor/combatarmor/atx_combatarmor_enclave.bgsm" in mats
    assert "materials/atx/armor/combatarmor/atx_combatarmor_blackrider.bgsm" in mats
    assert {r.record_type for r in result.new_records} == {"MSWP"}
    assert len(result.new_records) == 2  # one per variant
    assert result.slugs_walked == ["armor/combatarmor"]


def test_armor_substitution_paths_are_category_correct(tmp_path: Path):
    atx = tmp_path / "materials" / "atx" / "armor" / "combatarmor"
    _touch(atx / "atx_combatarmor_enclave.bgsm")

    graph = _graph("CombatArmor", "ARMO", "materials/armor/combatarmor/combatarmor.bgsm")
    result = discover_atx_skins(graph, extracted_dir=str(tmp_path), mod_name="TestMod")

    atx_asset = next(a for a in result.new_assets if a.source_path.lower().endswith("atx_combatarmor_enclave.bgsm"))
    # The ATX BGSM lives under the armor category, not weapons.
    assert "atx/armor/" in atx_asset.source_path.replace("\\", "/").lower()
    assert "atx/weapons/" not in atx_asset.source_path.replace("\\", "/").lower()


def test_weapon_atx_still_discovered(tmp_path: Path):
    # Regression guard: the original weapon path must keep working.
    atx = tmp_path / "materials" / "atx" / "weapons" / "gausspistol"
    _touch(atx / "atx_gausspistol_matteblack.bgsm")

    graph = _graph("GaussPistol", "WEAP", "materials/weapons/gausspistol/gausspistol.bgsm")
    result = discover_atx_skins(graph, extracted_dir=str(tmp_path), mod_name="TestMod")

    mats = {a.source_path.replace("\\", "/").lower() for a in result.new_assets if a.asset_type == "material"}
    assert "materials/atx/weapons/gausspistol/atx_gausspistol_matteblack.bgsm" in mats
    assert result.slugs_walked == ["weapons/gausspistol"]


def test_scopes_out_sibling_armor_family(tmp_path: Path):
    # Both armor families' base materials are in the graph, but only the
    # root's family (combatarmor) should be walked.
    _touch(tmp_path / "materials" / "atx" / "armor" / "combatarmor" / "atx_combatarmor_enclave.bgsm")
    _touch(tmp_path / "materials" / "atx" / "armor" / "leatherarmor" / "atx_leatherarmor_enclave.bgsm")

    root = RecordNode(form_key="54A165:SeventySix.esm", editor_id="CombatArmor", record_type="ARMO")
    graph = DependencyGraph(
        root=root,
        all_records=[root],
        all_assets=[
            AssetRef(asset_type="material", source_path="materials/armor/combatarmor/combatarmor.bgsm"),
            AssetRef(asset_type="material", source_path="materials/armor/leatherarmor/leatherarmor.bgsm"),
        ],
    )
    result = discover_atx_skins(graph, extracted_dir=str(tmp_path), mod_name="TestMod")

    assert result.slugs_walked == ["armor/combatarmor"]
    mats = {a.source_path.replace("\\", "/").lower() for a in result.new_assets if a.asset_type == "material"}
    assert all("leatherarmor" not in m for m in mats)
