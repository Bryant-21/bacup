from __future__ import annotations

import hashlib
import json
import struct
from pathlib import Path

from bacup_lib.daily_ops_ui import empty_hud_timeline
from bacup_lib.death_screen_ui import pick_translations
from bacup_lib.inspect_ui import fo4_stage, keep_imports, rebuild
from bacup_lib.legendary_perks_ui import imports, remap_menu_fonts, swf_tags, verify_script_only
from bacup_lib.translations import merge_ui_translations
from bacup_lib.ui_contract import render_as3_template, resolve_numbered_name

MENU = "radialmenu.swf"
OUTPUT = Path("Interface/FavoritesMenu.swf")
STOCK_OUTPUT = Path("Interface/B21_TFAStockFavoritesMenu.swf")
RESOURCES = Path(__file__).with_name("resources") / "favorites"
BUTTON_HINTS = RESOURCES.parent / "shared" / "B21ButtonHints.as"
# FO76's quick-inventory panel and row art are kept; their list plumbing is replaced.
CLASSES = ("RadialMenu", "RadialMenuRingInner", "RadialMenuEntryInner", "PlayerListEntry", "SecureTradePlayerInventory")
MANIFEST = Path("Interface/B21/TalesFromAppalachia/Favorites/conversion.json")
BACKGROUND_CLASS_STEM = "RadialMenu_fla.radialBackground_mc_"
CENTER_CLASS_STEM = "RadialMenu_fla.radialCenterGroup_mc_"
TAB_CLASS_STEM = "RadialMenu_fla.radialTab_"
# FO76 text the wheel shows. $SELECT, $INSPECT and $BACK already resolve through FO4's own table.
TEXT_KEYS = ("$SCROLL", "$RADIAL_MENU_USE", "$CHANGE", "$INSPECT/REPAIR", "$FAVORITES", "$QUICKINVENTORY",
             "$NO_ITEMS")
SHAPE_TAGS = (2, 22, 32, 83)


def read_icon_keywords(source_plugin: Path) -> dict[str, str]:
    from creation_lib.esp import Plugin

    icons = {}
    with Plugin.load(source_plugin, game="fo76", lazy_index=True) as plugin:
        for row in plugin.record_index_rows(signatures=["KYWD"]):
            record = plugin.read_authoring_record(row[4]) or {}
            fields = {key: value for field in record.get("fields", []) for key, value in field.items()}
            if fields.get("Type") == "UIIconLinkageName" and fields.get("IconName"):
                icons[record["eid"]] = fields["IconName"]
    if not icons:
        raise ValueError(f"No favorites icon-link keywords found in {source_plugin}")
    return dict(sorted(icons.items()))


def sprite_tags(sprite: bytes) -> list[tuple[int, bytes]]:
    tags, offset = [], 4
    while offset < len(sprite):
        head, = struct.unpack_from("<H", sprite, offset)
        offset += 2
        code, length = head >> 6, head & 63
        if length == 63:
            length, = struct.unpack_from("<I", sprite, offset)
            offset += 4
        tags.append((code, sprite[offset:offset + length]))
        offset += length
    return tags


def without_class_placements(movie: bytes, character: int) -> bytes:
    """Drop a sprite's place-by-class-name children; Ruffle rejects them and the bridge never uses them."""
    tags = []
    for code, payload in swf_tags(movie):
        if code == 39 and struct.unpack_from("<H", payload)[0] == character:
            kept = [(c, t) for c, t in sprite_tags(payload) if not (c == 70 and t[1] & 8 and not t[0] & 2)]
            payload = payload[:4] + b"".join(struct.pack("<HI", c << 6 | 63, len(t)) + t for c, t in kept)
        tags.append((code, payload))
    return rebuild(movie, tags)


def ring_disk(sprite: bytes, shapes: set[int]) -> int:
    """The lowest shape on the wheel sprite's first frame: FO76's filled disk behind the slots."""
    placed = {}
    for code, tag in sprite_tags(sprite):
        if code in (0, 1):
            break
        if code in (26, 70) and tag[0] & 2:
            depth_at = 1 if code == 26 else 2
            start = depth_at + 2
            if code == 70 and tag[1] & 8:
                start = tag.index(b"\0", start) + 1
            character, = struct.unpack_from("<H", tag, start)
            if character in shapes:
                placed[struct.unpack_from("<H", tag, depth_at)[0]] = character
    if not placed:
        raise ValueError("Favorites source wheel places no disk shape on its first frame")
    return placed[min(placed)]


