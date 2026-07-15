"""Tests for discover_weapon_animations — scoped FO76 per-weapon anim discovery."""
from __future__ import annotations

from pathlib import Path

from bacup_lib.atx_walker import discover_weapon_animations
from bacup_lib.models import AssetRef, DependencyGraph, RecordNode


def _weapon_graph(editor_id: str = "GaussPistol", record_type: str = "WEAP") -> DependencyGraph:
    # Production: the native walker stores the raw 4-char signature ("WEAP").
    root = RecordNode(form_key="54A165:SeventySix.esm", editor_id=editor_id, record_type=record_type)
    return DependencyGraph(root=root, all_records=[root], all_assets=[])


def _touch(path: Path) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(b"\x00")


def test_discovers_gausspistol_clips_across_all_roots(tmp_path: Path):
    m = tmp_path / "meshes" / "actors"
    # Should match: character 3rd-person, character 1st-person, power-armor.
    _touch(m / "character" / "animations" / "weapon" / "gausspistol" / "wpnreload.hkx")
    _touch(m / "character" / "animations" / "weapon" / "gausspistol" / "wpnassemblypose.hkx")
    _touch(m / "character" / "_1stperson" / "animations" / "gausspistol" / "wpnreload.hkx")
    _touch(m / "character" / "_1stperson" / "animations" / "gausspistol" / "drummag" / "wpnreload.hkx")
    _touch(m / "powerarmor" / "animations" / "weapons" / "gausspistol" / "wpnreload.hkx")

    assets = discover_weapon_animations(_weapon_graph(), extracted_dir=str(tmp_path))

    rels = {a.source_path.replace("\\", "/").lower() for a in assets}
    assert "meshes/actors/character/animations/weapon/gausspistol/wpnreload.hkx" in rels
    assert "meshes/actors/character/animations/weapon/gausspistol/wpnassemblypose.hkx" in rels
    assert "meshes/actors/character/_1stperson/animations/gausspistol/wpnreload.hkx" in rels
    assert "meshes/actors/character/_1stperson/animations/gausspistol/drummag/wpnreload.hkx" in rels
    assert "meshes/actors/powerarmor/animations/weapons/gausspistol/wpnreload.hkx" in rels
    assert len(assets) == 5
    assert all(a.asset_type == "animation" and a.resolved_path for a in assets)


def test_does_not_pull_shared_or_sibling_weapon_dirs(tmp_path: Path):
    m = tmp_path / "meshes" / "actors" / "character" / "animations" / "weapon"
    _touch(m / "gausspistol" / "wpnreload.hkx")     # match
    _touch(m / "pistol" / "wpnreload.hkx")          # shared class — must NOT match
    _touch(m / "gaussrifle" / "wpnreload.hkx")      # sibling weapon — must NOT match

    assets = discover_weapon_animations(_weapon_graph(), extracted_dir=str(tmp_path))

    rels = {a.source_path.replace("\\", "/").lower() for a in assets}
    assert rels == {"meshes/actors/character/animations/weapon/gausspistol/wpnreload.hkx"}


def test_accepts_weapon_display_label_record_type(tmp_path: Path):
    # Test/fixture code uses the display label; the guard must accept it too.
    _touch(tmp_path / "meshes" / "actors" / "character" / "animations" / "weapon" / "gausspistol" / "wpnreload.hkx")
    graph = _weapon_graph(record_type="Weapons")
    assets = discover_weapon_animations(graph, extracted_dir=str(tmp_path))
    assert len(assets) == 1


def test_skips_non_weapon_root_record(tmp_path: Path):
    _touch(tmp_path / "meshes" / "actors" / "character" / "animations" / "weapon" / "gausspistol" / "wpnreload.hkx")
    graph = _weapon_graph(record_type="RACE")
    assert discover_weapon_animations(graph, extracted_dir=str(tmp_path)) == []


def test_dedupes_against_existing_assets(tmp_path: Path):
    rel = "meshes/actors/character/animations/weapon/gausspistol/wpnreload.hkx"
    _touch(tmp_path / Path(rel))
    graph = _weapon_graph()
    graph.all_assets.append(AssetRef(asset_type="animation", source_path=rel))

    assets = discover_weapon_animations(graph, extracted_dir=str(tmp_path))
    assert assets == []
