"""Build B21_FullScreenMap data packs from converted worldspace assets.

Legacy Fallout worldspaces normally provide a single WRLD ICON map texture.
Skyrim worldspaces instead provide terrain LOD tiles, so those are assembled into
a north-up image and receive an explicit cell-frame calibration.
"""

from __future__ import annotations

import json
import re
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Iterable


_VIEW_MAPS_REL = Path("PrismaUI_F4/views/B21_FullScreenMap/maps")
_TERRAIN_TILE_RE = re.compile(
    r"^(?P<world>.+)\.(?P<level>\d+)\.(?P<x>-?\d+)\.(?P<y>-?\d+)\.dds$",
    re.IGNORECASE,
)


def _safe_id(value: str) -> str:
    clean = re.sub(r"[^a-z0-9_-]+", "-", value.casefold()).strip("-")
    return clean or "worldspace"


def _decode_path(data: bytes) -> str:
    return data.split(b"\0", 1)[0].decode("cp1252", errors="replace").strip()


def _texture_path(data_root: Path, value: str) -> Path:
    normalized = value.replace("\\", "/").lstrip("/")
    if normalized.casefold().startswith("textures/"):
        normalized = normalized[len("textures/") :]
    return data_root / "Textures" / Path(normalized)


def _write_pack(
    *,
    mod_root: Path,
    worldspace: str,
    title: str,
    image: Any,
    calibration: dict[str, Any],
    discovery: str = "native",
    pack_id: str | None = None,
) -> Path:
    pack_dir = mod_root / _VIEW_MAPS_REL / (pack_id or _safe_id(worldspace))
    pack_dir.mkdir(parents=True, exist_ok=True)
    image_path = pack_dir / "map.png"
    image.save(image_path, format="PNG", optimize=True)
    manifest = {
        "worldspace": worldspace,
        "title": title or worldspace.upper(),
        "image": "map.png",
        "discovery": discovery,
        "calibration": calibration,
    }
    (pack_dir / "map.json").write_text(
        json.dumps(manifest, indent=2) + "\n", encoding="utf-8"
    )
    return pack_dir


def export_single_map(
    *,
    mod_root: Path,
    source_image: Path,
    worldspace: str,
    title: str = "",
    calibration: dict[str, Any] | None = None,
    discovery: str = "native",
    pack_id: str | None = None,
) -> Path:
    """Convert one source/converted texture into a complete map pack."""
    from creation_lib.dds.io import load_image

    image = load_image(str(source_image), mode="RGBA")
    try:
        return _write_pack(
            mod_root=mod_root,
            worldspace=worldspace,
            title=title,
            image=image,
            calibration=calibration or {"mode": "mnam"},
            discovery=discovery,
            pack_id=pack_id,
        )
    finally:
        image.close()


def _worldspace_icon_rows(plugin_path: Path, source_game: str) -> list[tuple[str, str, str]]:
    """Return (EditorID, title, ICON path) for source WRLD records."""
    from creation_lib.esp import native_runtime
    from creation_lib.esp.plugin import Plugin

    plugin = Plugin.load(plugin_path, game=source_game, lazy_index=True)
    try:
        rows: list[tuple[str, str, str]] = []
        handle = getattr(plugin, "_rust_handle", None)
        if handle is None:
            return rows
        # Native index rows are (form_key, editor_id, signature, local_id, raw_form_id).
        for _form_key, editor_id, _sig, form_id, _raw_form_id in plugin.record_index_rows(
            signatures=["WRLD"]
        ):
            if not editor_id:
                continue
            subrecords = native_runtime.plugin_handle_record_subrecords(handle, form_id) or []
            icon = ""
            title = ""
            for sig, data, _semantic in subrecords:
                if sig == "ICON" and not icon:
                    icon = _decode_path(data)
                elif sig == "FULL" and not title:
                    title = _decode_path(data)
            if icon:
                rows.append((str(editor_id), title, icon))
        return rows
    finally:
        plugin.close()


