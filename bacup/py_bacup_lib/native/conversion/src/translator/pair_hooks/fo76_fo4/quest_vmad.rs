use super::*;

pub(super) const FO76_QUEST_EVENT_SCPT: u32 = 0x5450_4353;
pub(super) const FO4_QUEST_EVENT_LOCATION1: u32 = 12_620;
pub(super) const FO4_QUEST_EVENT_REFERENCE1: u32 = 12_626;
pub(super) const FO4_QUEST_EVENT_REFERENCE2: u32 = 12_882;
pub(super) const FO76_QUEST_EVENT_REFERENCE3: u32 = 13_138;
pub(super) const FO4_PLAYER_REF_FORM_ID: u32 = 0x0000_14;
pub(super) const FO76_CB00_MINE_REPAIR_FORM_ID: u32 = 0x0009_8969;
pub(super) const FO76_GQ_WORKSHOP_CLEAR_FORM_ID: u32 = 0x0017_89F9;
pub(super) const FO76_SQ_WORKSHOP_VERTIBIRD_FORM_ID: u32 = 0x0024_5337;
pub(super) const FO76_TW002_FORM_ID: u32 = 0x0010_E201;
const FO76_TW002_EID: &str = "tw002";
pub(super) const FO76_TW002_EVENT_TRIGGER_ALIAS_ID: u32 = 39;
pub(super) const FO76_VMAD_VERSION: u16 = 6;
pub(super) const FO76_VMAD_OBJECT_FORMAT: u16 = 2;
pub(super) const FO76_VMAD_ALIAS_VERSION: u16 = 6;
const FO76_W05_MQS_205_FORM_ID: u32 = 0x0041_CB6D;
const FO76_DEFAULT_QUEST_SHUTDOWN_SCRIPT: &[u8] = b"DefaultQuestShutdownScript";
pub(super) const FO76_RD01_ENC02_FORM_ID: u32 = 0x0078_F7A1;
const FO76_RD01_ENC02_SCRIPT: &[u8] = b"Raids:RD01:Enc02:QuestScript";
const FO76_COMPANION_EVENT_ALIAS_QUESTS: [(u32, &str, &[u32]); 20] = [
    (0x0054_F1A4, "comp_rq_fetch", &[1, 13]),
    (0x0056_FB76, "comp_rq_kill", &[1, 18]),
    (0x0057_27AD, "comp_rq_rescue", &[1, 18]),
    (0x0055_FD53, "comp_visitor", &[0, 1]),
    (
        0x0058_215B,
        "comp_rq_fetch_specificaliases_beckett_000_saddiary",
        &[1],
    ),
    (
        0x0058_2164,
        "comp_rq_rescue_specificaliases_beckett_001_cultistsage",
        &[1],
    ),
    (
        0x0058_2163,
        "comp_rq_fetch_specificaliases_beckett_002_key",
        &[1],
    ),
    (
        0x0058_2160,
        "comp_rq_kill_specificaliases_beckett_003_bronx",
        &[1],
    ),
    (
        0x0058_2167,
        "comp_rq_fetch_specificaliases_beckett_004_cave",
        &[1],
    ),
    (
        0x0058_2165,
        "comp_rq_kill_specificaliases_beckett_005_blood",
        &[1],
    ),
    (
        0x0058_215A,
        "comp_rq_rescue_specificaliases_beckett_006_pet",
        &[1],
    ),
    (
        0x0058_215E,
        "comp_rq_kill_specificaliases_beckett_007_dj",
        &[1],
    ),
    (
        0x0058_216A,
        "comp_rq_rescue_specificaliases_beckett_008_missnanny",
        &[1],
    ),
    (
        0x0058_2168,
        "comp_rq_fetch_specificaliases_beckett_009_holotapes",
        &[1],
    ),
    (
        0x0058_215F,
        "comp_rq_fetch_specificaliases_beckett_010_poisonedfood",
        &[1],
    ),
    (
        0x0058_215D,
        "comp_rq_kill_specificaliases_beckett_011_eye",
        &[1],
    ),
    (
        0x005A_272F,
        "comp_rq_kill_specificaliases_beckett_bloodeaglerandomloc",
        &[1],
    ),
    (
        0x005A_2730,
        "comp_rq_kill_specificaliases_beckett_bloodeagledungeon",
        &[1],
    ),
    (
        0x005A_272D,
        "comp_rq_fetch_specificaliases_legendaryarmor",
        &[1],
    ),
    (
        0x005A_272E,
        "comp_rq_fetch_specificaliases_legendaryweapon",
        &[1],
    ),
];
const FO76_RD01_ENC02_DEAD_TOPIC_PROPERTIES: &[&[u8]] = &[
    b"kDifficultyMaxedTopic",
    b"kDifficultyIncreasedTopic",
    b"kEncounterStartTopic",
];

#[derive(Default)]
struct QustVmadFragmentPlayerAliases {
    player_alias_ids: SmallVec<[u32; 4]>,
    owning_player_alias_ids: SmallVec<[u32; 4]>,
}

struct QustVmadProperty<'a> {
    name: &'a [u8],
    property_type: u8,
    flags: u8,
    value: &'a [u8],
    raw: &'a [u8],
}

pub(super) fn strip_w05_mqs_205_shutdown_script_binding(
    interner: &crate::sym::StringInterner,
    record: &mut Record,
) {
    if record.sig.0 != *b"QUST"
        || record.form_key.local & 0x00FF_FFFF != FO76_W05_MQS_205_FORM_ID
        || !interner
            .resolve(record.form_key.plugin)
            .is_some_and(|plugin| plugin.eq_ignore_ascii_case(FO76_MASTER_NAME))
    {
        return;
    }

    for entry in &mut record.fields {
        if entry.sig.0 != *b"VMAD" {
            continue;
        }
        let FieldValue::Bytes(bytes) = &mut entry.value else {
            continue;
        };
        let Some(stripped) = strip_root_script_binding(bytes, FO76_DEFAULT_QUEST_SHUTDOWN_SCRIPT)
        else {
            continue;
        };
        *bytes = SmallVec::from_vec(stripped);
    }
}

