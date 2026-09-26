from __future__ import annotations

import hashlib
import json
from pathlib import Path

from bacup_lib.legendary_perks_ui import imports, remap_menu_fonts, verify_script_only
from bacup_lib.translations import merge_ui_translations, read_table
from creation_lib.swf import native_runtime

MENU = "selfiemenu.swf"
OUTPUT = Path("Interface/B21/TalesFromAppalachia/PhotoMode")
BRIDGE = Path(__file__).with_name("resources") / "photo_mode/SelfieMenu.as"
CONTROLS = {
    "Lens": ["Field of View", "View Roll", "Camera Speed", "Depth of Field", "Strength", "Distance", "Range"],
    "Player": ["Show Player", "Expression", "Pose Category", "Pose", "Vanity Light Style", "Vanity Light Strength"],
    "Cinematic": ["Brightness", "Saturation", "Contrast"],
    "Filters": ["Filter", "Texture Category", "Texture"],
    "Overlays": ["Frame Category", "Frame"],
}
# PMFT 4E84D9/4E84DA, inspected from the installed source ESM.
FRAMES = ("textures/interface/photomode/frame/photomode_photograph01_d.dds",
          "textures/interface/photomode/frame/photomode_photograph02_d.dds")


def convert_photo_mode_ui(source_root: Path, output_data: Path) -> dict:
    source_root, output_data = Path(source_root), Path(output_data)
    interface = source_root / "interface"
    closure = {}
    pending = [MENU]
    while pending:
        name = pending.pop().lower()
        if name in closure:
            continue
        if "/" in name or "\\" in name or not name.endswith(".swf"):
            raise ValueError(f"Unsupported PhotoMode import: {name}")
        closure[name] = (interface / name).read_bytes()
        pending.extend(imports(closure[name]))
    source = closure[MENU]
    if "SelfieMenu" not in native_runtime.abc_class_names(source):
        raise ValueError("PhotoMode document class missing")
    initialize = native_runtime.abc_disassemble(source, "SelfieMenu", "Initialize")
    instructions = "\n".join(initialize[0]["code"])
    for controls in CONTROLS.values():
        for control in controls:
            if f'PushString "{control}"' not in instructions:
                raise ValueError(f"PhotoMode source control missing: {control}")
    assets = {path: (source_root / path).read_bytes() for path in FRAMES}
    for path, data in assets.items():
        if not data.startswith(b"DDS "):
            raise ValueError(f"Invalid PhotoMode frame: {path}")
    keys = {"$" + name for controls in CONTROLS.values() for name in controls}
    keys |= {"$TAKE SNAPSHOT", "$TOGGLE MENU", "$RESET", "$EXIT", "$ON", "$OFF", "$NONE"}
    translations = [line for line in read_table(interface / "translate_en.txt")
                    if line.split("\t", 1)[0] in keys or line.startswith("$Photomode")]
    movie = native_runtime.augment_as3_classes(source, {"SelfieMenu": BRIDGE.read_text(encoding="utf-8")},
                                              list(closure.values()))
    movie = native_runtime.augment_as3_classes(movie, {'Shared.AS3.IMenu':
        'package Shared.AS3 { public class IMenu extends BSDisplayObject {'
        'public function B21SetFO4Platform(platform:uint):void {'
        '_uiPlatform=platform; _bIsGen9=false; _uiController=platform; _uiKeyboard=0;'
        'dispatchEvent(new Shared.AS3.Events.PlatformChangeEvent(platform,false,platform,0)); } } }'}, list(closure.values()))
    for method, bridge in (("OnAccept", "B21Snapshot"), ("OnCancel", "B21Exit"),
                           ("OnToggleMenu", "B21Toggle")):
        movie = native_runtime.patch_as3_method(movie, "SelfieMenu", "$constructor", [["getproperty", method]],
            [["callproperty", bridge + "Handler", 0]])
    for class_name in ('SelfieMenu', 'Panel'):
        adapter = ('package { import flash.display.MovieClip; import flash.events.Event; public class ' + class_name +
                   ' extends MovieClip { public function B21RegisterHandler():Function { return B21Register; } '
                   'private function B21Register(event:Event):void { RegisterStageEvents(); } } }')
        movie = native_runtime.augment_as3_classes(movie, {class_name: adapter}, list(closure.values()))
        movie = native_runtime.patch_as3_method(movie, class_name, '$constructor', [['getproperty', 'RegisterStageEvents']],
            [['callproperty', 'B21RegisterHandler', 0]])
    for class_name in ('OptionSliderWithLabel', 'OptionStepperWithLabel'):
        movie = native_runtime.augment_as3_classes(movie, {class_name:
            'package { import flash.display.MovieClip; public class ' + class_name + ' extends MovieClip {'
            'public function B21ControlName():String { return Name; } } }'}, list(closure.values()))
    movie = native_runtime.patch_as3_method(movie, 'Panel', 'getControlByName', [['getproperty', 'Name']],
        [['callproperty', 'B21ControlName', 0]])
    movie = native_runtime.patch_as3_method(movie, 'SelfieMenu', 'OnControlChanged',
        [['getlocal', 4], ['getproperty', 'Name']], [['keep', 0], ['callproperty', 'B21ControlName', 0]])
    movie = native_runtime.patch_as3_method(movie, 'SelfieMenu', 'OnReset',
        [['callproperty', 'currentControl', 0], ['getproperty', 'Name']],
        [['keep', 0], ['callproperty', 'B21ControlName', 0]])
    movie = native_runtime.augment_as3_classes(movie, {
        name: BRIDGE.with_name(name + '.as').read_text(encoding='utf-8')
        for name in ('Panel', 'OptionStepperWithLabel', 'OptionSliderWithLabel')}, list(closure.values()))
    verify_script_only(source, movie)
    outputs = {OUTPUT / name: remap_menu_fonts(movie if name == MENU else data)
               for name, data in closure.items()}
    outputs.update({Path(path): data for path, data in assets.items()})
    manifest = {"schema_version": 1, "bridge_version": 2, "menu": (OUTPUT / MENU).as_posix(), "controls": CONTROLS,
                "source_sha256": hashlib.sha256(source).hexdigest(), "frames": list(FRAMES),
                "unsupported": ["Expression", "Vanity Light Style", "Vanity Light Strength",
                                "FO76 dynamic poses", "unconverted PMFT frames and textures"],
                "files": [{"path": path.as_posix(), "sha256": hashlib.sha256(data).hexdigest()}
                          for path, data in outputs.items()]}
    for path, data in outputs.items():
        destination = output_data / path
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_bytes(data)
    (output_data / OUTPUT / "conversion.json").write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    merge_ui_translations(output_data, translations)
    return manifest


if __name__ == "__main__":
    import argparse
    parser = argparse.ArgumentParser()
    parser.add_argument("source_root", type=Path)
    parser.add_argument("output_data", type=Path)
    args = parser.parse_args()
    print(json.dumps(convert_photo_mode_ui(args.source_root, args.output_data), indent=2))
