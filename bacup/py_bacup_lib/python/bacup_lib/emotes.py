from __future__ import annotations

import hashlib
import json
import re
import struct
import xml.etree.ElementTree as ET
import zlib
from pathlib import Path

from bacup_lib.inspect_ui import empty_hud_timeline, fo4_stage, rebuild, sprite_children
from bacup_lib.legendary_perks_ui import imports, remap_menu_fonts, swf_tags
from bacup_lib.ui_contract import resolve_numbered_name
from bacup_lib.translations import merge_ui_translations, packaged_tales_lines

RESOURCES = Path(__file__).with_name("resources") / "emotes"
OUTPUT = Path("Interface/B21/TalesFromAppalachia/Emotes")
CATALOG = Path("F4SE/Plugins/B21_TalesFromAppalachia/emotes.json")
SCHEMA_VERSION = 2
BRIDGE_VERSION = 3
EVENT = "dyn_ActivationCullWeapons"
SOURCE_EVENT = "dyn_ActivationLoopCullWeapons"
HUMAN_GRAPH = "meshes/actors/character/behaviors/weaponbehavior.hkx"
# PowerArmorRace names Actors\Character\Behaviors\WeaponBehavior.hkx as its weapon subgraph, so the
# PA root only has to declare the event and reach the swap generator that hosts that subgraph.
POWER_ARMOR_GRAPH = "meshes/actors/powerarmor/behaviors/powerarmorbehavior.hkx"
POWER_ARMOR_SKELETON = "meshes/actors/powerarmor/characterassets/skeleton.hkx"
CATEGORY_LIST = "5183FA"
MISC_CATEGORY = "006860"
EXCLUDED_PREFIXES = ("DEBUG_", "TEMPLATE_")
PET_EDITOR_ID = re.compile(r"pets?_", re.IGNORECASE)
HUMAN_CLIP_DIRS = ("meshes/actors/character/animations/common/emotes",
                   "meshes/actors/character/animations/dynamicanims")
FEMALE_CLIP_DIR = "meshes/actors/character/animations/common/emotes/female"
POWER_ARMOR_CLIP_DIR = "meshes/actors/powerarmor/animations/paired"
HUMAN_OUTPUT = "Actors\\Character\\Animations\\B21\\Emotes"
POWER_ARMOR_OUTPUT = "Actors\\PowerArmor\\Animations\\B21\\Emotes"
HUMAN_TRACKS = 95
# FO76 icon classes that emoteslibrary.swf does not ship fall back to this radialmenu.swf class.
ICON_FALLBACK = "radialIconEmpty"
ICON_ONLY_DURATION = 4.0
LAYOUT_INSTANCES = ("Background_mc", "CenterInfo_mc", "InnerRing", "OuterRing", "radialTab")
SHAPE_CODES = (2, 22, 32, 83)
BITMAP_CODES = (6, 20, 21, 35, 36, 90)