pub(super) fn strip_rd01_enc02_dead_topic_bindings(
    interner: &crate::sym::StringInterner,
    record: &mut Record,
) {
    if record.sig.0 != *b"QUST"
        || record.form_key.local & 0x00FF_FFFF != FO76_RD01_ENC02_FORM_ID
        || !interner
            .resolve(record.form_key.plugin)
            .is_some_and(|plugin| plugin.eq_ignore_ascii_case(FO76_MASTER_NAME))
    {
        return;
    }

    for entry in &mut record.fields {
        if entry.sig.0 != *b"VMAD" {
            continue;
        }
        let FieldValue::Bytes(bytes) = &mut entry.value else {
            continue;
        };
        let Some(stripped) = strip_root_script_properties(
            bytes,
            FO76_RD01_ENC02_SCRIPT,
            FO76_RD01_ENC02_DEAD_TOPIC_PROPERTIES,
        ) else {
            continue;
        };
        *bytes = SmallVec::from_vec(stripped);
    }
}

fn strip_root_script_properties(
    data: &[u8],
    target_script: &[u8],
    property_names: &[&[u8]],
) -> Option<Vec<u8>> {
    let version = qust_vmad_read_u16(data, 0)?;
    let object_format = qust_vmad_read_u16(data, 2)?;
    let script_count = qust_vmad_read_u16(data, 4)? as usize;
    if version != FO76_VMAD_VERSION || object_format != FO76_VMAD_OBJECT_FORMAT {
        return None;
    }

    let mut offset = 6;
    let mut changed = false;
    let mut scripts = Vec::with_capacity(script_count);
    for _ in 0..script_count {
        let script_start = offset;
        let script_name = qust_vmad_read_string(data, &mut offset)?;
        qust_vmad_advance(&mut offset, 1, data.len())?;
        let property_count_offset = offset;
        let property_count = qust_vmad_read_u16_advance(data, &mut offset)? as usize;
        let mut retained_properties = Vec::with_capacity(property_count);
        for _ in 0..property_count {
            let property_start = offset;
            let property_name = qust_vmad_read_string(data, &mut offset)?;
            let property_type = qust_vmad_read_u8_advance(data, &mut offset)?;
            qust_vmad_advance(&mut offset, 1, data.len())?;
            qust_vmad_skip_property_value(data, &mut offset, property_type, object_format)?;
            let should_drop = script_name.eq_ignore_ascii_case(target_script)
                && property_names
                    .iter()
                    .any(|candidate| property_name.eq_ignore_ascii_case(candidate));
            if should_drop {
                changed = true;
            } else {
                retained_properties.push(&data[property_start..offset]);
            }
        }

        if retained_properties.len() == property_count {
            scripts.push(data[script_start..offset].to_vec());
            continue;
        }
        let retained_count: u16 = retained_properties.len().try_into().ok()?;
        let mut script = data[script_start..property_count_offset].to_vec();
        script.extend_from_slice(&retained_count.to_le_bytes());
        for property in retained_properties {
            script.extend_from_slice(property);
        }
        scripts.push(script);
    }
    if !changed {
        return None;
    }

    let mut stripped = data[..6].to_vec();
    for script in scripts {
        stripped.extend_from_slice(&script);
    }
    stripped.extend_from_slice(&data[offset..]);
    Some(stripped)
}

fn strip_root_script_binding(data: &[u8], script_name_to_strip: &[u8]) -> Option<Vec<u8>> {
    let version = qust_vmad_read_u16(data, 0)?;
    let object_format = qust_vmad_read_u16(data, 2)?;
    let script_count = qust_vmad_read_u16(data, 4)? as usize;
    if version != FO76_VMAD_VERSION || object_format != FO76_VMAD_OBJECT_FORMAT {
        return None;
    }

    let mut offset = 6;
    let mut retained_scripts = Vec::with_capacity(script_count);
    for _ in 0..script_count {
        let script_start = offset;
        let script_name = qust_vmad_read_script(data, &mut offset, object_format)?;
        if !script_name.eq_ignore_ascii_case(script_name_to_strip) {
            retained_scripts.push(&data[script_start..offset]);
        }
    }
    if retained_scripts.len() == script_count {
        return None;
    }

    let retained_count: u16 = retained_scripts.len().try_into().ok()?;
    let mut stripped = Vec::with_capacity(data.len());
    stripped.extend_from_slice(&data[..4]);
    stripped.extend_from_slice(&retained_count.to_le_bytes());
    for script in retained_scripts {
        stripped.extend_from_slice(script);
    }
    stripped.extend_from_slice(&data[offset..]);
    Some(stripped)
}

pub(super) fn normalize_re_alias_vmad_properties(record: &mut Record) {
    if record.sig.0 != *b"QUST" {
        return;
    }

    for entry in &mut record.fields {
        if entry.sig.0 != *b"VMAD" {
            continue;
        }
        let FieldValue::Bytes(bytes) = &mut entry.value else {
            continue;
        };
        let Some(normalized) = normalize_re_alias_vmad_bytes(bytes) else {
            continue;
        };
        if normalized.as_slice() != bytes.as_slice() {
            *bytes = SmallVec::from_vec(normalized);
        }
    }
}

