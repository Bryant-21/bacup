from __future__ import annotations

import hashlib
import json
import struct
from pathlib import Path

from bacup_lib.daily_ops_ui import empty_hud_timeline
from bacup_lib.inspect_ui import rebuild
from bacup_lib.legendary_perks_ui import imports, remap_menu_fonts, swf_tags, verify_script_only
from bacup_lib.translations import TRANSLATIONS, merge_ui_translations, translation_path, merge_packaged

MENU = "seventysixmenu.swf"
OUTPUT = Path("Interface/B21/TalesFromAppalachia/Challenges")
RESOURCES = Path(__file__).with_name("resources") / "challenges"
CLASS_PREFIX = "B21TFA_Challenges"
BRIDGES = ("SeventySixMenu", "SeventySixMenuChallenges", "ChallengeListEntry")
MAP_OUTPUT = Path("F4SE/Plugins/B21_FullScreenMap/challenges")
MAP_ICONS = {"caps": "IconCR_Caps", "experience": "IconCR_Experience", "perk-pack": "IconCR_PerkPack"}


def convert_challenge_map_ui(source_root: Path, output_mod: Path) -> dict:
    import io

    from PIL import Image

    from bacup_lib.fullscreen_map import write_ui_dds
    from creation_lib.swf import native_runtime

    source = Path(source_root) / "interface/challengerewardiconlibrary.swf"
    movie = source.read_bytes()
    symbols = {name for _, name in native_runtime.list_symbols(movie)}
    missing = set(MAP_ICONS.values()) - symbols
    if missing:
        raise ValueError(f"Missing FO76 challenge reward artwork: {sorted(missing)}")
    output = Path(output_mod) / MAP_OUTPUT
    output.mkdir(parents=True, exist_ok=True)
    manifest = {"schema_version": 1, "presentation": "B21UI", "source": source.name,
                "source_sha256": hashlib.sha256(movie).hexdigest(), "files": []}
    for name, symbol in MAP_ICONS.items():
        png = native_runtime.render_symbol_png(movie, symbol, 1, 2.0)
        with Image.open(io.BytesIO(png)) as image:
            dds = write_ui_dds(image, output / f"{name}.dds")
        manifest["files"].append({"path": (MAP_OUTPUT / dds.name).as_posix(), "symbol": symbol,
                                  "sha256": hashlib.sha256(dds.read_bytes()).hexdigest()})
    (output / "conversion.json").write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    return manifest


def bind_challenge_components(movie: bytes, library: bytes) -> bytes:
    from creation_lib.swf import native_runtime

    classes = set(native_runtime.abc_class_names(movie))
    exports = [name for _, name in native_runtime.list_symbols(library) if name in classes]
    movie = native_runtime.inject_symbols(library, movie, exports)
    symbols = {name: cid for cid, name in native_runtime.list_symbols(movie)}
    tags = []
    for code, payload in swf_tags(movie):
        if code == 39 and int.from_bytes(payload[:2], "little") == symbols["SeventySixMenuChallenges"]:
            offset, nested = 4, []
            while offset < len(payload):
                header, = struct.unpack_from("<H", payload, offset)
                offset += 2
                child_code, size = header >> 6, header & 63
                if size == 63:
                    size, = struct.unpack_from("<I", payload, offset)
                    offset += 4
                body = payload[offset:offset + size]
                offset += size
                if child_code == 70 and body[:2] == b"\x24\x08":
                    end = body.index(b"\0", 4)
                    name = body[4:end].decode()
                    if name == "ScoreWidgetManager":
                        continue
                    if name != "MenuListComponent":
                        raise ValueError(f"Unexpected class-only challenge component: {name}")
                    # FO76's class-only placement is a Scaleform extension; bind its imported artwork explicitly.
                    child_code = 26
                    body = b"\x26" + body[2:4] + struct.pack("<H", symbols[name]) + body[end + 1:]
                nested.append(struct.pack("<HI", child_code << 6 | 63, len(body)) + body)
            payload = payload[:4] + b"".join(nested)
        tags.append((code, payload))
    return rebuild(movie, tags)


