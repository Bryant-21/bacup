from __future__ import annotations

import json
import os
import re
import struct
from pathlib import Path

import pytest
import yaml


@pytest.mark.integration
def test_m1_texture_authoring_builds(tmp_path):
    fo76_data = os.environ.get("FO76_DATA")
    if not fo76_data:
        pytest.skip("requires FO76_DATA")
    btd = Path(fo76_data) / "Terrain" / "Appalachia.btd"
    if not btd.is_file():
        pytest.skip("requires FO76_DATA/Terrain/Appalachia.btd")
    repo_root = Path(__file__).resolve().parents[2]
    records_db = _fo76_records_db(repo_root)
    if not records_db.is_file():
        pytest.skip("requires FO76_RECORDS_DB or a parent data/fo76_records.db")

    mod_root = tmp_path / "B21_AppalachiaTestWorld"
    yaml_dir = mod_root / "yaml"
    debug_dir = mod_root / "debug" / "terrain"

    from bacup_lib.terrain.fo76_btd import (
        TerrainConversionRequest,
        convert_fo76_btd_to_fo4_land,
    )

    report = convert_fo76_btd_to_fo4_land(
        TerrainConversionRequest(
            btd_path=str(btd),
            output_authoring_dir=str(yaml_dir),
            plugin_name="B21_AppalachiaTestWorld.esp",
            worldspace_editor_id="B21_AppalachiaTestWorld",
            source_min_x=0,
            source_min_y=0,
            source_max_x=0,
            source_max_y=0,
            debug_output_dir=str(debug_dir),
            records_db=str(records_db),
            fo76_data_dir=fo76_data,
        )
    )
    assert report["status"] == "ok"
    assert report["cells_written"] == 1
    assert report["converted_texture_count"] > 0
    assert (yaml_dir / "plugin.yaml").is_file()
    assert (mod_root / "data" / "textures" / "terrain").is_dir()
    assert Path(report["diagnostics_path"]).is_file()
    assert report["vhgt_delta_clamp_underflows"] >= 0
    assert report["vhgt_delta_clamp_overflows"] >= 0

    cell_yaml = (
        yaml_dir
        / "records"
        / "WRLD"
        / "B21_AppalachiaTestWorld - 000800_B21_AppalachiaTestWorld.esp"
        / "0, 0"
        / "0, 0"
        / "0, 0"
        / "RecordData.yaml"
    ).read_text()
    assert '  - signature: DATA\n    data_hex: "0200"\n' in cell_yaml
    assert (
        '  - signature: XCLC\n    data_hex: "000000000000000000000000"\n'
        in cell_yaml
    )
    assert '  - signature: XCLW\n    data_hex: "FFFF7F7F"\n' in cell_yaml
    land_fields = yaml.safe_load(cell_yaml)["Landscape"]["fields"]
    texture_fields = [entry for entry in land_fields if "BTXT" in entry or "ATXT" in entry]
    alpha_fields = [entry["AlphaLayerData"] for entry in land_fields if "AlphaLayerData" in entry]
    assert texture_fields
    assert alpha_fields
    current_base_quadrant = None
    current_group_alpha_count = 0
    for entry in texture_fields:
        layer = entry.get("ATXT") or entry.get("BTXT")
        quadrant = layer["Quadrant"]
        if "BTXT" in entry:
            if current_base_quadrant is not None:
                assert current_group_alpha_count > 0
            current_base_quadrant = quadrant
            current_group_alpha_count = 0
        else:
            assert quadrant == current_base_quadrant
            current_group_alpha_count += 1
        if "ATXT" in entry:
            assert layer["Layer"] <= 2
            assert layer["UnknownByte3"] == 127
        else:
            assert layer["UnknownByte3"] == 2
        assert layer["Texture"]["reference"]["plugin"] == "B21_AppalachiaTestWorld.esp"
        assert re.fullmatch(r"[0-9A-F]{6}", layer["Texture"]["reference"]["object_id"])
    assert current_group_alpha_count > 0
    for alpha in alpha_fields:
        payload = bytes.fromhex(alpha["raw_hex"])
        positions = [
            int.from_bytes(payload[index : index + 2], "little")
            for index in range(0, len(payload), 8)
        ]
        assert max(positions, default=0) <= 288
        assert all(position % 17 <= 16 and position // 17 <= 16 for position in positions)

    diagnostics = json.loads(Path(report["diagnostics_path"]).read_text())
    assert (
        diagnostics["diagnostics"]["vhgt_delta_clamp_underflows"]
        == report["vhgt_delta_clamp_underflows"]
    )
    assert (
        diagnostics["diagnostics"]["vhgt_delta_clamp_overflows"]
        == report["vhgt_delta_clamp_overflows"]
    )


@pytest.mark.integration
def test_m1_multicell_height_authoring_builds_esp(tmp_path):
    fo76_data = os.environ.get("FO76_DATA")
    if not fo76_data:
        pytest.skip("requires FO76_DATA")
    btd = Path(fo76_data) / "Terrain" / "Appalachia.btd"
    if not btd.is_file():
        pytest.skip("requires FO76_DATA/Terrain/Appalachia.btd")

    mod_root = tmp_path / "B21_AppalachiaMultiCell"
    yaml_dir = mod_root / "yaml"
    debug_dir = mod_root / "debug" / "terrain"
    esp_path = mod_root / "B21_AppalachiaMultiCell.esp"

    from creation_lib.esp import Plugin
    from bacup_lib.terrain.fo76_btd import (
        TerrainConversionRequest,
        convert_fo76_btd_to_fo4_land,
    )

    report = convert_fo76_btd_to_fo4_land(
        TerrainConversionRequest(
            btd_path=str(btd),
            output_authoring_dir=str(yaml_dir),
            plugin_name="B21_AppalachiaMultiCell.esp",
            worldspace_editor_id="B21_AppalachiaMultiCell",
            source_min_x=0,
            source_min_y=0,
            source_max_x=1,
            source_max_y=1,
            resample_mode="weighted",
            debug_output_dir=str(debug_dir),
            emit_textures=False,
            build_esp=True,
            esp_output_path=str(esp_path),
        )
    )

    assert report["status"] == "ok"
    assert report["cells_written"] == 4
    assert esp_path.is_file()

    with Plugin.load(esp_path, game="fo4", backend="native") as plugin:
        signatures = [record.signature for record in plugin.records]

    assert signatures.count("CELL") == 4
    assert signatures.count("LAND") == 4


@pytest.mark.integration
def test_m1_heightmap_export_writes_r32_float_dds(tmp_path):
    fo76_data = os.environ.get("FO76_DATA")
    if not fo76_data:
        pytest.skip("requires FO76_DATA")
    btd = Path(fo76_data) / "Terrain" / "Appalachia.btd"
    if not btd.is_file():
        pytest.skip("requires FO76_DATA/Terrain/Appalachia.btd")

    mod_root = tmp_path / "B21_AppalachiaHeightmap"
    yaml_dir = mod_root / "yaml"
    debug_dir = mod_root / "debug" / "terrain"

    from bacup_lib.terrain.fo76_btd import (
        TerrainConversionRequest,
        convert_fo76_btd_to_fo4_land,
    )

    report = convert_fo76_btd_to_fo4_land(
        TerrainConversionRequest(
            btd_path=str(btd),
            output_authoring_dir=str(yaml_dir),
            plugin_name="B21_AppalachiaHeightmap.esp",
            worldspace_editor_id="B21_AppalachiaHeightmap",
            source_min_x=0,
            source_min_y=0,
            source_max_x=0,
            source_max_y=0,
            debug_output_dir=str(debug_dir),
            emit_textures=False,
            export_heightmap=True,
        )
    )

    heightmap_path = Path(report["heightmap_output_path"])
    bytes_ = heightmap_path.read_bytes()
    assert bytes_[0:4] == b"DDS "
    assert _u32_at(bytes_, 8) == 0x0002100F
    assert _u32_at(bytes_, 12) == 33
    assert _u32_at(bytes_, 16) == 33
    assert _u32_at(bytes_, 20) == 33 * 4
    assert _u32_at(bytes_, 24) == 1
    assert _u32_at(bytes_, 28) == 1
    assert bytes_[32:76] == bytes(44)
    assert _u32_at(bytes_, 80) == 0x4
    assert _u32_at(bytes_, 84) == 0x72
    assert len(bytes_) == 128 + 33 * 33 * 4
    expected_heights = _north_up_btd_sample4_float_values(btd)
    expected_height_min = min(expected_heights)
    expected_height_max = max(expected_heights)
    expected_height_range = expected_height_max - expected_height_min
    expected_normalized_heights = [
        (value - expected_height_min) / expected_height_range
        for value in expected_heights
    ]
    assert min(_dds_float_values(bytes_)) == pytest.approx(0.0, abs=1e-6)
    assert max(_dds_float_values(bytes_)) == pytest.approx(1.0, abs=1e-6)
    assert _dds_float_values(bytes_) == pytest.approx(
        expected_normalized_heights,
        abs=1e-6,
    )
    preview_path = Path(report["heightmap_preview_path"])
    preview = preview_path.read_bytes()
    assert preview[0:2] == b"BM"
    assert _u32_at(preview, 10) == 14 + 40 + 256 * 4
    assert _u32_at(preview, 18) == 33
    assert _u32_at(preview, 22) == 33
    assert int.from_bytes(preview[28:30], "little") == 8
    stats_path = Path(report["heightmap_stats_path"])
    assert stats_path.read_bytes() == (
        f"Worldspace: B21_AppalachiaHeightmap [800]\r\n"
        f"Max height: {expected_height_max:.6f}\r\n"
        f"Min height: {expected_height_min:.6f}\r\n"
    ).encode("utf-8")


@pytest.mark.integration
def test_m1_heightmap_export_writes_cell_0_0_r32_float_dds(tmp_path):
    fo76_data = os.environ.get("FO76_DATA")
    if not fo76_data:
        pytest.skip("requires FO76_DATA")
    btd = Path(fo76_data) / "Terrain" / "Appalachia.btd"
    if not btd.is_file():
        pytest.skip("requires FO76_DATA/Terrain/Appalachia.btd")

    mod_root = tmp_path / "B21_AppalachiaCellHeightmap"
    yaml_dir = mod_root / "yaml"
    debug_dir = mod_root / "debug" / "terrain"

    from bacup_lib.terrain.fo76_btd import (
        TerrainConversionRequest,
        convert_fo76_btd_to_fo4_land,
    )

    report = convert_fo76_btd_to_fo4_land(
        TerrainConversionRequest(
            btd_path=str(btd),
            output_authoring_dir=str(yaml_dir),
            plugin_name="B21_AppalachiaCellHeightmap.esp",
            worldspace_editor_id="B21_AppalachiaCellHeightmap",
            source_min_x=0,
            source_min_y=0,
            source_max_x=1,
            source_max_y=1,
            debug_output_dir=str(debug_dir),
            emit_textures=False,
            export_heightmap=True,
        )
    )

    full_bytes = Path(report["heightmap_output_path"]).read_bytes()
    assert _u32_at(full_bytes, 12) == 65
    assert _u32_at(full_bytes, 16) == 65

    cell_path = Path(report["heightmap_cell_0_0_output_path"])
    assert cell_path.name == "B21_AppalachiaCellHeightmap_Cell_XP000_YP000_heightmap_r32f.dds"
    cell_bytes = cell_path.read_bytes()
    assert _u32_at(cell_bytes, 8) == 0x0002100F
    assert _u32_at(cell_bytes, 12) == 33
    assert _u32_at(cell_bytes, 16) == 33
    assert _u32_at(cell_bytes, 20) == 33 * 4
    assert _u32_at(cell_bytes, 24) == 1
    assert _u32_at(cell_bytes, 28) == 1
    assert cell_bytes[32:76] == bytes(44)
    assert _u32_at(cell_bytes, 80) == 0x4
    assert _u32_at(cell_bytes, 84) == 0x72
    assert len(cell_bytes) == 128 + 33 * 33 * 4

    expected_heights = _north_up_btd_sample4_cell_0_0_values_from_two_by_two_grid(btd)
    expected_height_min = min(expected_heights)
    expected_height_max = max(expected_heights)
    expected_height_range = expected_height_max - expected_height_min
    expected_normalized_heights = [
        (value - expected_height_min) / expected_height_range
        for value in expected_heights
    ]
    assert _dds_float_values(cell_bytes) == pytest.approx(
        expected_normalized_heights,
        abs=1e-6,
    )

    cell_stats_path = Path(report["heightmap_cell_0_0_stats_path"])
    assert cell_stats_path.read_bytes() == (
        f"Worldspace: B21_AppalachiaCellHeightmap [800]\r\n"
        f"Max height: {expected_height_max:.6f}\r\n"
        f"Min height: {expected_height_min:.6f}\r\n"
    ).encode("utf-8")


def _fo76_records_db(repo_root: Path) -> Path:
    override = os.environ.get("FO76_RECORDS_DB")
    if override:
        return Path(override)
    for root in (repo_root, *repo_root.parents):
        candidate = root / "data" / "fo76_records.db"
        if candidate.is_file():
            return candidate
    return repo_root / "data" / "fo76_records.db"


def _u32_at(bytes_: bytes, offset: int) -> int:
    return int.from_bytes(bytes_[offset : offset + 4], "little")


def _dds_float_values(bytes_: bytes) -> list[float]:
    return [
        struct.unpack_from("<f", bytes_, 128 + index * 4)[0]
        for index in range((len(bytes_) - 128) // 4)
    ]


def _north_up_btd_sample4_float_values(btd: Path) -> list[float]:
    return _north_up_btd_sample4_cell_values(btd, cells_x=1, cells_y=1)


def _north_up_btd_sample4_cell_0_0_values_from_two_by_two_grid(btd: Path) -> list[float]:
    return _north_up_btd_sample4_cell_values(btd, cells_x=2, cells_y=2)


def _north_up_btd_sample4_cell_values(
    btd: Path, *, cells_x: int, cells_y: int
) -> list[float]:
    from bacup_lib.terrain import native_runtime

    header = native_runtime.read_btd_header(str(btd))
    height_min = _f32(header["world_height_min"])
    height_max = _f32(header["world_height_max"])
    height_scale = _f32(_f32(height_max - height_min) / _f32(65535.0))

    cell_values: dict[tuple[int, int], list[float]] = {}

    def get_cell_values(cell_x: int, cell_y: int) -> list[float]:
        key = (cell_x, cell_y)
        if key not in cell_values:
            samples = native_runtime.probe_btd_cell(str(btd), cell_x, cell_y, lod=0)
            cell_values[key] = [
                _f32(height_min + _f32(_f32(sample) * height_scale))
                for sample in samples
            ]
        return cell_values[key]

    row_width = 128
    source_width = cells_x * row_width
    source_height = cells_y * row_width
    north_up_values: list[float] = []
    for target_y in range(32, -1, -1):
        source_y = min(target_y * 4, source_height - 1)
        cell_y = source_y // row_width
        local_y = source_y % row_width
        row_start = local_y * row_width
        for target_x in range(33):
            source_x = min(target_x * 4, source_width - 1)
            cell_x = source_x // row_width
            local_x = source_x % row_width
            north_up_values.append(get_cell_values(cell_x, cell_y)[row_start + local_x])
    return north_up_values


def _f32(value: float) -> float:
    return struct.unpack("<f", struct.pack("<f", value))[0]
