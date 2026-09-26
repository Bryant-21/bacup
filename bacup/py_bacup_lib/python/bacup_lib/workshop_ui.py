from __future__ import annotations

import hashlib
import json
import struct
import zlib
from pathlib import Path

from bacup_lib.daily_ops_ui import empty_hud_timeline
from bacup_lib.inspect_ui import rebuild
from bacup_lib.legendary_perks_ui import imports, remap_menu_fonts, swf_tags, verify_script_only
from bacup_lib.ui_contract import resolve_numbered_as3_call, resolve_numbered_name

RESOURCES = Path(__file__).with_name("resources") / "workshop"
OUTPUT = Path("Interface/B21/TalesFromAppalachia/WorkshopPrototype")
MENU = Path("Interface/Workshop.swf")
ART = "workshopnewlibrary.swf"
ART_CLASS_STEMS = (
    "WorkshopNewLibrary_fla.LeftPanel_BG_",
    "WorkshopNewLibrary_fla.LeftPanel_Title_Build_ContainerLong_",
    "WorkshopNewLibrary_fla.VariantsPanel_BG_Container_",
)
DESCRIPTION_SETTER_STEM = "__setProp_DescriptionBase_mc_WorkshopBase_Description_"


def uncompressed(data: bytes) -> bytes:
    if data[:3] == b"CWS":
        return b"FWS" + data[3:8] + zlib.decompress(data[8:])
    if data[:3] != b"FWS":
        raise ValueError("Unsupported workshop SWF compression")
    return data


def art_library(source: bytes, aliases: dict[str, str] | None = None) -> tuple[bytes, dict[str, str]]:
    from creation_lib.swf import native_runtime

    symbols = native_runtime.list_symbols(source)
    names = {name: (aliases or {}).get(name, f"B21_WorkshopArt_{character}") for character, name in symbols if character}
    tags = [(code, payload) for code, payload in swf_tags(empty_hud_timeline(source))
            if code not in (0, 1, 72, 76, 82)]
    tags += [(82, native_runtime.build_movieclip_class_doabc(list(names.values()))),
             (76, struct.pack("<H", len(names)) + b"".join(
                 struct.pack("<H", character) + names[name].encode() + b"\0"
                 for character, name in symbols if character)), (1, b""), (0, b"")]
    return rebuild(empty_hud_timeline(source), tags), names


def private_imports(data: bytes) -> bytes:
    data = remap_menu_fonts(data)
    tags = []
    for code, payload in swf_tags(data):
        if code in (57, 71):
            name, tail = payload.split(b"\0", 1)
            payload = OUTPUT.relative_to("Interface").as_posix().encode() + b"/" + name.lower() + b"\0" + tail
        tags.append((code, payload))
    return rebuild(uncompressed(data), tags)


def convert_workshop_prototype(source_root: Path, target_movie: Path, output_data: Path) -> dict:
    from creation_lib.swf import native_runtime

    interface = Path(source_root) / "interface"
    source = (interface / ART).read_bytes()
    target_source = Path(target_movie).read_bytes()
    library, names = art_library(source)
    required = [
        resolve_numbered_name(names, stem, "workshop artwork class")
        for stem in ART_CLASS_STEMS
    ]
    files = {path.name.casefold(): path for path in interface.iterdir() if path.is_file()}
    pending, dependencies = imports(library), {}
    while pending:
        name = pending.pop().casefold()
        if name in dependencies:
            continue
        if "/" in name or "\\" in name or not name.endswith(".swf"):
            raise ValueError(f"Unsupported workshop import: {name}")
        dependencies[name] = files[name].read_bytes()
        pending.extend(imports(dependencies[name]))

    bridge = (RESOURCES / "Workshop.as").read_text(encoding="utf-8")
    for index, name in enumerate(required):
        bridge = bridge.replace(f"/* ART_{index} */", names[name])
    movie = native_runtime.augment_as3_classes(target_source, {"Workshop": bridge})
    description_setter = resolve_numbered_as3_call(
        native_runtime,
        target_source,
        "Workshop",
        "$constructor",
        DESCRIPTION_SETTER_STEM,
        "workshop description setter",
    )
    movie = native_runtime.patch_as3_method(movie, "Workshop", "$constructor",
        [["getlocal", 0], ["callpropvoid", description_setter, 0]],
        [["keep", 0], ["keep", 1], ["getlocal", 0], ["callpropvoid", "B21Initialize", 0]])
    verify_script_only(target_source, movie)
    # FO4's controller, fonts, clip paths and native 3D icon positions remain intact.
    outputs = [(MENU, movie, target_source), (OUTPUT / ART, private_imports(library), source)]
    outputs.extend((OUTPUT / name, private_imports(data), data) for name, data in sorted(dependencies.items()))
    manifest = {"schema_version": 1, "prototype": True, "menu": MENU.as_posix(),
                "preview_renderer": "FO4 WorkshopMenu native 3D (ARTO/NIF and object models)",
                "catalog": "FO4 live workshop tree, unchanged", "art_symbols": names,
                "source_classes": required, "target_description_setter": description_setter,
                "bridge_sha256": hashlib.sha256(bridge.encode()).hexdigest(), "files": []}
    for relative, data, original in outputs:
        path = Path(output_data) / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(data)
        manifest["files"].append({"path": relative.as_posix(), "sha256": hashlib.sha256(data).hexdigest(),
                                  "source_sha256": hashlib.sha256(original).hexdigest()})
    (Path(output_data) / OUTPUT / "conversion.json").write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    return manifest


if __name__ == "__main__":
    import argparse

    parser = argparse.ArgumentParser(description="Build the opt-in FO76/FO4 workshop compatibility prototype")
    parser.add_argument("source_root", type=Path)
    parser.add_argument("target_movie", type=Path, help="Workshop.swf extracted from the user's Fallout 4")
    parser.add_argument("output_data", type=Path)
    args = parser.parse_args()
    print(json.dumps(convert_workshop_prototype(args.source_root, args.target_movie, args.output_data), indent=2))
