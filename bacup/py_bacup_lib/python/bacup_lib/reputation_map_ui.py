from __future__ import annotations

import copy
import hashlib
import io
import json
import math
import struct
from pathlib import Path

from PIL import Image

from bacup_lib.fullscreen_map import PLUGIN_ASSETS_REL, write_ui_dds
from bacup_lib.legendary_perks_ui import swf_tags
from bacup_lib.quest_area_ui import symbol_ids, tag
from bacup_lib.status_hud_source import (
    inject_art, placement, replace_tags, resolve_class_placements, select_children, sprite_tags, states, transform,
)
from bacup_lib.ui_contract import resolve_numbered_name
from creation_lib.swf import native_runtime
from creation_lib.swf.tags import DefineShapeTag

OUTPUT = PLUGIN_ASSETS_REL / "reputation"
SOURCES = ("socialreputationwidget.swf", "reputationlibrary.swf")
ENTRY = "SocialReputationWidgetEntry"
SCALE = 2.0


def neutralize_emblem(data: bytes) -> bytes:
    with Image.open(io.BytesIO(data)) as image:
        red, green, blue, alpha = image.convert("RGBA").split()
        # Match the quest overlay's cream-to-HUD transform, retaining source alpha and black outlines.
        blue = blue.point([min(255, round(value * 255 / 203)) for value in range(256)])
        result = io.BytesIO()
        Image.merge("RGBA", (red, green, blue, alpha)).save(result, format="PNG")
        return result.getvalue()


def frame_objects(payload: bytes, frame: int):
    active, current = {}, 0
    for code, body in sprite_tags(payload):
        item = placement(code, body)
        if item is not None:
            if item.move and item.depth in active:
                previous = active[item.depth]
                for field in ("character_id", "matrix", "color_transform", "ratio", "name", "clip_depth"):
                    if getattr(item, field) is None:
                        setattr(item, field, copy.deepcopy(getattr(previous, field)))
            item.move = False
            active[item.depth] = item
        elif code == 28:
            active.pop(struct.unpack_from("<H", body)[0], None)
        elif code == 1:
            current += 1
            if current == frame:
                return [active[depth] for depth in sorted(active)]
    raise ValueError(f"Reputation sprite has no frame {frame}")


def freeze(payload: bytes, frame: int) -> bytes:
    return payload[:2] + struct.pack("<H", 1) + b"".join(
        tag(26, item.to_bytes()) for item in frame_objects(payload, frame)
    ) + tag(1, b"") + tag(0, b"")