def wheel_symbol_and_disk(source: bytes) -> tuple[int, int]:
    from creation_lib.swf.native_runtime import list_symbols

    symbol = dict((name, character) for character, name in list_symbols(source))["RadialMenu"]
    tags = swf_tags(source)
    shapes = {struct.unpack_from("<H", payload)[0] for code, payload in tags if code in SHAPE_TAGS}
    sprite = next(payload for code, payload in tags
                  if code == 39 and struct.unpack_from("<H", payload)[0] == symbol)
    return symbol, ring_disk(sprite, shapes)


def wheel_timeline(source: bytes) -> bytes:
    symbol, disk = wheel_symbol_and_disk(source)
    movie = empty_hud_timeline(source)
    tags = []
    for code, payload in swf_tags(movie):
        if code == 39 and struct.unpack_from("<H", payload)[0] == symbol:
            # Construct only the favorites clips and the wheel's disk, never the trading/emotes/client-data UI.
            payload = (struct.pack("<HH", symbol, 1) + struct.pack("<HBHH", 26 << 6 | 5, 0x02, 1, disk)
                       + struct.pack("<HH", 1 << 6, 0))
        if code not in (0, 1):
            tags.append((code, payload))
    # FO4 binds its code object and favorite data to root.Menu_mc.
    tags += [(26, b"\x26" + struct.pack("<HH", 1, symbol) + b"\x00Menu_mc\0"), (1, b""), (0, b"")]
    return fo4_stage(rebuild(movie, tags))


def resolve_icon_keywords(source_keywords: dict[str, str], symbols: set[str]) -> dict[str, str]:
    # FO76's Nuka Mine keyword names NukaMineIcon, but its radial artwork exports NukeMineIcon.
    aliases = {"NukaMineIcon": "NukeMineIcon"}
    return {name: aliases.get(icon, icon) for name, icon in source_keywords.items()
            if aliases.get(icon, icon) in symbols}


def render_icon_resolver(script: str, icon_keywords: dict[str, str]) -> str:
    return render_as3_template(script, {
        "/* BACUP_ICON_KEYWORDS */": json.dumps(icon_keywords)[1:-1],
        "/* BACUP_ICON_RESOLVER */": (RESOURCES / "ResolveIcon.as").read_text(encoding="utf-8"),
    })


def bridge_sources(symbols: set[str], icon_keywords: dict[str, str]) -> tuple[dict[str, str], dict[str, str]]:
    source_classes = {
        "background": resolve_numbered_name(
            symbols, BACKGROUND_CLASS_STEM, "favorites background class"
        ),
        "center": resolve_numbered_name(symbols, CENTER_CLASS_STEM, "favorites center class"),
        "tab": resolve_numbered_name(symbols, TAB_CLASS_STEM, "favorites corner tab class"),
    }
    bridges = {name: (RESOURCES / f"{name}.as").read_text(encoding="utf-8") for name in CLASSES}
    bridges["RadialMenu"] = render_as3_template(render_icon_resolver(bridges["RadialMenu"], icon_keywords), {
        "__B21_RADIAL_BACKGROUND_CLASS__": source_classes["background"],
        "__B21_RADIAL_CENTER_CLASS__": source_classes["center"],
        "__B21_RADIAL_TAB_CLASS__": source_classes["tab"],
    })
    return bridges, source_classes


def with_button_hints(movie: bytes) -> bytes:
    """Compile Tales' FO4-style hint bar ahead of the movie's scripts so the bridge can name it."""
    from creation_lib.swf.native_runtime import compile_as3_do_abc

    tags = swf_tags(movie)
    first_script = next(i for i, (code, _) in enumerate(tags) if code == 82)
    tags.insert(first_script, (82, compile_as3_do_abc([BUTTON_HINTS.read_text(encoding="utf-8")])))
    return rebuild(movie, tags)


