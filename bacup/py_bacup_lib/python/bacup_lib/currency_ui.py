from __future__ import annotations

import hashlib
import json
from pathlib import Path

from bacup_lib.daily_ops_ui import empty_hud_timeline, replace_classes
from bacup_lib.legendary_perks_ui import imports, remap_menu_fonts

OUTPUT = Path("Interface/B21/TalesFromAppalachia/Currency")
BRIDGES = Path(__file__).with_name("resources") / "currency"
SOUNDS = tuple(Path(f"sound/fx/ui/caps/ui_caps_{cue}_0{variant}.xwm")
               for cue in ("appear", "disappear") for variant in (1, 2))


def convert_currency_ui(source_root: Path, output_data: Path) -> dict:
    interface = Path(source_root) / "interface"
    files = {p.name.casefold(): p for p in interface.iterdir() if p.is_file()}
    pending = ["hudmenu.swf", "currencyiconlibrary.swf", "challengerewardiconlibrary.swf"]
    closure = {}
    while pending:
        name = pending.pop().casefold()
        if name in closure:
            continue
        if "/" in name or "\\" in name or not name.endswith(".swf"):
            raise ValueError(f"Unsupported currency UI import: {name}")
        if name not in files:
            raise FileNotFoundError(f"Missing currency UI import: {name}")
        closure[name] = files[name].read_bytes()
        pending.extend(imports(closure[name]))
    replacements = {name: BRIDGES / f"{name}.as" for name in ("HUDMenu", "HUDCurrencyUpdatesWidget")}
    sounds = {path: (Path(source_root) / path).read_bytes() for path in SOUNDS}
    movie = replace_classes(empty_hud_timeline(closure["hudmenu.swf"]), replacements,
                            closure.values())
    output = Path(output_data) / OUTPUT
    output.mkdir(parents=True, exist_ok=True)
    manifest = {"schema_version": 1, "bridges": {name: hashlib.sha256(path.read_bytes()).hexdigest()
                                                 for name, path in replacements.items()}, "files": []}
    for name, original in sorted(closure.items()):
        converted = remap_menu_fonts(movie if name == "hudmenu.swf" else original)
        destination = "currencyhud.swf" if name == "hudmenu.swf" else name
        (output / destination).write_bytes(converted)
        manifest["files"].append({"path": (OUTPUT / destination).as_posix(), "source": name,
                                  "source_sha256": hashlib.sha256(original).hexdigest(),
                                  "sha256": hashlib.sha256(converted).hexdigest()})
    manifest["audio"] = []
    for path, data in sounds.items():
        target = Path(output_data) / path
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(data)
        manifest["audio"].append({"path": path.as_posix(), "sha256": hashlib.sha256(data).hexdigest()})
    (output / "conversion.json").write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    return manifest
