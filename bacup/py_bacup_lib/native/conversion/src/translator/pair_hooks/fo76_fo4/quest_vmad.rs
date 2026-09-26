use super::*;
// Source ALID names decode as interned strings; hand-built records use raw bytes.
use super::expedition_aliases::field_value_matches_zstring;

pub(super) const FO76_QUEST_EVENT_SCPT: u32 = 0x5450_4353;
/// FO76's player-connect event, fired when a player joins a world. FO4 has no
/// equivalent; `B21:B21_TFA_PlayerConnectBridge` supplies it on new game and
/// every load, and the converted `B21_SMEvent_PCON` keyword carries it.
pub(super) const FO76_QUEST_EVENT_PCON: u32 = u32::from_le_bytes(*b"PCON");
pub(super) const FO4_QUEST_EVENT_LOCATION1: u32 = 12_620;
/// Alias event-data slots are two-character ASCII codes: `L1` 12620,
/// `P1` 12624, `R1` 12626, `R2` 12882, `R3` 13138.
pub(super) const FO76_QUEST_EVENT_PLAYER1: u32 = 12_624;
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
/// Both games name a random encounter's trigger alias `TRIGGER`; FO4's
/// `REScript`/`QF_REObject*` fragments bind it as `Alias_TRIGGER`.
const FO76_RANDOM_ENCOUNTER_TRIGGER_ALIAS_NAME: &str = "TRIGGER";
pub(super) const FO76_VMAD_VERSION: u16 = 6;
pub(super) const FO76_VMAD_OBJECT_FORMAT: u16 = 2;
pub(super) const FO76_VMAD_ALIAS_VERSION: u16 = 6;
const FO76_W05_MQS_205_FORM_ID: u32 = 0x0041_CB6D;
const FO76_DEFAULT_QUEST_SHUTDOWN_SCRIPT: &[u8] = b"DefaultQuestShutdownScript";
pub(super) const FO76_RD01_ENC02_FORM_ID: u32 = 0x0078_F7A1;
const FO76_RD01_ENC02_SCRIPT: &[u8] = b"Raids:RD01:Enc02:QuestScript";
pub(super) const FO76_EN07_NUKE_MASTER_FORM_ID: u32 = 0x002D_0F67;
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

pub(super) fn strip_en07_invalid_code_topic_bindings(
    interner: &crate::sym::StringInterner,
    record: &mut Record,
) {
    if record.sig.0 != *b"QUST"
        || record.form_key.local & 0x00FF_FFFF != FO76_EN07_NUKE_MASTER_FORM_ID
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
        if let FieldValue::Bytes(bytes) = &mut entry.value {
            if let Some(repaired) = strip_en07_invalid_code_topics(bytes) {
                *bytes = SmallVec::from_vec(repaired);
            }
        }
    }
}

fn strip_en07_invalid_code_topics(data: &[u8]) -> Option<Vec<u8>> {
    if qust_vmad_read_u16(data, 0)? != FO76_VMAD_VERSION
        || qust_vmad_read_u16(data, 2)? != FO76_VMAD_OBJECT_FORMAT
    {
        return None;
    }
    let script_count = qust_vmad_read_u16(data, 4)?;
    let mut offset = 6;
    let mut edits = Vec::new();
    for _ in 0..script_count {
        let is_master = qust_vmad_read_string(data, &mut offset)?
            .eq_ignore_ascii_case(b"EN07_NukeMasterScript");
        qust_vmad_advance(&mut offset, 1, data.len())?;
        let property_count = qust_vmad_read_u16_advance(data, &mut offset)?;
        for _ in 0..property_count {
            let is_code_data =
                qust_vmad_read_string(data, &mut offset)?.eq_ignore_ascii_case(b"CodeData");
            let property_type = qust_vmad_read_u8_advance(data, &mut offset)?;
            qust_vmad_advance(&mut offset, 1, data.len())?;
            if !is_master || !is_code_data || property_type != 17 {
                qust_vmad_skip_property_value(data, &mut offset, property_type, 2)?;
                continue;
            }
            let struct_count = qust_vmad_read_nonnegative_count(data, &mut offset)?;
            for _ in 0..struct_count {
                let count_offset = offset;
                let member_count = qust_vmad_read_nonnegative_count(data, &mut offset)?;
                let mut removed = 0;
                for _ in 0..member_count {
                    let member_start = offset;
                    let is_invalid_topic = qust_vmad_read_string(data, &mut offset)?
                        .eq_ignore_ascii_case(b"InvalidCodeTopic");
                    let member_type = qust_vmad_read_u8_advance(data, &mut offset)?;
                    qust_vmad_advance(&mut offset, 1, data.len())?;
                    qust_vmad_skip_property_value(data, &mut offset, member_type, 2)?;
                    // FO4 rejects the entire array when these unused Topics fail to bind.
                    if is_invalid_topic && member_type == 1 {
                        edits.push((member_start, offset, Vec::new()));
                        removed += 1;
                    }
                }
                if removed > 0 {
                    edits.push((
                        count_offset,
                        count_offset + 4,
                        ((member_count - removed) as u32).to_le_bytes().to_vec(),
                    ));
                }
            }
        }
    }
    if edits.is_empty() {
        return None;
    }
    edits.sort_by_key(|edit| edit.0);
    let mut repaired = data.to_vec();
    for (start, end, replacement) in edits.into_iter().rev() {
        repaired.splice(start..end, replacement);
    }
    Some(repaired)
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

pub(super) fn normalize_re_alias_vmad_properties(
    interner: &crate::sym::StringInterner,
    record: &mut Record,
) {
    if record.sig.0 != *b"QUST" {
        return;
    }

    let source_quest = interner
        .resolve(record.form_key.plugin)
        .is_some_and(|plugin| plugin.eq_ignore_ascii_case(FO76_MASTER_NAME))
        .then_some(record.form_key.local & 0x00FF_FFFF);
    for entry in &mut record.fields {
        if entry.sig.0 != *b"VMAD" {
            continue;
        }
        let FieldValue::Bytes(bytes) = &mut entry.value else {
            continue;
        };
        let Some(normalized) = normalize_re_alias_vmad_bytes(bytes, source_quest) else {
            continue;
        };
        if normalized.as_slice() != bytes.as_slice() {
            *bytes = SmallVec::from_vec(normalized);
        }
    }
}

pub(super) fn normalize_w05_quest_distance_script(
    interner: &crate::sym::StringInterner,
    record: &mut Record,
) {
    if record.sig.0 != *b"QUST"
        || !matches!(
            record.form_key.local & 0x00FF_FFFF,
            0x40D28D | 0x54EDB9 | 0x53AF40
        )
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
        if let Some(normalized) = normalize_w05_quest_distance_bytes(bytes) {
            *bytes = SmallVec::from_vec(normalized);
        }
    }
}

