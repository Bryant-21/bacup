from __future__ import annotations

import hashlib
import json
import math
from pathlib import Path

from bacup_lib.plugin_catalog import form_key, write_document
from bacup_lib.reputation_catalog import (
    NativeReader, NativeResolver, first, load_esp_native, plugin_masters, reference_id,
)

FEATURE = "Pets"
SECTION_KEYS = ("pets", "apparel")


def active(editor_id: str) -> bool:
    name = editor_id.casefold()
    return not name.startswith("zzz") and not any(part in name for part in ("_template", "_debug", "_test"))


def bind_homes(homes: list[dict], actors: list[dict], identity_keywords: set[int]) -> tuple[list[dict], list[dict]]:
    bindings, rejected = [], []
    actors = [actor for actor in actors if active(actor["editor_id"])]
    for home in homes:
        if "camppets_spawnfurniture_" not in home["editor_id"].casefold() or not active(home["editor_id"]):
            continue
        matches = [(actor, home["keywords"] & actor["keywords"] & identity_keywords) for actor in actors]
        matches = [(actor, keys) for actor, keys in matches if keys]
        if len(matches) != 1 or len(matches[0][1]) != 1:
            rejected.append({"home": home["editor_id"], "candidates": [actor["editor_id"] for actor, _ in matches]})
            continue
        actor, keys = matches[0]
        bindings.append({"home": home, "actor": actor, "keyword": next(iter(keys))})
    return bindings, rejected


def to_document(catalog: dict, source_plugin: str) -> dict:
    """Tales layout: pets (key actor) and apparel (key form) are sections, everything else settings.
    Unconverted pets stay out of the section (they are listed in `missing_converted`)."""
    pets = [{**row, "source_keyword": form_key(source_plugin, int(row["source_keyword"], 16))}
            for row in catalog["pets"] if row["available"]]
    settings = {key: value for key, value in catalog.items() if key not in SECTION_KEYS and key != "schema_version"}
    return {"version": catalog["schema_version"], "settings": settings, "pets": pets,
            "apparel": [row for row in catalog["apparel"]]}


def sample_curve(points: list[dict], maximum: int) -> list[float]:
    if not points or points[0]["x"] != 1 or points[0]["y"] != 0:
        raise ValueError("Pet XP curve must start at level one with zero XP")
    if any(not math.isfinite(float(p[key])) for p in points for key in ("x", "y")):
        raise ValueError("Pet XP curve contains non-finite values")
    if any(a["x"] >= b["x"] or a["y"] >= b["y"] for a, b in zip(points, points[1:])):
        raise ValueError("Pet XP curve must increase")
    if maximum < 1 or maximum > points[-1]["x"]:
        raise ValueError("Pet XP curve does not cover the level cap")
    values, segment = [], 0
    for level in range(1, maximum + 1):
        while segment + 1 < len(points) - 1 and points[segment + 1]["x"] < level:
            segment += 1
        a, b = points[segment], points[min(segment + 1, len(points) - 1)]
        values.append(float(a["y"] if a == b else a["y"] + (b["y"] - a["y"]) * (level - a["x"]) / (b["x"] - a["x"])))
    return values


