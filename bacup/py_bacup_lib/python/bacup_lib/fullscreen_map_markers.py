"""FO76 map marker icons for the FullScreenMap Appalachia pack, built from the player's own files.

FullScreenMap ships only FO4 marker icons, and conversion folds FO76's marker types into FO4's.
This gives every FO76 map marker its FO76 symbol (REFR TNAM, named by the FO76 schema's type
labels, plus camps FO76 files only as generic door or quest markers) and renders each symbol from
the player's mapmarkerlibrary.swf into a pack atlas in the shipped atlas' format: a white shape
cell plus four band masks the map tints with the HUD palette.
"""

from __future__ import annotations

import io
import json
import math
import struct
from pathlib import Path
from typing import Any

from bacup_lib.fullscreen_map import PLUGIN_ASSETS_REL

PACK_REL = PLUGIN_ASSETS_REL / "maps" / "appalachia"
MARKER_LIBRARY_REL = Path("interface/mapmarkerlibrary.swf")
_CELL = 128
_COLUMNS = 24
_ICON_SIZE = 112
_RENDER_SCALE = 6.0
# Luma bounds of the four tint bands (shadow, dim, mid, bright), as the map's own atlas tool splits them.
_BAND_LIMITS = (0.12, 0.68, 0.93)

# Markers FO76 records only with a generic type (door, quest, event...) whose own symbol exists.
_SYMBOL_OVERRIDES: dict[int, str] = {
    0x0009A0CA: "CultistMarker", 0x000616AC: "SpaceStationMarker", 0x000616B5: "CultistMarker",
    0x0013E278: "BloodEagleMarker", 0x002C7635: "HammerWingMarker", 0x00356BEF: "BloodEagleMarker",
    0x00356BF6: "BloodEagleMarker", 0x00356BFC: "BloodEagleMarker", 0x00356C16: "BloodEagleMarker",
    0x00356C2A: "BloodEagleMarker", 0x00356C2C: "BloodEagleMarker", 0x0039199A: "BloodEagleMarker",
    0x003919B1: "BloodEagleMarker", 0x003919CF: "CultistMarker", 0x003919E6: "BloodEagleMarker",
    0x003919F1: "BloodEagleMarker", 0x003942AC: "CultistMarker", 0x003A58AC: "GleamingDepthsMarker",
    0x004F55B2: "BloodEagleMarker", 0x00501FB3: "BloodEagleMarker", 0x00553505: "BloodEagleMarker",
    0x00586D29: "BloodEagleMarker", 0x00586D2B: "BloodEagleMarker", 0x00586D2F: "CultistMarker",
    0x00586D32: "CultistMarker", 0x00590275: "CultistMarker", 0x0058D8BE: "Vault79Marker",
    0x005E9578: "Vault51Marker", 0x0072630D: "CultistMarker", 0x007E808C: "HighwayTownMarker",
}


def marker_symbols(source_plugin: Path) -> dict[int, str]:
    """FO76 map marker object ID -> marker symbol name."""
    from creation_lib.esp import native_runtime
    from creation_lib.esp.plugin import Plugin
    from creation_lib.esp.schema import get_schema

    labels = dict(get_schema("fo76").enums["REFR.TNAM.type"].labels)
    plugin = Plugin.load(Path(source_plugin), game="fo76", lazy_index=True)
    try:
        handle = plugin._rust_handle
        symbols: dict[int, str] = {}
        for form_id in native_runtime.plugin_handle_record_form_ids_with_subrecords(handle, ["XMRK"]):
            for signature, data, _semantic in native_runtime.plugin_handle_record_subrecords(handle, form_id) or []:
                if signature == "TNAM" and len(data) >= 2:
                    label = labels.get(struct.unpack_from("<H", data)[0])
                    if label:
                        symbols[form_id & 0xFFFFFF] = label
                    break
    finally:
        plugin.close()
    symbols.update(_SYMBOL_OVERRIDES)
    return symbols


def _band(luma: int) -> int:
    value = luma / 255
    return sum(value >= limit for limit in _BAND_LIMITS)


