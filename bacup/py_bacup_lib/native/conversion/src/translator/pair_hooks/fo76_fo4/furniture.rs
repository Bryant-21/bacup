use super::*;

pub(super) const FURNITURE_INTERACTION_POINT_BITS: u32 = 0x003F_FFFF;
/// FURN/TERM `MNAM` "Has Model" flag (bit 30). When set, Interaction Point 0 is
/// backed by the model's default furniture marker.
pub(super) const FURNITURE_HAS_MODEL_BIT: u32 = 0x4000_0000;
pub(super) const FURNITURE_MARKER_PARAMETERS_ROW_LEN: usize = 24;
pub(super) const FO4_POWER_ARMOR_FURNITURE_KEYWORD: u32 = 0x03430B;
pub(super) const FO4_POWER_ARMOR_FIRST_PERSON_KEYWORD: u32 = 0x0A56D7;
pub(super) const FO4_POWER_ARMOR_BATTERY_INSERT_ANIM_KEYWORD: u32 = 0x05BDA8;
pub(super) const FO4_PLAYER_PATH_TO_FURNITURE_KEYWORD: u32 = 0x06D5BB;
pub(super) const FO4_POWER_ARMOR_BATTERY_ITEM_KEYWORD: u32 = 0x05BDAA;
pub(super) const POWER_ARMOR_BATTERY_INSERT_SCRIPT: &str = "PowerArmorBatteryInsertScript";
/// `Workbench_General`, carried by every FO4 crafting bench.
pub(super) const FO4_WORKBENCH_GENERAL_KEYWORD: u32 = 0x091FD4;
pub(super) const FO4_WORKSHOP_ITEM_KEYWORD: u32 = 0x054BA6;
pub(super) const WORKBENCH_SCRIPT: &str = "WorkbenchScript";
pub(super) const FO4_VMAD_VERSION: u16 = 6;
pub(super) const FO4_VMAD_OBJECT_FORMAT: u16 = 2;
pub(super) const VMAD_PROPERTY_FLAG_EDITED: u8 = 1;

pub(super) fn fo4_keyword_value(
    value: &FieldValue,
    interner: &crate::sym::StringInterner,
    keyword: u32,
) -> bool {
    match value {
        FieldValue::FormKey(form_key) => {
            form_key.local == keyword
                && interner
                    .resolve(form_key.plugin)
                    .is_some_and(|plugin| plugin.eq_ignore_ascii_case(FO4_MASTER_NAME))
        }
        FieldValue::List(values) => values
            .iter()
            .any(|value| fo4_keyword_value(value, interner, keyword)),
        FieldValue::Struct(fields) => fields
            .iter()
            .any(|(_, value)| fo4_keyword_value(value, interner, keyword)),
        FieldValue::Bytes(bytes) => bytes.chunks_exact(4).any(|bytes| {
            u32::from_le_bytes(bytes.try_into().expect("four-byte FormID row")) & 0x00FF_FFFF
                == keyword
        }),
        _ => false,
    }
}

pub(super) fn power_armor_furniture_vmad_bytes() -> Vec<u8> {
    let masters = [FO4_MASTER_NAME.to_string()];
    let payload = serde_json::json!({
        "Version": FO4_VMAD_VERSION,
        "Object Format": FO4_VMAD_OBJECT_FORMAT,
        "Scripts": [{
            "ScriptName": POWER_ARMOR_BATTERY_INSERT_SCRIPT,
            "Properties": [
                fo4_vmad_object_property("firstPersonKW", FO4_POWER_ARMOR_FIRST_PERSON_KEYWORD),
                fo4_vmad_object_property("batteryInsertAnimKW", FO4_POWER_ARMOR_BATTERY_INSERT_ANIM_KEYWORD),
                fo4_vmad_object_property("PlayerPathToFurniture", FO4_PLAYER_PATH_TO_FURNITURE_KEYWORD),
                fo4_vmad_object_property("batteryItemKW", FO4_POWER_ARMOR_BATTERY_ITEM_KEYWORD),
                fo4_vmad_object_property("powerArmorFurnitureKW", FO4_POWER_ARMOR_FURNITURE_KEYWORD),
            ],
        }],
    });
    build_vmad_bytes_from_payload(&payload, &masters, FO76_MASTER_NAME)
        .expect("power armor VMAD payload must encode")
}

