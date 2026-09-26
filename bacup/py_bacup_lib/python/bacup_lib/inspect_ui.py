from __future__ import annotations

import hashlib
import json
import struct
from pathlib import Path

from bacup_lib.daily_ops_ui import empty_hud_timeline, replace_classes
from bacup_lib.legendary_perks_ui import imports, remap_menu_fonts, swf_tags
from bacup_lib.translations import TRANSLATIONS, merge_ui_translations, translation_path

MENU = "examinemenu.swf"
MOVIE = "inspectcard.swf"
BRIDGE_CLASS = "ExamineMenu"
# FO4's ExamineMenu classes win over a loaded movie's own, so the card keeps none of their names.
CLASS_PREFIX = "B21TFA_Inspect"
# The player implements these; a renamed copy would lose its native half.
ENGINE_PACKAGES = ("scaleform.gfx",)
# FO76 draws the item card on this sprite: the box, the heading strip and the heading itself.
# It is art the menu places by hand rather than an exported symbol, so the card binds it to a class.
PANEL_CLASS = "StatsPanel"
PANEL_LABEL = b"$ITEM STATS"
FOOTER_CLASS = "InspectTab"
OUTPUT = Path("Interface/B21/TalesFromAppalachia/Inspect")
BRIDGE = Path(__file__).with_name("resources") / "inspect/ExamineMenu.as"
HEALTH_ENTRY = BRIDGE.with_name("ItemCard_ItemHealthEntry.as")
CONDITION_BRIDGE = BRIDGE.with_name("ConditionMeter.as")
BUTTON_HINTS = BRIDGE.parent.parent / "shared/B21ButtonHints.as"
# The card needs only fonts and the caps icon; the perk and button-hint libraries serve FO76's workbench.
LIBRARIES = ("fonts_en.swf", "currencyiconlibrary.swf")
TRANSLATION_KEYS = ("$APCost", "$CAPACITY", "$ATTACKMODE", "$Automatic", "$ItemInfo_CND",
                    "$ITEM STATS", "$CURRENT MODS", "$INSPECT", "$StackWeight")


def card_translations(interface: Path) -> list[str]:
    data = (interface / "translate_en.txt").read_bytes()
    encoding = "utf-16" if data.startswith((b"\xff\xfe", b"\xfe\xff")) else "utf-8-sig"
    lines = {line.split("\t", 1)[0]: line for line in data.decode(encoding).splitlines() if "\t" in line}
    missing = [key for key in TRANSLATION_KEYS if key not in lines]
    if missing:
        raise ValueError(f"Source item card translations are missing: {', '.join(missing)}")
    return [lines[key] for key in TRANSLATION_KEYS]


def rebuild(data: bytes, tags: list[tuple[int, bytes]]) -> bytes:
    if data[:3] != b"FWS":
        raise ValueError("Expected an uncompressed movie")
    body = data[8:]
    size = (5 + 4 * (body[0] >> 3) + 7) // 8 + 4
    body = body[:size] + b"".join(struct.pack("<HI", code << 6 | 63, len(payload)) + payload for code, payload in tags)
    return data[:4] + struct.pack("<I", len(body) + 8) + body


def keep_imports(data: bytes, libraries: tuple[str, ...]) -> bytes:
    return rebuild(data, [(code, payload) for code, payload in swf_tags(data)
                          if code not in (57, 71) or payload.split(b"\0", 1)[0].decode("utf-8").casefold() in libraries])


def bind_root_class(data: bytes, class_name: str) -> bytes:
    tags, found = [], 0
    for code, payload in swf_tags(data):
        if code == 76:
            count, = struct.unpack_from("<H", payload)
            offset, entries = 2, []
            for _ in range(count):
                character, = struct.unpack_from("<H", payload, offset)
                end = payload.index(b"\0", offset + 2)
                name = payload[offset + 2:end]
                if name == class_name.encode("utf-8"):
                    character, found = 0, found + 1
                entries.append((character, name))
                offset = end + 1
            payload = struct.pack("<H", len(entries)) + b"".join(
                struct.pack("<H", character) + name + b"\0" for character, name in entries)
        tags.append((code, payload))
    if found != 1:
        raise ValueError(f"Expected one SymbolClass binding for {class_name}, found {found}")
    return rebuild(data, tags)


