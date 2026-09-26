from __future__ import annotations

import hashlib
import json
import struct
import zlib
from pathlib import Path

from bacup_lib.legendary_perks_ui import swf_tags
from bacup_lib.quest_area_ui import closure, symbol_ids, tag
from bacup_lib.status_hud_source import select_children, sprite_tags, states
from bacup_lib.translations import merge_ui_translations, packaged_tales_lines

OUTPUT = Path("Interface/B21/TalesFromAppalachia/QuickBoy.swf")
SCREEN_SOURCE = Path("meshes/interface/objects/screen.nif")
SCREEN_OUTPUT = Path("Meshes/B21/TalesFromAppalachia/QuickBoyScreen.nif")
BACKDROP_TEXTURE = Path("textures/shared/default_d.dds")
SCRIPT = Path(__file__).with_name("resources") / "quick_boy/B21_QuickBoy.as"


def build_movie(source: bytes) -> tuple[bytes, dict]:
    from creation_lib.swf.native_runtime import compile_as3_do_abc, unbacked_symbol_classes
    from creation_lib.swf.types import BitReader

    tags = swf_tags(source)
    symbols = symbol_ids(tags)
    if "PipboyMenu" not in symbols:
        raise ValueError("Quick-Boy requires the source PipboyMenu symbol")
    root = symbols["PipboyMenu"]
    sprites = {struct.unpack_from("<H", payload)[0]: payload for code, payload in tags if code == 39}
    layout = states(sprite_tags(sprites[root]))[0]
    if "MainBackground_mc" not in layout:
        raise ValueError("Quick-Boy requires PipboyMenu.MainBackground_mc")
    selected = select_children(sprites[root], {"MainBackground_mc"})
    art = closure([(c, selected if c == 39 and p == sprites[root] else p) for c, p in tags],
                  root, "PipboyMenu.MainBackground_mc")
    # Only the source background display list crosses the game boundary. Its
    # inventory code and same-named FO4 classes must never enter the host domain.
    scripts = [SCRIPT.read_text(encoding="utf-8"),
               "package { import flash.display.MovieClip; public dynamic class B21_QuickBoyArt extends MovieClip {} }"]
    bindings = struct.pack("<H", 2) + struct.pack("<H", 0) + b"B21_QuickBoy\0" + \
        struct.pack("<H", root) + b"B21_QuickBoyArt\0"
    body = zlib.decompress(source[8:]) if source[:3] == b"CWS" else source[8:]
    rect_bytes = (5 + 4 * (body[0] >> 3) + 7) // 8
    reader = BitReader(body)
    bits = reader.read_ubits(5)
    xmin, xmax, ymin, ymax = [reader.read_sbits(bits) for _ in range(4)]
    header = body[:rect_bytes + 2] + struct.pack("<H", 1)
    output = header + tag(69, struct.pack("<I", 8)) + b"".join(tag(c, p) for c, p in art) + \
        tag(82, compile_as3_do_abc(scripts)) + tag(76, bindings) + tag(1, b"") + tag(0, b"")
    movie = b"FWS" + source[3:4] + struct.pack("<I", len(output) + 8) + output
    if unbacked_symbol_classes(movie):
        raise ValueError("Unbound Quick-Boy artwork")
    return movie, {"schema_version": 1, "source_member": "PipboyMenu.MainBackground_mc",
                   "source_canvas": [(xmax - xmin) // 20, (ymax - ymin) // 20],
                   "art_tags": len(art), "imports": []}


def convert_quick_boy_ui(source_root: Path, output_data: Path) -> dict:
    from creation_lib.nif import native_runtime

    screen_source = Path(source_root) / SCREEN_SOURCE
    if not screen_source.is_file():
        raise FileNotFoundError(f"Quick-Boy requires the installed FO76 screen geometry: {screen_source}")
    backdrop_texture = (Path(source_root) / BACKDROP_TEXTURE).read_bytes()
    source = (Path(source_root) / "interface/pipboymenu.swf").read_bytes()
    movie, report = build_movie(source)
    screen = Path(output_data) / SCREEN_OUTPUT
    screen.parent.mkdir(parents=True, exist_ok=True)
    converted = native_runtime.convert_nif_file_raw(str(screen_source), str(screen), "fo76", "fo4", None,
                                                   {"source_path": SCREEN_SOURCE.as_posix()})
    if not converted.get("supported") or not screen.is_file():
        raise ValueError(f"Quick-Boy screen conversion failed: {converted}")
    target = Path(output_data) / OUTPUT
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_bytes(movie)
    texture = Path(output_data) / BACKDROP_TEXTURE
    texture.parent.mkdir(parents=True, exist_ok=True)
    texture.write_bytes(backdrop_texture)
    report.update(source_sha256=hashlib.sha256(source).hexdigest(),
                  screen_source=SCREEN_SOURCE.as_posix(),
                  screen_source_sha256=hashlib.sha256(screen_source.read_bytes()).hexdigest(),
                  bridge_sha256=hashlib.sha256(SCRIPT.read_bytes()).hexdigest(),
                  files=[{"path": OUTPUT.as_posix(), "sha256": hashlib.sha256(movie).hexdigest()},
                         {"path": SCREEN_OUTPUT.as_posix(), "sha256": hashlib.sha256(screen.read_bytes()).hexdigest()},
                         {"path": BACKDROP_TEXTURE.as_posix(), "sha256": hashlib.sha256(backdrop_texture).hexdigest(),
                          "source": BACKDROP_TEXTURE.as_posix()}])
    target.with_suffix(".json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    merge_ui_translations(output_data, [line for line in packaged_tales_lines() if line.startswith("$B21_TFA_QB_")])
    return report


if __name__ == "__main__":
    import argparse
    parser = argparse.ArgumentParser()
    parser.add_argument("source_root", type=Path)
    parser.add_argument("output_data", type=Path)
    args = parser.parse_args()
    print(json.dumps(convert_quick_boy_ui(args.source_root, args.output_data), indent=2))
