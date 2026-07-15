"""Decode FNV IMOD modifiers into FO4 OMOD Properties entries."""
from __future__ import annotations

_FIELD_MAP: dict[str, str] = {
    "AmmoCapacity": "AmmoCapacity",
    "ReloadSpeed": "AttackDelaySec",
    "Damage": "AttackDamage",
    "Spread": "AimModelMaxConeDegrees",
    "Range": "MaxRange",
    "Weight": "Weight",
    "Value": "Value",
}


def decode_imod_modifiers(modifiers: list) -> tuple[list[dict], list[str]]:
    props: list[dict] = []
    warnings: list[str] = []
    if not isinstance(modifiers, list):
        return props, warnings
    for entry in modifiers:
        if not isinstance(entry, dict):
            warnings.append(f"IMOD modifier not a dict: {entry!r}")
            continue
        field = entry.get("Field")
        if not field:
            warnings.append(f"IMOD modifier missing Field: {entry!r}")
            continue
        if "Value" not in entry:
            warnings.append(f"IMOD modifier {field!r} missing Value")
            continue
        target = _FIELD_MAP.get(field)
        if target is None:
            warnings.append(f"IMOD modifier field {field!r} has no FO4 OMOD analog")
            continue
        props.append(
            {
                "Property": target,
                "Value": float(entry["Value"]),
                "FunctionType": "Add" if entry.get("Operation", "Add") == "Add" else "Set",
            }
        )
    return props, warnings