fn normalize_re_alias_vmad_bytes(data: &[u8]) -> Option<Vec<u8>> {
    let version = qust_vmad_read_u16(data, 0)?;
    let object_format = qust_vmad_read_u16(data, 2)?;
    let script_count = qust_vmad_read_u16(data, 4)? as usize;
    if version != FO76_VMAD_VERSION || object_format != FO76_VMAD_OBJECT_FORMAT {
        return None;
    }

    let mut offset = 6;
    for _ in 0..script_count {
        qust_vmad_read_script(data, &mut offset, object_format)?;
    }

    let fragment_version = qust_vmad_read_u8_advance(data, &mut offset)?;
    if fragment_version != FO76_QUST_FRAGMENT_VERSION {
        return None;
    }
    let fragment_count = qust_vmad_read_u16_advance(data, &mut offset)? as usize;
    let fragment_script_name = qust_vmad_read_string(data, &mut offset)?;
    if !fragment_script_name.is_empty() {
        qust_vmad_advance(&mut offset, 1, data.len())?;
        let property_count = qust_vmad_read_u16_advance(data, &mut offset)? as usize;
        for _ in 0..property_count {
            qust_vmad_skip_property(data, &mut offset, object_format)?;
        }
    }
    for _ in 0..fragment_count {
        qust_vmad_advance(&mut offset, 9, data.len())?;
        qust_vmad_read_string(data, &mut offset)?;
        qust_vmad_read_string(data, &mut offset)?;
    }

    let alias_count = qust_vmad_read_u16_advance(data, &mut offset)? as usize;
    let mut normalized = data[..offset].to_vec();
    for _ in 0..alias_count {
        let alias_header_start = offset;
        qust_vmad_advance(&mut offset, 8, data.len())?;
        let alias_version = qust_vmad_read_u16_advance(data, &mut offset)?;
        let alias_object_format = qust_vmad_read_u16_advance(data, &mut offset)?;
        if alias_version != FO76_VMAD_ALIAS_VERSION
            || alias_object_format != FO76_VMAD_OBJECT_FORMAT
        {
            return None;
        }
        let alias_script_count = qust_vmad_read_u16_advance(data, &mut offset)? as usize;
        normalized.extend_from_slice(&data[alias_header_start..offset]);
        for _ in 0..alias_script_count {
            normalized.extend_from_slice(&normalize_re_alias_script(
                data,
                &mut offset,
                alias_object_format,
            )?);
        }
    }

    (offset == data.len()).then_some(normalized)
}

fn normalize_re_alias_script(
    data: &[u8],
    offset: &mut usize,
    object_format: u16,
) -> Option<Vec<u8>> {
    let script_start = *offset;
    let script_name = qust_vmad_read_string(data, offset)?;
    qust_vmad_advance(offset, 1, data.len())?;
    let property_count_offset = *offset;
    let property_count = qust_vmad_read_u16_advance(data, offset)? as usize;

    let mut properties = Vec::with_capacity(property_count);
    for _ in 0..property_count {
        let property_start = *offset;
        let property_name = qust_vmad_read_string(data, offset)?;
        let property_type = qust_vmad_read_u8_advance(data, offset)?;
        let flags = qust_vmad_read_u8_advance(data, offset)?;
        let value_start = *offset;
        qust_vmad_skip_property_value(data, offset, property_type, object_format)?;
        properties.push(QustVmadProperty {
            name: property_name,
            property_type,
            flags,
            value: &data[value_start..*offset],
            raw: &data[property_start..*offset],
        });
    }

    if !is_re_alias_script_name(script_name) {
        return Some(data[script_start..*offset].to_vec());
    }

    let mut track_death_value = false;
    let mut track_death_flags = None;
    let mut track_death_count = 0;
    let mut has_track_on_dying = false;
    for property in &properties {
        let is_track_death = property.name.eq_ignore_ascii_case(b"TrackDeath");
        let is_track_on_dying = property.name.eq_ignore_ascii_case(b"TrackOnDying");
        if property.property_type == 5 && (is_track_death || is_track_on_dying) {
            track_death_value |= property.value.first().copied().unwrap_or(0) != 0;
            track_death_flags.get_or_insert(property.flags);
            track_death_count += 1;
            has_track_on_dying |= is_track_on_dying;
        }
    }

    let mut retained = Vec::with_capacity(properties.len());
    let mut emitted_track_death = false;
    for property in &properties {
        let is_track_death = property.name.eq_ignore_ascii_case(b"TrackDeath");
        let is_track_on_dying = property.name.eq_ignore_ascii_case(b"TrackOnDying");
        if property.property_type == 5 && (is_track_death || is_track_on_dying) {
            if emitted_track_death {
                continue;
            }
            emitted_track_death = true;
            if !has_track_on_dying && track_death_count == 1 {
                retained.push(property.raw.to_vec());
            } else {
                let mut translated = Vec::new();
                qust_vmad_write_string(&mut translated, b"TrackDeath")?;
                translated.push(5);
                translated.push(track_death_flags?);
                translated.push(u8::from(track_death_value));
                retained.push(translated);
            }
            continue;
        }

        if re_alias_fo4_property_type(property.name) == Some(property.property_type) {
            retained.push(property.raw.to_vec());
        }
    }

    let retained_count: u16 = retained.len().try_into().ok()?;
    let mut normalized = data[script_start..property_count_offset].to_vec();
    normalized.extend_from_slice(&retained_count.to_le_bytes());
    for property in retained {
        normalized.extend_from_slice(&property);
    }
    Some(normalized)
}

fn is_re_alias_script_name(script_name: &[u8]) -> bool {
    script_name.eq_ignore_ascii_case(b"REAliasScript")
        || script_name.eq_ignore_ascii_case(b"RECollectionAliasScript")
}

fn re_alias_fo4_property_type(property_name: &[u8]) -> Option<u8> {
    if property_name.eq_ignore_ascii_case(b"OnHitFaction") {
        Some(1)
    } else if property_name.eq_ignore_ascii_case(b"GroupIndex")
        || property_name.eq_ignore_ascii_case(b"OnHitStage")
    {
        Some(3)
    } else if property_name.eq_ignore_ascii_case(b"IgnoreForCleanup")
        || property_name.eq_ignore_ascii_case(b"RegisterAlias")
        || property_name.eq_ignore_ascii_case(b"StartsDead")
    {
        Some(5)
    } else {
        None
    }
}

