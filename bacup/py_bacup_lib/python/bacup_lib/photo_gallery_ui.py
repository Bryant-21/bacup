from __future__ import annotations

import hashlib
import json
from pathlib import Path

from bacup_lib.legendary_perks_ui import remap_menu_fonts, verify_script_only
from bacup_lib.translations import TRANSLATIONS, merge_ui_translations
from bacup_lib.ui_contract import resolve_numbered_name
from creation_lib.swf import native_runtime

MENU = "photogallerymenu.swf"
SCALEFORM_OUTPUT = Path("data/Interface/B21/TalesFromAppalachia/PhotoGallery")
MAP_OUTPUT = Path("F4SE/Plugins/B21_FullScreenMap/photo-gallery")
RESOURCES = Path(__file__).with_name("resources") / "photo_gallery"
TRANSLATION_KEYS = {"$MainMenuPhotoGallery", "$MainMenuPhotoGalleryTooltip"}


def _translations(interface: Path) -> list[str]:
    data = (interface / "translate_en.txt").read_bytes()
    encoding = "utf-16" if data.startswith((b"\xff\xfe", b"\xfe\xff")) else "utf-8-sig"
    lines = [line for line in data.decode(encoding).splitlines()
             if line.split("\t", 1)[0] in TRANSLATION_KEYS]
    if {line.split("\t", 1)[0] for line in lines} != TRANSLATION_KEYS:
        raise ValueError("Photo gallery translations are missing")
    return lines


def convert_photo_gallery_ui(source_root: Path, output_root: Path) -> dict:
    source_root, output_root = Path(source_root), Path(output_root)
    interface = source_root / "interface"
    source_path = interface / MENU
    if not source_path.is_file():
        raise FileNotFoundError(f"Missing photo gallery UI source: {source_path}")
    source = source_path.read_bytes()
    contract = json.loads((RESOURCES / "presentation.json").read_text(encoding="utf-8"))
    classes = set(native_runtime.abc_class_names(source))
    symbols = {name for _, name in native_runtime.list_symbols(source)}
    resolved_generated_symbols = [
        resolve_numbered_name(symbols, stem, "photo-gallery presentation symbol")
        for stem in contract["generated_symbol_stems"]
    ]
    if contract["document_class"] not in classes or not set(contract["symbols"]) <= symbols:
        raise ValueError("Photo gallery source presentation does not match the expected FO76 movie")
    presentation_symbols = [*contract["symbols"], *resolved_generated_symbols]

    converted = verify_script_only(source, remap_menu_fonts(source))
    scaleform = output_root / SCALEFORM_OUTPUT
    map_output = output_root / MAP_OUTPUT
    scaleform.mkdir(parents=True, exist_ok=True)
    map_output.mkdir(parents=True, exist_ok=True)
    target = scaleform / MENU
    target.write_bytes(converted)
    manifest = {
        "schema_version": 1,
        "source": f"interface/{MENU}",
        "source_sha256": hashlib.sha256(source).hexdigest(),
        "sha256": hashlib.sha256(converted).hexdigest(),
        "menu": (SCALEFORM_OUTPUT.relative_to("data") / MENU).as_posix(),
        "runtime_view": MAP_OUTPUT.as_posix(),
        "stage": contract["stage"],
        "document_class": contract["document_class"],
        "symbols": presentation_symbols,
        "generated_symbol_stems": contract["generated_symbol_stems"],
        "presentation_preserved": True,
    }
    payload = json.dumps(manifest, indent=2) + "\n"
    (scaleform / "conversion.json").write_text(payload, encoding="utf-8")
    (map_output / "presentation.json").write_text(payload, encoding="utf-8")
    merge_ui_translations(output_root / "data", _translations(interface))
    return manifest


if __name__ == "__main__":
    import argparse

    parser = argparse.ArgumentParser()
    parser.add_argument("source_root", type=Path)
    parser.add_argument("output_root", type=Path)
    args = parser.parse_args()
    print(json.dumps(convert_photo_gallery_ui(args.source_root, args.output_root), indent=2))
