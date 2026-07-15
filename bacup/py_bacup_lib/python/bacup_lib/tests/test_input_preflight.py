from pathlib import Path
import shutil
import sqlite3
from types import SimpleNamespace

from bacup_lib.input_preflight import _resolve_ci_path, scan_conversion_inputs


def _paths(fo76_data, fo76_ext, fo4_data, catalog):
    return SimpleNamespace(
        source_data_dir=Path(fo76_data),
        source_extracted_dir=Path(fo76_ext),
        target_extracted_dir=None,
        target_data_dir=Path(fo4_data),
        target_asset_catalog_path=Path(catalog),
    )


def _make_complete_layout(tmp_path):
    fo76_data = tmp_path / "FO76" / "Data"
    fo76_ext = tmp_path / "ext" / "fo76"
    fo4_data = tmp_path / "FO4" / "Data"
    catalog = tmp_path / "fo4_target_assets.sqlite3"
    (fo76_data).mkdir(parents=True)
    (fo76_data / "SeventySix.esm").write_bytes(b"esm")
    bto_dir = fo76_ext / "Meshes" / "Terrain" / "Appalachia" / "Objects"
    bto_dir.mkdir(parents=True)
    (bto_dir / "Appalachia.16.-14.-13.bto").write_bytes(b"bto")
    fo4_data.mkdir(parents=True)
    (fo4_data / "Fallout4.esm").write_bytes(b"esm")
    (fo4_data / "Fallout4 - Main.ba2").write_bytes(b"ba2")
    with sqlite3.connect(catalog) as db:
        db.execute(
            "CREATE TABLE archives "
            "(name TEXT, content_pack TEXT, required INTEGER, priority INTEGER)"
        )
        db.execute(
            "INSERT INTO archives VALUES (?, ?, ?, ?)",
            ("Fallout4 - Main.ba2", "base", 1, 0),
        )
    return _paths(fo76_data, fo76_ext, fo4_data, catalog)


def test_complete_layout_has_no_required_missing(tmp_path):
    report = scan_conversion_inputs(_make_complete_layout(tmp_path))
    assert report.ok
    assert report.required_missing == []
    assert report.optional_missing == []


def test_target_extracted_dir_is_not_required(tmp_path):
    paths = _make_complete_layout(tmp_path)
    assert paths.target_extracted_dir is None
    assert scan_conversion_inputs(paths).ok


def test_missing_target_asset_catalog_is_left_for_auto_build(tmp_path):
    paths = _make_complete_layout(tmp_path)
    paths.target_asset_catalog_path = tmp_path / "missing_target_assets.sqlite3"

    report = scan_conversion_inputs(paths)

    assert report.ok
    assert not any(
        item.label == "FO4 target asset catalog"
        for item in report.required_missing
    )


def test_unreadable_target_asset_catalog_is_left_for_auto_rebuild(tmp_path):
    paths = _make_complete_layout(tmp_path)
    paths.target_asset_catalog_path = tmp_path / "unreadable_target_assets.sqlite3"
    paths.target_asset_catalog_path.write_bytes(b"not a sqlite database")

    report = scan_conversion_inputs(paths)

    assert report.ok
    assert not any(
        item.label == "FO4 target asset catalog"
        for item in report.required_missing
    )


def test_missing_required_fo4_archive_fails(tmp_path):
    paths = _make_complete_layout(tmp_path)
    (paths.target_data_dir / "Fallout4 - Main.ba2").unlink()

    report = scan_conversion_inputs(paths)

    assert not report.ok
    assert any("FO4 archive" in item.label for item in report.required_missing)


def test_missing_optional_dlc_archive_is_reported_but_not_required(tmp_path):
    paths = _make_complete_layout(tmp_path)
    with sqlite3.connect(paths.target_asset_catalog_path) as db:
        db.execute(
            "INSERT INTO archives VALUES (?, ?, ?, ?)",
            ("DLCCoast - Main.ba2", "DLCCoast", 0, 10),
        )

    report = scan_conversion_inputs(paths)

    assert report.ok
    assert [item.label for item in report.optional_missing] == [
        "FO4 archive (DLCCoast)"
    ]


def test_missing_btos_is_required_missing(tmp_path):
    paths = _make_complete_layout(tmp_path)
    for bto in (paths.source_extracted_dir / "Meshes" / "Terrain" / "Appalachia" / "Objects").glob("*.bto"):
        bto.unlink()
    report = scan_conversion_inputs(paths)
    assert not report.ok
    labels = [item.label for item in report.required_missing]
    assert any("BTO" in label or "terrain" in label.lower() for label in labels)


def test_missing_source_plugin_is_required_missing(tmp_path):
    paths = _make_complete_layout(tmp_path)
    (paths.source_data_dir / "SeventySix.esm").unlink()
    report = scan_conversion_inputs(paths)
    assert not report.ok
    assert any("SeventySix.esm" in item.checked_path for item in report.required_missing)


def test_bto_match_is_case_insensitive(tmp_path):
    paths = _make_complete_layout(tmp_path)
    report = scan_conversion_inputs(paths, worldspaces=("APPALACHIA",))
    assert report.ok


def test_missing_extracted_directory_is_reported_directly(tmp_path):
    paths = _make_complete_layout(tmp_path)
    shutil.rmtree(paths.source_extracted_dir)

    report = scan_conversion_inputs(paths)

    extracted_errors = [
        item for item in report.required_missing
        if item.label == "FO76 extracted directory"
    ]
    assert len(extracted_errors) == 1
    assert extracted_errors[0].checked_path == str(paths.source_extracted_dir)


def test_resolve_ci_path_with_repeated_segment(tmp_path):
    (tmp_path / "Meshes" / "Terrain").mkdir(parents=True)
    resolved = _resolve_ci_path(tmp_path, "Meshes", "Terrain", "Meshes")
    assert resolved == tmp_path / "Meshes" / "Terrain" / "Meshes"