def icon_cells(icon: Any) -> list[Any]:
    """The shape cell (white silhouette) and four band masks for one rendered icon, each _CELL square."""
    from PIL import Image, ImageChops

    icon = icon.convert("RGBA")
    icon.thumbnail((_ICON_SIZE, _ICON_SIZE), Image.Resampling.LANCZOS)
    placed = Image.new("RGBA", (_CELL, _CELL))
    placed.alpha_composite(icon, ((_CELL - icon.width) // 2, (_CELL - icon.height) // 2))
    alpha = placed.getchannel("A")
    luma = placed.convert("RGB").convert("L")

    def white(mask: Any) -> Any:
        cell = Image.new("RGBA", placed.size, (255, 255, 255, 0))
        cell.putalpha(mask)
        return cell

    cells = [white(alpha)]
    for band in range(4):
        selected = luma.point(lambda value, band=band: 255 if _band(value) == band else 0)
        cells.append(white(ImageChops.multiply(selected, alpha)))
    return cells


def build_atlas(icons: dict[str, Any]) -> tuple[Any, dict[str, Any]]:
    """Pack rendered icons into the FullScreenMap atlas layout; returns (image, atlas.json manifest)."""
    from PIL import Image

    names = sorted(icons)
    rows = max(1, math.ceil(len(names) * 5 / _COLUMNS))
    atlas = Image.new("RGBA", (_COLUMNS * _CELL, rows * _CELL))
    width, height = atlas.size

    def rect(slot: int) -> list[float]:
        x, y = slot % _COLUMNS, slot // _COLUMNS
        return [x * _CELL / width, y * _CELL / height, (x + 1) * _CELL / width, (y + 1) * _CELL / height]

    entries: dict[str, Any] = {}
    slot = 0
    for name in names:
        for offset, cell in enumerate(icon_cells(icons[name])):
            atlas.alpha_composite(cell, ((slot + offset) % _COLUMNS * _CELL, (slot + offset) // _COLUMNS * _CELL))
        entries[name] = {"tintable": True, "shape": rect(slot), "bands": [rect(slot + band) for band in range(1, 5)]}
        slot += 5
    manifest = {"version": 1, "texture": "markers.dds", "width": width, "height": height, "cell": _CELL, "icons": entries}
    return atlas, manifest


def render_symbols(library: bytes, names: set[str]) -> tuple[dict[str, Any], list[str]]:
    """Render marker symbols from mapmarkerlibrary.swf; returns (images by name, names that failed)."""
    from PIL import Image
    from creation_lib.swf import native_runtime

    rendered: dict[str, Any] = {}
    failed: list[str] = []
    for name in sorted(names):
        try:
            png = native_runtime.render_symbol_png(library, name, 1, _RENDER_SCALE)
        except ValueError:
            failed.append(name)
            continue
        rendered[name] = Image.open(io.BytesIO(png)).convert("RGBA")
    return rendered, failed


def export_marker_icons(*, source_data_dir: Path, source_plugin: Path, mod_root: Path) -> dict[str, Any]:
    """Write maps/appalachia/markers.json and icons/{atlas.json,markers.dds} into the converted mod."""
    from bacup_lib.fullscreen_map import write_ui_dds

    symbols = marker_symbols(source_plugin)
    library = (Path(source_data_dir) / MARKER_LIBRARY_REL).read_bytes()
    rendered, failed = render_symbols(library, set(symbols.values()))
    atlas, manifest = build_atlas(rendered)

    pack = Path(mod_root) / PACK_REL
    icons_dir = pack / "icons"
    icons_dir.mkdir(parents=True, exist_ok=True)
    write_ui_dds(atlas, icons_dir / "markers.dds")
    (icons_dir / "atlas.json").write_text(json.dumps(manifest, indent=1) + "\n", encoding="utf-8")
    markers = [{"id": f"{form_id:06X}", "sym": symbol} for form_id, symbol in sorted(symbols.items()) if symbol in rendered]
    (pack / "markers.json").write_text(json.dumps({"version": 1, "markers": markers}) + "\n", encoding="utf-8")
    return {"markers": len(markers), "icons": len(rendered), "failed_symbols": failed}
