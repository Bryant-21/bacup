"""OMOD Property uint16 ↔ name codec with Value1/Value2 packing.

Sources:
  - Property enum tables from xEdit's wbDefinitionsFO4.pas
    (wbWeaponPropertyEnum, wbArmorPropertyEnum, wbActorPropertyEnum)
  - Value packing rules verified against canonical OMOD records in
    data/fo4_esm_yaml/Fallout4/records/OMOD/

FormType encoding: xEdit stores FormType as a uint32 little-endian packed
signature, so b'WEAP' little-endian = 1346454871.

ValueType semantics (from wbOMODDataPropertyValue1Decider):
  0 Int        → Value1 is a raw uint32 integer
  1 Float      → Value1 is a float reinterpreted as uint32
  2 Bool       → Value1 is uint32 (0 or 1)
  4 FormID,Int → Value1 is a FormID uint32; Value2 is a secondary uint32
  5 Enum       → Value1 is a uint32 enum index
  6 FormID,Float → Value1 is a FormID uint32; Value2 is a float as uint32

If a property name or Property uint16 is not in the relevant table, encode/
decode raise KeyError — no silent fallback.
"""
from __future__ import annotations

import struct
from typing import Union

# ---------------------------------------------------------------------------
# Property tables: {form_sig: {uint16_id: str_name}}
# Source: wbDefinitionsFO4.pas lines 8882-8976
# Note: xEdit source has a typo labelling ImpactDataSet as {50} — real game
#       data uses index 60 (confirmed against 2409 canonical OMOD records).
# ---------------------------------------------------------------------------

PROPERTY_TABLES: dict[str, dict[int, str]] = {
    "Weapon": {
        0: "Speed",
        1: "Reach",
        2: "MinRange",
        3: "MaxRange",
        4: "AttackDelaySec",
        5: "Unknown 5",
        6: "OutOfRangeDamageMult",
        7: "SecondaryDamage",
        8: "CriticalChargeBonus",
        9: "HitBehaviour",
        10: "Rank",
        11: "Unknown 11",
        12: "AmmoCapacity",
        13: "Unknown 13",
        14: "Unknown 14",
        15: "Type",
        16: "IsPlayerOnly",
        17: "NPCsUseAmmo",
        18: "HasChargingReload",
        19: "IsMinorCrime",
        20: "IsFixedRange",
        21: "HasEffectOnDeath",
        22: "HasAlternateRumble",
        23: "IsNonHostile",
        24: "IgnoreResist",
        25: "IsAutomatic",
        26: "CantDrop",
        27: "IsNonPlayable",
        28: "AttackDamage",
        29: "Value",
        30: "Weight",
        31: "Keywords",
        32: "AimModel",
        33: "AimModelMinConeDegrees",
        34: "AimModelMaxConeDegrees",
        35: "AimModelConeIncreasePerShot",
        36: "AimModelConeDecreasePerSec",
        37: "AimModelConeDecreaseDelayMs",
        38: "AimModelConeSneakMultiplier",
        39: "AimModelRecoilDiminishSpringForce",
        40: "AimModelRecoilDiminishSightsMult",
        41: "AimModelRecoilMaxDegPerShot",
        42: "AimModelRecoilMinDegPerShot",
        43: "AimModelRecoilHipMult",
        44: "AimModelRecoilShotsForRunaway",
        45: "AimModelRecoilArcDeg",
        46: "AimModelRecoilArcRotateDeg",
        47: "AimModelConeIronSightsMultiplier",
        48: "HasScope",
        49: "ZoomDataFOVMult",
        50: "FireSeconds",
        51: "NumProjectiles",
        52: "AttackSound",
        53: "AttackSound2D",
        54: "AttackLoop",
        55: "AttackFailSound",
        56: "IdleSound",
        57: "EquipSound",
        58: "UnEquipSound",
        59: "SoundLevel",
        60: "ImpactDataSet",
        61: "Ammo",
        62: "CritEffect",
        63: "BashImpactDataSet",
        64: "BlockMaterial",
        65: "Enchantments",
        66: "AimModelBaseStability",
        67: "ZoomData",
        68: "ZoomDataOverlay",
        69: "ZoomDataImageSpace",
        70: "ZoomDataCameraOffsetX",
        71: "ZoomDataCameraOffsetY",
        72: "ZoomDataCameraOffsetZ",
        73: "EquipSlot",
        74: "SoundLevelMult",
        75: "NPCAmmoList",
        76: "ReloadSpeed",
        77: "DamageTypeValues",
        78: "AccuracyBonus",
        79: "AttackActionPointCost",
        80: "OverrideProjectile",
        81: "HasBoltAction",
        82: "StaggerValue",
        83: "SightedTransitionSeconds",
        84: "FullPowerSeconds",
        85: "HoldInputToPower",
        86: "HasRepeatableSingleFire",
        87: "MinPowerPerShot",
        88: "ColorRemappingIndex",
        89: "MaterialSwaps",
        90: "CriticalDamageMult",
        91: "FastEquipSound",
        92: "DisableShells",
        93: "HasChargingAttack",
        94: "ActorValues",
    },
    "Armor": {
        0: "Enchantments",
        1: "BashImpactDataSet",
        2: "BlockMaterial",
        3: "Keywords",
        4: "Weight",
        5: "Value",
        6: "Rating",
        7: "AddonIndex",
        8: "BodyPart",
        9: "DamageTypeValue",
        10: "ActorValues",
        11: "Health",
        12: "ColorRemappingIndex",
        13: "MaterialSwaps",
    },
    "Actor": {
        0: "Keywords",
        1: "ForcedInventory",
        2: "XPOffset",
        3: "Enchantments",
        4: "ColorRemappingIndex",
        5: "MaterialSwaps",
    },
    # Object FormType has no property enum in xEdit; lookups raise KeyError.
    "Object": {},
}

