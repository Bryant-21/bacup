from __future__ import annotations

import json
from pathlib import Path
from typing import Any

import yaml

MAP_PATH = Path("bacup/py_bacup_lib/native/conversion/src/embedded/translation_maps/fo76_to_fo4.yaml")
BASELINE_PATH = Path("bacup/tests/fixtures/conversion/fo76_to_fo4_semantic_inventory.json")


def _stable_payload(value: Any) -> str:
    return json.dumps(value, sort_keys=True, separators=(",", ":"))


def _record_blocks(data: dict[str, Any]) -> dict[str, dict[str, Any]]:
    return {
        str(key): value
        for key, value in data.items()
        if isinstance(key, str)
        and len(key) == 4
        and key.upper() == key
        and isinstance(value, dict)
    }


def build_inventory(data: dict[str, Any]) -> dict[str, Any]:
    records: dict[str, Any] = {}
    for sig, block in sorted(_record_blocks(data).items()):
        fields = block.get("fields") or {}
        transforms = block.get("transforms") or {}
        defaults = block.get("defaults") or {}
        value_maps = block.get("value_maps") or {}
        records[sig] = {
            "field_mappings": sorted(f"{src}->{dst}" for src, dst in fields.items()),
            "transforms": sorted(
                f"{name}:{_stable_payload(body)}"
                for name, body in transforms.items()
            ),
            "drops": sorted(str(item) for item in (block.get("drop") or [])),
            "defaults": sorted(
                f"{key}:{_stable_payload(value)}"
                for key, value in defaults.items()
            ),
            "value_maps": sorted(
                f"{key}:{_stable_payload(value)}"
                for key, value in value_maps.items()
            ),
        }
    return {"classified_missing_records": [], "records": records}


def test_fo76_to_fo4_semantic_inventory_preserved() -> None:
    current = build_inventory(yaml.safe_load(MAP_PATH.read_text(encoding="utf-8")) or {})
    baseline = json.loads(BASELINE_PATH.read_text(encoding="utf-8"))

    classified_missing_records = {
        item.split(":", 1)[1]
        for item in baseline.get("classified_missing_records", [])
        if item.startswith("LEGACY_ALIAS:")
        or item.startswith("REDUNDANT_IDENTITY:")
    }
    missing_records = sorted(set(baseline["records"]) - set(current["records"]))
    unexpected_missing_records = [
        sig for sig in missing_records
        if sig not in classified_missing_records
    ]
    assert unexpected_missing_records == []

    regressions: list[str] = []
    for sig, expected in baseline["records"].items():
        if sig in classified_missing_records and sig not in current["records"]:
            continue
        actual = current["records"][sig]
        for key in ("field_mappings", "transforms", "drops", "defaults", "value_maps"):
            missing = sorted(set(expected[key]) - set(actual[key]))
            allowed_missing = [
                item for item in missing
                if item.startswith("LEGACY_ALIAS:")
                or item.startswith("REDUNDANT_IDENTITY:")
            ]
            unexpected = sorted(set(missing) - set(allowed_missing))
            if unexpected:
                regressions.append(f"{sig}.{key}: {unexpected}")

    assert regressions == []
