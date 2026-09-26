from __future__ import annotations

import hashlib
import json
from pathlib import Path

from bacup_lib.casino_catalog import build_casino_catalog
from bacup_lib.legendary_perks_ui import imports, remap_menu_fonts, verify_script_only
from bacup_lib.translations import TRANSLATIONS, merge_ui_translations, translation_path


MENU = "casinomenu.swf"
ROOT_MOVIES = (MENU, "casinohudwidget.swf", "casinofanfare.swf")
OUTPUT = Path("Interface/B21/TalesFromAppalachia/Casino")
BRIDGE = Path(__file__).with_name("resources") / "casino/CasinoModalManager.as"


def convert_casino_ui(source_root: Path, output_data: Path) -> dict:
    source_root = Path(source_root)
    interface = source_root / "interface"
    files = {path.name.lower(): path for path in interface.iterdir() if path.is_file()}
    pending = list(ROOT_MOVIES)
    closure: dict[str, bytes] = {}
    while pending:
        name = pending.pop().lower()
        if name in closure:
            continue
        if "/" in name or "\\" in name or not name.endswith(".swf"):
            raise ValueError(f"Unsupported casino UI import: {name}")
        source = files.get(name)
        if source is None:
            raise FileNotFoundError(f"Missing casino UI dependency: {name}")
        closure[name] = source.read_bytes()
        pending.extend(imports(closure[name]))

    from creation_lib.swf.native_runtime import augment_as3_classes, patch_as3_method
    movie = augment_as3_classes(closure[MENU],
        {"CasinoModalManager": BRIDGE.read_text(encoding="utf-8")}, list(closure.values()))
    movie = patch_as3_method(movie, "CasinoModalManager", "$constructor",
        [["getlocal", 0], ["getproperty", "LuckyDiceGame_mc"]],
        [["keep", 0], ["keep", 1], ["getlocal", 0], ["callpropvoid", "B21Initialize", 0]])
    verify_script_only(closure[MENU], movie)

    output = Path(output_data) / OUTPUT
    output.mkdir(parents=True, exist_ok=True)
    manifest = {"schema_version": 1, "bridge_sha256": hashlib.sha256(BRIDGE.read_bytes()).hexdigest(),
                "menu": (OUTPUT / MENU).as_posix(), "files": []}
    for name, source in sorted(closure.items()):
        converted = remap_menu_fonts(movie if name == MENU else source)
        (output / name).write_bytes(converted)
        manifest["files"].append({"path": (OUTPUT / name).as_posix(),
            "source_sha256": hashlib.sha256(source).hexdigest(),
            "sha256": hashlib.sha256(converted).hexdigest()})
    catalog = build_casino_catalog(source_root)
    (output / "casino_catalog.json").write_text(json.dumps(catalog, indent=2) + "\n", encoding="utf-8")
    manifest["catalog"] = (OUTPUT / "casino_catalog.json").as_posix()
    (output / "conversion.json").write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")

    raw = (interface / "translate_en.txt").read_bytes()
    encoding = "utf-16" if raw.startswith((b"\xff\xfe", b"\xfe\xff")) else "utf-8-sig"
    shared = {"$PLACE_BET", "$UNDO", "$BACK", "$TRYAGAIN"}
    lines = [line for line in raw.decode(encoding).splitlines()
             if line.startswith("$CASINO_") or line.split("\t", 1)[0] in shared]
    if not any(line.startswith("$CASINO_HEADER_ROULETTE\t") for line in lines):
        raise ValueError("Source casino translations are missing")
    merge_ui_translations(output_data, lines)
    return manifest


if __name__ == "__main__":
    import argparse
    parser = argparse.ArgumentParser()
    parser.add_argument("source_root", type=Path)
    parser.add_argument("output_data", type=Path)
    args = parser.parse_args()
    print(json.dumps(convert_casino_ui(args.source_root, args.output_data), indent=2))
