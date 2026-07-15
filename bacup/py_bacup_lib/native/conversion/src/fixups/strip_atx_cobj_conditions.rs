//! Fixup: strip CTDA (condition) subrecords from ATX_ ConstructibleObject records.
//!

//!
//! # What this does
//! FO76 Atom Store items (ATX_co_* EditorIDs) carry HasEntitlement conditions
//! (CTDA subrecords, Function 859) that gate crafting behind a store purchase.
//! FO4 has no Atom Store, so these conditions make the recipes invisible at the
//! workbench.  This fixup strips every CTDA subrecord from any COBJ record whose
//! EditorID starts with "ATX_", making those recipes freely craftable after
//! conversion.
//!

use crate::fixups::{Fixup, FixupConfig, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::ids::{SigCode, SubrecordSig};
use crate::record::Record;
use crate::session::PluginSession;

// ---------------------------------------------------------------------------
// Public fixup struct
// ---------------------------------------------------------------------------

pub struct StripAtxCobjConditionsFixup;

impl Fixup for StripAtxCobjConditionsFixup {
    fn name(&self) -> &'static str {
        "strip_atx_cobj_conditions"
    }

    fn uses_session(&self) -> bool {
        true
    }

    fn applies_to_session(&self, _session: &PluginSession, _config: &FixupConfig) -> bool {
        true
    }

    fn run_with_session(
        &self,
        session: &mut PluginSession,
        mapper: &mut FormKeyMapper,
        config: &FixupConfig,
    ) -> Result<FixupReport, FixupError> {
        let cobj_sig =
            SigCode::from_str("COBJ").map_err(|e| FixupError::SchemaError(e.to_string()))?;

        let target_schema = config
            .target_schema
            .as_deref()
            .ok_or_else(|| FixupError::Other("missing target schema in fixup config".into()))?;
        let mut report = FixupReport::empty();
        let mut changed_records = Vec::new();

        let fks = session
            .form_keys_of_sig(cobj_sig, mapper.interner)
            .map_err(|e| FixupError::HandleError(e.to_string()))?;

        for fk in fks {
            let mut record = match session.record_decoded(&fk, target_schema, mapper.interner) {
                Ok(r) => r,
                Err(e) => {
                    let w = mapper.interner.intern(&format!("atx_cobj_read_err:{e}"));
                    report.warnings.push(w);
                    continue;
                }
            };

            // Skip records whose EditorID does not start with "ATX_".
            let eid_matches = record.eid.map_or(false, |sym| {
                mapper
                    .interner
                    .resolve(sym)
                    .map_or(false, |s| s.starts_with("ATX_"))
            });
            if !eid_matches {
                continue;
            }

            if apply_to_record(&mut record) {
                changed_records.push(record);
                report.records_changed += 1;
            }
        }

        let expected = changed_records.len();
        let replaced = session
            .replace_records_contents(changed_records, target_schema, mapper.interner)
            .map_err(|e| FixupError::HandleError(e.to_string()))?;
        if replaced != expected {
            return Err(FixupError::HandleError(format!(
                "strip_atx_cobj_conditions replaced {replaced} of {expected} expected records"
            )));
        }

        Ok(report)
    }
}

// ---------------------------------------------------------------------------
// Record-level mutation (extracted for unit-test access)
// ---------------------------------------------------------------------------

/// Remove all CTDA subrecords from `record`.
///
/// Returns `true` when at least one CTDA entry was removed.
///

