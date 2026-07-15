"""Phase: convert terrain."""

from __future__ import annotations

import json
from dataclasses import replace
from pathlib import Path
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from bacup_lib.models import ConversionContext, PhaseProgress
    from bacup_lib.runner import ConversionRunner


_SUPPORTED_SOURCES: set[str] = {"fo76"}

def convert_terrain(
    ctx: "ConversionContext",
    runner: "ConversionRunner",
    progress: "PhaseProgress",
) -> None:
    """Dispatch terrain conversion by source game."""
    source = (ctx.source_game or "").lower()
    if source == "fnv":
        runner.emit_log(
            "WARN",
            "convert_terrain: FNV LAND is emitted by Rust translate_records; "
            "legacy Python terrain phase has been removed",
        )
        progress.total_items = 0
        progress.completed_items = 0
        return
    if source not in _SUPPORTED_SOURCES:
        runner.emit_log(
            "WARN",
            f"convert_terrain: not implemented for source game '{source}', skipping",
        )
        progress.total_items = 0
        progress.completed_items = 0
        return

    _run_fo76_btd(ctx, runner, progress)


def _run_fo76_btd(ctx, runner, progress) -> None:
    try:
        work_items = fo76_btd_work_items(ctx)
    except FileNotFoundError as exc:
        runner.emit_log("ERROR", str(exc))
        progress.status = "error"
        raise
    if not work_items:
        runner.emit_log(
            "WARN",
            "convert_terrain: FO76 source but no BTD path configured; skipping",
        )
        progress.total_items = 0
        progress.completed_items = 0
        return

    plugin_name = ctx.output_plugin_name

    rust_run = getattr(ctx, "_rust_conversion_run", None)
    if rust_run is None:
        runner.emit_log(
            "ERROR",
            "convert_terrain: FO76 BTD conversion requires a native ConversionRun context",
        )
        progress.status = "error"
        return
    progress.total_items = len(work_items)
    progress.completed_items = 0
    for index, (opts, btd_path, worldspace_eid) in enumerate(work_items, start=1):
        _run_fo76_btd_native(
            ctx, runner, progress, opts, plugin_name, worldspace_eid, rust_run, btd_path
        )
        if progress.status == "error":
            return
        progress.total_items = len(work_items)
        progress.completed_items = index
        progress.status = "completed"
        runner.emit_item_progress(progress)


def fo76_btd_work_items(ctx) -> list[tuple[object, Path, str]]:
    opts = ctx.terrain_options
    if opts is None:
        return []
    btd_paths = _require_fo76_btd_paths(ctx, opts)
    if not btd_paths:
        return []

    items: list[tuple[object, Path, str]] = []
    plugin_stem = ctx.output_plugin_name.rsplit(".", 1)[0]
    for btd_path in btd_paths:
        worldspace_eid = _fo76_worldspace_editor_id(opts, btd_path, plugin_stem)
        source_worldspace_eid = (
            getattr(opts, "source_worldspace_editor_id", "") or worldspace_eid
        )
        item_opts = replace(
            opts,
            btd_path=str(btd_path),
            worldspace_editor_id=worldspace_eid,
            source_worldspace_editor_id=source_worldspace_eid,
        )
        items.append((item_opts, btd_path, worldspace_eid))
    return items


def _fo76_worldspace_editor_id(opts, btd_path: Path, plugin_stem: str) -> str:
    configured = getattr(opts, "worldspace_editor_id", "")
    if configured:
        return configured
    from bacup_lib.fo76_sources import fo76_worldspace_editor_id_from_btd

    return fo76_worldspace_editor_id_from_btd(btd_path) or plugin_stem


def _require_fo76_btd_paths(ctx, opts) -> tuple[Path, ...]:
    from bacup_lib.fo76_sources import (
        require_any_resolved,
        resolve_fo76_btd_paths,
    )

    configured = getattr(opts, "btd_path", "") if opts is not None else ""
    if configured:
        path = Path(configured)
        if path.is_file():
            return (path,)
        raise FileNotFoundError(f"FO76 BTD path does not exist: {path}")

    return require_any_resolved(
        resolve_fo76_btd_paths(
            source_data_dir=getattr(ctx, "source_data_dir", None),
            data_dir=getattr(opts, "fo76_data_dir", "") or None,
        ),
        label="FO76 BTD files",
    )


