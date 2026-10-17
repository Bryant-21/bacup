from __future__ import annotations

import json
from pathlib import Path

from bacup_lib.fullscreen_map import PLUGIN_ASSETS_REL

_EXCLUDED = ("border", "weather", "nudge", "workshop", "subregion", "test", "distant")


def _reference(value: dict) -> str:
    ref = value.get("reference", {})
    return f"{ref.get('plugin', '').lower()}:{str(ref.get('object_id', '')).lower()}"


def _name(value: object, language: str) -> str:
    if isinstance(value, str):
        return value
    if not isinstance(value, dict):
        return ""
    if isinstance(value.get("Value"), str):
        return value["Value"]
    for candidate in (language, "English"):
        for entry in value.get("Values", []):
            if entry.get("Language") == candidate:
                return entry.get("String", "")
    return ""


def build_region_catalog(regions: list[dict], locations: dict[str, dict], region_keyword: str,
                         world_key: str, worldspace: str, language: str = "English") -> dict:
    grouped: dict[str, dict] = {}
    for record in regions:
        editor_id = record.get("eid", "").lower()
        if any(part in editor_id for part in _EXCLUDED) or int(record.get("flags", "0"), 16) & 0x20:
            continue
        fields = {key: value for field in record.get("fields", []) for key, value in field.items()}
        if _reference(fields.get("WorldspaceReference", {})) != world_key:
            continue
        location_key = _reference(fields.get("Location", {}))
        location = locations.get(location_key, {})
        if not any(_reference(kw) == region_keyword for kw in location.get("Keywords", [])):
            continue
        name = _name(location.get("Name"), language).strip()
        if not name:
            continue
        polygons = [[[p["PointsX"], p["PointsY"]] for p in field["Points"]]
                    for field in record["fields"] if "Points" in field and len(field["Points"]) >= 3]
        if polygons:
            grouped.setdefault(location_key, {"name": name, "polygons": []})["polygons"].extend(polygons)
    return {"worldspace": worldspace, "regions": list(grouped.values())}


def export_region_labels(*, source_plugin: Path, source_root: Path, mod_root: Path,
                         language: str = "English") -> int:
    from creation_lib.esp import Plugin

    with Plugin.load(source_plugin, game="fo76", language=language, lazy_index=True,
                     strings_dirs=[source_root / "strings", source_plugin.parent / "strings"]) as plugin:
        rows = plugin.record_index_rows(signatures=["KYWD", "WRLD", "REGN", "LCTN"])
        keyword = next(row for row in rows if row[2] == "KYWD" and row[1] == "LocTypeRegion")
        world = next(row for row in rows if row[2] == "WRLD" and row[1].lower() == "appalachia")

        def key(row):
            return f"{source_plugin.name.lower()}:{row[3]:06x}"

        regions = [plugin.read_authoring_record(row[4]) or {} for row in rows if row[2] == "REGN"
                   and not any(part in row[1].lower() for part in _EXCLUDED)]
        referenced = {_reference(field["Location"]) for record in regions
                      for field in record.get("fields", []) if "Location" in field}
        locations = {}
        for row in rows:
            if row[2] == "LCTN" and key(row) in referenced:
                record = plugin.read_authoring_record(row[4]) or {}
                locations[key(row)] = {k: v for field in record.get("fields", []) for k, v in field.items()}
        catalog = build_region_catalog(regions, locations, key(keyword), key(world), world[1], language)
    path = mod_root / PLUGIN_ASSETS_REL / "maps/appalachia/regions.json"
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(catalog, ensure_ascii=False, separators=(",", ":")) + "\n", encoding="utf-8")
    return len(catalog["regions"])
