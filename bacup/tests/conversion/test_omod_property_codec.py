"""Tests for bacup_lib.omod_property_codec."""
from __future__ import annotations

import struct

import pytest

from bacup_lib.omod_property_codec import (
    FORM_TYPE_TO_TABLE,
    PROPERTY_TABLES,
    VT_BOOL,
    VT_FLOAT,
    VT_FORMID_INT,
    VT_INT,
    decode_property,
    encode_property,
    property_id_to_name,
    property_name_to_id,
)

_FT_WEAP = 1346454871  # b'WEAP' LE
_FT_ARMO = 1330467393  # b'ARMO' LE
_FT_NPC = 1598246990   # b'NPC_' LE


@pytest.mark.parametrize(
    "form_type,table",
    [(_FT_WEAP, "Weapon"), (_FT_ARMO, "Armor"), (_FT_NPC, "NPC")],
)
def test_name_roundtrip(form_type, table):
    assert FORM_TYPE_TO_TABLE[form_type] == table
    for prop_id, name in PROPERTY_TABLES[table].items():
        assert property_name_to_id(form_type, name) == prop_id
        assert property_id_to_name(form_type, prop_id) == name


def _float_bits(value: float) -> int:
    return struct.unpack("<I", struct.pack("<f", value))[0]


@pytest.mark.parametrize(
    "form_type,name,vt,value,kwargs,prop_id,value1",
    [
        (_FT_WEAP, "AttackDamage", VT_FLOAT, 12.5, {}, 28, _float_bits(12.5)),
        (_FT_WEAP, "MaxRange", VT_FLOAT, -1.0, {"function_type": 1}, None, _float_bits(-1.0)),
        (_FT_WEAP, "IsAutomatic", VT_BOOL, True, {}, 25, 1),
        (_FT_WEAP, "AmmoCapacity", VT_INT, 30, {}, 12, 30),
        (_FT_WEAP, "Keywords", VT_FORMID_INT, 182388, {"value2": 512, "function_type": 2}, 31, 182388),
        (_FT_ARMO, "Value", VT_INT, 100, {}, 5, 100),
        (_FT_NPC, "ForcedInventory", VT_FORMID_INT, 2396584, {"value2": 512}, 1, 2396584),
    ],
)
def test_encode_decode_roundtrip(form_type, name, vt, value, kwargs, prop_id, value1):
    encoded = encode_property(form_type, name, vt, value, **kwargs)
    assert encoded["ValueType"] == vt
    assert encoded["Value1"] == value1
    if prop_id is not None:
        assert encoded["Property"] == prop_id
    if "value2" in kwargs:
        assert encoded.get("Value2") == kwargs["value2"]
    if "function_type" in kwargs:
        assert encoded.get("FunctionType") == kwargs["function_type"]

    got_name, got_vt, v1, v2 = decode_property(form_type, encoded)
    assert got_name == name
    assert got_vt == vt
    if vt == VT_FLOAT:
        assert pytest.approx(v1, rel=1e-6) == value
    else:
        assert v1 == value
    if "value2" in kwargs:
        assert v2 == kwargs["value2"]


@pytest.mark.parametrize(
    "call,exc",
    [
        (lambda: encode_property(_FT_WEAP, "NotARealProp", VT_INT, 0), KeyError),
        (lambda: decode_property(_FT_WEAP, {"Property": 9999, "ValueType": VT_INT, "Value1": 0}), KeyError),
        (lambda: encode_property(_FT_WEAP, "IsAutomatic", VT_BOOL, 42), ValueError),
        (lambda: encode_property(_FT_WEAP, "AttackDamage", VT_FLOAT, "not_a_float"), (ValueError, TypeError)),
        (lambda: property_name_to_id(0xDEADBEEF, "Speed"), KeyError),
    ],
)
def test_invalid_inputs_raise(call, exc):
    with pytest.raises(exc):
        call()
