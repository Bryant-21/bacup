"""Build the Tales Infestation catalog and dark-fog map art from the user's FO76 install."""
from __future__ import annotations

import hashlib
import json
import math
import struct
import zlib
from collections import Counter
from pathlib import Path
from typing import Iterable, Protocol

from bacup_lib.reputation_catalog import NativeReader, fields_by_name, first, load_esp_native, plugin_masters

SCHEMA_VERSION = 1
PLUGIN = "SeventySix.esm"
CATALOG = Path("F4SE/Plugins/B21_TalesFromAppalachia/infestations.json")
OUTPUT = Path("PrismaUI_F4/views/B21_FullScreenMap/tales/infestations")
RESOURCES = Path(__file__).with_name("resources") / "infestations"
MAP_MOVIE = Path("interface/mapmenu.swf")
FOG_SYMBOL = "MapCloud"
RADIUS_CLIP = "RadiusCheck_mc"
MIN_CLEAR_ACTORS = 4

QUEST = "HTO_HostileTakeOver_Master_Quest"
WORLD = "APPALACHIA"
# HTO_MarkerScript's region toggles; the region locations carry the LocTypeRegion keyword.
REGIONS = (
    ("Forest", "RegionForestFloodlandsLocation", "HTO_LCP_HostileTakeOver_EventToggle_TheForestRegion"),
    ("ToxicValley", "RegionToxicValleyLocation", "HTO_LCP_HostileTakeOver_EventToggle_ToxicValleyRegion"),
    ("CranberryBog", "RegionCranberryBogLocation", "HTO_LCP_HostileTakeOver_EventToggle_CranberryBogRegion"),
    ("Mire", "RegionSwampForestLocation", "HTO_LCP_HostileTakeOver_EventToggle_TheMireRegion"),
    ("SavageDivide", "RegionMountainLocation", "HTO_LCP_HostileTakeOver_EventToggle_SavageDivideRegion"),
    ("AshHeap", "RegionMTRLocation", "HTO_LCP_HostileTakeOver_EventToggle_AshHeapRegion"),
)
FACTION_LIST = "HTO_FactionSelection_FormList"
MUTATION_LIST = "HTO_BossMutation_Valid_FormList"
FORMS = {
    "started_message": ("MESG", "HTO_HostileTakeOver_QuestMessage_Started"),
    "ended_message": ("MESG", "HTO_HostileTakeOver_QuestMessage_Ended"),
    "boss_message": ("MESG", "HTO_HostileTakeOver_QuestMessage_BossSpawn"),
    "announce_toggle": ("GLOB", "HTO_LCP_HostileTakeOver_QuestMessage_Toggle"),
    "boss_explosion": ("EXPL", "HTO_crBossSpawnExplosionVFX"),
    "boss_roar": ("SNDR", "HTO_crBossSpawnRoarSFX"),
}
REWARD = {
    "xp_global": ("GLOB", "HTO_LCP_QuestReward_XP"),
    "caps_global": ("GLOB", "HTO_LCP_QuestReward_Caps"),
    "scrip_global": ("GLOB", "HTO_LCP_QuestReward_LegendaryTokens"),
    "toggle": ("GLOB", "HTO_LCP_QuestReward_Toggle"),
    "scrip_item": ("CNCY", "LegendaryTokens"),
}
TUNING_GLOBALS = {
    "expire_seconds": "HTO_LCP_HostileTakeOver_QuestExpireGlobal_5Hrs",
    "marker_radius": "HTO_LCP_HostileTakeOver_TargetRadius_Marker",
    "location_radius": "HTO_LCP_HostileTakeOver_TargetRadius_Location",
}
MASTER_SCRIPT = "HostileTakeovers:HTO_MasterScript"
MASTER_PROPERTIES = {
    "boss_kill_fraction": "MobKillPercentToSpawnBoss",
    "remaining_fraction": "MobKillPercentToShowRemainingObjective",
}
# The legendary rank comes from the boss reward lists HTO_LegendaryItems_*_Rank4.
BOSS_RANK = 4


class Source(Protocol):
    def lookup(self, signature: str, editor_id: str) -> tuple[int, dict[str, list]]: ...
    def fields(self, object_id: int) -> tuple[str, str, dict[str, list]]: ...
    def ids(self, signature: str) -> list[int]: ...


class Converted(Protocol):
    def record(self, object_id: int) -> tuple[str, dict[str, list]] | None: ...


def local(value) -> int | None:
    try:
        return int(value["reference"]["object_id"], 16)
    except (KeyError, TypeError, ValueError):
        return None


