from __future__ import annotations

import hashlib
import json
import struct
from pathlib import Path

from bacup_lib.daily_ops_ui import replace_classes
from bacup_lib.legendary_perks_ui import imports, remap_menu_fonts, swf_tags
from bacup_lib.quest_area_ui import symbol_ids, tag
from bacup_lib.status_hud_source import placement, replace_tags, sprite_tags, transform
from bacup_lib.translations import TRANSLATIONS, merge_ui_translations, translation_path

SOURCE = "hudreputationmeter.swf"
OUTPUT_MOVIE = "reputationhud.swf"
OUTPUT = Path("Interface/B21/TalesFromAppalachia/Reputation")
BRIDGE = Path(__file__).with_name("resources") / "reputation/HUDReputationUpdatesWidget.as"
ROOT_CLASS = "B21Reputation_HUDReputationUpdatesWidget"
TRANSLATION_KEYS = {
    "$Crater",
    "$CraterReputation",
    "$Foundation",
    "$FoundationReputation",
    "$REPUTATION",
    "$REPUTATIONINCREASED",
    "$ReputationCurrentStanding",
    *(f"$ReputationStatus{index}" for index in range(7)),
}


def reputation_translations(interface: Path) -> list[str]:
    data = (interface / "translate_en.txt").read_bytes()
    encoding = "utf-16" if data.startswith((b"\xff\xfe", b"\xfe\xff")) else "utf-8-sig"
    found: dict[str, str] = {}
    for line in data.decode(encoding).splitlines():
        key = line.split("\t", 1)[0]
        if key in TRANSLATION_KEYS:
            found[key] = line
    missing = sorted(TRANSLATION_KEYS - found.keys())
    if missing:
        raise ValueError(f"Reputation translations are missing: {', '.join(missing)}")
    return [found[key] for key in sorted(found)]


def source_closure(interface: Path) -> dict[str, bytes]:
    files = {path.name.casefold(): path for path in interface.iterdir() if path.is_file()}
    pending = [SOURCE]
    closure: dict[str, bytes] = {}
    while pending:
        name = pending.pop().casefold()
        if name in closure:
            continue
        if "/" in name or "\\" in name or not name.endswith(".swf"):
            raise ValueError(f"Unsupported reputation UI import: {name}")
        if name not in files:
            raise FileNotFoundError(f"Missing reputation UI import: {name}")
        closure[name] = files[name].read_bytes()
        pending.extend(imports(closure[name]))
    return closure


def bind_document_widget(movie: bytes) -> bytes:
    entries = swf_tags(movie)
    symbols = symbol_ids(entries)
    widget = symbols["HUDReputationUpdatesWidget"]
    placed = [item for code, body in entries if (item := placement(code, body)) is not None]
    if len(placed) != 1 or placed[0].character_id != widget or transform(placed[0]) != [1, 0, 0, 1, 0, 0]:
        raise ValueError("Reputation HUD root placement changed; re-audit the source timeline")
    timeline = next(body for code, body in entries if code == 39 and struct.unpack_from("<H", body)[0] == widget)
    if struct.unpack_from("<H", timeline, 2)[0] != 1 or 0 in symbols.values():
        raise ValueError("Reputation HUD root binding changed")
    exports = [(0 if cid == widget else cid, name) for name, cid in symbols.items()]
    bindings = struct.pack("<H", len(exports)) + b"".join(
        struct.pack("<H", cid) + name.encode() + b"\0" for cid, name in exports)
    entries = [(code, body) for code, body in entries
               if code not in (0, 1, 26, 70, 76)
               and not (code == 39 and struct.unpack_from("<H", body)[0] == widget)]
    return replace_tags(movie, [*entries, (76, bindings), *sprite_tags(timeline)])


