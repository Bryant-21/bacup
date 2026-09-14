use super::*;

/// Four-byte subrecord sigs to drop from every record before translation.
/// Each entry names its YAML field (OPDS is dropped raw). The orchestrator must
/// apply the same list when processing YAML-level keys by name.
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
pub(super) const FO76_IDLM_UNKNOWN_5_FLAG: u8 = 0x20;
pub(super) const FO4_MOVEMENT_SPEED_DATA_LEN: usize = 112;
pub(super) const FO4_ANIO_UNLOAD_EVENT: &str = "AnimObjUnequip";
/// `INNR.UNAM.target` enum values, from the generated FO4 schema.
pub(super) const FO4_INNR_TARGET_ARMOR: u32 = 29;
pub(super) const FO4_INNR_TARGET_FURNITURE: u32 = 42;
pub(super) const FO4_INNR_TARGET_WEAPON: u32 = 43;
pub(super) const FO4_INNR_TARGET_ACTOR: u32 = 45;
pub(super) const WSBUNKER_INTERCOM_EDITOR_ID: &str = "WSBunkerIntercom";
pub(super) const FO76_XALG_SKIP_HAVOK_ON_LOAD: u64 = 0x0000_0001;
const STATIC_MODEL_PREFIX: &str = "BACUP_Static";

/// Map an FO76 `INNR.INRF` record sig onto the FO4 `INNR.UNAM` target enum.
/// Filters with no FO4 equivalent return `None`, leaving the record untouched
/// rather than inventing a target.
fn fo4_innr_target_for_filter(filter: &str) -> Option<u32> {
    match filter {
        "ARMO" => Some(FO4_INNR_TARGET_ARMOR),
        "FURN" => Some(FO4_INNR_TARGET_FURNITURE),
        "WEAP" => Some(FO4_INNR_TARGET_WEAPON),
        "NPC_" => Some(FO4_INNR_TARGET_ACTOR),
        _ => None,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Fo76MiscStaticModelVariant {
    pub source_path: String,
    pub output_subpath: String,
}

impl Fo76MiscStaticModelVariant {
    pub(crate) fn decision_message(&self) -> String {
        serde_json::json!({
            "source_path": &self.source_path,
            "output_subpath": &self.output_subpath,
        })
        .to_string()
    }
}

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

pub(crate) fn fo76_misc_static_model_variant(
    record: &Record,
    interner: &crate::sym::StringInterner,
) -> Option<Fo76MiscStaticModelVariant> {
    if record.sig.0 != *b"MISC" {
        return None;
    }
    let xalg = record
        .fields
        .iter()
        .find(|field| field.sig.0 == *b"XALG")
        .and_then(|field| field_value_to_u32(&field.value))?;
    if u64::from(xalg) & FO76_XALG_SKIP_HAVOK_ON_LOAD == 0 {
        return None;
    }
    let model = record
        .fields
        .iter()
        .find(|field| field.sig.0 == *b"MODL")
        .and_then(|field| match &field.value {
            FieldValue::String(model) => interner.resolve(*model),
            _ => None,
        })?;
    let canonical = model_paths::canonical_model_path(model)?;
    let source_model = canonical
        .strip_prefix(&format!("{STATIC_MODEL_PREFIX}\\"))
        .unwrap_or(&canonical);
    let source_model = source_model.replace('\\', "/");
    Some(Fo76MiscStaticModelVariant {
        source_path: format!("Meshes/{source_model}"),
        output_subpath: format!("Meshes/{STATIC_MODEL_PREFIX}/{source_model}"),
    })
}

impl Fo76Fo4Hook {
    pub(super) fn route_skip_havok_misc_to_static_model(
        interner: &crate::sym::StringInterner,
        record: &mut Record,
    ) {
        let Some(variant) = fo76_misc_static_model_variant(record, interner) else {
            return;
        };
        let record_model = variant
            .output_subpath
            .strip_prefix("Meshes/")
            .unwrap_or(&variant.output_subpath)
            .replace('/', "\\");
        let Some(model) = record
            .fields
            .iter_mut()
            .find(|field| field.sig.0 == *b"MODL")
        else {
            return;
        };
        model.value = FieldValue::String(interner.intern(&record_model));
    }

    pub(super) fn ensure_anio_unload_event(
        interner: &crate::sym::StringInterner,
        record: &mut Record,
    ) {
        if record.sig.0 != *b"ANIO" || record.fields.iter().any(|entry| entry.sig.0 == *b"BNAM") {
            return;
        }

        record.fields.push(FieldEntry {
            sig: SubrecordSig(*b"BNAM"),
            value: FieldValue::String(interner.intern(FO4_ANIO_UNLOAD_EVENT)),
        });
    }

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
            _ => {}
        }
    }

    /// FO76 names an `INNR`'s target with an `INRF` record sig; FO4 uses a
    /// `UNAM` enum. Converting in place keeps the subrecord ahead of the first
    /// ruleset `VNAM`, where FO4 expects it. An INNR that reaches FO4 without a
    /// `UNAM` is inert — the engine never applies its rules.
    pub(super) fn convert_innr_filter_to_fo4_target(
        interner: &crate::sym::StringInterner,
        record: &mut Record,
    ) {
        if record.sig.0 != *b"INNR" {
            return;
        }

        let Some(entry) = record
            .fields
            .iter_mut()
            .find(|field| field.sig.0 == *b"INRF")
        else {
            return;
        };

        let filter = match &entry.value {
            FieldValue::String(sym) => interner.resolve(*sym).map(|s| s.to_string()),
            FieldValue::Bytes(bytes) => {
                let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
                std::str::from_utf8(&bytes[..end]).ok().map(str::to_string)
            }
            _ => None,
        };

        let Some(target) = filter.as_deref().and_then(fo4_innr_target_for_filter) else {
            return;
        };

        entry.sig = SubrecordSig::from_str("UNAM").expect("UNAM is a valid sig");
        entry.value = FieldValue::Uint(u64::from(target));
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