fn qust_vmad_write_string(output: &mut Vec<u8>, value: &[u8]) -> Option<()> {
    let len: u16 = value.len().try_into().ok()?;
    output.extend_from_slice(&len.to_le_bytes());
    output.extend_from_slice(value);
    Some(())
}

pub(super) fn qust_vmad_player_event_consumer_alias_ids(record: &Record) -> SmallVec<[u32; 4]> {
    let mut vmad_fields = record.fields.iter().filter(|entry| entry.sig.0 == *b"VMAD");
    let Some(vmad) = vmad_fields.next() else {
        return SmallVec::new();
    };
    if vmad_fields.next().is_some() {
        return SmallVec::new();
    }
    let FieldValue::Bytes(bytes) = &vmad.value else {
        return SmallVec::new();
    };
    parse_qust_vmad_player_event_consumer_alias_ids(bytes).unwrap_or_default()
}

pub(super) fn qust_vmad_remove_players_alias_ids(record: &Record) -> SmallVec<[u32; 4]> {
    let mut vmad_fields = record.fields.iter().filter(|entry| entry.sig.0 == *b"VMAD");
    let Some(vmad) = vmad_fields.next() else {
        return SmallVec::new();
    };
    if vmad_fields.next().is_some() {
        return SmallVec::new();
    }
    let FieldValue::Bytes(bytes) = &vmad.value else {
        return SmallVec::new();
    };
    let mut alias_ids = parse_qust_vmad_remove_players_alias_ids(bytes).unwrap_or_default();
    let fragment_aliases =
        parse_qust_vmad_fragment_player_alias_ids(bytes, record.form_key.local).unwrap_or_default();
    for alias_id in fragment_aliases.player_alias_ids {
        if !alias_ids.contains(&alias_id) {
            alias_ids.push(alias_id);
        }
    }
    for alias_id in fragment_aliases.owning_player_alias_ids {
        if qust_alias_is_owning_player_scpt_reference3(record, alias_id)
            && !alias_ids.contains(&alias_id)
        {
            alias_ids.push(alias_id);
        }
    }
    alias_ids
}

fn parse_qust_vmad_fragment_player_alias_ids(
    data: &[u8],
    quest_form_id: u32,
) -> Option<QustVmadFragmentPlayerAliases> {
    let version = qust_vmad_read_u16(data, 0)?;
    let object_format = qust_vmad_read_u16(data, 2)?;
    let script_count = qust_vmad_read_u16(data, 4)? as usize;
    if version != FO76_VMAD_VERSION || object_format != FO76_VMAD_OBJECT_FORMAT {
        return None;
    }

    let mut offset = 6;
    for _ in 0..script_count {
        qust_vmad_read_script(data, &mut offset, object_format)?;
    }

    let fragment_version = qust_vmad_read_u8_advance(data, &mut offset)?;
    if fragment_version != FO76_QUST_FRAGMENT_VERSION {
        return None;
    }
    let _fragment_count = qust_vmad_read_u16_advance(data, &mut offset)?;
    let fragment_script_name = qust_vmad_read_string(data, &mut offset)?;
    if fragment_script_name.is_empty() {
        return Some(QustVmadFragmentPlayerAliases::default());
    }

    qust_vmad_advance(&mut offset, 1, data.len())?;
    let property_count = qust_vmad_read_u16_advance(data, &mut offset)? as usize;
    let mut aliases = QustVmadFragmentPlayerAliases::default();
    for _ in 0..property_count {
        let property_name = qust_vmad_read_string(data, &mut offset)?;
        let property_type = qust_vmad_read_u8_advance(data, &mut offset)?;
        qust_vmad_advance(&mut offset, 1, data.len())?;
        let is_player_alias = property_name.eq_ignore_ascii_case(b"Alias_Player")
            || property_name.eq_ignore_ascii_case(b"Alias_currentPlayer");
        let is_owning_player_alias = property_name.eq_ignore_ascii_case(b"Alias_owningPlayer");
        if property_type == 1 && (is_player_alias || is_owning_player_alias) {
            let alias_offset = offset.checked_add(2)?;
            let alias_id = i16::from_le_bytes(
                data.get(alias_offset..alias_offset.checked_add(2)?)?
                    .try_into()
                    .ok()?,
            );
            let form_id_offset = offset.checked_add(4)?;
            let form_id = u32::from_le_bytes(
                data.get(form_id_offset..form_id_offset.checked_add(4)?)?
                    .try_into()
                    .ok()?,
            );
            qust_vmad_advance(&mut offset, 8, data.len())?;
            if alias_id >= 0 {
                let alias_id = alias_id as u32;
                if is_player_alias {
                    aliases.player_alias_ids.push(alias_id);
                } else if form_id == quest_form_id {
                    aliases.owning_player_alias_ids.push(alias_id);
                }
            }
        } else {
            qust_vmad_skip_property_value(data, &mut offset, property_type, object_format)?;
        }
    }
    Some(aliases)
}

fn qust_alias_is_owning_player_scpt_reference3(record: &Record, alias_id: u32) -> bool {
    let mut current_alias_id = None;
    let mut has_owning_player_name = false;
    let mut has_scpt_reference3_fill = false;
    let mut index = 0;

    while index < record.fields.len() {
        let entry = &record.fields[index];
        if QUST_ALIAS_ANCHOR_SIGS.iter().any(|sig| entry.sig.0 == *sig) {
            if current_alias_id == Some(alias_id)
                && has_owning_player_name
                && has_scpt_reference3_fill
            {
                return true;
            }
            current_alias_id = (entry.sig.0 == *b"ALST")
                .then(|| field_value_to_u32(&entry.value))
                .flatten();
            has_owning_player_name = false;
            has_scpt_reference3_fill = false;
        } else if current_alias_id == Some(alias_id) {
            if entry.sig.0 == *b"ALID" {
                has_owning_player_name = qust_alias_name_is_owning_player(&entry.value);
            } else if entry.sig.0 == *b"ALFE" {
                has_scpt_reference3_fill = field_value_to_u32(&entry.value)
                    == Some(FO76_QUEST_EVENT_SCPT)
                    && record.fields.get(index + 1).is_some_and(|next| {
                        next.sig.0 == *b"ALFD"
                            && field_value_to_u32(&next.value) == Some(FO76_QUEST_EVENT_REFERENCE3)
                    });
            }
        }
        index += 1;
    }

    current_alias_id == Some(alias_id) && has_owning_player_name && has_scpt_reference3_fill
}

