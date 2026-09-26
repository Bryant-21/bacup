from __future__ import annotations

import json
import re
from pathlib import Path

from bacup_lib.esp_native_runtime import load_esp_native


OUTPUT = Path("F4SE/Plugins/B21_TalesFromAppalachia/LegendaryPerks.json")
RECORD = re.compile(r"B21_Legendary_[A-Za-z]+_[1-4](_Value)?\Z")


def binding_catalog(plugin: str, records: list) -> dict:
    forms = {}
    for raw_id, signature, editor_id, _ in records:
        match = RECORD.fullmatch(editor_id or "")
        if not match:
            continue
        expected = "GLOB" if match.group(1) else "PERK"
        if signature != expected:
            raise ValueError(f"{editor_id} must be {expected}, got {signature}")
        local = raw_id & 0xFFFFFF
        if not local or editor_id in forms:
            raise ValueError(f"Invalid or duplicate legendary binding: {editor_id}")
        forms[editor_id] = local
    return {"schema_version": 1, "plugin": plugin, "forms": dict(sorted(forms.items()))}


def emit_legendary_perk_bindings(plugin: Path, output_mod: Path) -> dict:
    native = load_esp_native()
    handle = native.plugin_handle_load_index(str(plugin), game="fo4")
    try:
        records = native.plugin_handle_search_records(
            handle, "B21_Legendary_*", signatures=["PERK", "GLOB"], case_sensitive=True,
        )
        catalog = binding_catalog(plugin.name, records)
    finally:
        native.plugin_handle_close(handle)
    destination = output_mod / OUTPUT
    destination.parent.mkdir(parents=True, exist_ok=True)
    destination.write_text(json.dumps(catalog, indent=2) + "\n", encoding="utf-8")
    return catalog


if __name__ == "__main__":
    import argparse

    parser = argparse.ArgumentParser()
    parser.add_argument("plugin", type=Path)
    parser.add_argument("output_mod", type=Path)
    args = parser.parse_args()
    result = emit_legendary_perk_bindings(args.plugin, args.output_mod)
    print(f"Wrote {len(result['forms'])} legendary bindings to {args.output_mod / OUTPUT}")
