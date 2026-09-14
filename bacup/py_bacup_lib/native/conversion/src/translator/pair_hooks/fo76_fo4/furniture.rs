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
pub(super) const MUSIC_INSTRUMENT_SCRIPT: &str = "B21MusicInstrumentScript";
pub(super) const DEFAULT_ONE_STATE_ACTIVATOR_SCRIPT: &str = "Default1StateActivator";
pub(super) const FO76_DEFAULT_ONE_STATE_ANIMATION: &str = "Play01";
pub(super) const DEFAULT_SEND_STORY_EVENT_ON_MENU_ITEM_RUN_SCRIPT: &str =
    "DefaultSendStoryEventOnMenuItemRun";
pub(super) const EXPECTED_QUEST_TO_START_MEMBER: &str = "ExpectedQuestToStart";
pub(super) const UNIQUE_QUEST_TO_ADD_PLAYER_MEMBER: &str = "UniqueQuestToAddPlayer";
pub(super) const EN05_TERMINAL_MENU_ITEM_TARGET: i64 = 1;
pub(super) const MTR06_PHYSICAL_EXAM_TERMINAL_FORM_ID: u32 = 0x065BDD;
pub(super) const MTR06_PHYSICAL_EXAM_TERMINAL_EDITOR_ID: &str =
    "MTR06_PhysicalExamTerminal_CharlestonHerald";
pub(super) const MTR06_PHYSICAL_EXAM_TERMINAL_SCRIPT: &str = "PhysicalExamTerminalRefScript";
pub(super) const MTR06_PHYSICAL_EXAM_MENU_ITEM_TARGET: i64 = 1;
pub(super) const MTR06_PHYSICAL_EXAM_STORY_EVENT_FORM_ID: u32 = 0x06EFC6;
pub(super) const FO4_VMAD_VERSION: u16 = 6;
pub(super) const FO4_VMAD_OBJECT_FORMAT: u16 = 2;
pub(super) const VMAD_PROPERTY_FLAG_EDITED: u8 = 1;

pub(super) const MUSIC_INSTRUMENT_ROLES: [(&str, usize); 4] =
    [("Intro", 0), ("Rhythm", 1), ("Lead", 2), ("Outro", 3)];

pub(super) const EN05_TERMINAL_STORY_QUEST_BINDINGS: &[(u32, &str, u32, u32)] = &[
    (
        0x1820CF,
        "EN05_MarksmanshipCourseTerminal",
        0x08D23B,
        0x1820D3,
    ),
    (0x1820DD, "EN05_ObstacleCourseTerminal", 0x09C824, 0x18211C),
    (0x182129, "EN05_PatriotismTerminal", 0x08C881, 0x18212A),
];

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

fn music_instrument_form_key(
    value: &FieldValue,
    source_plugin: crate::sym::Sym,
) -> Option<FormKey> {
    let form_key = match value {
        FieldValue::FormKey(form_key) => *form_key,
        FieldValue::Uint(value) => {
            source_form_key_from_raw(u32::try_from(*value).ok()?, source_plugin)
        }
        FieldValue::Int(value) => {
            source_form_key_from_raw(u32::try_from(*value).ok()?, source_plugin)
        }
        FieldValue::Bytes(bytes) if bytes.len() >= 4 => source_form_key_from_raw(
            u32::from_le_bytes(bytes[0..4].try_into().ok()?),
            source_plugin,
        ),
        _ => return None,
    };
    (form_key.local != 0).then_some(form_key)
}

fn music_instrument_form_keys(
    value: &FieldValue,
    source_plugin: crate::sym::Sym,
    interner: &crate::sym::StringInterner,
) -> Option<[Option<FormKey>; 4]> {
    let mut form_keys = [None; 4];
    match value {
        FieldValue::Bytes(bytes) if bytes.len() == 16 => {
            for (index, chunk) in bytes.chunks_exact(4).enumerate() {
                let raw = u32::from_le_bytes(chunk.try_into().ok()?);
                if raw != 0 {
                    form_keys[index] = Some(source_form_key_from_raw(raw, source_plugin));
                }
            }
        }
        FieldValue::Struct(fields) => {
            for (name, index) in MUSIC_INSTRUMENT_ROLES {
                form_keys[index] = named_value_canonical(fields, name, interner)
                    .and_then(|value| music_instrument_form_key(value, source_plugin));
            }
        }
        _ => return None,
    }
    (form_keys[1].is_some() && form_keys[2].is_some()).then_some(form_keys)
}

