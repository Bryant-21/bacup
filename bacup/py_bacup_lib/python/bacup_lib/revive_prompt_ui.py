"""Convert FO76's downed revive prompt for the Tales companion revive."""

import hashlib
import json
import struct
import zlib
from pathlib import Path

from bacup_lib.death_screen_ui import pick_translations
from bacup_lib.legendary_perks_ui import remap_menu_fonts, swf_tags
from bacup_lib.quest_area_ui import closure, symbol_ids, tag
from bacup_lib.translations import merge_ui_translations, translation_path
from bacup_lib.ui_contract import resolve_numbered_name

SOURCE = "hudmenu.swf"
SYMBOL_STEM = "HUDMenu_fla.RevivePrompt_"
PARTS = {"reviveTimer": "B21_ReviveTimerArt", "reviveTitle": "B21_ReviveTitleArt"}
ROOT_CLASS = "B21_RevivePrompt"
OUTPUT = Path("Interface/B21/TalesFromAppalachia/RevivePrompt")
MOVIE = "reviveprompt.swf"
SCRIPTS = Path(__file__).with_name("resources") / "revive_prompt"
TEXT_KEYS = ("$CRITICALLY INJURED", "$GIVEUP")

__all__ = ["SOURCE", "SYMBOL_STEM", "ROOT_CLASS", "OUTPUT", "MOVIE", "convert_revive_prompt_ui",
           "named_children", "symbol_ids", "translation_path"]


def _bits(data: bytes, position: int, count: int) -> int:
    value = 0
    for index in range(position, position + count):
        value = value << 1 | (data[index // 8] >> (7 - index % 8)) & 1
    return value


def _skip_matrix(data: bytes, offset: int) -> int:
    position = offset * 8
    for _ in range(2):  # scale, then rotate/skew
        present = _bits(data, position, 1)
        position += 1
        if present:
            count = _bits(data, position, 5)
            position += 5 + 2 * count
    count = _bits(data, position, 5)
    position += 5 + 2 * count
    return (position + 7) // 8


def _skip_color_transform(data: bytes, offset: int) -> int:
    position = offset * 8
    add, multiply, count = _bits(data, position, 1), _bits(data, position + 1, 1), _bits(data, position + 2, 4)
    position += 6 + 4 * count * (add + multiply)
    return (position + 7) // 8


def named_children(sprite: bytes) -> dict[str, int]:
    """Instance name -> character id for the PlaceObject2/3 tags of a DefineSprite payload."""
    names: dict[str, int] = {}
    offset = 4
    while offset < len(sprite):
        header = struct.unpack_from("<H", sprite, offset)[0]
        offset += 2
        code, length = header >> 6, header & 0x3F
        if length == 0x3F:
            length = struct.unpack_from("<I", sprite, offset)[0]
            offset += 4
        body, offset = sprite[offset:offset + length], offset + length
        if code == 0:
            break
        if code not in (26, 70):
            continue
        flags = body[0]
        cursor = 3 if code == 26 else 4
        if code == 70 and body[1] & 0x08:
            cursor = body.index(b"\0", cursor) + 1
        if not flags & 0x02 or not flags & 0x20:
            continue
        character = struct.unpack_from("<H", body, cursor)[0]
        cursor += 2
        if flags & 0x04:
            cursor = _skip_matrix(body, cursor)
        if flags & 0x08:
            cursor = _skip_color_transform(body, cursor)
        if flags & 0x10:
            cursor += 2
        names[body[cursor:body.index(b"\0", cursor)].decode("utf-8")] = character
    return names


def build_movie(source: bytes) -> tuple[bytes, str]:
    from creation_lib.swf.native_runtime import compile_as3_class_names, compile_as3_do_abc

    tags = swf_tags(source)
    symbols = symbol_ids(tags)
    symbol = resolve_numbered_name(symbols, SYMBOL_STEM, "revive prompt symbol")
    sprites = {struct.unpack_from("<H", payload)[0]: payload for code, payload in tags if code == 39}
    children = named_children(sprites[symbols[symbol]])
    missing = [name for name in PARTS if name not in children]
    if missing:
        raise ValueError(f"{symbol} lacks {', '.join(missing)}")
    scripts = [path.read_text(encoding="utf-8") for path in sorted(SCRIPTS.glob("*.as"))]
    if sorted(compile_as3_class_names(scripts)) != sorted([ROOT_CLASS, *PARTS.values()]):
        raise ValueError("revive prompt scripts must define exactly the root and part classes")
    art: list[bytes] = []
    seen: set[tuple[int, bytes]] = set()
    for name in PARTS:
        for code, payload in closure(tags, children[name], f"{symbol}.{name}"):
            if (code, payload) not in seen:
                seen.add((code, payload))
                art.append(tag(code, payload))
    rows = [(children[name], cls) for name, cls in PARTS.items()] + [(0, ROOT_CLASS)]
    exports = struct.pack("<H", len(rows)) + b"".join(
        struct.pack("<H", character) + cls.encode() + b"\0" for character, cls in rows)
    body = zlib.decompress(source[8:]) if source[:3] == b"CWS" else source[8:]
    header = body[:(5 + 4 * (body[0] >> 3) + 7) // 8 + 2] + struct.pack("<H", 1)
    body = header + tag(69, struct.pack("<I", 0x08)) + b"".join(art) + \
        tag(82, compile_as3_do_abc(scripts)) + tag(76, exports) + tag(1, b"") + tag(0, b"")
    return remap_menu_fonts(b"FWS" + source[3:4] + struct.pack("<I", len(body) + 8) + body), symbol


def convert_revive_prompt_ui(source_root: Path, output_data: Path) -> dict:
    interface = Path(source_root) / "interface"
    source = (interface / SOURCE).read_bytes()
    translations = pick_translations(interface, TEXT_KEYS, "Revive prompt")
    movie, symbol = build_movie(source)
    output = Path(output_data) / OUTPUT
    output.mkdir(parents=True, exist_ok=True)
    (output / MOVIE).write_bytes(movie)
    manifest = {"schema_version": 1, "symbol": symbol, "classes": [ROOT_CLASS, *PARTS.values()],
                "scripts": {path.name: hashlib.sha256(path.read_bytes()).hexdigest()
                            for path in sorted(SCRIPTS.glob("*.as"))},
                "files": [{"path": (OUTPUT / MOVIE).as_posix(), "source": SOURCE,
                           "source_sha256": hashlib.sha256(source).hexdigest(),
                           "sha256": hashlib.sha256(movie).hexdigest()}]}
    (output / "conversion.json").write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    merge_ui_translations(output_data, translations)
    return manifest
