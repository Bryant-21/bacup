"""Convert a FNV IMOD-like authoring dict into a FO4 MISC authoring dict."""
from __future__ import annotations

from typing import Any

_DEFAULT_VALUE = 10
_DEFAULT_WEIGHT = 0.1


def _flatten_fields(record: dict) -> dict[str, Any]:
    flat: dict[str, Any] = {}
    for entry in record.get("fields", []) or []:
        if isinstance(entry, dict) and len(entry) == 1:
            key, value = next(iter(entry.items()))
            flat[key] = value
    return flat


def _name_field(value: Any, fallback: str) -> dict[str, Any]:
    if isinstance(value, dict) and "Values" in value:
        return value
    text = value if isinstance(value, str) and value else fallback
    return {
        "TargetLanguage": "English",
        "Values": [{"Value": text}],
    }


def imod_to_misc(imod: dict) -> dict:
    src = _flatten_fields(imod)
    name_value = src.get("Name", src.get("FULL"))
    data = src.get("Data")
    value = _DEFAULT_VALUE
    weight = _DEFAULT_WEIGHT
    if isinstance(data, dict):
        value = data.get("Value", _DEFAULT_VALUE)
        weight = data.get("Weight", _DEFAULT_WEIGHT)

    fields: list[dict[str, Any]] = [
        {"Name": _name_field(name_value, imod.get("eid", ""))},
        {"Data": {"Value": value, "Weight": weight}},
    ]
    if "ObjectBounds" in src:
        fields.append({"ObjectBounds": src["ObjectBounds"]})
    model = src.get("ModelFileName", src.get("MODL"))
    if isinstance(model, dict):
        filename = model.get("Filename")
        if isinstance(filename, str) and filename.strip():
            fields.append({"ModelFileName": filename})
    elif isinstance(model, str) and model.strip():
        fields.append({"ModelFileName": model})
    icon = src.get("IconFileName")
    if icon:
        fields.append({"IconFileName": icon})

    return {
        "eid": imod.get("eid", ""),
        "form_id": imod.get("form_id", ""),
        "fields": fields,
    }
