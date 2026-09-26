from __future__ import annotations

import hashlib
import json
import struct
import zlib
from pathlib import Path

from bacup_lib.legendary_perks_ui import imports, remap_menu_fonts, verify_script_only, swf_tags
from bacup_lib.inspect_ui import rebuild
from bacup_lib.perk_cards_ui import embed_lettering_fonts, menu_translation_keys
from bacup_lib.translations import merge_ui_translations

MENU = "specialbuildsmenu.swf"
OUTPUT = Path("Interface/B21/TalesFromAppalachia/SpecialBuilds")
RESOURCES = Path(__file__).with_name("resources") / "special_builds"


def bind_button_bars(movie: bytes) -> bytes:
    if movie[:3] == b"CWS":
        movie = b"FWS" + movie[3:8] + zlib.decompress(movie[8:])
    tags = swf_tags(movie)
    definition_codes = {2, 6, 7, 10, 11, 14, 20, 21, 22, 32, 33, 34, 35, 36, 37, 39, 46, 48, 60, 62, 73, 75, 83, 84, 87, 88, 90, 91}
    next_id = max(int.from_bytes(payload[:2], "little") for code, payload in tags if code in definition_codes) + 1
    # FO76 class-only placements do not instantiate in older Scaleform players.
    symbols = {"Shared.AS3.BSButtonHintBar": next_id, "PerkLibraryImporter": next_id + 1}

    def placement(body: bytes) -> bytes:
        if not body[1] & 8 or body[0] & 2:
            return body
        end = body.index(b"\0", 4)
        name = body[4:end].decode()
        if name not in symbols:
            raise ValueError(f"Unexpected class-only SPECIAL component: {name}")
        return bytes([body[0] | 2, body[1] & ~8]) + body[2:4] + struct.pack("<H", symbols[name]) + body[end + 1:]

    converted = []
    for code, payload in tags:
        if code == 76:
            for name, cid in symbols.items():
                converted.append((39, struct.pack("<HHHH", cid, 1, 64, 0)))
            payload = struct.pack("<H", int.from_bytes(payload[:2], "little") + len(symbols)) + payload[2:] + b"".join(
                struct.pack("<H", cid) + name.encode() + b"\0" for name, cid in symbols.items())
        elif code == 39:
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
                if child_code == 70:
                    body = placement(body)
                nested.append(struct.pack("<HI", child_code << 6 | 63, len(body)) + body)
            payload = payload[:4] + b"".join(nested)
        elif code == 70:
            payload = placement(payload)
        elif code == 37:
            payload = payload.replace(b"$SPECIAL_LOADOUTS_SLOTPURCHASE", b"$B21_TFA_AddLoadout")
        converted.append((code, payload))
    return rebuild(movie, converted)


def convert_special_builds_ui(source_root: Path, output_data: Path) -> dict:
    interface = Path(source_root) / "interface"
    sources = {path.name.lower(): path for path in interface.iterdir() if path.is_file()}
    closure: dict[str, bytes] = {}
    pending = [MENU]
    while pending:
        name = pending.pop().lower()
        if name in closure:
            continue
        if "/" in name or "\\" in name or not name.endswith(".swf"):
            raise ValueError(f"Unsupported SPECIAL UI import: {name}")
        source = sources.get(name)
        if source is None:
            raise FileNotFoundError(f"Missing SPECIAL UI dependency: {name}")
        closure[name] = source.read_bytes()
        pending.extend(imports(closure[name]))

    from creation_lib.swf.native_runtime import augment_as3_classes, replace_as3_classes, patch_as3_method
    movie = replace_as3_classes(closure[MENU],
        {"SpecialBuilds.SpecialBuildsShared": (RESOURCES / "Shared.as").read_text(encoding="utf-8")}, list(closure.values()))
    movie = augment_as3_classes(movie, {
        "SpecialBuilds.SpecialBuildsMenu": (RESOURCES / "Bridge.as").read_text(encoding="utf-8"),
        "SpecialBuilds.EditSpecialModal": (RESOURCES / "EditBridge.as").read_text(encoding="utf-8")}, list(closure.values()))
    movie = patch_as3_method(movie, "SpecialBuilds.SpecialBuildsMenu", "$constructor",
        [["callpropvoid", "setTextAutoSize", 2]],
        [["keep", 0], ["getlocal", 0], ["callpropvoid", "B21Initialize", 0]])
    verify_script_only(closure[MENU], movie)
    movie = bind_button_bars(movie)

    output = Path(output_data) / OUTPUT
    output.mkdir(parents=True, exist_ok=True)
    manifest = {"schema_version": 1, "menu": (OUTPUT / MENU).as_posix(), "files": []}
    for name, source in sorted(closure.items()):
        converted = remap_menu_fonts(movie if name == MENU else source)
        if "fonts_en.swf" in closure:
            converted = embed_lettering_fonts(converted, closure["fonts_en.swf"])
        (output / name).write_bytes(converted)
        manifest["files"].append({"path": (OUTPUT / name).as_posix(),
            "source_sha256": hashlib.sha256(source).hexdigest(), "sha256": hashlib.sha256(converted).hexdigest()})
    (output / "conversion.json").write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")

    raw = (interface / "translate_en.txt").read_bytes()
    encoding = "utf-16" if raw.startswith((b"\xff\xfe", b"\xfe\xff")) else "utf-8-sig"
    shared = {"$UNLOCK", "$SET_ACTIVE", "$EDIT_ACTIVE_PERKS", "$RENAME", "$EDIT_SPECIAL", "$EXIT", "$ACCEPT",
        "$CANCEL", "$RESET", "$EDIT", "$STRENGTH", "$PERCEPTION", "$ENDURANCE", "$CHARISMA", "$INTELLIGENCE", "$AGILITY", "$LUCK"}
    shared = {key.casefold() for key in shared} | menu_translation_keys(list(closure.values()))
    lines = [line for line in raw.decode(encoding).splitlines()
        if line.split("\t", 1)[0].casefold() in shared or line.casefold().startswith(("$special", "$loadout", "$points_available"))]
    merge_ui_translations(output_data, lines)
    return manifest


if __name__ == "__main__":
    import argparse
    parser = argparse.ArgumentParser()
    parser.add_argument("source_root", type=Path)
    parser.add_argument("output_data", type=Path)
    args = parser.parse_args()
    print(json.dumps(convert_special_builds_ui(args.source_root, args.output_data), indent=2))
