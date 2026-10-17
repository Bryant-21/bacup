from __future__ import annotations

import hashlib
import html
import json
import re
import struct
import zlib
from collections.abc import Iterable
from pathlib import Path

from bacup_lib.character_creation_ui import character_translation_keys
from bacup_lib.inspect_ui import rebuild
from bacup_lib.legendary_perks_ui import imports, remap_menu_fonts, swf_tags, verify_script_only
from bacup_lib.translations import merge_ui_translations, read_table

MENU = "workshopmenunew.swf"
OUTPUT = Path("Interface/B21/TalesFromAppalachia/CampBuild")
PREFIX = "B21TFA_CampBuild"
RESOURCES = Path(__file__).with_name("resources") / "workshop"
BRIDGES = {"WorkshopMenuNew": "CampBuildBridge.as",
           "Shared.AS3.Data.UIDataFromClient": "UIDataFromClient.as",
           "Shared.AS3.BSScrollingList": "BSScrollingList.as",
           "ItemsListEntry": "ItemsListEntry.as",
           "WorkshopItemCardListEntry": "WorkshopItemCardListEntry.as"}


def source_closure(interface: Path) -> dict[str, bytes]:
    files = {path.name.casefold(): path for path in interface.iterdir() if path.is_file()}
    pending, closure = [MENU], {}
    while pending:
        name = pending.pop().casefold()
        if name in closure:
            continue
        if "/" in name or "\\" in name or ":" in name or not name.endswith(".swf"):
            raise ValueError(f"Unsupported CAMP build import: {name}")
        if name not in files:
            raise FileNotFoundError(f"Missing CAMP build dependency: {interface / name}")
        closure[name] = files[name].read_bytes()
        pending.extend(imports(closure[name]))
    return closure


def private_imports(data: bytes) -> bytes:
    if data[:3] == b"CWS":
        data = b"FWS" + data[3:8] + zlib.decompress(data[8:])
    tags = []
    for code, payload in swf_tags(data):
        if code in (57, 71):
            name, tail = payload.split(b"\0", 1)
            payload = name.lower() + b"\0" + tail
        tags.append((code, payload))
    return rebuild(data, tags)


def private_names(data: bytes, names: dict[str, str]) -> bytes:
    from bacup_lib.status_hud_source import sprite_tags, tag

    def u30(payload, cursor):
        value, shift = 0, 0
        while True:
            byte = payload[cursor]
            cursor += 1
            value |= (byte & 127) << shift
            if byte < 128:
                return value, cursor
            shift += 7

    def encoded(value):
        result = bytearray()
        while value >= 128:
            result.append((value & 127) | 128)
            value >>= 7
        result.append(value)
        return result

    def abc(payload, code):
        cursor = payload.index(b'\0', 4) + 5 if code == 82 else 4
        for _ in range(2):
            count, cursor = u30(payload, cursor)
            for _ in range(max(0, count - 1)):
                _, cursor = u30(payload, cursor)
        count, cursor = u30(payload, cursor)
        cursor += max(0, count - 1) * 8
        count, cursor = u30(payload, cursor)
        result = bytearray(payload[:cursor])
        for _ in range(max(0, count - 1)):
            size, cursor = u30(payload, cursor)
            value = payload[cursor:cursor + size].decode('utf-8')
            cursor += size
            # This source class also names the native data channel, rather than a dynamic class lookup.
            value = (value if value == 'WorkshopItemCard' else names.get(value, value)).encode('utf-8')
            result.extend(encoded(len(value)))
            result.extend(value)
        return bytes(result) + payload[cursor:]

    def rename(code, payload):
        if code in (72, 82):
            return abc(payload, code)
        if code == 39:
            return payload[:4] + b''.join(tag(kind, rename(kind, body)) for kind, body in sprite_tags(payload))
        if code == 70 and (payload[1] & 8 or payload[1] & 16 and payload[0] & 2):
            end = payload.index(b'\0', 4)
            name = payload[4:end].decode('utf-8')
            return payload[:4] + names.get(name, name).encode() + payload[end:]
        if code in (57, 71):
            cursor = payload.index(b'\0') + 1 + (2 if code == 71 else 0)
            count, = struct.unpack_from('<H', payload, cursor)
            cursor += 2
            result = bytearray(payload[:cursor])
            for _ in range(count):
                end = payload.index(b'\0', cursor + 2)
                name = payload[cursor + 2:end].decode('utf-8')
                result.extend(payload[cursor:cursor + 2] + names.get(name, name).encode() + b'\0')
                cursor = end + 1
            return bytes(result) + payload[cursor:]
        return payload

    if data[:3] == b'CWS':
        data = b'FWS' + data[3:8] + zlib.decompress(data[8:])
    return rebuild(data, [(code, rename(code, payload)) for code, payload in swf_tags(data)])