def source_assets(source_root: Path):
    sources = {name: (source_root / "interface" / name).read_bytes() for name in SOURCES}
    library = sources[SOURCES[1]]
    movie = inject_art(library, sources[SOURCES[0]], [
        (name, name) for _, name in native_runtime.list_symbols(library) if name != "ReputationLibraryImporter"
    ])
    entries = swf_tags(movie)
    symbols = symbol_ids(entries)
    entries = [(code, resolve_class_placements(body, symbols)) if code == 39 else (code, body)
               for code, body in entries]
    movie = replace_tags(movie, entries)
    sprites = {struct.unpack_from("<H", body)[0]: body for code, body in entries if code == 39}
    shapes = {struct.unpack_from("<H", body)[0]: DefineShapeTag.parse(body, code).shape
              for code, body in entries if code in (2, 22, 32, 83)}
    root = states(sprite_tags(sprites[symbols["SocialReputationWidget"]]))[0]
    entry = resolve_class_placements(sprites[symbols[ENTRY]], symbols)
    children = states(sprite_tags(entry))[0]
    meter = sprites[children["Progress_mc"].character_id]
    fill = states(sprite_tags(meter))[0]["Meter_mc"]
    internal = sprites[fill.character_id]
    if struct.unpack_from("<H", internal, 2)[0] != 201:
        raise ValueError("Reputation meter timeline changed; re-audit its progress mapping")
    meter_scales = [0.0]
    for frame in range(2, 202):
        items = frame_objects(internal, frame)
        if len(items) != 1:
            raise ValueError("Reputation meter no longer has one scalable fill")
        matrix = transform(items[0])
        if matrix[1:] != [0, 0, 1, 0, 0]:
            raise ValueError("Reputation fill transform changed")
        meter_scales.append(matrix[0])

    def bounds(character):
        return [v / 20 for v in shapes[character].bounds]

    def shape_asset(character, name):
        export = "B21ReputationMap_" + name
        binding = struct.pack("<HH", 1, character) + export.encode() + b"\0"
        data = replace_tags(movie, [*entries[:-1], (76, binding), entries[-1]])
        return native_runtime.render_symbol_png(data, export, 1, SCALE)

    background = next(item.character_id for item in frame_objects(entry, 1) if item.name is None)
    row_bounds = bounds(background)
    sizer = frame_objects(sprites[children["Sizer_mc"].character_id], 1)[0].character_id
    header_children = frame_objects(sprites[root["Header_mc"].character_id], 1)
    header_shape = next(item.character_id for item in header_children if item.character_id in shapes)
    header_bounds = bounds(header_shape)
    fill_shape = frame_objects(sprites[frame_objects(internal, 201)[0].character_id], 1)[0].character_id
    fill_bounds = bounds(fill_shape)
    progress_pos = transform(children["Progress_mc"])[4:]
    fill_pos = transform(fill)[4:]
    icon_frames = [frame_objects(sprites[symbols["FactionIcon"]], frame) for frame in (1, 2)]
    icon_bounds = []
    for items in icon_frames:
        if len(items) != 1 or transform(items[0]) != [1, 0, 0, 1, 0, 0]:
            raise ValueError("Reputation emblem geometry changed")
        icon_bounds.append(bounds(items[0].character_id))
    left, top = min(b[0] for b in icon_bounds), min(b[1] for b in icon_bounds)
    right, bottom = max(b[2] for b in icon_bounds), max(b[3] for b in icon_bounds)
    icon_scale, _, _, icon_y_scale, icon_x, icon_y = transform(children["Icon_mc"])
    layout = {
        "width": header_bounds[2], "headerHeight": root["List_mc"].matrix.translate_y / 20,
        "rowHeight": bounds(sizer)[3],
        "headerRect": [*transform(root["Header_mc"])[4:], header_bounds[2], header_bounds[3]],
        "rowRect": [row_bounds[0] + 1, row_bounds[1], row_bounds[2] - row_bounds[0], row_bounds[3] - row_bounds[1]],
        "name": [children["Name_tf"].matrix.translate_x / 20 + 1, children["Name_tf"].matrix.translate_y / 20, 32],
        "status": [children["Status_tf"].matrix.translate_x / 20 + 1, children["Status_tf"].matrix.translate_y / 20, 22],
        "title": [13.05, 2.0, 44],
        "fillRect": [progress_pos[0] + fill_pos[0] + fill_bounds[0] + 1,
                     progress_pos[1] + fill_pos[1] + fill_bounds[1],
                     fill_bounds[2] - fill_bounds[0], fill_bounds[3] - fill_bounds[1]],
        "meterScales": meter_scales,
        "emblemRect": [1 + icon_x + left * icon_scale, icon_y + top * icon_y_scale,
                       (right - left) * icon_scale, (bottom - top) * icon_y_scale],
    }
    assets = {"header": shape_asset(header_shape, "header"), "fill": shape_asset(fill_shape, "fill")}
    plate = symbols[resolve_numbered_name(symbols, "ReputationLibrary_fla.reputationWonkPlate_", "reputation plate")]
    expressions = [symbols[resolve_numbered_name(symbols, f"ReputationLibrary_fla.{faction}ExpressionContainer_", "reputation face")]
                   for faction in ("crater", "foundation")]
    for faction, icon_frame, face_frame in (("Crater", 1, 3), ("Foundation", 2, 2)):
        assets[f"{faction}-emblem"] = neutralize_emblem(native_runtime.render_symbol_png(movie, "FactionIcon", icon_frame, SCALE))
        for tier in range(7):
            replacements = {
                symbols[ENTRY]: select_children(entry, {"Face_mc", "Progress_mc"}, keep_unnamed=True),
                children["Progress_mc"].character_id: select_children(meter, set(), keep_unnamed=True),
                symbols["FactionStatusGraphic"]: freeze(sprites[symbols["FactionStatusGraphic"]], face_frame),
                plate: freeze(sprites[plate], 1 if tier == 0 else 7 if tier == 6 else 3),
                **{cid: freeze(sprites[cid], tier + 1) for cid in expressions},
            }
            rendered = replace_tags(movie, [(code, replacements.get(struct.unpack_from("<H", body)[0], body))
                                            if code == 39 else (code, body) for code, body in entries])
            assets[f"{faction}-{tier}"] = native_runtime.render_symbol_png(rendered, ENTRY, 1, SCALE)
    # Localized UI text remains text in the native renderer, never baked into artwork.
    return sources, layout, assets


def convert_reputation_map_ui(source_root: Path, output_root: Path) -> dict:
    sources, layout, assets = source_assets(Path(source_root))
    folder = Path(output_root) / OUTPUT
    folder.mkdir(parents=True, exist_ok=True)
    manifest = {"version": 2, "sourceWidget": "SocialReputationWidget", "layout": layout,
                "sources": [{"path": "interface/" + name, "sha256": hashlib.sha256(data).hexdigest()}
                            for name, data in sources.items()], "images": {}, "files": []}
    for name, data in assets.items():
        with Image.open(io.BytesIO(data)) as image:
            image = image.convert("RGBA")
            if not image.getchannel("A").getbbox():
                raise ValueError(f"Empty reputation map artwork: {name}")
            path = write_ui_dds(image, folder / f"{name}.dds", pad=True)
            manifest["images"][name] = {"path": f"reputation/{name}.dds",
                "uv": [image.width / (math.ceil(image.width / 4) * 4), image.height / (math.ceil(image.height / 4) * 4)]}
            manifest["files"].append({"path": path.relative_to(output_root).as_posix(),
                "sha256": hashlib.sha256(path.read_bytes()).hexdigest(), "width": image.width, "height": image.height})
    (folder / "catalog.json").write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    return manifest


if __name__ == "__main__":
    import argparse
    parser = argparse.ArgumentParser(description="Extract FO76 reputation artwork for the native map.")
    parser.add_argument("--source-root", type=Path, required=True)
    parser.add_argument("--output-root", type=Path, required=True)
    args = parser.parse_args()
    result = convert_reputation_map_ui(args.source_root, args.output_root)
    print(f"Wrote {len(result['files'])} reputation images")