def _run_fo76_btd_native(
    ctx, runner, progress, opts, plugin_name, worldspace_eid, rust_run, btd_path: Path
) -> None:
    fo76_data_dir = str(getattr(opts, "fo76_data_dir", "") or "")
    if not fo76_data_dir:
        runner.emit_log(
            "ERROR",
            "convert_terrain: FO76 terrain conversion requires TerrainOptions.fo76_data_dir for installed FO76 Data fallback reads",
        )
        progress.status = "error"
        return

    source_extracted_dir = str(
        getattr(opts, "source_extracted_dir", "")
        or getattr(ctx, "source_extracted_dir", "")
        or getattr(ctx, "source_data_dir", "")
        or ""
    )
    shared_asset_conversion = bool(
        getattr(ctx, "shared_asset_conversion_enabled", False)
    )
    diagnostics_root = Path(getattr(ctx, "diagnostics_root", None) or ctx.mod_path)
    debug_output_dir = diagnostics_root / "debug" / "terrain"
    source_worldspace_authoring_dir = opts.source_worldspace_authoring_dir or ""
    water_manifest_path = getattr(opts, "water_manifest_path", "") or ""
    can_auto_write_water = _can_auto_write_water_manifest(source_worldspace_authoring_dir)
    if not water_manifest_path:
        water_manifest_path = str(debug_output_dir / "water_manifest.json")
    if water_manifest_path and (can_auto_write_water or getattr(opts, "water_manifest_path", "")):
        from bacup_lib.terrain.fo76_btd import write_fo76_water_manifest

        write_fo76_water_manifest(
            source_worldspace_authoring_dir=source_worldspace_authoring_dir,
            output_manifest_path=water_manifest_path,
            source_min_x=0 if opts.source_min_x is None else int(opts.source_min_x),
            source_min_y=0 if opts.source_min_y is None else int(opts.source_min_y),
            source_max_x=-1 if opts.source_max_x is None else int(opts.source_max_x),
            source_max_y=-1 if opts.source_max_y is None else int(opts.source_max_y),
        )
    params = {
        "source_game": ctx.source_game,
        "fo76_data_dir": fo76_data_dir,
        "source_extracted_dir": source_extracted_dir,
        "btd_path": str(btd_path),
        "output_authoring_dir": str(ctx.mod_path / "yaml"),
        "plugin_name": plugin_name,
        "worldspace_editor_id": worldspace_eid,
        "source_min_x": (0 if opts.source_min_x is None else int(opts.source_min_x)),
        "source_min_y": (0 if opts.source_min_y is None else int(opts.source_min_y)),
        "source_max_x": (-1 if opts.source_max_x is None else int(opts.source_max_x)),
        "source_max_y": (-1 if opts.source_max_y is None else int(opts.source_max_y)),
        "resample_mode": opts.resample_mode,
        "btd4_output_path": str(ctx.mod_path / "Terrain" / f"{worldspace_eid}.btd4") if getattr(opts, "emit_btd4", True) else "",
        "debug_output_dir": str(debug_output_dir),
        "emit_textures": opts.emit_textures,
        "write_materials": not shared_asset_conversion,
        "populate_grass_assets": bool(opts.emit_textures),
        "convert_grass_assets": not shared_asset_conversion,
        "export_heightmap": opts.export_heightmap,
        "debug_flat_land": bool(getattr(opts, "debug_flat_land", False)),
        "preserve_source_ids": bool(getattr(ctx, "preserve_source_ids", True)),
        "conversion_workers": getattr(ctx, "conversion_workers", None),
        "water_manifest_path": water_manifest_path,
        "source_worldspace_authoring_dir": opts.source_worldspace_authoring_dir or "",
        "heightmap_output_path": "",
    }
    if shared_asset_conversion and params["populate_grass_assets"]:
        runner.emit_log(
            "INFO",
            "convert_terrain: grass asset conversion delegated to shared asset phases",
        )
    runner.emit_log(
        "INFO",
        "convert_terrain: running FO76 BTD -> FO4 LAND (native) for "
        f"{worldspace_eid} in {plugin_name}",
    )
    report = rust_run.run_phase(
        "convert_terrain",
        mod_path=str(ctx.mod_path),
        source_extracted_dir=source_extracted_dir,
        params=params,
    )
    timing_path = Path(params["debug_output_dir"]) / "terrain_timing.json"
    if timing_path.is_file():
        runner.emit_log("INFO", f"convert_terrain: timing_report={timing_path}")
    if shared_asset_conversion:
        added = _append_grass_assets_from_manifest(
            ctx,
            Path(params["debug_output_dir"]) / "texture_manifest.json",
        )
        if added:
            runner.emit_log(
                "INFO",
                f"convert_terrain: queued {added} grass asset refs for shared asset phases",
            )
    runner.emit_log(
        "INFO",
        f"convert_terrain: records_added={report.get('records_added', 0)}",
    )
    progress.total_items = 100
    progress.completed_items = 100
    progress.status = "completed"
    runner.emit_item_progress(progress)


def _can_auto_write_water_manifest(source_worldspace_authoring_dir: str) -> bool:
    if not source_worldspace_authoring_dir:
        return False
    path = Path(source_worldspace_authoring_dir)
    return path.is_dir() and path.parent.name.upper() == "WRLD"


def _append_grass_assets_from_manifest(ctx, manifest_path: Path) -> int:
    if not manifest_path.is_file():
        return 0

    assets = getattr(ctx, "assets", None)
    if not isinstance(assets, list):
        assets = []
        ctx.assets = assets

    from bacup_lib.models import AssetRef

    seen = {
        (str(asset.asset_type).lower(), _asset_key(str(asset.source_path)))
        for asset in assets
    }
    try:
        manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError):
        return 0

    added = 0
    for bundle in manifest.get("textures") or []:
        for grass in bundle.get("grass") or []:
            for item in grass.get("assets") or []:
                asset_type = str(item.get("asset_type") or "").strip()
                source_path = str(item.get("source_path") or "").strip()
                if not asset_type or not source_path:
                    continue
                key = (asset_type.lower(), _asset_key(source_path))
                if key in seen:
                    continue
                seen.add(key)
                assets.append(
                    AssetRef(
                        asset_type=asset_type,
                        source_path=source_path.replace("\\", "/"),
                        resolved_path=str(item.get("resolved_path") or "") or None,
                    )
                )
                added += 1
    return added


def _asset_key(source_path: str) -> str:
    return source_path.replace("\\", "/").strip().lower()