def private_class_names(classes: Iterable[str]) -> dict[str, str]:
    # FO76's fonts_en.swf names each font class after its `$` alias. Renaming one also rewrites every
    # runtime TextFormat font string to a name no font has, and GFx draws that text as boxes.
    names = {name: PREFIX + ('.' if '.' in name else '_') + name for name in classes
             if not name.startswith(('scaleform.gfx.', '$'))}
    # Item card rows look up their resource icon class as "WS_icon_" + actor value.
    names["WS_icon_"] = f"{PREFIX}_WS_icon_"
    return names


def private_movie(data: bytes, names: dict[str, str]) -> bytes:
    from creation_lib.swf import native_runtime

    classes = native_runtime.abc_class_names(data)
    kept = sorted(name for name in classes if name not in names)
    # Native class renaming leaves dynamic class strings and imported timeline bindings intact.
    if len(kept) < len(classes):
        data = native_runtime.rename_as3_classes(data, PREFIX, ("scaleform.gfx", *kept))
    return private_imports(private_names(remap_menu_fonts(data), names))


def translation_keys(data: bytes) -> set[str]:
    keys = character_translation_keys(data)
    for code, payload in swf_tags(data):
        if code == 37:
            # The translator looks up a field's whole text, and FO76 keys may contain spaces.
            text = html.unescape(re.sub(r"<[^>]*>", "", payload.split(b"\0")[-2].decode("utf-8", "replace")))
            if text.startswith("$"):
                keys.add(text)
    return keys


def repair_mode_header(data: bytes) -> bytes:
    from bacup_lib.status_hud_source import sprite_tags, tag
    from creation_lib.swf import native_runtime

    header = next((character for character, name in native_runtime.list_symbols(data)
                   if name == "WorkshopNewLibrary_fla.LeftPanel_Title_Entry_40"), None)
    if header is None:
        return data
    if data[:3] == b'CWS':
        data = b'FWS' + data[3:8] + zlib.decompress(data[8:])
    entries = []
    for code, payload in swf_tags(data):
        if code == 39 and struct.unpack_from('<H', payload)[0] == header:
            # FO76 labels the expanded two-line Modify frame as a second minimized frame.
            payload = payload[:4] + b''.join(tag(kind, b'modifyModeTwoLine_Expanded\0'
                if kind == 43 and body == b'modifyModeTwoLined_Minimized\0' else body)
                for kind, body in sprite_tags(payload))
        entries.append((code, payload))
    return rebuild(data, entries)