fn qust_alias_name_is_owning_player(value: &FieldValue) -> bool {
    let FieldValue::Bytes(bytes) = value else {
        return false;
    };
    bytes
        .strip_suffix(&[0])
        .unwrap_or(bytes)
        .eq_ignore_ascii_case(b"owningPlayer")
}

// MineEntranceClosed sends Player as Ref1 and its linked mine as Ref2. The cut
// CB00 quest asks for that Player alias from the nonexistent SCPT Ref3.
pub(super) fn qust_reference1_event_alias_ids(record: &Record) -> SmallVec<[u32; 1]> {
    if qust_is_cb00_mine_repair(record) {
        SmallVec::from_slice(&[0])
    } else {
        SmallVec::new()
    }
}

pub(super) fn qust_preserved_event_alias_ids(
    interner: &crate::sym::StringInterner,
    record: &Record,
) -> SmallVec<[u32; 1]> {
    let preserved = qust_structurally_preserved_event_alias_ids(record);
    if !preserved.is_empty() {
        preserved
    } else if qust_is_tw002_event_trigger(interner, record) {
        SmallVec::from_slice(&[FO76_TW002_EVENT_TRIGGER_ALIAS_ID])
    } else {
        SmallVec::new()
    }
}

fn qust_structurally_preserved_event_alias_ids(record: &Record) -> SmallVec<[u32; 1]> {
    if qust_is_cb00_mine_repair(record) {
        SmallVec::from_slice(&[1])
    } else if qust_is_workshop_clear(record) {
        SmallVec::from_slice(&[0])
    } else if qust_is_workshop_vertibird(record) {
        SmallVec::from_slice(&[0])
    } else {
        SmallVec::new()
    }
}

pub(super) fn qust_companion_event_alias_ids(
    interner: &crate::sym::StringInterner,
    record: &Record,
) -> SmallVec<[u32; 2]> {
    if !interner
        .resolve(record.form_key.plugin)
        .is_some_and(|plugin| plugin.eq_ignore_ascii_case(FO76_MASTER_NAME))
    {
        return SmallVec::new();
    }

    let local = record.form_key.local & 0x00FF_FFFF;
    let Some(eid) = qust_eid_lower(interner, record) else {
        return SmallVec::new();
    };
    FO76_COMPANION_EVENT_ALIAS_QUESTS
        .iter()
        .find_map(|(expected_local, expected_eid, alias_ids)| {
            (local == *expected_local && eid == *expected_eid)
                .then(|| SmallVec::from_slice(alias_ids))
        })
        .unwrap_or_default()
}

pub(super) fn qust_event_alias_rewrites_to_reference1(
    alias_id: u32,
    event: u32,
    event_data: u32,
    reference1_alias_ids: &[u32],
) -> bool {
    event == FO76_QUEST_EVENT_SCPT
        && event_data == FO76_QUEST_EVENT_REFERENCE3
        && reference1_alias_ids.contains(&alias_id)
}

pub(super) fn qust_event_alias_is_proven(
    alias_id: u32,
    event: u32,
    event_data: u32,
    preserved_alias_ids: &[u32],
) -> bool {
    event == FO76_QUEST_EVENT_SCPT
        && matches!(
            event_data,
            FO4_QUEST_EVENT_REFERENCE1 | FO4_QUEST_EVENT_REFERENCE2
        )
        && preserved_alias_ids.contains(&alias_id)
}

pub(super) fn qust_location_event_alias_is_proven(event: u32, event_data: u32) -> bool {
    event == FO76_QUEST_EVENT_SCPT && event_data == FO4_QUEST_EVENT_LOCATION1
}

fn qust_alias_has_name(record: &Record, alias_id: u32, expected: &[u8]) -> bool {
    let mut current_alias_id = None;
    for entry in &record.fields {
        if QUST_ALIAS_ANCHOR_SIGS.iter().any(|sig| entry.sig.0 == *sig) {
            current_alias_id = (entry.sig.0 == *b"ALST")
                .then(|| field_value_to_u32(&entry.value))
                .flatten();
        } else if current_alias_id == Some(alias_id) && entry.sig.0 == *b"ALID" {
            let FieldValue::Bytes(bytes) = &entry.value else {
                return false;
            };
            return bytes
                .strip_suffix(&[0])
                .unwrap_or(bytes)
                .eq_ignore_ascii_case(expected);
        }
    }
    false
}

fn qust_alias_has_event_fill(
    record: &Record,
    alias_id: u32,
    expected_event: u32,
    expected_event_data: u32,
) -> bool {
    let mut current_alias_id = None;
    for (index, entry) in record.fields.iter().enumerate() {
        if QUST_ALIAS_ANCHOR_SIGS.iter().any(|sig| entry.sig.0 == *sig) {
            current_alias_id = (entry.sig.0 == *b"ALST")
                .then(|| field_value_to_u32(&entry.value))
                .flatten();
        } else if current_alias_id == Some(alias_id)
            && entry.sig.0 == *b"ALFE"
            && field_value_to_u32(&entry.value) == Some(expected_event)
            && record.fields.get(index + 1).is_some_and(|next| {
                next.sig.0 == *b"ALFD"
                    && field_value_to_u32(&next.value) == Some(expected_event_data)
            })
        {
            return true;
        }
    }
    false
}

