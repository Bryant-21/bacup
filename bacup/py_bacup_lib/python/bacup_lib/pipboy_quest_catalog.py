from __future__ import annotations

import hashlib
import json
from pathlib import Path

from bacup_lib.esp_native_runtime import load_esp_native
from bacup_lib.reputation_catalog import NativeReader, first, plugin_masters
from bacup_lib.translations import merge_packaged, translation_path

SCHEMA_VERSION = 1
OUTPUT = Path("F4SE/Plugins/B21_TalesFromAppalachia/quest_categories.json")
CATEGORIES = ("MAIN", "SIDE", "MISC", "LEADS", "EVENTS")
SOURCE_TYPES = ("None", "Primary", "Secondary", "SideQuest", "Server", "Daily",
                "PublicEvent", "Miscellaneous", "Event", "DailyOps", "Expedition",
                "Module", "Caravan", "Raid")
# These source radio quests have a "Breadcrumb quest ended" stage and a VMAD
# fragment property binding the quest they introduce. Keep both sides explicit.
LEAD_BINDINGS = {"5E9591": "5EAD3C", "699466": "72A2A8", "78DE65": "78DE64"}


def quest_type(value) -> int:
    if isinstance(value, str) and value in SOURCE_TYPES:
        return SOURCE_TYPES.index(value)
    if isinstance(value, dict):
        value = value.get("value")
    if isinstance(value, int) and not isinstance(value, bool) and 0 <= value <= 255:
        return value
    raise ValueError(f"Unsupported source quest type: {value!r}")


def has_title(value) -> bool:
    if value is None:
        return False
    if isinstance(value, str):
        return bool(value.strip())
    if isinstance(value, dict):
        return any(row.get("String", "").strip() for row in value.get("Values", []))
    raise ValueError("Invalid source QUST FULL")


def classify(source_type: int, lead: bool = False) -> str:
    if lead:
        return "LEADS"
    if source_type in (6, 8, 12):
        return "EVENTS"
    if source_type in (1, 13):
        return "MAIN"
    if source_type in (2, 3, 5, 9):
        return "SIDE"
    return "MISC"


def is_source_lead(record: dict) -> bool:
    target = LEAD_BINDINGS.get(record["form_id"])
    if target is None:
        return False
    adapter = first(record["fields"], "VirtualMachineAdapter") or {}
    fragments = adapter.get("Script Fragments", {})
    properties = fragments.get("Script", {}).get("Properties", [])
    return any(
        (row.get("Value") or {}).get("FormID", {}).get("reference", {}).get("object_id") == target
        for row in properties if row.get("Type") == "Object"
    )


def build_catalog(records: list[dict], converted: list[tuple[str, str]],
                  source_plugin: str, output_plugin: str, presentation: dict) -> dict:
    by_name: dict[str, list[str]] = {}
    for key, editor_id in converted:
        by_name.setdefault(editor_id, []).append(key)
    quests, missing, used = [], [], set()
    for record in records:
        editor_id = record["eid"]
        fields = record["fields"]
        general = first(fields, "DATA")
        if general is not None and not isinstance(general, dict):
            raise ValueError(f"Invalid source QUST DATA: {record['form_id']}")
        source_type = quest_type((general or {}).get("QuestType", "None"))
        candidates = by_name.get(editor_id, []) or by_name.get(editor_id + "fo76", [])
        if not editor_id or len(candidates) != 1:
            missing.append(record["form_id"])
            continue
        key = candidates[0]
        if key in used:
            raise ValueError(f"Duplicate converted quest binding: {key}")
        used.add(key)
        titled = has_title(first(fields, "Name"))
        lead = is_source_lead(record)
        if record["form_id"] in LEAD_BINDINGS and not lead:
            raise ValueError(f"Source lead binding changed: {record['form_id']}")
        quests.append({"form": key, "source": f"{source_plugin}:{record['form_id']}",
                       "source_type": source_type, "has_title": titled,
                       "category": classify(source_type, lead)})
    if not quests:
        raise ValueError("No converted quests available for the Pip-Boy catalog")
    return {"schema_version": SCHEMA_VERSION, "output_plugin": output_plugin,
            "categories": list(CATEGORIES), "presentation": presentation,
            "quests": sorted(quests, key=lambda row: row["form"]),
            "missing_converted": sorted(missing)}