fn music_instrument_vmad_bytes(
    form_keys: [Option<FormKey>; 4],
    ctx: &PairCtx<'_>,
) -> Option<Vec<u8>> {
    let mut properties = Vec::new();
    for (name, index) in MUSIC_INSTRUMENT_ROLES {
        let Some(form_key) = form_keys[index] else {
            continue;
        };
        let plugin = ctx.interner.resolve(form_key.plugin)?;
        properties.push(serde_json::json!({
            "propertyName": name,
            "Type": "Object",
            "Flags": VMAD_PROPERTY_FLAG_EDITED,
            "Value": {
                "Alias": -1,
                "FormID": {
                    "reference": {
                        "plugin": plugin,
                        "object_id": format!("{:06X}", form_key.local),
                    },
                },
            },
        }));
    }
    let payload = serde_json::json!({
        "Version": FO4_VMAD_VERSION,
        "Object Format": FO4_VMAD_OBJECT_FORMAT,
        "Scripts": [{
            "ScriptName": MUSIC_INSTRUMENT_SCRIPT,
            "Properties": properties,
        }],
    });
    build_vmad_bytes_from_payload(&payload, ctx.target_master_names, FO76_MASTER_NAME)
}

/// VMAD script and property names are `u16` length-prefixed ASCII, so matching
/// the prefix too keeps this from tripping on an unrelated substring.
pub(super) fn vmad_contains_name(bytes: &[u8], name: &str) -> bool {
    let mut needle = Vec::with_capacity(2 + name.len());
    needle.extend_from_slice(&(name.len() as u16).to_le_bytes());
    needle.extend_from_slice(name.as_bytes());
    bytes.windows(needle.len()).any(|window| window == needle)
}

fn single_propertyless_vmad_script_property_count_offset(
    bytes: &[u8],
    script_name: &str,
) -> Option<usize> {
    if bytes.len() < 11
        || u16::from_le_bytes([bytes[0], bytes[1]]) != FO4_VMAD_VERSION
        || u16::from_le_bytes([bytes[2], bytes[3]]) != FO4_VMAD_OBJECT_FORMAT
        || u16::from_le_bytes([bytes[4], bytes[5]]) != 1
    {
        return None;
    }

    let name_len = usize::from(u16::from_le_bytes([bytes[6], bytes[7]]));
    let name_start = 8;
    let name_end = name_start + name_len;
    let property_count_offset = name_end + 1;
    let property_count_end = property_count_offset + 2;
    if property_count_end != bytes.len()
        || !bytes
            .get(name_start..name_end)?
            .eq_ignore_ascii_case(script_name.as_bytes())
        || u16::from_le_bytes(
            bytes
                .get(property_count_offset..property_count_end)?
                .try_into()
                .ok()?,
        ) != 0
    {
        return None;
    }
    Some(property_count_offset)
}

fn push_vmad_string(bytes: &mut SmallVec<[u8; 32]>, value: &str) {
    bytes.extend_from_slice(&(value.len() as u16).to_le_bytes());
    bytes.extend_from_slice(value.as_bytes());
}