fn qust_is_cb00_mine_repair(record: &Record) -> bool {
    record.form_key.local == FO76_CB00_MINE_REPAIR_FORM_ID
        && qust_alias_has_name(record, 0, b"Player")
        && qust_alias_has_name(record, 1, b"MINE")
}

fn qust_is_workshop_vertibird(record: &Record) -> bool {
    record.form_key.local == FO76_SQ_WORKSHOP_VERTIBIRD_FORM_ID
        && qust_alias_has_name(record, 0, b"AttackTarget")
}

fn qust_is_workshop_clear(record: &Record) -> bool {
    record.form_key.local == FO76_GQ_WORKSHOP_CLEAR_FORM_ID
        && qust_alias_has_name(record, 0, b"Workshop")
        && qust_alias_has_event_fill(record, 0, FO76_QUEST_EVENT_SCPT, FO4_QUEST_EVENT_REFERENCE1)
}

fn qust_is_tw002_event_trigger(interner: &crate::sym::StringInterner, record: &Record) -> bool {
    interner
        .resolve(record.form_key.plugin)
        .is_some_and(|plugin| plugin.eq_ignore_ascii_case(FO76_MASTER_NAME))
        && record.form_key.local & 0x00FF_FFFF == FO76_TW002_FORM_ID
        && qust_eid_lower(interner, record).as_deref() == Some(FO76_TW002_EID)
        && qust_alias_has_name(record, FO76_TW002_EVENT_TRIGGER_ALIAS_ID, b"EventTrigger")
        && qust_alias_has_event_fill(
            record,
            FO76_TW002_EVENT_TRIGGER_ALIAS_ID,
            FO76_QUEST_EVENT_SCPT,
            FO4_QUEST_EVENT_REFERENCE1,
        )
}

// FO76 fills this script's multiplayer participant aliases from Story Manager
// event data. In single-player FO4, its playerAliases property is the proof that
// those aliases should resolve to PlayerRef instead.
pub(super) fn parse_qust_vmad_remove_players_alias_ids(data: &[u8]) -> Option<SmallVec<[u32; 4]>> {
    let version = qust_vmad_read_u16(data, 0)?;
    let object_format = qust_vmad_read_u16(data, 2)?;
    let script_count = qust_vmad_read_u16(data, 4)? as usize;
    if version != FO76_VMAD_VERSION || object_format != FO76_VMAD_OBJECT_FORMAT {
        return None;
    }

    let mut offset = 6;
    let mut alias_ids = SmallVec::new();
    for _ in 0..script_count {
        let script_name = qust_vmad_read_string(data, &mut offset)?;
        qust_vmad_advance(&mut offset, 1, data.len())?;
        let property_count = qust_vmad_read_u16_advance(data, &mut offset)? as usize;
        for _ in 0..property_count {
            let property_name = qust_vmad_read_string(data, &mut offset)?;
            let property_type = qust_vmad_read_u8_advance(data, &mut offset)?;
            qust_vmad_advance(&mut offset, 1, data.len())?;
            if script_name.eq_ignore_ascii_case(b"DefaultQuestRemovePlayersScript")
                && property_name.eq_ignore_ascii_case(b"playerAliases")
                && property_type == 11
            {
                let count = qust_vmad_read_nonnegative_count(data, &mut offset)?;
                for _ in 0..count {
                    let alias_offset = offset.checked_add(2)?;
                    let alias_id = i16::from_le_bytes(
                        data.get(alias_offset..alias_offset.checked_add(2)?)?
                            .try_into()
                            .ok()?,
                    );
                    qust_vmad_advance(&mut offset, 8, data.len())?;
                    if alias_id >= 0 {
                        let alias_id = alias_id as u32;
                        if !alias_ids.contains(&alias_id) {
                            alias_ids.push(alias_id);
                        }
                    }
                }
            } else {
                qust_vmad_skip_property_value(data, &mut offset, property_type, object_format)?;
            }
        }
    }
    Some(alias_ids)
}

pub(super) fn qust_event_alias_rewrites_to_player(
    alias_id: u32,
    event: u32,
    event_data: u32,
    player_event_consumer_alias_ids: &[u32],
    remove_players_alias_ids: &[u32],
) -> bool {
    remove_players_alias_ids.contains(&alias_id)
        || (event == FO76_QUEST_EVENT_SCPT
            && event_data == FO76_QUEST_EVENT_REFERENCE3
            && player_event_consumer_alias_ids.contains(&alias_id))
}

/// `FNAM` bit `0x2` — xEdit/FO4 call this alias flag **Optional**. (The FO4
/// authoring schema names the same bit `NoStatsTracking`, which makes decoded
/// output read misleadingly; the raw bit is what matters here.)
const QUST_ALIAS_FLAG_OPTIONAL: u32 = 0x0000_0002;

/// True when the alias currently being walked is flagged Optional.
///
/// An Optional alias that fails to fill cannot abort a quest start — FO4 simply
/// leaves it empty. Only a NON-optional alias can silently kill the quest.
fn qust_alias_flags_are_optional(value: &FieldValue) -> bool {
    field_value_to_u32(value).is_some_and(|flags| flags & QUST_ALIAS_FLAG_OPTIONAL != 0)
}

pub(crate) fn qust_has_untranslatable_event_alias(record: &Record) -> bool {
    let preserved_event_alias_ids = qust_structurally_preserved_event_alias_ids(record);
    qust_has_untranslatable_event_alias_with_preserved(record, &preserved_event_alias_ids)
}

pub(crate) fn qust_has_untranslatable_event_alias_for_source(
    interner: &crate::sym::StringInterner,
    record: &Record,
) -> bool {
    let preserved_event_alias_ids = qust_preserved_event_alias_ids(interner, record);
    qust_has_untranslatable_event_alias_with_preserved(record, &preserved_event_alias_ids)
}

