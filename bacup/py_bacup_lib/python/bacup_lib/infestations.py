"""Build the Tales Infestation catalog and dark-fog map art from the user's FO76 install."""
from __future__ import annotations

import hashlib
import io
import json
import math
import struct
import zlib
from collections import Counter
from pathlib import Path
from typing import Iterable

from bacup_lib.native_runtime import load_native_module
from bacup_lib.plugin_catalog import catalog_path, form_key, write_document

SCHEMA_VERSION = 1
FEATURE = "Infestations"
PLUGIN = "SeventySix.esm"
CATALOG = catalog_path(FEATURE, PLUGIN)
OUTPUT = Path("F4SE/Plugins/B21_FullScreenMap/infestations")
LEGACY_OUTPUT = Path("PrismaUI_F4/views/B21_FullScreenMap/tales/infestations")
MAP_MOVIE = Path("interface/mapmenu.swf")
FOG_SYMBOL = "MapCloud"
RADIUS_CLIP = "RadiusCheck_mc"


def png(width: int, height: int, rgba: bytes) -> bytes:
    def chunk(kind: bytes, payload: bytes) -> bytes:
        return struct.pack(">I", len(payload)) + kind + payload + struct.pack(">I", zlib.crc32(kind + payload))
    rows = b"".join(b"\0" + rgba[y * width * 4:(y + 1) * width * 4] for y in range(height))
    return (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0)) +
            chunk(b"IDAT", zlib.compress(rows, 9)) + chunk(b"IEND", b""))


