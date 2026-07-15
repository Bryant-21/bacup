from __future__ import annotations

import gc
import sqlite3
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

import pytest

from bacup_lib.target_assets import (
    TargetAssetStore,
    default_target_asset_catalog,
    normalize_target_asset_path,
)


def _write_catalog(
    path: Path,
    *,
    archive_name: str = "Fallout4 - Test.ba2",
    expected_size: int = 3,
    required: bool = True,
) -> None:
    with sqlite3.connect(path) as db:
        db.executescript(
            """
            CREATE TABLE metadata(key TEXT PRIMARY KEY, value TEXT NOT NULL);
            CREATE TABLE archives(
                id INTEGER PRIMARY KEY,
                name TEXT NOT NULL UNIQUE COLLATE NOCASE,
                content_pack TEXT NOT NULL,
                required INTEGER NOT NULL,
                expected_size INTEGER NOT NULL,
                priority INTEGER NOT NULL
            );
            CREATE TABLE directories(
                id INTEGER PRIMARY KEY,
                path_key TEXT NOT NULL UNIQUE
            );
            CREATE TABLE assets(
                id INTEGER PRIMARY KEY,
                directory_id INTEGER NOT NULL,
                name_key TEXT NOT NULL,
                kind TEXT NOT NULL,
                UNIQUE(directory_id, name_key)
            );
            CREATE TABLE asset_owners(
                asset_id INTEGER NOT NULL,
                archive_id INTEGER NOT NULL,
                priority INTEGER NOT NULL,
                PRIMARY KEY(asset_id, archive_id)
            ) WITHOUT ROWID;
            CREATE TABLE asset_dependencies(
                source_asset_id INTEGER NOT NULL,
                target_asset_id INTEGER NOT NULL,
                ref_kind TEXT NOT NULL,
                PRIMARY KEY(source_asset_id, target_asset_id, ref_kind)
            ) WITHOUT ROWID;
            """
        )
        db.executemany(
            "INSERT INTO metadata VALUES (?, ?)",
            [("schema_version", "2"), ("target_game", "fo4")],
        )
        db.execute(
            "INSERT INTO archives VALUES (1, ?, 'base', ?, ?, 10)",
            (archive_name, int(required), expected_size),
        )
        db.executemany(
            "INSERT INTO directories VALUES (?, ?)",
            [(1, "meshes/actors"), (2, "materials/actors")],
        )
        db.executemany(
            "INSERT INTO assets VALUES (?, ?, ?, ?)",
            [(1, 1, "head.nif", "nif"), (2, 2, "head.bgsm", "material")],
        )
        db.executemany(
            "INSERT INTO asset_owners VALUES (?, 1, 10)", [(1,), (2,)]
        )
        db.execute(
            "INSERT INTO asset_dependencies VALUES (1, 2, 'nif_material')"
        )


def _store_layout(tmp_path: Path, *, required: bool = True):
    data = tmp_path / "Data"
    data.mkdir()
    archive = data / "Fallout4 - Test.ba2"
    if required:
        archive.write_bytes(b"ba2")
    catalog = tmp_path / "catalog.sqlite3"
    _write_catalog(catalog, required=required)
    overlay = tmp_path / "overlay"
    overlay_head = overlay / "Meshes" / "Actors" / "Head.nif"
    overlay_head.parent.mkdir(parents=True)
    overlay_head.write_bytes(b"head-bytes")
    return data, catalog, overlay, overlay_head


def test_normalize_target_asset_path_is_data_relative_and_casefolded():
    assert (
        normalize_target_asset_path(r"C:\Fallout 4\Data\Meshes\Actors\Head.NIF")
        == "meshes/actors/head.nif"
    )