def sprite_children(payload: bytes) -> set[int]:
    """The characters a DefineSprite places on its own timeline."""
    body, children, offset = payload[4:], set(), 0
    while offset < len(body):
        head, = struct.unpack_from("<H", body, offset)
        offset += 2
        code, length = head >> 6, head & 63
        if length == 63:
            length, = struct.unpack_from("<I", body, offset)
            offset += 4
        tag, offset = body[offset:offset + length], offset + length
        if code in (26, 70) and tag[0] & 2:
            start = 3 if code == 26 else 4
            if code == 70 and tag[1] & 8:
                start = tag.index(b"\0", start) + 1
            children.add(struct.unpack_from("<H", tag, start)[0])
    return children


def panel_character(movie: bytes) -> int:
    """The sprite that holds FO76's item-card heading, and with it the whole panel."""
    labels = {struct.unpack_from("<H", payload)[0] for code, payload in swf_tags(movie)
              if code == 37 and PANEL_LABEL in payload}
    owners = [struct.unpack_from("<H", payload)[0] for code, payload in swf_tags(movie)
              if code == 39 and sprite_children(payload) & labels]
    if len(owners) != 1:
        raise ValueError(f"Expected one sprite holding {PANEL_LABEL.decode()}, found {len(owners)}")
    return owners[0]


def export_character(movie: bytes, character: int, class_name: str) -> bytes:
    """Bind a character to a class of its own, so the card can instantiate the art directly."""
    from creation_lib.swf.native_runtime import build_movieclip_class_doabc
    tags, found = [], 0
    for code, payload in swf_tags(movie):
        if code == 76:
            # The class has to be defined before the binding that names it.
            tags.append((82, build_movieclip_class_doabc([class_name])))
            count, = struct.unpack_from("<H", payload)
            payload = (struct.pack("<H", count + 1) + payload[2:]
                       + struct.pack("<H", character) + class_name.encode("utf-8") + b"\0")
            found += 1
        tags.append((code, payload))
    if found != 1:
        raise ValueError(f"Expected one SymbolClass tag, found {found}")
    return rebuild(movie, tags)


def inspect_tab_character(movie: bytes) -> int:
    from creation_lib.swf.native_runtime import list_symbols
    root, = [character for character, name in list_symbols(movie) if name == BRIDGE_CLASS]
    sprite = next(payload for code, payload in swf_tags(movie)
                  if code == 39 and struct.unpack_from("<H", payload)[0] == root)
    body, offset = sprite[4:], 0
    while offset < len(body):
        head, = struct.unpack_from("<H", body, offset)
        offset += 2
        code, size = head >> 6, head & 63
        if size == 63:
            size, = struct.unpack_from("<I", body, offset)
            offset += 4
        tag, offset = body[offset:offset + size], offset + size
        if code == 26 and b"Header_mc\0" in tag and tag[0] & 2:
            return struct.unpack_from("<H", tag, 3)[0]
    raise ValueError("Source ExamineMenu Header_mc is missing")


def fo4_stage(movie: bytes) -> bytes:
    """Rewrite the frame size to FO4's 1280x720 menu stage.

    The card is its own movie view in game, so it is drawn against its own frame
    rather than FO76's 1920x1080 one, which would scale every position by 2/3."""
    body = movie[8:]
    size = (5 + 4 * (body[0] >> 3) + 7) // 8
    bits = "10000" + "".join(format(twips, "016b") for twips in (0, 1280 * 20, 0, 720 * 20)) + "000"
    body = int(bits, 2).to_bytes(9, "big") + body[size:]
    return movie[:4] + struct.pack("<I", len(body) + 8) + body


def rename_classes(data: bytes, prefix: str = CLASS_PREFIX) -> bytes:
    from creation_lib.swf.native_runtime import rename_as3_classes
    return rename_as3_classes(data, prefix, ENGINE_PACKAGES)