pub(super) fn workbench_script_vmad_bytes() -> Vec<u8> {
    let masters = [FO4_MASTER_NAME.to_string()];
    let payload = serde_json::json!({
        "Version": FO4_VMAD_VERSION,
        "Object Format": FO4_VMAD_OBJECT_FORMAT,
        "Scripts": [{
            "ScriptName": WORKBENCH_SCRIPT,
            "Properties": [
                fo4_vmad_object_property("WorkshopItemKeyword", FO4_WORKSHOP_ITEM_KEYWORD),
            ],
        }],
    });
    build_vmad_bytes_from_payload(&payload, &masters, FO76_MASTER_NAME)
        .expect("workbench VMAD payload must encode")
}

/// VMAD script and property names are `u16` length-prefixed ASCII, so matching
/// the prefix too keeps this from tripping on an unrelated substring.
pub(super) fn vmad_contains_name(bytes: &[u8], name: &str) -> bool {
    let mut needle = Vec::with_capacity(2 + name.len());
    needle.extend_from_slice(&(name.len() as u16).to_le_bytes());
    needle.extend_from_slice(name.as_bytes());
    bytes.windows(needle.len()).any(|window| window == needle)
}

pub(super) fn fo4_vmad_object_property(name: &str, form_id: u32) -> serde_json::Value {
    serde_json::json!({
        "propertyName": name,
        "Type": "Object",
        "Flags": VMAD_PROPERTY_FLAG_EDITED,
        "Value": {
            "Alias": -1,
            "FormID": {
                "reference": {
                    "plugin": FO4_MASTER_NAME,
                    "object_id": format!("{form_id:06X}"),
                },
            },
        },
    })
}

pub(super) fn furniture_record_has_model(record: &Record) -> bool {
    record.fields.iter().any(|entry| {
        entry.sig.0 == *b"MNAM" && u32_field_has_bits(&entry.value, FURNITURE_HAS_MODEL_BIT)
    })
}

pub(super) fn u32_field_has_bits(value: &FieldValue, mask: u32) -> bool {
    let raw = match value {
        FieldValue::Uint(n) => *n as u32,
        FieldValue::Int(n) => *n as u32,
        FieldValue::Bytes(bytes) if bytes.len() >= 4 => {
            u32::from_le_bytes(bytes[0..4].try_into().unwrap())
        }
        FieldValue::Struct(fields) => match fields.first() {
            Some((_, first_value)) => return u32_field_has_bits(first_value, mask),
            None => return false,
        },
        _ => return false,
    };
    raw & mask != 0
}

pub(super) fn target_furniture_marker_count(record: &Record) -> usize {
    let mut count = 0usize;
    for entry in &record.fields {
        let entry_count = match &entry.sig.0 {
            b"SNAM" => marker_parameters_row_count(&entry.value),
            b"FNPR" | b"ENAM" | b"NAM0" => marker_entry_count(&entry.value),
            _ => 0,
        };
        count = count.max(entry_count);
    }
    count
}

pub(super) fn marker_entry_count(value: &FieldValue) -> usize {
    match value {
        FieldValue::None => 0,
        FieldValue::List(items) => items.len(),
        _ => 1,
    }
}

pub(super) fn marker_parameters_row_count(value: &FieldValue) -> usize {
    match value {
        FieldValue::None => 0,
        FieldValue::List(items) => items.len(),
        FieldValue::Bytes(bytes) => bytes.len() / FURNITURE_MARKER_PARAMETERS_ROW_LEN,
        _ => 1,
    }
}
impl Fo76Fo4Hook {
    pub(super) fn rename_furniture_marker_parameters(record: &mut Record) {
        if record.sig.0 != *b"FURN" {
            return;
        }

        for entry in &mut record.fields {
            if entry.sig.0 == *b"ZNAM" {
                entry.sig = SubrecordSig::from_str("SNAM").expect("SNAM is a valid signature");
            }
        }
    }

