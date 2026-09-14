use super::*;

pub(super) const TESLA_CANNON_BASE_MODEL: &str = "weapons/teslacannon/weapon_teslacannon.nif";
pub(super) const POWER_ARMOR_MODEL_PREFIX: &str = "actors/powerarmor/";

#[derive(Clone, Copy)]
pub(super) enum ObjectModPropertyTarget {
    Weapon,
    Armor,
    Actor,
    Object,
}

pub(super) const OBJECT_MOD_PROPERTY_ROW_LEN: usize = 24;
pub(super) const OBJECT_MOD_PROPERTY_FUNCTION_TYPE_OFFSET: usize = 4;
pub(super) const OBJECT_MOD_PROPERTY_ID_OFFSET: usize = 8;
pub(super) const OBJECT_TEMPLATE_PROPERTY_COUNT_OFFSET: usize = 4;
pub(super) const OBJECT_TEMPLATE_FIXED_HEADER_LEN: usize = 16;
pub(super) const OBJECT_TEMPLATE_KEYWORD_COUNT_OFFSET: usize = 15;
pub(super) const OBJECT_TEMPLATE_INCLUDE_ROW_LEN: usize = 7;
pub(super) const OMOD_DATA_HEADER_LEN: usize = 20;
pub(super) const OMOD_DATA_FORM_TYPE_OFFSET: usize = 10;
pub(super) const OMOD_DATA_ATTACH_POINT_OFFSET: usize = 16;
pub(super) const OMOD_DATA_ATTACH_PARENT_SLOT_COUNT_LEN: usize = 4;
pub(super) const OMOD_DATA_ITEM_ROW_LEN: usize = 4;
pub(super) const OMOD_DATA_INCLUDE_ROW_LEN: usize = 7;
pub(super) const FO76_MSTT_FORM_TYPE: u32 = 0x5454_534D;
pub(super) const LIBERATOR_BODY_ARMOR_OMOD_EDITOR_ID: &str = "Bot_Liberator_BodyArmor";
pub(super) const LIBERATOR_RACE_EDITOR_ID: &str = "LiberatorRace";
pub(super) const FO4_AP_BOT_ARMOR_SLOT1_OBJECT_ID: u32 = 0x000C_3666;

/// FO76's six per-part power-armor attach points — `ap_PowerArmor_HeadMod`,
/// `BodyMod`, `LArmMod`, `RArmMod`, `RLegMod`, `LLegMod`. Both games ship these
/// EditorIDs at these object-ids, so the EditorID remap maps them onto
/// themselves — but they mean opposite things. In FO76 each is displayed as
/// "Paint" and carries the piece's paint OMOD; in FO4 each is an unnamed
/// structural root carrying the chassis OMOD, which in turn grants
/// `ap_PowerArmor_Paint`.
pub(super) const FO76_POWER_ARMOR_PART_ATTACH_POINT_OBJECT_IDS: [u32; 6] = [
    0x0003_DAFC, // ap_PowerArmor_HeadMod
    0x0005_5F8C, // ap_PowerArmor_BodyMod
    0x0005_5F8D, // ap_PowerArmor_LArmMod
    0x0005_5F8E, // ap_PowerArmor_RArmMod
    0x0005_5F8F, // ap_PowerArmor_RLegMod
    0x0005_5F90, // ap_PowerArmor_LLegMod
];

/// `ap_PowerArmor_Paint` — FO4's paint attach point, displayed as "Material".
pub(super) const FO4_AP_POWER_ARMOR_PAINT_OBJECT_ID: u32 = 0x0017_DAA8;

/// Attach points that hold a weapon's STRUCTURAL geometry — the receiver and
/// everything that hangs off it.
///
/// FO76 puts a material swap on these (the Enclave tint lives on the receiver, not
/// on a separate paint mod), so the material-swap heuristic in
/// `strip_material_omod_models` would delete the mesh from the parts the weapon is
/// built out of. `WEAP.MODL` for these guns is a geometry-free dummy receiver
/// (vanilla `PlasmaGun` uses the same dummy and gets its body from the receiver
/// OMOD), so losing the receiver model leaves nothing to draw AND nothing for the
/// barrel/grip/mag/scope to attach into: the whole assembly renders empty.
/// Measured 2026-09-01: 141 WEAP structural OMODs hit this, against 3 that
/// actually sit on `ap_WeaponMaterial`.
///
/// A denylist of structural slots rather than an allowlist of paint slots: the
/// power-armor paint-doubling case the strip targets keys on FO76's per-part
/// attach points, which are still un-retargeted here
/// (`retarget_power_armor_paint_attach_point` runs later in `pre_translate`).
/// Every non-weapon OMOD is unaffected.
pub(super) const FO4_STRUCTURAL_MOD_ATTACH_POINT_OBJECT_IDS: [u32; 13] = [
    0x0002_4004, // ap_gun_receiver
    0x0002_249C, // ap_gun_Muzzle
    0x0002_249D, // ap_gun_Barrel
    0x0002_249F, // ap_gun_Grip
    0x0002_2499, // ap_gun_Scope
    0x0005_D4D7, // ap_gun_Mag
    0x0014_9CA8, // ap_gun_Casing
    0x0014_CDA5, // ap_gun_Sight
    0x001E_9904, // ap_gun_FlashFar
    0x001E_9905, // ap_gun_FlashMid
    0x001E_9906, // ap_gun_FlashShort
    0x001E_8CAD, // ap_gun_TankMove
    0x0005_524C, // ap_melee_MeleeMod
];

