//! Fixup: drop loading-screen (LSCR) records whose gating conditions reference
//! FO76-only condition functions that have no FO4 equivalent.
//!
//! # Why
//! The translation pair hook (`drop_fo4_incompatible_conditions`) strips any
//! CTDA whose function id is FO4-incompatible (id > 817, or a FO76-only
//! blocklist entry) to keep the FO4 CK from indexing a non-existent
//! function-table slot and crashing on load. For most record types an
//! over-permissive condition list is harmless, but a loading screen that loses a
//! gating condition becomes UNCONDITIONALLY eligible and shows constantly. FO76
//! gates many of its loading screens (public-event / region / shelter screens)
//! on such functions.
//!
//! `IsQuestActive` (876) IS faithfully remapped to FO4 `GetQuestRunning` (56) by
//! the pair hook, so it is NOT treated as untranslatable here (see
//! `FO76_REMAPPED_CONDITION_FUNCTION_IDS`). Every other FO4-incompatible
//! function on an LSCR has no faithful conversion, so the safe action is to drop
//! the whole loading screen rather than leave it always-on.
//!
//! Scoped to LSCR on purpose: applying "drop the record" to the thousands of
//! INFO/COBJ/PERK records also gated by FO76-only functions would gut the game.
//! For those, dropping just the condition (the pair-hook behavior) is retained.
//!
//! Source-driven: the decision reads the FO76 source LSCR (whose original CTDA
//! functions are still present); the matching output LSCR — already condition-
//! stripped by translation — is then removed. Every drop is logged with the
//! offending function id(s).

use crate::fixups::{Fixup, FixupConfig, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::ids::SigCode;
use crate::record::{FieldValue, Record};
use crate::session::PluginSession;
use crate::translator::pair_hooks::fo76_fo4::{
    FO76_REMAPPED_CONDITION_FUNCTION_IDS, is_fo4_incompatible_condition_function_id,
};

pub struct DropUntranslatableLoadscreenRecordsFixup;

impl Fixup for DropUntranslatableLoadscreenRecordsFixup {
    fn name(&self) -> &'static str {
        "drop_untranslatable_loadscreen_records"
    }

    fn uses_session(&self) -> bool {
        true
    }

    fn applies_to_session(&self, session: &PluginSession, _config: &FixupConfig) -> bool {
        // Needs the FO76 source to know which conditions were originally present.
        session.source_id().is_some()
    }

    fn run_with_session(
        &self,
        session: &mut PluginSession,
        mapper: &mut FormKeyMapper,
        _config: &FixupConfig,
    ) -> Result<FixupReport, FixupError> {
        let mut report = FixupReport::empty();

        let source_schema = session
            .source_schema()
            .map_err(|e| FixupError::HandleError(e.to_string()))?;
        let lscr_sig =
            SigCode::from_str("LSCR").map_err(|e| FixupError::SchemaError(e.to_string()))?;
        let source_fks = session
            .source_form_keys_of_sig(lscr_sig, mapper.interner)
            .map_err(|e| FixupError::HandleError(e.to_string()))?;

        let mut dropped = 0u32;
        for fk in source_fks {
            let source_record =
                match session.source_record_decoded(&fk, &source_schema, mapper.interner) {
                    Ok(record) => record,
                    Err(_) => continue,
                };
            let untranslatable = collect_untranslatable_functions(&source_record);
            if untranslatable.is_empty() {
                continue;
            }
            let removed = session
                .remove_record(&fk)
                .map_err(|e| FixupError::HandleError(e.to_string()))?;
            if !removed {
                continue;
            }
            dropped += 1;
            let plugin = mapper.interner.resolve(fk.plugin).unwrap_or("?");
            let eid = source_record
                .eid
                .and_then(|s| mapper.interner.resolve(s))
                .unwrap_or("<no-eid>");
            eprintln!(
                "[drop-untranslatable-lscr] dropped LSCR {eid} ({:06X}@{plugin}): \
                 untranslatable condition function(s) {untranslatable:?} \
                 (no FO4 equivalent; gating lost)",
                fk.local,
            );
            let warning = mapper.interner.intern(&format!(
                "drop_untranslatable_lscr:{:06X}@{plugin}:{eid}:functions={untranslatable:?}",
                fk.local,
            ));
            report.warnings.push(warning);
        }

        report.records_dropped = dropped;
        Ok(report)
    }
}

