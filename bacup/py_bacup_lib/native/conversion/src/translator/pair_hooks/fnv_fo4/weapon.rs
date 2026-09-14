use crate::ids::{FormKey, SubrecordSig};
use crate::record::{FieldEntry, FieldValue, Record};
use crate::sym::StringInterner;
use crate::target_fo4_melee::{
    FO4_BOTH_HANDS_EQUIP_LOCAL, FO4_RIGHT_HAND_EQUIP_LOCAL, FO4_UNARMED_WEAPON_EQUIP_LOCAL,
    Fo4MeleeProfile, Fo4MeleeSourcePayload, emit_fo4_melee_weapon,
};

const HATCHET_LOCAL: u32 = 0x11_A8E4;
const HATCHET_FIRST_PERSON_STAT_LOCAL: u32 = 0x11_A8E3;
const DUMMY_NV_CLEAVER_LOCAL: u32 = 0x10_9A08;
const HATCHET_REPAIR_LIST_LOCAL: u32 = 0x11_BC5E;
const HATCHET_PICKUP_SOUND_LOCAL: u32 = 0x08_54E0;
const HATCHET_DROP_SOUND_LOCAL: u32 = 0x08_54DF;
const HATCHET_IMPACT_SET_LOCAL: u32 = 0x06_0A0F;
const HATCHET_BLOCK_SOUND_LOCAL: u32 = 0x0B_F0EF;
const HATCHET_EQUIP_SOUND_LOCAL: u32 = 0x08_B5B2;
const HATCHET_UNEQUIP_SOUND_LOCAL: u32 = 0x08_B5B3;
const HATCHET_MODEL: &str = "weapons\\1handmelee\\Hatchet.NIF";
const HATCHET_ICON: &str = "interface\\icons\\pipboyimages\\weapons\\weapons_hatchet.dds";
const HATCHET_EDITOR_ID: &str = "WeapNVHatchet";
const HATCHET_NAME: &str = "Hatchet";
const HATCHET_VATS_ATTACK_NAME: &str = "Back Slash";

const BASE_ATTACK_SHOTS_SEC_BITS: u32 = 0x4022_7626;
const OWB_ATTACK_SHOTS_SEC_BITS: u32 = 0x4022_7627;

const FNV_MELEE_EQUIPMENT_TYPE: i64 = 3;
const FNV_UNARMED_EQUIPMENT_TYPE: i64 = 4;
const FO4_MACHETE_BLOCK_IMPACT_LOCAL: u32 = 0x0A_726B;
const FO4_MACHETE_BLOCK_MATERIAL_LOCAL: u32 = 0x0A_726C;
const FO4_MACHETE_SWING_SOUND_LOCAL: u32 = 0x03_C730;
const FO4_MACHETE_EQUIP_SOUND_LOCAL: u32 = 0x24_98BB;
const FO4_MACHETE_UNEQUIP_SOUND_LOCAL: u32 = 0x15_26AC;
const FO4_MACHETE_IMPACT_LOCAL: u32 = 0x01_3CAC;

const FO4_MACHETE_KEYWORDS: &[u32] = &[
    0x02_3465, 0x04_A0A4, 0x0C_F6B7, 0x09_3EB9, 0x10_C89B, 0x0F_4AEA,
];

const HATCHET_PROFILE: Fo4MeleeProfile = Fo4MeleeProfile {
    block_impact: FO4_MACHETE_BLOCK_IMPACT_LOCAL,
    block_material: FO4_MACHETE_BLOCK_MATERIAL_LOCAL,
    keywords: FO4_MACHETE_KEYWORDS,
    swing_sound: FO4_MACHETE_SWING_SOUND_LOCAL,
    idle_sound: None,
    equip_sound: FO4_MACHETE_EQUIP_SOUND_LOCAL,
    unequip_sound: FO4_MACHETE_UNEQUIP_SOUND_LOCAL,
    impact: FO4_MACHETE_IMPACT_LOCAL,
    animation_type: 1,
    flags: 256,
    animation_attack_seconds: 1.117_955,
    action_point_cost: 30.0,
    stagger: 1,
    trailing_unknowns: [0, 0, 0, 0],
};

