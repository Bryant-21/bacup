//! Fixup: strip or normalise FO76 fields that produce FO4 ESP checker errors.
//!
//! Creature conversions only (root sig NPC_ or LVLN). Per record type:
//!
//! - **All records**: sync `KSIZ` to the `KWDA` row count.
//! - **LVLN / LVLC**: ensure `LVLD` (Chance None), mask `LVLF` to the FO4 3-bit
//!   range, drop FO76-only list subrecords and null-`Reference` `LVLO`/`LVLE`
//!   entries, sync `LLCT`.
//! - **LVLI**: drop null-`Reference` `LVLO`/`LVLE` entries, sync `LLCT`.
//! - **SNDR**: remove `HNAM`, `INAM`, `PNAM`, `QNAM`.
//! - **MGEF**: remove `VMAD` and `CTDA`.
//! - **ALCH / ENCH / SPEL**: remove `CTDA`; remove `EFIT` with no preceding `EFID`.
//! - **QUST**: remove `VMAD`, `CTDA`, and `FNAM` payloads larger than 8 bytes.
//!
//! STAG non-SNDR sounds are handled by `fix_stag_sound_refs`, and the FO76 0x10
//! record flag never survives `RecordFlags::from_bits_truncate`. WEAP raw-hex
//! DNAM and QUST alias scrubbing would need a typed decode of `struct:` codecs,
//! which arrive as `FieldValue::Bytes`.

use crate::fixups::prune_orphaned_records::is_creature_root_sig;
use crate::fixups::{Fixup, FixupConfig, FixupContext, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::full_plugin::FixupScope;
use crate::ids::{SigCode, SubrecordSig};
use crate::record::{FieldEntry, FieldValue, Record};
use crate::session::{EditOutcome, PluginSession};
use crate::sym::Sym;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// FO4 LVLF.flags is a 1-byte enum; only the low 3 bits are valid in FO4.
const LVLF_FO4_MASK: u8 = 0x07;

/// FNAM payload threshold in QUST: payloads larger than 8 bytes are FO76-only
/// extensions that fail the FO4 ESP checker.
const QUST_FNAM_MAX_LEN: usize = 8;

/// Subrecord sigs stripped wholesale from SNDR records.
const SNDR_REJECTED_SIGS: &[&str] = &["HNAM", "INAM", "PNAM", "QNAM"];

/// FO76 leveled-NPC/list extensions not valid in FO4 LVLN/LVLC records.
const LEVELED_NPC_REJECTED_SIGS: &[&str] = &["ONAM", "LVMV", "LVIV", "LVLV", "ENLS", "AUUV"];

// ---------------------------------------------------------------------------
// Public fixup struct
// ---------------------------------------------------------------------------

pub struct CleanCreatureEspCheckFieldsFixup;

enum CreatureRecordEdit {
    Replace { record: Record, dropped: u32 },
    Warn(String),
}

impl Fixup for CleanCreatureEspCheckFieldsFixup {
    fn name(&self) -> &'static str {
        "clean_creature_esp_check_fields"
    }

    fn scope(&self) -> FixupScope {
        FixupScope::GraphOnly
    }

    fn uses_session(&self) -> bool {
        true
    }

    fn applies_to(&self, ctx: &FixupContext) -> bool {
        ctx.config
            .root_sig
            .map(is_creature_root_sig)
            .unwrap_or(false)
    }

    fn applies_to_session(&self, _session: &PluginSession, config: &FixupConfig) -> bool {
        config.root_sig.map(is_creature_root_sig).unwrap_or(false)
    }

    fn run_with_session(
        &self,
        session: &mut PluginSession,
        mapper: &mut FormKeyMapper,
        config: &FixupConfig,
    ) -> Result<FixupReport, FixupError> {
        let target_schema = config
            .target_schema
            .as_deref()
            .ok_or_else(|| FixupError::Other("missing target schema in fixup config".into()))?;
        let interner = mapper.interner;
        let mut report = FixupReport::empty();
        let reference_sym = interner.intern("Reference");

        // Each record type drives a different branch; iterate the relevant
        // sigs and call the per-record helper.  Sigs absent from the plugin
        // simply emit no FormKeys.
        let sigs: &[&str] = &[
            "LVLN", "LVLC", "LVLI", "SNDR", "MGEF", "ALCH", "ENCH", "SPEL", "QUST",
        ];

        for sig_str in sigs {
            let sig =
                SigCode::from_str(sig_str).map_err(|e| FixupError::SchemaError(e.to_string()))?;
            let mut sig_dropped = 0u32;
            let mut sig_warnings = Vec::new();
            let sig_report = session.map_apply_by_sig(
                sig,
                mapper,
                |view, _snapshot, fk| match view.record_decoded(fk, target_schema, interner) {
                    Ok(mut record) => {
                        let (dropped, changed) = apply_to_record(&mut record, reference_sym);
                        changed.then_some(CreatureRecordEdit::Replace { record, dropped })
                    }
                    Err(err) => Some(CreatureRecordEdit::Warn(format!(
                        "clean_creature_esp_read:{err}"
                    ))),
                },
                |session, mapper, _fk, edit| match edit {
                    CreatureRecordEdit::Replace { record, dropped } => {
                        session
                            .replace_record(record, target_schema, mapper.interner)
                            .map_err(|e| FixupError::HandleError(e.to_string()))?;
                        sig_dropped += dropped;
                        Ok(EditOutcome::Changed)
                    }
                    CreatureRecordEdit::Warn(message) => {
                        sig_warnings.push(mapper.interner.intern(&message));
                        Ok(EditOutcome::NoOp)
                    }
                },
            )?;
            report.records_changed += sig_report.records_changed;
            report.records_added += sig_report.records_added;
            report.records_dropped += sig_report.records_dropped + sig_dropped;
            report.warnings.extend(sig_report.warnings);
            report.warnings.extend(sig_warnings);
        }

        Ok(report)
    }
}