/// Collect the FO4-incompatible-and-unmapped CTDA function ids on a record.
/// Empty means every condition either has an FO4 equivalent or is remapped by
/// the pair hook (e.g. 876 IsQuestActive → 56 GetQuestRunning).
fn collect_untranslatable_functions(record: &Record) -> Vec<u16> {
    let mut out = Vec::new();
    for entry in &record.fields {
        if !matches!(&entry.sig.0, b"CTDA" | b"CTDT") {
            continue;
        }
        let FieldValue::Bytes(bytes) = &entry.value else {
            continue;
        };
        if bytes.len() < 10 {
            continue;
        }
        let function_id = u16::from_le_bytes([bytes[8], bytes[9]]);
        if is_fo4_incompatible_condition_function_id(function_id)
            && !FO76_REMAPPED_CONDITION_FUNCTION_IDS.contains(&function_id)
        {
            out.push(function_id);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::{FormKey, SigCode, SubrecordSig};
    use crate::record::{FieldEntry, RecordFlags};
    use crate::sym::StringInterner;
    use smallvec::SmallVec;

    fn ctda(function_id: u16) -> FieldEntry {
        let mut bytes = vec![0u8; 32];
        bytes[8..10].copy_from_slice(&function_id.to_le_bytes());
        FieldEntry {
            sig: SubrecordSig::from_str("CTDA").unwrap(),
            value: FieldValue::Bytes(SmallVec::from_vec(bytes)),
        }
    }

    fn lscr(fields: Vec<FieldEntry>) -> Record {
        let interner = StringInterner::new();
        Record {
            sig: SigCode::from_str("LSCR").unwrap(),
            form_key: FormKey {
                local: 0x2F8A7A,
                plugin: interner.intern("SeventySix.esm"),
            },
            eid: None,
            flags: RecordFlags::empty(),
            fields: fields.into_iter().collect(),
            warnings: SmallVec::new(),
        }
    }

    #[test]
    fn collects_only_fo4_incompatible_unmapped_functions() {
        // 359: valid FO4 function → keep.
        // 844: FO76 GetIsCurrentLocationExact, remapped to GetInCurrentLocation → keep.
        // 876: FO76 IsQuestActive, remapped to GetQuestRunning → not untranslatable.
        // 867: FO76-only (> 817), no remap → untranslatable.
        let record = lscr(vec![ctda(359), ctda(844), ctda(876), ctda(867)]);
        assert_eq!(collect_untranslatable_functions(&record), vec![867]);
    }

    #[test]
    fn empty_when_every_function_is_translatable() {
        // 560 valid FO4, 844 and 876 remapped → nothing untranslatable.
        let record = lscr(vec![ctda(560), ctda(844), ctda(876)]);
        assert!(collect_untranslatable_functions(&record).is_empty());
    }

    #[test]
    fn flags_blocklist_function_under_max() {
        // 596 is in the FO76-only-under-817 blocklist → untranslatable.
        let record = lscr(vec![ctda(596)]);
        assert_eq!(collect_untranslatable_functions(&record), vec![596]);
    }

    // --- session integration: drop the untranslatable LSCR, keep the rest ---

    use crate::formkey_mapper::{FormKeyMapper, MapperOptions, MapperState};
    use crate::record::FieldValue as FV;
    use crate::session::open_session;
    use esp_authoring_core::plugin_runtime::plugin_handle_new_native;

    fn rec_with(
        sig: &str,
        local: u32,
        eid: &str,
        ctdas: &[u16],
        interner: &StringInterner,
    ) -> Record {
        let eid_sym = interner.intern(eid);
        let mut fields: SmallVec<[FieldEntry; 8]> = smallvec::smallvec![FieldEntry {
            sig: SubrecordSig::from_str("EDID").unwrap(),
            value: FV::String(eid_sym),
        }];
        for &f in ctdas {
            fields.push(ctda(f));
        }
        Record {
            sig: SigCode::from_str(sig).unwrap(),
            form_key: FormKey {
                local,
                plugin: interner.intern("SeventySix.esm"),
            },
            eid: Some(eid_sym),
            flags: RecordFlags::empty(),
            fields,
            warnings: SmallVec::new(),
        }
    }

    fn seed_handle(records: &[Record], interner: &StringInterner) -> u64 {
        let handle = plugin_handle_new_native("SeventySix.esm", Some("fo4")).expect("handle");
        let mut session = open_session(handle, None).expect("session");
        let schema = session.schema().expect("schema");
        for r in records {
            session
                .add_record(r.clone(), schema.as_ref(), interner)
                .expect("add record");
        }
        handle
    }

    #[test]
    fn drops_untranslatable_lscr_keeps_remapped_and_non_lscr() {
        let interner = StringInterner::new();
        // 0x900: LSCR gated by 867 (FO76-only, no remap)      → DROP
        // 0x901: LSCR gated only by 876 (remapped to 56)       → KEEP
        // 0x902: LSCR gated by 359 (valid FO4)                 → KEEP
        // 0x903: INFO gated by 867 (untranslatable, not LSCR)  → KEEP (scope)
        // 0x904: LSCR gated only by 844 (remapped to 359)      → KEEP
        let records = vec![
            rec_with("LSCR", 0x900, "DropMe", &[867], &interner),
            rec_with("LSCR", 0x901, "QuestActive", &[876], &interner),
            rec_with("LSCR", 0x902, "Valid", &[359], &interner),
            rec_with("INFO", 0x903, "InfoKeep", &[867], &interner),
            rec_with("LSCR", 0x904, "CurrentLocationExact", &[844], &interner),
        ];
        let source = seed_handle(&records, &interner);
        let target = seed_handle(&records, &interner);

        let mut state = MapperState::new(std::iter::empty(), MapperOptions::default());
        let mut mapper = FormKeyMapper::from_state(&mut state, &interner);
        let mut session = open_session(target, Some(source)).expect("session");
        let config = FixupConfig::default();

        let report = DropUntranslatableLoadscreenRecordsFixup
            .run_with_session(&mut session, &mut mapper, &config)
            .expect("fixup runs");

        assert_eq!(report.records_dropped, 1, "exactly one LSCR dropped");

        let lscr_sig = SigCode::from_str("LSCR").unwrap();
        let info_sig = SigCode::from_str("INFO").unwrap();
        let remaining_lscr: Vec<u32> = session
            .form_keys_of_sig(lscr_sig, &interner)
            .unwrap()
            .iter()
            .map(|fk| fk.local)
            .collect();
        assert!(
            !remaining_lscr.contains(&0x900),
            "untranslatable LSCR removed"
        );
        assert!(remaining_lscr.contains(&0x901), "remapped-876 LSCR kept");
        assert!(remaining_lscr.contains(&0x902), "valid-function LSCR kept");
        assert!(remaining_lscr.contains(&0x904), "remapped-844 LSCR kept");
        let remaining_info: Vec<u32> = session
            .form_keys_of_sig(info_sig, &interner)
            .unwrap()
            .iter()
            .map(|fk| fk.local)
            .collect();
        assert!(remaining_info.contains(&0x903), "non-LSCR record untouched");
    }
}
