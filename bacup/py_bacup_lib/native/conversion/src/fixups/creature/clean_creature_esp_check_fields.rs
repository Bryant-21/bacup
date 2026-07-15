//! Fixup: strip or normalise FO76 fields that produce FO4 ESP checker errors.
//!

//!
//! # What this does
//! For creature conversions (root sig NPC_ or LVLN), iterates every record in
//! the target plugin and applies per-record-type normalisations.  The branches
//! ported at the schema-decoded `Record` / `FieldValue` level are:
//!
//! - **All records**: sync `KSIZ` to actual `KWDA` row count.
//! - **LVLN / LVLC**: ensure `LVLD` (Chance None) is present, mask `LVLF` to the
//!   FO4 3-bit range, drop FO76-only list subrecords, drop `LVLO`/`LVLE`
//!   entries with null `Reference`, sync `LLCT` to the surviving entry count.
//! - **LVLI**: drop `LVLO`/`LVLE` entries with null `Reference`, sync `LLCT`.
//! - **SNDR**: remove subrecords `HNAM`, `INAM`, `PNAM`, `QNAM`.
//! - **MGEF**: remove `VMAD` and `CTDA`.
//! - **ALCH / ENCH / SPEL**: remove `CTDA`; remove `EFIT` when no `EFID`
//!   precedes (Python keys on `BaseEffect`, which is the YAML name for `EFID`).
//! - **QUST**: remove `VMAD`, `CTDA`, and `FNAM` payloads larger than 8 bytes.
//!
//! # Branches deferred to typed-struct decode
//! The Python YAML-canonical view exposes per-field semantics that the current
//! Rust pipeline does not surface for `struct:...` codecs (these decode to
//! `FieldValue::Bytes`).  Following the rationale in
//! `fix_creature_npc_records.rs`, the following branches are documented but
//! not ported:
//!
//! - **STAG non-SNDR `Sound` stripping** — already covered by the standalone
//!   `fix_stag_sound_refs` fixup, which runs as part of the standard registry.
//! - **NPC_ `Unused` field removal** — `Unused` is the YAML name for an
//!   unnamed struct field; no FO4 NPC_ subrecord has the sig "Unused".
//! - **WEAP raw-hex `Data` → structured FO4 default** — Python keys on the
//!   `raw_hex` marker that only appears in the YAML view when the translator
//!   failed to decode the struct.  The Rust pipeline always emits DNAM as
//!   `FieldValue::Bytes`, so there is no signal to distinguish "raw-hex from a
//!   failed decode" from a normal decode.  Deferred to typed-struct decode.
//! - **QUST `QuestDialogueConditions` / alias scrubbing / `NextAliasID` reset**
//!   — these are YAML field names (`ReferenceAliasID`, `CollectionAliasID`,
//!   `ALID`, `ALED`, …) that don't map 1:1 to FO4 subrecord sigs.
//! - **KYWD record-flag 0x10 strip / global FO76 0x10 flag strip** —
//!   `RecordFlags::from_bits_truncate` already drops unknown bits on
//!   `source_read`, so the FO76 0x10 flag never survives into the Rust
//!   `Record`.  No-op in the current pipeline.
//!
//! # Reference type lookup
//! Python looks up the record type for a referenced FormKey via
//! `fk_to_type` (built from the conversion graph) and a SQLite fallback.
//! The Rust pipeline has no graph view here; the relevant lookups are
//! confined to STAG branches that already live in `fix_stag_sound_refs.rs`.

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

/// Ensure `LVLD` (Chance None) is present.  Adds a zero-valued LVLD if absent.
///
/// In the decoded view, LVLD's codec is `uint8` → `FieldValue::Uint(0)`.
/// We treat "missing" as the only case to add; an already-present LVLD
/// (even Uint(0)) is left alone — equivalent to Python's behaviour where a
/// numeric LVLD short-circuits the `raw_hex` check.
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

