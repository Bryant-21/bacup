from __future__ import annotations

import hashlib
import json
from pathlib import Path

from bacup_lib.legendary_perks_ui import imports, remap_menu_fonts, verify_script_only
from bacup_lib.translations import merge_ui_translations, parse_lines

MENU = "vatsmenu.swf"
OUTPUT = Path("Interface/B21/TalesFromAppalachia/RealtimeVATS")
RESOURCES = Path(__file__).with_name("resources") / "realtime_vats"
TRANSLATION_KEYS = {"$RETURN", "$PART", "$TARGET", "$CRITICAL", "$ABORT", "$LEVEL"}


def convert_realtime_vats_ui(source_root: Path, output_data: Path) -> dict:
    from creation_lib.swf import native_runtime as native

    interface = Path(source_root) / "interface"
    files = {p.name.casefold(): p for p in interface.iterdir() if p.is_file()}
    pending = [MENU]
    closure = {}
    while pending:
        name = pending.pop().casefold()
        if name in closure:
            continue
        if "/" in name or "\\" in name or not name.endswith(".swf"):
            raise ValueError(f"Unsupported VATS import: {name}")
        if name not in files:
            raise FileNotFoundError(f"Missing VATS source dependency: {name}")
        closure[name] = files[name].read_bytes()
        pending.extend(imports(closure[name]))

    raw = (interface / "translate_en.txt").read_bytes()
    encoding = "utf-16" if raw.startswith((b"\xff\xfe", b"\xfe\xff")) else "utf-8-sig"
    translations = [line for line in parse_lines(raw.decode(encoding))
                    if line.split("\t", 1)[0] in TRANSLATION_KEYS]
    if {line.split("\t", 1)[0] for line in translations} != TRANSLATION_KEYS:
        raise ValueError("Missing VATS translations")

    bridges = {"VATSMenu.as": (RESOURCES / "VATSMenu.as").read_text(encoding="utf-8")}
    movie = native.augment_as3_classes(closure[MENU], {"VATSMenu": bridges["VATSMenu.as"]}, closure.values())
    movie = native.patch_as3_method(movie, "VATSMenu", "$constructor",
        [["getlex", "Shared.AS3.Data.BSUIDataManager"]], [["getlocal", 0]])
    movie = native.patch_as3_method(movie, "VATSMenu", "$constructor",
        [["callpropvoid", "Subscribe", 2]], [["callpropvoid", "B21Initialize", 2]])
    verify_script_only(closure[MENU], movie)
    outputs = {OUTPUT / name: remap_menu_fonts(movie if name == MENU else source)
               for name, source in closure.items()}
    manifest = {
        "schema_version": 1, "bridge_version": 2, "menu": (OUTPUT / MENU).as_posix(),
        "code_object": "root.MenuInstance", "frame_method": "B21SetFrame",
        "platform_method": "root.SetPlatform",
        "platform_arguments": ["uiPlatform", "bIsGen9", "uiController", "uiKeyboard"],
        "activation": "requires_verified_native_adapter", "replaces_stock_menu": False,
        "bridges": {name: hashlib.sha256(text.encode()).hexdigest() for name, text in bridges.items()},
        "files": [{"path": path.as_posix(), "sha256": hashlib.sha256(data).hexdigest(),
                   "source_sha256": hashlib.sha256(closure[path.name]).hexdigest()}
                  for path, data in sorted(outputs.items())],
    }
    for path, data in outputs.items():
        destination = Path(output_data) / path
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_bytes(data)
    merge_ui_translations(output_data, translations)
    (Path(output_data) / OUTPUT / "conversion.json").write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    return manifest


if __name__ == "__main__":
    import argparse
    parser = argparse.ArgumentParser()
    parser.add_argument("source_root", type=Path)
    parser.add_argument("output_data", type=Path)
    args = parser.parse_args()
    print(json.dumps(convert_realtime_vats_ui(args.source_root, args.output_data), indent=2))
