"""Per-WEAP fan-out for synthesized attachment records."""
from __future__ import annotations

import struct
from dataclasses import dataclass, field
from typing import Any

from bacup_lib.attach_point_keyword_naming import (
    association_keyword_eid,
    attach_point_eid,
    attachment_misc_eid,
    attachment_nif_relpath,
    attachment_omod_eid,
    cobj_eid,
)
from bacup_lib.imod_to_misc import imod_to_misc
from bacup_lib.weapon_property_decoder import decode_imod_modifiers
from bacup_lib.yaml_helpers import to_ref

_FO4_COMPONENT_FORMKEYS = {
    "Steel": "0731A4:Fallout4.esm",
    "Adhesive": "1BF72E:Fallout4.esm",
}


@dataclass
class SynthesizedRecords:
    extra_records: list[dict] = field(default_factory=list)
    weap_field_updates: dict[str, Any] = field(default_factory=dict)
    slot_record_index: dict[tuple[str, int], list[str]] = field(default_factory=dict)
    warnings: list[str] = field(default_factory=list)


def _flatten_fields(record: dict) -> dict[str, Any]:
    flat: dict[str, Any] = {}
    for entry in record.get("fields", []) or []:
        if isinstance(entry, dict) and len(entry) == 1:
            key, value = next(iter(entry.items()))
            flat[key] = value
    return flat


def _imod_modifiers(imod: dict) -> list:
    data = _flatten_fields(imod).get("Data")
    if not isinstance(data, dict):
        return []
    modifiers = data.get("Modifiers")
    return modifiers if isinstance(modifiers, list) else []


def _make_keyword(eid: str) -> dict:
    return {"eid": eid, "fields": []}


def _make_omod(
    eid: str,
    weap_eid: str,
    slot: int,
    mod_prefix: str,
    association_eid: str,
    misc_eid: str,
    properties: list[dict],
) -> dict:
    return {
        "eid": eid,
        "fields": [
            {"ModelFileName": attachment_nif_relpath(mod_prefix, weap_eid, slot)},
            {
                "Data": {
                    "FormType": struct.unpack("<I", b"WEAP")[0],
                    "AttachPoint": attach_point_eid(mod_prefix, weap_eid, slot),
                    "Includes": [],
                    "Properties": properties,
                }
            },
            {"TargetOMODKeywords": [association_eid]},
            {"LooseMod": misc_eid},
        ],
    }


def _make_cobj(eid: str, omod_eid: str) -> dict:
    return {
        "eid": eid,
        "fields": [
            {"CreatedObject": omod_eid},
            {"Data": {"CreatedObjectCount": 1}},
            {
                "Components": [
                    {
                        "ComponentsComponent": to_ref(_FO4_COMPONENT_FORMKEYS["Steel"]),
                        "ComponentsCount": 2,
                    },
                    {
                        "ComponentsComponent": to_ref(_FO4_COMPONENT_FORMKEYS["Adhesive"]),
                        "ComponentsCount": 1,
                    },
                ]
            },
        ],
    }


def _make_misc_from_imod_or_default(
    weap_eid: str,
    slot: int,
    mod_prefix: str,
    imod: dict | None,
) -> dict:
    eid = attachment_misc_eid(mod_prefix, weap_eid, slot)
    if imod is not None:
        misc = imod_to_misc(imod)
        misc["eid"] = eid
        return misc
    return {
        "eid": eid,
        "fields": [
            {
                "Name": {
                    "TargetLanguage": "English",
                    "Values": [{"Value": f"{weap_eid} Mod {slot}"}],
                }
            },
            {"Data": {"Value": 10, "Weight": 0.1}},
        ],
    }


def synthesize_weapon_attachments(
    weap: dict,
    imod_lookup: dict[str, dict],
    mod_prefix: str,
) -> SynthesizedRecords:
    out = SynthesizedRecords()
    flat = _flatten_fields(weap)
    weap_eid = weap.get("eid", "")

    populated_slots = [slot for slot in (1, 2, 3) if flat.get(f"ModelMod{slot}")]
    if not populated_slots:
        return out

    association_eid = association_keyword_eid(mod_prefix, weap_eid)
    for slot in populated_slots:
        ap_eid = attach_point_eid(mod_prefix, weap_eid, slot)
        omod_eid = attachment_omod_eid(mod_prefix, weap_eid, slot)
        misc_eid = attachment_misc_eid(mod_prefix, weap_eid, slot)
        cobj_record_eid = cobj_eid(mod_prefix, weap_eid, slot)
        linked_imod_eid = flat.get(f"ModSlot{slot}Linked")
        imod = imod_lookup.get(linked_imod_eid) if linked_imod_eid else None
        properties: list[dict] = []
        if imod is not None:
            properties, decode_warnings = decode_imod_modifiers(_imod_modifiers(imod))
            out.warnings.extend(decode_warnings)

        out.extra_records.append(_make_keyword(ap_eid))
        out.extra_records.append(
            _make_omod(
                omod_eid,
                weap_eid,
                slot,
                mod_prefix,
                association_eid,
                misc_eid,
                properties,
            )
        )
        out.extra_records.append(
            _make_misc_from_imod_or_default(weap_eid, slot, mod_prefix, imod)
        )
        out.extra_records.append(_make_cobj(cobj_record_eid, omod_eid))
        out.slot_record_index[(weap_eid, slot)] = [
            ap_eid,
            omod_eid,
            misc_eid,
            cobj_record_eid,
        ]

    out.extra_records.append(_make_keyword(association_eid))
    out.weap_field_updates = {
        "AttachParentSlots": [
            attach_point_eid(mod_prefix, weap_eid, slot) for slot in populated_slots
        ],
        "Keywords": [association_eid],
    }
    return out