const SUPPORTED_SOURCE_FIELDS: &[[u8; 4]] = &[
    *b"EDID", *b"OBND", *b"FULL", *b"MODL", *b"ICON", *b"EITM", *b"REPL", *b"ETYP", *b"YNAM",
    *b"ZNAM", *b"VANM", *b"INAM", *b"WNAM", *b"NAM6", *b"NAM9", *b"NAM8", *b"DATA", *b"DNAM",
    *b"CRDT", *b"VATS", *b"VNAM",
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LegacyCreatureWeaponSupport {
    pub reason_codes: Vec<String>,
}

impl LegacyCreatureWeaponSupport {
    pub(crate) fn is_ready(&self) -> bool {
        self.reason_codes.is_empty()
    }
}

pub(crate) fn classify_legacy_creature_weapon(
    record: &Record,
    interner: &StringInterner,
) -> LegacyCreatureWeaponSupport {
    let mut reasons = Vec::new();
    if record.sig.as_str() != "WEAP" {
        reasons.push("legacy_weapon_wrong_signature".to_string());
        return LegacyCreatureWeaponSupport {
            reason_codes: reasons,
        };
    }

    let data = exact_field(record, "DATA");
    match data.and_then(|value| source_game_data(value, interner)) {
        Some((_, health, _, _, _)) if health != 0 => {
            reasons.push("legacy_weapon_condition_has_no_fo4_equivalent".to_string())
        }
        Some(_) => {}
        None => reasons.push("legacy_weapon_data_shape_unverified".to_string()),
    }

    match exact_field(record, "DNAM") {
        Some(FieldValue::Bytes(bytes)) if bytes.len() >= 136 => {}
        Some(FieldValue::Struct(_)) => {}
        _ => reasons.push("legacy_weapon_dnam_shape_unverified".to_string()),
    }

    match exact_field(record, "ETYP") {
        Some(FieldValue::FormKey(_)) => {}
        Some(FieldValue::Int(_)) | Some(FieldValue::Uint(_)) => {
            reasons.push("legacy_weapon_equipment_type_requires_target_equip_slot".to_string())
        }
        _ => reasons.push("legacy_weapon_equipment_type_shape_unverified".to_string()),
    }

    match exact_field(record, "CRDT") {
        Some(value) => match legacy_critical_data(value, interner) {
            Some(critical) if critical.is_neutral() => {}
            Some(_) => {
                reasons.push("legacy_weapon_critical_chance_semantics_unrepresented".to_string())
            }
            None => reasons.push("legacy_weapon_critical_data_shape_unverified".to_string()),
        },
        None => reasons.push("legacy_weapon_critical_data_missing".to_string()),
    }

    if exact_field(record, "VATS").is_some_and(field_has_nonzero_scalar) {
        reasons.push("legacy_weapon_vats_attack_semantics_unrepresented".to_string());
    }
    if exact_field(record, "REPL").is_some_and(field_has_nonzero_scalar) {
        reasons.push("legacy_weapon_repair_list_has_no_fo4_condition_system".to_string());
    }
    if exact_field(record, "SCRI").is_some_and(field_has_nonzero_scalar) {
        reasons.push("legacy_weapon_script_requires_semantic_port".to_string());
    }
    if exact_field(record, "BIPL").is_some_and(field_has_nonzero_scalar) {
        reasons.push("legacy_weapon_biped_model_list_requires_target_projection".to_string());
    }
    if exact_field(record, "NAM6").is_some_and(field_has_nonzero_scalar) {
        reasons.push("legacy_weapon_block_sound_requires_material_impact_routing".to_string());
    }
    if count_fields(record, b"SNAM") > 1 {
        reasons.push("legacy_weapon_distance_sound_requires_compound_descriptor".to_string());
    }
    if ["WMS1", "WMS2"]
        .into_iter()
        .any(|signature| exact_field(record, signature).is_some_and(field_has_nonzero_scalar))
    {
        reasons.push("legacy_weapon_mod_sound_requires_imod_omod_projection".to_string());
    }
    if ["MOD2", "MOD3", "MOD4"]
        .into_iter()
        .any(|signature| exact_field(record, signature).is_some())
    {
        reasons.push("legacy_weapon_model_slot_relayout_unverified".to_string());
    }
    if exact_field(record, "WNAM").is_some_and(field_has_nonzero_scalar) {
        reasons.push("legacy_weapon_first_person_model_reference_unlowered".to_string());
    }
    if record.fields.iter().any(|field| {
        matches!(
            field.sig.as_str(),
            "MWD1"
                | "MWD2"
                | "MWD3"
                | "MWD4"
                | "MWD5"
                | "MWD6"
                | "MWD7"
                | "WMI1"
                | "WMI2"
                | "WMI3"
                | "WNM1"
                | "WNM2"
                | "WNM3"
                | "WNM4"
                | "WNM5"
                | "WNM6"
                | "WNM7"
        )
    }) {
        reasons.push("legacy_weapon_modular_models_require_imod_omod_projection".to_string());
    }
    if exact_field(record, "NNAM").is_some_and(field_has_nonzero_scalar) {
        reasons.push("legacy_weapon_embedded_mod_node_requires_target_projection".to_string());
    }

    reasons.sort();
    reasons.dedup();
    LegacyCreatureWeaponSupport {
        reason_codes: reasons,
    }
}

#[derive(Clone, Copy)]
struct LegacyCriticalData {
    damage: u16,
    multiplier: f32,
    flags: u8,
    effect_nonzero: bool,
}

impl LegacyCriticalData {
    fn is_neutral(self) -> bool {
        self.damage == 0
            && self.multiplier.to_bits() == 0.0_f32.to_bits()
            && self.flags == 0
            && !self.effect_nonzero
    }
}

fn legacy_critical_data(
    value: &FieldValue,
    interner: &StringInterner,
) -> Option<LegacyCriticalData> {
    match value {
        FieldValue::Bytes(bytes) if bytes.len() == 16 => Some(LegacyCriticalData {
            damage: u16::from_le_bytes(bytes[0..2].try_into().ok()?),
            multiplier: f32::from_le_bytes(bytes[4..8].try_into().ok()?),
            flags: bytes[8],
            effect_nonzero: u32::from_le_bytes(bytes[12..16].try_into().ok()?) != 0,
        }),
        FieldValue::Struct(fields) => Some(LegacyCriticalData {
            damage: named_u16(fields, "critical_damage", interner)?,
            multiplier: named_f32(fields, "crit_mult", interner)?,
            flags: named_u8(fields, "flags", interner)?,
            effect_nonzero: field_has_nonzero_scalar(named_value(fields, "effect", interner)?),
        }),
        _ => None,
    }
}

fn field_has_nonzero_scalar(value: &FieldValue) -> bool {
    match value {
        FieldValue::None => false,
        FieldValue::Bool(value) => *value,
        FieldValue::Int(value) => *value != 0,
        FieldValue::Uint(value) => *value != 0,
        FieldValue::Float(value) => value.to_bits() != 0.0_f32.to_bits(),
        FieldValue::FormKey(value) => value.local != 0,
        FieldValue::Bytes(bytes) => bytes.iter().any(|byte| *byte != 0),
        FieldValue::String(_) => true,
        FieldValue::List(values) => values.iter().any(field_has_nonzero_scalar),
        FieldValue::Struct(fields) => fields
            .iter()
            .any(|(_, value)| field_has_nonzero_scalar(value)),
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct HatchetSource {
    pub value: u32,
    pub weight: f32,
    pub damage: u16,
}

pub(crate) fn classify_hatchet(
    record: &Record,
    interner: &StringInterner,
) -> Option<HatchetSource> {
    if record.sig.as_str() != "WEAP"
        || record.form_key.local != HATCHET_LOCAL
        || interner.resolve(record.form_key.plugin) != Some("FalloutNV.esm")
        || record.flags.bits() != 0
        || record.eid.and_then(|editor_id| interner.resolve(editor_id)) != Some(HATCHET_EDITOR_ID)
        || record.fields.len() != SUPPORTED_SOURCE_FIELDS.len()
        || record
            .fields
            .iter()
            .any(|field| !SUPPORTED_SOURCE_FIELDS.contains(&field.sig.0))
        || SUPPORTED_SOURCE_FIELDS
            .iter()
            .any(|signature| count_fields(record, signature) != 1)
    {
        return None;
    }

    if string_value(exact_field(record, "EDID")?, interner) != Some(HATCHET_EDITOR_ID)
        || !source_bounds_are_hatchet(exact_field(record, "OBND")?, interner)
        || string_value(exact_field(record, "FULL")?, interner) != Some(HATCHET_NAME)
        || string_value(exact_field(record, "MODL")?, interner) != Some(HATCHET_MODEL)
        || string_value(exact_field(record, "ICON")?, interner) != Some(HATCHET_ICON)
        || string_value(exact_field(record, "VANM")?, interner) != Some(HATCHET_VATS_ATTACK_NAME)
        || integer_value(exact_field(record, "ETYP")?) != Some(FNV_MELEE_EQUIPMENT_TYPE)
    {
        return None;
    }

    for (signature, local) in [
        ("EITM", DUMMY_NV_CLEAVER_LOCAL),
        ("REPL", HATCHET_REPAIR_LIST_LOCAL),
        ("YNAM", HATCHET_PICKUP_SOUND_LOCAL),
        ("ZNAM", HATCHET_DROP_SOUND_LOCAL),
        ("INAM", HATCHET_IMPACT_SET_LOCAL),
        ("WNAM", HATCHET_FIRST_PERSON_STAT_LOCAL),
        ("NAM6", HATCHET_BLOCK_SOUND_LOCAL),
        ("NAM9", HATCHET_EQUIP_SOUND_LOCAL),
        ("NAM8", HATCHET_UNEQUIP_SOUND_LOCAL),
    ] {
        let form_key = form_key_value(exact_field(record, signature)?)?;
        if form_key.local != local || form_key.plugin != record.form_key.plugin {
            return None;
        }
    }

    let (value, health, weight, damage, clip_size) =
        source_game_data(exact_field(record, "DATA")?, interner)?;
    if value != 75
        || health != 300
        || !float_eq(weight, 2.0)
        || damage != 16
        || clip_size != 1
        || !source_dnam_is_hatchet(exact_field(record, "DNAM")?, interner)
        || !source_critical_data_is_hatchet(exact_field(record, "CRDT")?, interner)
        || !source_vats_is_hatchet(exact_field(record, "VATS")?, interner)
        || integer_value(exact_field(record, "VNAM")?) != Some(0)
    {
        return None;
    }

    Some(HatchetSource {
        value,
        weight,
        damage,
    })
}

pub(crate) fn project_hatchet(record: &mut Record, interner: &StringInterner) -> bool {
    let Some(source) = classify_hatchet(record, interner) else {
        return false;
    };
    let Some(editor_id) = clone_field(record, "EDID") else {
        return false;
    };
    let Some(bounds) = clone_field(record, "OBND") else {
        return false;
    };
    let Some(name) = clone_field(record, "FULL") else {
        return false;
    };
    let Some(model) = clone_field(record, "MODL") else {
        return false;
    };
    emit_fo4_melee_weapon(
        record,
        Fo4MeleeSourcePayload {
            editor_id,
            bounds,
            display_name: name,
            world_model: model,
            first_person_model: interner.intern(HATCHET_MODEL),
            value: source.value,
            weight: source.weight,
            damage: source.damage,
        },
        HATCHET_PROFILE,
        interner,
    );
    true
}

/// Rewrite a legacy `WEAP.ETYP` equipment-type ordinal as the FO4 EQUP it means.
///
/// FNV/FO3 `ETYP` is an `equip_type_enum` int32; FO4's is an EQUP FormID of the
/// same width under the same 4CC, so the generic translate carries the ordinal
/// through and the engine resolves `Fallout4.esm:00000N` — a marker STAT, or
/// nothing at all. `BGSEquipType` type-checks that, stores a null equip slot,
/// and `AIProcess::ProcessDialogueActivate` dereferences the null while walking
/// `MiddleHighProcessData::equippedItems`, so activating any actor holding such
/// a weapon is an access violation. Assign the same EQUP `target_fo4_melee`
/// gives a weapon it rebuilds, so both copy paths agree.
pub(super) fn relayout_weap_equipment_type(record: &mut Record, interner: &StringInterner) {
    let Some(equipment_type) = exact_field(record, "ETYP").and_then(integer_value) else {
        return;
    };
    let local = match equipment_type {
        FNV_MELEE_EQUIPMENT_TYPE => FO4_RIGHT_HAND_EQUIP_LOCAL,
        FNV_UNARMED_EQUIPMENT_TYPE => FO4_UNARMED_WEAPON_EQUIP_LOCAL,
        _ => FO4_BOTH_HANDS_EQUIP_LOCAL,
    };
    let plugin = interner.intern("Fallout4.esm");
    for entry in &mut record.fields {
        if entry.sig.0 == *b"ETYP" {
            entry.value = FieldValue::FormKey(FormKey { local, plugin });
        }
    }
}

fn source_game_data(
    value: &FieldValue,
    interner: &StringInterner,
) -> Option<(u32, u32, f32, u16, u8)> {
    match value {
        FieldValue::Bytes(bytes) if bytes.len() == 15 => Some((
            u32::try_from(i32::from_le_bytes(bytes[0..4].try_into().ok()?)).ok()?,
            u32::try_from(i32::from_le_bytes(bytes[4..8].try_into().ok()?)).ok()?,
            f32::from_le_bytes(bytes[8..12].try_into().ok()?),
            u16::try_from(i16::from_le_bytes(bytes[12..14].try_into().ok()?)).ok()?,
            bytes[14],
        )),
        FieldValue::Struct(fields) if fields.len() == 5 => Some((
            named_u32(fields, "value", interner)?,
            named_u32(fields, "health", interner)?,
            named_f32(fields, "weight", interner)?,
            named_u16(fields, "base_damage", interner)?,
            named_u8(fields, "clip_size", interner)?,
        )),
        _ => None,
    }
}

fn source_bounds_are_hatchet(value: &FieldValue, interner: &StringInterner) -> bool {
    match value {
        FieldValue::Bytes(bytes) => bytes.as_slice() == canonical_source_bounds(),
        FieldValue::Struct(fields) if fields.len() == 6 => [
            ("object_bounds_x1", -3),
            ("object_bounds_y1", -10),
            ("object_bounds_z1", -1),
            ("object_bounds_x2", 8),
            ("object_bounds_y2", 22),
            ("object_bounds_z2", 1),
        ]
        .into_iter()
        .all(|(name, expected)| named_i64(fields, name, interner) == Some(expected)),
        _ => false,
    }
}

fn source_dnam_is_hatchet(value: &FieldValue, _interner: &StringInterner) -> bool {
    let FieldValue::Bytes(bytes) = value else {
        return false;
    };
    let Some(attack_shots_sec_bits) = read_u32(bytes, 88) else {
        return false;
    };
    if !matches!(
        attack_shots_sec_bits,
        BASE_ATTACK_SHOTS_SEC_BITS | OWB_ATTACK_SHOTS_SEC_BITS
    ) {
        return false;
    }
    let mut expected = canonical_source_dnam();
    expected[88..92].copy_from_slice(&attack_shots_sec_bits.to_le_bytes());
    bytes.as_slice() == expected
}

fn source_critical_data_is_hatchet(value: &FieldValue, interner: &StringInterner) -> bool {
    match value {
        FieldValue::Struct(fields) if fields.len() == 9 => {
            named_i64(fields, "critical_damage", interner) == Some(16)
                && named_i64(fields, "unknown_u8_1", interner) == Some(0)
                && named_i64(fields, "unknown_u8_2", interner) == Some(0)
                && named_f32(fields, "crit_mult", interner).is_some_and(|v| float_eq(v, 1.0))
                && named_i64(fields, "flags", interner) == Some(1)
                && named_i64(fields, "unknown_u8_5", interner) == Some(0)
                && named_i64(fields, "unknown_u8_6", interner) == Some(0)
                && named_i64(fields, "unknown_u8_7", interner) == Some(0)
                && named_i64(fields, "effect", interner) == Some(0)
        }
        FieldValue::Bytes(bytes) => bytes.as_slice() == canonical_source_critical_data(),
        _ => false,
    }
}

fn source_vats_is_hatchet(value: &FieldValue, interner: &StringInterner) -> bool {
    match value {
        FieldValue::Struct(fields) if fields.len() == 8 => {
            named_i64(fields, "effect", interner) == Some(0)
                && named_f32(fields, "skill", interner).is_some_and(|v| float_eq(v, 50.0))
                && named_f32(fields, "dam_mult", interner).is_some_and(|v| float_eq(v, 0.7))
                && named_f32(fields, "ap", interner).is_some_and(|v| float_eq(v, 17.0))
                && named_i64(fields, "silent", interner) == Some(1)
                && named_i64(fields, "mod_required", interner) == Some(0)
                && named_i64(fields, "unknown_u8_6", interner) == Some(0)
                && named_i64(fields, "unknown_u8_7", interner) == Some(0)
        }
        FieldValue::Bytes(bytes) => bytes.as_slice() == canonical_source_vats(),
        _ => false,
    }
}

fn canonical_source_bounds() -> [u8; 12] {
    let mut bounds = [0_u8; 12];
    for (index, value) in [-3_i16, -10, -1, 8, 22, 1].into_iter().enumerate() {
        bounds[index * 2..index * 2 + 2].copy_from_slice(&value.to_le_bytes());
    }
    bounds
}

fn canonical_source_dnam() -> [u8; 204] {
    let mut dnam = [0_u8; 204];
    dnam[0..4].copy_from_slice(&1_u32.to_le_bytes());
    dnam[4..8].copy_from_slice(&1.0_f32.to_le_bytes());
    dnam[8..12].copy_from_slice(&0.7_f32.to_le_bytes());
    dnam[13] = 255;
    dnam[14] = 1;
    dnam[40] = 12;
    dnam[41] = 255;
    dnam[42] = 1;
    dnam[44..48].copy_from_slice(&500.0_f32.to_le_bytes());
    dnam[48..52].copy_from_slice(&2000.0_f32.to_le_bytes());
    dnam[52..56].copy_from_slice(&1_u32.to_le_bytes());
    dnam[56..60].copy_from_slice(&10_u32.to_le_bytes());
    dnam[60..64].copy_from_slice(&1.1_f32.to_le_bytes());
    dnam[68..72].copy_from_slice(&22.0_f32.to_le_bytes());
    dnam[88..92].copy_from_slice(&BASE_ATTACK_SHOTS_SEC_BITS.to_le_bytes());
    dnam[104..108].copy_from_slice(&38_i32.to_le_bytes());
    dnam[116..120].copy_from_slice(&1.5_f32.to_le_bytes());
    dnam[120..124].copy_from_slice(&(-1_i32).to_le_bytes());
    dnam[164..168].copy_from_slice(&99_u32.to_le_bytes());
    dnam[168..172].copy_from_slice(&2_u32.to_le_bytes());
    dnam[200..204].copy_from_slice(&25_u32.to_le_bytes());
    dnam
}

fn canonical_source_critical_data() -> [u8; 16] {
    let mut critical = [0_u8; 16];
    critical[0..2].copy_from_slice(&16_u16.to_le_bytes());
    critical[4..8].copy_from_slice(&1.0_f32.to_le_bytes());
    critical[8] = 1;
    critical
}

fn canonical_source_vats() -> [u8; 20] {
    let mut vats = [0_u8; 20];
    vats[4..8].copy_from_slice(&50.0_f32.to_le_bytes());
    vats[8..12].copy_from_slice(&0.7_f32.to_le_bytes());
    vats[12..16].copy_from_slice(&17.0_f32.to_le_bytes());
    vats[16] = 1;
    vats
}

fn exact_field<'a>(record: &'a Record, signature: &str) -> Option<&'a FieldValue> {
    let mut fields = record
        .fields
        .iter()
        .filter(|field| field.sig.as_str() == signature);
    let value = &fields.next()?.value;
    fields.next().is_none().then_some(value)
}

fn count_fields(record: &Record, signature: &[u8; 4]) -> usize {
    record
        .fields
        .iter()
        .filter(|field| field.sig.0 == *signature)
        .count()
}

fn clone_field(record: &Record, signature: &str) -> Option<FieldEntry> {
    let mut fields = record
        .fields
        .iter()
        .filter(|field| field.sig.as_str() == signature);
    let value = fields.next()?.clone();
    fields.next().is_none().then_some(value)
}

fn form_key_value(value: &FieldValue) -> Option<FormKey> {
    match value {
        FieldValue::FormKey(form_key) => Some(*form_key),
        _ => None,
    }
}

fn string_value<'a>(value: &FieldValue, interner: &'a StringInterner) -> Option<&'a str> {
    match value {
        FieldValue::String(value) => interner.resolve(*value),
        _ => None,
    }
}

