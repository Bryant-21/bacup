"""Public YAML field helpers used across the conversion pipeline."""
from __future__ import annotations

from typing import Any, Iterator


def to_ref(fk: str | None) -> dict | None:
    """'OBJID:Plugin.esm' shorthand → canonical {reference: {plugin, object_id}} or None.

    Inverse of ``from_ref``. Used by phase-2 mutations that emit canonical
    references built from FormKey strings.
    """
    if not isinstance(fk, str) or ":" not in fk:
        return None
    obj_id, plugin = fk.split(":", 1)
    return {"reference": {"plugin": plugin, "object_id": obj_id}}


def from_ref(ref: dict | None) -> str | None:
    """Canonical {reference: {plugin, object_id}} → 'OBJID:Plugin.esm' shorthand or None."""
    if not isinstance(ref, dict):
        return None
    inner = ref.get("reference")
    if not isinstance(inner, dict):
        return None
    plugin = inner.get("plugin", "")
    obj_id = inner.get("object_id", "")
    if not plugin or obj_id is None:
        return None
    obj_id_str = str(obj_id)
    # Null FormKey — skip
    if obj_id_str.lstrip("0") == "":
        return None
    return f"{obj_id_str}:{plugin}"


def field_at(record: dict, label: str) -> Any | None:
    """Return value of first matching label in record['fields'], or None."""
    for entry in record.get("fields", []):
        if not isinstance(entry, dict):
            continue
        val = entry.get(label)
        if val is not None:
            return val
        if label in entry:
            return entry[label]
    return None


def field_all(record: dict, label: str) -> list:
    """Return values of ALL occurrences of label in record['fields']."""
    results = []
    for entry in record.get("fields", []):
        if not isinstance(entry, dict):
            continue
        if label in entry:
            results.append(entry[label])
    return results


def set_field(record: dict, label: str, value: Any, *, before: str | None = None) -> None:
    """Insert or replace a fields-list entry. Optional position anchor."""
    fields = record.setdefault("fields", [])
    # Replace first existing occurrence
    for i, entry in enumerate(fields):
        if isinstance(entry, dict) and label in entry:
            fields[i] = {label: value}
            return
    # Insert before anchor if specified
    if before is not None:
        for i, entry in enumerate(fields):
            if isinstance(entry, dict) and before in entry:
                fields.insert(i, {label: value})
                return
    fields.append({label: value})


def iter_fields(record: dict) -> Iterator[tuple[str, Any]]:
    """Yield (label, value) pairs in canonical schema order."""
    for entry in record.get("fields", []):
        if isinstance(entry, dict):
            for k, v in entry.items():
                yield k, v


def remove_field(record: dict, label: str) -> int:
    """Remove every fields-list entry whose label matches. Returns count removed."""
    fields = record.get("fields")
    if not isinstance(fields, list):
        return 0
    before = len(fields)
    fields[:] = [
        entry
        for entry in fields
        if not (isinstance(entry, dict) and label in entry)
    ]
    return before - len(fields)