def hex_id(object_id: int) -> str:
    return f"{object_id:06X}"


def position(fields: dict[str, list]) -> list[float] | None:
    data = first(fields, "DATA")
    if not isinstance(data, dict):
        return None
    point = [data.get(f"PositionRotationPosition{axis}") for axis in "XYZ"]
    return [float(value) for value in point] if all(isinstance(v, (int, float)) and math.isfinite(v) for v in point) else None


def converted_record(converted: Converted, object_id: int, signature: str | None = None) -> dict[str, list]:
    found = converted.record(object_id)
    if not found or (signature and found[0] != signature):
        raise ValueError(f"Converted {signature or 'record'} {hex_id(object_id)} is missing")
    return found[1]


def global_value(source: Source, editor_id: str) -> float:
    value = first(source.lookup("GLOB", editor_id)[1], "Value")
    if not isinstance(value, (int, float)) or not math.isfinite(value):
        raise ValueError(f"Global {editor_id} has no value")
    return float(value)


def script_properties(fields: dict[str, list], script: str) -> dict[str, object]:
    adapter = first(fields, "VirtualMachineAdapter") or {}
    for entry in adapter.get("Scripts", []):
        if entry.get("ScriptName") == script:
            return {item["propertyName"]: item.get("Value") for item in entry.get("Properties", [])}
    raise ValueError(f"{script} is not attached to {QUEST}")


def form_list(source: Source, editor_id: str) -> list[int]:
    return [object_id for object_id in (local(item) for item in source.lookup("FLST", editor_id)[1].get("FormID", [])) if object_id]


def faction_lists(keyword: str) -> tuple[str, str]:
    if keyword.startswith("SDOW_HTO_FactionSelection_"):
        name = keyword.removeprefix("SDOW_HTO_FactionSelection_").removesuffix("Shadow")
        return f"SDOW_HTO_LChar_Faction_{name}", f"SDOW_HTO_LChar_Faction_{name}_Boss"
    name = keyword.removeprefix("HTO_FactionSelection_")
    return f"HTO_LChar_Faction_{name}", f"HTO_LChar_Faction_{name}_Boss"


def build_factions(source: Source, converted: Converted) -> list[dict]:
    factions = []
    for keyword_id in form_list(source, FACTION_LIST):
        _, keyword, _ = source.fields(keyword_id)
        mob_name, boss_name = faction_lists(keyword)
        mob, _ = source.lookup("LVLN", mob_name)
        boss, _ = source.lookup("LVLN", boss_name)
        for object_id in (mob, boss):
            if not converted_record(converted, object_id, "LVLN").get("LVLO"):
                raise ValueError(f"Converted faction list {hex_id(object_id)} is empty")
        factions.append({"key": keyword.split("_FactionSelection_")[-1], "keyword": hex_id(keyword_id),
                         "mob": hex_id(mob), "boss": hex_id(boss)})
    if not factions:
        raise ValueError("The FO76 Infestation faction list is empty")
    return factions


def build_sites(source: Source, converted: Converted, regions: dict[int, str], world: int) -> tuple[list[dict], Counter]:
    ref_types = {name: source.lookup("LCRT", name)[0]
                 for name in ("LocationCenterMarker", "MapMarkerRefType", "Boss", "LocationClearActor")}
    records = {object_id: source.fields(object_id) for object_id in source.ids("LCTN")}

    def region_of(object_id: int | None) -> int | None:
        for _ in range(24):
            if object_id is None or object_id in regions:
                return object_id
            parent = first(records.get(object_id, ("", "", {}))[2], "ParentLocation")
            object_id = local(parent)
        return None

    skipped: Counter = Counter()
    sites = []
    for object_id, (_, editor_id, fields) in sorted(records.items()):
        region = region_of(object_id)
        if region is None or object_id == region:
            continue
        refs = first(fields, "MasterSpecialReferences") or []
        by_type: dict[int, list[int]] = {}
        worlds = set()
        for row in refs:
            kind, ref = local(row.get("MasterSpecialReferencesLocRefType")), local(row.get("MasterSpecialReferencesRef"))
            worlds.add(local(row.get("MasterSpecialReferencesWorldCell")))
            if kind and ref:
                by_type.setdefault(kind, []).append(ref)
        centers = by_type.get(ref_types["LocationCenterMarker"], [])
        if worlds != {world}:
            skipped["outside_world"] += 1
        elif not centers:
            skipped["no_center"] += 1
        elif not by_type.get(ref_types["MapMarkerRefType"]):
            skipped["no_map_marker"] += 1
        elif not by_type.get(ref_types["Boss"]):
            skipped["no_boss"] += 1
        elif len(by_type.get(ref_types["LocationClearActor"], [])) < MIN_CLEAR_ACTORS:
            skipped["few_clear_actors"] += 1
        else:
            try:
                converted_record(converted, object_id, "LCTN")
                center = position(converted_record(converted, centers[0]))
                clear = [ref for ref in by_type[ref_types["LocationClearActor"]] if converted.record(ref)]
                boss = [ref for ref in by_type[ref_types["Boss"]] if converted.record(ref)]
            except ValueError:
                skipped["not_converted"] += 1
                continue
            if center is None or len(clear) < MIN_CLEAR_ACTORS or not boss:
                skipped["not_converted"] += 1
                continue
            sites.append({"location": hex_id(object_id), "editor_id": editor_id, "region": hex_id(region),
                          "world": hex_id(world), "center": center,
                          "marker": hex_id(by_type[ref_types["MapMarkerRefType"]][0]),
                          "clear": [hex_id(ref) for ref in sorted(set(clear))],
                          "boss": [hex_id(ref) for ref in sorted(set(boss))]})
    if not sites:
        raise ValueError("No FO76 location qualifies as an Infestation site")
    return sites, skipped


