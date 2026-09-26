from __future__ import annotations

import hashlib
import json
import re
from pathlib import Path

from bacup_lib.legendary_perks_ui import imports, remap_menu_fonts, swf_tags
from bacup_lib.status_hud_source import replace_tags, sprite_tags
from bacup_lib.translations import merge_ui_translations, read_table
from creation_lib.swf import native_runtime


MENU = "looksmenu.swf"
OUTPUT = Path("Interface/B21/TalesFromAppalachia/CharacterCreation")
BRIDGE = Path(__file__).with_name("resources") / "character_creation/LooksMenu.as"


def build_character_creation(source: bytes, dependencies: list[bytes]) -> bytes:
    methods = native_runtime.abc_disassemble(source, "LooksMenu")
    movie = native_runtime.augment_as3_classes(
        source, {"LooksMenu": BRIDGE.read_text(encoding="utf-8")}, dependencies)
    movie = native_runtime.patch_as3_method(movie, "LooksMenu", "$constructor",
        [["callpropvoid", "UpdateButtons", 0]],
        [["keep", 0], ["getlocal", 0], ["callpropvoid", "B21InitializeFO4", 0]])
    movie = native_runtime.patch_as3_method(movie, "LooksMenu", "ProcessUserEvent",
        [["getproperty", "InputFunctionsA"]],
        [["getlocal", 1], ["getlocal", 2], ["callproperty", "B21InputFunctions", 2]])
    movie = native_runtime.patch_as3_method(movie, "LooksMenu", "ProcessUserEvent",
        [["getproperty", "UIHidden"]],
        [["getlocal", 1], ["getlocal", 2], ["callproperty", "B21FilterNameInput", 2]])
    movie = native_runtime.augment_as3_classes(movie, {"Shared.AS3.IMenu":
        "package Shared.AS3 { public class IMenu extends BSDisplayObject {"
        "public function B21SetFO4Platform(platform:uint):void {"
        "_uiPlatform=platform; _bIsGen9=false; _uiController=platform; _uiKeyboard=0;"
        "dispatchEvent(new Shared.AS3.Events.PlatformChangeEvent(platform,false,platform,0)); } } }"},
        dependencies)
    for method in methods:
        label = method["method"]
        if label in ("constructor", "static initializer"):
            continue
        name = label.removeprefix("get ").removeprefix("set ")
        code = "\n".join(method["code"])
        for original, replacement in (("FacialBoneRegions", "B21BoneRegions"),
                                      ("BGSCodeObj", "B21Native")):
            if "GetProperty " + original in code:
                movie = native_runtime.patch_as3_method(movie, "LooksMenu", name,
                    [["getproperty", original]], [["callproperty", replacement, 0]],
                    expected_matches=sum(line.endswith("GetProperty " + original)
                                         for line in method["code"]))
    if native_runtime.unbacked_symbol_classes(movie):
        raise ValueError("Character creation has unbacked symbols")
    return movie


def character_translation_keys(data: bytes) -> set[str]:
    keys = {value for *_, strings in native_runtime.abc_string_pools(data)
            for value in strings if value.startswith("$")}
    pending = list(swf_tags(data))
    while pending:
        code, payload = pending.pop()
        if code == 39:
            pending.extend(sprite_tags(payload))
        elif code == 37:
            # DefineEditText ends with its variable name and optional initial text.
            keys.update(key.decode("ascii") for key in
                        re.findall(rb"\$[A-Za-z_][A-Za-z0-9_]*", payload.split(b"\0")[-2]))
    return keys


def convert_character_creation_ui(source_root: Path, output_data: Path) -> dict:
    interface = Path(source_root) / "interface"
    closure: dict[str, bytes] = {}
    pending = [MENU]
    while pending:
        name = pending.pop().casefold()
        if name in closure:
            continue
        if "/" in name or "\\" in name or not name.endswith(".swf"):
            raise ValueError(f"Unsupported character creation import: {name}")
        closure[name] = (interface / name).read_bytes()
        pending.extend(imports(closure[name]))
    source = closure[MENU]
    movie = build_character_creation(source, list(closure.values()))
    outputs = {OUTPUT / name: remap_menu_fonts(movie if name == MENU else data)
               for name, data in closure.items()}
    # LoadMovie keeps FO4's native LooksMenu and its code object; only the movie changes.
    route = Path("Interface/B21_TFACharacterCreation.swf")
    tags = [(code, (OUTPUT.relative_to("Interface").as_posix() + "/" +
             payload.split(b"\0", 1)[0].decode()).encode() +
             payload[len(payload.split(b"\0", 1)[0]):]) if code in (57, 71) else (code, payload)
            for code, payload in swf_tags(movie)]
    outputs[route] = remap_menu_fonts(replace_tags(movie, tags))
    keys = set().union(*(character_translation_keys(data) for data in closure.values()))
    # Currency glyphs need the currency converter's font mapping; the offline creator has no store.
    keys.difference_update({"$AtomsGlyph", "$ZEUSGLYPH"})
    translations = [line for line in read_table(interface / "translate_en.txt")
                    if line.split("\t", 1)[0] in keys]
    manifest = {"schema_version": 1, "bridge_version": 3, "menu": route.as_posix(),
                "source_sha256": hashlib.sha256(source).hexdigest(),
                "controls": ["presets", "sex", "face", "sculpt", "hair", "extras", "body", "name"],
                "files": [{"path": path.as_posix(), "sha256": hashlib.sha256(data).hexdigest()}
                          for path, data in outputs.items()]}
    for path, data in outputs.items():
        target = Path(output_data) / path
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(data)
    (Path(output_data) / OUTPUT / "conversion.json").write_text(
        json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    merge_ui_translations(Path(output_data), translations)
    return manifest


if __name__ == "__main__":
    import argparse
    parser = argparse.ArgumentParser()
    parser.add_argument("source_root", type=Path)
    parser.add_argument("output_data", type=Path)
    args = parser.parse_args()
    print(json.dumps(convert_character_creation_ui(args.source_root, args.output_data), indent=2))
