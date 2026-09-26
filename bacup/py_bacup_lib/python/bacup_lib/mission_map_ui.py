from __future__ import annotations

import hashlib
import json
import tempfile
from pathlib import Path

from PIL import Image

from bacup_lib.fullscreen_map import PLUGIN_ASSETS_REL, align_to_blocks, write_ui_dds
from creation_lib.swf import native_runtime


OUTPUT = PLUGIN_ASSETS_REL / "missions"
ICONS = {"uplink": "DOMode_Uplink", "decryption": "DOMode_Uplink2"}
EXPEDITIONS = {
    "6baa3d": "expeditions_atlanticcity02.dds",
    "6b231c": "expeditions_atlanticcity01.dds",
    "6274ec": "expeditions_thepitt01.dds",
    "648280": "expeditions_thepitt04.dds",
}
DISTRICT_ICONS = {1: "CityMarker", 3: "FactoryMarker", 4: "MonumentMarker",
                  10: "TownRuinsMarker", 35: "PierMarker"}
SPRITES = {
    "dailyopsiconlibrary.swf": [(name, symbol, 1) for name, symbol in ICONS.items()],
    "expeditions.swf": [("outline-pitt", "Expeditions_fla.DottedLines_", 1),
                         ("outline-atlanticcity", "Expeditions_fla.DottedLines_", 2)],
    "mapmarkerslibrary.swf": [(f"district-icon-{index}", symbol, 1) for index, symbol in DISTRICT_ICONS.items()]
                            + [("camp", "DefaultCampMarker", 1)],
}


def _fields(record):
    result = {}
    for field in record.get("fields", []):
        result.update(field)
    return result


def _text(value, language):
    if isinstance(value, str):
        return value
    if isinstance(value, dict) and isinstance(value.get("Value"), str):
        return value["Value"]
    values = value.get("Values", []) if isinstance(value, dict) else []
    for candidate in (language, "English"):
        for item in values:
            if item.get("Language") == candidate:
                return item.get("String", "")
    return ""


def read_expedition_catalog(source_plugin: Path, source_root: Path, language: str) -> dict:
    from creation_lib.esp import Plugin

    with Plugin.load(source_plugin, game="fo76", language=language, lazy_index=True,
                     strings_dirs=[source_root / "strings", source_plugin.parent / "strings"]) as plugin:
        records = {f"{row[3]:06x}": _fields(plugin.read_authoring_record(row[4]) or {})
                   for row in plugin.record_index_rows(signatures=["DIST"])}
        locations, districts = {}, []
        for key, fields in records.items():
            parent = fields.get("ParentDistrict", {}).get("reference", {}).get("object_id", "").lower()
            if parent not in records:
                continue
            def entry(identifier, data):
                if not _text(data.get("Name"), language):
                    raise ValueError(f"Unresolved Expedition district name {identifier}: {data.get('Name')!r}")
                return {"id": identifier, "name": _text(data.get("Name"), language),
                        "description": _text(data.get("Description"), language), "texture": data.get("Image", "")}
            if parent not in locations:
                locations[parent] = entry(parent, records[parent])
                locations[parent]["outline"] = {"75ec22": "outline-pitt", "75ec23": "outline-atlanticcity"}.get(parent, "")
            district = entry(key, fields)
            position = fields.get("Position", {})
            district.update(location=parent, x=position.get("Float320", .5), y=position.get("Float321", .5))
            icon = int.from_bytes(bytes.fromhex(fields.get("DICO", {}).get("raw_hex", "")), "little")
            district["icon"] = f"district-icon-{icon}" if icon in DISTRICT_ICONS else ""
            quest = fields.get("Quest", {}).get("reference", {}).get("object_id", "").lower()
            district["quest"] = quest
            if quest:
                quest_fields = _fields(plugin.read_authoring_record(int(quest, 16)) or {})
                district["missionDescription"] = _text(quest_fields.get("DESC"), language)
            districts.append(district)
        return {"version": 1, "locations": list(locations.values()), "districts": districts}