def build_catalog(source: Source, converted: Converted, data_version: str = "") -> dict:
    quest_id, quest = source.lookup("QUST", QUEST)
    converted_record(converted, quest_id, "QUST")
    properties = script_properties(quest, MASTER_SCRIPT)
    tuning = {key: global_value(source, name) for key, name in TUNING_GLOBALS.items()}
    for key, name in MASTER_PROPERTIES.items():
        value = properties.get(name)
        if not isinstance(value, (int, float)) or not 0 < value <= 1:
            raise ValueError(f"{MASTER_SCRIPT}.{name} is not a kill fraction")
        tuning[key] = round(float(value), 6)

    def resolved(table: dict[str, tuple[str, str]]) -> dict[str, str]:
        result = {}
        for key, (signature, editor_id) in table.items():
            object_id, _ = source.lookup(signature, editor_id)
            # FO76 currencies convert to FO4 miscellaneous items.
            converted_record(converted, object_id, "MISC" if signature == "CNCY" else signature)
            result[key] = hex_id(object_id)
        return result

    regions, region_ids = [], {}
    for key, location_name, toggle_name in REGIONS:
        location_id, fields = source.lookup("LCTN", location_name)
        toggle_id, _ = source.lookup("GLOB", toggle_name)
        converted_record(converted, location_id, "LCTN")
        converted_record(converted, toggle_id, "GLOB")
        regions.append({"key": key, "location": hex_id(location_id), "toggle": hex_id(toggle_id)})
        region_ids[location_id] = key
    world, _ = source.lookup("WRLD", WORLD)
    converted_record(converted, world, "WRLD")
    mutations = [object_id for object_id in form_list(source, MUTATION_LIST) if converted.record(object_id)]
    sites, skipped = build_sites(source, converted, region_ids, world)
    return {
        "version": SCHEMA_VERSION,
        "plugin": PLUGIN,
        "source": {"quest": hex_id(quest_id), "data_version": data_version},
        "tuning": tuning,
        "forms": {"quest": hex_id(quest_id), **resolved(FORMS)},
        "reward": {**resolved(REWARD), "boss_rank": BOSS_RANK},
        "mutations": [hex_id(object_id) for object_id in mutations],
        "factions": build_factions(source, converted),
        "regions": regions,
        "sites": sites,
        "skipped": dict(sorted(skipped.items())),
    }


def png(width: int, height: int, rgba: bytes) -> bytes:
    def chunk(kind: bytes, payload: bytes) -> bytes:
        return struct.pack(">I", len(payload)) + kind + payload + struct.pack(">I", zlib.crc32(kind + payload))
    rows = b"".join(b"\0" + rgba[y * width * 4:(y + 1) * width * 4] for y in range(height))
    return (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0)) +
            chunk(b"IDAT", zlib.compress(rows, 9)) + chunk(b"IEND", b""))


