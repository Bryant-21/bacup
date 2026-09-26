from __future__ import annotations

import hashlib
import json
import struct
import zlib
from pathlib import Path

from bacup_lib.legendary_perks_ui import remap_menu_fonts, swf_tags
from bacup_lib.translations import TRANSLATIONS, merge_ui_translations, translation_path
from bacup_lib.ui_contract import resolve_numbered_name

SOURCE = "hudmenu.swf"
SYMBOL_STEM = "HUDMenu_fla.AreaQuest_CompassWithinRect_"
CLIP_CLASS = "B21_QuestAreaWithinClip"
ROOT_CLASS = "B21_QuestAreaCompass"
OUTPUT = Path("Interface/B21/TalesFromAppalachia/QuestAreas")
MOVIE = "compassarea.swf"
SCRIPTS = Path(__file__).with_name("resources") / "quest_areas"
TEXT_KEYS = ("$INSIDE_SEARCH_AREA",)

SHAPES = {2, 22, 32, 83, 46, 84}
SPRITE = 39
EDIT_TEXT = 37
# Per-character companions keyed by the character id they describe.
COMPANIONS = {74, 78}
SYMBOL_CLASS = 76


def tag(code: int, payload: bytes) -> bytes:
    return struct.pack("<HI", code << 6 | 63, len(payload)) + payload


def symbol_ids(tags: list[tuple[int, bytes]]) -> dict[str, int]:
    ids = {}
    for code, payload in tags:
        if code != SYMBOL_CLASS:
            continue
        offset = 2
        for _ in range(struct.unpack_from("<H", payload)[0]):
            character, = struct.unpack_from("<H", payload, offset)
            end = payload.index(b"\0", offset + 2)
            ids[payload[offset + 2:end].decode("utf-8")] = character
            offset = end + 1
    return ids


def placed_characters(sprite: bytes) -> set[int]:
    characters, offset = set(), 4
    while offset < len(sprite):
        header, = struct.unpack_from("<H", sprite, offset)
        offset += 2
        code, size = header >> 6, header & 63
        if size == 63:
            size, = struct.unpack_from("<I", sprite, offset)
            offset += 4
        body = sprite[offset:offset + size]
        if code == 26 and body[0] & 0x02:
            characters.add(struct.unpack_from("<H", body, 3)[0])
        elif code == 70 and body[0] & 0x02:
            at = 4
            if body[1] & 0x18:
                at = body.index(b"\0", at) + 1
            characters.add(struct.unpack_from("<H", body, at)[0])
        elif code == 4:
            characters.add(struct.unpack_from("<H", body)[0])
        offset += size
    return characters


def closure(tags: list[tuple[int, bytes]], root: int, symbol: str) -> list[tuple[int, bytes]]:
    defines = {struct.unpack_from("<H", payload)[0]: (code, payload)
               for code, payload in tags if code in SHAPES | {SPRITE, EDIT_TEXT}}
    wanted, pending = set(), [root]
    while pending:
        character = pending.pop()
        if character in wanted:
            continue
        if character not in defines:
            raise ValueError(f"{symbol} references unsupported character {character}")
        wanted.add(character)
        code, payload = defines[character]
        if code == SPRITE:
            pending.extend(placed_characters(payload))
        elif code == EDIT_TEXT and payload[2 + (5 + 4 * (payload[2] >> 3) + 7) // 8] & 0x01:
            raise ValueError(f"{symbol} text field {character} embeds a font")
    return [(code, payload) for code, payload in tags
            if code in SHAPES | COMPANIONS | {SPRITE, EDIT_TEXT}
            and struct.unpack_from("<H", payload)[0] in wanted]


def build_movie(source: bytes) -> tuple[bytes, int, str]:
    from creation_lib.swf.native_runtime import compile_as3_class_names, compile_as3_do_abc

    tags = swf_tags(source)
    symbols = symbol_ids(tags)
    symbol = resolve_numbered_name(symbols, SYMBOL_STEM, "quest-area compass symbol")
    root = symbols[symbol]
    scripts = [path.read_text(encoding="utf-8") for path in sorted(SCRIPTS.glob("*.as"))]
    if sorted(compile_as3_class_names(scripts)) != sorted([CLIP_CLASS, ROOT_CLASS]):
        raise ValueError("quest area scripts must define exactly the clip and root classes")
    exports = struct.pack("<H", 2) + struct.pack("<H", root) + CLIP_CLASS.encode() + b"\0" + \
        struct.pack("<H", 0) + ROOT_CLASS.encode() + b"\0"
    art = [tag(code, payload) for code, payload in closure(tags, root, symbol)]
    body = zlib.decompress(source[8:]) if source[:3] == b"CWS" else source[8:]
    header = body[:(5 + 4 * (body[0] >> 3) + 7) // 8 + 2] + struct.pack("<H", 1)
    body = header + tag(69, struct.pack("<I", 0x08)) + b"".join(art) + \
        tag(82, compile_as3_do_abc(scripts)) + tag(SYMBOL_CLASS, exports) + tag(1, b"") + tag(0, b"")
    return remap_menu_fonts(b"FWS" + source[3:4] + struct.pack("<I", len(body) + 8) + body), 0, symbol


def text_translations(interface: Path) -> list[str]:
    data = (interface / "translate_en.txt").read_bytes()
    encoding = "utf-16" if data.startswith((b"\xff\xfe", b"\xfe\xff")) else "utf-8-sig"
    lines = [line for line in data.decode(encoding).splitlines() if line.split("\t", 1)[0] in TEXT_KEYS]
    missing = set(TEXT_KEYS) - {line.split("\t", 1)[0] for line in lines}
    if missing:
        raise ValueError(f"Quest area translations are missing: {sorted(missing)}")
    return lines


def convert_quest_area_ui(source_root: Path, output_data: Path) -> dict:
    interface = Path(source_root) / "interface"
    source = (interface / SOURCE).read_bytes()
    translations = text_translations(interface)
    movie, cleared, symbol = build_movie(source)
    output = Path(output_data) / OUTPUT
    output.mkdir(parents=True, exist_ok=True)
    (output / MOVIE).write_bytes(movie)
    manifest = {"schema_version": 2, "symbol": symbol, "classes": [ROOT_CLASS, CLIP_CLASS],
                "backdrop_fills_cleared": cleared,
                "scripts": {path.name: hashlib.sha256(path.read_bytes()).hexdigest()
                            for path in sorted(SCRIPTS.glob("*.as"))},
                "files": [{"path": (OUTPUT / MOVIE).as_posix(), "source": SOURCE,
                           "source_sha256": hashlib.sha256(source).hexdigest(),
                           "sha256": hashlib.sha256(movie).hexdigest()}]}
    (output / "conversion.json").write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    merge_ui_translations(output_data, translations)
    return manifest
