from pathlib import Path
import shutil
import sqlite3
from types import SimpleNamespace

import pytest

from bacup_lib.input_preflight import scan_conversion_inputs


def _paths(fo76_data, fo76_ext, fo4_data, catalog):
    return SimpleNamespace(
        source_data_dir=Path(fo76_data),
        source_extracted_dir=Path(fo76_ext),
        target_extracted_dir=None,
        target_data_dir=Path(fo4_data),
        target_asset_catalog_path=Path(catalog),
    )


def _make_papyrus_corpus(root, anchors=("ScriptObject", "Form", "ObjectReference")):
    """The compiled .pex the game ships and extraction unpacks."""
    scripts = Path(root) / "Scripts"
    scripts.mkdir(parents=True, exist_ok=True)
    for anchor in anchors:
        (scripts / f"{anchor}.pex").write_bytes(b"pex")
    return scripts


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
    _make_papyrus_corpus(fo4_data)
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


@pytest.fixture
def no_bundled_corpus(monkeypatch):
    """Isolate install detection from the shipped fallback.

    A bundled corpus lets a bare install run, which would mask every assertion
    about recognizing one.
    """
    monkeypatch.setattr(
        "creation_lib.pex.corpus.bundled_corpus_archive", lambda game: None
    )


def _drop_catalog(paths, tmp_path):
    paths.target_asset_catalog_path = tmp_path / "missing_target_assets.sqlite3"


def _corrupt_catalog(paths, tmp_path):
    paths.target_asset_catalog_path = tmp_path / "unreadable_target_assets.sqlite3"
    paths.target_asset_catalog_path.write_bytes(b"not a sqlite database")


def _drop_scripts(paths, _tmp_path):
    shutil.rmtree(Path(paths.target_data_dir) / "Scripts")


def _scripts_only_in_extracted(paths, tmp_path):
    """Extraction is the normal source of the corpus; Data is the fallback."""
    _drop_scripts(paths, tmp_path)
    paths.target_extracted_dir = tmp_path / "ext" / "fo4"
    _make_papyrus_corpus(paths.target_extracted_dir)


@pytest.mark.parametrize(
    ("mutate", "worldspaces"),
    [
        (None, None),
        (None, ("APPALACHIA",)),
        (_drop_catalog, None),
        (_corrupt_catalog, None),
        (_scripts_only_in_extracted, None),
        # The bundled corpus lets a bare install run.
        (_drop_scripts, None),
    ],
)
def test_usable_layout_has_no_required_missing(tmp_path, mutate, worldspaces):
    paths = _make_complete_layout(tmp_path)
    if mutate is not None:
        mutate(paths, tmp_path)

    kwargs = {"worldspaces": worldspaces} if worldspaces else {}
    report = scan_conversion_inputs(paths, **kwargs)

    assert report.ok
    assert report.required_missing == []


def _unlink_archive(paths, _tmp_path):
    (paths.target_data_dir / "Fallout4 - Main.ba2").unlink()


def _unlink_btos(paths, _tmp_path):
    for bto in (paths.source_extracted_dir / "Meshes" / "Terrain" / "Appalachia" / "Objects").glob("*.bto"):
        bto.unlink()


def _unlink_source_plugin(paths, _tmp_path):
    (paths.source_data_dir / "SeventySix.esm").unlink()


def _drop_extracted(paths, _tmp_path):
    shutil.rmtree(paths.source_extracted_dir)


def _empty_scripts_dir(paths, _tmp_path):
    """An `is_dir()` check alone would pass this and still fail every compile."""
    for pex in (Path(paths.target_data_dir) / "Scripts").glob("*.pex"):
        pex.unlink()


@pytest.mark.parametrize(
    ("mutate", "matches"),
    [
        (_unlink_archive, lambda item, _paths: "FO4 archive" in item.label),
        (_unlink_btos, lambda item, _paths: "BTO" in item.label or "terrain" in item.label.lower()),
        (_unlink_source_plugin, lambda item, _paths: "SeventySix.esm" in item.checked_path),
        (
            _drop_extracted,
            lambda item, paths: item.label == "FO76 extracted directory"
            and item.checked_path == str(paths.source_extracted_dir),
        ),
        (_empty_scripts_dir, lambda item, _paths: "Papyrus" in item.label),
    ],
)
def test_missing_required_input_blocks_the_run(tmp_path, no_bundled_corpus, mutate, matches):
    paths = _make_complete_layout(tmp_path)
    mutate(paths, tmp_path)

    report = scan_conversion_inputs(paths)

    assert not report.ok
    assert sum(matches(item, paths) for item in report.required_missing) == 1


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


def test_missing_papyrus_sources_blocks_the_run_without_a_bundled_corpus(
    tmp_path, no_bundled_corpus
):
    """No types from anywhere means nothing compiles, so stop before converting.

    Regression cover for a user run that reported success while producing
    compiled=0 and stripping 13624 VMAD bindings.
    """
    paths = _make_complete_layout(tmp_path)
    shutil.rmtree(Path(paths.target_data_dir) / "Scripts")

    report = scan_conversion_inputs(paths)

    assert not report.ok
    entry = next(
        item for item in report.required_missing if "Papyrus" in item.label
    )
    assert "ScriptObject" in entry.label
    assert "archives" in entry.fix_hint


def test_partial_source_install_names_only_the_missing_anchors(
    tmp_path, no_bundled_corpus
):
    paths = _make_complete_layout(tmp_path)
    (Path(paths.target_data_dir) / "Scripts" / "Form.pex").unlink()

    entry = next(
        item
        for item in scan_conversion_inputs(paths).required_missing
        if "Papyrus" in item.label
    )

    assert "Form" in entry.label
    assert "ScriptObject" not in entry.label
    assert "ObjectReference" not in entry.label


def test_papyrus_anchors_found_in_archives_do_not_block(monkeypatch, tmp_path):
    """A player who never extracted the game is not a broken install.

    Fallout 4 keeps its compiled scripts inside `Fallout4 - Misc.ba2`, so the
    loose Scripts directory is empty on a stock install while the conversion
    reads those .pex out of the archives perfectly well.
    """
    from bacup_lib import input_preflight

    fo4_data = tmp_path / "Data"
    fo4_data.mkdir()

    class _Store:
        def __init__(self, **kwargs):
            pass

        def has_asset(self, path):
            return path in {
                "scripts/scriptobject.pex",
                "scripts/form.pex",
                "scripts/objectreference.pex",
            }

    monkeypatch.setattr(
        "bacup_lib.target_assets.TargetAssetStore", _Store, raising=False
    )
    assert input_preflight._missing_papyrus_anchors(fo4_data, fo4_data=fo4_data) == []
