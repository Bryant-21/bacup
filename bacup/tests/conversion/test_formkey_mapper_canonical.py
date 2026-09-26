"""Tests for FormKeyMapper.rewrite_formkeys on canonical-shape records.

Canonical-shape records embed FormKey references as
``{reference: {plugin, object_id}}`` dicts alongside the ``"OBJID:Plugin.esm"``
string form.
"""
from __future__ import annotations

from bacup_lib.formkey.formkey_mapper import FormKeyMapper


def test_rewrites_string_and_canonical_refs():
    mapping = {
        "591667:SeventySix.esm": {"new_formkey": "013F42:Fallout4.esm"},
        "55C153:SeventySix.esm": {"new_formkey": "000800:B21_Test.esp"},
        "248AB9:SeventySix.esm": {"new_formkey": "248AB9:Fallout4.esm"},
        "0F4AE8:Fallout4.esm": {"new_formkey": "ABC123:B21_Test.esp"},
    }
    record = {
        "FormKey": "55C153:SeventySix.esm",
        "EquipmentType": "591667:SeventySix.esm",
        "PreviewTransform": {
            "reference": {"plugin": "SeventySix.esm", "object_id": "248AB9"},
            "weight": 1.0,
        },
        "fields": [
            {"Keywords": [
                {"reference": {"plugin": "Fallout4.esm", "object_id": "0F4AE8"}},
                {"reference": {"plugin": "Fallout4.esm", "object_id": "DEADBE"}},
            ]},
            {"MODL": "Weapons\\10mmPistol\\10mmRecieverDummy.nif"},
            {"Unmapped": "ABCDEF:Fallout4.esm"},
        ],
    }
    result = FormKeyMapper.rewrite_formkeys(record, mapping)
    assert result["FormKey"] == "000800:B21_Test.esp"
    assert result["EquipmentType"] == "013F42:Fallout4.esm"
    assert result["PreviewTransform"] == {
        "reference": {"plugin": "Fallout4.esm", "object_id": "248AB9"},
        "weight": 1.0,
    }
    assert result["fields"][0]["Keywords"] == [
        {"reference": {"plugin": "B21_Test.esp", "object_id": "ABC123"}},
        {"reference": {"plugin": "Fallout4.esm", "object_id": "DEADBE"}},
    ]
    assert result["fields"][1:] == record["fields"][1:]
