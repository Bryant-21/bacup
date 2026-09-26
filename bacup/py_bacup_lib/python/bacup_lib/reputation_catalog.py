"""Build the source-derived FO76 faction reputation catalog used by Tales."""
from __future__ import annotations

import hashlib
import json
import math
from pathlib import Path
from typing import Protocol

from bacup_lib.esp_native_runtime import load_esp_native

SCHEMA_VERSION = 1
OUTPUT = Path("F4SE/Plugins/B21_TalesFromAppalachia/reputation.json")

FACTIONS = (
    {
        "code": "Crater",
        "name": "$Crater",
        "actor_value": "Reputation_AV_Crater",
        "actor_value_default": "ReputationAVCrater_DO",
        "faction_default": "ReputationFactionCrater_DO",
        "tier_list": "ReputationTiersCrater",
    },
    {
        "code": "Foundation",
        "name": "$Foundation",
        "actor_value": "Reputation_AV_Foundation",
        "actor_value_default": "ReputationAVFoundation_DO",
        "faction_default": "ReputationFactionFoundation_DO",
        "tier_list": "ReputationTiersFoundation",
    },
)
SOUNDS = {
    "increase": "UIReputationIncrease",
    "decrease": "UIReputationDecrease",
    "level_up": "UIReputationLevelUp",
}


class Reader(Protocol):
    def lookup(self, signature: str, editor_id: str) -> tuple[int, dict[str, list]]: ...
    def fields(self, object_id: int) -> tuple[str, str, dict[str, list]]: ...


class Resolver(Protocol):
    def resolve(self, signature: str, editor_id: str) -> str | None: ...


def fields_by_name(record: dict) -> dict[str, list]:
    fields: dict[str, list] = {}
    for item in record["fields"]:
        for key, value in item.items():
            fields.setdefault(key, []).append(value)
    return fields


def first(fields: dict[str, list], key: str):
    values = fields.get(key)
    return values[0] if values else None


def reference_id(value) -> int | None:
    if isinstance(value, dict) and isinstance(value.get("reference"), dict):
        return int(value["reference"]["object_id"], 16)
    return None


def source_form(source_name: str, object_id: int) -> str:
    return f"{source_name}:{object_id:06X}"


def required_lookup(reader: Reader, signature: str, editor_id: str) -> tuple[int, dict[str, list]]:
    try:
        return reader.lookup(signature, editor_id)
    except (KeyError, ValueError):
        raise ValueError(f"Missing source {signature} {editor_id}") from None


def default_object(reader: Reader, editor_id: str, expected_signature: str,
                   expected_editor_id: str) -> tuple[int, str]:
    object_id, fields = required_lookup(reader, "DFOB", editor_id)
    target_id = reference_id(first(fields, "Object"))
    if target_id is None:
        raise ValueError(f"Source DFOB {editor_id} has no object")
    signature, target_editor_id, _ = reader.fields(target_id)
    if signature != expected_signature or target_editor_id != expected_editor_id:
        raise ValueError(
            f"Source DFOB {editor_id} points to {signature} {target_editor_id}, "
            f"expected {expected_signature} {expected_editor_id}"
        )
    return object_id, target_editor_id


def build_catalog(reader: Reader, resolver: Resolver, output_plugin: str,
                  source_name: str, source_sha256: str) -> dict:
    factions = []
    missing_converted = []

    def resolve(signature: str, editor_id: str) -> str | None:
        result = resolver.resolve(signature, editor_id)
        if result is None:
            missing_converted.append(f"{signature}:{editor_id}")
        return result

    for spec in FACTIONS:
        actor_id, actor_fields = required_lookup(reader, "AVIF", spec["actor_value"])
        actor_default_id, _ = default_object(
            reader, spec["actor_value_default"], "AVIF", spec["actor_value"]
        )
        faction_default_id, faction_editor_id = default_object(
            reader, spec["faction_default"], "FACT",
            "W05_CraterRaiderFaction" if spec["code"] == "Crater" else "W05_SettlerFaction",
        )
        tier_list_id, tier_fields = required_lookup(reader, "FLST", spec["tier_list"])
        tier_ids = [reference_id(value) for value in tier_fields.get("FormID", [])]
        if len(tier_ids) != 7 or any(value is None for value in tier_ids):
            raise ValueError(f"Source FLST {spec['tier_list']} must contain seven tiers")
        tiers = []
        previous = -math.inf
        for index, tier_id in enumerate(tier_ids):
            signature, editor_id, fields = reader.fields(tier_id)
            value = first(fields, "Value")
            if signature != "GLOB" or not editor_id or not isinstance(value, (int, float)):
                raise ValueError(f"Invalid tier {index} in {spec['tier_list']}")
            value = float(value)
            if not math.isfinite(value) or value <= previous:
                raise ValueError(f"Non-increasing tier {index} in {spec['tier_list']}")
            previous = value
            tiers.append({
                "index": index,
                "label": f"$ReputationStatus{index}",
                "threshold": value,
                "global": resolve("GLOB", editor_id),
                "source_global": source_form(source_name, tier_id),
                "source_global_editor_id": editor_id,
            })
        minimum = first(actor_fields, "MinimumValue")
        maximum = first(actor_fields, "MaximumValue")
        default = first(actor_fields, "DefaultValue")
        if not all(isinstance(value, (int, float)) and math.isfinite(float(value))
                   for value in (minimum, maximum, default)):
            raise ValueError(f"Source AVIF {spec['actor_value']} has invalid bounds")
        actor_value = resolve("AVIF", spec["actor_value"])
        factions.append({
            "available": actor_value is not None,
            "code": spec["code"],
            "name": spec["name"],
            "actor_value": actor_value,
            "source_actor_value": source_form(source_name, actor_id),
            "source_actor_value_default": source_form(source_name, actor_default_id),
            "faction": resolve("FACT", faction_editor_id),
            "source_faction_default": source_form(source_name, faction_default_id),
            "tier_list": source_form(source_name, tier_list_id),
            "minimum": float(minimum),
            "maximum": float(maximum),
            "default": float(default),
            "tiers": tiers,
        })
    sounds = {}
    for key, editor_id in SOUNDS.items():
        object_id, _ = required_lookup(reader, "SNDR", editor_id)
        sounds[key] = {
            "form": resolve("SNDR", editor_id),
            "source": source_form(source_name, object_id),
            "editor_id": editor_id,
        }
    return {
        "schema_version": SCHEMA_VERSION,
        "source": {"plugin": source_name, "sha256": source_sha256},
        "output_plugin": output_plugin,
        "source_movie_contract": {
            "movie": "hudreputationmeter.swf",
            "root_class": "HUDReputationUpdatesWidget",
            "factions": [spec["code"] for spec in FACTIONS],
        },
        "factions": factions,
        "sounds": sounds,
        "missing_converted": missing_converted,
    }


