from __future__ import annotations

import hashlib
import json
from pathlib import Path

from bacup_lib.legendary_perks_ui import imports, remap_menu_fonts, verify_script_only
from bacup_lib.translations import TRANSLATIONS, merge_ui_translations, translation_path

MENU = "expeditionspostmatchmenu.swf"
OUTPUT = Path("Interface/B21/TalesFromAppalachia/Expeditions")
BRIDGE = Path(__file__).with_name("resources") / "expedition_results" / "ExpeditionsPostMatchMenu.as"
RESULT_TRANSLATION_KEYS = {
    "$EXIT",
    "$Expedition",
    "$REWARDS",
    "$XP",
    "$XPD_Pitt01_MissionTitle",
    "$XPD_Pitt01_OptionalObjective1",
    "$XPD_Pitt01_OptionalObjective2",
    "$XPD_Pitt01_OptionalObjective3",
    "$XPD_Pitt02_MissionTitle",
    "$XPD_Pitt02_OptionalObjective1",
    "$XPD_Pitt02_OptionalObjective2",
    "$XPD_Pitt02_OptionalObjective3",
    "$XPD_AC01_MissionTitle",
    "$XPD_AC01_OptionalObjective1",
    "$XPD_AC01_OptionalObjective2",
    "$XPD_AC01_OptionalObjective3",
    "$XPD_AC02_MissionTitle",
    "$XPD_AC02_OptionalObjective1",
    "$XPD_AC02_OptionalObjective2",
    "$XPD_AC02_OptionalObjective3",
    "$XPD_AC03_MissionTitle",
    "$XPD_AC03_OptionalObjective1",
    "$XPD_AC03_OptionalObjective2",
    "$XPD_AC03_OptionalObjective3",
}


def menu_translations(interface: Path) -> dict[str, str]:
    data = (interface / "translate_en.txt").read_bytes()
    encoding = "utf-16" if data.startswith((b"\xff\xfe", b"\xfe\xff")) else "utf-8-sig"
    translations = {
        key: value
        for line in data.decode(encoding).splitlines()
        if "\t" in line
        for key, value in [line.split("\t", 1)]
        if key in RESULT_TRANSLATION_KEYS
    }
    if set(translations) != RESULT_TRANSLATION_KEYS:
        raise ValueError("Expedition results translations are missing")
    return translations


def convert_expedition_results_ui(source_root: Path, output_data: Path) -> dict:
    from creation_lib.swf.native_runtime import replace_as3_classes

    interface = Path(source_root) / "interface"
    translations = menu_translations(interface)
    files = {path.name.casefold(): path for path in interface.iterdir() if path.is_file()}
    pending = [MENU]
    closure: dict[str, bytes] = {}
    while pending:
        name = pending.pop().casefold()
        if name in closure:
            continue
        if "/" in name or "\\" in name or not name.endswith(".swf"):
            raise ValueError(f"Unsupported Expedition results UI import: {name}")
        source = files.get(name)
        if source is None:
            raise FileNotFoundError(f"Missing Expedition results UI import: {interface / name}")
        closure[name] = source.read_bytes()
        pending.extend(imports(closure[name]))

    source = closure[MENU]
    bridge = BRIDGE.read_text(encoding="utf-8").replace(
        "__EXPEDITION_TRANSLATIONS__", json.dumps(translations, ensure_ascii=False, separators=(",", ":")),
    )
    converted = verify_script_only(source, replace_as3_classes(
        source, {"ExpeditionsPostMatchMenu": bridge}, list(closure.values())))
    output = Path(output_data) / OUTPUT
    output.mkdir(parents=True, exist_ok=True)
    manifest = {
        "schema_version": 1,
        "bridge_sha256": hashlib.sha256(BRIDGE.read_bytes()).hexdigest(),
        "menu": (OUTPUT / MENU).as_posix(),
        "code_object": "root1",
        "files": [],
    }
    for name, original in sorted(closure.items()):
        target = output / name
        artifact = remap_menu_fonts(converted if name == MENU else original)
        target.write_bytes(artifact)
        manifest["files"].append({
            "path": (OUTPUT / name).as_posix(),
            "source_sha256": hashlib.sha256(original).hexdigest(),
            "sha256": hashlib.sha256(artifact).hexdigest(),
        })
    (output / "conversion.json").write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    merge_ui_translations(output_data, [f"{key}\t{value}" for key, value in translations.items()])
    return manifest


if __name__ == "__main__":
    import argparse

    parser = argparse.ArgumentParser()
    parser.add_argument("source_root", type=Path)
    parser.add_argument("output_data", type=Path)
    args = parser.parse_args()
    print(json.dumps(convert_expedition_results_ui(args.source_root, args.output_data), indent=2))
