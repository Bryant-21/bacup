from __future__ import annotations

import json
from pathlib import Path

from bacup_lib.plugin_catalog import form_key, write_document
from bacup_lib.reputation_catalog import first, reference_id
from bacup_lib.player_ghoul_native import build_native_rules

FEATURE = "PerkCardExtras"
SOURCE_PLUGIN = "SeventySix.esm"


def build_catalog(source, resolver, source_root, condition_data, number, base_resolver=None):
    def binding(raw):
        signature, eid, _ = source.fields(raw)
        value = resolver.resolve(signature, eid)
        if value is None and base_resolver is not None and signature in ("AVIF", "KYWD"):
            value = base_resolver.resolve(signature, eid)
        return value

    player = source.record(7)
    vmad = first({k: [v] for f in player["fields"] for k, v in f.items()}, "VirtualMachineAdapter")
    script = next(row for row in vmad["Scripts"] if row["ScriptName"] == "Perks_PlayerSpecificPerkFunctions")
    properties = {row["propertyName"]: row["Value"] for row in script["Properties"]}
    meat = []
    for row in properties["MeatDataTableArray"]:
        values = {v["memberName"]: v["Value"] for v in row}
        raw = reference_id(values["MeatType"]["FormID"])
        meat.append({"form": binding(raw), "maximum": values.get("MaxQuantity", 3), "source": raw})
    chances = []
    for row in properties["MysteryMeatPerksArray"]:
        values = {v["memberName"]: v["Value"] for v in row}
        chances.append(number(values["standardChance"]))
    survival = build_native_rules(source, resolver, [], condition_data, number, (0x026841, 0x054DF3))
    for raw, record in survival["records"].items():
        if record["signature"] in ("AVIF", "KYWD"):
            record["form"] = binding(int(raw))
    return {"version": 1, "mystery_meat": {"meat": meat, "chances": chances,
            "stimpak_keyword": binding(reference_id(properties["ObjectTypeStimpak"]["FormID"])),
            "cooldown": properties.get("cooldownSeconds", 60),
            "radiation_maximum": 1000, "radiation_multiplier": [.5, 1.5]},
            "survival": survival, "survival_spells": [0x026841, 0x054DF3]}


def to_document(catalog):
    """Tales layout: meat rewards and survival spells are keyed sections, the rest are settings."""
    mystery = dict(catalog["mystery_meat"])
    meat = [{"form": row["form"], "maximum": row["maximum"], "source": form_key(SOURCE_PLUGIN, row["source"])}
            for row in mystery.pop("meat") if row["form"]]
    spells = [{"form": form_key(SOURCE_PLUGIN, object_id)} for object_id in catalog["survival_spells"]]
    return {"version": catalog["version"], "settings": {"mystery_meat": mystery, "survival": catalog["survival"]},
            "meat": meat, "survival_spells": spells}


def emit_catalog(source, resolver, output_mod, source_root, condition_data, number, base_resolver=None):
    catalog = build_catalog(source, resolver, source_root, condition_data, number, base_resolver)
    document = to_document(catalog)
    json.dumps(document, allow_nan=False)
    write_document(FEATURE, resolver.plugin, document, output_mod)
    return catalog


if __name__ == "__main__":
    import argparse
    from bacup_lib.esp_native_runtime import load_esp_native
    from bacup_lib.player_ghoul import Source, condition_data, number
    from bacup_lib.reputation_catalog import NativeResolver, plugin_masters

    parser = argparse.ArgumentParser()
    parser.add_argument("--source", type=Path, required=True)
    parser.add_argument("--converted-plugin", type=Path, required=True)
    parser.add_argument("--source-root", type=Path, required=True)
    parser.add_argument("--output-mod", type=Path, required=True)
    parser.add_argument("--target-data-dir", type=Path)
    args = parser.parse_args()
    native = load_esp_native()
    handles = []
    try:
        source = native.plugin_handle_load_index(str(args.source), game="fo76")
        handles.append(source)
        target = native.plugin_handle_load_index(str(args.converted_plugin), game="fo4")
        handles.append(target)
        base_resolver = None
        if args.target_data_dir:
            path = args.target_data_dir / "Fallout4.esm"
            base = native.plugin_handle_load_index(str(path), game="fo4")
            handles.append(base)
            base_resolver = NativeResolver(native, path.name, base, plugin_masters(native, base))
        result = emit_catalog(Source(native, source), NativeResolver(native, args.converted_plugin.name, target, plugin_masters(native, target)),
                              args.output_mod, args.source_root, condition_data, number, base_resolver)
        print(f"Extra perk metadata: {len(result['mystery_meat']['meat'])} meat rewards, {len(result['survival']['records'])} survival records")
    finally:
        for handle in reversed(handles):
            native.plugin_handle_close(handle)
