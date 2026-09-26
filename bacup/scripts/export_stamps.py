from __future__ import annotations

import argparse
import json
from pathlib import Path

from export_gold_bullion import query


def stock_children(row: dict) -> list[int]:
    entries = row.get("fields.LVLO") or []
    if isinstance(entries, dict):
        entries = [entries]
    if any(entry.get("variant") != "reference" for entry in entries):
        raise ValueError("Unsupported currency stock entry")
    return [entry["value"] for entry in entries]


def build_catalog(items: list[dict], targets: list[dict], plugin: str) -> dict:
    target_map = {int(row["form_id"], 16): row for row in targets}
    prices = []
    skipped = 0
    for item in items:
        item_id = int(item["form_id"], 16)
        value_field = {"ALCH": "fields.EffectData", "ARMO": "fields.ArmorData"}.get(item["signature"], "fields.Data")
        price = (item.get(value_field) or {}).get("Value")
        if item["signature"] not in {"BOOK", "ALCH", "MISC", "ARMO"} or type(price) is not int or not 0 < price < 2**31:
            raise ValueError(f"Invalid currency merchandise: {item}")
        target = target_map.get(item_id)
        if target is None:
            skipped += 1
            continue
        if target["signature"] != item["signature"] or target["eid"] != item["eid"]:
            raise ValueError(f"Converted identity mismatch for {item['eid']}; use mapped regeneration")
        prices.append({"plugin": plugin, "object_id": item_id, "price": price})
    return {"schema_version": 1, "prices": sorted(prices, key=lambda row: row["object_id"]), "skipped_unmapped": skipped}


def read_stock(source: Path, roots: list[int]) -> dict[int, list[dict]]:
    pending, visited, records = set(roots), set(), {}
    while pending:
        requested = pending - visited
        if not requested:
            break
        filters = [value for key in sorted(requested) for value in ("--record", f"{key:06X}")]
        rows = query(source, "fo76", "form_id,eid,signature,fields.Data,fields.EffectData,fields.ArmorData,fields.LVLO", *filters)
        if {int(row["form_id"], 16) for row in rows} != requested:
            raise ValueError("Incomplete currency stock graph")
        visited.update(requested)
        pending = set()
        for row in rows:
            records[int(row["form_id"], 16)] = row
            if row["signature"] == "LVLI":
                pending.update(stock_children(row))
    catalogs = {}
    for root in roots:
        if records[root]["signature"] != "LVLI":
            raise ValueError(f"Expected stock list at {root:06X}")
        pending, visited, items = {root}, set(), []
        while pending:
            key = pending.pop()
            if key in visited:
                continue
            visited.add(key)
            row = records[key]
            if row["signature"] == "LVLI":
                pending.update(stock_children(row))
            else:
                items.append(row)
        catalogs[root] = items
    return catalogs


def write_catalog(output: Path, catalog: dict) -> None:
    output.parent.mkdir(parents=True, exist_ok=True)
    temporary = output.with_suffix(".pending")
    temporary.write_text(json.dumps(catalog, separators=(",", ":")) + "\n", encoding="utf-8")
    temporary.replace(output)
    print(json.dumps({"path": str(output), "prices": len(catalog["prices"]), "skipped": catalog["skipped_unmapped"]}))


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--source", type=Path, required=True)
    parser.add_argument("--target", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    items = read_stock(args.source, [0x63966F])[0x63966F]
    targets = query(args.target, "fo4", "form_id,eid,signature", "--type", "BOOK", "--type", "ALCH", "--type", "MISC")
    catalog = build_catalog(items, targets, args.target.name)
    write_catalog(args.output, catalog)


if __name__ == "__main__":
    main()