    pub(super) fn strip_term_looping_sound_snam(record: &mut Record) {
        if record.sig.0 != *b"TERM" {
            return;
        }

        // FO76 TERM reuses SNAM for a looping-sound formid; FO4's loader only
        // accepts TERM SNAM as marker-parameter rows and hard-crashes at
        // startup form load on any payload that is not a whole number of
        // rows. Sound links and empty subrecords have no FO4 representation,
        // so only decoded marker rows survive.
        record.fields.retain(|entry| {
            if entry.sig.0 != *b"SNAM" {
                return true;
            }
            match &entry.value {
                FieldValue::List(items) => !items.is_empty(),
                FieldValue::Struct(_) => true,
                FieldValue::Bytes(bytes) => {
                    !bytes.is_empty() && bytes.len() % FURNITURE_MARKER_PARAMETERS_ROW_LEN == 0
                }
                _ => false,
            }
        });
    }

    pub(super) fn clear_invalid_furniture_active_marker_bits(record: &mut Record) {
        if !matches!(&record.sig.0, b"FURN" | b"TERM") {
            return;
        }

        let mut marker_count = target_furniture_marker_count(record).min(22);
        // Interaction Point 0 is backed by the model's default furniture marker
        // whenever the record has a model, so it stays valid even when the record
        // carries no explicit marker subrecords. Many FO76 terminals rely on the
        // model marker with only `MNAM = InteractionPoint0 | HasModel` and no
        // SNAM/NAM0/ENAM rows; without this, `target_furniture_marker_count`
        // returns 0, Interaction Point 0 is cleared, and the terminal becomes
        // unusable in FO4 (no interaction point to activate).
        if marker_count == 0 && furniture_record_has_model(record) {
            marker_count = 1;
        }
        let valid_marker_bits = if marker_count == 0 {
            0
        } else {
            (1_u32 << marker_count) - 1
        };
        let invalid_marker_bits = FURNITURE_INTERACTION_POINT_BITS & !valid_marker_bits;
        if invalid_marker_bits == 0 {
            return;
        }

        for entry in &mut record.fields {
            if entry.sig.0 == *b"MNAM" {
                clear_u32_bits(&mut entry.value, invalid_marker_bits);
            }
        }
    }

    pub(super) fn ensure_power_armor_furniture_vmad(
        interner: &crate::sym::StringInterner,
        record: &mut Record,
    ) {
        if record.sig.0 != *b"FURN"
            || record.fields.iter().any(|entry| entry.sig.0 == *b"VMAD")
            || !record.fields.iter().any(|entry| {
                entry.sig.0 == *b"KWDA"
                    && fo4_keyword_value(&entry.value, interner, FO4_POWER_ARMOR_FURNITURE_KEYWORD)
            })
        {
            return;
        }

        record.fields.insert(
            0,
            FieldEntry {
                sig: SubrecordSig::from_str("VMAD").expect("VMAD is a valid signature"),
                value: FieldValue::Bytes(SmallVec::from_vec(power_armor_furniture_vmad_bytes())),
            },
        );
    }