def convert_challenge_ui(source_root: Path, output_data: Path) -> dict:
    from creation_lib.swf import native_runtime

    interface = Path(source_root) / "interface"
    files = {path.name.casefold(): path for path in interface.iterdir() if path.is_file()}
    closure: dict[str, bytes] = {}
    pending = [MENU, "challengerewardiconlibrary.swf"]
    while pending:
        name = pending.pop().casefold()
        if name in closure:
            continue
        if "/" in name or "\\" in name or not name.endswith(".swf"):
            raise ValueError(f"Unsupported challenge UI dependency: {name}")
        if name not in files:
            raise FileNotFoundError(f"Missing challenge UI source: {interface / name}")
        closure[name] = files[name].read_bytes()
        pending.extend(imports(closure[name]))
    symbols = {name for _, name in native_runtime.list_symbols(closure[MENU])}
    if not {"SeventySixMenu", "SeventySixMenuChallenges", "ChallengeListEntry"}.issubset(symbols):
        raise ValueError("Source challenge menu does not contain the required FO76 screen symbols")
    reward_symbols = {name for _, name in native_runtime.list_symbols(closure["challengerewardiconlibrary.swf"])}
    if not {"IconCR_Caps", "IconCR_Experience"}.issubset(reward_symbols):
        raise ValueError("Source challenge reward library lacks caps/experience artwork")
    bridges = {name: (RESOURCES / f"{name}.as").read_text(encoding="utf-8") for name in BRIDGES}
    movie = empty_hud_timeline(closure[MENU])
    movie = bind_challenge_components(movie, closure["menulistcomponent.swf"])
    icons = ["IconCR_Caps", "IconCR_Experience"]
    movie = native_runtime.inject_symbols(closure["challengerewardiconlibrary.swf"], movie, icons)
    tags = swf_tags(movie)
    symbol_index = next(index for index, (code, _) in enumerate(tags) if code == 76)
    tags.insert(symbol_index, (82, native_runtime.build_movieclip_class_doabc(icons)))
    movie = rebuild(movie, tags)
    movie = verify_script_only(movie, native_runtime.replace_as3_classes(movie, bridges, closure.values()))
    movie = native_runtime.rename_as3_classes(movie, CLASS_PREFIX, ("scaleform.gfx", "Icon_mc", "SmallImage_mc"))
    source_text = (interface / "translate_en.txt").read_bytes()
    encoding = "utf-16" if source_text.startswith((b"\xff\xfe", b"\xfe\xff")) else "utf-8-sig"
    labels = [line for line in source_text.decode(encoding).splitlines()
              if line.startswith(("$Challenge", "$CHALLENGE")) or line.split("\t", 1)[0] in
              {"$TRACK", "$UNTRACK", "$BACK", "$DAILY", "$WEEKLY", "$LIFETIME", "$Caps", "$Experience"}]
    output = Path(output_data) / OUTPUT
    output.mkdir(parents=True, exist_ok=True)
    manifest = {"schema_version": 1, "source": MENU, "source_class": "SeventySixMenuChallenges",
                "menu": (OUTPUT / MENU).as_posix(), "class_prefix": CLASS_PREFIX,
                "source_sha256": hashlib.sha256(closure[MENU]).hexdigest(),
                "bridge_sha256": {name: hashlib.sha256(bridges[name].encode()).hexdigest() for name in BRIDGES},
                "code_object": "root1", "reward_icons": ["IconCR_Caps", "IconCR_Experience"], "files": []}
    for name, source in sorted(closure.items()):
        converted = remap_menu_fonts(movie if name == MENU else source)
        (output / name).write_bytes(converted)
        manifest["files"].append({"path": (OUTPUT / name).as_posix(),
                                  "source_sha256": hashlib.sha256(source).hexdigest(),
                                  "sha256": hashlib.sha256(converted).hexdigest()})
    merge_ui_translations(output_data, labels)
    merge_packaged(translation_path(output_data))
    (output / "conversion.json").write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    return manifest


if __name__ == "__main__":
    import argparse

    parser = argparse.ArgumentParser(description="Export FO76 challenge artwork for the FullScreenMap challenges panel")
    parser.add_argument("source_root", type=Path)
    parser.add_argument("output_mod", type=Path)
    args = parser.parse_args()
    print(json.dumps(convert_challenge_map_ui(args.source_root, args.output_mod), indent=2))