def export_legacy_world_maps(
    *,
    mod_root: Path,
    source_plugins: Iterable[Path],
    source_game: str,
) -> set[str]:
    """Export FNV/FO3 WRLD ICON textures already converted into the mod."""
    data_root = mod_root / "data"
    exported: set[str] = set()
    for plugin_path in source_plugins:
        if not Path(plugin_path).is_file():
            continue
        for editor_id, title, icon in _worldspace_icon_rows(Path(plugin_path), source_game):
            source_image = _texture_path(data_root, icon)
            if not source_image.is_file():
                continue
            export_single_map(
                mod_root=mod_root,
                source_image=source_image,
                worldspace=editor_id,
                title=title,
            )
            exported.add(editor_id.casefold())
    return exported


@dataclass(frozen=True)
class _TerrainTile:
    path: Path
    level: int
    x: int
    y: int


def _terrain_tiles(world_dir: Path) -> list[_TerrainTile]:
    candidates: list[_TerrainTile] = []
    for path in world_dir.glob("*.dds"):
        if path.stem.casefold().endswith("_msn"):
            continue
        match = _TERRAIN_TILE_RE.match(path.name)
        if not match:
            continue
        candidates.append(
            _TerrainTile(
                path=path,
                level=int(match.group("level")),
                x=int(match.group("x")),
                y=int(match.group("y")),
            )
        )
    if not candidates:
        return []
    coarsest = max(tile.level for tile in candidates)
    return [tile for tile in candidates if tile.level == coarsest]


def export_terrain_world_maps(*, mod_root: Path, skip_worldspaces: set[str] | None = None) -> set[str]:
    """Assemble Skyrim/legacy terrain LOD tiles into FullScreenMap packs."""
    from PIL import Image
    from creation_lib.dds.io import load_image

    terrain_root = mod_root / "data" / "Textures" / "terrain"
    if not terrain_root.is_dir():
        return set()
    skipped = skip_worldspaces or set()
    exported: set[str] = set()
    for world_dir in sorted((p for p in terrain_root.iterdir() if p.is_dir()), key=lambda p: p.name.casefold()):
        worldspace = world_dir.name
        if worldspace.casefold() in skipped:
            continue
        tiles = _terrain_tiles(world_dir)
        if not tiles:
            continue

        first = load_image(str(tiles[0].path), mode="RGBA")
        try:
            tile_w, tile_h = first.size
        finally:
            first.close()
        xs = {tile.x for tile in tiles}
        ys = {tile.y for tile in tiles}
        level = tiles[0].level
        min_x, max_x = min(xs), max(xs)
        min_y, max_y = min(ys), max(ys)
        columns = (max_x - min_x) // level + 1
        rows = (max_y - min_y) // level + 1
        mosaic = Image.new("RGBA", (columns * tile_w, rows * tile_h), (0, 0, 0, 255))
        try:
            for tile in tiles:
                image = load_image(str(tile.path), mode="RGBA")
                try:
                    if image.size != (tile_w, tile_h):
                        continue
                    column = (tile.x - min_x) // level
                    row = (max_y - tile.y) // level
                    mosaic.paste(image, (column * tile_w, row * tile_h))
                finally:
                    image.close()
            calibration = {
                "mode": "frame",
                "nwCellX": min_x,
                "nwCellY": max_y + level - 1,
                "seCellX": max_x + level - 1,
                "seCellY": min_y,
                "x0": 0,
                "y0": 0,
                "x1": mosaic.width,
                "y1": mosaic.height,
            }
            _write_pack(
                mod_root=mod_root,
                worldspace=worldspace,
                title=worldspace.upper(),
                image=mosaic,
                calibration=calibration,
            )
        finally:
            mosaic.close()
        exported.add(worldspace.casefold())
    return exported


def finalize_converted_world_maps(request: Any, ctx: Any) -> list[str]:
    """Create all supported FullScreenMap packs for one conversion output."""
    source_game = str(request.source_game).casefold()
    target_game = str(request.target_game).casefold()
    if target_game != "fo4" or source_game not in {"fo76", "fnv", "fo3", "skyrimse"}:
        return []
    if not request.options.convert_textures:
        return []

    mod_root = Path(ctx.mod_path)
    exported: set[str] = set()
    if source_game in {"fnv", "fo3"}:
        exported |= export_legacy_world_maps(
            mod_root=mod_root,
            source_plugins=(Path(path) for path in request.source_plugins),
            source_game=source_game,
        )
    if source_game in {"fnv", "fo3", "skyrimse"}:
        exported |= export_terrain_world_maps(mod_root=mod_root, skip_worldspaces=exported)
    return sorted(exported)