def _export_sprites(movie: Path, folder: Path, specs):
    data = movie.read_bytes()
    symbols = native_runtime.list_symbols(data)
    exported = []
    for name, symbol, frame in specs:
        matches = [export for _, export in symbols
                   if export == symbol or (symbol.endswith("_") and export.startswith(symbol))]
        if len(matches) != 1:
            raise ValueError(f"Expected one mission UI sprite named {symbol}")
        export = matches[0]
        destination = folder / f"{name}.png"
        try:
            destination.write_bytes(native_runtime.render_symbol_png(data, export, frame, 2.0))
        except ValueError as error:
            raise ValueError(f"Mission artwork {movie.name}, {export}, frame {frame}: {error}") from error
        exported.append((name, export, frame, destination))
    return exported


def convert_mission_map_ui(source_root: Path, output_data: Path, *,
                           source_plugin: Path | None = None, language: str = "English") -> dict:
    source_root, output_data = Path(source_root), Path(output_data)
    textures = source_root / "textures/interface/expeditions"
    sources = [*(source_root / "interface" / movie for movie in SPRITES), *(textures / name for name in EXPEDITIONS.values())]
    for source in sources:
        if not source.is_file():
            raise FileNotFoundError(f"Mission map artwork has not been extracted: {source}")
    catalog = read_expedition_catalog(Path(source_plugin), source_root, language) if source_plugin else None
    manifest = {"version": 2, "files": []}

    def emit(source, name, image, symbol=None, frame=None):
        pixels = align_to_blocks(image)
        if not pixels.getchannel("A").getbbox():
            raise ValueError(f"Empty mission map artwork: {source}")
        destination = write_ui_dds(pixels, output_data / OUTPUT / f"{name}.dds")
        manifest["files"].append({
            "path": destination.relative_to(output_data).as_posix(),
            "source": source.relative_to(source_root).as_posix(),
            "source_sha256": hashlib.sha256(source.read_bytes()).hexdigest(),
            "symbol": symbol, "frame": frame, "width": pixels.width, "height": pixels.height,
            "sha256": hashlib.sha256(destination.read_bytes()).hexdigest(),
        })

    with tempfile.TemporaryDirectory(prefix="b21-mission-map-") as directory:
        for filename, specs in SPRITES.items():
            movie = source_root / "interface" / filename
            folder = Path(directory) / movie.stem
            folder.mkdir()
            for name, symbol, frame, path in _export_sprites(movie, folder, specs):
                with Image.open(path) as pixels:
                    emit(movie, name, pixels, symbol, frame)
        for quest, name in EXPEDITIONS.items():
            with Image.open(textures / name) as pixels:
                emit(textures / name, f"expedition-{quest}", pixels)
        if catalog:
            for entry in catalog["locations"] + catalog["districts"]:
                filename = entry.pop("texture")
                if Path(filename).name != filename or Path(filename).suffix.lower() != ".dds":
                    raise ValueError(f"Invalid Expedition artwork path: {filename}")
                name = "district-" + entry["id"]
                with Image.open(textures / filename) as pixels:
                    emit(textures / filename, name, pixels)
                entry["image"] = "missions/" + name + ".dds"
            destination = output_data / OUTPUT / "catalog.json"
            destination.write_text(json.dumps(catalog, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
            with Path(source_plugin).open("rb") as plugin_file:
                source_hash = hashlib.file_digest(plugin_file, "sha256").hexdigest()
            manifest["files"].append({"path": destination.relative_to(output_data).as_posix(),
                                      "source": Path(source_plugin).name, "language": language,
                                      "source_sha256": source_hash,
                                      "sha256": hashlib.sha256(destination.read_bytes()).hexdigest()})
    (output_data / OUTPUT / "conversion.json").write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    return manifest