def convert_favorites_ui(source_root: Path, output_data: Path, *, source_plugin: Path, target_data_dir: Path) -> dict:
    from creation_lib.ba2.native_runtime import extract_one
    from creation_lib.swf import native_runtime

    interface = Path(source_root) / "interface"
    files = {path.name.casefold(): path for path in interface.iterdir() if path.is_file()}
    if MENU not in files:
        raise FileNotFoundError(f"Missing favorites wheel source: {interface / MENU}")
    stock_archive = Path(target_data_dir) / "Fallout4 - Interface.ba2"
    stock = extract_one(str(stock_archive), "interface/favoritesmenu.swf")
    if not stock:
        raise FileNotFoundError(f"Missing stock favorites menu in {stock_archive}")
    source = files[MENU].read_bytes()
    source_keywords = read_icon_keywords(source_plugin)
    characters = {name: character for character, name in native_runtime.list_symbols(source)}
    symbols = set(characters)
    icon_keywords = resolve_icon_keywords(source_keywords, symbols)
    unavailable = {name: icon for name, icon in source_keywords.items() if name not in icon_keywords}
    missing_wheel_icons = {name: icon for name, icon in unavailable.items() if name.startswith("UI_")}
    if missing_wheel_icons:
        raise ValueError(f"Favorites source lacks keyword-linked artwork: {missing_wheel_icons}")
    translations = pick_translations(interface, TEXT_KEYS, "Favorites wheel")
    _, disk = wheel_symbol_and_disk(source)
    movie = keep_imports(wheel_timeline(source), ("fonts_en.swf",))
    movie = with_button_hints(without_class_placements(movie, characters["SecureTradePlayerInventory"]))
    closure = {}
    pending = imports(movie)
    while pending:
        name = pending.pop().casefold()
        if name in closure:
            continue
        if "/" in name or "\\" in name or not name.endswith(".swf"):
            raise ValueError(f"Unsupported favorites import: {name}")
        if name not in files:
            raise FileNotFoundError(f"Missing favorites dependency: {interface / name}")
        closure[name] = files[name].read_bytes()
        pending.extend(imports(closure[name]))
    bridges, source_classes = bridge_sources(symbols, icon_keywords)
    movie = verify_script_only(movie, native_runtime.replace_as3_classes(movie, bridges, list(closure.values())))
    movie = remap_menu_fonts(movie)
    tags = [(code, payload.replace(payload.split(b"\0", 1)[0],
                                  b"B21/TalesFromAppalachia/Favorites/" + payload.split(b"\0", 1)[0], 1))
            if code in (57, 71) else (code, payload) for code, payload in swf_tags(movie)]
    movie = rebuild(movie, tags)
    manifest = {"schema_version": 4, "source": MENU, "source_sha256": hashlib.sha256(source).hexdigest(),
                "menu": OUTPUT.as_posix(), "code_object": "root.Menu_mc", "slots": 12,
                "ring_disk_character": disk, "translation_keys": list(TEXT_KEYS),
                "button_hints_sha256": hashlib.sha256(BUTTON_HINTS.read_bytes()).hexdigest(),
                "stock_menu": STOCK_OUTPUT.as_posix(), "stock_source_archive": stock_archive.name,
                "icon_keywords": icon_keywords,
                "non_wheel_icon_keywords": unavailable,
                "source_classes": source_classes,
                "source_plugin": Path(source_plugin).name,
                "source_icon_keywords_sha256": hashlib.sha256(json.dumps(source_keywords, sort_keys=True).encode()).hexdigest(),
                "icon_exports": sorted(name for name in symbols if name.endswith("Icon") or name == "radialIconEmpty"),
                "bridge_sha256": {name: hashlib.sha256((RESOURCES / f"{name}.as").read_bytes()).hexdigest()
                                  for name in CLASSES}, "files": []}
    outputs = [(OUTPUT, movie, source), (STOCK_OUTPUT, stock, stock)] + [(MANIFEST.parent / name, remap_menu_fonts(data), data)
                                         for name, data in sorted(closure.items())]
    for relative, data, original in outputs:
        target = Path(output_data) / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(data)
        manifest["files"].append({"path": relative.as_posix(), "sha256": hashlib.sha256(data).hexdigest(),
                                  "source_sha256": hashlib.sha256(original).hexdigest()})
    receipt = Path(output_data) / MANIFEST
    receipt.parent.mkdir(parents=True, exist_ok=True)
    receipt.write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    merge_ui_translations(output_data, translations)
    return manifest


if __name__ == "__main__":
    import argparse

    parser = argparse.ArgumentParser(description="Convert FO76's radial favorites UI for Fallout 4")
    parser.add_argument("source_root", type=Path)
    parser.add_argument("output_data", type=Path)
    parser.add_argument("--source-plugin", type=Path, required=True)
    parser.add_argument("--target-data-dir", type=Path, required=True)
    args = parser.parse_args()
    print(json.dumps(convert_favorites_ui(args.source_root, args.output_data, source_plugin=args.source_plugin,
                                         target_data_dir=args.target_data_dir), indent=2))