fn integer_value(value: &FieldValue) -> Option<i64> {
    match value {
        FieldValue::Int(value) => Some(*value),
        FieldValue::Uint(value) => i64::try_from(*value).ok(),
        FieldValue::FormKey(form_key) if form_key.local == 0 => Some(0),
        _ => None,
    }
}

fn named_value<'a>(
    fields: &'a [(crate::sym::Sym, FieldValue)],
    name: &str,
    interner: &StringInterner,
) -> Option<&'a FieldValue> {
    let name = canonical_key(name);
    fields.iter().find_map(|(key, value)| {
        (canonical_key(interner.resolve(*key).unwrap_or_default()) == name).then_some(value)
    })
}

fn named_i64(
    fields: &[(crate::sym::Sym, FieldValue)],
    name: &str,
    interner: &StringInterner,
) -> Option<i64> {
    integer_value(named_value(fields, name, interner)?)
}

fn named_u32(
    fields: &[(crate::sym::Sym, FieldValue)],
    name: &str,
    interner: &StringInterner,
) -> Option<u32> {
    named_i64(fields, name, interner).and_then(|value| u32::try_from(value).ok())
}

fn named_u16(
    fields: &[(crate::sym::Sym, FieldValue)],
    name: &str,
    interner: &StringInterner,
) -> Option<u16> {
    named_i64(fields, name, interner).and_then(|value| u16::try_from(value).ok())
}

