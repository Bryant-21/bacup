"""FO76 food spoilage catalog for the Tales runtime (FoodSpoilage/<plugin>.json).

Evidence (SeventySix.esm, FO76 client):
- ALCH ENIT carries `Health` (a CURV whose JASF names a misc/curvetables JSON: condition
  points by level) and `Spoiled` (the item a stack turns into).
- Stash activator groups set cold-storage rates through default objects:
  WorkshopStashActivatorGroup{Refrigerator,Freezer,Fermenter}SpoilRate_DO -> GLOB 0.5 / 0.0 / 2.0.
  An ACTI joins a group through VendableItemDatas[].VendableItemDatasUnknown (the group GLOB);
  fridges also carry the keyword Workbench_Crafting_Refrigerator, cryo freezers
  Workbench_Crafting_Freezer.
- The decay runs on the server ("SpoilFoodItem must be executed on the server!") at GMST
  fFoodHealthDegradationRatePerMinute (1.0) condition points per minute; fast travel ages food by
  fFastTravelFoodItemDegradation (0.0). Tales applies the rate per minute of unpaused play.
"""
from __future__ import annotations

import json
from collections import Counter
from pathlib import Path

from bacup_lib.plugin_catalog import write_document
from bacup_lib.reputation_catalog import first, reference_id

FEATURE = "FoodSpoilage"
DECAY_POLICY = "fFoodHealthDegradationRatePerMinute condition points per minute of unpaused play; the curve x is the player's level."
RATE_GMST = "fFoodHealthDegradationRatePerMinute"

# Group GLOB EditorID -> (class, spoil-rate DFOB EditorID). The pairing is the engine's
# ("Spoil rate for the refrigerator shared group."), not a record link.
STORAGE_GROUPS = {
    "WorkshopStashActivatorGroup_Refrigerator": ("refrigerator", "WorkshopStashActivatorGroupRefrigeratorSpoilRate_DO"),
    "WorkshopStashActivatorGroup_Cryo_Freezer": ("freezer", "WorkshopStashActivatorGroupFreezerSpoilRate_DO"),
    "WorkshopStashActivatorGroup_Fermenter": ("fermenter", "WorkshopStashActivatorGroupFermenterSpoilRate_DO"),
}
STORAGE_KEYWORDS = {"refrigerator": "Workbench_Crafting_Refrigerator", "freezer": "Workbench_Crafting_Freezer"}

# Default profiles for food FO76 has no record for, keyed by FO76's own food keywords.
CATEGORY_KEYWORDS = {
    "raw_meat": ({"IngredientTypeMeat", "MealTypeRaw"}, set()),
    "cooked_meat": ({"IngredientTypeMeat", "MealTypeCooked"}, {"MealTypeTasty"}),
    "raw_vegetable": ({"IngredientTypeVegetable", "MealTypeRaw"}, set()),
    "cooked_vegetable": ({"IngredientTypeVegetable", "MealTypeCooked"}, {"MealTypeTasty", "IngredientTypeMeat"}),
}


def read_curve(source_root: Path, relative: str, number):
    root = (Path(source_root) / "misc/curvetables/json").resolve()
    path = (root / relative.replace("\\", "/")).resolve()
    if not path.is_relative_to(root):
        raise ValueError("Food curve escapes source root")
    points = json.loads(path.read_text(encoding="utf-8"))["curve"]
    previous = float("-inf")
    for point in points:
        x, y = number(point["x"]), number(point["y"])
        if x <= previous or y < 0:
            raise ValueError(f"Invalid food condition curve {relative}")
        previous = x
    return [{"x": point["x"], "y": point["y"]} for point in points]


def keyword_ids(fields) -> list[int]:
    keywords = first(fields, "Keywords") or []
    return [value for value in (reference_id(row) for row in keywords) if value]