fn qust_has_untranslatable_event_alias_with_preserved(
    record: &Record,
    preserved_event_alias_ids: &[u32],
) -> bool {
    let player_event_consumer_alias_ids = qust_vmad_player_event_consumer_alias_ids(record);
    let remove_players_alias_ids = qust_vmad_remove_players_alias_ids(record);
    let reference1_event_alias_ids = qust_reference1_event_alias_ids(record);
    let mut current_alias_id = None;
    let mut current_alias_is_location = false;
    let mut current_alias_optional = false;
    let mut index = 0;

    while index < record.fields.len() {
        let entry = &record.fields[index];
        if QUST_ALIAS_ANCHOR_SIGS.iter().any(|sig| entry.sig.0 == *sig) {
            current_alias_is_location = entry.sig.0 == *b"ALLS";
            current_alias_id = (entry.sig.0 == *b"ALST")
                .then(|| field_value_to_u32(&entry.value))
                .flatten();
            current_alias_optional = false;
        } else if entry.sig.0 == *b"FNAM" {
            // `FNAM` follows its alias anchor and precedes that alias's ALFE/ALFD.
            current_alias_optional = qust_alias_flags_are_optional(&entry.value);
        } else if entry.sig.0 == *b"ALFE" {
            let Some(next) = record.fields.get(index + 1) else {
                return true;
            };
            let safe = if next.sig.0 != *b"ALFD" {
                false
            } else {
                let event = field_value_to_u32(&entry.value);
                let event_data = field_value_to_u32(&next.value);
                (current_alias_is_location
                    && event.zip(event_data).is_some_and(|(event, event_data)| {
                        qust_location_event_alias_is_proven(event, event_data)
                    }))
                    || current_alias_id
                        .zip(field_value_to_u32(&entry.value))
                        .zip(field_value_to_u32(&next.value))
                        .is_some_and(|((alias_id, event), event_data)| {
                            qust_event_alias_rewrites_to_reference1(
                                alias_id,
                                event,
                                event_data,
                                &reference1_event_alias_ids,
                            ) || qust_event_alias_is_proven(
                                alias_id,
                                event,
                                event_data,
                                preserved_event_alias_ids,
                            ) || qust_event_alias_rewrites_to_player(
                                alias_id,
                                event,
                                event_data,
                                &player_event_consumer_alias_ids,
                                &remove_players_alias_ids,
                            )
                        })
            };
            // An unresolvable event fill on an OPTIONAL alias is survivable: the
            // alias stays empty and the quest still starts. Disqualifying the
            // quest would skip its Story Manager node, and an event-scoped quest
            // with no SMQN can never start. `EN01_Misc` ("Uncle Sam") has event
            // fills only on the Optional aliases `NoteObject`/`NoteObjectStory`.
            if !safe && !current_alias_optional {
                return true;
            }
            index += 2;
            continue;
        } else if entry.sig.0 == *b"ALFD" && !current_alias_optional {
            return true;
        }
        index += 1;
    }
    false
}

pub(super) fn parse_qust_vmad_player_event_consumer_alias_ids(
    data: &[u8],
) -> Option<SmallVec<[u32; 4]>> {
    let version = qust_vmad_read_u16(data, 0)?;
    let object_format = qust_vmad_read_u16(data, 2)?;
    let script_count = qust_vmad_read_u16(data, 4)? as usize;
    if version != FO76_VMAD_VERSION || object_format != FO76_VMAD_OBJECT_FORMAT {
        return None;
    }

    let mut offset = 6;
    for _ in 0..script_count {
        qust_vmad_read_script(data, &mut offset, object_format)?;
    }
    let fragment_version = qust_vmad_read_u8_advance(data, &mut offset)?;
    if fragment_version != FO76_QUST_FRAGMENT_VERSION {
        return None;
    }
    let fragment_count = qust_vmad_read_u16_advance(data, &mut offset)? as usize;
    let fragment_script_name = qust_vmad_read_string(data, &mut offset)?;
    if !fragment_script_name.is_empty() {
        qust_vmad_advance(&mut offset, 1, data.len())?;
        let property_count = qust_vmad_read_u16_advance(data, &mut offset)? as usize;
        for _ in 0..property_count {
            qust_vmad_skip_property(data, &mut offset, object_format)?;
        }
    }
    for _ in 0..fragment_count {
        qust_vmad_advance(&mut offset, 9, data.len())?;
        qust_vmad_read_string(data, &mut offset)?;
        qust_vmad_read_string(data, &mut offset)?;
    }

    let alias_count = qust_vmad_read_u16_advance(data, &mut offset)? as usize;
    let mut player_event_consumer_alias_ids = SmallVec::new();
    for _ in 0..alias_count {
        let alias_offset = offset.checked_add(2)?;
        let alias_id = i16::from_le_bytes(
            data.get(alias_offset..alias_offset.checked_add(2)?)?
                .try_into()
                .ok()?,
        );
        qust_vmad_advance(&mut offset, 8, data.len())?;
        let alias_version = qust_vmad_read_u16_advance(data, &mut offset)?;
        let alias_object_format = qust_vmad_read_u16_advance(data, &mut offset)?;
        if alias_version != FO76_VMAD_ALIAS_VERSION
            || alias_object_format != FO76_VMAD_OBJECT_FORMAT
        {
            return None;
        }
        let alias_script_count = qust_vmad_read_u16_advance(data, &mut offset)? as usize;
        let mut is_player_event_consumer_alias = false;
        for _ in 0..alias_script_count {
            let script_name = qust_vmad_read_script(data, &mut offset, alias_object_format)?;
            is_player_event_consumer_alias |= is_player_event_consumer_script_name(script_name);
        }
        if is_player_event_consumer_alias && alias_id >= 0 {
            let alias_id = alias_id as u32;
            if !player_event_consumer_alias_ids.contains(&alias_id) {
                player_event_consumer_alias_ids.push(alias_id);
            }
        }
    }
    (offset == data.len()).then_some(player_event_consumer_alias_ids)
}