// ---------------------------------------------------------------------------
// Record-level mutation
// ---------------------------------------------------------------------------

/// Apply every per-record-type branch to `record`.
///
/// Returns `(dropped_count, changed)` where `dropped_count` is the number of
/// subrecords removed and `changed` is `true` when any mutation occurred.
pub fn apply_to_record(record: &mut Record, reference_sym: Sym) -> (u32, bool) {
    let mut dropped: u32 = 0;
    let mut changed = false;

    // All records: KSIZ ↔ KWDA sync.
    if sync_ksiz_to_kwda(record) {
        changed = true;
    }

    match record.sig.as_str() {
        "LVLN" | "LVLC" => {
            if ensure_lvld_present(record) {
                changed = true;
            }
            if mask_lvlf_to_fo4_bits(record) {
                changed = true;
            }
            let removed = remove_subrecords(record, LEVELED_NPC_REJECTED_SIGS);
            if removed > 0 {
                dropped += removed;
                changed = true;
            }
            let removed = drop_null_leveled_entries(record, reference_sym);
            if removed > 0 {
                dropped += removed;
                changed = true;
            }
            if sync_llct_to_leveled_entries(record) {
                changed = true;
            }
        }
        "LVLI" => {
            let removed = drop_null_leveled_entries(record, reference_sym);
            if removed > 0 {
                dropped += removed;
                changed = true;
            }
            if sync_llct_to_leveled_entries(record) {
                changed = true;
            }
        }
        "SNDR" => {
            let removed = remove_subrecords(record, SNDR_REJECTED_SIGS);
            if removed > 0 {
                dropped += removed;
                changed = true;
            }
        }
        "MGEF" => {
            let removed = remove_subrecords(record, &["VMAD", "CTDA"]);
            if removed > 0 {
                dropped += removed;
                changed = true;
            }
        }
        "ALCH" | "ENCH" | "SPEL" => {
            let mut removed = remove_subrecords(record, &["CTDA"]);
            if !has_subrecord(record, "EFID") {
                removed += remove_subrecords(record, &["EFIT"]);
            }
            if removed > 0 {
                dropped += removed;
                changed = true;
            }
        }
        "QUST" => {
            let mut removed = remove_subrecords(record, &["VMAD", "CTDA"]);
            if drop_oversize_fnam(record) {
                removed += 1;
            }
            if removed > 0 {
                dropped += removed;
                changed = true;
            }
        }
        _ => {}
    }

    (dropped, changed)
}