def build_movie(source: bytes, dependencies) -> bytes:
    from creation_lib.swf.native_runtime import rename_as3_classes, unbacked_symbol_classes

    movie = replace_classes(source, {"HUDReputationUpdatesWidget": BRIDGE}, dependencies)
    movie = bind_document_widget(movie)
    movie = bind_faction_labels(movie)
    movie = rename_as3_classes(movie, "B21Reputation", keep=("scaleform.gfx",))
    if symbol_ids(swf_tags(movie)).get(ROOT_CLASS) != 0 or unbacked_symbol_classes(movie):
        raise ValueError("Reputation HUD document class is not bound")
    return remap_menu_fonts(movie)


def bind_faction_labels(movie: bytes) -> bytes:
    entries = swf_tags(movie)
    symbols = symbol_ids(entries)
    clips = {symbols["HUDReputationMeter_fla." + name] for name in (
        "FactionIcon_12", "FactionStatusGraphic_13", "FactionStatusGraphicFlipped_32")}
    output = []
    for code, body in entries:
        if code == 39 and struct.unpack_from("<H", body)[0] in clips:
            timeline = []
            found = set()
            for child_code, child_body in sprite_tags(body):
                timeline.append((child_code, child_body))
                if child_code == 43 and child_body in (b"crater\0", b"foundation\0"):
                    found.add(child_body)
                    # The source calls both title-case and uppercase labels on lowercase timelines.
                    timeline.extend((43, alias) for alias in (child_body.title(), child_body.upper()))
            if found != {b"crater\0", b"foundation\0"}:
                raise ValueError("Reputation faction frame labels changed; re-audit the source")
            body = body[:4] + b"".join(tag(c, p) for c, p in timeline)
        output.append((code, body))
    return replace_tags(movie, output)


def convert_reputation_ui(source_root: Path, output_data: Path) -> dict:
    interface = Path(source_root) / "interface"
    translations = reputation_translations(interface)
    closure = source_closure(interface)
    converted_root = build_movie(closure[SOURCE], closure.values())
    converted: dict[str, bytes] = {}
    for name, source in sorted(closure.items()):
        destination = OUTPUT_MOVIE if name == SOURCE else name
        converted[destination] = remap_menu_fonts(converted_root if name == SOURCE else source)
    manifest = {
        "schema_version": 1,
        "source_movie": SOURCE,
        "source_root_class": "HUDReputationUpdatesWidget",
        "document_class": ROOT_CLASS,
        "source_factions": ["Crater", "Foundation"],
        "excluded_source_movies": ["reputationlibrary.swf", "socialreputationwidget.swf"],
        "bridge_sha256": hashlib.sha256(BRIDGE.read_bytes()).hexdigest(),
        "menu": (OUTPUT / OUTPUT_MOVIE).as_posix(),
        "code_object": "root1",
        "files": [],
    }
    for destination, data in sorted(converted.items()):
        source_name = SOURCE if destination == OUTPUT_MOVIE else destination
        manifest["files"].append({
            "path": (OUTPUT / destination).as_posix(),
            "source": source_name,
            "source_sha256": hashlib.sha256(closure[source_name]).hexdigest(),
            "sha256": hashlib.sha256(data).hexdigest(),
        })
    output = Path(output_data) / OUTPUT
    output.mkdir(parents=True, exist_ok=True)
    for destination, data in converted.items():
        (output / destination).write_bytes(data)
    (output / "conversion.json").write_text(
        json.dumps(manifest, indent=2) + "\n", encoding="utf-8", newline="\n"
    )
    merge_ui_translations(output_data, translations)
    return manifest


if __name__ == "__main__":
    import argparse

    parser = argparse.ArgumentParser(description="Convert the FO76 reputation HUD for Tales.")
    parser.add_argument("--source-root", type=Path, required=True)
    parser.add_argument("--output-data", type=Path, required=True)
    args = parser.parse_args()
    result = convert_reputation_ui(args.source_root, args.output_data)
    print(f"Wrote {len(result['files'])} files to {args.output_data / OUTPUT}")
