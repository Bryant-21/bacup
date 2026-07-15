from __future__ import annotations

from dataclasses import dataclass
import json
from pathlib import Path
from typing import Any


@dataclass(frozen=True, slots=True)
class TerrainConversionRequest:
    btd_path: str
    output_authoring_dir: str
    plugin_name: str
    worldspace_editor_id: str
    source_min_x: int
    source_min_y: int
    source_max_x: int
    source_max_y: int
    resample_mode: str = "lanczos"
    debug_output_dir: str | None = None
    records_db: str | None = None
    fo76_data_dir: str | None = None
    source_extracted_dir: str | None = None
    source_worldspace_authoring_dir: str | None = None
    emit_textures: bool = True
    export_heightmap: bool = False
    preserve_source_ids: bool = True
    water_manifest_path: str | None = None
    heightmap_output_path: str | None = None
    btd4_output_path: str | None = None
    populate_grass_assets: bool = True
    convert_grass_assets: bool = True
    build_esp: bool = False
    esp_output_path: str | None = None
    source_plugin_path: str | None = None
    source_game: str = "fo76"


def _terrain_native() -> Any:
    from creation_lib._native import terrain_native

    return terrain_native


def _string_or_empty(value: str | None) -> str:
    return value or ""


def write_fo76_water_manifest(
    *,
    source_worldspace_authoring_dir: str | None,
    output_manifest_path: str | None,
    source_min_x: int,
    source_min_y: int,
    source_max_x: int,
    source_max_y: int,
) -> str:
    if not source_worldspace_authoring_dir or not output_manifest_path:
        return ""
    options = {
        "source_worldspace_authoring_dir": str(source_worldspace_authoring_dir),
        "output_manifest_path": str(output_manifest_path),
        "source_min_x": int(source_min_x),
        "source_min_y": int(source_min_y),
        "source_max_x": int(source_max_x),
        "source_max_y": int(source_max_y),
    }
    return str(_terrain_native().write_water_manifest(json.dumps(options)))


def convert_fo76_btd_to_fo4_land(request: TerrainConversionRequest) -> dict[str, Any]:
    if not request.source_plugin_path:
        raise ValueError("source_plugin_path is required for FO76 terrain conversion")
    if not request.fo76_data_dir:
        raise ValueError("fo76_data_dir is required for FO76 terrain conversion fallback reads")

    water_manifest_path = _string_or_empty(request.water_manifest_path)
    if water_manifest_path:
        write_fo76_water_manifest(
            source_worldspace_authoring_dir=request.source_worldspace_authoring_dir,
            output_manifest_path=water_manifest_path,
            source_min_x=request.source_min_x,
            source_min_y=request.source_min_y,
            source_max_x=request.source_max_x,
            source_max_y=request.source_max_y,
        )
    options = {
        "source_game": request.source_game,
        "source_plugin_path": request.source_plugin_path,
        "fo76_data_dir": request.fo76_data_dir,
        "source_extracted_dir": _string_or_empty(request.source_extracted_dir),
        "btd_path": request.btd_path,
        "output_authoring_dir": request.output_authoring_dir,
        "plugin_name": request.plugin_name,
        "worldspace_editor_id": request.worldspace_editor_id,
        "source_min_x": int(request.source_min_x),
        "source_min_y": int(request.source_min_y),
        "source_max_x": int(request.source_max_x),
        "source_max_y": int(request.source_max_y),
        "resample_mode": request.resample_mode,
        "btd4_output_path": _string_or_empty(request.btd4_output_path),
        "debug_output_dir": _string_or_empty(request.debug_output_dir),
        "water_manifest_path": water_manifest_path,
        "populate_grass_assets": bool(request.populate_grass_assets),
        "emit_textures": bool(request.emit_textures),
        "export_heightmap": bool(request.export_heightmap),
        "preserve_source_ids": bool(request.preserve_source_ids),
        "source_worldspace_authoring_dir": _string_or_empty(request.source_worldspace_authoring_dir),
        "heightmap_output_path": _string_or_empty(request.heightmap_output_path),
        "convert_grass_assets": bool(request.convert_grass_assets),
    }

    from bacup_lib.native_runtime import load_native_module

    conversion_native = load_native_module()
    report = json.loads(conversion_native.conversion_terrain_with_textures(json.dumps(options)))

    if request.build_esp:
        from creation_lib.esp import build_authoring_dir

        esp_output_path = request.esp_output_path or str(
            Path(request.output_authoring_dir).parent / request.plugin_name
        )
        build_authoring_dir(request.output_authoring_dir, esp_output_path, game="fo4")
        report["esp_output_path"] = esp_output_path

        # The synthesized WRLD is a bare EDID/NAMA/DATA/NAM0/NAM9 skeleton; carry
        # the FO76 source worldspace header (land data, map image, map frame, and
        # any climate/water/location links present in the output) so the FO4
        # renderer/map activate. Same carry the placed-records and full
        # plugin-port paths use, keeping all three regen modes unified.
        if request.source_plugin_path:
            from creation_lib.esp import Plugin
            from bacup_lib.worldspace_services import (
                patch_target_worldspace_subrecords,
            )
            with Plugin.load(request.source_plugin_path, game=request.source_game) as source_plugin:
                report["worldspace_header_copied"] = patch_target_worldspace_subrecords(
                    source_plugin=source_plugin,
                    target_plugin_path=Path(esp_output_path),
                    worldspace_editor_id=request.worldspace_editor_id,
                    target_game="fo4",
                )

    return report