// ---------------------------------------------------------------------------
// Branch: KSIZ ↔ KWDA sync
// ---------------------------------------------------------------------------

/// Sync the `KSIZ` (keyword count) subrecord to match the number of FormID
/// entries in `KWDA`.  KWDA decodes as raw bytes (the `formid_array` codec
/// has no dispatch in `source_read`), so each entry is exactly 4 bytes.
///
/// Returns `true` when KSIZ was updated.  When KWDA is absent, no change.
fn sync_ksiz_to_kwda(record: &mut Record) -> bool {
    let Some(kwda_sig) = sig("KWDA") else {
        return false;
    };
    let Some(ksiz_sig) = sig("KSIZ") else {
        return false;
    };

    let mut kwda_count: Option<u32> = None;
    for entry in record.fields.iter() {
        if entry.sig != kwda_sig {
            continue;
        }
        match &entry.value {
            FieldValue::Bytes(data) => {
                kwda_count = Some((data.len() / 4) as u32);
            }
            FieldValue::List(items) => {
                kwda_count = Some(items.len() as u32);
            }
            _ => {}
        }
        break;
    }

    let Some(expected) = kwda_count else {
        return false;
    };

    for entry in record.fields.iter_mut() {
        if entry.sig != ksiz_sig {
            continue;
        }
        match &mut entry.value {
            FieldValue::Uint(n) => {
                if *n != expected as u64 {
                    *n = expected as u64;
                    return true;
                }
                return false;
            }
            FieldValue::Bytes(data) if data.len() >= 4 => {
                let current = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
                if current != expected {
                    let bytes = expected.to_le_bytes();
                    data[0] = bytes[0];
                    data[1] = bytes[1];
                    data[2] = bytes[2];
                    data[3] = bytes[3];
                    return true;
                }
                return false;
            }
            _ => return false,
        }
    }

    false
}

// ---------------------------------------------------------------------------
// Branch: LVLN/LVLC LVLD presence
// ---------------------------------------------------------------------------

/// Add a zero `LVLD` (Chance None, `uint8`) when absent; an existing LVLD is
/// left alone.
fn ensure_lvld_present(record: &mut Record) -> bool {
    let Some(lvld_sig) = sig("LVLD") else {
        return false;
    };
    if record.fields.iter().any(|e| e.sig == lvld_sig) {
        return false;
    }
    record.fields.push(FieldEntry {
        sig: lvld_sig,
        value: FieldValue::Uint(0),
    });
    true
}

// ---------------------------------------------------------------------------
// Branch: LVLN/LVLC LVLF mask
// ---------------------------------------------------------------------------

/// Mask `LVLF.flags` to the FO4-valid 3-bit range.  Returns `true` when a
/// change was made.
fn mask_lvlf_to_fo4_bits(record: &mut Record) -> bool {
    let Some(lvlf_sig) = sig("LVLF") else {
        return false;
    };

    for entry in record.fields.iter_mut() {
        if entry.sig != lvlf_sig {
            continue;
        }
        match &mut entry.value {
            FieldValue::Uint(n) => {
                let masked = (*n as u8) & LVLF_FO4_MASK;
                if (*n as u8) != masked {
                    *n = masked as u64;
                    return true;
                }
                return false;
            }
            FieldValue::Bytes(data) if !data.is_empty() => {
                let masked = data[0] & LVLF_FO4_MASK;
                if data[0] != masked {
                    data[0] = masked;
                    return true;
                }
                return false;
            }
            _ => return false,
        }
    }

    false
}

// ---------------------------------------------------------------------------
// Branch: drop LVLO/LVLE entries with null Reference
// ---------------------------------------------------------------------------

/// Remove `LVLO`/`LVLE` entries whose `Reference` is null, missing, or not a
/// FormKey (no master-existence check). Returns the number removed.
fn drop_null_leveled_entries(record: &mut Record, reference_sym: Sym) -> u32 {
    let mut removed: u32 = 0;
    let mut kept: smallvec::SmallVec<[FieldEntry; 8]> = smallvec::SmallVec::new();

    for entry in record.fields.drain(..) {
        if !is_leveled_entry_sig(&entry) {
            kept.push(entry);
            continue;
        }

        let has_valid_ref = has_non_null_leveled_reference(&entry.value, reference_sym);

        if has_valid_ref {
            kept.push(entry);
        } else {
            removed += 1;
        }
    }

    record.fields = kept;
    removed
}