/// Remove every `LVLO`/`LVLE` subrecord whose `Reference` field is a null
/// FormKey (`local == 0`) or is missing/non-FormKey.  Returns the count of
/// entries removed.
///
/// Drop-only on null: a leveled entry is removed only when its reference FK is
/// empty (no master-existence check).
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
    use crate::fixups::{FixupConfig, FixupContext, FixupRegistry};
    use crate::formkey_mapper::{FormKeyMapper, MapperOptions};
    use crate::ids::{FormKey, SigCode, SubrecordSig};
    use crate::record::{FieldEntry, FieldValue, Record, RecordFlags};
    use crate::schema::AuthoringSchema;
    use crate::session::open_session;
    use crate::sym::StringInterner;
    use esp_authoring_core::plugin_runtime::plugin_handle_new_native;

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

    fn lvlo_entry(local: u32, plugin: &str, interner: &StringInterner) -> FieldEntry {
        lvlo_entry_with_field("Reference", local, plugin, interner)
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
    fn registry_no_op_when_no_records() {
        let schema = AuthoringSchema::for_game("fo4").expect("fo4 schema");
        let target_handle =
            plugin_handle_new_native("CleanCreatureEspCheckFieldsTest.esp", Some("fo4"))
                .expect("test plugin handle");
        let config = FixupConfig {
            root_sig: Some(SigCode::from_str("NPC_").unwrap()),
            target_schema: Some(schema.clone()),
            ..Default::default()
        };
        let mut mapper_interner = StringInterner::new();
        let mut mapper = FormKeyMapper::new([], MapperOptions::default(), &mut mapper_interner);
        let mut session = open_session(target_handle, None).expect("open session");

        let mut registry = FixupRegistry::new();
        registry.register(Box::new(CleanCreatureEspCheckFieldsFixup));
        let reports = registry
            .run_all_in_session(&mut session, &mut mapper, &config)
            .expect("run_all_in_session");
        assert_eq!(reports.len(), 1);
        assert!(reports[0].1.is_no_op());
    }

    #[test]
    fn applies_to_npc_root() {
        let schema = AuthoringSchema::for_game("fo4").unwrap();
        let config = FixupConfig {
            root_sig: Some(SigCode::from_str("NPC_").unwrap()),
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
        assert!(CleanCreatureEspCheckFieldsFixup.applies_to(&ctx));
    }

    #[test]
    fn applies_to_lvln_root() {
        let schema = AuthoringSchema::for_game("fo4").unwrap();
        let config = FixupConfig {
            root_sig: Some(SigCode::from_str("LVLN").unwrap()),
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
        assert!(CleanCreatureEspCheckFieldsFixup.applies_to(&ctx));
    }

    #[test]
    fn does_not_apply_when_no_root_sig() {
        let schema = AuthoringSchema::for_game("fo4").unwrap();
        let config = FixupConfig::default();
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
        assert!(!CleanCreatureEspCheckFieldsFixup.applies_to(&ctx));
    }

    #[test]
    fn does_not_apply_to_armo_root() {
        let schema = AuthoringSchema::for_game("fo4").unwrap();
        let config = FixupConfig {
            root_sig: Some(SigCode::from_str("ARMO").unwrap()),
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
        assert!(!CleanCreatureEspCheckFieldsFixup.applies_to(&ctx));
    }

    #[test]
    fn syncs_ksiz_to_kwda_byte_count() {
        let mut interner = StringInterner::new();
        let mut r = make_record("NPC_", 0x000100, "Out.esp", &mut interner);
        // KWDA holds 3 FormIDs (12 bytes), KSIZ claims 1.
        let mut kwda_payload = Vec::new();
        for fid in [0x000123_u32, 0x000456_u32, 0x000789_u32] {
            kwda_payload.extend_from_slice(&fid.to_le_bytes());
        }
        push_bytes(&mut r, "KWDA", kwda_payload);
        push_uint(&mut r, "KSIZ", 1);

        let changed = sync_ksiz_to_kwda(&mut r);
        assert!(changed);
        assert_eq!(first_uint(&r, "KSIZ"), Some(3));
    }

    #[test]
    fn ksiz_sync_noop_when_already_in_sync() {
        let mut interner = StringInterner::new();
        let mut r = make_record("NPC_", 0x000101, "Out.esp", &mut interner);
        let mut kwda_payload = Vec::new();
        for fid in [0x000123_u32, 0x000456_u32] {
            kwda_payload.extend_from_slice(&fid.to_le_bytes());
        }
        push_bytes(&mut r, "KWDA", kwda_payload);
        push_uint(&mut r, "KSIZ", 2);

        let changed = sync_ksiz_to_kwda(&mut r);
        assert!(!changed);
    }

    #[test]
    fn ksiz_sync_noop_when_kwda_absent() {
        let mut interner = StringInterner::new();
        let mut r = make_record("NPC_", 0x000102, "Out.esp", &mut interner);
        push_uint(&mut r, "KSIZ", 5);

        let changed = sync_ksiz_to_kwda(&mut r);
        assert!(!changed);
    }

    #[test]
    fn lvln_lvld_added_when_missing() {
        let mut interner = StringInterner::new();
        let mut r = make_record("LVLN", 0x000200, "Out.esp", &mut interner);
        let rsym = ref_sym(&mut interner);
        let (_, changed) = apply_to_record(&mut r, rsym);
        assert!(changed);
        assert_eq!(count_subrecords(&r, "LVLD"), 1);
        assert_eq!(first_uint(&r, "LVLD"), Some(0));
    }

    #[test]
    fn lvln_lvld_preserved_when_present() {
        let mut interner = StringInterner::new();
        let mut r = make_record("LVLN", 0x000201, "Out.esp", &mut interner);
        push_uint(&mut r, "LVLD", 50);
        let rsym = ref_sym(&mut interner);
        let _ = apply_to_record(&mut r, rsym);
        assert_eq!(count_subrecords(&r, "LVLD"), 1);
        assert_eq!(first_uint(&r, "LVLD"), Some(50));
    }

    #[test]
    fn lvln_lvlf_masked_to_low_three_bits() {
        let mut interner = StringInterner::new();
        let mut r = make_record("LVLN", 0x000300, "Out.esp", &mut interner);
        push_uint(&mut r, "LVLF", 0xFF);
        let rsym = ref_sym(&mut interner);
        let (_, changed) = apply_to_record(&mut r, rsym);
        assert!(changed);
        assert_eq!(first_uint(&r, "LVLF"), Some(0x07));
    }

    #[test]
    fn lvln_lvlf_already_masked_is_noop() {
        let mut interner = StringInterner::new();
        let mut r = make_record("LVLN", 0x000301, "Out.esp", &mut interner);
        push_uint(&mut r, "LVLF", 0x05);
        push_uint(&mut r, "LVLD", 0); // pre-seed so no LVLD synthesis fires
        let rsym = ref_sym(&mut interner);
        let lvlf_changed = mask_lvlf_to_fo4_bits(&mut r);
        assert!(!lvlf_changed);
        let _ = rsym;
    }

    #[test]
    fn lvlc_lvlf_byte_payload_is_masked() {
        let mut interner = StringInterner::new();
        let mut r = make_record("LVLC", 0x000302, "Out.esp", &mut interner);
        push_bytes(&mut r, "LVLF", vec![0xF0]);
        let changed = mask_lvlf_to_fo4_bits(&mut r);
        assert!(changed);
        let data = first_bytes(&r, "LVLF").unwrap();
        assert_eq!(data[0], 0x00, "0xF0 & 0x07 = 0");
    }

    #[test]
    fn lvln_strips_fo76_only_list_subrecords() {
        let mut interner = StringInterner::new();
        let mut r = make_record("LVLN", 0x000303, "Out.esp", &mut interner);
        for sig in ["ONAM", "LVMV", "LVIV", "LVLV", "ENLS", "AUUV"] {
            push_bytes(&mut r, sig, vec![1]);
        }
        push_uint(&mut r, "LLCT", 1);
        r.fields
            .push(lvlo_entry(0x001000, "Out.esp", &mut interner));

        let rsym = ref_sym(&mut interner);
        let (dropped, changed) = apply_to_record(&mut r, rsym);
        assert!(changed);
        assert_eq!(dropped, 6);
        for sig in ["ONAM", "LVMV", "LVIV", "LVLV", "ENLS", "AUUV"] {
            assert_eq!(count_subrecords(&r, sig), 0);
        }
        assert_eq!(count_subrecords(&r, "LVLO"), 1);
    }

    #[test]
    fn lvln_drops_null_reference_and_syncs_llct() {
        let mut interner = StringInterner::new();
        let mut r = make_record("LVLN", 0x000400, "Out.esp", &mut interner);
        push_uint(&mut r, "LLCT", 3);
        // 2 valid, 1 null.
        r.fields
            .push(lvlo_entry(0x001000, "Out.esp", &mut interner));
        r.fields
            .push(lvlo_entry(0x000000, "Out.esp", &mut interner));
        r.fields
            .push(lvlo_entry(0x002000, "Out.esp", &mut interner));

        let rsym = ref_sym(&mut interner);
        let (dropped, changed) = apply_to_record(&mut r, rsym);
        assert!(changed);
        assert_eq!(dropped, 1, "exactly one null LVLO must be dropped");
        // 2 valid LVLO entries remain.
        assert_eq!(count_subrecords(&r, "LVLO"), 2);
        assert_eq!(first_uint(&r, "LLCT"), Some(2));
    }

    #[test]
    fn lvln_drops_null_reference_and_creates_llct_when_missing() {
        let mut interner = StringInterner::new();
        let mut r = make_record("LVLN", 0x000401, "Out.esp", &mut interner);
        r.fields
            .push(lvlo_entry(0x001000, "Out.esp", &mut interner));
        r.fields
            .push(lvlo_entry(0x000000, "Out.esp", &mut interner));

        let rsym = ref_sym(&mut interner);
        let (dropped, changed) = apply_to_record(&mut r, rsym);
        assert!(changed);
        assert_eq!(dropped, 1);
        // LLCT was missing; must be appended at 1.
        assert_eq!(first_uint(&r, "LLCT"), Some(1));
    }

    #[test]
    fn lvli_drops_null_reference_and_syncs_llct() {
        let mut interner = StringInterner::new();
        let mut r = make_record("LVLI", 0x000500, "Out.esp", &mut interner);
        push_uint(&mut r, "LLCT", 2);
        r.fields
            .push(lvlo_entry(0x000000, "Out.esp", &mut interner));
        r.fields
            .push(lvlo_entry(0x003000, "Out.esp", &mut interner));

        let rsym = ref_sym(&mut interner);
        let (dropped, changed) = apply_to_record(&mut r, rsym);
        assert!(changed);
        assert_eq!(dropped, 1);
        assert_eq!(count_subrecords(&r, "LVLO"), 1);
        assert_eq!(first_uint(&r, "LLCT"), Some(1));
    }

    #[test]
    fn lvli_keeps_item_reference_and_syncs_llct() {
        let mut interner = StringInterner::new();
        let mut r = make_record("LVLI", 0x000502, "Out.esp", &mut interner);
        push_uint(&mut r, "LLCT", 1);
        r.fields.push(lvlo_entry_with_field(
            "item",
            0x00000F,
            "Fallout4.esm",
            &mut interner,
        ));

        let rsym = ref_sym(&mut interner);
        let (dropped, changed) = apply_to_record(&mut r, rsym);

        assert_eq!(dropped, 0, "FO4 LVLI item reference should be kept");
        assert!(!changed);
        assert_eq!(count_subrecords(&r, "LVLO"), 1);
        assert_eq!(first_uint(&r, "LLCT"), Some(1));
    }

    #[test]
    fn lvli_does_not_synthesize_lvld_or_lvlf() {
        let mut interner = StringInterner::new();
        let mut r = make_record("LVLI", 0x000501, "Out.esp", &mut interner);
        // Pre-seed LLCT=0 so the count-sync branch is a no-op.  Without it
        // the missing-LLCT branch appends LLCT=0 and reports `changed=true`,
        // matching Python where `field_at(...) != entry_count` is True for
        // `None != 0`.
        push_uint(&mut r, "LLCT", 0);
        let rsym = ref_sym(&mut interner);
        let (_, changed) = apply_to_record(&mut r, rsym);
        assert!(!changed);
        assert_eq!(count_subrecords(&r, "LVLD"), 0);
        assert_eq!(count_subrecords(&r, "LVLF"), 0);
    }

    #[test]
    fn sndr_strips_hnam_inam_pnam_qnam() {
        let mut interner = StringInterner::new();
        let mut r = make_record("SNDR", 0x000600, "Out.esp", &mut interner);
        push_bytes(&mut r, "HNAM", vec![1, 2, 3, 4]);
        push_bytes(&mut r, "INAM", vec![5, 6, 7, 8]);
        push_bytes(&mut r, "PNAM", vec![9]);
        push_bytes(&mut r, "QNAM", vec![10]);
        // A non-rejected subrecord must survive.
        push_bytes(&mut r, "GNAM", vec![0xAA]);

        let rsym = ref_sym(&mut interner);
        let (dropped, changed) = apply_to_record(&mut r, rsym);
        assert!(changed);
        assert_eq!(dropped, 4);
        assert_eq!(count_subrecords(&r, "HNAM"), 0);
        assert_eq!(count_subrecords(&r, "INAM"), 0);
        assert_eq!(count_subrecords(&r, "PNAM"), 0);
        assert_eq!(count_subrecords(&r, "QNAM"), 0);
        assert_eq!(count_subrecords(&r, "GNAM"), 1);
    }

    #[test]
    fn sndr_with_none_of_the_rejected_subrecords_is_noop() {
        let mut interner = StringInterner::new();
        let mut r = make_record("SNDR", 0x000601, "Out.esp", &mut interner);
        push_bytes(&mut r, "GNAM", vec![0xAA]);
        let rsym = ref_sym(&mut interner);
        let (dropped, changed) = apply_to_record(&mut r, rsym);
        assert!(!changed);
        assert_eq!(dropped, 0);
    }

    #[test]
    fn mgef_strips_vmad_and_ctda() {
        let mut interner = StringInterner::new();
        let mut r = make_record("MGEF", 0x000700, "Out.esp", &mut interner);
        push_bytes(&mut r, "VMAD", vec![1, 2, 3]);
        push_bytes(&mut r, "CTDA", vec![0u8; 32]);
        push_bytes(&mut r, "DNAM", vec![0xAA]);

        let rsym = ref_sym(&mut interner);
        let (dropped, changed) = apply_to_record(&mut r, rsym);
        assert!(changed);
        assert_eq!(dropped, 2);
        assert_eq!(count_subrecords(&r, "VMAD"), 0);
        assert_eq!(count_subrecords(&r, "CTDA"), 0);
        assert_eq!(count_subrecords(&r, "DNAM"), 1);
    }

    #[test]
    fn alch_strips_ctda_and_efit_when_efid_missing() {
        let mut interner = StringInterner::new();
        let mut r = make_record("ALCH", 0x000800, "Out.esp", &mut interner);
        push_bytes(&mut r, "CTDA", vec![0u8; 32]);
        push_bytes(&mut r, "EFIT", vec![1u8; 12]);

        let rsym = ref_sym(&mut interner);
        let (dropped, changed) = apply_to_record(&mut r, rsym);
        assert!(changed);
        assert_eq!(dropped, 2);
        assert_eq!(count_subrecords(&r, "CTDA"), 0);
        assert_eq!(count_subrecords(&r, "EFIT"), 0);
    }

    #[test]
    fn spel_keeps_efit_when_efid_present() {
        let mut interner = StringInterner::new();
        let mut r = make_record("SPEL", 0x000801, "Out.esp", &mut interner);
        push_bytes(&mut r, "CTDA", vec![0u8; 32]);
        push_bytes(&mut r, "EFID", vec![0u8; 4]);
        push_bytes(&mut r, "EFIT", vec![1u8; 12]);

        let rsym = ref_sym(&mut interner);
        let (dropped, changed) = apply_to_record(&mut r, rsym);
        assert!(changed);
        assert_eq!(
            dropped, 1,
            "only CTDA should be dropped when EFID is present"
        );
        assert_eq!(count_subrecords(&r, "EFID"), 1);
        assert_eq!(count_subrecords(&r, "EFIT"), 1);
    }

    #[test]
    fn ench_strips_ctda_only_when_no_effects() {
        let mut interner = StringInterner::new();
        let mut r = make_record("ENCH", 0x000802, "Out.esp", &mut interner);
        push_bytes(&mut r, "CTDA", vec![0u8; 32]);
        let rsym = ref_sym(&mut interner);
        let (dropped, changed) = apply_to_record(&mut r, rsym);
        assert!(changed);
        assert_eq!(dropped, 1);
        assert_eq!(count_subrecords(&r, "CTDA"), 0);
    }

    #[test]
    fn qust_strips_vmad_ctda_and_oversize_fnam() {
        let mut interner = StringInterner::new();
        let mut r = make_record("QUST", 0x000900, "Out.esp", &mut interner);
        push_bytes(&mut r, "VMAD", vec![1, 2, 3]);
        push_bytes(&mut r, "CTDA", vec![0u8; 32]);
        push_bytes(&mut r, "FNAM", vec![0u8; 16]); // >8 bytes

        let rsym = ref_sym(&mut interner);
        let (dropped, changed) = apply_to_record(&mut r, rsym);
        assert!(changed);
        assert_eq!(dropped, 3);
        assert_eq!(count_subrecords(&r, "VMAD"), 0);
        assert_eq!(count_subrecords(&r, "CTDA"), 0);
        assert_eq!(count_subrecords(&r, "FNAM"), 0);
    }

    #[test]
    fn qust_preserves_small_fnam() {
        let mut interner = StringInterner::new();
        let mut r = make_record("QUST", 0x000901, "Out.esp", &mut interner);
        push_bytes(&mut r, "FNAM", vec![0u8; 4]);

        let rsym = ref_sym(&mut interner);
        let (_, changed) = apply_to_record(&mut r, rsym);
        assert!(!changed);
        assert_eq!(count_subrecords(&r, "FNAM"), 1);
    }

    #[test]
    fn unrelated_record_type_with_no_kwda_is_noop() {
        let mut interner = StringInterner::new();
        let mut r = make_record("ARMO", 0x000A00, "Out.esp", &mut interner);
        push_bytes(&mut r, "DNAM", vec![0xAA]);

        let rsym = ref_sym(&mut interner);
        let (_, changed) = apply_to_record(&mut r, rsym);
        assert!(!changed);
    }
}
