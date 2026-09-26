from __future__ import annotations

import hashlib
import json
import struct
import zlib
from pathlib import Path

from bacup_lib.legendary_perks_ui import imports, remap_menu_fonts, swf_tags, verify_script_only
from bacup_lib.translations import TRANSLATIONS, merge_ui_translations, translation_path
from bacup_lib.quest_area_ui import closure as widget_closure, symbol_ids, tag
from bacup_lib.status_hud_source import replace_tags, select_children, sprite_tags

MENU = "dailyopshud.swf"
OUTPUT = Path("Interface/B21/TalesFromAppalachia/DailyOps")
BRIDGE = Path(__file__).with_name("resources") / "daily_ops/Bridge.as"
HUD_BRIDGE = BRIDGE.with_name("HUDMenu.as")
VOICE_BRIDGE = BRIDGE.with_name("VOFlyoutManager.as")
HUD = "dailyopsactive.swf"


def menu_translations(interface: Path) -> list[str]:
    data = (interface / "translate_en.txt").read_bytes()
    encoding = "utf-16" if data.startswith((b"\xff\xfe", b"\xfe\xff")) else "utf-8-sig"
    shared = {"$VIEW_REWARDS", "$EXIT", "$REWARDS", "$CONTINUE", "$NONE"}
    lines = [line for line in data.decode(encoding).splitlines()
             if line.startswith(("$DO_", "$DailyOps", "$DAILYOPS", "$QuestTrackerState_")) or line.split("\t", 1)[0] in shared]
    if not any(line.startswith("$DO_TIME\t") for line in lines):
        raise ValueError("Daily Ops time translation is missing")
    return lines


def empty_hud_timeline(data: bytes) -> bytes:
    body = zlib.decompress(data[8:]) if data[:3] == b"CWS" else data[8:]
    size = (5 + 4 * (body[0] >> 3) + 7) // 8 + 4
    header = body[:size - 2] + struct.pack("<H", 1)
    # Keep the source symbol library, but never construct the full FO76 HUD.
    controls = {0, 1, 4, 5, 9, 12, 26, 28, 43, 59, 70, 94}
    tags = [(code, payload) for code, payload in swf_tags(data) if code not in controls]
    tags += [(1, b""), (0, b"")]
    body = header + b"".join(struct.pack("<HI", code << 6 | 63, len(payload)) + payload for code, payload in tags)
    return b"FWS" + data[3:4] + struct.pack("<I", len(body) + 8) + body


def replace_classes(source: bytes, replacements: dict[str, Path | str], dependencies=()) -> bytes:
    from creation_lib.swf.native_runtime import replace_as3_classes
    sources = {
        name: value if isinstance(value, str) else value.read_text(encoding="utf-8")
        for name, value in replacements.items()
    }
    return verify_script_only(source, replace_as3_classes(source, sources, list(dependencies)))


def build_radio_movie(source: bytes) -> bytes:
    from creation_lib.swf.native_runtime import compile_as3_do_abc, unbacked_symbol_classes

    entries = swf_tags(source)
    root = symbol_ids(entries)["VOFlyoutManager"]
    entries = [(c, select_children(p, {"DOVOWaveForm_mc", "DOVOFlyoutGraphic_mc"}))
               if c == 39 and struct.unpack_from("<H", p)[0] == root else (c, p) for c, p in entries]
    art = widget_closure(entries, root, "Daily Ops radio")
    classes = {root: "B21_DailyOpsRadio"}
    scripts = [HUD_BRIDGE.read_text(encoding="utf-8"), VOICE_BRIDGE.read_text(encoding="utf-8")]
    for code, payload in art:
        if code != 39:
            continue
        cid, = struct.unpack_from("<H", payload)
        if cid == root:
            continue
        name = f"B21_DailyOpsRadioArt{cid}"
        classes[cid] = name
        frame, stops = 0, []
        for c, p in sprite_tags(payload):
            if c == 1:
                frame += 1
            elif c == 43 and p.split(b"\0",1)[0] in (b"off", b"on"):
                stops.extend((str(frame), "stop"))
        initializer = "addFrameScript(" + ",".join(stops) + ");" if stops else ""
        scripts.append(f"package {{ import flash.display.MovieClip; public dynamic class {name} extends MovieClip {{"
                       f"public function {name}() {{ {initializer} }} }} }}")
    exports = [(0,"B21_DailyOpsActive"),*sorted(classes.items())]
    bindings = struct.pack("<H",len(exports)) + b"".join(struct.pack("<H",cid)+name.encode()+b"\0" for cid,name in exports)
    movie = replace_tags(empty_hud_timeline(source), [(69,struct.pack("<I",8)),*art,
                         (82,compile_as3_do_abc(scripts)),(76,bindings),(1,b""),(0,b"")])
    if unbacked_symbol_classes(movie):
        raise ValueError("Unbound Daily Ops radio artwork")
    return remap_menu_fonts(movie)


def convert_daily_ops_ui(source_root: Path, output_data: Path) -> dict:
    interface = Path(source_root) / "interface"
    translations = menu_translations(interface)
    files = {p.name.casefold(): p for p in interface.iterdir() if p.is_file()}
    pending = [MENU, "vocharacteranim.swf"]
    closure = {}
    while pending:
        name = pending.pop().casefold()
        if name in closure:
            continue
        if "/" in name or "\\" in name or not name.endswith(".swf"):
            raise ValueError(f"Unsupported Daily Ops UI import: {name}")
        if name not in files:
            raise FileNotFoundError(f"Missing Daily Ops UI import: {name}")
        closure[name] = files[name].read_bytes()
        pending.extend(imports(closure[name]))

    movie = replace_classes(closure[MENU], {"DailyOpsModalManager": BRIDGE}, closure.values())
    hud = build_radio_movie((interface / "hudmenu.swf").read_bytes())

    output = Path(output_data) / OUTPUT
    output.mkdir(parents=True, exist_ok=True)
    manifest = {"schema_version": 3, "bridge_sha256": hashlib.sha256(BRIDGE.read_bytes()).hexdigest(),
                "hud_bridge_sha256": hashlib.sha256(HUD_BRIDGE.read_bytes()).hexdigest(),
                "voice_bridge_sha256": hashlib.sha256(VOICE_BRIDGE.read_bytes()).hexdigest(),
                "source_classes": ["VOFlyoutManager"], "quest_owner": "StatusHUD",
                "menu": (OUTPUT / MENU).as_posix(), "hud": (OUTPUT / HUD).as_posix(),
                "code_object": "root1.DOModal_mc", "files": []}
    previous = output / "conversion.json"
    if previous.is_file():
        retained = {name.casefold() for name in closure} | {HUD}
        for entry in json.loads(previous.read_text(encoding="utf-8")).get("files",[]):
            candidate = (Path(output_data) / entry["path"]).resolve()
            if candidate.parent == output.resolve() and candidate.suffix.lower() == ".swf" and candidate.name.casefold() not in retained:
                candidate.unlink(missing_ok=True)
    for name, source in sorted((closure | {HUD:hud}).items()):
        converted = remap_menu_fonts(movie if name == MENU else source)
        (output / name).write_bytes(converted)
        manifest["files"].append({"path": (OUTPUT / name).as_posix(),
                                  "source_sha256": hashlib.sha256(source).hexdigest(),
                                  "sha256": hashlib.sha256(converted).hexdigest()})
    (output / "conversion.json").write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    merge_ui_translations(output_data, translations)
    return manifest
