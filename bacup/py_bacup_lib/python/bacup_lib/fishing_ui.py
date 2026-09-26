from __future__ import annotations

import hashlib
import json
from pathlib import Path

from bacup_lib.legendary_perks_ui import imports, remap_menu_fonts, verify_script_only
from bacup_lib.translations import TRANSLATIONS, merge_ui_translations, translation_path


OUTPUT = Path("Interface/B21/TalesFromAppalachia/Fishing")
BRIDGE = Path(__file__).with_name("resources") / "fishing/Bridge.as"


def convert_fishing_ui(source_root: Path, output_data: Path) -> dict:
    interface = Path(source_root) / "interface"
    source_text = (interface / "translate_en.txt").read_bytes()
    encoding = "utf-16" if source_text.startswith((b"\xff\xfe", b"\xfe\xff")) else "utf-8-sig"
    shared_labels = {"$ROTATE", "$SELECT", "$EXIT", "$INSPECT", "$CONTINUE", "$WORKSHOPTOGGLEFREECAM"}
    translations = [line for line in source_text.decode(encoding).splitlines()
                    if line.startswith("$FISHING") or line.split("\t", 1)[0] in shared_labels]
    if not any(line.startswith("$FISHING_CAST\t") for line in translations):
        raise ValueError("Source fishing translations are missing")
    translations.extend([
        "$B21_FISHING_WAIT_FOR_BITE\tWAIT FOR A BITE",
        "$B21_FISHING_HOOK_NOW\tFISH BITING! REEL IN",
    ])
    source_files = {path.name.lower(): path for path in interface.iterdir() if path.is_file()}
    pending = ["fishingmenu.swf"]
    closure = {}
    while pending:
        name = pending.pop().lower()
        if name in closure:
            continue
        if "/" in name or "\\" in name or not name.endswith(".swf"):
            raise ValueError(f"Unsupported fishing UI import: {name}")
        closure[name] = source_files[name].read_bytes()
        pending.extend(imports(closure[name]))

    from creation_lib.swf.native_runtime import augment_as3_classes, patch_as3_method
    movie = augment_as3_classes(closure["fishingmenu.swf"],
        {"FishingMenu": BRIDGE.read_text(encoding="utf-8")}, list(closure.values()))
    movie = patch_as3_method(movie, "FishingMenu", "$constructor",
        [["getlocal", 0], ["getlex", "GAME_STATE_WAITING"], ["callpropvoid", "setGameState", 1]],
        [["getlocal", 0], ["callpropvoid", "B21Initialize", 0], ["keep", 0], ["keep", 1], ["keep", 2]])
    movie = patch_as3_method(movie, "FishingMenu", "onBaitSelectionChanged",
        [["getlocal", 0], ["getproperty", "ItemDataA"], ["getlocal", 0],
         ["getproperty", "FishingData"], ["getproperty", "selectedBaitIndex"],
         ["getproperty", "*"], ["iffalse", None]],
        [["getlocal", 0], ["callproperty", "B21HasSelectedBait", 0], ["keep", 6]])
    verify_script_only(closure["fishingmenu.swf"], movie)

    output = Path(output_data) / OUTPUT
    output.mkdir(parents=True, exist_ok=True)
    manifest = {"schema_version": 1, "bridge_sha256": hashlib.sha256(BRIDGE.read_bytes()).hexdigest(),
                "menu": (OUTPUT / "fishingmenu.swf").as_posix(), "files": []}
    for name, source in sorted(closure.items()):
        converted = remap_menu_fonts(movie if name == "fishingmenu.swf" else source)
        (output / name).write_bytes(converted)
        manifest["files"].append({"path": (OUTPUT / name).as_posix(), "source_sha256": hashlib.sha256(source).hexdigest(),
                                  "sha256": hashlib.sha256(converted).hexdigest()})
    (output / "conversion.json").write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    merge_ui_translations(output_data, translations)
    return manifest