fn append_vmad_string_property(
    bytes: &mut SmallVec<[u8; 32]>,
    property_count_offset: usize,
    name: &str,
    value: &str,
) {
    bytes[property_count_offset..property_count_offset + 2].copy_from_slice(&1_u16.to_le_bytes());
    push_vmad_string(bytes, name);
    bytes.push(2);
    bytes.push(VMAD_PROPERTY_FLAG_EDITED);
    push_vmad_string(bytes, value);
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

pub(super) fn raw_vmad_object_member(name: &str, raw_form_id: u32) -> serde_json::Value {
    serde_json::json!({
        "memberName": name,
        "Type": "Object",
        "Flags": VMAD_PROPERTY_FLAG_EDITED,
        "Value": {
            "Alias": -1,
            "FormID": {
                "raw": format!("{raw_form_id:08X}"),
            },
        },
    })
}

fn vmad_raw_form_id_for_object_id(value: &serde_json::Value, object_id: u32) -> Option<u32> {
    match value {
        serde_json::Value::Array(values) => values
            .iter()
            .find_map(|value| vmad_raw_form_id_for_object_id(value, object_id)),
        serde_json::Value::Object(fields) => {
            if fields
                .get("reference")
                .and_then(serde_json::Value::as_object)
                .and_then(|reference| reference.get("object_id"))
                .and_then(serde_json::Value::as_str)
                .and_then(|value| u32::from_str_radix(value, 16).ok())
                == Some(object_id)
            {
                if let Some(raw) = fields
                    .get("raw")
                    .and_then(serde_json::Value::as_str)
                    .and_then(|value| u32::from_str_radix(value, 16).ok())
                {
                    return Some(raw);
                }
                if fields
                    .get("reference")
                    .and_then(serde_json::Value::as_object)
                    .and_then(|reference| reference.get("plugin"))
                    .and_then(serde_json::Value::as_str)
                    == Some(FO76_MASTER_NAME)
                {
                    return Some((1 << 24) | object_id);
                }
            }
            fields
                .values()
                .find_map(|value| vmad_raw_form_id_for_object_id(value, object_id))
        }
        _ => None,
    }
}

pub(super) fn vmad_struct_entry_i32(value: &serde_json::Value, name: &str) -> Option<i64> {
    value.as_array()?.iter().find_map(|member| {
        (member.get("memberName").and_then(serde_json::Value::as_str) == Some(name))
            .then(|| member.get("Value").and_then(serde_json::Value::as_i64))
            .flatten()
    })
}

fn workbench_data_has_no_bench_type(
    value: &FieldValue,
    interner: &crate::sym::StringInterner,
) -> bool {
    match value {
        FieldValue::None => true,
        FieldValue::Uint(value) => value & 0xFF == 0,
        FieldValue::Int(value) => value & 0xFF == 0,
        FieldValue::Bytes(bytes) => bytes.first().is_none_or(|bench_type| *bench_type == 0),
        FieldValue::Struct(fields) => fields
            .iter()
            .find(|(name, _)| {
                interner
                    .resolve(*name)
                    .is_some_and(|name| name.eq_ignore_ascii_case("BenchType"))
            })
            .is_none_or(|(_, value)| workbench_data_has_no_bench_type(value, interner)),
        _ => false,
    }
}

pub(super) fn u32_field_has_bits(value: &FieldValue, mask: u32) -> bool {
    u32_field_value(value).is_some_and(|raw| raw & mask != 0)
}

fn u32_field_value(value: &FieldValue) -> Option<u32> {
    match value {
        FieldValue::Uint(n) => Some(*n as u32),
        FieldValue::Int(n) => Some(*n as u32),
        FieldValue::Bytes(bytes) if bytes.len() >= 4 => {
            Some(u32::from_le_bytes(bytes[0..4].try_into().unwrap()))
        }
        FieldValue::Struct(fields) => match fields.first() {
            Some((_, first_value)) => u32_field_value(first_value),
            None => None,
        },
        _ => None,
    }
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
    pub(super) fn lower_music_instrument_to_vmad(ctx: &PairCtx<'_>, record: &mut Record) {
        if record.sig.0 != *b"FURN" {
            return;
        }

        record.fields.retain(|entry| entry.sig.0 != *b"VMAD");
        let Some(entry) = record
            .fields
            .iter_mut()
            .find(|entry| entry.sig.0 == *b"FNMU")
        else {
            return;
        };
        let Some(form_keys) =
            music_instrument_form_keys(&entry.value, record.form_key.plugin, ctx.interner)
        else {
            return;
        };
        let Some(bytes) = music_instrument_vmad_bytes(form_keys, ctx) else {
            return;
        };

        entry.sig = SubrecordSig::from_str("VMAD").expect("VMAD is a valid signature");
        entry.value = FieldValue::Bytes(SmallVec::from_vec(bytes));
    }

    pub(super) fn ensure_default_one_state_activator_animation(record: &mut Record) {
        if record.sig.0 != *b"ACTI" {
            return;
        }

        let Some(FieldEntry {
            value: FieldValue::Bytes(bytes),
            ..
        }) = record
            .fields
            .iter_mut()
            .find(|entry| entry.sig.0 == *b"VMAD")
        else {
            return;
        };
        let Some(property_count_offset) = single_propertyless_vmad_script_property_count_offset(
            bytes,
            DEFAULT_ONE_STATE_ACTIVATOR_SCRIPT,
        ) else {
            return;
        };

        // FO76's one-state graph listens for Play01, while FO4's script default
        // is "open". Persist the FO76 default on each converted base record.
        append_vmad_string_property(
            bytes,
            property_count_offset,
            "Anim",
            FO76_DEFAULT_ONE_STATE_ANIMATION,
        );
    }

    pub(super) fn drop_furniture_workbench_data_without_bench_type(
        interner: &crate::sym::StringInterner,
        record: &mut Record,
    ) {
        if record.sig.0 != *b"FURN" {
            return;
        }
        record.fields.retain(|entry| {
            entry.sig.0 != *b"WBDT" || !workbench_data_has_no_bench_type(&entry.value, interner)
        });
    }

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

        let marker_count = target_furniture_marker_count(record).min(22);

        for entry in &mut record.fields {
            if entry.sig.0 != *b"MNAM" {
                continue;
            }
            let Some(raw) = u32_field_value(&entry.value) else {
                continue;
            };
            let active_marker_bits = raw & FURNITURE_INTERACTION_POINT_BITS;
            let mut valid_marker_bits = 0_u32;
            // Point 0 can come from the model; explicit marker rows then map to
            // the remaining set bits in the sparse active-point mask.
            if raw & FURNITURE_HAS_MODEL_BIT != 0 && active_marker_bits & 1 != 0 {
                valid_marker_bits |= 1;
            }
            let mut remaining_markers = marker_count;
            for marker_index in 0..22 {
                let marker_bit = 1_u32 << marker_index;
                if active_marker_bits & marker_bit == 0 || valid_marker_bits & marker_bit != 0 {
                    continue;
                }
                if remaining_markers == 0 {
                    break;
                }
                valid_marker_bits |= marker_bit;
                remaining_markers -= 1;
            }
            clear_u32_bits(&mut entry.value, active_marker_bits & !valid_marker_bits);
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
        Self::suppress_mtr06_physical_exam_generic_story_event(interner, record);
        Self::ensure_en05_terminal_story_quest_binding(interner, record);

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

    fn suppress_mtr06_physical_exam_generic_story_event(
        interner: &crate::sym::StringInterner,
        record: &mut Record,
    ) {
        if record.sig.0 != *b"TERM"
            || record.form_key.local != MTR06_PHYSICAL_EXAM_TERMINAL_FORM_ID
            || interner.resolve(record.form_key.plugin) != Some(FO76_MASTER_NAME)
            || record.eid.and_then(|eid| interner.resolve(eid))
                != Some(MTR06_PHYSICAL_EXAM_TERMINAL_EDITOR_ID)
        {
            return;
        }

        let mut vmad_fields = record
            .fields
            .iter_mut()
            .filter(|entry| entry.sig.0 == *b"VMAD");
        let Some(vmad_field) = vmad_fields.next() else {
            return;
        };
        if vmad_fields.next().is_some() {
            return;
        }
        let FieldValue::Bytes(bytes) = &mut vmad_field.value else {
            return;
        };

        let masters = [FO4_MASTER_NAME.to_string()];
        let Some(mut payload) = esp_authoring_core::plugin_runtime::authoring::authoring_serialize::compact_vmad_payload_json(
            bytes,
            &masters,
            FO76_MASTER_NAME,
            Some("TERM"),
        ) else {
            return;
        };
        if build_vmad_bytes_from_payload(&payload, &masters, FO76_MASTER_NAME).as_deref()
            != Some(bytes.as_slice())
        {
            return;
        }

        let Some(scripts) = payload
            .get_mut("Scripts")
            .and_then(serde_json::Value::as_array_mut)
        else {
            return;
        };
        if scripts
            .iter()
            .filter(|script| {
                script.get("ScriptName").and_then(serde_json::Value::as_str)
                    == Some(MTR06_PHYSICAL_EXAM_TERMINAL_SCRIPT)
            })
            .count()
            != 1
        {
            return;
        }
        let mut generic_scripts = scripts.iter_mut().filter(|script| {
            script.get("ScriptName").and_then(serde_json::Value::as_str)
                == Some(DEFAULT_SEND_STORY_EVENT_ON_MENU_ITEM_RUN_SCRIPT)
        });
        let Some(generic_script) = generic_scripts.next() else {
            return;
        };
        if generic_scripts.next().is_some() {
            return;
        }
        let Some(properties) = generic_script
            .get_mut("Properties")
            .and_then(serde_json::Value::as_array_mut)
        else {
            return;
        };
        let mut menu_data_properties = properties.iter_mut().filter(|property| {
            property
                .get("propertyName")
                .and_then(serde_json::Value::as_str)
                == Some("MenuData")
                && property.get("Type").and_then(serde_json::Value::as_str)
                    == Some("Array of Struct")
        });
        let Some(menu_data) = menu_data_properties.next() else {
            return;
        };
        if menu_data_properties.next().is_some() {
            return;
        }
        let Some(rows) = menu_data
            .get_mut("Value")
            .and_then(serde_json::Value::as_array_mut)
        else {
            return;
        };
        let matching_rows = rows
            .iter()
            .enumerate()
            .filter(|(_, row)| {
                vmad_struct_entry_i32(row, "iMenuItemTarget")
                    == Some(MTR06_PHYSICAL_EXAM_MENU_ITEM_TARGET)
                    && row.as_array().is_some_and(|members| {
                        members.iter().any(|member| {
                            member.get("memberName").and_then(serde_json::Value::as_str)
                                == Some("StoryEventToSend")
                                && vmad_raw_form_id_for_object_id(
                                    member,
                                    MTR06_PHYSICAL_EXAM_STORY_EVENT_FORM_ID,
                                )
                                .is_some()
                        })
                    })
            })
            .map(|(index, _)| index)
            .collect::<Vec<_>>();
        let [row_index] = matching_rows.as_slice() else {
            return;
        };
        rows.remove(*row_index);

        let Some(encoded) = build_vmad_bytes_from_payload(&payload, &masters, FO76_MASTER_NAME)
        else {
            return;
        };
        *bytes = SmallVec::from_vec(encoded);
    }

    fn ensure_en05_terminal_story_quest_binding(
        interner: &crate::sym::StringInterner,
        record: &mut Record,
    ) {
        if record.sig.0 != *b"TERM" {
            return;
        }

        let Some((_, _, quest_form_id, self_reference_form_id)) =
            EN05_TERMINAL_STORY_QUEST_BINDINGS.iter().find(
                |(terminal_form_id, editor_id, _, _)| {
                    record.form_key.local == *terminal_form_id
                        && record.eid.and_then(|eid| interner.resolve(eid)) == Some(*editor_id)
                },
            )
        else {
            return;
        };

        let mut vmad_fields = record
            .fields
            .iter_mut()
            .filter(|entry| entry.sig.0 == *b"VMAD");
        let Some(vmad_field) = vmad_fields.next() else {
            return;
        };
        if vmad_fields.next().is_some() {
            return;
        }
        let FieldValue::Bytes(bytes) = &mut vmad_field.value else {
            return;
        };

        let masters = [FO4_MASTER_NAME.to_string()];
        let Some(mut payload) = esp_authoring_core::plugin_runtime::authoring::authoring_serialize::compact_vmad_payload_json(
            bytes,
            &masters,
            FO76_MASTER_NAME,
            Some("TERM"),
        ) else {
            return;
        };
        if build_vmad_bytes_from_payload(&payload, &masters, FO76_MASTER_NAME).as_deref()
            != Some(bytes.as_slice())
        {
            return;
        }

        let Some(scripts) = payload
            .get_mut("Scripts")
            .and_then(serde_json::Value::as_array_mut)
        else {
            return;
        };
        let mut matching_scripts = scripts.iter().enumerate().filter(|(_, script)| {
            script.get("ScriptName").and_then(serde_json::Value::as_str)
                == Some(DEFAULT_SEND_STORY_EVENT_ON_MENU_ITEM_RUN_SCRIPT)
        });
        let Some((script_index, _)) = matching_scripts.next() else {
            return;
        };
        if matching_scripts.next().is_some() {
            return;
        }
        let script = &mut scripts[script_index];
        let Some(properties) = script
            .get_mut("Properties")
            .and_then(serde_json::Value::as_array_mut)
        else {
            return;
        };
        let mut menu_data_properties = properties.iter_mut().filter(|property| {
            property
                .get("propertyName")
                .and_then(serde_json::Value::as_str)
                == Some("MenuData")
                && property.get("Type").and_then(serde_json::Value::as_str)
                    == Some("Array of Struct")
        });
        let Some(menu_data) = menu_data_properties.next() else {
            return;
        };
        if menu_data_properties.next().is_some() {
            return;
        }
        let Some(rows) = menu_data
            .get_mut("Value")
            .and_then(serde_json::Value::as_array_mut)
        else {
            return;
        };
        let mut matching_rows = rows.iter().enumerate().filter(|(_, row)| {
            vmad_struct_entry_i32(row, "iMenuItemTarget") == Some(EN05_TERMINAL_MENU_ITEM_TARGET)
        });
        let Some((row_index, _)) = matching_rows.next() else {
            return;
        };
        if matching_rows.next().is_some() {
            return;
        }
        let Some(row) = rows[row_index].as_array_mut() else {
            return;
        };
        let quest_member_names = [
            EXPECTED_QUEST_TO_START_MEMBER,
            UNIQUE_QUEST_TO_ADD_PLAYER_MEMBER,
        ];
        for member_name in quest_member_names {
            if let Some(member) = row.iter().find(|member| {
                member.get("memberName").and_then(serde_json::Value::as_str) == Some(member_name)
            }) {
                if vmad_raw_form_id_for_object_id(member, *quest_form_id).is_none() {
                    return;
                }
            }
        }
        if quest_member_names.iter().all(|member_name| {
            row.iter().any(|member| {
                member.get("memberName").and_then(serde_json::Value::as_str) == Some(*member_name)
            })
        }) {
            return;
        }
        // PairCtx does not expose the target master list. Reuse the load-order
        // byte from this exact menu row's carried same-plugin VMAD reference.
        let Some(self_raw_form_id) = row
            .iter()
            .find_map(|member| vmad_raw_form_id_for_object_id(member, *self_reference_form_id))
        else {
            return;
        };
        let raw_quest_form_id = (self_raw_form_id & 0xFF00_0000) | *quest_form_id;
        for member_name in quest_member_names {
            if !row.iter().any(|member| {
                member.get("memberName").and_then(serde_json::Value::as_str) == Some(member_name)
            }) {
                row.push(raw_vmad_object_member(member_name, raw_quest_form_id));
            }
        }
        let Some(encoded) = build_vmad_bytes_from_payload(&payload, &masters, FO76_MASTER_NAME)
        else {
            return;
        };
        *bytes = SmallVec::from_vec(encoded);
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