fn normalize_w05_quest_distance_bytes(data: &[u8]) -> Option<Vec<u8>> {
    const SOURCE: &[u8] = b"DefaultQuestDistanceCheckScript";
    const TARGET: &[u8] = b"B21_W05QuestDistanceCheckScript";
    if qust_vmad_read_u16(data, 0)? != 6 || qust_vmad_read_u16(data, 2)? != 2 {
        return None;
    }
    let script_count = qust_vmad_read_u16(data, 4)?;
    let mut offset = 6;
    let mut replacement = None;
    for _ in 0..script_count {
        let start = offset;
        let name = qust_vmad_read_script(data, &mut offset, 2)?;
        if name.eq_ignore_ascii_case(SOURCE) {
            if replacement.is_some() {
                return None;
            }
            replacement = Some(start..start + 2 + name.len());
        }
    }
    let span = replacement?;
    let mut output = data[..span.start].to_vec();
    output.extend_from_slice(&(TARGET.len() as u16).to_le_bytes());
    output.extend_from_slice(TARGET);
    output.extend_from_slice(&data[span.end..]);
    Some(output)
}

pub(super) fn normalize_w05_perk_fragment_vmad(
    interner: &crate::sym::StringInterner,
    record: &mut Record,
) {
    let local_id = record.form_key.local & 0x00FF_FFFF;
    if record.sig.0 != *b"PERK"
        || !matches!(
            local_id,
            0x593DD5 | 0x5614E5 | 0x41B765 | 0x40483A | 0x40483B | 0x59276C | 0x5A11A2
        )
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
        if let Some(normalized) = normalize_w05_perk_fragment_bytes(bytes, local_id) {
            *bytes = SmallVec::from_vec(normalized);
        }
    }
}

fn normalize_w05_perk_fragment_bytes(data: &[u8], local_id: u32) -> Option<Vec<u8>> {
    if qust_vmad_read_u16(data, 0)? != 6 || qust_vmad_read_u16(data, 2)? != 2 {
        return None;
    }
    let script_count = qust_vmad_read_u16(data, 4)?;
    let mut offset = 6;
    for _ in 0..script_count {
        qust_vmad_read_script(data, &mut offset, 2)?;
    }
    let version_offset = offset;
    if qust_vmad_read_u8_advance(data, &mut offset)? != 4 {
        return None;
    }
    let script = qust_vmad_read_script(data, &mut offset, 2)?;
    let script_name = std::str::from_utf8(script).ok()?.to_ascii_lowercase();
    let indices: &[u32] = match local_id {
        0x40483A | 0x40483B => &[1],
        0x5A11A2 => &[0, 2],
        _ => &[0],
    };
    if !script_name.starts_with("fragments:perks:prkf_w05_")
        || !script_name.ends_with(&format!("_{local_id:08x}"))
        || qust_vmad_read_u16_advance(data, &mut offset)? as usize != indices.len()
    {
        return None;
    }
    for index in indices {
        if qust_vmad_read_u32_advance(data, &mut offset)? != *index
            || qust_vmad_read_u8_advance(data, &mut offset)? != 1
            || !qust_vmad_read_string(data, &mut offset)?.eq_ignore_ascii_case(script)
            || !qust_vmad_read_string(data, &mut offset)?
                .eq_ignore_ascii_case(format!("Fragment_Entry_{index:02}").as_bytes())
        {
            return None;
        }
    }
    if data.get(offset..)? != [3, 0] {
        return None;
    }
    let mut normalized = data[..offset].to_vec();
    normalized[version_offset] = 3;
    Some(normalized)
}

