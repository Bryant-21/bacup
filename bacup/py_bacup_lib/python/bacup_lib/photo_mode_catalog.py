from __future__ import annotations

import json
import struct
from pathlib import Path

from bacup_lib.emotes import (ClipConverter, HUMAN_GRAPH, POWER_ARMOR_GRAPH, consuming_route,
                              english, fields, power_armor_route, reference, sha, source_clip)
from bacup_lib.translations import merge_ui_translations

CATALOG = Path("F4SE/Plugins/B21_TalesFromAppalachia/photo-mode.json")
POSE_DIR = "meshes/actors/character/animations/common/photomode"
PA_DIR = "meshes/actors/powerarmor/animations/paired/photomode"


def overlay_geometry(data: dict) -> dict:
    kind, x, y, width, height, stage_width, stage_height, flags = struct.unpack(
        "<8I", bytes.fromhex(data["FNAM"]["raw_hex"]))
    if kind not in (0, 1) or not width or not height or not stage_width or not stage_height:
        raise ValueError(f"Unexpected PMFT geometry: {data['FNAM']}")
    return {"kind": "frames" if kind else "textures", "x": x, "y": y, "width": width,
            "height": height, "stage_width": stage_width, "stage_height": stage_height, "flags": flags}


def convert_catalog(source_root: Path, output_data: Path, source_plugin: Path, target_root: Path) -> dict:
    from creation_lib.esp import Plugin
    from creation_lib.havok.native_runtime import load_native_module

    havok = load_native_module()
    route = consuming_route(havok.unpack_hkx_to_xml(str(target_root / HUMAN_GRAPH)), "dyn_ActivationLoop")
    route["power_armor"] = power_armor_route(havok.unpack_hkx_to_xml(str(target_root / POWER_ARMOR_GRAPH)), route)
    clips = ClipConverter(havok, source_root, target_root)
    catalog = {"schema_version": 1, "source_plugin": "SeventySix.esm", "playback_route": route,
               "poses": [], "frames": [], "textures": [], "lights": [], "files": [], "skipped": []}
    translations = {}
    written = set()

    def write(path: str, original: bytes, converted: bytes | None = None):
        if path in written:
            return
        written.add(path)
        converted = original if converted is None else converted
        target = output_data / path
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(converted)
        catalog["files"].append({"path": path, "sha256": sha(converted), "source_sha256": sha(original)})

    def label(record: dict, value: dict, suffix: str = "") -> str:
        key = "$B21_PM_" + record["form_id"] + suffix
        translations[key] = english(value)
        return key

    with Plugin.load(source_plugin, game="fo76", lazy_index=True) as plugin:
        sounds = {row[1]: row for row in plugin.record_index_rows(signatures=["SNDR"])}
        def get(local: str) -> dict:
            return plugin.read_authoring_record(int(local, 16))

        def category(local: str) -> str:
            record = get(local)
            return label(record, fields(record)["Name"])

        def sound_files(record: dict, seen: set[str]):
            if record["form_id"] in seen:
                return
            seen.add(record["form_id"])
            for entry in record["fields"]:
                if "Sound" in entry and isinstance(entry["Sound"], str):
                    path = entry["Sound"].replace("\\", "/").lower().removeprefix("data/")
                    if not (source_root / path).is_file() and (source_root / Path(path).with_suffix(".xwm")).is_file():
                        path = Path(path).with_suffix(".xwm").as_posix()
                    write(path, (source_root / path).read_bytes())
                if "Descriptor" in entry:
                    sound_files(get(reference(entry["Descriptor"])), seen)

        for index in plugin.record_index_rows(signatures=["IDLE"]):
            if "photomode" not in index[1].lower() or index[1].startswith(("DEBUG_", "TEMPLATE_")):
                continue
            record = plugin.read_authoring_record(index[4])
            data = fields(record)
            if "Name" not in data or "AnimationFile" not in data:
                continue
            stem = Path(data["AnimationFile"].replace("\\", "/")).stem.lower()
            row = {"id": record["form_id"], "name": label(record, data["Name"]),
                   "category": category(reference(data["PhotomodeCategoryKeyword"]) if "PhotomodeCategoryKeyword" in data else "5215E7"),
                   "animation": "", "animation_female": "", "animation_power_armor": ""}
            human_dirs = (POSE_DIR, "meshes/actors/character/animations/dynamicanims")
            if stem == "wpnidleready":
                human_dirs = ("meshes/actors/character/animations/player",)
            if data["AnimationFile"].replace("\\", "/").lower().endswith("h2h/idle.hkx"):
                human_dirs = ("meshes/actors/character/animations/h2h",)
            for key, directories, armor in (("animation", human_dirs, False),
                                           ("animation_female", (POSE_DIR + "/female",), False),
                                           ("animation_power_armor", (PA_DIR,), True)):
                source = source_clip(source_root, directories, stem)
                if not source:
                    continue
                original, converted, metadata = clips.convert(source, armor)
                relative = f"Actors/{'PowerArmor' if armor else 'Character'}/Animations/B21/PhotoMode/{'Female/' if key == 'animation_female' else ''}{stem}.hkx"
                write("Meshes/" + relative, original, converted)
                row[key] = relative.replace("/", "\\")
                row[key + "_duration"] = metadata["duration"]
                row[key + "_sounds"] = []
                for time, name in metadata["sound_cues"]:
                    if name not in sounds:
                        catalog["skipped"].append({"id": row["id"], "reason": "animation sound descriptor missing", "sound": name})
                        continue
                    sound = plugin.read_authoring_record(sounds[name][4])
                    sound_files(sound, set())
                    row[key + "_sounds"].append({"time": time, "id": sound["form_id"]})
            if any(row[key] for key in ("animation", "animation_female", "animation_power_armor")):
                duplicate = next((p for p in catalog["poses"] if p["category"] == row["category"] and
                                  (p["animation"] == row["animation"] or stem == "idle" and
                                   p["animation"].lower().endswith("wpnidleready.hkx"))), None)
                if duplicate is None:
                    catalog["poses"].append(row)
                elif stem == "idle":
                    for suffix in ("", "_duration", "_sounds"):
                        duplicate["animation_unarmed" + suffix] = row["animation" + suffix]
            else:
                catalog["skipped"].append({"id": row["id"], "reason": "source pose clip missing", "clip": stem})

        for index in plugin.record_index_rows(signatures=["PMFT"]):
            record = plugin.read_authoring_record(index[4])
            if record["eid"].startswith(("DEBUG_", "TEMPLATE_")):
                continue
            data = fields(record)
            if not all(key in data for key in ("Name", "IconFileName")):
                catalog["skipped"].append({"id": record["form_id"], "editor_id": record["eid"],
                                           "reason": "unused source overlay lacks image, name or category"})
                continue
            if struct.unpack_from("<I", bytes.fromhex(data["FNAM"]["raw_hex"]))[0] == 2:
                catalog["skipped"].append({"id": record["form_id"], "reason": "name and level card is not an image overlay"})
                continue
            geometry = overlay_geometry(data)
            path = data["IconFileName"].replace("\\", "/").lower().removeprefix("data/")
            source = source_root / path
            if not source.is_file():
                catalog["skipped"].append({"id": record["form_id"], "reason": "source overlay missing", "path": path})
                continue
            category_id = reference(data["Keyword"]) if "Keyword" in data else "3E198F"
            row = {"id": record["form_id"], "name": label(record, data["Name"]),
                   "category": category(category_id), "path": path, **geometry}
            image = source.read_bytes()
            if not image.startswith(b"DDS "):
                raise ValueError(f"Invalid photo-mode overlay: {path}")
            write(path, image)
            catalog[geometry["kind"]].append(row)

        recipes = get("87D1B5")
        for entry in recipes["fields"]:
            if "FormID" not in entry:
                continue
            recipe = get(reference(entry["FormID"]))
            data = fields(recipe)
            template = get(reference(data["CreatedObject"]))
            style = {"name": label(recipe, data["BuildGroupName"]), "lights": []}
            for item in template["fields"]:
                if "Node" not in item:
                    continue
                _, node_id, x, y, z, pitch, roll, yaw, _ = struct.unpack(
                    "<II6fI", bytes.fromhex(item["Node"]["raw_hex"]))
                node = get(f"{node_id:06X}")
                light = reference(fields(node)["IlluminationLight"])
                style["lights"].append({"id": light, "position": [x, y, z], "rotation": [pitch, roll, yaw]})
            catalog["lights"].append(style)

    catalog["poses"].sort(key=lambda row: (row["category"] != "$B21_PM_5215E7", row["id"]))
    destination = output_data.parent / CATALOG
    destination.parent.mkdir(parents=True, exist_ok=True)
    destination.write_text(json.dumps(catalog, indent=2) + "\n", encoding="utf-8")
    merge_ui_translations(output_data, [key + "\t" + value for key, value in translations.items()])
    return catalog
