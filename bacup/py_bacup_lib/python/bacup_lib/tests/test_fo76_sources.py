from __future__ import annotations

from pathlib import Path

from bacup_lib.fo76_sources import (
    PathResolution,
    require_resolved,
    resolve_appalachia_btd,
    resolve_fo76_plugin,
)


def test_resolve_fo76_plugin_prefers_data_dir(tmp_path: Path) -> None:
    data_dir = tmp_path / "Data"
    extracted_dir = tmp_path / "extracted" / "fo76"
    data_dir.mkdir(parents=True)
    extracted_dir.mkdir(parents=True)
    data_plugin = data_dir / "SeventySix.esm"
    extracted_plugin = extracted_dir / "SeventySix.esm"
    data_plugin.write_bytes(b"data")
    extracted_plugin.write_bytes(b"extracted")

    result = resolve_fo76_plugin(
        "SeventySix.esm",
        data_dir=data_dir,
        extracted_dir=extracted_dir,
    )

    assert result.path == data_plugin
    assert result.candidates[0] == data_plugin
    assert extracted_plugin in result.candidates


def test_resolve_fo76_plugin_falls_back_to_extracted_dir(tmp_path: Path) -> None:
    data_dir = tmp_path / "Data"
    extracted_dir = tmp_path / "extracted" / "fo76"
    extracted_dir.mkdir(parents=True)
    extracted_plugin = extracted_dir / "SeventySix.esm"
    extracted_plugin.write_bytes(b"extracted")

    result = resolve_fo76_plugin(
        "SeventySix.esm",
        data_dir=data_dir,
        extracted_dir=extracted_dir,
    )

    assert result.path == extracted_plugin
    assert data_dir / "SeventySix.esm" in result.candidates


def test_resolve_appalachia_btd_checks_standard_terrain_paths(tmp_path: Path) -> None:
    extracted_dir = tmp_path / "extracted" / "fo76"
    btd_path = extracted_dir / "Terrain" / "Appalachia.btd"
    btd_path.parent.mkdir(parents=True)
    btd_path.write_bytes(b"btd")
    expected_candidates = (
        extracted_dir / "Terrain" / "Appalachia.btd",
        extracted_dir / "terrain" / "appalachia.btd",
        extracted_dir / "Terrain" / "APPALACHIA.btd",
    )

    result = resolve_appalachia_btd(extracted_dir=extracted_dir)

    assert result.path == btd_path
    assert result.candidates == expected_candidates


def test_require_resolved_raises_with_candidates(tmp_path: Path) -> None:
    missing = PathResolution(
        path=None,
        candidates=(tmp_path / "Terrain" / "Appalachia.btd",),
    )

    try:
        require_resolved(missing, label="Appalachia.btd")
    except FileNotFoundError as exc:
        message = str(exc)
    else:
        raise AssertionError("require_resolved must fail when path is None")

    assert "Appalachia.btd not found" in message
    assert str(tmp_path / "Terrain" / "Appalachia.btd") in message
