"""Tests for Bulk NIF Converter helper functions."""
import os
from unittest.mock import MagicMock

import pytest

from bacup_ui.tools.nif_converter import (
    _auto_output_dir,
    _export_texture_file,
    _resolve_output_path,
)


def test_resolve_preserves_rel_dir():
    result = _resolve_output_path("weapons/guns", "ak47.nif", "/out", ".nif")
    assert result == os.path.normpath(os.path.join("/out", "weapons", "guns", "ak47.nif"))


def test_resolve_no_rel_dir_none():
    result = _resolve_output_path(None, "ak47.nif", "/out", ".nif")
    assert result == os.path.normpath(os.path.join("/out", "ak47.nif"))


def test_resolve_no_rel_dir_empty_string():
    # collect_files sets rel_dir="" for root-level files; must behave same as None
    result = _resolve_output_path("", "ak47.nif", "/out", ".nif")
    assert result == os.path.normpath(os.path.join("/out", "ak47.nif"))


def test_resolve_changes_extension_to_fbx():
    result = _resolve_output_path(None, "weapon.nif", "/out", ".fbx")
    assert result.endswith(".fbx")
    assert "weapon" in result


def test_resolve_strips_original_extension():
    result = _resolve_output_path(None, "mesh.nif", "/out", ".nif")
    # Should not produce mesh.nif.nif
    assert result.endswith("mesh.nif")
    assert not result.endswith(".nif.nif")


def test_auto_output_dir_appends_converted():
    result = _auto_output_dir(os.path.join("/some", "path", "my_nifs"))
    assert result == os.path.normpath(os.path.join("/some", "path", "my_nifs_converted"))


def test_auto_output_dir_ignores_trailing_sep():
    path = os.path.join("/some", "path", "my_nifs") + os.sep
    result = _auto_output_dir(path)
    assert result == os.path.normpath(os.path.join("/some", "path", "my_nifs_converted"))


def test_export_fo76_reflectivity_merges_lighting_sibling(monkeypatch, tmp_path):
    from creation_lib.core.game_profiles import FO4_PROFILE, FO76_PROFILE

    mod_root = tmp_path / "mod"
    texture_dir = mod_root / "Textures" / "Bundle"
    texture_dir.mkdir(parents=True)
    (texture_dir / "bundle_r.dds").write_bytes(b"dds")
    (texture_dir / "bundle_l.dds").write_bytes(b"dds")

    native_mock = MagicMock(
        return_value={"converted": [{"role": "specular", "path": "out.dds"}]}
    )
    monkeypatch.setattr(
        "bacup_lib.texture.native.convert_texture_paths",
        native_mock,
    )

    status, note = _export_texture_file(
        "Textures/Bundle/bundle_r.dds",
        str(mod_root),
        str(tmp_path / "out"),
        FO76_PROFILE,
        FO4_PROFILE,
    )

    assert (status, note) == ("ok", "converted")
    args = native_mock.call_args.args
    assert [(path.name, role) for path, role in args[0]] == [
        ("bundle_r.dds", "reflectivity"),
        ("bundle_l.dds", "lighting"),
    ]


def test_export_fo76_lighting_noops_when_reflectivity_sibling_exists(monkeypatch, tmp_path):
    from creation_lib.core.game_profiles import FO4_PROFILE, FO76_PROFILE

    mod_root = tmp_path / "mod"
    texture_dir = mod_root / "Textures" / "Bundle"
    texture_dir.mkdir(parents=True)
    (texture_dir / "bundle_r.dds").write_bytes(b"dds")
    (texture_dir / "bundle_l.dds").write_bytes(b"dds")

    native_mock = MagicMock()
    monkeypatch.setattr(
        "bacup_lib.texture.native.convert_texture_paths",
        native_mock,
    )

    status, note = _export_texture_file(
        "Textures/Bundle/bundle_l.dds",
        str(mod_root),
        str(tmp_path / "out"),
        FO76_PROFILE,
        FO4_PROFILE,
    )

    assert (status, note) == ("ok", "merged with reflectivity sibling")
    native_mock.assert_not_called()