def build_catalog(source, resolver, source_root, number, base_resolver=None):
    def binding(signature, raw):
        if not raw:
            return None
        found_signature, eid, _ = source.fields(raw)
        if found_signature != signature:
            return None
        value = resolver.resolve(signature, eid)
        if value is None and base_resolver is not None and signature == "KYWD":
            value = base_resolver.resolve(signature, eid)
        return value

    def editor_id(raw):
        return source.fields(raw)[1]

    curves: dict[int, tuple[str, list]] = {}
    foods, profiles = [], []
    for raw, _, eid, _ in source.rows("ALCH"):
        _, _, fields = source.fields(raw)
        data = first(fields, "EffectData") or {}
        health, spoiled = reference_id(data.get("Health")), reference_id(data.get("Spoiled"))
        if not health or not spoiled:
            continue
        if health not in curves:
            curve_eid, curve_fields = editor_id(health), source.fields(health)[2]
            relative = first(curve_fields, "CRVE") or first(curve_fields, "JASF")
            if not isinstance(relative, str):
                raise ValueError(f"Food condition curve missing for {eid}")
            curves[health] = (curve_eid, read_curve(source_root, relative, number))
        curve_eid, points = curves[health]
        keywords = {editor_id(value) for value in keyword_ids(fields)}
        profiles.append((keywords, health, spoiled))
        foods.append({"form": resolver.resolve("ALCH", eid), "editor_id": eid, "spoiled": binding("ALCH", spoiled),
                      "curve_id": curve_eid, "curve": points})

    defaults = {}
    for category, (required, excluded) in CATEGORY_KEYWORDS.items():
        votes = Counter((health, spoiled) for keywords, health, spoiled in profiles
                        if required <= keywords and not (excluded & keywords))
        if not votes:
            raise ValueError(f"No FO76 food matches the {category} default")
        (health, spoiled), count = votes.most_common(1)[0]
        defaults[category] = {"curve_id": curves[health][0], "curve": curves[health][1],
                              "spoiled": binding("ALCH", spoiled), "fo76_items": count, "fo76_matches": sum(votes.values())}

    storage = {}
    for group_eid, (name, rate_eid) in STORAGE_GROUPS.items():
        group = next((raw for raw, _, found, _ in source.rows("GLOB", group_eid) if found == group_eid), None)
        rate_object = next((raw for raw, _, found, _ in source.rows("DFOB", rate_eid) if found == rate_eid), None)
        if group is None or rate_object is None:
            raise ValueError(f"FO76 storage group {group_eid} or {rate_eid} missing")
        rate_global = reference_id(first(source.fields(rate_object)[2], "Object"))
        rate = number(first(source.fields(rate_global)[2], "Value"))
        storage[name] = {"rate": rate, "group": group_eid, "bases": [], "keyword": None}
        keyword_eid = STORAGE_KEYWORDS.get(name)
        if keyword_eid:
            keyword = next((raw for raw, _, found, _ in source.rows("KYWD", keyword_eid) if found == keyword_eid), None)
            storage[name]["keyword"] = binding("KYWD", keyword)
        storage[name]["_group_id"] = group & 0xFFFFFF
    by_group = {entry.pop("_group_id"): entry for entry in storage.values()}
    for raw, _, eid, _ in source.rows("ACTI"):
        for row in first(source.fields(raw)[2], "VendableItemDatas") or []:
            entry = by_group.get(int(row.get("VendableItemDatasUnknown") or 0) & 0xFFFFFF)
            if entry is not None:
                form = resolver.resolve("ACTI", eid)
                if form and form not in entry["bases"]:
                    entry["bases"].append(form)
    for entry in storage.values():
        entry["bases"].sort()

    rate_setting = next((raw for raw, _, found, _ in source.rows("GMST", RATE_GMST) if found == RATE_GMST), None)
    if rate_setting is None:
        raise ValueError(f"FO76 GMST {RATE_GMST} missing")
    value = first(source.fields(rate_setting)[2], "Value")
    points_per_minute = number(value.get("value") if isinstance(value, dict) else value)
    if points_per_minute < 0:
        raise ValueError(f"Invalid {RATE_GMST} {points_per_minute}")

    can_spoil = next((raw for raw, _, found, _ in source.rows("KYWD", "ObjectTypeCanSpoil") if found == "ObjectTypeCanSpoil"), None)
    return {"version": 1, "decay_policy": DECAY_POLICY, "points_per_minute": points_per_minute, "foods": foods, "defaults": defaults,
            "storage": storage, "can_spoil_keyword": binding("KYWD", can_spoil)}


def to_document(catalog):
    settings = {key: catalog[key] for key in ("decay_policy", "points_per_minute", "defaults", "storage", "can_spoil_keyword")}
    return {"version": catalog["version"], "settings": settings, "foods": [row for row in catalog["foods"] if row["form"]]}


def emit_catalog(source, resolver, output_mod, source_root, number, base_resolver=None):
    catalog = build_catalog(source, resolver, source_root, number, base_resolver)
    document = to_document(catalog)
    json.dumps(document, allow_nan=False)
    write_document(FEATURE, resolver.plugin, document, output_mod)
    return catalog
