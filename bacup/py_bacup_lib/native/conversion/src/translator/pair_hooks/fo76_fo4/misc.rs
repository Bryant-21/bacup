use super::*;

/// Four-byte subrecord sigs to drop from every record before translation.
///
/// Mirrors `_GLOBAL_DROP_FIELDS` in `fo76_to_fo4.py`. The Python names are
/// YAML object-level field names that map 1-to-1 to subrecord sigs except
/// where noted:
///
/// | Python field             | Subrecord sig |
/// |--------------------------|---------------|
/// | ObjectPlacementDefaults  | OPDS (dropped as raw) |
/// | VersionControl           | VCTX          |
/// | FormVersion              | FVER          |
/// | Fallout76MajorRecordFlags| FL76          |
/// | MajorRecordFlagsRaw      | FLWR          |
/// | MaxItemID                | MIID          |
/// | MAGF                     | MAGF          |
/// | CODV                     | CODV          |
///
/// Note: Python field names that do not map directly to 4-char sigs are
/// represented here by their canonical subrecord equivalents. Orchestrator
/// must apply the same list when processing YAML-level keys by name.
pub(super) const GLOBAL_DROP_SIGS: &[[u8; 4]] = &[
    *b"VCTX", // VersionControl
    *b"FVER", // FormVersion
    *b"FL76", // Fallout76MajorRecordFlags
    *b"FLWR", // MajorRecordFlagsRaw
    *b"MIID", // MaxItemID
    *b"MAGF", // MAGF (direct sig)
    *b"CODV", // CODV (direct sig)
    *b"OPDS", // ObjectPlacementDefaults
];
pub(super) const FO4_WORKBENCH_DATA_LEN: usize = 1;
pub(super) const FO4_MAX_MGEF_ARCHETYPE: u32 = 49;
pub(super) const FO76_DAMAGE_TYPE_ROW_LEN: usize = 12;
pub(super) const FO4_DAMAGE_TYPE_ROW_LEN: usize = 8;
pub(super) const FO76_IDLM_UNKNOWN_5_FLAG: u8 = 0x20;
pub(super) const FO4_MOVEMENT_SPEED_DATA_LEN: usize = 112;
pub(crate) const FO76_RADIO_FREQUENCY_NAMESPACE_OFFSET: f32 = 2048.0;
pub(super) const WSBUNKER_INTERCOM_EDITOR_ID: &str = "WSBunkerIntercom";

pub(super) const FO76_FONT_ALIAS_REPLACEMENTS: [(&str, &str); 3] = [
    ("$Typewriter_Font", "$Terminal_Font"),
    ("$76HandwrittenNeat_Font", "$HandwrittenFont"),
    ("$76HandwrittenIlliterate", "$HandwrittenFont"),
];

pub(crate) fn rewritten_fo76_font_aliases_for_fo4(text: &str) -> Option<String> {
    if !FO76_FONT_ALIAS_REPLACEMENTS
        .iter()
        .any(|(source, _)| text.contains(source))
    {
        return None;
    }

    let mut rewritten = text.to_string();
    for (source, target) in FO76_FONT_ALIAS_REPLACEMENTS {
        rewritten = rewritten.replace(source, target);
    }
    Some(rewritten)
}

pub(super) fn rewrite_fo76_font_aliases_in_record(
    record: &mut Record,
    interner: &crate::sym::StringInterner,
) {
    for field in &mut record.fields {
        let FieldValue::String(symbol) = &mut field.value else {
            continue;
        };
        if let Some(rewritten) = interner
            .resolve(*symbol)
            .and_then(rewritten_fo76_font_aliases_for_fo4)
        {
            *symbol = interner.intern(&rewritten);
        }
    }
}

pub(crate) fn namespace_fo76_radio_frequency(frequency: &mut f32) -> bool {
    if !frequency.is_finite()
        || *frequency <= 0.0
        || *frequency >= FO76_RADIO_FREQUENCY_NAMESPACE_OFFSET
    {
        return false;
    }
    *frequency += FO76_RADIO_FREQUENCY_NAMESPACE_OFFSET;
    true
}
impl Fo76Fo4Hook {
    pub(super) fn strip_wsbunker_intercom_radio(
        interner: &crate::sym::StringInterner,
        record: &mut Record,
    ) {
        if record.sig.0 != *b"ACTI"
            || !record
                .eid
                .and_then(|eid| interner.resolve(eid))
                .is_some_and(|eid| eid.eq_ignore_ascii_case(WSBUNKER_INTERCOM_EDITOR_ID))
        {
            return;
        }
        record
            .fields
            .retain(|entry| entry.sig.0 != *b"FNAM" && entry.sig.0 != *b"RADR");
    }

