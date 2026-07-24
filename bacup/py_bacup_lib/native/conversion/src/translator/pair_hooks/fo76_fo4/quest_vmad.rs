use super::*;

pub(super) const FO76_QUEST_EVENT_SCPT: u32 = 0x5450_4353;
pub(super) const FO76_QUEST_EVENT_REFERENCE3: u32 = 13_138;
pub(super) const FO4_PLAYER_REF_FORM_ID: u32 = 0x0000_14;
pub(super) const FO76_VMAD_VERSION: u16 = 6;
pub(super) const FO76_VMAD_OBJECT_FORMAT: u16 = 2;
pub(super) const FO76_VMAD_ALIAS_VERSION: u16 = 6;

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
    for alias_id in parse_qust_vmad_fragment_player_alias_ids(bytes).unwrap_or_default() {
        if !alias_ids.contains(&alias_id) {
            alias_ids.push(alias_id);
        }
    }
    alias_ids
}

fn parse_qust_vmad_fragment_player_alias_ids(data: &[u8]) -> Option<SmallVec<[u32; 4]>> {
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
        return Some(SmallVec::new());
    }

    qust_vmad_advance(&mut offset, 1, data.len())?;
    let property_count = qust_vmad_read_u16_advance(data, &mut offset)? as usize;
    let mut alias_ids = SmallVec::new();
    for _ in 0..property_count {
        let property_name = qust_vmad_read_string(data, &mut offset)?;
        let property_type = qust_vmad_read_u8_advance(data, &mut offset)?;
        qust_vmad_advance(&mut offset, 1, data.len())?;
        if property_type == 1
            && (property_name.eq_ignore_ascii_case(b"Alias_Player")
                || property_name.eq_ignore_ascii_case(b"Alias_currentPlayer"))
        {
            let alias_offset = offset.checked_add(2)?;
            let alias_id = i16::from_le_bytes(
                data.get(alias_offset..alias_offset.checked_add(2)?)?
                    .try_into()
                    .ok()?,
            );
            qust_vmad_advance(&mut offset, 8, data.len())?;
            if alias_id >= 0 {
                alias_ids.push(alias_id as u32);
            }
        } else {
            qust_vmad_skip_property_value(data, &mut offset, property_type, object_format)?;
        }
    }
    Some(alias_ids)
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

pub(crate) fn qust_has_untranslatable_event_alias(record: &Record) -> bool {
    let player_event_consumer_alias_ids = qust_vmad_player_event_consumer_alias_ids(record);
    let remove_players_alias_ids = qust_vmad_remove_players_alias_ids(record);
    let mut current_alias_id = None;
    let mut index = 0;

    while index < record.fields.len() {
        let entry = &record.fields[index];
        if QUST_ALIAS_ANCHOR_SIGS.iter().any(|sig| entry.sig.0 == *sig) {
            current_alias_id = (entry.sig.0 == *b"ALST")
                .then(|| field_value_to_u32(&entry.value))
                .flatten();
        } else if entry.sig.0 == *b"ALFE" {
            let Some(next) = record.fields.get(index + 1) else {
                return true;
            };
            let safe = next.sig.0 == *b"ALFD"
                && current_alias_id
                    .zip(field_value_to_u32(&entry.value))
                    .zip(field_value_to_u32(&next.value))
                    .is_some_and(|((alias_id, event), event_data)| {
                        qust_event_alias_rewrites_to_player(
                            alias_id,
                            event,
                            event_data,
                            &player_event_consumer_alias_ids,
                            &remove_players_alias_ids,
                        )
                    });
            if !safe {
                return true;
            }
            index += 2;
            continue;
        } else if entry.sig.0 == *b"ALFD" {
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