    /// Every FO4 crafting bench carries `WorkbenchScript`, which claims an
    /// unowned linked workshop on activation. FO76 benches carry their own
    /// scripts instead, so converted benches never claim a workshop. Append the
    /// FO4 script rather than replacing the carried FO76 ones.
    pub(super) fn ensure_workbench_script_vmad(
        interner: &crate::sym::StringInterner,
        record: &mut Record,
    ) {
        if record.sig.0 != *b"FURN"
            || !record.fields.iter().any(|entry| {
                entry.sig.0 == *b"KWDA"
                    && fo4_keyword_value(&entry.value, interner, FO4_WORKBENCH_GENERAL_KEYWORD)
            })
        {
            return;
        }

        let script = workbench_script_vmad_bytes();
        let Some(entry) = record
            .fields
            .iter_mut()
            .find(|entry| entry.sig.0 == *b"VMAD")
        else {
            record.fields.insert(
                0,
                FieldEntry {
                    sig: SubrecordSig::from_str("VMAD").expect("VMAD is a valid signature"),
                    value: FieldValue::Bytes(SmallVec::from_vec(script)),
                },
            );
            return;
        };

        // FURN VMAD has no fragment section in FO4, so a new script entry can be
        // appended after the existing ones. Only splice into a blob that already
        // uses the version/object-format `script` was encoded against.
        let FieldValue::Bytes(bytes) = &mut entry.value else {
            return;
        };
        if bytes.len() < 6
            || u16::from_le_bytes([bytes[0], bytes[1]]) != FO4_VMAD_VERSION
            || u16::from_le_bytes([bytes[2], bytes[3]]) != FO4_VMAD_OBJECT_FORMAT
            || vmad_contains_name(bytes, WORKBENCH_SCRIPT)
        {
            return;
        }

        let count = u16::from_le_bytes([bytes[4], bytes[5]]);
        let Some(count) = count.checked_add(1) else {
            return;
        };
        bytes[4..6].copy_from_slice(&count.to_le_bytes());
        bytes.extend_from_slice(&script[6..]);
    }

    pub(super) fn ensure_terminal_player_path_keyword(
        interner: &crate::sym::StringInterner,
        record: &mut Record,
    ) {
        if record.sig.0 != *b"TERM"
            || !record.fields.iter().any(|entry| {
                entry.sig.0 == *b"MNAM"
                    && u32_field_has_bits(&entry.value, FURNITURE_INTERACTION_POINT_BITS)
            })
            || record.fields.iter().any(|entry| {
                entry.sig.0 == *b"KWDA"
                    && fo4_keyword_value(
                        &entry.value,
                        interner,
                        FO4_PLAYER_PATH_TO_FURNITURE_KEYWORD,
                    )
            })
        {
            return;
        }

        let keyword = FieldValue::FormKey(FormKey {
            local: FO4_PLAYER_PATH_TO_FURNITURE_KEYWORD,
            plugin: interner.intern(FO4_MASTER_NAME),
        });
        if let Some(entry) = record
            .fields
            .iter_mut()
            .find(|entry| entry.sig.0 == *b"KWDA")
        {
            match &mut entry.value {
                FieldValue::List(keywords) => keywords.push(keyword),
                FieldValue::Bytes(bytes) => {
                    bytes.extend_from_slice(&FO4_PLAYER_PATH_TO_FURNITURE_KEYWORD.to_le_bytes())
                }
                FieldValue::FormKey(_) => {
                    let existing = std::mem::replace(&mut entry.value, FieldValue::None);
                    entry.value = FieldValue::List(vec![existing, keyword]);
                }
                _ => return,
            }
        } else {
            record.fields.push(FieldEntry {
                sig: SubrecordSig::from_str("KWDA").expect("KWDA is a valid signature"),
                value: FieldValue::List(vec![keyword]),
            });
        }

        Self::sync_keyword_count(record);
    }

    pub(super) fn sync_keyword_count(record: &mut Record) {
        let count = record
            .fields
            .iter()
            .filter(|entry| entry.sig.0 == *b"KWDA")
            .map(|entry| match &entry.value {
                FieldValue::List(keywords) => keywords.len() as u32,
                FieldValue::Bytes(bytes) => (bytes.len() / 4) as u32,
                FieldValue::FormKey(_) => 1,
                _ => 0,
            })
            .sum();

        if let Some(entry) = record
            .fields
            .iter_mut()
            .find(|entry| entry.sig.0 == *b"KSIZ")
        {
            set_u32_count(&mut entry.value, count);
            return;
        }

        let insert_at = record
            .fields
            .iter()
            .position(|entry| entry.sig.0 == *b"KWDA")
            .unwrap_or(record.fields.len());
        record.fields.insert(
            insert_at,
            FieldEntry {
                sig: SubrecordSig::from_str("KSIZ").expect("KSIZ is a valid signature"),
                value: FieldValue::Uint(u64::from(count)),
            },
        );
    }
}