/// Returns `true` when the subrecord sig is `LVLO` or `LVLE`.
fn is_leveled_entry_sig(entry: &FieldEntry) -> bool {
    matches!(entry.sig.as_str(), "LVLO" | "LVLE")
}

/// Returns `true` when the leveled entry carries a non-null reference FormKey.
fn has_non_null_leveled_reference(value: &FieldValue, sym: Sym) -> bool {
    match value {
        FieldValue::Struct(fields) => {
            fields.iter().any(|(field_sym, field_val)| {
                *field_sym == sym && matches!(field_val, FieldValue::FormKey(fk) if fk.local != 0)
            }) || fields
                .iter()
                .any(|(_, field_val)| matches!(field_val, FieldValue::FormKey(fk) if fk.local != 0))
        }
        FieldValue::Bytes(data) if data.len() >= 8 => {
            u32::from_le_bytes([data[4], data[5], data[6], data[7]]) & 0x00FF_FFFF != 0
        }
        FieldValue::FormKey(fk) => fk.local != 0,
        _ => false,
    }
}

// ---------------------------------------------------------------------------
// Branch: sync LLCT to surviving LVLO/LVLE count
// ---------------------------------------------------------------------------

/// Sync the `LLCT` (entry count) subrecord to the number of `LVLO`/`LVLE`
/// entries currently in the record.  Returns `true` when LLCT was updated.
///
/// If LLCT is missing it is created.
fn sync_llct_to_leveled_entries(record: &mut Record) -> bool {
    let Some(llct_sig) = sig("LLCT") else {
        return false;
    };

    let expected: u32 = record
        .fields
        .iter()
        .filter(|e| is_leveled_entry_sig(e))
        .count() as u32;

    for entry in record.fields.iter_mut() {
        if entry.sig != llct_sig {
            continue;
        }
        match &mut entry.value {
            FieldValue::Uint(n) => {
                if *n != expected as u64 {
                    *n = expected as u64;
                    return true;
                }
                return false;
            }
            FieldValue::Bytes(data) if !data.is_empty() => {
                // LLCT codec is uint8.  Truncate to byte width.
                let new_byte = expected.min(0xFF) as u8;
                if data[0] != new_byte {
                    data[0] = new_byte;
                    return true;
                }
                return false;
            }
            _ => return false,
        }
    }

    // LLCT missing — append.  Use Uint(u8) representation matching the codec.
    record.fields.push(FieldEntry {
        sig: llct_sig,
        value: FieldValue::Uint(expected as u64),
    });
    true
}

// ---------------------------------------------------------------------------
// Branch: remove subrecords by signature
// ---------------------------------------------------------------------------

/// Remove every subrecord whose signature is in `sigs`.  Returns the count
/// of entries removed.
fn remove_subrecords(record: &mut Record, sig_strs: &[&str]) -> u32 {
    let targets: smallvec::SmallVec<[SubrecordSig; 4]> = sig_strs
        .iter()
        .filter_map(|s| SubrecordSig::from_str(s).ok())
        .collect();
    if targets.is_empty() {
        return 0;
    }

    let before = record.fields.len();
    record
        .fields
        .retain(|entry| !targets.iter().any(|t| *t == entry.sig));
    (before - record.fields.len()) as u32
}

/// Returns `true` when at least one subrecord with `sig_str` exists.
fn has_subrecord(record: &Record, sig_str: &str) -> bool {
    let Some(s) = sig(sig_str) else {
        return false;
    };
    record.fields.iter().any(|e| e.sig == s)
}

// ---------------------------------------------------------------------------
// Branch: QUST FNAM oversize strip
// ---------------------------------------------------------------------------