def sha(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def fields(record: dict) -> dict:
    value = record.get("fields", {})
    return value if isinstance(value, dict) else {k: v for row in value for k, v in row.items()}


def reference(value: dict) -> str:
    ref = value["reference"]
    if ref["plugin"] != "SeventySix.esm":
        raise ValueError(f"Unexpected source emote reference: {ref}")
    return ref["object_id"]


def english(value: dict) -> str:
    return next(row["String"] for row in value["Values"] if row["Language"] == "English")


def graph_objects(xml: str) -> tuple[dict, list[str]]:
    root = ET.fromstring(xml)
    objects = {o.attrib["name"]: o for o in root.findall(".//hksection/hkobject")}
    strings = next(o for o in objects.values() if o.get("class") == "hkbBehaviorGraphStringData")
    return objects, [p.text for p in strings.find("hkparam[@name='eventNames']")]


def text(obj, name: str) -> str:
    p = obj.find(f"hkparam[@name='{name}']")
    return p.text.strip() if p is not None and p.text else ""


def reachable(objects: dict) -> set[str]:
    graph = next(o for o in objects.values() if o.get("class") == "hkbBehaviorGraph")
    pending, seen = [text(graph, "rootGenerator")], set()
    while pending:
        node_id = pending.pop()
        if node_id in seen or node_id not in objects:
            continue
        seen.add(node_id)
        pending.extend(re.findall(r"#\d+", ET.tostring(objects[node_id], encoding="unicode")))
    return seen


def consuming_route(xml: str, event: str) -> dict:
    objects, events = graph_objects(xml)
    if event not in events:
        raise ValueError(f"No graph event {event}")
    event_id = events.index(event)
    live = reachable(objects)
    for machine_id, machine in objects.items():
        if machine.get("class") != "hkbStateMachine" or machine_id not in live:
            continue
        states = {text(objects[s], "stateId"): objects[s] for s in text(machine, "states").split()}
        arrays = [text(machine, "wildcardTransitions")]
        arrays += [text(state, "transitions") for state in states.values()]
        for array_id in arrays:
            if array_id not in objects:
                continue
            for transition in objects[array_id].findall("hkparam[@name='transitions']/hkobject"):
                if text(transition, "eventId") != str(event_id) or text(transition, "condition") != "null":
                    continue
                state = states.get(text(transition, "toStateId"))
                if state is None:
                    continue
                pending = [text(state, "generator")]
                seen = set()
                while pending:
                    node_id = pending.pop()
                    if node_id in seen or node_id not in objects:
                        continue
                    seen.add(node_id)
                    node = objects[node_id]
                    if node.get("class") == "DynamicAnimationTaggingGenerator":
                        return {"event": event, "event_id": event_id, "machine": machine_id,
                                "state": state.get("name"), "state_name": text(state, "name"),
                                "dynamic_generator": node_id, "graph_sha256": sha(xml.encode())}
                    # Only generator ownership links establish a playback route.
                    for name in ("generator", "children"):
                        pending.extend(text(node, name).split())
    raise ValueError(f"Event {event} has no unconditional transition to a dynamic animation generator")


def power_armor_route(root_xml: str, weapon_route: dict) -> dict:
    """The PA root forwards the event into the race's weapon subgraph, which is the human one."""
    objects, events = graph_objects(root_xml)
    if weapon_route["event"] not in events:
        raise ValueError(f"Power armor graph lacks {weapon_route['event']}")
    swaps = sorted(i for i in reachable(objects) if objects[i].get("class") == "BSBehaviorGraphSwapGenerator")
    if not swaps:
        raise ValueError("Power armor graph has no reachable subgraph swap generator")
    return {"event": weapon_route["event"], "root_graph": POWER_ARMOR_GRAPH,
            "root_event_id": events.index(weapon_route["event"]), "swap_generators": swaps,
            "root_graph_sha256": sha(root_xml.encode()), "subgraph": HUMAN_GRAPH,
            "subgraph_state": weapon_route["state_name"], "dynamic_generator": weapon_route["dynamic_generator"]}


def animation_metadata(xml: str, tracks_expected: int = HUMAN_TRACKS) -> dict:
    root = ET.fromstring(xml)
    animations = [o for o in root.findall(".//hkobject") if o.get("class") in
                  ("hkaSplineCompressedAnimation", "hkaInterleavedUncompressedAnimation")]
    if len(animations) != 1:
        raise ValueError("Expected one emote animation")
    anim = animations[0]
    duration = float(anim.find("hkparam[@name='duration']").text)
    tracks = int(anim.find("hkparam[@name='numberOfTransformTracks']").text)
    binding = root.find(".//hkobject[@class='hkaAnimationBinding']")
    indices = [int(i) for i in binding.find("hkparam[@name='transformTrackToBoneIndices']").text.split()]
    annotations = [p.text for p in anim.findall(".//hkparam[@name='text']")]
    # A clip may leave trailing bones unanimated (MothmanWorship binds 94 of 95); What a Drag runs 69 s.
    if not 0 < duration <= 120 or not 0 < tracks <= tracks_expected or indices != list(range(tracks)):
        raise ValueError(f"Emote did not produce an FO4 {tracks_expected}-bone binding "
                         f"({tracks} tracks, {len(indices)} bound, {duration} s)")
    return {"duration": duration, "tracks": tracks, "annotations": annotations}


def skeleton_bones(havok, data: bytes) -> list[str]:
    try:
        xml = havok.hkx_to_xml(data)
    except Exception:
        xml = havok.hkx_to_xml(bytes(havok.havok_convert_bytes(data, "hk_2014.1.0-r1")))
    rigs = [s for s in ET.fromstring(xml).findall(".//hkobject[@class='hkaSkeleton']") if text(s, "name") == "Root"]
    if len(rigs) != 1:
        raise ValueError("Expected one Root skeleton")
    return [text(bone, "name") for bone in rigs[0].find("hkparam[@name='bones']")]


def trim_power_armor(xml: str, bones: list[str]) -> str:
    """Drop FO76's trailing PA-only tracks (AimSource) so the clip binds FO4's PA skeleton."""
    from creation_lib.havok.native_runtime import extract_clip_native, write_animation_xml_native

    clip = extract_clip_native(xml)
    if clip["warnings"]:
        raise ValueError(f"PA clip decode warnings: {clip['warnings']}")
    count = len(bones)
    if clip["track_to_bone_indices"][:count] != list(range(count)):
        raise ValueError("PA clip tracks do not start with the FO4 PA skeleton order")
    clip["channels"] = clip["channels"][:count]
    clip["track_to_bone_indices"] = list(range(count))
    replacement = ET.fromstring(write_animation_xml_native(clip, bones)).find(".//hkobject[@name='#animation']")
    root = ET.fromstring(xml)
    section = root.find("hksection")
    original = next(o for o in section if o.get("class") in
                    ("hkaSplineCompressedAnimation", "hkaInterleavedUncompressedAnimation"))
    replacement.set("name", original.get("name"))
    replacement.find("hkparam[@name='extractedMotion']").text = text(original, "extractedMotion") or "null"
    section[list(section).index(original)] = replacement
    binding = section.find("hkobject[@class='hkaAnimationBinding']")
    indices = binding.find("hkparam[@name='transformTrackToBoneIndices']")
    indices.text = " ".join(map(str, range(count)))
    indices.set("numelements", str(count))
    return ET.tostring(root, encoding="unicode")


def sprite_tags(payload: bytes) -> list[tuple[int, bytes]]:
    # A sprite embeds the same tag stream as a movie, after its ID/frame count.
    body = b"\x08\x00\x00\x1e\x01\x00" + payload[4:]
    return swf_tags(b"FWS\x20" + struct.pack("<I", len(body) + 8) + body)


def placed_character(code: int, tag: bytes) -> int | None:
    if not tag[0] & 2:
        return None
    start = 3 if code == 26 else 4
    if code == 70 and tag[1] & 8:
        start = tag.index(b"\0", start) + 1
    return struct.unpack_from("<H", tag, start)[0]


def radial_layout(payload: bytes, shapes: set[int] = frozenset()) -> bytes:
    """Keep the source root's first frame: the named layout clips and the ring disk shape."""
    placements = []
    found = set()
    disks = 0
    for code, tag in sprite_tags(payload):
        if code == 1:
            break
        if code not in (26, 70):
            continue
        if not tag[0] & 0x20 and placed_character(code, tag) in shapes:
            placements.append((code, tag))
            disks += 1
            continue
        for name in LAYOUT_INSTANCES:
            if name.encode() + b"\0" in tag:
                if name in found:
                    raise ValueError(f"Duplicate source radial placement: {name}")
                placements.append((code, tag))
                found.add(name)
    if found != set(LAYOUT_INSTANCES):
        raise ValueError(f"Source radial layout changed: {found}")
    if shapes and disks != 1:
        raise ValueError(f"Source radial ring disk changed: {disks} unnamed shapes")
    return payload[:2] + struct.pack("<H", 1) + b"".join(
        struct.pack("<HI", code << 6 | 63, len(tag)) + tag
        for code, tag in placements + [(1, b""), (0, b"")])


def library_without_text(library: bytes) -> tuple[bytes, set[int]]:
    """Unplace text from every library sprite: symbol injection cannot carry fonts or text fields.

    Only LuckyDice1's DiceValue number and ImSelling1's label use text; the wheel draws the dice
    value itself."""
    if library[:3] == b"CWS":
        library = b"FWS" + library[3:8] + zlib.decompress(library[8:])
    tags = list(swf_tags(library))
    sprites = {struct.unpack_from("<H", p)[0]: p for c, p in tags if c == 39}
    textual = {struct.unpack_from("<H", p)[0] for c, p in tags if c == 37}
    changed = True
    while changed:
        changed = False
        for cid, payload in sprites.items():
            if cid not in textual and any(placed_character(c, t) in textual
                                          for c, t in sprite_tags(payload) if c in (26, 70)):
                textual.add(cid)
                changed = True
    rebuilt = []
    for code, payload in tags:
        if code == 39:
            kept = [(c, t) for c, t in sprite_tags(payload) if c not in (26, 70) or placed_character(c, t) not in textual]
            if len(kept) != len(sprite_tags(payload)):
                payload = payload[:4] + b"".join(struct.pack("<HI", c << 6 | 63, len(t)) + t for c, t in kept)
        rebuilt.append((code, payload))
    return rebuild(library, rebuilt), textual


def radial_closure(source: bytes, names: list[str]) -> bytes:
    from creation_lib.swf.native_runtime import list_symbols

    symbols = list_symbols(source)
    tags = list(swf_tags(source))
    shapes = {struct.unpack_from("<H", p)[0] for c, p in tags if c in SHAPE_CODES}
    root_id = next(cid for cid, name in symbols if name == "RadialMenu")
    tags = [(c, radial_layout(p, shapes) if c == 39 and struct.unpack_from("<H", p)[0] == root_id else p)
            for c, p in tags]
    definitions = {struct.unpack_from("<H", p)[0]: (c, p) for c, p in tags
                   if c in (2, 22, 32, 83, 39, 46, 84, 6, 21, 35, 90, 20, 36, 10, 48, 75, 11, 33, 37)}
    pending = [cid for cid, name in symbols if name in names]
    keep = set()
    while pending:
        cid = pending.pop()
        if cid in keep:
            continue
        keep.add(cid)
        code, payload = definitions[cid]
        if code == 39:
            pending.extend(sprite_children(payload))
    # Shapes reference their bitmap fills by id rather than placement; the sector backers and
    # selection tiles are such bitmaps, so every source bitmap travels with the closure.
    keep |= {struct.unpack_from("<H", p)[0] for c, p in tags if c in BITMAP_CODES}
    exports = [(cid, name) for cid, name in symbols if cid in keep]
    selected = [(c, p) for c, p in tags if c in (69, 8) or
                (c in (57, 71) and p.split(b"\0", 1)[0].lower() == b"fonts_en.swf") or
                (c in (10, 48, 75, 13, 62, 73, 88)) or
                (c in {v[0] for v in definitions.values()} and struct.unpack_from("<H", p)[0] in keep)]
    selected += [(76, struct.pack("<H", len(exports)) + b"".join(
        struct.pack("<H", cid) + name.encode() + b"\0" for cid, name in exports)), (1, b""), (0, b"")]
    return fo4_stage(rebuild(empty_hud_timeline(source), selected))


def artwork_class(movie: bytes, name: str) -> str:
    from creation_lib.swf.native_runtime import abc_disassemble

    package, _, short = name.rpartition(".")
    scripts = []
    methods = abc_disassemble(movie, name)
    constructor = next(method["code"] for method in methods if method["method"] == "constructor")
    registrations = {}
    for index, op in enumerate(constructor):
        if index < 2 or "GetProperty " not in op:
            continue
        frame = re.search(r"Push(?:Byte|Short) \{ value: (\d+) \}", constructor[index - 2])
        if frame:
            registrations[op.split("GetProperty ", 1)[1]] = int(frame[1])
    for method in methods:
        match = re.fullmatch(r"(?:\w+\.)?frame(\d+)", method["method"])
        if not match:
            continue
        code = [re.sub(r"^\s*\d+\s+", "", op) for op in method["code"]]
        if code == ["GetLocal { index: 0 }", "PushScope", "ReturnVoid"]:
            continue
        # LuckyDice1's last frame tells FO76's HUD widget to hide; in the wheel the roll just holds.
        hide = ["GetLocal { index: 0 }", "PushScope", "FindPropStrict dispatchEvent", "FindPropStrict flash.events.Event",
                'PushString "AnimCompleteForceHide"', "PushTrue", "PushTrue", "ConstructProp flash.events.Event (3)",
                "CallPropVoid dispatchEvent (1)", "ReturnVoid"]
        if code != ["GetLocal { index: 0 }", "PushScope", "FindPropStrict stop", "CallPropVoid stop (0)", "ReturnVoid"] \
                and code != hide:
            raise ValueError(f"Source artwork requires a nontrivial frame adapter: {name}.{method['method']}")
        if method["method"] not in registrations:
            raise ValueError(f"Source artwork frame registration changed: {name}.{method['method']}")
        scripts.extend((str(registrations[method["method"]]), "stop"))
    body = "addFrameScript(" + ", ".join(scripts) + ");" if scripts else ""
    return (f"package {package} {{ import flash.display.MovieClip; public dynamic class {short} extends MovieClip "
            f"{{ public function {short}() {{ super(); {body} }} }} }}")


def convert_ui(source_root: Path, icons: list[str]) -> tuple[bytes, dict, bytes]:
    from creation_lib.swf import native_runtime as swf

    radial = (source_root / "interface/radialmenu.swf").read_bytes()
    library = (source_root / "interface/emoteslibrary.swf").read_bytes()
    symbols = {name for _, name in swf.list_symbols(radial)}
    background = resolve_numbered_name(symbols, "RadialMenu_fla.radialBackground_mc_", "emote background")
    center = resolve_numbered_name(symbols, "RadialMenu_fla.radialCenterGroup_mc_", "emote center")
    tab = resolve_numbered_name(symbols, "RadialMenu_fla.radialTab_", "emote corner tab")
    selected = ["RadialMenu", background, center, tab, "RadialMenuRingInner", "RadialMenuRingOuter", ICON_FALLBACK]
    movie = radial_closure(radial, selected)
    textless, textual = library_without_text(library)
    stripped = [name for cid, name in swf.list_symbols(library) if name in icons and cid in textual]
    movie = swf.inject_symbols(textless, movie, [icon for icon in icons if icon != ICON_FALLBACK])
    bridges = {}
    for name in ("RadialMenuRingInner", "RadialMenuRingOuter", "RadialMenuEntryInner", "RadialMenuEntryOuter"):
        template = "Ring" if "Ring" in name else "Entry"
        bridges[name] = (RESOURCES / f"{template}.as").read_text().replace("CLASS_NAME", name).replace(
            "SELECTED_FRAME", "15" if "Inner" in name else "11").replace(
            "IDLE_FRAME", "3" if "Inner" in name else "22")
    fonts = (source_root / "interface/fonts_en.swf").read_bytes()
    adapter = (RESOURCES / "B21EmoteMenu.as").read_text().replace(
        "BACKGROUND_CLASS", background).replace("CENTER_CLASS", center)
    classes = []
    for _, name in swf.list_symbols(movie):
        if name in bridges:
            classes.append(bridges[name])
        else:
            classes.append(artwork_class(radial if name in symbols else library, name))
    hints = (RESOURCES.parent / "shared/B21ButtonHints.as").read_text()
    abc = swf.compile_as3_do_abc(classes + [hints, adapter])
    tags = [(code, payload) for code, payload in swf_tags(movie) if code not in (0, 1)]
    bindings = [(c, p) for c, p in tags if c == 76]
    tags = [(c, p) for c, p in tags if c != 76]
    tags += [(82, abc)] + bindings + [(76, struct.pack("<HH", 1, 0) + b"B21EmoteMenu\0"), (1, b""), (0, b"")]
    movie = rebuild(movie, tags)
    movie = swf.rename_as3_classes(movie, "B21Emotes")
    movie = remap_menu_fonts(movie)
    if any(name.lower() != "fonts_en.swf" for name in imports(movie)):
        raise ValueError(f"Unexpected emote imports: {imports(movie)}")
    tags = [(code, payload.replace(payload.split(b"\0", 1)[0],
             b"B21/TalesFromAppalachia/Emotes/fonts_en.swf", 1)) if code in (57, 71) else (code, payload)
            for code, payload in swf_tags(movie)]
    movie = rebuild(movie, tags)
    return movie, {"bridge_version": BRIDGE_VERSION, "menu": (OUTPUT / "menu.swf").as_posix(),
                   "radial_sha256": sha(radial), "library_sha256": sha(library),
                   "symbols": selected + icons, "classes": "B21Emotes",
                   "layout_instances": list(LAYOUT_INSTANCES), "ring_disk": True, "corner_tab": tab,
                   "icon_fallback": ICON_FALLBACK,
                   "text_stripped_icons": stripped, "source_stage": [1920, 1080],
                   "target_stage": [1280, 720], "ring_entries": [12, 16]}, remap_menu_fonts(fonts)


def clip_stem(animation_file: str) -> str | None:
    match = re.fullmatch(r"\$\(subgraph\)\\([^\\]+)\.hk[xt]", animation_file, re.IGNORECASE)
    return match[1] if match else None


def source_clip(source_root: Path, directories: tuple[str, ...], stem: str) -> str | None:
    return next((f"{d}/{stem.lower()}.hkx" for d in directories
                 if (source_root / f"{d}/{stem.lower()}.hkx").is_file()), None)


class ClipConverter:
    def __init__(self, havok, source_root: Path, target_root: Path):
        self.havok = havok
        self.source_root = source_root
        self.pa_bones = skeleton_bones(havok, (target_root / POWER_ARMOR_SKELETON).read_bytes())
        source_bones = skeleton_bones(havok, (source_root / POWER_ARMOR_SKELETON).read_bytes())
        if source_bones[:len(self.pa_bones)] != self.pa_bones:
            raise ValueError("FO76 PA skeleton no longer extends the FO4 PA skeleton")
        self.pa_dropped = source_bones[len(self.pa_bones):]
        self.done: dict[str, tuple] = {}

    def convert(self, source: str, power_armor: bool) -> tuple[bytes, bytes, dict]:
        if source not in self.done:
            original = (self.source_root / source).read_bytes()
            converted = bytes(self.havok.havok_convert_bytes(original, "hk_2014.1.0-r1"))
            if power_armor:
                converted = bytes(self.havok.xml_to_hkx(trim_power_armor(self.havok.hkx_to_xml(converted), self.pa_bones)))
                metadata = animation_metadata(self.havok.hkx_to_xml(converted), len(self.pa_bones))
            else:
                metadata = animation_metadata(self.havok.hkx_to_xml(converted))
            self.done[source] = (original, converted, metadata)
        return self.done[source]


def classify(editor_id: str, data: dict) -> str:
    if PET_EDITOR_ID.search(editor_id):
        return "pet"
    return "human" if "Animation" in data else "icon"


def convert_emotes(source_root: Path, output_mod: Path, *, source_plugin: Path, target_root: Path) -> dict:
    from creation_lib.esp import Plugin
    from creation_lib.havok.native_runtime import load_native_module
    from creation_lib.swf import native_runtime as swf

    havok = load_native_module()
    route = consuming_route(havok.unpack_hkx_to_xml(str(target_root / HUMAN_GRAPH)), EVENT)
    route["graph"] = HUMAN_GRAPH
    try:
        route["power_armor"] = power_armor_route(havok.unpack_hkx_to_xml(str(target_root / POWER_ARMOR_GRAPH)), route)
    except (OSError, ValueError, StopIteration):
        route["power_armor"] = None
    clips = ClipConverter(havok, source_root, target_root)
    library_symbols = {name for _, name in swf.list_symbols((source_root / "interface/emoteslibrary.swf").read_bytes())}
    files: dict[Path, tuple[bytes, str, bytes]] = {}
    categories, entries, skipped, excluded = [], [], [], []
    with Plugin.load(source_plugin, game="fo76", lazy_index=True) as plugin:
        order = plugin.read_authoring_record(int(CATEGORY_LIST, 16))
        if order["eid"] != "EmoteCategoriesSorted":
            raise ValueError(f"Source emote category list changed: {order['eid']}")
        for row in order["fields"]:
            category_id = reference(row["FormID"])
            record = plugin.read_authoring_record(int(category_id, 16))
            data = fields(record)
            categories.append({"id": category_id, "editor_id": record["eid"], "name": english(data["Name"]),
                               "icon": "", "icon_source": data.get("SWFClassName", ""), "icon_fallback": False})
        known = {row["id"] for row in categories}
        if len(categories) != 12 or MISC_CATEGORY not in known:
            raise ValueError(f"Source emote categories changed: {[row['id'] for row in categories]}")
        unlisted = [r[1] for r in plugin.record_index_rows(signatures=["ECAT"]) if r[0].split(":")[1] not in known]
        if unlisted:
            raise ValueError(f"Source emote categories outside {CATEGORY_LIST}: {unlisted}")
        sounds = {r[1]: r for r in plugin.record_index_rows(signatures=["SNDR"])}
        for index in plugin.record_index_rows(signatures=["EMOT"]):
            record = plugin.read_authoring_record(index[4])
            editor_id, form_id = record["eid"], record["form_id"]
            if editor_id.startswith(EXCLUDED_PREFIXES):
                excluded.append({"source_id": form_id, "editor_id": editor_id})
                continue
            data = fields(record)
            kind = classify(editor_id, data)
            category_id = reference(data["Category"]) if "Category" in data else ""
            if kind != "pet" and category_id not in known:
                category_id = MISC_CATEGORY
            icon_source = data.get("SwfClassName", "")
            faces = int(data.get("NumberOfPossibleAnimations") or 0)
            entry = {"source_id": form_id, "editor_id": editor_id,
                     "name": english(data["Name"]) if "Name" in data else editor_id,
                     "category_id": category_id, "kind": kind,
                     "icon": icon_source if icon_source in library_symbols else ICON_FALLBACK,
                     "icon_source": icon_source, "icon_fallback": icon_source not in library_symbols,
                     "idle_id": reference(data["Animation"]) if "Animation" in data else "",
                     "animation": "", "animation_female": "", "animation_power_armor": "",
                     "power_armor_only": False, "duration": ICON_ONLY_DURATION,
                     "dice_faces": faces if faces > 1 else 0,
                     "sound_ids": []}
            entries.append(entry)
            if kind != "human":
                continue
            idle = fields(plugin.read_authoring_record(int(entry["idle_id"], 16)))
            stem = clip_stem(idle.get("AnimationFile", ""))
            if idle.get("AnimationEvent") != SOURCE_EVENT or stem is None:
                skipped.append({"source_id": form_id, "editor_id": editor_id, "clip": idle.get("AnimationFile", ""),
                                "reason": f"source IDLE route {idle.get('AnimationEvent')} is not a human emote route"})
                entry["kind"] = "icon"
                continue
            entry["power_armor_only"] = "powerarmor" in idle.get("BehaviorGraph", "").lower()
            annotations, durations = [], []
            variants = [("animation", HUMAN_CLIP_DIRS, HUMAN_OUTPUT, False),
                        ("animation_female", (FEMALE_CLIP_DIR,), HUMAN_OUTPUT + "\\Female", False)]
            if route["power_armor"] is not None:
                variants.append(("animation_power_armor", (POWER_ARMOR_CLIP_DIR,), POWER_ARMOR_OUTPUT, True))
            for key, directories, output, power_armor in variants:
                source = source_clip(source_root, directories, stem)
                if source is None:
                    continue
                try:
                    original, converted, metadata = clips.convert(source, power_armor)
                except Exception as error:
                    skipped.append({"source_id": form_id, "editor_id": editor_id, "clip": source,
                                    "reason": f"{type(error).__name__}: {error}"})
                    continue
                relative = f"{output}\\{stem}.hkx"
                files[Path("Meshes", *relative.split("\\"))] = (converted, source, original)
                entry[key] = relative
                durations.append(metadata["duration"])
                annotations += metadata["annotations"]
            if durations:
                entry["duration"] = durations[0]
            if entry["animation_power_armor"] and not (entry["animation"] or entry["animation_female"]):
                entry["power_armor_only"] = True
                skipped.append({"source_id": form_id, "editor_id": editor_id, "clip": idle.get("AnimationFile", ""),
                                "reason": "no human clip in the extracted source data; playable in power armor only"})
            if not (entry["animation"] or entry["animation_female"] or entry["animation_power_armor"]):
                entry["kind"] = "icon"
                continue
            for annotation in dict.fromkeys(annotations):
                if not annotation.startswith("SoundPlay."):
                    continue
                sound_name = annotation.split(".", 1)[1].strip()
                if not sound_name:
                    continue
                try:
                    sound_record = plugin.read_authoring_record(sounds[sound_name][4])
                    sound_path = fields(sound_record)["Sound"].replace("\\", "/").lower().removeprefix("data/")
                    if not (source_root / sound_path).is_file() and (source_root / Path(sound_path).with_suffix(".xwm")).is_file():
                        sound_path = Path(sound_path).with_suffix(".xwm").as_posix()
                    sound = (source_root / sound_path).read_bytes()
                except (KeyError, OSError) as error:
                    skipped.append({"source_id": form_id, "editor_id": editor_id, "clip": annotation,
                                    "reason": f"sound unavailable: {error}"})
                    continue
                if sound_record["form_id"] not in entry["sound_ids"]:
                    entry["sound_ids"].append(sound_record["form_id"])
                files[Path(sound_path)] = (sound, sound_path, sound)
    rank = {row["id"]: i for i, row in enumerate(categories)}
    entries.sort(key=lambda e: (e["kind"] == "pet", rank.get(e["category_id"], len(rank)), e["source_id"]))
    for category in categories:
        members = [e for e in entries if e["category_id"] == category["id"] and e["kind"] != "pet"]
        if category["icon_source"] in library_symbols:
            category["icon"] = category["icon_source"]
        else:
            # FO76 ships no Category* icon classes; the category's first own emote icon stands in.
            category["icon"] = next((e["icon"] for e in members if not e["icon_fallback"]), ICON_FALLBACK)
            category["icon_fallback"] = True
    icons = list(dict.fromkeys([row["icon"] for row in categories] + [e["icon"] for e in entries]))
    movie, ui, fonts = convert_ui(source_root, icons)
    files[OUTPUT / "menu.swf"] = (movie, "interface/radialmenu.swf + emoteslibrary.swf",
                                   (source_root / "interface/radialmenu.swf").read_bytes() +
                                   (source_root / "interface/emoteslibrary.swf").read_bytes())
    files[OUTPUT / "fonts_en.swf"] = (fonts, "interface/fonts_en.swf", (source_root / "interface/fonts_en.swf").read_bytes())
    manifest = {"schema_version": SCHEMA_VERSION, "event_name": "B21EmoteV1", "source_plugin": source_plugin.name,
                "source_plugin_sha256": sha(source_plugin.read_bytes()), "playback_route": route, "ui": ui,
                "categories": categories, "entries": entries,
                "power_armor_dropped_tracks": clips.pa_dropped, "skipped": skipped, "excluded": excluded,
                "files": []}
    for relative, (data, source, original) in files.items():
        target = output_mod / "data" / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(data)
        manifest["files"].append({"path": relative.as_posix(), "sha256": sha(data),
                                  "source": source, "source_sha256": sha(original) if original else None})
    catalog = output_mod / CATALOG
    catalog.parent.mkdir(parents=True, exist_ok=True)
    catalog.write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    lines = [line for line in packaged_tales_lines() if line.startswith(("$B21_TFA_Emote", "$B21_TFA_PromptSelect\t"))]
    merge_ui_translations(output_mod / "data", lines)
    return manifest