fn normalize_re_alias_vmad_bytes(data: &[u8], source_quest: Option<u32>) -> Option<Vec<u8>> {
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
        let alias_id = qust_vmad_read_u16(data, offset.checked_add(2)?)?;
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
                source_quest.map(|quest| (quest, alias_id)),
            )?);
        }
    }

    (offset == data.len()).then_some(normalized)
}

fn normalize_re_alias_script(
    data: &[u8],
    offset: &mut usize,
    object_format: u16,
    source_alias: Option<(u32, u16)>,
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

    if script_name.eq_ignore_ascii_case(b"DefaultShowMessageOnActivateAlias") {
        let mut normalized = Vec::new();
        qust_vmad_write_string(&mut normalized, b"B21_ShowMessageOnActivateAlias")?;
        normalized.extend_from_slice(&data[property_count_offset - 1..*offset]);
        return Some(normalized);
    }

    let repair_clue_cutoff = match source_alias {
        Some((0x003F_28C3, 22..=24)) => script_name.eq_ignore_ascii_case(b"DefaultAliasOnRead"),
        Some((0x003F_28C3, 0)) => [
            b"DefaultAliasInventoryManagementC".as_slice(),
            b"DefaultAliasInventoryManagementD".as_slice(),
            b"DefaultAliasInventoryManagementE".as_slice(),
        ]
        .iter()
        .any(|name| script_name.eq_ignore_ascii_case(name)),
        _ => false,
    };
    if repair_clue_cutoff {
        let mut normalized = data[script_start..property_count_offset + 2].to_vec();
        for property in &properties {
            normalized.extend_from_slice(property.raw);
            if property.name.eq_ignore_ascii_case(b"TurnOffStage")
                && property.property_type == 3
                && property.value == 430_i32.to_le_bytes()
            {
                // Wrong-password stages 431/432 must not disable unread clues.
                let value_start = normalized.len() - 4;
                normalized[value_start..].copy_from_slice(&449_i32.to_le_bytes());
            }
        }
        return Some(normalized);
    }

    if !is_re_alias_script_name(script_name) {
        if let Some((source_name, target_name)) = default_alias_player_filter(script_name) {
            let has_target = properties
                .iter()
                .any(|property| property.name.eq_ignore_ascii_case(target_name));
            let requires_item = script_name.eq_ignore_ascii_case(b"DefaultAliasOnActivate")
                && properties.iter().any(|property| {
                    property.name.eq_ignore_ascii_case(b"ItemRequired")
                        && property.property_type == 1
                });
            let mut normalized = if requires_item {
                let mut header = Vec::new();
                qust_vmad_write_string(&mut header, b"B21_ActivateAliasWithRequiredItem")?;
                header.push(data[property_count_offset - 1]);
                header
            } else {
                data[script_start..property_count_offset].to_vec()
            };
            let mut output_properties = Vec::new();
            let mut output_count = 0_u16;
            for property in &properties {
                if property.name.eq_ignore_ascii_case(source_name) && property.property_type == 3 {
                    let mode = i32::from_le_bytes(property.value.try_into().ok()?);
                    // Source modes: 0 any, 1 player, 2 active player and team, 3 active
                    // player. Every -1 in the master pairs with an alias allow-list,
                    // which FO4 ignores while the player filter is on.
                    if (-1..=3).contains(&mode) {
                        if !has_target {
                            qust_vmad_write_string(&mut output_properties, target_name)?;
                            output_properties.extend_from_slice(&[
                                5,
                                property.flags,
                                u8::from(mode > 0),
                            ]);
                            output_count += 1;
                        }
                        continue;
                    }
                }
                output_properties.extend_from_slice(property.raw);
                output_count += 1;
            }
            if source_alias == Some((0x0042_F31B, 40))
                && script_name.eq_ignore_ascii_case(b"DefaultAliasOnContainerChangedTo")
                && properties.iter().any(|property| {
                    property.name.eq_ignore_ascii_case(b"StageToSet")
                        && property.property_type == 3
                        && property.value == 5200_i32.to_le_bytes()
                })
                && properties.iter().any(|property| {
                    property.name.eq_ignore_ascii_case(b"PrereqStage")
                        && property.property_type == 3
                        && property.value == 5100_i32.to_le_bytes()
                })
                && !properties
                    .iter()
                    .any(|property| property.name.eq_ignore_ascii_case(b"TurnOffStage"))
            {
                // A chem pickup after the deadline must not turn failure into success.
                qust_vmad_write_string(&mut output_properties, b"TurnOffStage")?;
                output_properties.extend_from_slice(&[3, 1]);
                output_properties.extend_from_slice(&5300_i32.to_le_bytes());
                output_count += 1;
            }
            normalized.extend_from_slice(&output_count.to_le_bytes());
            normalized.extend_from_slice(&output_properties);
            return Some(normalized);
        }
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

fn default_alias_player_filter(script_name: &[u8]) -> Option<(&'static [u8], &'static [u8])> {
    if script_name.eq_ignore_ascii_case(b"DefaultAliasOnHit") {
        Some((b"PlayerHitType", b"PlayerHitOnly"))
    } else if script_name.eq_ignore_ascii_case(b"DefaultAliasOnOpen") {
        Some((b"PlayerActivateType", b"PlayerTriggerOnly"))
    } else if script_name.eq_ignore_ascii_case(b"DefaultAliasOnActivate") {
        Some((b"PlayerActivateType", b"PlayerActivateOnly"))
    } else if script_name.eq_ignore_ascii_case(b"DefaultAliasOnContainerChangedTo") {
        Some((b"PlayerPickupType", b"PlayerPickupOnly"))
    } else if script_name.eq_ignore_ascii_case(b"DefaultAliasOnTriggerEnter")
        // Declaration-only FO76 children of FO4's stock trigger script.
        || script_name.eq_ignore_ascii_case(b"DefaultAliasOnTriggerEnterA")
        || script_name.eq_ignore_ascii_case(b"DefaultAliasOnTriggerEnterB")
        || script_name.eq_ignore_ascii_case(b"DefaultAliasOnTriggerLeave")
    {
        Some((b"PlayerTriggerType", b"PlayerTriggerOnly"))
    } else {
        None
    }
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

/// Script-event `Reference3` player fills proven per quest. `R3` carries the
/// player for 762 of FO76's 764 aliases that fill from it, but `Attacker` and
/// `MTNS02_WendigoAlias` fill from it too, so the slot alone is not proof.
const SCPT_REFERENCE3_PLAYER_ALIASES: &[(u32, u32)] = &[
    // RSVP03_MiscPointer `currentPlayer`. Its only producer is the placed
    // `RSVP03_MiscPointer_StartMarker` 4FC085, a `DefaultRefOnDistanceSendEvent`
    // that fires within 4000 units of the player; the FO4 send passes no player
    // reference, so the alias can only be the player.
    (0x004F_C083, 2),
    // Location "misc pointer" quests with the same shape: each one's only
    // producer is a placed `DefaultRefOnDistanceSendEvent` start marker whose
    // `MyStoryManagerKeyword` is the quest's SMQN selector.
    (0x0046_ED03, 2), // Tutorial_MineOre `currentPlayer`, markers 46ED04/46ED07
    (0x004F_7BF4, 2), // LC010_BeckleyMisc `currentPlayer`, marker 4F7BF6
    (0x004F_7BFA, 2), // LC047_FloodedTrainyardMisc `currentPlayer`, marker 4F7BF8
    (0x004F_7BFD, 2), // LC049_GeneralsSteakhouseMisc `currentPlayer`, marker 4F7C01
    (0x004F_852A, 2), // LC090_MonongahMineMisc `currentPlayer`, marker 4F8529
    (0x004F_852F, 2), // LC142_NewGadMisc `currentPlayer`, marker 4F8530
    (0x004F_8A93, 2), // LC154_TylerCountyFairgroundsMisc `LC154_Player`, marker 4F8A92
    (0x004F_900F, 2), // LC139_SummersvilleMisc `LC139_Player`, marker 4F900E
    (0x0050_DE51, 2), // LC178_WatogaCivicCenterMisc `currentPlayer`, marker 50DE56
    (0x0050_EAF6, 1), // FF20_MorgantownMisc `currentPlayer`, marker 50EAFD
    (0x0050_F989, 2), // LC172_WatogaEmergencyMisc `currentPlayer`, marker 50F988
    // SFL02_Track_RadioQuest `SFL02RadioPlayer` and SFL02_Track_VertibotQuest
    // `SFL02VertibotPlayers`. Their start selectors 32BB60 and 32BB5C are bound
    // only on SFL02_Track's stage fragments, so the sender is the player's own
    // Tracking Unknowns quest.
    (0x0001_8ECF, 0),
    (0x0032_BB59, 1),
];

pub(super) fn qust_vmad_player_event_consumer_alias_ids(record: &Record) -> SmallVec<[u32; 4]> {
    let quest = record.form_key.local & 0x00FF_FFFF;
    let mut alias_ids: SmallVec<[u32; 4]> = SCPT_REFERENCE3_PLAYER_ALIASES
        .iter()
        .filter(|(quest_id, _)| *quest_id == quest)
        .map(|(_, alias_id)| *alias_id)
        .collect();
    let mut vmad_fields = record.fields.iter().filter(|entry| entry.sig.0 == *b"VMAD");
    let Some(vmad) = vmad_fields.next() else {
        return alias_ids;
    };
    if vmad_fields.next().is_some() {
        return alias_ids;
    }
    let FieldValue::Bytes(bytes) = &vmad.value else {
        return alias_ids;
    };
    for alias_id in parse_qust_vmad_player_event_consumer_alias_ids(bytes).unwrap_or_default() {
        if !alias_ids.contains(&alias_id) {
            alias_ids.push(alias_id);
        }
    }
    alias_ids
}

pub(super) fn qust_vmad_remove_players_alias_ids(
    interner: &crate::sym::StringInterner,
    record: &Record,
) -> SmallVec<[u32; 4]> {
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
        if qust_alias_is_owning_player_scpt_reference3(interner, record, alias_id)
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

fn qust_alias_is_owning_player_scpt_reference3(
    interner: &crate::sym::StringInterner,
    record: &Record,
    alias_id: u32,
) -> bool {
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
                has_owning_player_name =
                    field_value_matches_zstring(interner, &entry.value, "owningPlayer");
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

// MineEntranceClosed sends Player as Ref1 and its linked mine as Ref2. The cut
// CB00 quest asks for that Player alias from the nonexistent SCPT Ref3.
pub(super) fn qust_reference1_event_alias_ids(
    interner: &crate::sym::StringInterner,
    record: &Record,
) -> SmallVec<[u32; 1]> {
    if qust_is_cb00_mine_repair(interner, record) {
        SmallVec::from_slice(&[0])
    } else {
        SmallVec::new()
    }
}

pub(super) fn qust_preserved_event_alias_ids(
    interner: &crate::sym::StringInterner,
    record: &Record,
) -> SmallVec<[u32; 1]> {
    let preserved = qust_structurally_preserved_event_alias_ids(interner, record);
    if !preserved.is_empty() {
        preserved
    } else if qust_is_tw002_event_trigger(interner, record) {
        SmallVec::from_slice(&[FO76_TW002_EVENT_TRIGGER_ALIAS_ID])
    } else {
        SmallVec::new()
    }
}

fn qust_structurally_preserved_event_alias_ids(
    interner: &crate::sym::StringInterner,
    record: &Record,
) -> SmallVec<[u32; 1]> {
    if qust_is_cb00_mine_repair(interner, record) {
        SmallVec::from_slice(&[1])
    } else if qust_is_workshop_clear(interner, record) {
        SmallVec::from_slice(&[0])
    } else if qust_is_workshop_vertibird(interner, record) {
        SmallVec::from_slice(&[0])
    } else {
        let workshop_attack = qust_workshop_attack_event_alias_ids(interner, record);
        if !workshop_attack.is_empty() {
            return workshop_attack;
        }
        qust_random_encounter_trigger_alias_ids(interner, record)
    }
}

/// The `TRIGGER` alias of a FO76 random encounter, which keeps its event fill.
///
/// FO4's own `RETriggerScript` sends the trigger as Reference1
/// (`EncounterType.SendStoryEventAndWait(GetCurrentLocation(), Self, None, …)`),
/// the same shape FO76 authored, so the fill survives untouched. It is the only
/// event fill in the family: every other alias — center marker, travel markers,
/// containers, spawn points — fills `LinkedFrom` that trigger, so dropping this
/// one fill left the whole encounter unresolvable and disqualified its quest.
fn qust_random_encounter_trigger_alias_ids(
    interner: &crate::sym::StringInterner,
    record: &Record,
) -> SmallVec<[u32; 1]> {
    if !interner
        .resolve(record.form_key.plugin)
        .is_some_and(|plugin| plugin.eq_ignore_ascii_case(FO76_MASTER_NAME))
        || !qust_eid_lower(interner, record)
            .is_some_and(|editor_id| qust_eid_is_random_encounter(&editor_id))
        || qust_event_type(record) != Some(FO76_QUEST_EVENT_SCPT)
    {
        return SmallVec::new();
    }
    qust_alias_id_with_name(interner, record, FO76_RANDOM_ENCOUNTER_TRIGGER_ALIAS_NAME)
        .filter(|alias_id| {
            qust_alias_has_event_fill(
                record,
                *alias_id,
                FO76_QUEST_EVENT_SCPT,
                FO4_QUEST_EVENT_REFERENCE1,
            )
        })
        .map(|alias_id| SmallVec::from_slice(&[alias_id]))
        .unwrap_or_default()
}

/// FO76 names wilderness encounters `RE_*` or `<region>_RE_*`: `RE_TravelKMK01`,
/// `W05_RE_Camp_JP17_Campers`, `BS_RE_AssaultCMB03`, `Burn_RE_Object_LD01`,
/// `Storm_RE_SceneRK01`.
///
/// Expedition hub events (`XPD_HubRE_*`) and scoreboard quests (`SCORE_*`) are
/// deliberately outside the family: no RE trigger sends them, so their aliases
/// have no Reference1 to fill from.
fn qust_eid_is_random_encounter(editor_id: &str) -> bool {
    editor_id.starts_with("re_") || editor_id.contains("_re_")
}

fn qust_event_type(record: &Record) -> Option<u32> {
    record
        .fields
        .iter()
        .find(|entry| entry.sig.0 == *b"ENAM")
        .and_then(|entry| field_value_to_u32(&entry.value))
}

fn qust_alias_id_with_name(
    interner: &crate::sym::StringInterner,
    record: &Record,
    expected: &str,
) -> Option<u32> {
    let mut current_alias_id = None;
    for entry in &record.fields {
        if QUST_ALIAS_ANCHOR_SIGS.iter().any(|sig| entry.sig.0 == *sig) {
            current_alias_id = (entry.sig.0 == *b"ALST")
                .then(|| field_value_to_u32(&entry.value))
                .flatten();
        } else if entry.sig.0 == *b"ALID"
            && current_alias_id.is_some()
            && field_value_matches_zstring(interner, &entry.value, expected)
        {
            return current_alias_id;
        }
    }
    None
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

fn qust_alias_has_name(
    interner: &crate::sym::StringInterner,
    record: &Record,
    alias_id: u32,
    expected: &str,
) -> bool {
    let mut current_alias_id = None;
    for entry in &record.fields {
        if QUST_ALIAS_ANCHOR_SIGS.iter().any(|sig| entry.sig.0 == *sig) {
            current_alias_id = (entry.sig.0 == *b"ALST")
                .then(|| field_value_to_u32(&entry.value))
                .flatten();
        } else if current_alias_id == Some(alias_id) && entry.sig.0 == *b"ALID" {
            return field_value_matches_zstring(interner, &entry.value, expected);
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

fn qust_is_cb00_mine_repair(interner: &crate::sym::StringInterner, record: &Record) -> bool {
    record.form_key.local == FO76_CB00_MINE_REPAIR_FORM_ID
        && qust_alias_has_name(interner, record, 0, "Player")
        && qust_alias_has_name(interner, record, 1, "MINE")
}

fn qust_is_workshop_vertibird(interner: &crate::sym::StringInterner, record: &Record) -> bool {
    record.form_key.local == FO76_SQ_WORKSHOP_VERTIBIRD_FORM_ID
        && qust_alias_has_name(interner, record, 0, "AttackTarget")
}

fn qust_is_workshop_clear(interner: &crate::sym::StringInterner, record: &Record) -> bool {
    record.form_key.local == FO76_GQ_WORKSHOP_CLEAR_FORM_ID
        && qust_alias_has_name(interner, record, 0, "Workshop")
        && qust_alias_has_event_fill(record, 0, FO76_QUEST_EVENT_SCPT, FO4_QUEST_EVENT_REFERENCE1)
}

/// The workshop-attack family's workshop alias, which keeps its event fill.
///
/// FO4's own `WorkshopParentScript.TriggerAttack` sends
/// `WorkshopEventAttack.SendStoryEventAndWait(workshopRef.myLocation, strength,
/// workshopRef)`, so the workshop arrives as Reference1 — exactly the shape
/// FO76 authored, and the same reason `GQ_WorkshopClear` keeps its own. Without
/// it the whole chain collapses: `AttackLocation` fills from this alias and
/// `CenterMarker` fills from `AttackLocation`, and neither is Optional.
const FO76_GQ_WORKSHOP_ATTACK_EVENT_ALIASES: [(u32, u32, &str); 3] = [
    (0x0001_B46A, 23, "WorkshopFromEvent"),
    (0x0001_1CCC, 12, "WorkshopFromEvent"),
    (0x0000_9179, 1, "Workshop"),
];

fn qust_workshop_attack_event_alias_ids(
    interner: &crate::sym::StringInterner,
    record: &Record,
) -> SmallVec<[u32; 1]> {
    // The source-plugin guard belongs to the callers: this runs against a fresh
    // interner too, where no plugin name resolves. The quest FormID, the alias
    // id, its name and the exact event fill are the guard.
    let local = record.form_key.local & 0x00FF_FFFF;
    for (form_id, alias_id, alias_name) in FO76_GQ_WORKSHOP_ATTACK_EVENT_ALIASES {
        if local == form_id
            && qust_alias_has_name(interner, record, alias_id, alias_name)
            && qust_alias_has_event_fill(
                record,
                alias_id,
                FO76_QUEST_EVENT_SCPT,
                FO4_QUEST_EVENT_REFERENCE1,
            )
        {
            return SmallVec::from_slice(&[alias_id]);
        }
    }
    SmallVec::new()
}

fn qust_is_tw002_event_trigger(interner: &crate::sym::StringInterner, record: &Record) -> bool {
    interner
        .resolve(record.form_key.plugin)
        .is_some_and(|plugin| plugin.eq_ignore_ascii_case(FO76_MASTER_NAME))
        && record.form_key.local & 0x00FF_FFFF == FO76_TW002_FORM_ID
        && qust_eid_lower(interner, record).as_deref() == Some(FO76_TW002_EID)
        && qust_alias_has_name(
            interner,
            record,
            FO76_TW002_EVENT_TRIGGER_ALIAS_ID,
            "EventTrigger",
        )
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
        || qust_event_alias_is_player_connect_subject(event, event_data)
}

/// The player-connect event's subject is the connecting player, so a `P1` fill
/// on it names exactly one actor in a single-player game.
///
/// Measured across the FO76 master: 58 aliases fill from `PCON`, 57 of them on
/// `P1` and every one of those named `Player`, `Alias_Player`, `currentPlayer`
/// or `PlayerAlias`. (The 58th fills `R1` and is left to the Reference1 path.)
/// Without this, `classify_quest` rejects the owning quest as
/// `unsupported_quest` because the fill is unproven and the alias is not
/// Optional, and an event-scoped quest whose `SMQN` was never emitted can never
/// start. That is what left 29 of the 32 `DefaultQuestlineRestartScript`
/// questline-restart quests unroutable.
pub(super) fn qust_event_alias_is_player_connect_subject(event: u32, event_data: u32) -> bool {
    event == FO76_QUEST_EVENT_PCON && event_data == FO76_QUEST_EVENT_PLAYER1
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

#[cfg(test)]
pub(crate) fn qust_has_untranslatable_event_alias(record: &Record) -> bool {
    // Test records spell alias names as raw bytes, so no interned names resolve.
    let interner = crate::sym::StringInterner::new();
    let preserved_event_alias_ids = qust_structurally_preserved_event_alias_ids(&interner, record);
    qust_has_untranslatable_event_alias_with_preserved(
        &interner,
        record,
        &preserved_event_alias_ids,
    )
}

pub(crate) fn qust_has_untranslatable_event_alias_for_source(
    interner: &crate::sym::StringInterner,
    record: &Record,
) -> bool {
    let preserved_event_alias_ids = qust_preserved_event_alias_ids(interner, record);
    qust_has_untranslatable_event_alias_with_preserved(interner, record, &preserved_event_alias_ids)
}

fn qust_has_untranslatable_event_alias_with_preserved(
    interner: &crate::sym::StringInterner,
    record: &Record,
    preserved_event_alias_ids: &[u32],
) -> bool {
    let player_event_consumer_alias_ids = qust_vmad_player_event_consumer_alias_ids(record);
    let remove_players_alias_ids = qust_vmad_remove_players_alias_ids(interner, record);
    let reference1_event_alias_ids = qust_reference1_event_alias_ids(interner, record);
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

/// FO4's stock `DefaultRef` family names its quest property `MyQuest` and its
/// actor filters `Player*Only` (Bool); FO76's twins use `MyUniqueQuest` and a
/// `Player*Type` mode enum. A placed reference carries FO76's names through
/// conversion unchanged, so `DefaultRef.TryToSetStage` resolves no quest and
/// the trigger, activator or container never sets its stage — silently, because
/// a missing property only warns at load. The QUST **alias** path already
/// renames its own filters in `default_alias_player_filter`; record-scope
/// bindings on REFR/ACTI/TERM/KEYM had no equivalent.
pub(super) fn normalize_placed_default_ref_properties(
    interner: &crate::sym::StringInterner,
    record: &mut Record,
) {
    // The rename is a measured no-op in the emitted plugin even though the walker
    // is unit-tested against the real byte layout, so each record that carries an
    // FO76 property name reports what this pass saw. `pre_translate` forwards
    // `record.warnings` into the run report: no `placed_default_ref` line for
    // REFR 51D8D7 means this function never sees the record that gets emitted,
    // and `renamed=false` means the walker rejected bytes it accepts in tests.
    let mut diagnostics: Vec<String> = Vec::new();
    for entry in &mut record.fields {
        if entry.sig.0 != *b"VMAD" {
            continue;
        }
        let FieldValue::Bytes(bytes) = &mut entry.value else {
            continue;
        };
        let carries_fo76_name = placed_default_ref_fo76_property_present(bytes);
        let Some(normalized) = normalize_placed_default_ref_bytes(bytes) else {
            if carries_fo76_name {
                diagnostics.push(format!(
                    "placed_default_ref:{:06X}:walker_rejected:len={}",
                    record.form_key.local,
                    bytes.len()
                ));
            }
            continue;
        };
        let changed = normalized.as_slice() != bytes.as_slice();
        if changed {
            *bytes = SmallVec::from_vec(normalized);
        }
        if carries_fo76_name {
            diagnostics.push(format!(
                "placed_default_ref:{:06X}:renamed={changed}",
                record.form_key.local
            ));
        }
    }
    for diagnostic in diagnostics {
        record.warnings.push(interner.intern(&diagnostic));
    }
}

fn placed_default_ref_fo76_property_present(data: &[u8]) -> bool {
    [
        b"MyUniqueQuest".as_slice(),
        b"PlayerTriggerType".as_slice(),
        b"PlayerActivateType".as_slice(),
    ]
    .iter()
    .any(|name| {
        data.windows(name.len())
            .any(|window| window.eq_ignore_ascii_case(name))
    })
}

/// `None` when the script is not one of FO4's stock `DefaultRef*` scripts or
/// the property already carries FO4's name and type.
fn placed_default_ref_rename(
    script_name: &[u8],
    property_name: &[u8],
    property_type: u8,
) -> Option<&'static [u8]> {
    if script_name.len() < 10 || !script_name[..10].eq_ignore_ascii_case(b"DefaultRef") {
        return None;
    }
    if property_type == 1 && property_name.eq_ignore_ascii_case(b"MyUniqueQuest") {
        return Some(b"MyQuest");
    }
    if property_type != 3 {
        return None;
    }
    if property_name.eq_ignore_ascii_case(b"PlayerTriggerType") {
        Some(b"PlayerTriggerOnly")
    } else if property_name.eq_ignore_ascii_case(b"PlayerActivateType") {
        Some(b"PlayerActivateOnly")
    } else if property_name.eq_ignore_ascii_case(b"PlayerHitType") {
        Some(b"PlayerHitOnly")
    } else if property_name.eq_ignore_ascii_case(b"PlayerPickupType") {
        Some(b"PlayerPickupOnly")
    } else {
        None
    }
}

fn normalize_placed_default_ref_bytes(data: &[u8]) -> Option<Vec<u8>> {
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

        let mut properties = Vec::with_capacity(property_count);
        for _ in 0..property_count {
            let property_start = offset;
            let property_name = qust_vmad_read_string(data, &mut offset)?;
            let property_type = qust_vmad_read_u8_advance(data, &mut offset)?;
            let flags = qust_vmad_read_u8_advance(data, &mut offset)?;
            let value_start = offset;
            qust_vmad_skip_property_value(data, &mut offset, property_type, object_format)?;
            properties.push(QustVmadProperty {
                name: property_name,
                property_type,
                flags,
                value: &data[value_start..offset],
                raw: &data[property_start..offset],
            });
        }

        let mut rebuilt = Vec::new();
        let mut script_changed = false;
        for property in &properties {
            let Some(target_name) =
                placed_default_ref_rename(script_name, property.name, property.property_type)
            else {
                rebuilt.extend_from_slice(property.raw);
                continue;
            };
            // An FO76 record that already carries FO4's name keeps it; renaming
            // would duplicate the property and the second copy wins at load.
            if properties
                .iter()
                .any(|other| other.name.eq_ignore_ascii_case(target_name))
            {
                rebuilt.extend_from_slice(property.raw);
                continue;
            }
            if property.property_type == 1 {
                qust_vmad_write_string(&mut rebuilt, target_name)?;
                rebuilt.extend_from_slice(&[property.property_type, property.flags]);
                rebuilt.extend_from_slice(property.value);
                script_changed = true;
                continue;
            }
            // Modes: 0 any, 1 player, 2 active player and team, 3 active player.
            // FO4 keeps only "player or anyone", matching FO76's own default.
            let mode = i32::from_le_bytes(property.value.try_into().ok()?);
            if !(-1..=3).contains(&mode) {
                rebuilt.extend_from_slice(property.raw);
                continue;
            }
            qust_vmad_write_string(&mut rebuilt, target_name)?;
            rebuilt.extend_from_slice(&[5, property.flags, u8::from(mode > 0)]);
            script_changed = true;
        }

        if !script_changed {
            scripts.push(data[script_start..offset].to_vec());
            continue;
        }
        changed = true;
        let mut script = data[script_start..property_count_offset].to_vec();
        script.extend_from_slice(&(properties.len() as u16).to_le_bytes());
        script.extend_from_slice(&rebuilt);
        scripts.push(script);
    }
    if !changed {
        return None;
    }

    let mut normalized = data[..6].to_vec();
    for script in scripts {
        normalized.extend_from_slice(&script);
    }
    normalized.extend_from_slice(&data[offset..]);
    Some(normalized)
}
