from __future__ import annotations

import hashlib
import json
from pathlib import Path

from bacup_lib.daily_ops_ui import replace_classes
from bacup_lib.legendary_perks_ui import imports, remap_menu_fonts

MENU = "keypadmenu.swf"
OUTPUT = Path("Interface/B21/TalesFromAppalachia/Keypad")
BRIDGE = Path(__file__).with_name("resources") / "keypad/KeypadMenu.as"


def convert_keypad_ui(source_root: Path, output_data: Path) -> dict:
    interface = Path(source_root) / "interface"
    files = {p.name.casefold(): p for p in interface.iterdir() if p.is_file()}
    pending = [MENU]
    closure = {}
    while pending:
        name = pending.pop().casefold()
        if name in closure:
            continue
        if "/" in name or "\\" in name or not name.endswith(".swf"):
            raise ValueError(f"Unsupported keypad UI import: {name}")
        if name not in files:
            raise FileNotFoundError(f"Missing keypad UI import: {name}")
        closure[name] = files[name].read_bytes()
        pending.extend(imports(closure[name]))

    movie = replace_classes(closure[MENU], {"KeypadMenu": BRIDGE}, closure.values())

    output = Path(output_data) / OUTPUT
    output.mkdir(parents=True, exist_ok=True)
    manifest = {"schema_version": 1, "bridge_sha256": hashlib.sha256(BRIDGE.read_bytes()).hexdigest(),
                "menu": (OUTPUT / MENU).as_posix(), "code_object": "root1.Menu_mc", "files": []}
    for name, source in sorted(closure.items()):
        converted = remap_menu_fonts(movie if name == MENU else source)
        (output / name).write_bytes(converted)
        manifest["files"].append({"path": (OUTPUT / name).as_posix(),
                                  "source_sha256": hashlib.sha256(source).hexdigest(),
                                  "sha256": hashlib.sha256(converted).hexdigest()})
    (output / "conversion.json").write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    return manifest


if __name__ == "__main__":
    import argparse
    parser = argparse.ArgumentParser()
    parser.add_argument("source_root", type=Path)
    parser.add_argument("output_data", type=Path)
    args = parser.parse_args()
    print(json.dumps(convert_keypad_ui(args.source_root, args.output_data), indent=2))