# NPC_ FormType uses the Actor property enum (xEdit: wbActorPropertyEnum).
PROPERTY_TABLES["NPC"] = PROPERTY_TABLES["Actor"]

# Reverse lookups built at module load
_REVERSE: dict[str, dict[str, int]] = {
    sig: {name: uid for uid, name in tbl.items()}
    for sig, tbl in PROPERTY_TABLES.items()
}

# ---------------------------------------------------------------------------
# FormType → table name
# Keys are uint32 SIG bytes packed little-endian (same encoding as the
# FormType field in canonical OMOD Data YAMLs).
# FormType=NONE (b'NONE' LE = 1162760014) covers 393 canonical OMOD records,
# all with empty Properties; intentionally not mapped — encode/decode raises
# if it's ever exercised.
# ---------------------------------------------------------------------------
FORM_TYPE_TO_TABLE: dict[int, str] = {
    int.from_bytes(b"WEAP", "little"): "Weapon",
    int.from_bytes(b"ARMO", "little"): "Armor",
    int.from_bytes(b"ARMA", "little"): "Armor",
    int.from_bytes(b"NPC_", "little"): "NPC",
}

# ---------------------------------------------------------------------------
# ValueType constants (from wbEnum in wbObjectModProperties)
# ---------------------------------------------------------------------------
VT_INT = 0
VT_FLOAT = 1
VT_BOOL = 2
VT_UNKNOWN_3 = 3
VT_FORMID_INT = 4
VT_ENUM = 5
VT_FORMID_FLOAT = 6


def _table_for(form_type: int) -> str:
    """Return the table key for this FormType, defaulting to 'Object'."""
    return FORM_TYPE_TO_TABLE.get(form_type, "Object")


def property_id_to_name(form_type: int, property_id: int) -> str:
    """Resolve a uint16 Property ID to its canonical name.

    Raises KeyError if the ID is not in the table for this form_type.
    """
    tbl_key = _table_for(form_type)
    tbl = PROPERTY_TABLES[tbl_key]
    if property_id not in tbl:
        raise KeyError(
            f"Unknown Property ID {property_id} for FormType {form_type!r} "
            f"(table '{tbl_key}')"
        )
    return tbl[property_id]


def property_name_to_id(form_type: int, name: str) -> int:
    """Resolve a property name to its uint16 ID.

    Raises KeyError if the name is not in the table for this form_type.
    """
    tbl_key = _table_for(form_type)
    rev = _REVERSE[tbl_key]
    if name not in rev:
        raise KeyError(
            f"Unknown Property name {name!r} for FormType {form_type!r} "
            f"(table '{tbl_key}')"
        )
    return rev[name]


# ---------------------------------------------------------------------------
# Value packing helpers
# ---------------------------------------------------------------------------

def _pack_float(v: float) -> int:
    return struct.unpack("<I", struct.pack("<f", v))[0]