/// FO76 OMOD `MNAM` (Target OMOD Keywords) entries with no useful FO4 role,
/// dropped entirely during translation. Values are SeventySix.esm object-ids
/// (master byte already stripped on decode). Material OMOD associations need a
/// forced FO4 keyword substitution instead because this filter preserves them.
pub(super) const FO76_REDUNDANT_OMOD_TARGET_KEYWORD_OBJECT_IDS: &[u32] = &[
    0x0037_D0B2, // ma_Gun_Appearance (ModAssociation)
];

pub(super) fn strip_record_obts_properties(
    interner: &crate::sym::StringInterner,
    record: &mut Record,
    target: ObjectModPropertyTarget,
) {
    for entry in &mut record.fields {
        if entry.sig.0 == *b"OBTS" {
            strip_object_template_property_rows(interner, &mut entry.value, target);
        }
    }
}

/// Remove every entry from a decoded FormKey-array subrecord value whose
/// object-id (lower 24 bits) is in `drop_object_ids`. Handles the decoded
/// `List<FormKey>` form (the FormKey path also checks `source_plugin` so a FO4
/// master ref with a colliding local is never dropped) and the raw `Bytes`
/// formid-array fallback (matched on the lower 24 bits only).
pub(super) fn filter_formkey_array_value(
    value: &mut FieldValue,
    source_plugin: crate::sym::Sym,
    drop_object_ids: &[u32],
) {
    match value {
        FieldValue::List(items) => {
            items.retain(|item| !formkey_array_item_matches(item, source_plugin, drop_object_ids))
        }
        FieldValue::Bytes(bytes) => {
            let mut out = Vec::with_capacity(bytes.len());
            for chunk in bytes.chunks(4) {
                if let [a, b, c, d] = chunk {
                    let raw = u32::from_le_bytes([*a, *b, *c, *d]);
                    if drop_object_ids.contains(&(raw & 0x00FF_FFFF)) {
                        continue;
                    }
                }
                out.extend_from_slice(chunk);
            }
            *bytes = smallvec::SmallVec::from_vec(out);
        }
        _ => {}
    }
}

pub(super) fn formkey_array_item_matches(
    value: &FieldValue,
    source_plugin: crate::sym::Sym,
    drop_object_ids: &[u32],
) -> bool {
    match value {
        FieldValue::FormKey(fk) => {
            fk.plugin == source_plugin && drop_object_ids.contains(&(fk.local & 0x00FF_FFFF))
        }
        FieldValue::Uint(raw) => drop_object_ids.contains(&((*raw as u32) & 0x00FF_FFFF)),
        FieldValue::Int(raw) if *raw >= 0 => {
            drop_object_ids.contains(&((*raw as u32) & 0x00FF_FFFF))
        }
        FieldValue::Bytes(bytes) if bytes.len() == 4 => drop_object_ids.contains(
            &(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) & 0x00FF_FFFF),
        ),
        _ => false,
    }
}

pub(super) fn formkey_array_value_is_empty(value: &FieldValue) -> bool {
    match value {
        FieldValue::List(items) => items.is_empty(),
        FieldValue::Bytes(bytes) => bytes.is_empty(),
        _ => false,
    }
}

pub(super) fn dedupe_formkey_array_value(value: &mut FieldValue) -> bool {
    match value {
        FieldValue::List(items) => {
            let before = items.len();
            let mut unique = Vec::with_capacity(before);
            for item in std::mem::take(items) {
                if !unique.contains(&item) {
                    unique.push(item);
                }
            }
            *items = unique;
            items.len() != before
        }
        FieldValue::Bytes(bytes) => {
            let before = bytes.len();
            let chunks = bytes.chunks_exact(4);
            let remainder = chunks.remainder().to_vec();
            let mut seen = Vec::with_capacity(before / 4);
            let mut unique = Vec::with_capacity(before);
            for chunk in chunks {
                let raw = u32::from_le_bytes(chunk.try_into().unwrap());
                if !seen.contains(&raw) {
                    seen.push(raw);
                    unique.extend_from_slice(chunk);
                }
            }
            unique.extend_from_slice(&remainder);
            *bytes = smallvec::SmallVec::from_vec(unique);
            bytes.len() != before
        }
        _ => false,
    }
}

/// Whether `raw` is one of the six FO76 per-part power-armor attach points.
///
/// Matched on the object-id alone, ignoring the plugin: FO76 keeps its own
/// object-ids on output, and both games use these ids for these keywords, so
/// `SeventySix.esm:055F8C` and `Fallout4.esm:055F8C` are the same keyword.
pub(super) fn is_fo76_power_armor_part_attach_point(raw: u32) -> bool {
    FO76_POWER_ARMOR_PART_ATTACH_POINT_OBJECT_IDS.contains(&(raw & 0x00FF_FFFF))
}

pub(super) fn is_power_armor_part_attach_point_value(value: &FieldValue) -> bool {
    let raw = match value {
        FieldValue::FormKey(fk) => fk.local,
        FieldValue::Uint(raw) => *raw as u32,
        FieldValue::Int(raw) if *raw >= 0 => *raw as u32,
        FieldValue::Bytes(bytes) if bytes.len() == 4 => {
            u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
        }
        _ => return false,
    };
    is_fo76_power_armor_part_attach_point(raw)
}

pub(super) fn formkey_array_value_len(value: &FieldValue) -> Option<usize> {
    match value {
        FieldValue::List(items) => Some(items.len()),
        FieldValue::Bytes(bytes) => Some(bytes.len() / 4),
        _ => None,
    }
}

