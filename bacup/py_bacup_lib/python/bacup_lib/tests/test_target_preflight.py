from __future__ import annotations

from pathlib import Path
from types import SimpleNamespace

import pytest


def test_record_preflight_dispatches_official_paths_and_keeps_native_rows(monkeypatch, tmp_path):
    from bacup_lib import target_preflight

    fallout4 = tmp_path / "Fallout4.esm"
    robot = tmp_path / "DLCRobot.esm"
    fallout4.touch()
    robot.touch()
    rows = [("Ammo10mm", "AMMO", "01F276:Fallout4.esm")]
    warnings = ["native duplicate warning"]
    calls = []

    def build(game, paths):
        calls.append((game, paths))
        return rows, warnings

    monkeypatch.setattr(target_preflight, "load_native_module", lambda: SimpleNamespace(
        conversion_target_record_preflight=build,
    ))
    result = target_preflight.build_target_record_preflight("fo4", target_data_dir=tmp_path)

    assert calls == [("fo4", [str(fallout4), str(robot)])]
    assert result.rows_for_native() is rows
    assert result.warnings is warnings
    assert result.master_names == ["Fallout4.esm", "DLCRobot.esm"]
    assert result.missing_masters == [
        "DLCworkshop01.esm", "DLCCoast.esm", "DLCworkshop02.esm",
        "DLCworkshop03.esm", "DLCNukaWorld.esm",
    ]


def test_record_preflight_preserves_native_failure(monkeypatch, tmp_path):
    from bacup_lib import target_preflight

    (tmp_path / "Fallout4.esm").touch()

    def fail(game, paths):
        raise RuntimeError("invalid target master")

    monkeypatch.setattr(target_preflight, "load_native_module", lambda: SimpleNamespace(
        conversion_target_record_preflight=fail,
    ))
    with pytest.raises(RuntimeError, match="invalid target master"):
        target_preflight.build_target_record_preflight("fo4", target_data_dir=tmp_path)


@pytest.mark.parametrize(
    ("target_file", "asset_type", "source_path"),
    [
        ("Meshes/DLC03/Landscape/Plants/Bramble01.nif", "nif", "meshes/dlc03/landscape/plants/bramble01.nif"),
        ("meshes/DLC03/Landscape/Plants/Bramble01.nif", "nif", "Meshes/DLC03/Landscape/Plants/Bramble01.nif"),
        ("Meshes/DLC03/Landscape/Plants/Bramble01.nif", "nif", "dlc03/landscape/plants/bramble01.nif"),
        ("Textures/DLC03/Plants/Bramble01_d.dds", "texture", "dlc03/plants/bramble01_d.dds"),
        ("Meshes/DLC03/Landscape/Plants/Bramble01.nif", "nif", "<source_root>/meshes/dlc03/landscape/plants/bramble01.nif"),
    ],
)
def test_target_asset_index_matches_asset_ref(tmp_path, target_file, asset_type, source_path):
    from bacup_lib.target_preflight import build_target_asset_index

    target_root = tmp_path / "extracted" / "fo4"
    path = target_root / target_file
    path.parent.mkdir(parents=True)
    path.write_bytes(b"asset")
    if source_path.startswith("<source_root>/"):
        source_path = tmp_path / "extracted" / "fo76" / source_path.removeprefix("<source_root>/")

    index = build_target_asset_index(target_root)

    assert index.has_asset(SimpleNamespace(asset_type=asset_type, source_path=source_path))


def test_target_asset_index_indexes_extracted_and_data_roots_in_parallel(tmp_path):
    from bacup_lib.target_preflight import build_target_asset_index

    extracted = tmp_path / "extracted" / "fo4"
    data = tmp_path / "Fallout4" / "Data"
    extracted_texture = (
        extracted / "Textures" / "DLC05" / "Architecture" / "WarehouseGarageDoor01_d.DDS"
    )
    data_mesh = data / "Meshes" / "Actors" / "Robot" / "Protectron.nif"
    extracted_texture.parent.mkdir(parents=True)
    data_mesh.parent.mkdir(parents=True)
    extracted_texture.write_bytes(b"dds")
    data_mesh.write_bytes(b"nif")

    index = build_target_asset_index([extracted, data], workers=4)

    assert index.workers == 4
    assert index.files_scanned == 2
    assert index.has_asset(
        SimpleNamespace(
            asset_type="texture",
            source_path="textures/dlc05/architecture/warehousegaragedoor01_d.dds",
        )
    )
    assert index.has_asset(
        SimpleNamespace(
            asset_type="nif",
            source_path="meshes/actors/robot/protectron.nif",
        )
    )