def convert_inspect_ui(source_root: Path, output_data: Path) -> dict:
    interface = Path(source_root) / "interface"
    translations = card_translations(interface)
    files = {path.name.casefold(): path for path in interface.iterdir() if path.is_file()}
    if MENU not in files:
        raise FileNotFoundError(f"Missing item card source: {interface / MENU}")
    source = files[MENU].read_bytes()
    inspect_tab = inspect_tab_character(source)
    movie = bind_root_class(keep_imports(empty_hud_timeline(source), LIBRARIES), BRIDGE_CLASS)
    pending, closure = imports(movie), {}
    while pending:
        name = pending.pop().casefold()
        if name in closure:
            continue
        if "/" in name or "\\" in name or not name.endswith(".swf"):
            raise ValueError(f"Unsupported item card import: {name}")
        if name not in files:
            raise FileNotFoundError(f"Missing item card dependency: {interface / name}")
        closure[name] = files[name].read_bytes()
        pending.extend(imports(closure[name]))
    # Before the bridge compiles, so its own source can name the panel class.
    movie = export_character(movie, panel_character(movie), PANEL_CLASS)
    movie = export_character(movie, inspect_tab, FOOTER_CLASS)
    meter = replace_classes(movie, {BRIDGE_CLASS: CONDITION_BRIDGE, "ItemCard_ItemHealthEntry": HEALTH_ENTRY}, closure.values())
    meter = fo4_stage(rename_classes(meter, "B21TFA_Condition"))
    from creation_lib.swf.native_runtime import compile_as3_do_abc
    tags = swf_tags(movie)
    first_script = next(i for i, (code, _) in enumerate(tags) if code == 82)
    tags.insert(first_script, (82, compile_as3_do_abc([BUTTON_HINTS.read_text(encoding="utf-8")])))
    movie = replace_classes(rebuild(movie, tags), {BRIDGE_CLASS: BRIDGE,
        "ItemCard_ItemHealthEntry": HEALTH_ENTRY}, closure.values())
    movie = fo4_stage(rename_classes(movie))
    inspect_fonts = rename_classes(closure["fonts_en.swf"], "B21TFA_InspectFonts")

    output = Path(output_data) / OUTPUT
    output.mkdir(parents=True, exist_ok=True)
    manifest = {"schema_version": 1, "source": MENU, "source_sha256": hashlib.sha256(source).hexdigest(),
                "bridge_sha256": hashlib.sha256(BRIDGE.read_bytes()).hexdigest(),
                "button_hints_sha256": hashlib.sha256(BUTTON_HINTS.read_bytes()).hexdigest(),
                "health_entry_sha256": hashlib.sha256(HEALTH_ENTRY.read_bytes()).hexdigest(),
                "condition_bridge_sha256": hashlib.sha256(CONDITION_BRIDGE.read_bytes()).hexdigest(),
                "class_prefix": CLASS_PREFIX,
                "menu": (OUTPUT / MOVIE).as_posix(), "files": []}
    for name, data, original in [(MOVIE, movie, source), ("bartercard.swf", movie, source),
                                  ("conditionmeter.swf", meter, source),
                                  ("b21_inspect_fonts.swf", inspect_fonts, closure["fonts_en.swf"])] + [(name, data, data) for name, data in sorted(closure.items())]:
        converted = remap_menu_fonts(data)
        (output / name).write_bytes(converted)
        manifest["files"].append({"path": (OUTPUT / name).as_posix(),
                                  "source_sha256": hashlib.sha256(original).hexdigest(),
                                  "sha256": hashlib.sha256(converted).hexdigest()})
    (output / "conversion.json").write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    merge_ui_translations(output_data, translations)
    return manifest


if __name__ == "__main__":
    import argparse

    parser = argparse.ArgumentParser(description="Convert FO76's item card into the Tales Inspect overlay")
    parser.add_argument("source_root", type=Path)
    parser.add_argument("output_data", type=Path)
    arguments = parser.parse_args()
    print(json.dumps(convert_inspect_ui(arguments.source_root, arguments.output_data), indent=2))