def test_membership_listing_dependencies_and_case_insensitive_overlay(tmp_path):
    data, catalog, overlay, _ = _store_layout(tmp_path)
    store = TargetAssetStore(
        target_data_dir=data,
        catalog_path=catalog,
        cache_dir=tmp_path / "cache",
        overlay_dir=overlay,
    )

    assert store.has_asset(r"DATA\MESHES\ACTORS\HEAD.NIF")
    assert store.list_assets(prefix="MESHES/", suffix=".NIF") == [
        "meshes/actors/head.nif"
    ]
    assert store.dependency_closure(["Meshes/Actors/Head.nif"]) == [
        "materials/actors/head.bgsm",
        "meshes/actors/head.nif",
    ]


def test_materialization_is_persistent_and_atomic(tmp_path):
    data, catalog, overlay, _ = _store_layout(tmp_path)
    store = TargetAssetStore(
        target_data_dir=data,
        catalog_path=catalog,
        cache_dir=tmp_path / "cache",
        overlay_dir=overlay,
    )

    with ThreadPoolExecutor(max_workers=8) as pool:
        paths = list(pool.map(store.materialize, ["meshes/actors/head.nif"] * 8))

    assert len(set(paths)) == 1
    assert paths[0].read_bytes() == b"head-bytes"
    assert store.stats()["files_extracted"] == 1

    second = TargetAssetStore(
        target_data_dir=data,
        catalog_path=catalog,
        cache_dir=tmp_path / "cache",
        overlay_dir=overlay,
    )
    assert second.materialize("meshes/actors/head.nif") == paths[0]
    assert second.stats()["files_extracted"] == 0
    assert second.stats()["cache_hits"] == 1


def test_second_identical_run_performs_zero_archive_extractions(tmp_path):
    from creation_lib.ba2.native_runtime import pack_archive

    data = tmp_path / "Data"
    data.mkdir()
    source = tmp_path / "archive_source"
    member = source / "Meshes" / "Actors" / "Head.nif"
    member.parent.mkdir(parents=True)
    member.write_bytes(b"official-head-bytes")
    archive = data / "Fallout4 - Test.ba2"
    pack_archive(str(source), str(archive), "fo4", compress=False)
    catalog = tmp_path / "catalog.sqlite3"
    _write_catalog(catalog, expected_size=archive.stat().st_size)

    first = TargetAssetStore(
        target_data_dir=data,
        catalog_path=catalog,
        cache_dir=tmp_path / "cache",
    )
    cached = first.materialize("meshes/actors/head.nif")
    assert cached.read_bytes() == b"official-head-bytes"
    assert first.stats()["files_extracted"] == 1

    second = TargetAssetStore(
        target_data_dir=data,
        catalog_path=catalog,
        cache_dir=tmp_path / "cache",
    )
    assert second.materialize("meshes/actors/head.nif") == cached
    assert second.stats()["files_extracted"] == 0
    assert second.stats()["cache_hits"] == 1


def test_overlay_fingerprint_change_namespaces_stale_cache(tmp_path):
    data, catalog, overlay, overlay_head = _store_layout(tmp_path)
    first = TargetAssetStore(
        target_data_dir=data,
        catalog_path=catalog,
        cache_dir=tmp_path / "cache",
        overlay_dir=overlay,
    )
    first_path = first.materialize("meshes/actors/head.nif")
    del first
    gc.collect()
    overlay_head.write_bytes(b"changed-head-bytes")
    second = TargetAssetStore(
        target_data_dir=data,
        catalog_path=catalog,
        cache_dir=tmp_path / "cache",
        overlay_dir=overlay,
    )
    second_path = second.materialize("meshes/actors/head.nif")

    assert first_path != second_path
    assert first_path.read_bytes() == b"head-bytes"
    assert second_path.read_bytes() == b"changed-head-bytes"


def test_missing_required_archive_is_an_error(tmp_path):
    data = tmp_path / "Data"
    data.mkdir()
    catalog = tmp_path / "catalog.sqlite3"
    _write_catalog(catalog, required=True)

    with pytest.raises(ValueError, match="required FO4 archive is missing"):
        TargetAssetStore(
            target_data_dir=data,
            catalog_path=catalog,
            cache_dir=tmp_path / "cache",
        )


