"""Tests for bacup_lib.omod_property_codec.

Round-trips every Weapon and Armor property name, validates real OMOD records,
and asserts error paths raise correctly.
"""
from __future__ import annotations

import os
import struct
from pathlib import Path

import pytest
import yaml

from bacup_lib.omod_property_codec import (
    FORM_TYPE_TO_TABLE,
    PROPERTY_TABLES,
    VT_BOOL,
    VT_ENUM,
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

_FO4_ESM_YAML = Path(
    os.environ.get("FO4_ESM_YAML_DIR")
    or Path(__file__).resolve().parents[3] / "data" / "fo4_esm_yaml"
)
_OMOD_DIR = _FO4_ESM_YAML / "Fallout4" / "records" / "OMOD"


# ---------------------------------------------------------------------------
# Table coverage: every name in every table has a round-trippable ID
# ---------------------------------------------------------------------------

@pytest.mark.parametrize("table_key", ["Weapon", "Armor", "Actor", "NPC", "Object"])
def test_all_tables_present(table_key):
    assert table_key in PROPERTY_TABLES


@pytest.mark.parametrize("prop_id,name", sorted(PROPERTY_TABLES["Weapon"].items()))
def test_weapon_name_roundtrip(prop_id, name):
    assert property_name_to_id(_FT_WEAP, name) == prop_id
    assert property_id_to_name(_FT_WEAP, prop_id) == name


@pytest.mark.parametrize("prop_id,name", sorted(PROPERTY_TABLES["Armor"].items()))
def test_armor_name_roundtrip(prop_id, name):
    assert property_name_to_id(_FT_ARMO, name) == prop_id
    assert property_id_to_name(_FT_ARMO, prop_id) == name


# ---------------------------------------------------------------------------
# Encode → decode round-trips for representative typed values
# ---------------------------------------------------------------------------

def test_encode_decode_float():
    """Float properties: Value1 = float reinterpreted as uint32."""
    encoded = encode_property(_FT_WEAP, "AttackDamage", VT_FLOAT, 12.5)
    assert encoded["ValueType"] == VT_FLOAT
    assert encoded["Property"] == 28  # AttackDamage
    # Value1 must be float bits
    expected_bits = struct.unpack("<I", struct.pack("<f", 12.5))[0]
    assert encoded["Value1"] == expected_bits

    name, vt, v1, _v2 = decode_property(_FT_WEAP, encoded)
    assert name == "AttackDamage"
    assert vt == VT_FLOAT
    assert pytest.approx(v1, rel=1e-6) == 12.5


def test_encode_decode_bool():
    """Bool properties: Value1 = 0 or 1."""
    encoded = encode_property(_FT_WEAP, "IsAutomatic", VT_BOOL, True)
    assert encoded["Property"] == 25
    assert encoded["Value1"] == 1

    name, vt, v1, _v2 = decode_property(_FT_WEAP, encoded)
    assert name == "IsAutomatic"
    assert vt == VT_BOOL
    assert v1 is True


def test_encode_decode_int():
    """Int properties: Value1 = raw uint32."""
    encoded = encode_property(_FT_WEAP, "AmmoCapacity", VT_INT, 30)
    assert encoded["Property"] == 12
    assert encoded["Value1"] == 30

    name, vt, v1, _v2 = decode_property(_FT_WEAP, encoded)
    assert name == "AmmoCapacity"
    assert vt == VT_INT
    assert v1 == 30


def test_encode_decode_formid():
    """FormID,Int properties: Value1 = FormID, Value2 = secondary uint32."""
    encoded = encode_property(
        _FT_WEAP, "Keywords", VT_FORMID_INT, 182388, value2=512, function_type=2
    )
    assert encoded["Property"] == 31
    assert encoded["Value1"] == 182388
    assert encoded.get("Value2") == 512
    assert encoded.get("FunctionType") == 2

    name, vt, v1, v2 = decode_property(_FT_WEAP, encoded)
    assert name == "Keywords"
    assert vt == VT_FORMID_INT
    assert v1 == 182388
    assert v2 == 512


def test_encode_decode_armor_value():
    """Armor Value property (ID=5, Int type)."""
    encoded = encode_property(_FT_ARMO, "Value", VT_INT, 100)
    assert encoded["Property"] == 5

    name, vt, v1, _v2 = decode_property(_FT_ARMO, encoded)
    assert name == "Value"
    assert v1 == 100


def test_encode_decode_npc_forced_inventory():
    """NPC ForcedInventory property (ID=1, FormID,Int type)."""
    encoded = encode_property(_FT_NPC, "ForcedInventory", VT_FORMID_INT, 2396584, value2=512)
    assert encoded["Property"] == 1

    name, vt, v1, v2 = decode_property(_FT_NPC, encoded)
    assert name == "ForcedInventory"
    assert v1 == 2396584


def test_negative_float_roundtrip():
    """Negative float values (e.g. MaxRange multiplier -1.0)."""
    encoded = encode_property(_FT_WEAP, "MaxRange", VT_FLOAT, -1.0, function_type=1)
    expected = struct.unpack("<I", struct.pack("<f", -1.0))[0]
    assert encoded["Value1"] == expected

    name, vt, v1, _v2 = decode_property(_FT_WEAP, encoded)
    assert name == "MaxRange"
    assert pytest.approx(v1, rel=1e-6) == -1.0


# ---------------------------------------------------------------------------
# Real-data round-trip: decode Properties from canonical OMOD YAML records
# ---------------------------------------------------------------------------

def _load_omod_records(form_type: int, max_records: int = 5) -> list[dict]:
    """Load up to max_records OMOD records with the given FormType."""
    if not _OMOD_DIR.exists():
        return []
    records = []
    for fname in sorted(_OMOD_DIR.iterdir()):
        if len(records) >= max_records:
            break
        try:
            with open(fname, encoding="utf-8") as f:
                doc = yaml.safe_load(f)
            for field in doc.get("fields", []):
                if isinstance(field, dict) and "Data" in field:
                    data = field["Data"]
                    if data.get("FormType") == form_type:
                        props = [
                            p for p in data.get("Properties", [])
                            if p.get("Property") is not None
                        ]
                        if props:
                            records.append({"ft": form_type, "props": props, "fname": fname.name})
        except (yaml.YAMLError, OSError):
            pass
    return records


def test_real_weap_omod_decode():
    """Decode Properties from 5 real WEAP OMOD records."""
    records = _load_omod_records(_FT_WEAP, 5)
    if not records:
        pytest.skip("OMOD YAML dir not available")

    assert len(records) >= 1
    for rec in records:
        for prop in rec["props"]:
            # Must resolve to a known name without raising
            name, vt, v1, v2 = decode_property(rec["ft"], prop)
            assert isinstance(name, str) and name
            # Float round-trip: repacking Value1 must reproduce the same uint32
            if vt == VT_FLOAT:
                repacked = struct.unpack("<I", struct.pack("<f", v1))[0]
                assert repacked == prop["Value1"], (
                    f"{rec['fname']} prop={prop}: float repack mismatch"
                )
            elif vt == VT_BOOL:
                assert v1 in (True, False)
            elif vt == VT_INT:
                assert isinstance(v1, int)
            elif vt == VT_FORMID_INT:
                assert isinstance(v1, int)


def test_real_armo_omod_decode():
    """Decode Properties from 5 real ARMO OMOD records."""
    records = _load_omod_records(_FT_ARMO, 5)
    if not records:
        pytest.skip("OMOD YAML dir not available")

    assert len(records) >= 1
    for rec in records:
        for prop in rec["props"]:
            name, vt, v1, v2 = decode_property(rec["ft"], prop)
            assert isinstance(name, str) and name


def test_real_npc_omod_decode():
    """Decode Properties from 5 real NPC_ OMOD records."""
    records = _load_omod_records(_FT_NPC, 5)
    if not records:
        pytest.skip("OMOD YAML dir not available")

    assert len(records) >= 1
    for rec in records:
        for prop in rec["props"]:
            name, vt, v1, v2 = decode_property(rec["ft"], prop)
            assert isinstance(name, str) and name


# ---------------------------------------------------------------------------
# Error paths
# ---------------------------------------------------------------------------

def test_encode_unknown_property_raises():
    with pytest.raises(KeyError, match="NotARealProp"):
        encode_property(_FT_WEAP, "NotARealProp", VT_INT, 0)


def test_decode_unknown_property_id_raises():
    with pytest.raises(KeyError):
        decode_property(_FT_WEAP, {"Property": 9999, "ValueType": VT_INT, "Value1": 0})


def test_encode_bool_bad_value_raises():
    with pytest.raises(ValueError, match="Bool"):
        encode_property(_FT_WEAP, "IsAutomatic", VT_BOOL, 42)


def test_encode_float_bad_value_raises():
    with pytest.raises((ValueError, TypeError)):
        encode_property(_FT_WEAP, "AttackDamage", VT_FLOAT, "not_a_float")


def test_unknown_form_type_uses_object_table():
    """FormType not in FORM_TYPE_TO_TABLE falls back to 'Object'."""
    unknown_ft = 0xDEADBEEF
    # Object table is empty, so any name raises KeyError
    with pytest.raises(KeyError):
        property_name_to_id(unknown_ft, "Speed")


def test_form_type_to_table_covers_seen_form_types():
    """All FormTypes seen in real OMOD records are in FORM_TYPE_TO_TABLE."""
    seen = {_FT_WEAP, _FT_ARMO, _FT_NPC}
    for ft in seen:
        assert ft in FORM_TYPE_TO_TABLE, f"Missing FormType {ft} in FORM_TYPE_TO_TABLE"


# ---------------------------------------------------------------------------
# FORM_TYPE_TO_TABLE encoding sanity
# ---------------------------------------------------------------------------

def test_form_type_encoding():
    """Verify FormType values decode to expected ASCII SIGs via LE uint32."""
    expected = {
        _FT_WEAP: b"WEAP",
        _FT_ARMO: b"ARMO",
        _FT_NPC: b"NPC_",
    }
    for ft, sig in expected.items():
        assert struct.pack("<I", ft) == sig, f"FormType {ft} != {sig!r}"
