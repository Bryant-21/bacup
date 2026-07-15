"""Tests for DbBuilder smart YAML change detection."""
import os
import sys
import tempfile
from pathlib import Path

import pytest

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))
from ui.toolkit.db_builder import DbBuilder


def test_smart_flag_stored():
    b = DbBuilder(smart=True)
    assert b._smart is True


def test_smart_flag_default_false():
    b = DbBuilder()
    assert b._smart is False


def test_detect_new_plugin():
    """Plugin with no YAML dir → to_extract."""
    with tempfile.TemporaryDirectory() as tmpdir:
        yaml_dir = Path(tmpdir) / "yaml"
        yaml_dir.mkdir()
        plugin = Path(tmpdir) / "NewDLC.esm"
        plugin.touch()

        b = DbBuilder()
        to_extract, to_reextract = b._detect_smart_changes([plugin], yaml_dir)

        assert plugin in to_extract
        assert plugin not in to_reextract


def test_detect_modified_plugin():
    """Plugin newer than its YAML dir → to_reextract."""
    with tempfile.TemporaryDirectory() as tmpdir:
        yaml_dir = Path(tmpdir) / "yaml"
        yaml_dir.mkdir()
        out_dir = yaml_dir / "Starfield"
        out_dir.mkdir()

        plugin = Path(tmpdir) / "Starfield.esm"
        plugin.touch()
        # Make YAML dir appear 10 seconds older than the plugin
        old_mtime = plugin.stat().st_mtime - 10
        os.utime(out_dir, (old_mtime, old_mtime))

        b = DbBuilder()
        to_extract, to_reextract = b._detect_smart_changes([plugin], yaml_dir)

        assert plugin not in to_extract
        assert plugin in to_reextract


def test_detect_up_to_date_plugin():
    """Plugin older than its YAML dir → neither bucket."""
    with tempfile.TemporaryDirectory() as tmpdir:
        yaml_dir = Path(tmpdir) / "yaml"
        yaml_dir.mkdir()
        out_dir = yaml_dir / "Starfield"
        out_dir.mkdir()

        plugin = Path(tmpdir) / "Starfield.esm"
        plugin.touch()
        # Make YAML dir appear 10 seconds newer than the plugin
        new_mtime = plugin.stat().st_mtime + 10
        os.utime(out_dir, (new_mtime, new_mtime))

        b = DbBuilder()
        to_extract, to_reextract = b._detect_smart_changes([plugin], yaml_dir)

        assert plugin not in to_extract
        assert plugin not in to_reextract


def test_force_records_rebuild_reextracts_existing_yaml(tmp_path, monkeypatch):
    """Force YAML extraction must not reuse existing plugin YAML dirs."""
    db_dir = tmp_path / "data"
    game_root = tmp_path / "Skyrim Special Edition"
    game_data = game_root / "Data"
    yaml_dir = db_dir / "skyrimse_esm_yaml"
    game_data.mkdir(parents=True)
    yaml_dir.mkdir(parents=True)
    db_dir.mkdir(exist_ok=True)

    for name in ["Dawnguard.esm", "Skyrim.esm"]:
        (game_data / name).touch()
        (yaml_dir / Path(name).stem).mkdir()
    (db_dir / "skyrimse_records.db").touch()

    monkeypatch.setattr("ui.toolkit.db_builder.get_db_dir", lambda: db_dir)

    extracted = []

    def fake_extract_plugins(plugin_paths, game_data_arg, progress_start, progress_end):
        extracted.extend(p.name for p in plugin_paths)

    def fake_run_preprocess(**kwargs):
        return None

    builder = DbBuilder(
        game_root=str(game_root),
        build_fo4_data=True,
        build_nifs=False,
        build_behaviors=False,
        force_rebuild=True,
        game="skyrimse",
    )
    monkeypatch.setattr(builder, "_extract_plugins", fake_extract_plugins)
    monkeypatch.setattr(builder, "_run_preprocess", fake_run_preprocess)

    builder._build_records()

    assert extracted == ["Dawnguard.esm", "Skyrim.esm"]