def convert_camp_build_ui(source_root: Path, output_data: Path) -> dict:
    from creation_lib.swf import native_runtime

    interface = Path(source_root) / "interface"
    closure = source_closure(interface)
    bridge_sources = {name: (RESOURCES / path).read_text(encoding="utf-8") for name, path in BRIDGES.items()}
    source = closure[MENU]
    patched = {}
    for name, data in closure.items():
        defined = set(native_runtime.abc_class_names(data))
        bridges = {name: text for name, text in bridge_sources.items() if name in defined}
        movie = native_runtime.augment_as3_classes(data, bridges, list(closure.values())) if bridges else data
        if 'WorkshopMenuNew' in defined:
            movie = native_runtime.patch_as3_method(movie, 'WorkshopMenuNew', 'onAddedToStage',
                [['getproperty', 'onWorkshopCategoriesDataUpdate']],
                [['getproperty', 'B21CategoriesUpdate']])
        if 'ItemsListEntry' in defined:
            movie = native_runtime.patch_as3_method(movie, "ItemsListEntry", "SetEntryText", [["getproperty", "VariantIndicator_mc"]],
                [["getlocal", 1], ["callpropvoid", "B21ShowName", 1], ["getlocal", 0], ["keep", 0]])
            movie = native_runtime.patch_as3_method(movie, "ItemsListEntry", "setDisplayState",
                [["callpropvoid", "gotoAndStop", 1]], [["callpropvoid", "B21GotoState", 1]], expected_matches=5)
        if 'WorkshopItemCardListEntry' in defined:
            movie = native_runtime.patch_as3_method(movie, 'WorkshopItemCardListEntry', 'SetEntryText',
                [['getlocal', 0], ['getproperty', 'Icon_mc'], ['callpropvoid', 'removeChildren', 0]],
                [['getlocal', 0], ['callpropvoid', 'B21ClearIcon', 0]])
        if 'WorkshopItemCard' in defined:
            # Its card calls a non-public list helper; a class-local bridge keeps that lookup valid after conversion.
            movie = native_runtime.patch_as3_method(movie, 'WorkshopItemCard', 'refreshData',
                [['callproperty', 'GetEntryHeight', 1]], [['callproperty', 'B21EntryHeight', 1]])
        patched[name] = repair_mode_header(verify_script_only(data, movie))
    # Bridge code and static field text carry keys the source ActionScript never names.
    keys = set().union(*(translation_keys(data) for data in patched.values()))
    translations = [line for line in read_table(interface / "translate_en.txt") if line.split("\t", 1)[0] in keys]
    names = private_class_names(name for data in patched.values() for name in native_runtime.abc_class_names(data))
    outputs ={OUTPUT / name: private_movie(data, names) for name, data in patched.items()}
    document = next(name for character, name in native_runtime.list_symbols(outputs[OUTPUT / MENU]) if character == 0)
    traits = native_runtime.abc_class_outline(outputs[OUTPUT / MENU], document)["instance_traits"]
    signatures = {trait["name"]: trait.get("signature") for trait in traits}
    expected = {"B21Start": (["Object"], "Boolean"), "B21ApplyFrame": (["Object"], "Boolean"),
                "B21SetMode": (["uint"], "void"), "B21SetPlatform": (["uint"], "void"),
                "B21ProcessInput": (["String", "Boolean"], "Boolean"), "B21Stop": ([], "void")}
    expected['B21SetCoreHints'] = (["Boolean", "Boolean", "String"], "void")
    for name, (params, returns) in expected.items():
        signature = signatures.get(name)
        if not signature or (signature["params"], signature["returns"]) != (params, returns):
            raise ValueError(f"Converted CAMP build bridge has an incompatible {name}")
    manifest = {"schema_version": 1, "bridge_version": 14, "menu": (OUTPUT / MENU).as_posix(),
                "document_class": document, "class_prefix": PREFIX,
                "source_sha256": hashlib.sha256(source).hexdigest(),
                "bridge_sha256": {name: hashlib.sha256(text.encode()).hexdigest() for name, text in bridge_sources.items()},
                "catalog": "live Fallout 4 workshop tree", "files": []}
    for path, data in outputs.items():
        destination = Path(output_data) / path
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_bytes(data)
        manifest["files"].append({"path": path.as_posix(), "sha256": hashlib.sha256(data).hexdigest()})
    merge_ui_translations(Path(output_data), translations)
    (Path(output_data) / OUTPUT / "conversion.json").write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    return manifest


if __name__ == "__main__":
    import argparse
    parser = argparse.ArgumentParser()
    parser.add_argument("source_root", type=Path)
    parser.add_argument("output_data", type=Path)
    args = parser.parse_args()
    manifest = convert_camp_build_ui(args.source_root, args.output_data)
    print(json.dumps({"menu": manifest["menu"], "document_class": manifest["document_class"],
                      "files": len(manifest["files"])}))
