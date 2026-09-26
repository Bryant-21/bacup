from __future__ import annotations

import argparse
import json
import struct
import subprocess
from pathlib import Path


def query(plugin: Path, game: str, fields: str, *filters: str) -> list[dict]:
    command = ["modkit.exe", "--game", game, "--format", "compact", "--fields", fields]
    if game == "fo76" and "GoldBullionValue" in fields:
        command += ["--where", "fields.GoldBullionValue!=null"]
    command += ["esp", "query", str(plugin), *filters]
    result = subprocess.run(command, capture_output=True, text=True, encoding="utf-8", check=True)
    report = json.loads(result.stdout)
    if report["meta"]["truncated"]:
        raise ValueError("Incomplete bullion record report")
    return report["data"]


def build_catalog(items: list[dict], globals_: list[dict], targets: list[dict], plugin: str) -> dict:
    globals_by_id = {int(row["form_id"], 16): row["fields.Value"] for row in globals_}
    targets_by_id = {int(row["form_id"], 16): row for row in targets}
    prices = []
    skipped = 0
    for row in items:
        item_id = int(row["form_id"], 16)
        raw = bytes.fromhex(row["fields.GoldBullionValue"]["raw_hex"])
        if len(raw) != 4:
            raise ValueError(f"Invalid bullion reference for {row['eid']}")
        price = globals_by_id[struct.unpack("<I", raw)[0]]
        if not isinstance(price, (int, float)) or not 0 < price < 2**31 or int(price) != price:
            raise ValueError(f"Invalid bullion price for {row['eid']}")
        target = targets_by_id.get(item_id)
        if target is None:
            skipped += 1
            continue
        if target["eid"] != row["eid"] or target["signature"] != row["signature"]:
            raise ValueError(f"Converted identity mismatch for {row['eid']}; use mapped regeneration")
        prices.append({"plugin": plugin, "object_id": item_id, "price": int(price)})
    return {"schema_version": 1, "prices": sorted(prices, key=lambda row: row["object_id"]), "skipped_unmapped": skipped}


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--source", type=Path, required=True)
    parser.add_argument("--target", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    types = [value for signature in ("BOOK", "MISC", "ALCH", "ARMO", "WEAP", "AMMO") for value in ("--type", signature)]
    items = query(args.source, "fo76", "form_id,eid,signature,fields.GoldBullionValue", *types)
    globals_ = query(args.source, "fo76", "form_id,eid,fields.Value", "--type", "GLOB")
    targets = query(args.target, "fo4", "form_id,eid,signature", *types)
    catalog = build_catalog(items, globals_, targets, args.target.name)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    pending = args.output.with_suffix(".pending")
    pending.write_text(json.dumps(catalog, separators=(",", ":")) + "\n", encoding="utf-8")
    pending.replace(args.output)
    print(json.dumps({"path": str(args.output), "prices": len(catalog["prices"]), "skipped": catalog["skipped_unmapped"]}))


if __name__ == "__main__":
    main()