pub fn apply_to_record(record: &mut Record) -> bool {
    let ctda_sig = match SubrecordSig::from_str("CTDA") {
        Ok(s) => s,
        Err(_) => return false,
    };

    let before = record.fields.len();
    record.fields.retain(|entry| entry.sig != ctda_sig);
    let dropped = record.fields.len() < before;
    if dropped {
        // No-op unless this COBJ carries a CITC, but keeps the count in lockstep
        // with the (now zero) CTDA rows if it does.
        record.sync_condition_count();
    }
    dropped
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::{FormKey, SigCode, SubrecordSig};
    use crate::record::{FieldEntry, FieldValue, Record, RecordFlags};
    use crate::sym::StringInterner;

    fn cobj_sig() -> SigCode {
        SigCode::from_str("COBJ").unwrap()
    }

    fn make_fk(hex: &str, plugin: &str, interner: &StringInterner) -> FormKey {
        FormKey::parse(&format!("{hex}@{plugin}"), interner).unwrap()
    }

    fn ctda_entry() -> FieldEntry {
        FieldEntry {
            sig: SubrecordSig::from_str("CTDA").unwrap(),
            value: FieldValue::Bytes(smallvec::smallvec![0u8; 32]),
        }
    }

    fn full_entry(interner: &StringInterner) -> FieldEntry {
        let sym = interner.intern("SomeRecipeName");
        FieldEntry {
            sig: SubrecordSig::from_str("FULL").unwrap(),
            value: FieldValue::String(sym),
        }
    }

    fn make_cobj(
        fk: FormKey,
        eid: Option<&str>,
        entries: Vec<FieldEntry>,
        interner: &StringInterner,
    ) -> Record {
        Record {
            sig: cobj_sig(),
            form_key: fk,
            eid: eid.map(|s| interner.intern(s)),
            flags: RecordFlags::empty(),
            fields: entries.into_iter().collect(),
            warnings: smallvec::SmallVec::new(),
        }
    }

    // -----------------------------------------------------------------------
    // -----------------------------------------------------------------------

    #[test]
    fn apply_to_record_no_op_when_no_ctda() {
        let mut interner = StringInterner::new();
        let fk = make_fk("000800", "Output.esp", &mut interner);
        let entry = full_entry(&mut interner);
        let mut record = make_cobj(fk, Some("ATX_coSomeRecipe"), vec![entry], &mut interner);

        let changed = apply_to_record(&mut record);
        assert!(!changed, "no CTDA entries means no change");
        assert_eq!(record.fields.len(), 1);
    }

    // -----------------------------------------------------------------------
    // -----------------------------------------------------------------------

    #[test]
    fn apply_to_record_strips_single_ctda() {
        let mut interner = StringInterner::new();
        let fk = make_fk("000800", "Output.esp", &mut interner);
        let mut record = make_cobj(
            fk,
            Some("ATX_coSomeRecipe"),
            vec![ctda_entry()],
            &mut interner,
        );

        let changed = apply_to_record(&mut record);
        assert!(changed, "CTDA should be stripped");
        assert!(record.fields.is_empty());
    }

    // -----------------------------------------------------------------------
    // -----------------------------------------------------------------------

    #[test]
    fn apply_to_record_strips_all_ctda_entries() {
        let mut interner = StringInterner::new();
        let fk = make_fk("000800", "Output.esp", &mut interner);
        let mut record = make_cobj(
            fk,
            Some("ATX_coSomeRecipe"),
            vec![ctda_entry(), ctda_entry(), ctda_entry()],
            &mut interner,
        );

        let changed = apply_to_record(&mut record);
        assert!(changed);
        assert!(
            record.fields.is_empty(),
            "all three CTDA entries should be removed"
        );
    }

    // -----------------------------------------------------------------------
    // -----------------------------------------------------------------------

    #[test]
    fn apply_to_record_keeps_non_ctda_fields() {
        let mut interner = StringInterner::new();
        let fk = make_fk("000800", "Output.esp", &mut interner);
        let full = full_entry(&mut interner);
        let mut record = make_cobj(
            fk,
            Some("ATX_coSomeRecipe"),
            vec![full, ctda_entry(), ctda_entry()],
            &mut interner,
        );

        let changed = apply_to_record(&mut record);
        assert!(changed);
        assert_eq!(
            record.fields.len(),
            1,
            "FULL should survive; both CTDA removed"
        );
        assert_eq!(record.fields[0].sig.as_str(), "FULL");
    }

    // -----------------------------------------------------------------------
    // -----------------------------------------------------------------------

    #[test]
    fn apply_to_record_empty_record_no_op() {
        let mut interner = StringInterner::new();
        let fk = make_fk("000800", "Output.esp", &mut interner);
        let mut record = make_cobj(fk, Some("ATX_coSomeRecipe"), vec![], &mut interner);

        let changed = apply_to_record(&mut record);
        assert!(!changed);
        assert!(record.fields.is_empty());
    }
}
