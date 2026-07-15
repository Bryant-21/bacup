//! Fixup: null the `Keyword` field of QUST ALLA "Linked Alias" elements whose
//! keyword FormID does not resolve to a KYWD in the output plugin or its masters.
//!
//! # Root cause
//! ALLA "Linked Aliases" is `array_struct:I,i` = (KYWD FormID `linked_aliases_keyword`,
//! alias-index i32). The keyword is a KYWD formlink (`null_allowed`). Some FO76
//! QUSTs carry an ALLA keyword pointing at a FO76 self-KYWD (e.g. `RELinkPatrol`,
//! `SeventySix.esm:0002FD66`) that has NO FO4 equivalent and is dropped during
//! conversion. The dropped record's FormID is left UNMAPPED (still load index 0),
//! so under the FO4 master order it now addresses a *different* record in
//! `Fallout4.esm` (e.g. `0002FD66` is a REFR there). xEdit reports
//! "ALLA \ Linked Alias \ Keyword -> Found a REFR reference, expected: KYWD,NULL".
//!
//! # What this does
//! Builds the set of every valid KYWD encoded FormID (output plugin + target
//! masters), in the same `(load_index << 24) | local` encoding the ALLA keyword
//! bytes use. For each output QUST, every ALLA element whose keyword is non-zero
//! and not a known KYWD has its keyword zeroed (KYWD is `null_allowed`, so a NULL
//! keyword is FO4-valid). The alias-index half of the element is untouched (its
//! dangling-index cleanup happens in `target_normalize::emit_qust_alias_segment`).
//!
//! Idempotent: an ALLA whose keywords all resolve (or are already NULL) is left
//! byte-identical. ALLA reaches the decoded record as raw `Bytes` (the generic
//! decoder leaves `array_struct:` codecs unparsed).

use rustc_hash::FxHashSet;

use crate::fixups::{Fixup, FixupConfig, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::ids::{FormKey, SigCode};
use crate::record::{FieldValue, Record};
use crate::session::PluginSession;
use crate::sym::StringInterner;

const ALLA_ELEMENT_LEN: usize = 8;

pub struct NullInvalidQustAllaKeywordsFixup;

impl Fixup for NullInvalidQustAllaKeywordsFixup {
    fn name(&self) -> &'static str {
        "null_invalid_qust_alla_keywords"
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
        let mut report = FixupReport::empty();
        let target_schema = config
            .target_schema
            .as_deref()
            .ok_or_else(|| FixupError::Other("missing target schema in fixup config".into()))?;

        let qust_sig =
            SigCode::from_str("QUST").map_err(|e| FixupError::SchemaError(e.to_string()))?;
        let valid_keyword_ids =
            collect_valid_keyword_encoded_ids(session, mapper.interner, config, &mut report)?;
        // No KYWD anywhere (degenerate) → cannot classify; do nothing rather than
        // null every ALLA keyword blind.
        if valid_keyword_ids.is_empty() {
            return Ok(report);
        }

        let fks = session
            .form_keys_of_sig(qust_sig, mapper.interner)
            .map_err(|e| FixupError::HandleError(e.to_string()))?;
        let mut changed_records = Vec::new();
        for fk in fks {
            let mut record = match session.record_decoded(&fk, target_schema, mapper.interner) {
                Ok(r) => r,
                Err(_) => continue,
            };
            if null_invalid_alla_keywords(&mut record, &valid_keyword_ids) {
                changed_records.push(record);
            }
        }

        let expected = changed_records.len();
        if expected == 0 {
            return Ok(report);
        }
        let replaced = session
            .replace_records_contents(changed_records, target_schema, mapper.interner)
            .map_err(|e| FixupError::HandleError(e.to_string()))?;
        if replaced != expected {
            return Err(FixupError::HandleError(format!(
                "null_invalid_qust_alla_keywords replaced {replaced} of {expected} expected records"
            )));
        }
        report.records_changed = replaced.try_into().unwrap_or(u32::MAX);
        Ok(report)
    }
}

/// Build the set of every valid KYWD encoded FormID across the output plugin and
/// the target masters, in the same `(load_index << 24) | local` encoding the ALLA
/// keyword bytes use.
///
/// `pub(crate)`: shared with the store2 sweep visitor (Plan 4) so both drivers
/// gather through the identical code path.
pub(crate) fn collect_valid_keyword_encoded_ids(
    session: &mut PluginSession,
    interner: &StringInterner,
    config: &FixupConfig,
    report: &mut FixupReport,
) -> Result<FxHashSet<u32>, FixupError> {
    let kywd_sig = SigCode::from_str("KYWD").map_err(|e| FixupError::SchemaError(e.to_string()))?;
    let target_masters = session.target_masters().to_vec();
    let mut out = FxHashSet::default();

    let output_fks = session
        .form_keys_of_sig(kywd_sig, interner)
        .map_err(|e| FixupError::HandleError(e.to_string()))?;
    for fk in output_fks {
        if let Some(encoded) = encode_form_id(&fk, interner, &target_masters) {
            out.insert(encoded);
        }
    }

    for &handle_id in &config.target_master_handle_ids {
        let fks = match session.form_keys_of_sig_in_handle(handle_id, kywd_sig, interner) {
            Ok(fks) => fks,
            Err(e) => {
                let w = interner.intern(&format!("null_invalid_qust_alla_keywords_master:{e}"));
                report.warnings.push(w);
                continue;
            }
        };
        for fk in fks {
            if let Some(encoded) = encode_form_id(&fk, interner, &target_masters) {
                out.insert(encoded);
            }
        }
    }

    Ok(out)
}