class NativeReader:
    def __init__(self, native, handle: int):
        self.native = native
        self.handle = handle
        self.cache: dict[int, tuple[str, str, dict[str, list]]] = {}

    def lookup(self, signature: str, editor_id: str) -> tuple[int, dict[str, list]]:
        rows = self.native.plugin_handle_search_records(
            self.handle, editor_id, signatures=[signature], case_sensitive=True
        )
        exact = [raw_id for raw_id, _, found, _ in rows if found == editor_id]
        if len(exact) != 1:
            raise ValueError(f"Expected one source {signature} {editor_id}, found {len(exact)}")
        raw_id = exact[0]
        return raw_id & 0xFFFFFF, self.fields(raw_id)[2]

    def fields(self, object_id: int) -> tuple[str, str, dict[str, list]]:
        if object_id not in self.cache:
            payload = self.native.plugin_handle_inspect_record(self.handle, object_id)
            if payload is None:
                raise ValueError(f"Unable to inspect record {object_id:08X}")
            data = json.loads(payload)
            self.cache[object_id] = (
                data["signature"], data["record"].get("eid") or "",
                fields_by_name(data["record"]),
            )
        return self.cache[object_id]


class NativeResolver:
    def __init__(self, native, plugin: str, handle: int, masters: list[str]):
        self.native, self.plugin, self.handle, self.masters = native, plugin, handle, masters

    def resolve(self, signature: str, editor_id: str) -> str | None:
        rows = self.native.plugin_handle_search_records(
            self.handle, editor_id, signatures=[signature], case_sensitive=True
        )
        exact = [(raw_id, found) for raw_id, _, found, _ in rows if found == editor_id]
        if len(exact) != 1:
            return None
        raw_id = exact[0][0]
        index = raw_id >> 24
        if index > len(self.masters):
            raise ValueError(f"Invalid master index {index} for {self.plugin}:{raw_id:08X}")
        owner = self.masters[index] if index < len(self.masters) else self.plugin
        return f"{owner}:{raw_id & 0xFFFFFF:06X}"


def plugin_masters(native, handle: int) -> list[str]:
    return list(native.plugin_handle_get_meta(handle)[4][5])


def emit_reputation_catalog(source_plugin: Path, converted_plugin: Path, output_mod: Path,
                            *, source_sha256: str | None = None) -> dict:
    source_plugin, converted_plugin, output_mod = map(Path, (source_plugin, converted_plugin, output_mod))
    for path in (source_plugin, converted_plugin):
        if not path.is_file():
            raise FileNotFoundError(f"Reputation catalog input is missing: {path}")
    native = load_esp_native()
    handles: list[int] = []
    try:
        source = native.plugin_handle_load_index(str(source_plugin), game="fo76")
        handles.append(source)
        converted = native.plugin_handle_load_index(str(converted_plugin), game="fo4")
        handles.append(converted)
        output_plugin = converted_plugin.name
        resolver = NativeResolver(native, output_plugin, converted, plugin_masters(native, converted))
        if source_sha256 is None:
            with source_plugin.open("rb") as stream:
                source_sha256 = hashlib.file_digest(stream, "sha256").hexdigest()
        catalog = build_catalog(
            NativeReader(native, source), resolver, output_plugin, source_plugin.name, source_sha256
        )
    finally:
        for handle in reversed(handles):
            native.plugin_handle_close(handle)
    destination = output_mod / OUTPUT
    destination.parent.mkdir(parents=True, exist_ok=True)
    destination.write_text(
        json.dumps(catalog, indent=2, sort_keys=True, allow_nan=False) + "\n",
        encoding="utf-8", newline="\n",
    )
    return catalog


if __name__ == "__main__":
    import argparse

    parser = argparse.ArgumentParser(description="Emit the Tales faction reputation catalog.")
    parser.add_argument("--source", type=Path, required=True)
    parser.add_argument("--converted-plugin", type=Path, required=True)
    parser.add_argument("--output-mod", type=Path, required=True)
    args = parser.parse_args()
    result = emit_reputation_catalog(args.source, args.converted_plugin, args.output_mod)
    print(f"Wrote {len(result['factions'])} factions to {args.output_mod / OUTPUT}")