def lossless_png(payload: bytes) -> tuple[int, int, int, bytes]:
    """Decode a DefineBitsLossless2 32-bit bitmap (premultiplied ARGB) to a straight-alpha PNG."""
    character, kind, width, height = struct.unpack_from("<HBHH", payload, 0)
    if kind != 5 or not width or not height:
        raise ValueError(f"Bitmap {character} uses unsupported lossless format {kind}")
    pixels = zlib.decompress(payload[7:])
    if len(pixels) != width * height * 4:
        raise ValueError(f"Bitmap {character} has {len(pixels)} bytes for {width}x{height}")
    out = bytearray(len(pixels))
    for i in range(0, len(pixels), 4):
        alpha = pixels[i]
        out[i + 3] = alpha
        if alpha:
            for channel in range(3):
                out[i + channel] = min(255, (pixels[i + 1 + channel] * 255 + alpha // 2) // alpha)
    return character, width, height, png(width, height, bytes(out))


def matrix_scale(matrix) -> float:
    return math.hypot(matrix.scale_x, matrix.rotate_skew_0) if matrix else 1.0


def place_object3(data: bytes) -> tuple[int, int, object] | None:
    """Decode the depth, character and matrix of a PlaceObject3 the SWF parser leaves raw."""
    from creation_lib.swf.types import MATRIX, BitReader

    reader = BitReader(data)
    flags, extra, depth = reader.read_ui8(), reader.read_ui8(), reader.read_ui16()
    has_character, has_matrix = flags & 0x02, flags & 0x04
    if extra & 0x08 or (extra & 0x10 and has_character):
        reader.read_string()
    if not has_character:
        return None
    character = reader.read_ui16()
    return depth, character, MATRIX.parse(reader) if has_matrix else None


def placements(doc, sprite_id: int) -> dict[int, tuple[int, object]]:
    """First character placed at each depth anywhere in the sprite's loop."""
    found: dict[int, tuple[int, object]] = {}
    for frame in doc.sprites[sprite_id].timeline.frames:
        for depth, entry in frame.placements.items():
            if entry.character_id is not None:
                found.setdefault(depth, (entry.character_id, entry.matrix))
    for tag in getattr(doc, "tags", ()):
        if getattr(tag, "sprite_id", None) != sprite_id:
            continue
        for nested in getattr(tag, "tags", ()):
            if getattr(nested, "tag_id", None) == 70 and (placed := place_object3(nested.data)):
                found.setdefault(placed[0], placed[1:])
    return found


def fog_layers(doc, bitmaps: dict[int, bytes]) -> dict:
    """Walk the MapCloud symbol to its bitmap-filled shapes, in stacking order (each depth at its first placement)."""
    symbol = next((character for character, name in doc.symbols if name == FOG_SYMBOL), None)
    if symbol not in doc.sprites:
        raise ValueError(f"{MAP_MOVIE} does not export {FOG_SYMBOL}")
    root = doc.sprites[symbol].timeline
    radius = None
    layers: dict[int, dict] = {}

    def extent(character: int) -> float | None:
        shape = doc.shapes.get(character)
        if shape:
            left, top, right, bottom = shape.bounds_px
            return max(right - left, bottom - top)
        sprite = doc.sprites.get(character)
        if not sprite or not sprite.timeline.frames:
            return None
        sizes = [extent(entry.character_id) for entry in sprite.timeline.frames[0].placements.values()]
        sizes = [size for size in sizes if size]
        return max(sizes) if sizes else None

    def walk(character: int, scale: float, frames: int, order: list[int]):
        shape = doc.shapes.get(character)
        if shape:
            left, top, right, bottom = shape.bounds_px
            for fill in shape.fill_styles:
                if fill.bitmap_id in bitmaps:
                    size = max(right - left, bottom - top) * scale
                    layer = layers.setdefault(fill.bitmap_id, {"order": order, "size": 0.0, "frames": frames})
                    layer["size"] = max(layer["size"], size)
                    layer["frames"] = max(layer["frames"], frames)
            return
        sprite = doc.sprites.get(character)
        if not sprite or not sprite.timeline.frames:
            return
        count = sprite.timeline.frame_count
        for depth, (child, matrix) in sorted(placements(doc, character).items()):
            walk(child, scale * matrix_scale(matrix), max(frames, count), order + [depth])

    for depth, entry in sorted(root.frames[0].placements.items()):
        if entry.name == RADIUS_CLIP:
            size = extent(entry.character_id)
            radius = size * matrix_scale(entry.matrix) / 2 if size else None
        else:
            walk(entry.character_id, matrix_scale(entry.matrix), 1, [depth])
    if not radius or not layers:
        raise ValueError(f"{FOG_SYMBOL} has no {RADIUS_CLIP} or no bitmap layers")
    ordered = sorted(layers.items(), key=lambda item: item[1]["order"])
    fps = float(getattr(doc.header, "fps", 30) or 30)
    return {"radius": radius, "layers": [
        {"bitmap": bitmap, "scale": round(layer["size"] / (2 * radius), 4), "period": round(layer["frames"] / fps, 3)}
        for bitmap, layer in ordered]}


def extract_fog(movie: bytes) -> tuple[dict, dict[str, bytes]]:
    from creation_lib.swf.parser import parse_swf

    doc = parse_swf(movie)
    raw = {}
    for tag in doc.tags:
        if getattr(tag, "tag_id", None) == 36 and len(getattr(tag, "data", b"")) > 7:
            raw[struct.unpack_from("<H", tag.data, 0)[0]] = tag.data
    layout = fog_layers(doc, raw)
    files, layers = {}, []
    for index, layer in enumerate(layout["layers"]):
        _, width, height, image = lossless_png(raw[layer["bitmap"]])
        name = f"fog{index}.png"
        files[name] = image
        layers.append({"file": name, "width": width, "height": height, "scale": layer["scale"],
                       "period": layer["period"], "sha256": hashlib.sha256(image).hexdigest()})
    return {"symbol": FOG_SYMBOL, "layers": layers}, files




def data_version(source_root: Path) -> str:
    info = Path(source_root) / "buildinfo.txt"
    if not info.is_file():
        return ""
    for line in info.read_text(encoding="utf-8", errors="replace").splitlines():
        key, _, value = line.partition("=")
        if key.strip() == "data_version":
            return value.strip()
    return ""


def to_document(catalog: dict) -> dict:
    """Tales layout: tuning and form roles in `settings`; sites (SeventySix.esm only), factions, regions and
    boss mutations as keyed sections. The native catalog's bare hex ids are relative to its plugin."""
    plugin = catalog.get("plugin", PLUGIN)

    def key(value):
        return form_key(plugin, int(value, 16)) if isinstance(value, str) and value else value

    def keyed(rows, *fields):
        return [{**row, **{field: key(row[field]) for field in fields if field in row}} for row in rows]

    settings = {name: catalog[name] for name in ("plugin", "tuning", "fog_sha256", "skipped") if name in catalog}
    if "source" in catalog:
        settings["source"] = {**catalog["source"], **({"quest": key(catalog["source"]["quest"])} if "quest" in catalog["source"] else {})}
    for name in ("forms", "reward"):
        if name in catalog:
            settings[name] = {field: key(value) for field, value in catalog[name].items()}
    document = {"version": catalog["version"], "settings": settings}
    if "sites" in catalog:
        sites = keyed(catalog["sites"], "location", "region", "world", "marker")
        document["sites"] = [{**row, "clear": [key(v) for v in row["clear"]], "boss": [key(v) for v in row["boss"]]} for row in sites]
    if "factions" in catalog:
        document["factions"] = keyed(catalog["factions"], "keyword", "mob", "boss")
    if "regions" in catalog:
        document["regions"] = keyed(catalog["regions"], "location", "toggle")
    if "mutations" in catalog:
        document["mutations"] = [{"form": key(value)} for value in catalog["mutations"]]
    return document


def write_bundle(output_mod: Path, catalog: dict, fog: dict, images: dict[str, bytes]) -> dict:
    from PIL import Image
    from bacup_lib.fullscreen_map import align_to_blocks, write_ui_dds

    bundle = Path(output_mod) / OUTPUT
    bundle.mkdir(parents=True, exist_ok=True)
    manifest = {"version": SCHEMA_VERSION, "fog": {"symbol": fog["symbol"], "layers": []}, "files": []}
    for layer in fog["layers"]:
        with Image.open(io.BytesIO(images[layer["file"]])) as image:
            pixels = align_to_blocks(image)
            destination = write_ui_dds(pixels, bundle / layer["file"])
        digest = hashlib.sha256(destination.read_bytes()).hexdigest()
        manifest["fog"]["layers"].append({**layer, "file": destination.name,
            "width": pixels.width, "height": pixels.height, "sha256": digest})
        manifest["files"].append({"path": (OUTPUT / destination.name).as_posix(), "sha256": digest})
    (bundle / "presentation.json").write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    write_document(FEATURE, PLUGIN, to_document(catalog), output_mod)
    remove_legacy_bundle(output_mod)
    return manifest


def remove_legacy_bundle(output_mod: Path) -> None:
    legacy = Path(output_mod) / LEGACY_OUTPUT
    for name in ("fog0.png", "fog1.png", "fog2.png", "presentation.json", "map-extension.js"):
        (legacy / name).unlink(missing_ok=True)
    for relative in (LEGACY_OUTPUT, *LEGACY_OUTPUT.parents[:-1]):
        try:
            (Path(output_mod) / relative).rmdir()
        except OSError:
            break


def build_catalog(source_plugin: Path, converted_plugin: Path, data_version: str = "") -> dict:
    return json.loads(load_native_module().build_infestation_catalog(
        str(source_plugin), str(converted_plugin), data_version,
    ))


def convert_infestations(source_root: Path, output_mod: Path, *, source_plugin: Path, converted_plugin: Path) -> dict:
    movie = Path(source_root) / MAP_MOVIE
    if not movie.is_file():
        raise FileNotFoundError(f"Infestation fog requires the extracted FO76 map movie: {movie}")
    fog, images = extract_fog(movie.read_bytes())
    catalog = build_catalog(source_plugin, converted_plugin, data_version(source_root))
    catalog["fog_sha256"] = [layer["sha256"] for layer in fog["layers"]]
    return {"catalog": catalog, "presentation": write_bundle(output_mod, catalog, fog, images)}


def summarize(catalog: dict) -> Iterable[str]:
    regions = Counter(site["region"] for site in catalog["sites"])
    yield f"{len(catalog['sites'])} sites, {len(catalog['factions'])} factions, {len(catalog['mutations'])} boss mutations"
    yield "sites by region: " + ", ".join(f"{row['key']}={regions[row['location']]}" for row in catalog["regions"])
    yield "skipped: " + ", ".join(f"{key}={value}" for key, value in catalog["skipped"].items())
