from __future__ import annotations

import hashlib
import json
import struct
import zlib
from pathlib import Path

from bacup_lib.translations import TRANSLATIONS, merge_ui_translations, translation_path, parse_lines

MENU = "legendaryperksmenu.swf"
OUTPUT = Path("Interface/B21/TalesFromAppalachia/LegendaryPerks")
BRIDGE = Path(__file__).with_name("resources") / "legendary_perks/MainTimeline.as"


def menu_translations(source_interface: Path) -> str:
    source = (source_interface / "translate_en.txt").read_bytes()
    encoding = "utf-16" if source.startswith((b"\xff\xfe", b"\xfe\xff")) else "utf-8-sig"
    prefixes = ("$LegendaryPerk", "$Perks_Rank\t", "$Perks_Cost\t", "$Perks_MaxRank\t",
                "$Perks_OwnedCount\t", "$Perks_Inspection_DuplicateCountFormat\t", "$RANKS\t",
                "$FILTER", "$TYPE_")
    lines = [line for line in source.decode(encoding).splitlines() if line.startswith(prefixes)]
    if not any(line.startswith("$LegendaryPerks\t") for line in lines):
        raise ValueError("Source legendary perk translations are missing")
    return "\n".join(lines) + "\n"


def swf_tags(data: bytes) -> list[tuple[int, bytes]]:
    if data[:3] == b"CWS":
        body = zlib.decompress(data[8:])
    elif data[:3] == b"FWS":
        body = data[8:]
    else:
        raise ValueError("Expected an FWS or CWS menu")
    if len(body) + 8 != struct.unpack_from("<I", data, 4)[0]:
        raise ValueError("SWF length mismatch")
    offset = (5 + 4 * (body[0] >> 3) + 7) // 8 + 4
    tags = []
    while offset < len(body):
        header, = struct.unpack_from("<H", body, offset)
        offset += 2
        size = header & 63
        if size == 63:
            size, = struct.unpack_from("<I", body, offset)
            offset += 4
        if offset + size > len(body):
            raise ValueError("Truncated SWF tag")
        tags.append((header >> 6, body[offset:offset + size]))
        offset += size
        if header >> 6 == 0:
            if offset != len(body):
                raise ValueError("Trailing data after SWF End tag")
            return tags
    raise ValueError("Missing SWF End tag")


def imports(data: bytes) -> list[str]:
    return [body.split(b"\0", 1)[0].decode("utf-8")
            for code, body in swf_tags(data) if code in (57, 71)]


def verify_script_only(source: bytes, converted: bytes) -> bytes:
    if [tag for tag in swf_tags(source) if tag[0] not in (72, 82)] != [
            tag for tag in swf_tags(converted) if tag[0] not in (72, 82)]:
        raise ValueError("UI conversion changed non-ActionScript tags")
    return converted


def remap_menu_fonts(data: bytes) -> bytes:
    tags = swf_tags(data)
    converted = [(code, payload.replace(b"$ChowderHead\0", b"$MAIN_Font\0")
                              .replace(b"$Brush_Script_Std\0", b"$MAIN_Font\0")
                              .replace(b"$Futura_Bold\0", b"$MAIN_Font_Bold\0")
                              .replace(b"$Futura_bold\0", b"$MAIN_Font_Bold\0")
                              .replace(b"$BerlinDemi\0", b"$MAIN_Font_Bold\0") if code == 37 else payload)
                 for code, payload in tags]
    if converted == tags:
        return data
    body = zlib.decompress(data[8:]) if data[:3] == b"CWS" else data[8:]
    frame_header_size = (5 + 4 * (body[0] >> 3) + 7) // 8 + 4
    body = body[:frame_header_size] + b"".join(
        struct.pack("<HI", code << 6 | 63, len(payload)) + payload for code, payload in converted)
    header = data[:4] + struct.pack("<I", len(body) + 8)
    return header + (zlib.compress(body) if data[:3] == b"CWS" else body)


def convert_legendary_perk_ui(
    source_root: Path, output_data: Path,
) -> dict:
    source_interface = Path(source_root) / "interface"
    translations = menu_translations(source_interface)
    source_files = {p.name.lower(): p for p in source_interface.iterdir() if p.is_file()}
    pending = [MENU]
    closure: dict[str, bytes] = {}
    while pending:
        name = pending.pop().lower()
        if name in closure:
            continue
        if "/" in name or "\\" in name or not name.endswith(".swf"):
            raise ValueError(f"Unsupported legendary UI import: {name}")
        source = source_files.get(name)
        if source is None:
            raise FileNotFoundError(f"Missing legendary UI dependency: {source_interface / name}")
        closure[name] = source.read_bytes()
        pending.extend(imports(closure[name]))

    from creation_lib.swf.native_runtime import replace_as3_classes, patch_as3_method
    patched = replace_as3_classes(closure[MENU],
        {"LegendaryPerksMenu_fla.MainTimeline": BRIDGE.read_text(encoding="utf-8")}, list(closure.values()))
    patched = patch_as3_method(patched, "LegendaryPerksMenu", "populateUpgradeModal",
        [["getlex", "Shared.GlobalFunc"], ["getscopeobject", None], ["getslot", None],
         ["callproperty", "CloneObject", 1]],
        [["getlex", "LegendaryPerksMenu_fla.MainTimeline"], ["keep", 1], ["keep", 2],
         ["callproperty", "B21CopyCard", 1]])
    verify_script_only(closure[MENU], patched)

    output = Path(output_data) / OUTPUT
    output.mkdir(parents=True, exist_ok=True)
    manifest = {"schema_version": 1, "bridge_sha256": hashlib.sha256(BRIDGE.read_bytes()).hexdigest(),
                "menu": (OUTPUT / MENU).as_posix(), "files": []}
    for name, source in sorted(closure.items()):
        converted = remap_menu_fonts(patched if name == MENU else source)
        destination = output / name
        destination.write_bytes(converted)
        manifest["files"].append({"path": (OUTPUT / name).as_posix(),
                                  "source_sha256": hashlib.sha256(source).hexdigest(),
                                  "sha256": hashlib.sha256(converted).hexdigest()})
    (output / "conversion.json").write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    merge_ui_translations(output_data, parse_lines(translations))
    return manifest