/// Remove a single `FNAM` subrecord when its raw byte payload exceeds the
/// 8-byte FO4 limit.  Python keys on `raw_hex` length; the byte payload is
/// the source of truth in the decoded view.  Returns `true` when removed.
fn drop_oversize_fnam(record: &mut Record) -> bool {
    let Some(fnam_sig) = sig("FNAM") else {
        return false;
    };

    let mut target_idx: Option<usize> = None;
    for (i, e) in record.fields.iter().enumerate() {
        if e.sig != fnam_sig {
            continue;
        }
        if let FieldValue::Bytes(data) = &e.value {
            if data.len() > QUST_FNAM_MAX_LEN {
                target_idx = Some(i);
                break;
            }
        }
    }

    match target_idx {
        Some(i) => {
            record.fields.remove(i);
            true
        }
        None => false,
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn sig(name: &str) -> Option<SubrecordSig> {
    SubrecordSig::from_str(name).ok()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixups::{FixupConfig, FixupContext};
    use crate::ids::{FormKey, SigCode, SubrecordSig};
    use crate::record::{FieldEntry, FieldValue, Record, RecordFlags};
    use crate::schema::AuthoringSchema;
    use crate::sym::StringInterner;

    // ── helpers ──────────────────────────────────────────────────────────────

    fn make_record(sig_str: &str, local: u32, plugin: &str, interner: &StringInterner) -> Record {
        let s = SigCode::from_str(sig_str).unwrap();
        let fk = FormKey {
            local,
            plugin: interner.intern(plugin),
        };
        Record {
            sig: s,
            form_key: fk,
            eid: None,
            flags: RecordFlags::empty(),
            fields: smallvec::SmallVec::new(),
            warnings: smallvec::SmallVec::new(),
        }
    }

    fn push_bytes(record: &mut Record, sig_str: &str, data: Vec<u8>) {
        let s = SubrecordSig::from_str(sig_str).unwrap();
        let mut buf: smallvec::SmallVec<[u8; 32]> = smallvec::SmallVec::new();
        buf.extend_from_slice(&data);
        record.fields.push(FieldEntry {
            sig: s,
            value: FieldValue::Bytes(buf),
        });
    }

    fn push_uint(record: &mut Record, sig_str: &str, n: u64) {
        let s = SubrecordSig::from_str(sig_str).unwrap();
        record.fields.push(FieldEntry {
            sig: s,
            value: FieldValue::Uint(n),
        });
    }

    fn lvlo_entry_with_field(
        field_name: &str,
        local: u32,
        plugin: &str,
        interner: &StringInterner,
    ) -> FieldEntry {
        let field_sym = interner.intern(field_name);
        let fk = FormKey {
            local,
            plugin: interner.intern(plugin),
        };
        FieldEntry {
            sig: SubrecordSig::from_str("LVLO").unwrap(),
            value: FieldValue::Struct(vec![(field_sym, FieldValue::FormKey(fk))]),
        }
    }

    fn ref_sym(interner: &StringInterner) -> Sym {
        interner.intern("Reference")
    }

    fn count_subrecords(record: &Record, sig_str: &str) -> usize {
        let s = SubrecordSig::from_str(sig_str).unwrap();
        record.fields.iter().filter(|e| e.sig == s).count()
    }

    fn first_bytes<'a>(record: &'a Record, sig_str: &str) -> Option<&'a [u8]> {
        let s = SubrecordSig::from_str(sig_str).ok()?;
        for e in record.fields.iter() {
            if e.sig == s {
                if let FieldValue::Bytes(data) = &e.value {
                    return Some(data.as_slice());
                }
            }
        }
        None
    }

    fn first_uint(record: &Record, sig_str: &str) -> Option<u64> {
        let s = SubrecordSig::from_str(sig_str).ok()?;
        for e in record.fields.iter() {
            if e.sig == s {
                if let FieldValue::Uint(n) = &e.value {
                    return Some(*n);
                }
            }
        }
        None
    }

    #[test]
    fn applies_only_to_creature_roots() {
        let schema = AuthoringSchema::for_game("fo4").unwrap();
        for (root, expected) in [
            (Some("NPC_"), true),
            (Some("LVLN"), true),
            (Some("ARMO"), false),
            (None, false),
        ] {
            let config = FixupConfig {
                root_sig: root.map(|sig| SigCode::from_str(sig).unwrap()),
                ..Default::default()
            };
            let ctx = FixupContext {
                source_handle_id: 1,
                target_handle_id: 2,
                schema_target: &schema,
                schema_source: &schema,
                skip_record_sigs: crate::fixups::empty_skip_record_sigs(),
                mod_path: None,
                source_extracted_dir: None,
                target_master_handle_ids: &[],
                config: &config,
            };
            assert_eq!(
                CleanCreatureEspCheckFieldsFixup.applies_to(&ctx),
                expected,
                "{root:?}"
            );
        }
    }

    #[test]
    fn syncs_ksiz_to_kwda_byte_count() {
        for (keywords, ksiz, expected) in [
            (Some(3usize), 1u64, Some(3u64)),
            (Some(2), 2, None),
            (None, 5, None),
        ] {
            let interner = StringInterner::new();
            let mut r = make_record("NPC_", 0x000100, "Out.esp", &interner);
            if let Some(count) = keywords {
                let payload = (0..count as u32)
                    .flat_map(|index| (0x000123 + index).to_le_bytes())
                    .collect();
                push_bytes(&mut r, "KWDA", payload);
            }
            push_uint(&mut r, "KSIZ", ksiz);

            assert_eq!(
                sync_ksiz_to_kwda(&mut r),
                expected.is_some(),
                "{keywords:?}/{ksiz}"
            );
            assert_eq!(first_uint(&r, "KSIZ"), Some(expected.unwrap_or(ksiz)));
        }
    }

    #[test]
    fn leveled_list_header_gains_lvld_and_masks_lvlf() {
        let interner = StringInterner::new();
        let rsym = ref_sym(&interner);

        let mut r = make_record("LVLN", 0x000200, "Out.esp", &interner);
        let (_, changed) = apply_to_record(&mut r, rsym);
        assert!(changed);
        assert_eq!(count_subrecords(&r, "LVLD"), 1);
        assert_eq!(first_uint(&r, "LVLD"), Some(0), "missing LVLD is added");

        let mut r = make_record("LVLN", 0x000201, "Out.esp", &interner);
        push_uint(&mut r, "LVLD", 50);
        let _ = apply_to_record(&mut r, rsym);
        assert_eq!(count_subrecords(&r, "LVLD"), 1);
        assert_eq!(first_uint(&r, "LVLD"), Some(50), "authored LVLD is kept");

        let mut r = make_record("LVLN", 0x000300, "Out.esp", &interner);
        push_uint(&mut r, "LVLF", 0xFF);
        let (_, changed) = apply_to_record(&mut r, rsym);
        assert!(changed);
        assert_eq!(first_uint(&r, "LVLF"), Some(0x07));

        let mut r = make_record("LVLN", 0x000301, "Out.esp", &interner);
        push_uint(&mut r, "LVLF", 0x05);
        assert!(!mask_lvlf_to_fo4_bits(&mut r), "already masked");

        let mut r = make_record("LVLC", 0x000302, "Out.esp", &interner);
        push_bytes(&mut r, "LVLF", vec![0xF0]);
        assert!(mask_lvlf_to_fo4_bits(&mut r));
        assert_eq!(
            first_bytes(&r, "LVLF").unwrap()[0],
            0x00,
            "byte payload is masked"
        );
    }

    #[test]
    fn lvln_strips_fo76_only_list_subrecords() {
        let mut interner = StringInterner::new();
        let mut r = make_record("LVLN", 0x000303, "Out.esp", &mut interner);
        for sig in ["ONAM", "LVMV", "LVIV", "LVLV", "ENLS", "AUUV"] {
            push_bytes(&mut r, sig, vec![1]);
        }
        push_uint(&mut r, "LLCT", 1);
        r.fields.push(lvlo_entry_with_field(
            "Reference",
            0x001000,
            "Out.esp",
            &interner,
        ));

        let rsym = ref_sym(&mut interner);
        let (dropped, changed) = apply_to_record(&mut r, rsym);
        assert!(changed);
        assert_eq!(dropped, 6);
        for sig in ["ONAM", "LVMV", "LVIV", "LVLV", "ENLS", "AUUV"] {
            assert_eq!(count_subrecords(&r, sig), 0);
        }
        assert_eq!(count_subrecords(&r, "LVLO"), 1);
    }

    /// LVLI rows pre-seed LLCT so the count sync is a no-op: without it the
    /// missing-LLCT branch appends LLCT=0 and reports `changed`.
    #[test]
    fn leveled_lists_drop_null_references_and_sync_llct() {
        for (name, sig, llct, entries, dropped, lvlo, expected_llct, changed) in [
            (
                "lvln_null",
                "LVLN",
                Some(3),
                vec![
                    ("Reference", 0x001000, "Out.esp"),
                    ("Reference", 0, "Out.esp"),
                    ("Reference", 0x002000, "Out.esp"),
                ],
                1,
                2,
                2,
                true,
            ),
            (
                "lvln_missing_llct",
                "LVLN",
                None,
                vec![
                    ("Reference", 0x001000, "Out.esp"),
                    ("Reference", 0, "Out.esp"),
                ],
                1,
                1,
                1,
                true,
            ),
            (
                "lvli_null",
                "LVLI",
                Some(2),
                vec![
                    ("Reference", 0, "Out.esp"),
                    ("Reference", 0x003000, "Out.esp"),
                ],
                1,
                1,
                1,
                true,
            ),
            (
                "lvli_fo4_item_reference",
                "LVLI",
                Some(1),
                vec![("item", 0x00000F, "Fallout4.esm")],
                0,
                1,
                1,
                false,
            ),
            ("lvli_empty", "LVLI", Some(0), vec![], 0, 0, 0, false),
        ] {
            let interner = StringInterner::new();
            let mut r = make_record(sig, 0x000400, "Out.esp", &interner);
            if let Some(count) = llct {
                push_uint(&mut r, "LLCT", count);
            }
            for (field, local, plugin) in entries {
                r.fields
                    .push(lvlo_entry_with_field(field, local, plugin, &interner));
            }

            let result = apply_to_record(&mut r, ref_sym(&interner));
            assert_eq!(result, (dropped, changed), "{name}");
            assert_eq!(count_subrecords(&r, "LVLO"), lvlo, "{name}");
            assert_eq!(first_uint(&r, "LLCT"), Some(expected_llct), "{name}");
            if sig == "LVLI" {
                assert_eq!(count_subrecords(&r, "LVLD"), 0, "{name}");
                assert_eq!(count_subrecords(&r, "LVLF"), 0, "{name}");
            }
        }
    }

    /// ALCH drops EFIT only when no EFID pairs with it; QUST drops only an
    /// FNAM wider than 8 bytes.
    #[test]
    fn strips_fo76_only_subrecords_per_record_type() {
        for (sig, fields, dropped, remaining) in [
            (
                "SNDR",
                vec![
                    ("HNAM", 4),
                    ("INAM", 4),
                    ("PNAM", 1),
                    ("QNAM", 1),
                    ("GNAM", 1),
                ],
                4,
                vec!["GNAM"],
            ),
            ("SNDR", vec![("GNAM", 1)], 0, vec!["GNAM"]),
            (
                "MGEF",
                vec![("VMAD", 3), ("CTDA", 32), ("DNAM", 1)],
                2,
                vec!["DNAM"],
            ),
            ("ALCH", vec![("CTDA", 32), ("EFIT", 12)], 2, vec![]),
            (
                "SPEL",
                vec![("CTDA", 32), ("EFID", 4), ("EFIT", 12)],
                1,
                vec!["EFID", "EFIT"],
            ),
            ("ENCH", vec![("CTDA", 32)], 1, vec![]),
            (
                "QUST",
                vec![("VMAD", 3), ("CTDA", 32), ("FNAM", 16)],
                3,
                vec![],
            ),
            ("QUST", vec![("FNAM", 4)], 0, vec!["FNAM"]),
            ("ARMO", vec![("DNAM", 1)], 0, vec!["DNAM"]),
        ] {
            let interner = StringInterner::new();
            let mut r = make_record(sig, 0x000600, "Out.esp", &interner);
            for (field, len) in &fields {
                push_bytes(&mut r, field, vec![1u8; *len]);
            }
            let label = format!("{sig} {fields:?}");

            let result = apply_to_record(&mut r, ref_sym(&interner));
            assert_eq!(result, (dropped, dropped > 0), "{label}");
            let observed: Vec<&str> = r.fields.iter().map(|e| e.sig.as_str()).collect();
            assert_eq!(observed, remaining, "{label}");
        }
    }
}