def lossless_png(payload: bytes) -> tuple[int, int, int, bytes]:
    """Decode a DefineBitsLossless2 32-bit bitmap (premultiplied ARGB) to a straight-alpha PNG."""
    character, kind, width, height = struct.unpack_from("<HBHH", payload, 0)
    if kind != 5 or not width or not height:
        raise ValueError(f"Bitmap {character} uses unsupported lossless format {kind}")
    pixels = zlib.decompress(payload[7:])
    if len(pixels) != width * height * 4:
        raise ValueError(f"Bitmap {character} has {len(pixels)} bytes for {width}x{height}")
    out = bytearray(len(pixels))
    for i in range(0, len(pixels), 4):
        alpha = pixels[i]
        out[i + 3] = alpha
        if alpha:
            for channel in range(3):
                out[i + channel] = min(255, (pixels[i + 1 + channel] * 255 + alpha // 2) // alpha)
    return character, width, height, png(width, height, bytes(out))


def matrix_scale(matrix) -> float:
    return math.hypot(matrix.scale_x, matrix.rotate_skew_0) if matrix else 1.0


def place_object3(data: bytes) -> tuple[int, int, object] | None:
    """Decode the depth, character and matrix of a PlaceObject3 the SWF parser leaves raw."""
    from creation_lib.swf.types import MATRIX, BitReader

    reader = BitReader(data)
    flags, extra, depth = reader.read_ui8(), reader.read_ui8(), reader.read_ui16()
    has_character, has_matrix = flags & 0x02, flags & 0x04
    if extra & 0x08 or (extra & 0x10 and has_character):
        reader.read_string()
    if not has_character:
        return None
    character = reader.read_ui16()
    return depth, character, MATRIX.parse(reader) if has_matrix else None


def placements(doc, sprite_id: int) -> dict[int, tuple[int, object]]:
    """First character placed at each depth anywhere in the sprite's loop."""
    found: dict[int, tuple[int, object]] = {}
    for frame in doc.sprites[sprite_id].timeline.frames:
        for depth, entry in frame.placements.items():
            if entry.character_id is not None:
                found.setdefault(depth, (entry.character_id, entry.matrix))
    for tag in getattr(doc, "tags", ()):
        if getattr(tag, "sprite_id", None) != sprite_id:
            continue
        for nested in getattr(tag, "tags", ()):
            if getattr(nested, "tag_id", None) == 70 and (placed := place_object3(nested.data)):
                found.setdefault(placed[0], placed[1:])
    return found


def fog_layers(doc, bitmaps: dict[int, bytes]) -> dict:
    """Walk the MapCloud symbol to its bitmap-filled shapes, in stacking order (each depth at its first placement)."""
    symbol = next((character for character, name in doc.symbols if name == FOG_SYMBOL), None)
    if symbol not in doc.sprites:
        raise ValueError(f"{MAP_MOVIE} does not export {FOG_SYMBOL}")
    root = doc.sprites[symbol].timeline
    radius = None
    layers: dict[int, dict] = {}

    def extent(character: int) -> float | None:
        shape = doc.shapes.get(character)
        if shape:
            left, top, right, bottom = shape.bounds_px
            return max(right - left, bottom - top)
        sprite = doc.sprites.get(character)
        if not sprite or not sprite.timeline.frames:
            return None
        sizes = [extent(entry.character_id) for entry in sprite.timeline.frames[0].placements.values()]
        sizes = [size for size in sizes if size]
        return max(sizes) if sizes else None

    def walk(character: int, scale: float, frames: int, order: list[int]):
        shape = doc.shapes.get(character)
        if shape:
            left, top, right, bottom = shape.bounds_px
            for fill in shape.fill_styles:
                if fill.bitmap_id in bitmaps:
                    size = max(right - left, bottom - top) * scale
                    layer = layers.setdefault(fill.bitmap_id, {"order": order, "size": 0.0, "frames": frames})
                    layer["size"] = max(layer["size"], size)
                    layer["frames"] = max(layer["frames"], frames)
            return
        sprite = doc.sprites.get(character)
        if not sprite or not sprite.timeline.frames:
            return
        count = sprite.timeline.frame_count
        for depth, (child, matrix) in sorted(placements(doc, character).items()):
            walk(child, scale * matrix_scale(matrix), max(frames, count), order + [depth])

    for depth, entry in sorted(root.frames[0].placements.items()):
        if entry.name == RADIUS_CLIP:
            size = extent(entry.character_id)
            radius = size * matrix_scale(entry.matrix) / 2 if size else None
        else:
            walk(entry.character_id, matrix_scale(entry.matrix), 1, [depth])
    if not radius or not layers:
        raise ValueError(f"{FOG_SYMBOL} has no {RADIUS_CLIP} or no bitmap layers")
    ordered = sorted(layers.items(), key=lambda item: item[1]["order"])
    fps = float(getattr(doc.header, "fps", 30) or 30)
    return {"radius": radius, "layers": [
        {"bitmap": bitmap, "scale": round(layer["size"] / (2 * radius), 4), "period": round(layer["frames"] / fps, 3)}
        for bitmap, layer in ordered]}


def extract_fog(movie: bytes) -> tuple[dict, dict[str, bytes]]:
    from creation_lib.swf.parser import parse_swf

    doc = parse_swf(movie)
    raw = {}
    for tag in doc.tags:
        if getattr(tag, "tag_id", None) == 36 and len(getattr(tag, "data", b"")) > 7:
            raw[struct.unpack_from("<H", tag.data, 0)[0]] = tag.data
    layout = fog_layers(doc, raw)
    files, layers = {}, []
    for index, layer in enumerate(layout["layers"]):
        _, width, height, image = lossless_png(raw[layer["bitmap"]])
        name = f"fog{index}.png"
        files[name] = image
        layers.append({"file": name, "width": width, "height": height, "scale": layer["scale"],
                       "period": layer["period"], "sha256": hashlib.sha256(image).hexdigest()})
    return {"symbol": FOG_SYMBOL, "layers": layers}, files


class NativeSource:
    def __init__(self, native, handle: int):
        self.native, self.handle, self.reader = native, handle, NativeReader(native, handle)

    def lookup(self, signature, editor_id):
        return self.reader.lookup(signature, editor_id)

    def fields(self, object_id):
        return self.reader.fields(object_id)

    def ids(self, signature):
        return [raw & 0xFFFFFF for raw in self.native.plugin_handle_record_form_ids(self.handle, [signature])]


class NativeConverted:
    def __init__(self, native, handle: int):
        self.native, self.handle = native, handle
        # Converted records carry the plugin's own master index in their raw FormID.
        self.own = len(plugin_masters(native, handle)) << 24
        self.cache: dict[int, tuple[str, dict[str, list]] | None] = {}

    def record(self, object_id):
        if object_id not in self.cache:
            payload = self.native.plugin_handle_inspect_record(self.handle, self.own | object_id)
            data = json.loads(payload) if payload else None
            self.cache[object_id] = (data["signature"], fields_by_name(data["record"])) if data else None
        return self.cache[object_id]


def data_version(source_root: Path) -> str:
    info = Path(source_root) / "buildinfo.txt"
    if not info.is_file():
        return ""
    for line in info.read_text(encoding="utf-8", errors="replace").splitlines():
        key, _, value = line.partition("=")
        if key.strip() == "data_version":
            return value.strip()
    return ""


def write_bundle(output_mod: Path, catalog: dict, fog: dict, images: dict[str, bytes]) -> dict:
    bundle = Path(output_mod) / OUTPUT
    bundle.mkdir(parents=True, exist_ok=True)
    manifest = {"version": SCHEMA_VERSION, "fog": fog, "files": []}
    for name, content in [*images.items(), ("map-extension.js", (RESOURCES / "map-extension.js").read_bytes())]:
        (bundle / name).write_bytes(content)
        manifest["files"].append({"path": (OUTPUT / name).as_posix(), "sha256": hashlib.sha256(content).hexdigest()})
    (bundle / "presentation.json").write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    target = Path(output_mod) / CATALOG
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_text(json.dumps(catalog, indent=2) + "\n", encoding="utf-8")
    return manifest


def convert_infestations(source_root: Path, output_mod: Path, *, source_plugin: Path, converted_plugin: Path) -> dict:
    movie = Path(source_root) / MAP_MOVIE
    if not movie.is_file():
        raise FileNotFoundError(f"Infestation fog requires the extracted FO76 map movie: {movie}")
    fog, images = extract_fog(movie.read_bytes())
    native = load_esp_native()
    handles: list[int] = []
    try:
        handles.append(native.plugin_handle_load_index(str(source_plugin), game="fo76"))
        handles.append(native.plugin_handle_load_index(str(converted_plugin), game="fo4"))
        catalog = build_catalog(NativeSource(native, handles[0]), NativeConverted(native, handles[1]),
                                data_version(source_root))
    finally:
        for handle in reversed(handles):
            native.plugin_handle_close(handle)
    catalog["fog_sha256"] = [layer["sha256"] for layer in fog["layers"]]
    return {"catalog": catalog, "presentation": write_bundle(output_mod, catalog, fog, images)}


def summarize(catalog: dict) -> Iterable[str]:
    regions = Counter(site["region"] for site in catalog["sites"])
    yield f"{len(catalog['sites'])} sites, {len(catalog['factions'])} factions, {len(catalog['mutations'])} boss mutations"
    yield "sites by region: " + ", ".join(f"{row['key']}={regions[row['location']]}" for row in catalog["regions"])
    yield "skipped: " + ", ".join(f"{key}={value}" for key, value in catalog["skipped"].items())