fn named_u8(
    fields: &[(crate::sym::Sym, FieldValue)],
    name: &str,
    interner: &StringInterner,
) -> Option<u8> {
    named_i64(fields, name, interner).and_then(|value| u8::try_from(value).ok())
}

fn named_f32(
    fields: &[(crate::sym::Sym, FieldValue)],
    name: &str,
    interner: &StringInterner,
) -> Option<f32> {
    match named_value(fields, name, interner)? {
        FieldValue::Float(value) => Some(*value),
        _ => None,
    }
}

fn canonical_key(value: &str) -> String {
    value
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn float_eq(left: f32, right: f32) -> bool {
    left.is_finite() && left.to_bits() == right.to_bits()
}

fn read_u32(bytes: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_le_bytes(
        bytes.get(offset..offset + 4)?.try_into().ok()?,
    ))
}

fn field(signature: &str, value: FieldValue) -> FieldEntry {
    FieldEntry {
        sig: SubrecordSig::from_str(signature).expect("valid subrecord signature"),
        value,
    }
}

fn form_key_field(signature: &str, plugin: crate::sym::Sym, local: u32) -> FieldEntry {
    field(signature, FieldValue::FormKey(FormKey { local, plugin }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::SigCode;
    use crate::schema::AuthoringSchema;
    use crate::target_fo4_melee::{
        FO4_RIGHT_HAND_EQUIP_LOCAL, audit_fo4_melee_dependencies, fo4_melee_dependencies,
        projection_snapshot,
    };
    use crate::target_normalize::{TargetRecordNormalization, TargetRecordNormalizer};
    use smallvec::SmallVec;

    fn source_hatchet(interner: &StringInterner) -> Record {
        let plugin = interner.intern("FalloutNV.esm");
        let mut record = Record::new(
            SigCode::from_str("WEAP").unwrap(),
            FormKey {
                local: HATCHET_LOCAL,
                plugin,
            },
        );
        record.eid = Some(interner.intern(HATCHET_EDITOR_ID));
        let source_ref = |signature, local| form_key_field(signature, plugin, local);
        record.fields = SmallVec::from_vec(vec![
            field(
                "EDID",
                FieldValue::String(interner.intern(HATCHET_EDITOR_ID)),
            ),
            field(
                "OBND",
                FieldValue::Bytes(SmallVec::from_slice(&canonical_source_bounds())),
            ),
            field("FULL", FieldValue::String(interner.intern(HATCHET_NAME))),
            field("MODL", FieldValue::String(interner.intern(HATCHET_MODEL))),
            field("ICON", FieldValue::String(interner.intern(HATCHET_ICON))),
            source_ref("EITM", DUMMY_NV_CLEAVER_LOCAL),
            source_ref("REPL", HATCHET_REPAIR_LIST_LOCAL),
            field("ETYP", FieldValue::Int(FNV_MELEE_EQUIPMENT_TYPE)),
            source_ref("YNAM", HATCHET_PICKUP_SOUND_LOCAL),
            source_ref("ZNAM", HATCHET_DROP_SOUND_LOCAL),
            field(
                "VANM",
                FieldValue::String(interner.intern(HATCHET_VATS_ATTACK_NAME)),
            ),
            source_ref("INAM", HATCHET_IMPACT_SET_LOCAL),
            source_ref("WNAM", HATCHET_FIRST_PERSON_STAT_LOCAL),
            source_ref("NAM6", HATCHET_BLOCK_SOUND_LOCAL),
            source_ref("NAM9", HATCHET_EQUIP_SOUND_LOCAL),
            source_ref("NAM8", HATCHET_UNEQUIP_SOUND_LOCAL),
            field(
                "DATA",
                structured(
                    interner,
                    vec![
                        ("value", FieldValue::Int(75)),
                        ("health", FieldValue::Int(300)),
                        ("weight", FieldValue::Float(2.0)),
                        ("base_damage", FieldValue::Int(16)),
                        ("clip_size", FieldValue::Uint(1)),
                    ],
                ),
            ),
            field(
                "DNAM",
                FieldValue::Bytes(SmallVec::from_slice(&canonical_source_dnam())),
            ),
            field(
                "CRDT",
                FieldValue::Bytes(SmallVec::from_slice(&canonical_source_critical_data())),
            ),
            field(
                "VATS",
                FieldValue::Bytes(SmallVec::from_slice(&canonical_source_vats())),
            ),
            field("VNAM", FieldValue::Uint(0)),
        ]);
        record
    }

    fn structured(interner: &StringInterner, values: Vec<(&str, FieldValue)>) -> FieldValue {
        FieldValue::Struct(
            values
                .into_iter()
                .map(|(name, value)| (interner.intern(name), value))
                .collect(),
        )
    }

    #[test]
    fn exact_hatchet_and_owb_float_variant_admit() {
        let interner = StringInterner::new();
        let exact = source_hatchet(&interner);
        assert!(classify_hatchet(&exact, &interner).is_some());

        let mut owb = exact;
        let FieldValue::Bytes(bytes) = &mut owb
            .fields
            .iter_mut()
            .find(|field| field.sig.as_str() == "DNAM")
            .unwrap()
            .value
        else {
            panic!("DNAM fixture");
        };
        bytes[88..92].copy_from_slice(&OWB_ATTACK_SHOTS_SEC_BITS.to_le_bytes());
        assert!(classify_hatchet(&owb, &interner).is_some());
    }

    #[test]
    fn creature_weapon_classifier_reports_exact_unlowered_legacy_semantics() {
        let interner = StringInterner::new();
        let support = classify_legacy_creature_weapon(&source_hatchet(&interner), &interner);

        assert_eq!(
            support.reason_codes,
            vec![
                "legacy_weapon_block_sound_requires_material_impact_routing",
                "legacy_weapon_condition_has_no_fo4_equivalent",
                "legacy_weapon_critical_chance_semantics_unrepresented",
                "legacy_weapon_equipment_type_requires_target_equip_slot",
                "legacy_weapon_first_person_model_reference_unlowered",
                "legacy_weapon_repair_list_has_no_fo4_condition_system",
                "legacy_weapon_vats_attack_semantics_unrepresented",
            ]
        );
    }

    #[test]
    fn creature_weapon_classifier_accepts_only_a_fully_target_valid_shape() {
        let interner = StringInterner::new();
        let source_plugin = interner.intern("FalloutNV.esm");
        let fallout4 = interner.intern("Fallout4.esm");
        let mut record = Record::new(
            SigCode::from_str("WEAP").unwrap(),
            FormKey {
                local: 0x123,
                plugin: source_plugin,
            },
        );
        record.fields = SmallVec::from_vec(vec![
            form_key_field("ETYP", fallout4, FO4_RIGHT_HAND_EQUIP_LOCAL),
            field(
                "DATA",
                structured(
                    &interner,
                    vec![
                        ("value", FieldValue::Int(1)),
                        ("health", FieldValue::Int(0)),
                        ("weight", FieldValue::Float(1.0)),
                        ("base_damage", FieldValue::Int(1)),
                        ("clip_size", FieldValue::Uint(1)),
                    ],
                ),
            ),
            field("DNAM", FieldValue::Bytes(SmallVec::from_slice(&[0; 136]))),
            field("CRDT", FieldValue::Bytes(SmallVec::from_slice(&[0; 16]))),
        ]);

        assert!(classify_legacy_creature_weapon(&record, &interner).is_ready());
    }

    #[test]
    fn classifier_rejects_contract_drift_and_throwing_hatchet() {
        let interner = StringInterner::new();

        let mut cases = Vec::new();
        let mut wrong_owner = source_hatchet(&interner);
        wrong_owner.form_key.plugin = interner.intern("OldWorldBlues.esm");
        cases.push(wrong_owner);

        let mut throwing = source_hatchet(&interner);
        throwing.form_key.local = 0x14_DE1D;
        throwing.eid = Some(interner.intern("WeapNVThrowingHatchet"));
        cases.push(throwing);

        let mut flagged = source_hatchet(&interner);
        flagged.flags = crate::record::RecordFlags::from_bits_retain(1);
        cases.push(flagged);

        for (signature, value) in [
            ("MODL", FieldValue::String(interner.intern("wrong.nif"))),
            ("ETYP", FieldValue::Int(2)),
            (
                "WNAM",
                FieldValue::FormKey(FormKey {
                    local: HATCHET_FIRST_PERSON_STAT_LOCAL + 1,
                    plugin: interner.intern("FalloutNV.esm"),
                }),
            ),
            (
                "EITM",
                FieldValue::FormKey(FormKey {
                    local: DUMMY_NV_CLEAVER_LOCAL + 1,
                    plugin: interner.intern("FalloutNV.esm"),
                }),
            ),
        ] {
            let mut drift = source_hatchet(&interner);
            drift
                .fields
                .iter_mut()
                .find(|field| field.sig.as_str() == signature)
                .unwrap()
                .value = value;
            cases.push(drift);
        }

        for signature in ["SCRI", "NAM0", "WMI1", "DEST", "MOD3", "UNKN"] {
            let mut drift = source_hatchet(&interner);
            drift.fields.push(field(signature, FieldValue::Uint(0)));
            cases.push(drift);
        }

        let mut ranged = source_hatchet(&interner);
        let FieldValue::Bytes(bytes) = &mut ranged
            .fields
            .iter_mut()
            .find(|field| field.sig.as_str() == "DNAM")
            .unwrap()
            .value
        else {
            panic!("DNAM fixture");
        };
        bytes[0..4].copy_from_slice(&2_u32.to_le_bytes());
        cases.push(ranged);

        for (signature, offset) in [("OBND", 0), ("DNAM", 15), ("CRDT", 2), ("VATS", 19)] {
            let mut drift = source_hatchet(&interner);
            let FieldValue::Bytes(bytes) = &mut drift
                .fields
                .iter_mut()
                .find(|field| field.sig.as_str() == signature)
                .unwrap()
                .value
            else {
                panic!("raw source fixture");
            };
            bytes[offset] ^= 1;
            cases.push(drift);
        }

        assert!(
            cases
                .iter()
                .all(|record| classify_hatchet(record, &interner).is_none())
        );
    }

    #[test]
    fn projection_is_target_native_and_normalizes_to_fo4_widths() {
        let interner = StringInterner::new();
        let mut record = source_hatchet(&interner);
        assert!(project_hatchet(&mut record, &interner));

        assert_eq!(
            string_value(exact_field(&record, "MOD4").unwrap(), &interner),
            Some(HATCHET_MODEL)
        );
        let fallout4 = interner.intern("Fallout4.esm");
        let mut emitted_dependencies = Vec::new();
        for field in &record.fields {
            collect_form_keys(&field.value, &mut emitted_dependencies);
        }
        assert!(!emitted_dependencies.is_empty());
        assert!(
            emitted_dependencies
                .iter()
                .all(|form_key| form_key.plugin == fallout4)
        );
        assert!(audit_fo4_melee_dependencies(
            &record,
            HATCHET_PROFILE,
            &interner
        ));
        assert!(
            fo4_melee_dependencies(HATCHET_PROFILE)
                .iter()
                .all(|dependency| {
                    emitted_dependencies
                        .iter()
                        .any(|form_key| form_key.local == dependency.local)
                })
        );
        assert_eq!(
            projection_snapshot(&record, &interner),
            r#"EDID=s:WeapNVHatchet|OBND=b:fdfff6ffffff080016000100|FULL=s:Hatchet|MODL=s:weapons\1handmelee\Hatchet.NIF|ETYP=r:013F42@Fallout4.esm|BIDS=r:0A726B@Fallout4.esm|BAMT=r:0A726C@Fallout4.esm|KSIZ=u:6|KWDA=[r:023465@Fallout4.esm,r:04A0A4@Fallout4.esm,r:0CF6B7@Fallout4.esm,r:093EB9@Fallout4.esm,r:10C89B@Fallout4.esm,r:0F4AEA@Fallout4.esm]|MOD4=s:weapons\1handmelee\Hatchet.NIF|MO4T=b:0400000000000000000000000000000000000000|DNAM={ammo=u:0,speed=f:3F800000,reload_speed=f:3F800000,reach=f:3F4CCCCD,min_range=f:00000000,max_range=f:41200000,attack_delay=f:00000000,unused=f:00000000,damage_outofrange_mult=f:BF800000,on_hit=u:1,skill=u:0,resist=u:0,flags=u:256,capacity=u:0,animation_type=u:1,damage_secondary=f:00000000,weight=f:40000000,value=u:75,damage_base=u:16,sound_level=u:4,sound_attack=r:03C730@Fallout4.esm,sound_attack_2d=u:0,sound_attack_loop=u:0,sound_attack_fail=u:0,sound_idle=u:0,sound_equip_sound=r:2498BB@Fallout4.esm,sound_unequip_sound=r:1526AC@Fallout4.esm,sound_fast_equip_sound=u:0,accuracy_bonus=u:0,animation_attack_seconds=f:3F8F1926,unknown_u8_30=u:0,unknown_u8_31=u:0,action_point_cost=f:41F00000,full_power_seconds=f:00000000,min_power_per_shot=f:00000000,stagger=u:1,unknown_u8_36=u:0,unknown_u8_37=u:0,unknown_u8_38=u:0,unknown_u8_39=u:0}|CRDT={crit_damage_mult=f:40000000,crit_charge_bonus=f:3F800000,crit_effect=u:0}|INAM=r:013CAC@Fallout4.esm"#
        );
        assert_eq!(
            integer_value(exact_field(&record, "KSIZ").unwrap()),
            Some(6)
        );

        let source_schema = AuthoringSchema::for_game("fnv").unwrap();
        let target_schema = AuthoringSchema::for_game("fo4").unwrap();
        let normalizer = TargetRecordNormalizer {
            target_schema: &target_schema,
            source_record_def: source_schema.record_def("WEAP"),
            interner: Some(&interner),
        };
        let TargetRecordNormalization::Keep(record) = normalizer.normalize(record) else {
            panic!("projected Hatchet must remain target-supported");
        };
        assert_eq!(
            structured_encoded_width(exact_field(&record, "DNAM").unwrap()),
            132
        );
        assert_eq!(
            structured_encoded_width(exact_field(&record, "CRDT").unwrap()),
            12
        );
    }

    fn collect_form_keys(value: &FieldValue, output: &mut Vec<FormKey>) {
        match value {
            FieldValue::FormKey(form_key) if form_key.local != 0 => output.push(*form_key),
            FieldValue::List(values) => {
                for value in values {
                    collect_form_keys(value, output);
                }
            }
            FieldValue::Struct(fields) => {
                for (_, value) in fields {
                    collect_form_keys(value, output);
                }
            }
            _ => {}
        }
    }

    fn structured_encoded_width(value: &FieldValue) -> usize {
        let FieldValue::Struct(fields) = value else {
            return 0;
        };
        fields
            .iter()
            .map(|(_, value)| match value {
                FieldValue::Bytes(bytes) => bytes.len(),
                FieldValue::FormKey(_) => 4,
                _ => 0,
            })
            .sum()
    }
}