def test_absent_optional_archive_is_filtered(tmp_path):
    data = tmp_path / "Data"
    data.mkdir()
    catalog = tmp_path / "catalog.sqlite3"
    _write_catalog(catalog, required=False)
    store = TargetAssetStore(
        target_data_dir=data,
        catalog_path=catalog,
        cache_dir=tmp_path / "cache",
    )

    assert not store.has_asset("meshes/actors/head.nif")
    assert store.list_assets() == []


def test_size_mismatch_reindexes_only_installed_archive_and_writes_overlay(tmp_path):
    from creation_lib.ba2.native_runtime import pack_archive

    data = tmp_path / "Data"
    data.mkdir()
    source = tmp_path / "archive_source"
    member = source / "Meshes" / "New" / "Graph.hkx"
    member.parent.mkdir(parents=True)
    member.write_bytes(b"hkx")
    archive = data / "Fallout4 - Test.ba2"
    pack_archive(str(source), str(archive), "fo4", compress=False)
    catalog = tmp_path / "catalog.sqlite3"
    _write_catalog(catalog, expected_size=archive.stat().st_size + 1)

    store = TargetAssetStore(
        target_data_dir=data,
        catalog_path=catalog,
        cache_dir=tmp_path / "cache",
    )

    assert store.has_asset("meshes/new/graph.hkx")
    assert not store.has_asset("meshes/actors/head.nif")
    assert store.stats()["archives_reindexed"] == 1
    assert (store.cache_data_root.parent / "catalog_overlay.sqlite3").is_file()


def test_packaged_catalog_is_versioned_metadata_only_corpus():
    catalog = default_target_asset_catalog()
    assert catalog.is_file()
    with sqlite3.connect(f"file:{catalog.as_posix()}?mode=ro", uri=True) as db:
        metadata = dict(db.execute("SELECT key, value FROM metadata"))
        assert metadata["schema_version"] == "2"
        assert metadata["target_game"] == "fo4"
        assert metadata["game_build"]
        assert db.execute("SELECT COUNT(*) FROM archives").fetchone()[0] >= 20
        assert db.execute("SELECT COUNT(*) FROM assets").fetchone()[0] > 100_000
        assert (
            db.execute("SELECT COUNT(*) FROM asset_dependencies").fetchone()[0]
            > 1_000
        )
        views = {
            row[0]
            for row in db.execute(
                "SELECT name FROM sqlite_master WHERE type='view'"
            )
        }
        assert {"catalog_assets", "catalog_dependencies"} <= views
        assert (
            db.execute(
                "SELECT COUNT(*) FROM catalog_assets "
                "WHERE path_key != lower(path_key) OR instr(path_key, '\\') != 0"
            ).fetchone()[0]
            == 0
        )
        declared_types = {
            str(row[2]).casefold()
            for table in (
                "metadata",
                "archives",
                "directories",
                "assets",
                "asset_owners",
                "asset_dependencies",
            )
            for row in db.execute(f"PRAGMA table_info({table})")
        }
        assert "blob" not in declared_types


def test_release_builder_publishes_catalog_without_preextracting(tmp_path):
    from creation_lib.ba2.native_runtime import pack_archive
    from bacup_lib.native_runtime import load_native_module

    data = tmp_path / "Data"
    source = tmp_path / "source"
    script = source / "Scripts" / "Base" / "Example.pex"
    script.parent.mkdir(parents=True)
    script.write_bytes(b"pex")
    data.mkdir()
    archive = data / "Fallout4 - Test.ba2"
    pack_archive(str(source), str(archive), "fo4", compress=False)
    output = tmp_path / "built.sqlite3"

    load_native_module().conversion_build_target_asset_catalog(
        str(data), str(output), "test-build"
    )

    assert output.is_file()
    with sqlite3.connect(output) as db:
        assert dict(db.execute("SELECT key, value FROM metadata"))["game_build"] == (
            "test-build"
        )
        assert db.execute("SELECT COUNT(*) FROM assets").fetchone()[0] == 1
        assert db.execute(
            "SELECT path_key FROM catalog_assets"
        ).fetchone()[0] == "scripts/base/example.pex"
