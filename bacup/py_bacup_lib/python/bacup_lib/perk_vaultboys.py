"""FO76 HUD perk Vault Boys as standalone FO4 HUD perk clips.

FO76's HUD builds a perk popup from a ``perkslibrary_small.swf`` class (``getDefinitionByName`` on
the space-stripped file name, then its ``animated`` frame). FO4's HUD instead loads
``Interface/<path>.swf`` and plays its root timeline until the last frame, so each clip becomes a
550x400 movie whose root runs as long as the art.
"""

from __future__ import annotations

import hashlib
import json
import struct
import zlib
from pathlib import Path

from bacup_lib.legendary_perks_ui import swf_tags
from bacup_lib.quest_area_ui import placed_characters, symbol_ids, tag
from bacup_lib.status_hud_source import sprite_tags

LIBRARY = "perkslibrary_small.swf"
OUTPUT = Path("Interface/Components/VaultBoys/Perks")
CATALOG = Path("F4SE/Plugins/B21_TalesFromAppalachia/perk_vaultboys.json")
SCRIPT = Path(__file__).with_name("resources") / "perk_vaultboys" / "B21PerkVaultBoy.as"
ROOT_CLASS = "B21PerkVaultBoy"
ART_NAME = "Art_mc"
STAGE = (550, 400)
# FO4 scales the clip by 128 / its frame-1 height; leave a margin inside the stage rectangle.
FILL = 0.9
SAMPLES = 8
# A one-frame clip would reach its last frame at once and be faded out immediately.
STATIC_SECONDS = 2

SHAPES = {2, 22, 32, 83, 46, 84}
SPRITE = 39
BITMAPS = {6, 20, 21, 35, 36, 90}
TEXT = {11, 33, 37}
COMPANIONS = {78}
DEFINITIONS = SHAPES | BITMAPS | TEXT | {SPRITE}


def _definitions(tags) -> dict[int, tuple[int, bytes]]:
    return {struct.unpack_from("<H", payload)[0]: (code, payload) for code, payload in tags if code in DEFINITIONS}


def animated_clip(definitions: dict[int, tuple[int, bytes]], class_id: int) -> int | None:
    """The sprite FO76 shows on the class's ``animated`` frame (the display list, not just new placements)."""
    code, payload = definitions.get(class_id, (0, b""))
    if code != SPRITE:
        return None
    depths, labelled = {}, False
    for code, body in sprite_tags(payload):
        if code == 43:
            labelled = body.split(b"\0", 1)[0] == b"animated"
        elif code == 26 and body[0] & 0x02:
            depths[struct.unpack_from("<H", body, 1)[0]] = struct.unpack_from("<H", body, 3)[0]
        elif code == 70 and body[0] & 0x02:
            at = body.index(b"\0", 4) + 1 if body[1] & 0x18 else 4
            depths[struct.unpack_from("<H", body, 2)[0]] = struct.unpack_from("<H", body, at)[0]
        elif code == 28:
            depths.pop(struct.unpack_from("<H", body)[0], None)
        elif code == 1:
            if labelled:
                shown = list(depths.values())
                return shown[0] if len(shown) == 1 and definitions.get(shown[0], (0,))[0] == SPRITE else None
    return None


def art_closure(tags, definitions: dict[int, tuple[int, bytes]], root: int) -> list[tuple[int, bytes]]:
    bitmaps = [cid for cid, (code, _) in definitions.items() if code in BITMAPS]
    wanted, pending = set(), [root]
    while pending:
        cid = pending.pop()
        if cid in wanted:
            continue
        if cid not in definitions:
            raise ValueError(f"character {cid} is not a supported definition")
        code, payload = definitions[cid]
        if code in TEXT:
            raise ValueError(f"character {cid} is text, which needs FO76 fonts")
        wanted.add(cid)
        if code == SPRITE:
            pending.extend(placed_characters(payload))
        elif code in SHAPES:
            # Bitmap fill styles are type 0x40-0x43 followed by the bitmap id; a false match only adds a bitmap.
            pending.extend(bitmap for bitmap in bitmaps
                           if any(bytes((kind,)) + struct.pack("<H", bitmap) in payload for kind in range(0x40, 0x44)))
    return [(code, payload) for code, payload in tags
            if code in DEFINITIONS | COMPANIONS and struct.unpack_from("<H", payload)[0] in wanted]


