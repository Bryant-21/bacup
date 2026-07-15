"""Deterministic naming for weapon attachment records and connect points."""
from __future__ import annotations

_VALID_SLOTS = (1, 2, 3)


def _check_slot(slot: int) -> None:
    if slot not in _VALID_SLOTS:
        raise ValueError(f"slot must be 1, 2, or 3 (got {slot})")


def attach_point_eid(prefix: str, weap_edid: str, slot: int) -> str:
    _check_slot(slot)
    return f"{prefix}_AP_{weap_edid}_Slot{slot}"


def attachment_omod_eid(prefix: str, weap_edid: str, slot: int) -> str:
    _check_slot(slot)
    return f"{prefix}_OMOD_{weap_edid}_Mod{slot}"


def attachment_misc_eid(prefix: str, weap_edid: str, slot: int) -> str:
    _check_slot(slot)
    return f"{prefix}_MISC_{weap_edid}_Mod{slot}"


def cobj_eid(prefix: str, weap_edid: str, slot: int) -> str:
    _check_slot(slot)
    return f"{prefix}_COBJ_{weap_edid}_Mod{slot}"


def association_keyword_eid(prefix: str, weap_edid: str) -> str:
    return f"{prefix}_KW_{weap_edid}Association"


def attachment_nif_relpath(prefix: str, weap_edid: str, slot: int) -> str:
    _check_slot(slot)
    return f"weapons/{prefix}_{weap_edid}_Mod{slot}.nif"


def connect_point_parent_name(slot: int) -> str:
    _check_slot(slot)
    return f"P-Mod{slot}"


def connect_point_child_name(slot: int) -> str:
    _check_slot(slot)
    return f"C-Mod{slot}"
