from __future__ import annotations

import hashlib
import json
import struct
from pathlib import Path

from bacup_lib.daily_ops_ui import empty_hud_timeline
from bacup_lib.inspect_ui import fo4_stage, keep_imports, rebuild
from bacup_lib.legendary_perks_ui import imports, remap_menu_fonts, swf_tags, verify_script_only
from bacup_lib.ui_contract import render_as3_template, resolve_numbered_name

MENU = "radialmenu.swf"
OUTPUT = Path("Interface/FavoritesMenu.swf")
STOCK_OUTPUT = Path("Interface/B21_TFAStockFavoritesMenu.swf")
RESOURCES = Path(__file__).with_name("resources") / "favorites"
CLASSES = ("RadialMenu", "RadialMenuRingInner", "RadialMenuEntryInner")
MANIFEST = Path("Interface/B21/TalesFromAppalachia/Favorites/conversion.json")
BACKGROUND_CLASS_STEM = "RadialMenu_fla.radialBackground_mc_"
CENTER_CLASS_STEM = "RadialMenu_fla.radialCenterGroup_mc_"


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


def wheel_timeline(source: bytes) -> bytes:
    from creation_lib.swf.native_runtime import list_symbols

    symbol = dict((name, character) for character, name in list_symbols(source))["RadialMenu"]
    movie = empty_hud_timeline(source)
    tags = []
    for code, payload in swf_tags(movie):
        if code == 39 and struct.unpack_from("<H", payload)[0] == symbol:
            # Construct only the favorites clips, never the trading/emotes/client-data UI.
            payload = struct.pack("<HHHH", symbol, 1, 64, 0)
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
    }
    bridges = {name: (RESOURCES / f"{name}.as").read_text(encoding="utf-8") for name in CLASSES}
    bridges["RadialMenu"] = render_as3_template(render_icon_resolver(bridges["RadialMenu"], icon_keywords), {
        "__B21_RADIAL_BACKGROUND_CLASS__": source_classes["background"],
        "__B21_RADIAL_CENTER_CLASS__": source_classes["center"],
    })
    return bridges, source_classes


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
    symbols = {name for _, name in native_runtime.list_symbols(source)}
    icon_keywords = resolve_icon_keywords(source_keywords, symbols)
    unavailable = {name: icon for name, icon in source_keywords.items() if name not in icon_keywords}
    missing_wheel_icons = {name: icon for name, icon in unavailable.items() if name.startswith("UI_")}
    if missing_wheel_icons:
        raise ValueError(f"Favorites source lacks keyword-linked artwork: {missing_wheel_icons}")
    movie = keep_imports(wheel_timeline(source), ("fonts_en.swf",))
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
    manifest = {"schema_version": 3, "source": MENU, "source_sha256": hashlib.sha256(source).hexdigest(),
                "menu": OUTPUT.as_posix(), "code_object": "root.Menu_mc", "slots": 12,
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