def _rect(x_max: int, y_max: int) -> bytes:
    bits = max(x_max.bit_length(), y_max.bit_length()) + 1
    value = bits
    for item in (0, x_max, 0, y_max):
        value = value << bits | item
    total = 5 + 4 * bits
    return (value << (-total % 8)).to_bytes((total + 7) // 8, "big")


def _movie(version: int, rate: bytes, frames: int, body: bytes) -> bytes:
    header = _rect(STAGE[0] * 20, STAGE[1] * 20) + rate + struct.pack("<H", frames)
    return b"CWS" + bytes((version,)) + struct.pack("<I", 8 + len(header) + len(body)) + zlib.compress(header + body, 9)


def _stage_shape(shape_id: int) -> bytes:
    from creation_lib.swf.shapes import EndShape, ShapeDef, StraightEdge, StyleChange
    from creation_lib.swf.tags import DefineShapeTag
    from creation_lib.swf.types import RGBA, FillStyle

    width, height = STAGE[0] * 20, STAGE[1] * 20
    # A transparent stage-sized fill fixes FO4's 128 / height scale, whatever the art's first frame shows.
    shape = ShapeDef(shape_id=shape_id, bounds=(0, 0, width, height),
                     fill_styles=[FillStyle(fill_type=0, color=RGBA(0, 0, 0, 0))], line_styles=[],
                     records=[StyleChange(move_x=0, move_y=0, fill1=1), StraightEdge(width, 0),
                              StraightEdge(0, height), StraightEdge(-width, 0), StraightEdge(0, -height), EndShape()],
                     shape_version=3)
    return DefineShapeTag(shape=shape, tag_id=32).to_bytes()


def _art_bounds(version: int, rate: bytes, art: list[tuple[int, bytes]], clip: int, frames: int):
    from creation_lib.swf.native_runtime import sprite_frame_bounds

    movie = _movie(version, rate, 1, tag(69, struct.pack("<I", 0x08)) + b"".join(tag(*entry) for entry in art)
                   + tag(1, b"") + tag(0, b""))
    samples = sorted({1 + (frames - 1) * step // (SAMPLES - 1) for step in range(SAMPLES)})
    boxes = [box for box in sprite_frame_bounds(movie, clip, samples) if box is not None]
    if not boxes:
        raise ValueError("clip draws nothing")
    return (min(b[0] for b in boxes), min(b[1] for b in boxes), max(b[2] for b in boxes), max(b[3] for b in boxes))


def build_vaultboy(version: int, rate: bytes, tags, definitions, clip: int, abc: bytes) -> bytes:
    from creation_lib.swf.tags import PlaceObject2Tag
    from creation_lib.swf.types import MATRIX

    art = art_closure(tags, definitions, clip)
    frames = struct.unpack_from("<H", definitions[clip][1], 2)[0]
    x0, y0, x1, y1 = _art_bounds(version, rate, art, clip, frames)
    scale = min(STAGE[0] * FILL / max(x1 - x0, 1), STAGE[1] * FILL / max(y1 - y0, 1))
    matrix = MATRIX(scale_x=scale, scale_y=scale,
                    translate_x=round((STAGE[0] / 2 - scale * (x0 + x1) / 2) * 20),
                    translate_y=round((STAGE[1] / 2 - scale * (y0 + y1) / 2) * 20))
    stage_id = max(definitions) + 1
    if frames == 1:
        frames = max(1, round(STATIC_SECONDS * int.from_bytes(rate, "little") / 256))
    exports = struct.pack("<HH", 1, 0) + ROOT_CLASS.encode() + b"\0"
    timeline = (tag(69, struct.pack("<I", 0x08)) + b"".join(tag(*entry) for entry in art)
                + tag(32, _stage_shape(stage_id)) + tag(82, abc) + tag(76, exports)
                + tag(26, PlaceObject2Tag(depth=1, character_id=stage_id).to_bytes())
                + tag(26, PlaceObject2Tag(depth=2, character_id=clip, matrix=matrix, name=ART_NAME).to_bytes())
                + tag(1, b"") * frames + tag(0, b""))
    return _movie(version, rate, frames, timeline)


def convert_perk_vaultboys(source_root: Path, output_data: Path) -> dict:
    from creation_lib.swf.native_runtime import compile_as3_do_abc

    source = (Path(source_root) / "interface" / LIBRARY).read_bytes()
    body = zlib.decompress(source[8:]) if source[:3] == b"CWS" else source[8:]
    rect = (5 + 4 * (body[0] >> 3) + 7) // 8
    rate = body[rect:rect + 2]
    del body
    tags = swf_tags(source)
    definitions = _definitions(tags)
    abc = compile_as3_do_abc([SCRIPT.read_text(encoding="utf-8")])
    output = Path(output_data) / OUTPUT
    output.mkdir(parents=True, exist_ok=True)
    files, skipped = [], {}
    for name, class_id in sorted(symbol_ids(tags).items()):
        if "." in name:
            continue
        clip = animated_clip(definitions, class_id)
        if clip is None:
            continue
        try:
            movie = build_vaultboy(source[3], rate, tags, definitions, clip, abc)
        except ValueError as error:
            skipped[name] = str(error)
            continue
        # FO76 resolves the class from the space-stripped file name; a stale loose copy of the same name is replaced.
        (output / f"{name}.swf").write_bytes(movie)
        files.append({"path": (OUTPUT / f"{name}.swf").as_posix(), "class": name,
                      "sha256": hashlib.sha256(movie).hexdigest()})
    manifest = {"schema_version": 1, "source": LIBRARY, "source_sha256": hashlib.sha256(source).hexdigest(),
                "script_sha256": hashlib.sha256(SCRIPT.read_bytes()).hexdigest(), "files": files, "skipped": skipped}
    (output / "conversion.json").write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    return manifest


def clip_name(icon: str) -> str:
    """FO76's HUD takes the file stem of a perk icon path, or a bare class name, without spaces."""
    stem = icon.replace("\\", "/").rsplit("/", 1)[-1]
    if stem.lower().endswith(".swf"):
        stem = stem[:-4]
    return stem.replace(" ", "")


def available_clips(output_data: Path) -> dict[str, str]:
    """Space-stripped, lower-case clip name -> file name; a generated class file wins over a spaced loose copy."""
    folder = Path(output_data) / OUTPUT
    clips: dict[str, str] = {}
    for path in sorted(folder.glob("*.swf"), key=lambda item: " " in item.name):
        clips.setdefault(clip_name(path.name).lower(), path.name)
    return clips


def build_catalog(source, resolver, base_resolver, clips: dict[str, str], source_name: str = "SeventySix.esm") -> dict:
    from bacup_lib.reputation_catalog import first, reference_id

    def form(signature: str, editor_id: str) -> str | None:
        return resolver.resolve(signature, editor_id) or (base_resolver.resolve(signature, editor_id) if base_resolver else None)

    def sound(reference) -> str | None:
        object_id = reference_id(reference)
        if object_id is None:
            return None
        signature, editor_id, _ = source.fields(object_id)
        return form("SNDR", editor_id) if signature == "SNDR" and editor_id else None

    entries = {name: {"path": (Path(*OUTPUT.parts[1:]) / file).as_posix(), "sound": None, "cooldown": None}
               for name, file in clips.items()}
    perks, missing = [], set()
    for raw_id, _, editor_id, _ in source.native.plugin_handle_search_records(
            source.handle, "*", signatures=["PERK"], case_sensitive=False):
        fields = source.fields(raw_id)[2]
        icon = first(fields, "PerkActivationIcon")
        if not icon or not editor_id:
            continue
        name = clip_name(icon).lower()
        if name not in entries:
            missing.add(clip_name(icon))
            continue
        activation = sound(first(fields, "PerkActivationSound"))
        # Tales' perk-card catalogs name ranks by their FO76 source form, so keep that key beside the converted one.
        perks.append({"perk": form("PERK", editor_id), "source_perk": f"{raw_id & 0xFFFFFF:06X}:{source_name}",
                      "editor_id": editor_id, "clip": name, "sound": activation})
        entries[name]["sound"] = entries[name]["sound"] or activation
    # Script-driven popups (VaultBoyHUDEffectScript) carry their own sound and repeat cooldown.
    for raw_id, _, editor_id, _ in source.native.plugin_handle_search_records(
            source.handle, "*", signatures=["MGEF"], case_sensitive=False):
        for adapter in source.fields(raw_id)[2].get("VirtualMachineAdapter", []):
            for script in adapter.get("Scripts", []):
                if script.get("ScriptName") != "VaultBoyHUDEffectScript":
                    continue
                values = {item.get("propertyName"): item.get("Value") for item in script.get("Properties", [])}
                icon = values.get("VaultBoySwfName")
                if not isinstance(icon, str) or clip_name(icon.strip('"')).lower() not in entries:
                    continue
                entry = entries[clip_name(icon.strip('"')).lower()]
                reference = values.get("VaultBoySound")
                entry["sound"] = entry["sound"] or sound(reference.get("FormID") if isinstance(reference, dict) else None)
                cooldown = values.get("Cooldown")
                if isinstance(cooldown, (int, float)) and cooldown > 0:
                    entry["cooldown"] = max(float(cooldown), entry["cooldown"] or 0.0)
    generic, _ = source.lookup("SNDR", "UIPerkActivateGeneric")
    return {"schema_version": 1, "default_sound": form("SNDR", source.fields(generic)[1]),
            "clips": dict(sorted(entries.items())), "perks": sorted(perks, key=lambda item: item["editor_id"]),
            "missing_clips": sorted(missing)}


def emit_perk_vaultboys(source_plugin: Path, converted_plugin: Path, output_mod: Path,
                        target_data_dir: Path | None = None) -> dict:
    from bacup_lib.esp_native_runtime import load_esp_native
    from bacup_lib.reputation_catalog import NativeReader, NativeResolver, plugin_masters

    clips = available_clips(Path(output_mod) / "data")
    if not clips:
        raise FileNotFoundError("No converted perk Vault Boy clips; run convert_perk_vaultboys first")
    native = load_esp_native()
    handles = []
    try:
        source = native.plugin_handle_load_index(str(source_plugin), game="fo76")
        handles.append(source)
        converted = native.plugin_handle_load_index(str(converted_plugin), game="fo4")
        handles.append(converted)
        base_resolver = None
        base_path = Path(target_data_dir or Path(converted_plugin).parent) / "Fallout4.esm"
        if base_path.is_file():
            base = native.plugin_handle_load_index(str(base_path), game="fo4")
            handles.append(base)
            base_resolver = NativeResolver(native, base_path.name, base, plugin_masters(native, base))
        catalog = build_catalog(NativeReader(native, source),
                                NativeResolver(native, Path(converted_plugin).name, converted,
                                               plugin_masters(native, converted)),
                                base_resolver, clips, Path(source_plugin).name)
    finally:
        for handle in reversed(handles):
            native.plugin_handle_close(handle)
    destination = Path(output_mod) / CATALOG
    destination.parent.mkdir(parents=True, exist_ok=True)
    temporary = destination.with_suffix(".tmp")
    temporary.write_text(json.dumps(catalog, indent=2) + "\n", encoding="utf-8")
    temporary.replace(destination)
    return catalog