    pub(super) fn namespace_radio_receiver_frequency(
        interner: &crate::sym::StringInterner,
        record: &mut Record,
    ) {
        if record.sig.0 != *b"ACTI" {
            return;
        }
        for entry in &mut record.fields {
            if entry.sig.0 != *b"RADR" {
                continue;
            }
            match &mut entry.value {
                FieldValue::Bytes(bytes) if bytes.len() >= 8 => {
                    let mut frequency =
                        f32::from_le_bytes(bytes[4..8].try_into().expect("RADR frequency"));
                    if namespace_fo76_radio_frequency(&mut frequency) {
                        bytes[4..8].copy_from_slice(&frequency.to_le_bytes());
                    }
                }
                FieldValue::Struct(fields) => {
                    let Some((_, FieldValue::Float(frequency))) = fields
                        .iter_mut()
                        .find(|(name, _)| Self::struct_field_name_is(interner, *name, "Frequency"))
                    else {
                        continue;
                    };
                    namespace_fo76_radio_frequency(frequency);
                }
                _ => {}
            }
        }
    }

    /// Drop all subrecords whose sig is in `GLOBAL_DROP_SIGS`.
    pub(super) fn drop_global_fields(record: &mut Record) {
        record
            .fields
            .retain(|entry| !GLOBAL_DROP_SIGS.iter().any(|sig| entry.sig.0 == *sig));
    }

    pub(super) fn strip_info_editor_id(record: &mut Record) {
        if record.sig.0 == *b"INFO" {
            record.fields.retain(|entry| entry.sig.0 != *b"EDID");
        }
    }

    pub(super) fn strip_orphan_term_conditions(record: &mut Record) {
        if record.sig.0 != *b"TERM" {
            return;
        }

        let old_fields: Vec<FieldEntry> = record.fields.drain(..).collect();
        let mut retained = smallvec::SmallVec::new();
        let mut condition_anchor_active = false;
        let mut condition_group_started = false;
        let mut keep_condition_strings = false;

        for entry in old_fields {
            match &entry.sig.0 {
                b"BSIZ" | b"ISIZ" => {
                    condition_anchor_active = false;
                    condition_group_started = false;
                    keep_condition_strings = false;
                    retained.push(entry);
                }
                b"BTXT" | b"ITXT" => {
                    condition_anchor_active = true;
                    condition_group_started = false;
                    keep_condition_strings = false;
                    retained.push(entry);
                }
                b"CTDA" | b"CTDT" => {
                    keep_condition_strings = condition_anchor_active;
                    if keep_condition_strings {
                        condition_group_started = true;
                        retained.push(entry);
                    }
                }
                b"CIS1" | b"CIS2" => {
                    if keep_condition_strings {
                        retained.push(entry);
                    }
                }
                _ => {
                    if condition_group_started {
                        condition_anchor_active = false;
                        condition_group_started = false;
                    }
                    keep_condition_strings = false;
                    retained.push(entry);
                }
            }
        }

        record.fields = retained;
    }

    pub(super) fn normalize_note_scene_ref(record: &mut Record) {
        if record.sig.0 != *b"NOTE" {
            return;
        }

        for entry in &mut record.fields {
            if entry.sig.0 == *b"SNAM"
                && let Some(value) = source_form_key_value(&entry.value, record.form_key.plugin)
            {
                entry.value = value;
            }
        }
    }

    pub(super) fn strip_fo76_only_subrecord_tails(record: &mut Record) {
        match &record.sig.0 {
            b"FURN" | b"TERM" => {
                truncate_raw_subrecord(record, b"WBDT", FO4_WORKBENCH_DATA_LEN);
            }
            b"MOVT" => {
                truncate_raw_subrecord(record, b"SPED", FO4_MOVEMENT_SPEED_DATA_LEN);
            }
            b"ARMO" | b"WEAP" => {
                project_raw_array_rows(
                    record,
                    b"DAMA",
                    FO76_DAMAGE_TYPE_ROW_LEN,
                    FO4_DAMAGE_TYPE_ROW_LEN,
                );
            }
            _ => {}
        }
    }

    pub(super) fn normalize_idlm_flags(record: &mut Record) {
        if record.sig.0 != *b"IDLM" {
            return;
        }

        for entry in &mut record.fields {
            if entry.sig.0 != *b"IDLF" {
                continue;
            }
            match &mut entry.value {
                FieldValue::Uint(value) => *value &= !u64::from(FO76_IDLM_UNKNOWN_5_FLAG),
                FieldValue::Int(value) => *value &= !i64::from(FO76_IDLM_UNKNOWN_5_FLAG),
                FieldValue::Bytes(bytes) if bytes.len() == 1 => {
                    bytes[0] &= !FO76_IDLM_UNKNOWN_5_FLAG;
                }
                _ => {}
            }
        }
    }
}