def inspect_presentation(source_root: Path) -> dict:
    from creation_lib.swf.native_runtime import abc_class_names, abc_disassemble

    movie = Path(source_root) / "interface/newpipboy_datapage.swf"
    source = movie.read_bytes()
    required = {"NewPipBoy_DataPage", "NewPipBoyShared", "QuestsList", "ObjectivesList"}
    if not required.issubset(abc_class_names(source)):
        raise ValueError("FO76 quest presentation classes are unavailable")
    methods = abc_disassemble(source, "NewPipBoyShared", "static initializer")
    code = "\n".join(line for method in methods for line in method["code"])
    keys = ("$QUESTS_PRIMARY", "$QUESTS_SECONDARY", "$QUESTS_DAILY", "$QUESTS_LEADS", "$QUESTS_MISC")
    if any(f'PushString "{key}"' not in code for key in keys):
        raise ValueError("Unsupported FO76 quest tab contract")
    return {"available": True, "source_movie": movie.name,
            "sha256": hashlib.sha256(source).hexdigest(), "source_labels": list(keys),
            "component_owner": "vanilla_fo4", "converted_movies": []}


def emit_quest_catalog(source_plugin: Path, converted_plugin: Path,
                       source_root: Path, output_mod: Path) -> dict:
    source_plugin, converted_plugin, output_mod = map(Path, (source_plugin, converted_plugin, output_mod))
    destination = output_mod / OUTPUT
    # Never let a previous successful catalog enable a partial/new failed conversion.
    destination.unlink(missing_ok=True)
    presentation = inspect_presentation(source_root)
    native = load_esp_native()
    handles = []
    try:
        source = native.plugin_handle_load_index(str(source_plugin), game="fo76")
        handles.append(source)
        target = native.plugin_handle_load_index(str(converted_plugin), game="fo4")
        handles.append(target)
        reader = NativeReader(native, source)
        records = []
        for raw_id, _, editor_id, _ in native.plugin_handle_search_records(source, "*", signatures=["QUST"]):
            local_id = raw_id & 0xFFFFFF
            records.append({"form_id": f"{local_id:06X}", "eid": editor_id or "",
                            "fields": reader.fields(local_id)[2]})
        owners = plugin_masters(native, target) + [converted_plugin.name]
        converted = []
        for raw_id, _, editor_id, _ in native.plugin_handle_search_records(target, "*", signatures=["QUST"]):
            converted.append((f"{owners[raw_id >> 24]}:{raw_id & 0xFFFFFF:06X}", editor_id or ""))
        result = build_catalog(records, converted, source_plugin.name, converted_plugin.name, presentation)
        with source_plugin.open("rb") as stream:
            result["source_sha256"] = hashlib.file_digest(stream, "sha256").hexdigest()
    finally:
        for handle in reversed(handles):
            native.plugin_handle_close(handle)
    merge_packaged(translation_path(output_mod / "data"))
    destination.parent.mkdir(parents=True, exist_ok=True)
    destination.write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
    return result


if __name__ == "__main__":
    import argparse
    parser = argparse.ArgumentParser()
    parser.add_argument("--source", type=Path, required=True)
    parser.add_argument("--converted-plugin", type=Path, required=True)
    parser.add_argument("--source-root", type=Path, required=True)
    parser.add_argument("--output-mod", type=Path, required=True)
    args = parser.parse_args()
    catalog = emit_quest_catalog(args.source, args.converted_plugin, args.source_root, args.output_mod)
    print(f"Wrote {len(catalog['quests'])} quest categories; {len(catalog['missing_converted'])} unmapped")