/// Encode a FormKey to `(load_index << 24) | local`, where `load_index` is the
/// plugin's position in the target master list, or `masters.len()` (the output
/// plugin's own index) when not a listed master.
fn encode_form_id(fk: &FormKey, interner: &StringInterner, masters: &[String]) -> Option<u32> {
    if fk.local == 0 {
        return None;
    }
    let plugin_name = interner.resolve(fk.plugin)?;
    let load_index = masters
        .iter()
        .position(|m| m.eq_ignore_ascii_case(plugin_name))
        .unwrap_or(masters.len());
    if load_index > u8::MAX as usize || fk.local > 0x00FF_FFFF {
        return None;
    }
    Some(((load_index as u32) << 24) | fk.local)
}

/// Zero the keyword (first 4 bytes) of every ALLA element whose keyword is
/// non-zero and not a known KYWD. Returns `true` when at least one keyword was
/// nulled.
///
/// `pub(crate)`: the store2 sweep visitor (Plan 4) calls this same kernel.
pub(crate) fn null_invalid_alla_keywords(
    record: &mut Record,
    valid_keyword_ids: &FxHashSet<u32>,
) -> bool {
    let mut changed = false;
    for entry in record.fields.iter_mut() {
        if entry.sig.0 != *b"ALLA" {
            continue;
        }
        let FieldValue::Bytes(bytes) = &mut entry.value else {
            continue;
        };
        for element in bytes.chunks_exact_mut(ALLA_ELEMENT_LEN) {
            let keyword = u32::from_le_bytes([element[0], element[1], element[2], element[3]]);
            if keyword != 0 && !valid_keyword_ids.contains(&keyword) {
                element[0..4].copy_from_slice(&0u32.to_le_bytes());
                changed = true;
            }
        }
    }
    changed
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::SubrecordSig;
    use crate::record::{FieldEntry, RecordFlags};
    use smallvec::SmallVec;

    fn alla(rows: &[(u32, i32)]) -> FieldEntry {
        let mut bytes = Vec::new();
        for (keyword, alias_index) in rows {
            bytes.extend_from_slice(&keyword.to_le_bytes());
            bytes.extend_from_slice(&alias_index.to_le_bytes());
        }
        FieldEntry {
            sig: SubrecordSig::from_str("ALLA").unwrap(),
            value: FieldValue::Bytes(SmallVec::from_vec(bytes)),
        }
    }

    fn record(fields: Vec<FieldEntry>) -> Record {
        let interner = StringInterner::new();
        Record {
            sig: SigCode::from_str("QUST").unwrap(),
            form_key: FormKey {
                local: 0x000800,
                plugin: interner.intern("SeventySix.esm"),
            },
            eid: None,
            flags: RecordFlags::empty(),
            fields: fields.into_iter().collect(),
            warnings: SmallVec::new(),
        }
    }

    fn alla_rows(record: &Record) -> Vec<(u32, i32)> {
        let FieldValue::Bytes(bytes) = &record.fields[0].value else {
            panic!("ALLA must be bytes");
        };
        bytes
            .chunks_exact(ALLA_ELEMENT_LEN)
            .map(|c| {
                (
                    u32::from_le_bytes([c[0], c[1], c[2], c[3]]),
                    i32::from_le_bytes([c[4], c[5], c[6], c[7]]),
                )
            })
            .collect()
    }

    #[test]
    fn nulls_keyword_that_is_not_a_kywd() {
        // 0x0002FD66 (RELinkPatrol, dropped → now a Fallout4.esm REFR) is not in
        // the valid KYWD set → null its keyword, keep the alias index.
        let mut valid = FxHashSet::default();
        valid.insert(0x0000_404D); // a real KYWD (SQLinkHover)
        let mut rec = record(vec![alla(&[(0x0002_FD66, 1), (0x0000_404D, 2)])]);
        assert!(null_invalid_alla_keywords(&mut rec, &valid));
        assert_eq!(
            alla_rows(&rec),
            vec![(0, 1), (0x0000_404D, 2)],
            "bad keyword nulled, valid keyword + both alias indices preserved"
        );
    }

    #[test]
    fn keeps_null_and_valid_keywords_byte_identical() {
        let mut valid = FxHashSet::default();
        valid.insert(0x0000_404D);
        let mut rec = record(vec![alla(&[(0, 5), (0x0000_404D, 6)])]);
        assert!(!null_invalid_alla_keywords(&mut rec, &valid));
        assert_eq!(alla_rows(&rec), vec![(0, 5), (0x0000_404D, 6)]);
    }

    #[test]
    fn leaves_record_without_alla_untouched() {
        let valid = FxHashSet::default();
        let mut rec = record(vec![FieldEntry {
            sig: SubrecordSig::from_str("FULL").unwrap(),
            value: FieldValue::Bytes(SmallVec::from_slice(b"q\0")),
        }]);
        assert!(!null_invalid_alla_keywords(&mut rec, &valid));
    }
}