def emit_pet_catalog(source_plugin: Path, converted_plugin: Path, output_mod: Path, *, source_root: Path | None = None) -> dict:
    native = load_esp_native()
    handles = []
    try:
        source = native.plugin_handle_load_index(str(source_plugin), game="fo76")
        handles.append(source)
        converted = native.plugin_handle_load_index(str(converted_plugin), game="fo4")
        handles.append(converted)
        reader = NativeReader(native, source)
        resolver = NativeResolver(native, converted_plugin.name, converted, plugin_masters(native, converted))

        def discover(signature: str, pattern: str) -> list[dict]:
            records = []
            for raw_id, _, editor_id, _ in native.plugin_handle_search_records(source, f"*{pattern}*", signatures=[signature]):
                _, _, fields = reader.fields(raw_id)
                keywords = {reference_id(value) for value in first(fields, "Keywords") or []}
                records.append({"id": raw_id, "editor_id": editor_id, "keywords": keywords - {None}})
            return records

        _, keyword_fields = reader.lookup("FLST", "CAMPPets_Keywords")
        keywords = {reference_id(value) for value in keyword_fields.get("FormID", [])} - {None}
        homes = discover("FURN", "CAMPPets_SpawnFurniture_")
        actors = discover("NPC_", "CAMPPets_Actor_")
        bindings, rejected = bind_homes(homes, actors, keywords)
        pets, missing = [], []
        for binding in bindings:
            home, actor = binding["home"], binding["actor"]
            home_form = resolver.resolve("FURN", home["editor_id"])
            actor_form = resolver.resolve("NPC_", actor["editor_id"])
            species = actor["editor_id"].casefold().split("camppets_actor_", 1)[1].split("_", 1)[0]
            if species not in ("dog", "cat", "radhog", "deathclaw"):
                rejected.append({"home": home["editor_id"], "reason": "unrecognized species"})
                continue
            if not home_form or not actor_form:
                missing.append(home["editor_id"])
            pets.append({"home": home_form, "actor": actor_form, "species": species,
                         "home_editor_id": home["editor_id"], "actor_editor_id": actor["editor_id"],
                         "source_keyword": f"{binding['keyword']:06X}",
                         "available": bool(home_form and actor_form)})
        def converted_source(object_id: int) -> str | None:
            try:
                signature, editor_id, _ = reader.fields(object_id)
            except ValueError:
                return None
            return resolver.resolve(signature, editor_id) if editor_id else None

        runtime_bindings = {key: converted_source(object_id) for key, object_id in {
            "sandbox": 0x77D823, "follow": 0x8ABEF7, "world_keyword": 0x8ABEF6,
            "owner_link": 0x8ABEF5, "player_link": 0x7875BA, "home_link": 0x7875BB,
            "marker_link": 0x7875BC, "workshop_link": 0x7875BD, "object_workshop_link": 0x7875B8,
            "player_pet": 0x786D52, "player_pet_first": 0x7A8365, "player_pet_pa": 0x7BD0EA,
        }.items()}
        runtime_bindings["stimpak"] = "Fallout4.esm:023736"
        interactions = {str(species): {action: converted_source(object_id) for action, object_id in zip(("pet", "speak", "sit"), ids)}
                        for species, ids in enumerate(((0x7A833A, 0x7A8339, 0x7A8338), (0x786D51, 0x786C5A, 0x786C5B),
                                                       (0x8B1CC3, 0x8B1CA1, 0x8B1CCA), (0x8AC4B2, 0x8AC4B4, 0x8AC4B3)))}
        apparel = []
        races = {0x77D825: 0, 0x77D824: 1, 0x84FB91: 2, 0x8A52BE: 3}
        for raw_id, _, editor_id, _ in native.plugin_handle_search_records(source, "*", signatures=["ARMO"]):
            if not active(editor_id):
                continue
            _, _, fields = reader.fields(raw_id)
            race = reference_id(first(fields, "Race"))
            form = resolver.resolve("ARMO", editor_id)
            if race in races and form:
                apparel.append({"form": form, "species": races[race], "slots": first(fields, "BipedBodyTemplate") or []})
        xp = [0.0]
        root = source_root or source_plugin.parent
        try:
            _, curve = reader.lookup("CURV", "CT_WorldPets_XP_LevelingProgression")
        except ValueError:
            curve = None
        progression = {"available": False, "reason": "Source pet progression data is absent"}
        if curve is not None:
            try:
                from bacup_lib.pet_progression_ui import convert_pet_progression_ui, parse_track

                curve_path = root / "misc/curvetables/json" / str(first(curve, "JASF")).replace("\\", "/")
                _, cap = reader.lookup("GLOB", "WorldPets_MaxLVL")
                xp = sample_curve(json.loads(curve_path.read_text(encoding="utf-8"))["curve"], int(first(cap, "Value")))
                tracks = []
                for species, race in enumerate(races):
                    _, _, fields = reader.fields(race)
                    track_ref = first(fields, "PGTF")
                    track_id = reference_id(track_ref)
                    if track_id is None and isinstance(track_ref, dict):
                        track_id = int.from_bytes(bytes.fromhex(track_ref["raw_hex"]), "little")
                    if not track_id:
                        raise ValueError(f"Pet race {race:06X} has no progression track")
                    track = json.loads(native.plugin_handle_inspect_record(source, track_id))["record"]
                    nodes = parse_track(track["fields"])
                    from bacup_lib.pet_rewards import node_delivery
                    for node in nodes:
                        node["delivery"] = node_delivery(node, reader, resolver)
                        _, _, icon = reader.fields(node.pop("icon_id"))
                        node["icon"] = first(icon, "IconName") or ""
                    tracks.append({"species": species, "nodes": nodes})
                progression = convert_pet_progression_ui(root, output_mod / "data")
                if progression["available"]:
                    progression["tracks"] = tracks
            except Exception as error:
                progression = {"available": False, "reason": f"Pet progression data unavailable: {error}"}
        with source_plugin.open("rb") as stream:
            source_hash = hashlib.file_digest(stream, "sha256").hexdigest()
        catalog = {"schema_version": 1, "source": {"plugin": source_plugin.name, "sha256": source_hash},
                   "pets": pets, "rejected": rejected, "missing_converted": missing,
                   "bindings": runtime_bindings, "interactions": interactions, "apparel": apparel,
                   "xp_thresholds": xp, "xp_per_second": 1.0, "progression": progression}
    finally:
        for handle in reversed(handles):
            native.plugin_handle_close(handle)
    write_document(FEATURE, converted_plugin.name, to_document(catalog, source_plugin.name), output_mod)
    return catalog