/// Read the OMOD's attach point out of `DATA` (offset 16), handling both the raw
/// byte layout and the decoded struct shape the schema sometimes yields.
pub(super) fn omod_attach_point_raw(
    interner: &crate::sym::StringInterner,
    record: &Record,
) -> Option<u32> {
    record.fields.iter().find_map(|entry| {
        if entry.sig.0 != *b"DATA" {
            return None;
        }
        match &entry.value {
            FieldValue::Bytes(bytes) if bytes.len() >= OMOD_DATA_HEADER_LEN => {
                read_u32_le_at(bytes, OMOD_DATA_ATTACH_POINT_OFFSET)
            }
            FieldValue::Struct(fields) => {
                let index = field_index_canonical(fields, "attach_point", interner)?;
                match &fields[index].1 {
                    FieldValue::FormKey(fk) => Some(fk.local),
                    FieldValue::Uint(raw) => Some(*raw as u32),
                    FieldValue::Int(raw) if *raw >= 0 => Some(*raw as u32),
                    FieldValue::Bytes(bytes) if bytes.len() == 4 => {
                        Some(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
                    }
                    _ => None,
                }
            }
            _ => None,
        }
    })
}

/// Matched on the object-id alone, ignoring the plugin: FO76 keeps its own
/// object-ids on output and both games use these ids for these keywords.
pub(super) fn omod_attaches_at_structural_point(
    interner: &crate::sym::StringInterner,
    record: &Record,
) -> bool {
    omod_attach_point_raw(interner, record).is_some_and(|raw| {
        FO4_STRUCTURAL_MOD_ATTACH_POINT_OBJECT_IDS.contains(&(raw & 0x00FF_FFFF))
    })
}

pub(super) fn omod_has_power_armor_model(
    interner: &crate::sym::StringInterner,
    record: &Record,
) -> bool {
    record.fields.iter().any(|entry| {
        if entry.sig.0 != *b"MODL" {
            return false;
        }
        let path = match &entry.value {
            FieldValue::String(path) => interner.resolve(*path),
            FieldValue::Bytes(bytes) => std::str::from_utf8(bytes).ok(),
            _ => None,
        };
        path.is_some_and(|path| {
            path.trim_end_matches('\0')
                .replace('\\', "/")
                .to_ascii_lowercase()
                .starts_with(POWER_ARMOR_MODEL_PREFIX)
        })
    })
}

pub(super) fn strip_omod_data_properties(
    interner: &crate::sym::StringInterner,
    record: &mut Record,
) {
    for entry in &mut record.fields {
        if entry.sig.0 != *b"DATA" {
            continue;
        }
        let target = omod_property_target(&entry.value, interner);
        strip_omod_data_property_rows(interner, &mut entry.value, target);
    }
}

pub(super) fn omod_data_form_type(
    value: &FieldValue,
    interner: &crate::sym::StringInterner,
) -> Option<u32> {
    match value {
        FieldValue::Struct(fields) => {
            named_value_canonical(fields, "formtype", interner).and_then(field_value_to_u32)
        }
        FieldValue::Bytes(bytes) => read_u32_le_at(bytes, OMOD_DATA_FORM_TYPE_OFFSET),
        _ => None,
    }
}

pub(super) fn omod_property_target(
    value: &FieldValue,
    interner: &crate::sym::StringInterner,
) -> ObjectModPropertyTarget {
    let form_type = omod_data_form_type(value, interner);
    let Some(form_type) = form_type else {
        return ObjectModPropertyTarget::Object;
    };
    match &form_type.to_le_bytes() {
        b"WEAP" => ObjectModPropertyTarget::Weapon,
        b"ARMO" | b"ARMA" => ObjectModPropertyTarget::Armor,
        b"NPC_" => ObjectModPropertyTarget::Actor,
        _ => ObjectModPropertyTarget::Object,
    }
}

pub(super) fn omod_has_material_swap_data(
    interner: &crate::sym::StringInterner,
    record: &Record,
) -> bool {
    if record.fields.iter().any(|entry| entry.sig.0 == *b"MODS") {
        return true;
    }

    record
        .fields
        .iter()
        .filter(|entry| entry.sig.0 == *b"DATA")
        .any(|entry| {
            let target = omod_property_target(&entry.value, interner);
            material_swap_property_id(target).is_some_and(|property_id| {
                omod_data_has_property_id(&entry.value, interner, property_id)
            })
        })
}

pub(super) fn material_swap_property_id(target: ObjectModPropertyTarget) -> Option<u16> {
    match target {
        ObjectModPropertyTarget::Weapon => Some(89),
        ObjectModPropertyTarget::Armor => Some(13),
        ObjectModPropertyTarget::Actor => Some(5),
        ObjectModPropertyTarget::Object => None,
    }
}

pub(super) fn omod_data_has_property_id(
    value: &FieldValue,
    interner: &crate::sym::StringInterner,
    property_id: u16,
) -> bool {
    match value {
        FieldValue::Struct(fields) => {
            let Some(FieldValue::List(properties)) =
                named_value_canonical(fields, "properties", interner)
            else {
                return false;
            };
            properties.iter().any(|property| {
                property_id_from_row(property, interner).is_some_and(|id| id == property_id)
            })
        }
        FieldValue::Bytes(bytes) => raw_omod_data_has_property_id(bytes, property_id),
        _ => false,
    }
}

pub(super) fn raw_omod_data_has_property_id(bytes: &[u8], property_id: u16) -> bool {
    if bytes.len() < OMOD_DATA_HEADER_LEN + OMOD_DATA_ATTACH_PARENT_SLOT_COUNT_LEN {
        return false;
    }
    let Some(include_count) = read_u32_le_at(bytes, 0).map(|count| count as usize) else {
        return false;
    };
    let Some(property_count) =
        read_u32_le_at(bytes, OBJECT_TEMPLATE_PROPERTY_COUNT_OFFSET).map(|count| count as usize)
    else {
        return false;
    };
    let Some(includes_len) = include_count.checked_mul(OMOD_DATA_INCLUDE_ROW_LEN) else {
        return false;
    };
    let Some(attach_parent_slot_count) =
        read_u32_le_at(bytes, OMOD_DATA_HEADER_LEN).map(|count| count as usize)
    else {
        return false;
    };
    let Some(attach_parent_slots_len) = attach_parent_slot_count.checked_mul(4) else {
        return false;
    };
    let slots_start = OMOD_DATA_HEADER_LEN + OMOD_DATA_ATTACH_PARENT_SLOT_COUNT_LEN;
    let Some(item_start) = slots_start.checked_add(attach_parent_slots_len) else {
        return false;
    };
    let Some(includes_start) = item_start.checked_add(OMOD_DATA_ITEM_ROW_LEN) else {
        return false;
    };
    let Some(properties_start) = includes_start.checked_add(includes_len) else {
        return false;
    };

    (0..property_count).any(|index| {
        let row_start = properties_start + index * OBJECT_MOD_PROPERTY_ROW_LEN;
        let Some(property_bytes) = bytes.get(
            row_start + OBJECT_MOD_PROPERTY_ID_OFFSET
                ..row_start + OBJECT_MOD_PROPERTY_ID_OFFSET + 2,
        ) else {
            return false;
        };
        u16::from_le_bytes([property_bytes[0], property_bytes[1]]) == property_id
    })
}

pub(super) fn strip_object_template_property_rows(
    interner: &crate::sym::StringInterner,
    value: &mut FieldValue,
    target: ObjectModPropertyTarget,
) -> u32 {
    match value {
        FieldValue::Struct(_) => strip_struct_property_rows(interner, value, target),
        FieldValue::Bytes(bytes) => strip_raw_object_template_property_rows(bytes, target),
        _ => 0,
    }
}

pub(super) fn strip_omod_data_property_rows(
    interner: &crate::sym::StringInterner,
    value: &mut FieldValue,
    target: ObjectModPropertyTarget,
) -> u32 {
    match value {
        FieldValue::Struct(_) => strip_struct_property_rows(interner, value, target),
        FieldValue::Bytes(bytes) => strip_raw_omod_data_property_rows(bytes, target),
        _ => 0,
    }
}

pub(super) fn set_omod_data_property_function_type(
    interner: &crate::sym::StringInterner,
    value: &mut FieldValue,
    property_id: u16,
    function_type: u8,
) -> u32 {
    match value {
        FieldValue::Struct(_) => {
            set_struct_property_function_type(interner, value, property_id, function_type)
        }
        FieldValue::Bytes(bytes) => {
            set_raw_omod_data_property_function_type(bytes, property_id, function_type)
        }
        _ => 0,
    }
}

pub(super) fn set_struct_property_function_type(
    interner: &crate::sym::StringInterner,
    value: &mut FieldValue,
    property_id: u16,
    function_type: u8,
) -> u32 {
    let FieldValue::Struct(fields) = value else {
        return 0;
    };
    let Some(properties_index) = field_index_canonical(fields, "properties", interner) else {
        return 0;
    };
    let FieldValue::List(properties) = &mut fields[properties_index].1 else {
        return 0;
    };

    let mut changed = 0_u32;
    for property in properties {
        if property_id_from_row(property, interner).is_some_and(|id| id == property_id)
            && set_property_row_function_type(interner, property, function_type)
        {
            changed += 1;
        }
    }
    changed
}

pub(super) fn set_property_row_function_type(
    interner: &crate::sym::StringInterner,
    value: &mut FieldValue,
    function_type: u8,
) -> bool {
    match value {
        FieldValue::Struct(fields) => {
            if let Some(index) = field_index_canonical(fields, "functiontype", interner) {
                if field_value_to_u16(&fields[index].1) == Some(function_type as u16) {
                    return false;
                }
                set_u32_count(&mut fields[index].1, function_type as u32);
                return true;
            }
            fields.push((
                interner.intern("FunctionType"),
                FieldValue::Uint(function_type as u64),
            ));
            true
        }
        FieldValue::Bytes(bytes) if bytes.len() > OBJECT_MOD_PROPERTY_FUNCTION_TYPE_OFFSET => {
            if bytes[OBJECT_MOD_PROPERTY_FUNCTION_TYPE_OFFSET] == function_type {
                return false;
            }
            bytes[OBJECT_MOD_PROPERTY_FUNCTION_TYPE_OFFSET] = function_type;
            true
        }
        _ => false,
    }
}

pub(super) fn set_raw_omod_data_property_function_type(
    bytes: &mut smallvec::SmallVec<[u8; 32]>,
    property_id: u16,
    function_type: u8,
) -> u32 {
    if bytes.len() < OMOD_DATA_HEADER_LEN + OMOD_DATA_ATTACH_PARENT_SLOT_COUNT_LEN {
        return 0;
    }
    let Some(include_count) = read_u32_le_at(bytes, 0).map(|count| count as usize) else {
        return 0;
    };
    let Some(property_count) =
        read_u32_le_at(bytes, OBJECT_TEMPLATE_PROPERTY_COUNT_OFFSET).map(|count| count as usize)
    else {
        return 0;
    };
    let Some(includes_len) = include_count.checked_mul(OMOD_DATA_INCLUDE_ROW_LEN) else {
        return 0;
    };
    let Some(attach_parent_slot_count) =
        read_u32_le_at(bytes, OMOD_DATA_HEADER_LEN).map(|count| count as usize)
    else {
        return 0;
    };
    let Some(attach_parent_slots_len) = attach_parent_slot_count.checked_mul(4) else {
        return 0;
    };
    let slots_start = OMOD_DATA_HEADER_LEN + OMOD_DATA_ATTACH_PARENT_SLOT_COUNT_LEN;
    let Some(item_start) = slots_start.checked_add(attach_parent_slots_len) else {
        return 0;
    };
    let Some(includes_start) = item_start.checked_add(OMOD_DATA_ITEM_ROW_LEN) else {
        return 0;
    };
    let Some(properties_start) = includes_start.checked_add(includes_len) else {
        return 0;
    };
    let Some(properties_len) = property_count.checked_mul(OBJECT_MOD_PROPERTY_ROW_LEN) else {
        return 0;
    };
    let Some(properties_end) = properties_start.checked_add(properties_len) else {
        return 0;
    };
    if properties_end > bytes.len() {
        return 0;
    }

    let mut changed = 0_u32;
    for index in 0..property_count {
        let row_start = properties_start + index * OBJECT_MOD_PROPERTY_ROW_LEN;
        let Some(property_bytes) = bytes.get(
            row_start + OBJECT_MOD_PROPERTY_ID_OFFSET
                ..row_start + OBJECT_MOD_PROPERTY_ID_OFFSET + 2,
        ) else {
            continue;
        };
        if u16::from_le_bytes([property_bytes[0], property_bytes[1]]) != property_id {
            continue;
        }
        let function_type_offset = row_start + OBJECT_MOD_PROPERTY_FUNCTION_TYPE_OFFSET;
        if bytes[function_type_offset] != function_type {
            bytes[function_type_offset] = function_type;
            changed += 1;
        }
    }
    changed
}

pub(super) fn strip_struct_property_rows(
    interner: &crate::sym::StringInterner,
    value: &mut FieldValue,
    target: ObjectModPropertyTarget,
) -> u32 {
    let FieldValue::Struct(fields) = value else {
        return 0;
    };
    let Some(properties_index) = field_index_canonical(fields, "properties", interner) else {
        return 0;
    };

    let Some((removed, kept_count)) =
        strip_properties_value(interner, &mut fields[properties_index].1, target)
    else {
        return 0;
    };
    if removed == 0 {
        return 0;
    }

    if let Some(count_index) = field_index_canonical(fields, "propertycount", interner) {
        set_u32_count(&mut fields[count_index].1, kept_count as u32);
    }
    removed
}

pub(super) fn strip_raw_object_template_property_rows(
    bytes: &mut smallvec::SmallVec<[u8; 32]>,
    target: ObjectModPropertyTarget,
) -> u32 {
    if bytes.len() < OBJECT_TEMPLATE_FIXED_HEADER_LEN + 2 {
        return 0;
    }
    let Some(include_count) = read_u32_le_at(bytes, 0).map(|count| count as usize) else {
        return 0;
    };
    let Some(property_count) =
        read_u32_le_at(bytes, OBJECT_TEMPLATE_PROPERTY_COUNT_OFFSET).map(|count| count as usize)
    else {
        return 0;
    };
    let keyword_count = bytes[OBJECT_TEMPLATE_KEYWORD_COUNT_OFFSET] as usize;
    let Some(properties_start) = keyword_count
        .checked_mul(4)
        .and_then(|len| OBJECT_TEMPLATE_FIXED_HEADER_LEN.checked_add(len))
        .and_then(|offset| offset.checked_add(2))
        .and_then(|offset| {
            include_count
                .checked_mul(OBJECT_TEMPLATE_INCLUDE_ROW_LEN)
                .and_then(|len| offset.checked_add(len))
        })
    else {
        return 0;
    };

    strip_raw_object_mod_property_rows(
        bytes,
        OBJECT_TEMPLATE_PROPERTY_COUNT_OFFSET,
        properties_start,
        property_count,
        target,
    )
}

pub(super) fn strip_raw_omod_data_property_rows(
    bytes: &mut smallvec::SmallVec<[u8; 32]>,
    target: ObjectModPropertyTarget,
) -> u32 {
    if bytes.len() < OMOD_DATA_HEADER_LEN + OMOD_DATA_ATTACH_PARENT_SLOT_COUNT_LEN {
        return 0;
    }
    let Some(include_count) = read_u32_le_at(bytes, 0).map(|count| count as usize) else {
        return 0;
    };
    let Some(property_count) =
        read_u32_le_at(bytes, OBJECT_TEMPLATE_PROPERTY_COUNT_OFFSET).map(|count| count as usize)
    else {
        return 0;
    };
    let Some(includes_len) = include_count.checked_mul(OMOD_DATA_INCLUDE_ROW_LEN) else {
        return 0;
    };
    let Some(properties_len) = property_count.checked_mul(OBJECT_MOD_PROPERTY_ROW_LEN) else {
        return 0;
    };
    let Some(attach_parent_slot_count) =
        read_u32_le_at(bytes, OMOD_DATA_HEADER_LEN).map(|count| count as usize)
    else {
        return 0;
    };
    let Some(attach_parent_slots_len) = attach_parent_slot_count.checked_mul(4) else {
        return 0;
    };
    let slots_start = OMOD_DATA_HEADER_LEN + OMOD_DATA_ATTACH_PARENT_SLOT_COUNT_LEN;
    let Some(item_start) = slots_start.checked_add(attach_parent_slots_len) else {
        return 0;
    };
    let Some(includes_start) = item_start.checked_add(OMOD_DATA_ITEM_ROW_LEN) else {
        return 0;
    };
    let Some(properties_start) = includes_start.checked_add(includes_len) else {
        return 0;
    };
    let Some(properties_end) = properties_start.checked_add(properties_len) else {
        return 0;
    };
    if properties_end > bytes.len() {
        return 0;
    }

    strip_raw_object_mod_property_rows(
        bytes,
        OBJECT_TEMPLATE_PROPERTY_COUNT_OFFSET,
        properties_start,
        property_count,
        target,
    )
}

pub(super) fn strip_raw_object_mod_property_rows(
    bytes: &mut smallvec::SmallVec<[u8; 32]>,
    property_count_offset: usize,
    properties_start: usize,
    property_count: usize,
    target: ObjectModPropertyTarget,
) -> u32 {
    let Some(properties_len) = property_count.checked_mul(OBJECT_MOD_PROPERTY_ROW_LEN) else {
        return 0;
    };
    let Some(properties_end) = properties_start.checked_add(properties_len) else {
        return 0;
    };
    if properties_end > bytes.len() {
        return 0;
    }

    let mut kept = Vec::with_capacity(properties_len);
    let mut kept_count = 0_usize;
    for index in 0..property_count {
        let row_start = properties_start + index * OBJECT_MOD_PROPERTY_ROW_LEN;
        let row_end = row_start + OBJECT_MOD_PROPERTY_ROW_LEN;
        let row = &bytes[row_start..row_end];
        let property_id = u16::from_le_bytes([
            row[OBJECT_MOD_PROPERTY_ID_OFFSET],
            row[OBJECT_MOD_PROPERTY_ID_OFFSET + 1],
        ]);
        if valid_object_mod_property(target, property_id) {
            kept.extend_from_slice(row);
            kept_count += 1;
        }
    }

    if kept_count == property_count {
        return 0;
    }
    let suffix = bytes[properties_end..].to_vec();
    bytes.truncate(properties_start);
    bytes.extend_from_slice(&kept);
    bytes.extend_from_slice(&suffix);
    set_u32_le_at(bytes, property_count_offset, kept_count as u32);
    (property_count - kept_count) as u32
}

pub(super) fn strip_properties_value(
    interner: &crate::sym::StringInterner,
    value: &mut FieldValue,
    target: ObjectModPropertyTarget,
) -> Option<(u32, usize)> {
    let FieldValue::List(properties) = value else {
        return None;
    };
    let before = properties.len();
    properties.retain(|property| {
        property_id_from_row(property, interner)
            .is_some_and(|property_id| valid_object_mod_property(target, property_id))
    });
    Some(((before - properties.len()) as u32, properties.len()))
}

pub(super) fn property_id_from_row(
    value: &FieldValue,
    interner: &crate::sym::StringInterner,
) -> Option<u16> {
    match value {
        FieldValue::Struct(fields) => {
            named_value_canonical(fields, "property", interner).and_then(field_value_to_u16)
        }
        FieldValue::Bytes(bytes) if bytes.len() >= 10 => Some(u16::from_le_bytes([
            bytes[OBJECT_MOD_PROPERTY_ID_OFFSET],
            bytes[OBJECT_MOD_PROPERTY_ID_OFFSET + 1],
        ])),
        _ => None,
    }
}

pub(super) fn valid_object_mod_property(target: ObjectModPropertyTarget, property_id: u16) -> bool {
    match target {
        ObjectModPropertyTarget::Weapon => property_id <= 94,
        ObjectModPropertyTarget::Armor => property_id <= 13,
        ObjectModPropertyTarget::Actor => property_id <= 5,
        ObjectModPropertyTarget::Object => false,
    }
}
impl Fo76Fo4Hook {
    pub(super) fn dedupe_mapped_keyword_arrays(record: &mut Record) {
        let (array_sig, sync_keyword_count) = match &record.sig.0 {
            b"OMOD" => (*b"MNAM", false),
            b"ARMO" => (*b"KWDA", true),
            _ => return,
        };

        let mut changed = false;
        for entry in record
            .fields
            .iter_mut()
            .filter(|entry| entry.sig.0 == array_sig)
        {
            changed |= dedupe_formkey_array_value(&mut entry.value);
        }
        if !changed || !sync_keyword_count {
            return;
        }

        let keyword_count = record
            .fields
            .iter()
            .filter(|entry| entry.sig.0 == array_sig)
            .filter_map(|entry| formkey_array_value_len(&entry.value))
            .sum::<usize>()
            .try_into()
            .unwrap_or(u32::MAX);
        if let Some(entry) = record
            .fields
            .iter_mut()
            .find(|entry| entry.sig.0 == *b"KSIZ")
        {
            crate::record::write_u32_field(&mut entry.value, keyword_count);
        }
    }

    pub(super) fn strip_invalid_object_mod_properties(
        interner: &crate::sym::StringInterner,
        record: &mut Record,
    ) {
        match &record.sig.0 {
            b"WEAP" => {
                strip_record_obts_properties(interner, record, ObjectModPropertyTarget::Weapon)
            }
            b"ARMO" => {
                strip_record_obts_properties(interner, record, ObjectModPropertyTarget::Armor)
            }
            b"NPC_" => {
                strip_record_obts_properties(interner, record, ObjectModPropertyTarget::Actor)
            }
            b"OMOD" => strip_omod_data_properties(interner, record),
            _ => {}
        }
    }

    /// Drop FO76-only mod-association keywords from OMOD `MNAM` (Target OMOD
    /// Keywords) that have no FO4 equivalent. Runs in `pre_translate` so the
    /// FO76 keyword never reaches the mapper. If a filter empties an `MNAM`
    /// array the now-empty subrecord is removed entirely.
    pub(super) fn strip_redundant_omod_target_keywords(
        interner: &crate::sym::StringInterner,
        record: &mut Record,
    ) {
        if record.sig.0 != *b"OMOD" || omod_has_material_swap_data(interner, record) {
            return;
        }
        let source_plugin = interner.intern(FO76_MASTER_NAME);
        for entry in &mut record.fields {
            if entry.sig.0 != *b"MNAM" {
                continue;
            }
            filter_formkey_array_value(
                &mut entry.value,
                source_plugin,
                FO76_REDUNDANT_OMOD_TARGET_KEYWORD_OBJECT_IDS,
            );
        }
        record
            .fields
            .retain(|entry| entry.sig.0 != *b"MNAM" || !formkey_array_value_is_empty(&entry.value));
    }

    pub(super) fn strip_material_omod_models(
        interner: &crate::sym::StringInterner,
        record: &mut Record,
    ) {
        if record.sig.0 != *b"OMOD"
            || !omod_has_material_swap_data(interner, record)
            || omod_has_power_armor_model(interner, record)
            || omod_attaches_at_structural_point(interner, record)
        {
            return;
        }

        record
            .fields
            .retain(|entry| !matches!(&entry.sig.0, b"MODL" | b"MODB" | b"MODT" | b"MODF"));
    }

    pub(super) fn strip_tesla_cannon_receiver_model(
        interner: &crate::sym::StringInterner,
        record: &mut Record,
    ) {
        if record.sig.0 != *b"OMOD"
            || !record.fields.iter().any(|entry| {
                entry.sig.0 == *b"INDX"
                    && match &entry.value {
                        FieldValue::Uint(index) => *index == 0,
                        FieldValue::Int(index) => *index == 0,
                        FieldValue::Bytes(bytes) => bytes.as_slice() == [0],
                        _ => false,
                    }
            })
        {
            return;
        }

        let uses_base_model = record.fields.iter().any(|entry| {
            if entry.sig.0 != *b"MODL" {
                return false;
            }
            let path = match &entry.value {
                FieldValue::String(path) => interner.resolve(*path),
                FieldValue::Bytes(bytes) => std::str::from_utf8(bytes).ok(),
                _ => None,
            };
            path.is_some_and(|path| {
                path.trim_end_matches('\0')
                    .replace('\\', "/")
                    .eq_ignore_ascii_case(TESLA_CANNON_BASE_MODEL)
            })
        });
        if !uses_base_model {
            return;
        }

        // FO4 has no OMOD INDX semantics and otherwise attaches a second copy
        // of the full weapon body instead of reusing the base model.
        record.fields.retain(|entry| {
            !matches!(
                &entry.sig.0,
                b"MODL"
                    | b"MODB"
                    | b"MODT"
                    | b"MODS"
                    | b"MODF"
                    | b"MODD"
                    | b"XFLG"
                    | b"ENLT"
                    | b"ENLS"
                    | b"AUUV"
                    | b"INDX"
            )
        });
    }

    pub(super) fn normalize_omod_material_swap_functions(
        interner: &crate::sym::StringInterner,
        record: &mut Record,
    ) {
        if record.sig.0 != *b"OMOD" {
            return;
        }

        for entry in &mut record.fields {
            if entry.sig.0 != *b"DATA" {
                continue;
            }
            let target = omod_property_target(&entry.value, interner);
            let Some(property_id) = material_swap_property_id(target) else {
                continue;
            };
            set_omod_data_property_function_type(interner, &mut entry.value, property_id, 2);
        }
    }

    /// Move a power-armor paint OMOD onto FO4's paint attach point.
    ///
    /// FO76 hangs the paint off the piece's per-part root
    /// (`ap_PowerArmor_HeadMod` and friends, each displayed as "Paint"). FO4
    /// uses those same EditorIDs for unnamed structural roots and puts paints
    /// on `ap_PowerArmor_Paint` ("Material"), which the chassis OMOD grants.
    /// Because the EditorIDs match, the remap maps the FO76 slot onto FO4's
    /// root, so every paint lands on a slot the workbench renders no category
    /// for and no power armor can be repainted. `expose_power_armor_paint_slot`
    /// does the matching `ARMO` side; the paint keeps its own
    /// `AttachParentSlots`, so lining/misc/headlamp stay reachable through it.
    pub(super) fn retarget_power_armor_paint_attach_point(
        interner: &crate::sym::StringInterner,
        record: &mut Record,
    ) {
        if record.sig.0 != *b"OMOD" {
            return;
        }

        let paint_slot = FormKey {
            local: FO4_AP_POWER_ARMOR_PAINT_OBJECT_ID,
            plugin: interner.intern(FO4_MASTER_NAME),
        };
        for entry in &mut record.fields {
            if entry.sig.0 != *b"DATA" {
                continue;
            }
            match &mut entry.value {
                FieldValue::Bytes(bytes) if bytes.len() >= OMOD_DATA_HEADER_LEN => {
                    let raw = u32::from_le_bytes(
                        bytes[OMOD_DATA_ATTACH_POINT_OFFSET..OMOD_DATA_ATTACH_POINT_OFFSET + 4]
                            .try_into()
                            .unwrap(),
                    );
                    if is_fo76_power_armor_part_attach_point(raw) {
                        set_u32_le_at(
                            bytes,
                            OMOD_DATA_ATTACH_POINT_OFFSET,
                            (raw & 0xFF00_0000) | FO4_AP_POWER_ARMOR_PAINT_OBJECT_ID,
                        );
                    }
                }
                FieldValue::Struct(fields) => {
                    let Some(index) = field_index_canonical(fields, "attach_point", interner)
                    else {
                        continue;
                    };
                    if is_power_armor_part_attach_point_value(&fields[index].1) {
                        fields[index].1 = FieldValue::FormKey(paint_slot);
                    }
                }
                _ => {}
            }
        }
    }

    /// Expose `ap_PowerArmor_Paint` on a power-armor piece.
    ///
    /// The `ARMO` half of `retarget_power_armor_paint_attach_point`: the piece
    /// advertises exactly one per-part slot, which that hook has moved the
    /// paints off, so it must advertise FO4's paint slot instead.
    pub(super) fn expose_power_armor_paint_slot(
        interner: &crate::sym::StringInterner,
        record: &mut Record,
    ) {
        if record.sig.0 != *b"ARMO" {
            return;
        }

        let paint_slot = FormKey {
            local: FO4_AP_POWER_ARMOR_PAINT_OBJECT_ID,
            plugin: interner.intern(FO4_MASTER_NAME),
        };
        for entry in &mut record.fields {
            if entry.sig.0 != *b"APPR" {
                continue;
            }
            match &mut entry.value {
                FieldValue::List(items) => {
                    for item in items.iter_mut() {
                        if is_power_armor_part_attach_point_value(item) {
                            *item = FieldValue::FormKey(paint_slot);
                        }
                    }
                }
                FieldValue::Bytes(bytes) => {
                    for chunk in bytes.chunks_exact_mut(4) {
                        let raw = u32::from_le_bytes(chunk.try_into().unwrap());
                        if is_fo76_power_armor_part_attach_point(raw) {
                            chunk.copy_from_slice(
                                &((raw & 0xFF00_0000) | FO4_AP_POWER_ARMOR_PAINT_OBJECT_ID)
                                    .to_le_bytes(),
                            );
                        }
                    }
                }
                _ => {}
            }
            // `APPR` carries no paired count subrecord, so collapsing a piece
            // that advertised more than one part slot needs no count fixup.
            dedupe_formkey_array_value(&mut entry.value);
        }
    }

    pub(super) fn repair_liberator_body_omod_attach_point(
        interner: &crate::sym::StringInterner,
        record: &mut Record,
    ) {
        if record.sig.0 != *b"OMOD"
            || !record
                .eid
                .and_then(|eid| interner.resolve(eid))
                .is_some_and(|eid| eid.eq_ignore_ascii_case(LIBERATOR_BODY_ARMOR_OMOD_EDITOR_ID))
        {
            return;
        }

        let attach_point = FormKey {
            local: FO4_AP_BOT_ARMOR_SLOT1_OBJECT_ID,
            plugin: interner.intern(FO4_MASTER_NAME),
        };
        for entry in &mut record.fields {
            if entry.sig.0 != *b"DATA" {
                continue;
            }
            match &mut entry.value {
                FieldValue::Bytes(bytes) if bytes.len() >= OMOD_DATA_HEADER_LEN => {
                    set_u32_le_at(
                        bytes,
                        OMOD_DATA_ATTACH_POINT_OFFSET,
                        FO4_AP_BOT_ARMOR_SLOT1_OBJECT_ID,
                    );
                }
                FieldValue::Struct(fields) => {
                    if let Some(index) = field_index_canonical(fields, "attach_point", interner) {
                        fields[index].1 = FieldValue::FormKey(attach_point);
                    } else {
                        fields.push((
                            interner.intern("attach_point"),
                            FieldValue::FormKey(attach_point),
                        ));
                    }
                }
                _ => {}
            }
        }
    }

    /// Expose `ap_Bot_ArmorSlot1` on `LiberatorRace` so the shell OMOD can bind.
    ///
    /// FO4 only attaches a model-bearing OMOD when its attach point is in the
    /// actor's reachable slot set — the RACE's `APPR` plus the
    /// `AttachParentSlots` of already-attached parent mods. FO76 attached the
    /// Liberator shell with a NULL attach point, so
    /// `repair_liberator_body_omod_attach_point` gives it
    /// `ap_Bot_ArmorSlot1`; but in FO4 that keyword is exposed only by
    /// `Bot_TorsoHandy`, a Mister Handy chassis the Liberator never attaches,
    /// and the race carries just `ap_customName`. Without this the shell mod
    /// resolves against nothing and the robot renders bare.
    pub(super) fn ensure_liberator_race_armor_attach_slot(
        interner: &crate::sym::StringInterner,
        record: &mut Record,
    ) {
        if record.sig.0 != *b"RACE"
            || !record
                .eid
                .and_then(|eid| interner.resolve(eid))
                .is_some_and(|eid| eid.eq_ignore_ascii_case(LIBERATOR_RACE_EDITOR_ID))
        {
            return;
        }

        let slot = FormKey {
            local: FO4_AP_BOT_ARMOR_SLOT1_OBJECT_ID,
            plugin: interner.intern(FO4_MASTER_NAME),
        };
        for entry in &mut record.fields {
            if entry.sig.0 != *b"APPR" {
                continue;
            }
            match &mut entry.value {
                FieldValue::List(items) => {
                    if !items.contains(&FieldValue::FormKey(slot)) {
                        items.push(FieldValue::FormKey(slot));
                    }
                }
                FieldValue::Bytes(bytes) => {
                    if !bytes.chunks_exact(4).any(|chunk| {
                        u32::from_le_bytes(chunk.try_into().unwrap())
                            == FO4_AP_BOT_ARMOR_SLOT1_OBJECT_ID
                    }) {
                        bytes.extend_from_slice(&FO4_AP_BOT_ARMOR_SLOT1_OBJECT_ID.to_le_bytes());
                    }
                }
                _ => {}
            }
        }
    }

    pub(super) fn drop_mstt_omod_data(interner: &crate::sym::StringInterner, record: &mut Record) {
        if record.sig.0 != *b"OMOD" {
            return;
        }
        record.fields.retain(|entry| {
            entry.sig.0 != *b"DATA"
                || omod_data_form_type(&entry.value, interner)
                    .is_none_or(|form_type| form_type != FO76_MSTT_FORM_TYPE)
        });
    }
}