def _unpack_float(v: int) -> float:
    return struct.unpack("<f", struct.pack("<I", v))[0]


# ---------------------------------------------------------------------------
# Public encode / decode
# ---------------------------------------------------------------------------

DecodedValue = Union[float, int, bool]


def encode_property(
    form_type: int,
    property_name: str,
    value_type: int,
    value: DecodedValue,
    value2: DecodedValue = 0,
    function_type: int = 0,
) -> dict:
    """Return a canonical property dict {ValueType, FunctionType, Property, Value1, Value2}.

    Args:
        form_type:     uint32 FormType from OMOD Data (little-endian SIG).
        property_name: human-readable property name (e.g. 'AttackDamage').
        value_type:    ValueType integer (VT_INT=0, VT_FLOAT=1, VT_BOOL=2, …).
        value:         the typed payload for Value1.
        value2:        the typed payload for Value2 (default 0).
        function_type: FunctionType integer (SET=0, MUL+ADD/AND/REM=1, ADD/OR=2).

    Raises:
        KeyError:   if property_name is not in the table for this form_type.
        ValueError: if value cannot be encoded for the given value_type.
    """
    prop_id = property_name_to_id(form_type, property_name)

    v1 = _encode_value(value_type, value, "Value1")
    v2 = _encode_value2(value_type, value2)

    out: dict = {
        "ValueType": value_type,
        "Property": prop_id,
        "Value1": v1,
    }
    if function_type:
        out["FunctionType"] = function_type
    if v2:
        out["Value2"] = v2
    return out


def decode_property(form_type: int, encoded: dict) -> tuple[str, int, DecodedValue, DecodedValue]:
    """Decode a canonical property dict to (name, value_type, value1, value2).

    Returns the property name plus the typed Value1/Value2.  The caller is
    responsible for interpreting value_type (VT_FLOAT vs VT_INT, etc.).

    Raises:
        KeyError: if the Property uint16 is not in the table for this form_type.
    """
    prop_id = encoded.get("Property", 0)
    name = property_id_to_name(form_type, prop_id)

    vt = encoded.get("ValueType", 0)
    raw1 = encoded.get("Value1", 0)
    raw2 = encoded.get("Value2", 0)

    v1 = _decode_value(vt, raw1)
    v2 = _decode_value2(vt, raw2)

    return name, vt, v1, v2


# ---------------------------------------------------------------------------
# Internal packing helpers
# ---------------------------------------------------------------------------

def _encode_value(vt: int, value: DecodedValue, label: str) -> int:
    if vt == VT_FLOAT:
        try:
            return _pack_float(float(value))
        except (TypeError, ValueError) as e:
            raise ValueError(f"{label}: cannot pack {value!r} as float: {e}") from e
    if vt == VT_BOOL:
        if not isinstance(value, (bool, int)) or value not in (0, 1, True, False):
            raise ValueError(f"{label}: Bool value must be 0/1/True/False, got {value!r}")
        return int(value)
    if vt in (VT_INT, VT_ENUM, VT_UNKNOWN_3):
        if not isinstance(value, int):
            raise ValueError(f"{label}: Int/Enum value must be int, got {type(value).__name__!r}")
        return value
    if vt in (VT_FORMID_INT, VT_FORMID_FLOAT):
        if not isinstance(value, int):
            raise ValueError(f"{label}: FormID value must be int, got {type(value).__name__!r}")
        return value
    raise ValueError(f"{label}: unknown ValueType {vt}")


def _encode_value2(vt: int, value: DecodedValue) -> int:
    if vt == VT_FLOAT:
        return _pack_float(float(value)) if value else 0
    if vt == VT_BOOL:
        return int(bool(value))
    if vt in (VT_INT, VT_FORMID_INT):
        return int(value)
    if vt == VT_FORMID_FLOAT:
        return _pack_float(float(value)) if value else 0
    return 0


def _decode_value(vt: int, raw: int) -> DecodedValue:
    if vt == VT_FLOAT:
        return _unpack_float(raw)
    if vt == VT_BOOL:
        return bool(raw)
    return raw


def _decode_value2(vt: int, raw: int) -> DecodedValue:
    if vt in (VT_FLOAT,):
        return _unpack_float(raw) if raw else 0.0
    if vt == VT_FORMID_FLOAT:
        return _unpack_float(raw) if raw else 0.0
    if vt == VT_BOOL:
        return bool(raw)
    return raw
