//! Read a target master's record with its raw FormIDs numbered like the output plugin.
//!
//! Decoding resolves typed fields (KWDA, APPR, LNAM) to FormKeys, but struct and
//! custom-codec subrecords stay `FieldValue::Bytes` holding FormIDs in the master's
//! own numbering. A DLC whose only master is Fallout4.esm calls itself `01`; in the
//! output, `01` is DLCRobot.esm. Copying those bytes, or resolving them against the
//! output's masters, turns every Nuka-World or Far Harbor self-reference into a
//! missing DLCRobot form: the Handmade Rifle lost its receiver and ammo this way, and
//! NPCs materialized from DLC templates lost their factions and default mods.

use esp_authoring_core::plugin_runtime::authoring::authoring_serialize::rewrite_schema_form_ids_in_subrecord;
use esp_authoring_core::plugin_runtime::{
    CompiledSchema, compiled_schema_for_game_str, schema_record_spec, schema_subrecord_spec,
};
use rustc_hash::FxHashMap;

use crate::ids::{FormKey, SubrecordSig};
use crate::record::{FieldValue, Record};
use crate::schema::AuthoringSchema;
use crate::session::{PluginSession, SessionError};
use crate::sym::StringInterner;

const OBTS_FIXED_HEADER_LEN: usize = 16;
const OBTS_KEYWORD_COUNT_OFFSET: usize = 15;
const OBTS_INCLUDE_PADDING_LEN: usize = 2;
const OBTS_INCLUDE_ROW_LEN: usize = 7;
const OBTS_PROPERTY_ROW_LEN: usize = 24;
const OBTS_PROPERTY_VALUE_1_OFFSET: usize = 12;

/// [`PluginSession::record_decoded_in_handle`] for a target master's handle, with the
/// raw FormIDs in `Bytes` fields renumbered into the output plugin's master list.
pub(crate) fn record_decoded_in_master_for_target(
    session: &mut PluginSession,
    handle_id: u64,
    fk: &FormKey,
    schema: &AuthoringSchema,
    interner: &StringInterner,
) -> Result<Record, SessionError> {
    let mut record = session.record_decoded_in_handle(handle_id, fk, schema, interner)?;
    let (record_masters, record_plugin) = session.handle_load_order(handle_id)?;
    let target_index_by_record_index: Vec<Option<u32>> = record_masters
        .iter()
        .map(String::as_str)
        .chain(std::iter::once(record_plugin))
        .map(|plugin| {
            session
                .target_masters()
                .iter()
                .position(|master| master.eq_ignore_ascii_case(plugin))
                .and_then(|index| u32::try_from(index).ok())
        })
        .collect();
    let numbering_matches = target_index_by_record_index
        .iter()
        .enumerate()
        .all(|(index, target)| *target == u32::try_from(index).ok());
    if numbering_matches {
        return Ok(record);
    }
    let game = session
        .target_slot()
        .parsed
        .game
        .as_deref()
        .ok_or_else(|| SessionError::Other("target plugin has no game".into()))?;
    let compiled_schema = compiled_schema_for_game_str(game).map_err(SessionError::Other)?;
    renumber_raw_form_ids(&mut record, &target_index_by_record_index, &compiled_schema);
    Ok(record)
}

fn renumber_raw_form_ids(
    record: &mut Record,
    target_index_by_record_index: &[Option<u32>],
    schema: &CompiledSchema,
) {
    let mut renumber = |raw: u32| {
        if raw == 0 {
            return None;
        }
        let index = (*target_index_by_record_index.get(usize::try_from(raw >> 24).ok()?)?)?;
        let renumbered = (index << 24) | (raw & 0x00FF_FFFF);
        (renumbered != raw).then_some(renumbered)
    };
    let Some(record_spec) = schema_record_spec(schema, record.sig.as_str()) else {
        return;
    };
    let mut occurrences: FxHashMap<SubrecordSig, usize> = FxHashMap::default();
    for entry in &mut record.fields {
        let occurrence = occurrences.entry(entry.sig).or_default();
        let sub_spec = schema_subrecord_spec(record_spec, entry.sig.as_str(), *occurrence);
        *occurrence += 1;
        let FieldValue::Bytes(bytes) = &mut entry.value else {
            continue;
        };
        if entry.sig.0 == *b"OBTS" {
            renumber_obts_form_ids(bytes.as_mut_slice(), &mut renumber);
        } else if let Some(sub_spec) = sub_spec {
            rewrite_schema_form_ids_in_subrecord(
                sub_spec,
                schema,
                bytes.as_mut_slice(),
                &mut renumber,
            );
        }
    }
}

/// The schema walker stops before OBTS include rows, and the schema cannot say that a
/// property's Value 1 is a FormID only for the `FormID,Int` (4) and `FormID,Float` (6)
/// value types, so OBTS is walked here.
fn renumber_obts_form_ids(bytes: &mut [u8], renumber: &mut dyn FnMut(u32) -> Option<u32>) {
    let Some(header) = bytes.get(..OBTS_FIXED_HEADER_LEN) else {
        return;
    };
    let include_count = u32::from_le_bytes([header[0], header[1], header[2], header[3]]) as usize;
    let property_count = u32::from_le_bytes([header[4], header[5], header[6], header[7]]) as usize;
    let keyword_count = usize::from(header[OBTS_KEYWORD_COUNT_OFFSET]);
    let includes_start = OBTS_FIXED_HEADER_LEN + keyword_count * 4 + OBTS_INCLUDE_PADDING_LEN;
    let properties_start = includes_start + include_count * OBTS_INCLUDE_ROW_LEN;
    let Some(property_bytes) = bytes.len().checked_sub(properties_start) else {
        return;
    };
    let property_count = property_count.min(property_bytes / OBTS_PROPERTY_ROW_LEN);
    let form_id_offsets: Vec<usize> = (0..keyword_count)
        .map(|index| OBTS_FIXED_HEADER_LEN + index * 4)
        .chain((0..include_count).map(|index| includes_start + index * OBTS_INCLUDE_ROW_LEN))
        .chain(
            (0..property_count)
                .map(|index| properties_start + index * OBTS_PROPERTY_ROW_LEN)
                .filter(|&row| matches!(bytes[row], 4 | 6))
                .map(|row| row + OBTS_PROPERTY_VALUE_1_OFFSET),
        )
        .collect();
    for offset in form_id_offsets {
        let slot = &mut bytes[offset..offset + 4];
        let raw = u32::from_le_bytes([slot[0], slot[1], slot[2], slot[3]]);
        if let Some(renumbered) = renumber(raw) {
            slot.copy_from_slice(&renumbered.to_le_bytes());
        }
    }
}

#[cfg(test)]
mod tests;