pub(super) fn is_player_event_consumer_script_name(script_name: &[u8]) -> bool {
    is_daim_script_name(script_name)
        || script_name.eq_ignore_ascii_case(b"W05_MQR_202P_PlayerScript")
        || script_name.eq_ignore_ascii_case(b"W05_MQR_PlayerVault79KeypadObjective")
}

pub(super) fn is_daim_script_name(script_name: &[u8]) -> bool {
    const PREFIX: &[u8] = b"DefaultAliasInventoryManagement";
    if script_name.len() < PREFIX.len() || !script_name[..PREFIX.len()].eq_ignore_ascii_case(PREFIX)
    {
        return false;
    }
    script_name.len() == PREFIX.len()
        || (script_name.len() == PREFIX.len() + 1
            && matches!(script_name[PREFIX.len()].to_ascii_uppercase(), b'A'..=b'M'))
}

pub(super) fn qust_vmad_read_script<'a>(
    data: &'a [u8],
    offset: &mut usize,
    object_format: u16,
) -> Option<&'a [u8]> {
    let script_name = qust_vmad_read_string(data, offset)?;
    qust_vmad_advance(offset, 1, data.len())?;
    let property_count = qust_vmad_read_u16_advance(data, offset)? as usize;
    for _ in 0..property_count {
        qust_vmad_skip_property(data, offset, object_format)?;
    }
    Some(script_name)
}

pub(super) fn qust_vmad_skip_property(
    data: &[u8],
    offset: &mut usize,
    object_format: u16,
) -> Option<()> {
    qust_vmad_read_string(data, offset)?;
    let property_type = qust_vmad_read_u8_advance(data, offset)?;
    qust_vmad_advance(offset, 1, data.len())?;
    qust_vmad_skip_property_value(data, offset, property_type, object_format)
}

pub(super) fn qust_vmad_skip_property_value(
    data: &[u8],
    offset: &mut usize,
    property_type: u8,
    object_format: u16,
) -> Option<()> {
    match property_type {
        0 | 6 => Some(()),
        1 => {
            if !matches!(object_format, 1 | 2) {
                return None;
            }
            qust_vmad_advance(offset, 8, data.len())
        }
        2 => qust_vmad_read_string(data, offset).map(|_| ()),
        3 | 4 => qust_vmad_advance(offset, 4, data.len()),
        5 => qust_vmad_advance(offset, 1, data.len()),
        7 => qust_vmad_skip_struct(data, offset, object_format),
        11 => {
            let count = qust_vmad_read_nonnegative_count(data, offset)?;
            qust_vmad_advance(offset, count.checked_mul(8)?, data.len())
        }
        12 => {
            let count = qust_vmad_read_nonnegative_count(data, offset)?;
            for _ in 0..count {
                qust_vmad_read_string(data, offset)?;
            }
            Some(())
        }
        13 | 14 => {
            let count = qust_vmad_read_nonnegative_count(data, offset)?;
            qust_vmad_advance(offset, count.checked_mul(4)?, data.len())
        }
        15 => {
            let count = qust_vmad_read_nonnegative_count(data, offset)?;
            qust_vmad_advance(offset, count, data.len())
        }
        16 => qust_vmad_advance(offset, 4, data.len()),
        17 => {
            let count = qust_vmad_read_nonnegative_count(data, offset)?;
            for _ in 0..count {
                qust_vmad_skip_struct(data, offset, object_format)?;
            }
            Some(())
        }
        _ => None,
    }
}

pub(super) fn qust_vmad_skip_struct(
    data: &[u8],
    offset: &mut usize,
    object_format: u16,
) -> Option<()> {
    let count = qust_vmad_read_nonnegative_count(data, offset)?;
    for _ in 0..count {
        qust_vmad_read_string(data, offset)?;
        let member_type = qust_vmad_read_u8_advance(data, offset)?;
        qust_vmad_advance(offset, 1, data.len())?;
        qust_vmad_skip_property_value(data, offset, member_type, object_format)?;
    }
    Some(())
}

pub(super) fn qust_vmad_read_nonnegative_count(data: &[u8], offset: &mut usize) -> Option<usize> {
    let count = qust_vmad_read_u32_advance(data, offset)? as i32;
    usize::try_from(count).ok()
}

pub(super) fn qust_vmad_read_string<'a>(data: &'a [u8], offset: &mut usize) -> Option<&'a [u8]> {
    let len = qust_vmad_read_u16_advance(data, offset)? as usize;
    let end = offset.checked_add(len)?;
    let value = data.get(*offset..end)?;
    *offset = end;
    Some(value)
}

pub(super) fn qust_vmad_read_u8_advance(data: &[u8], offset: &mut usize) -> Option<u8> {
    let value = *data.get(*offset)?;
    *offset = offset.checked_add(1)?;
    Some(value)
}

pub(super) fn qust_vmad_read_u16(data: &[u8], offset: usize) -> Option<u16> {
    Some(u16::from_le_bytes(
        data.get(offset..offset.checked_add(2)?)?.try_into().ok()?,
    ))
}

pub(super) fn qust_vmad_read_u16_advance(data: &[u8], offset: &mut usize) -> Option<u16> {
    let value = qust_vmad_read_u16(data, *offset)?;
    *offset = offset.checked_add(2)?;
    Some(value)
}

pub(super) fn qust_vmad_read_u32_advance(data: &[u8], offset: &mut usize) -> Option<u32> {
    let value = u32::from_le_bytes(data.get(*offset..offset.checked_add(4)?)?.try_into().ok()?);
    *offset = offset.checked_add(4)?;
    Some(value)
}

pub(super) fn qust_vmad_advance(offset: &mut usize, amount: usize, len: usize) -> Option<()> {
    let next = offset.checked_add(amount)?;
    if next > len {
        return None;
    }
    *offset = next;
    Some(())
}
