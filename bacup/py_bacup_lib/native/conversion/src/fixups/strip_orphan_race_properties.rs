//! Fixup: strip PRPS (Properties) entries whose actor-value FormID is null.
//!
//! FO76 actor values with no FO4 equivalent leave RACE Properties rows whose
//! `PropertiesActorValue` is null (FormID 0) after the translation sweep, and xEdit
//! warns on them. Those rows are dropped from every RACE's PRPS.
//!
//! # Binary layout of PRPS (codec `array_struct:I,f`, 8 bytes per row)
//! | Offset | Size | Field                   |
//! |--------|------|-------------------------|
//! |      0 |    4 | PropertiesActorValue (formid) |
//! |      4 |    4 | PropertiesValue (float32)     |

use crate::fixups::{Fixup, FixupConfig, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::ids::{SigCode, SubrecordSig};
use crate::record::{FieldEntry, FieldValue, Record};
use crate::session::PluginSession;

// ---------------------------------------------------------------------------
// PRPS row constants
// ---------------------------------------------------------------------------

/// Size of one PRPS row in bytes (struct I,f = 4 + 4).
const PRPS_ROW_SIZE: usize = 8;

// ---------------------------------------------------------------------------
// Public fixup struct
// ---------------------------------------------------------------------------

pub struct StripOrphanRacePropertiesFixup;

impl Fixup for StripOrphanRacePropertiesFixup {
    fn name(&self) -> &'static str {
        "strip_orphan_race_properties"
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
        let race_sig =
            SigCode::from_str("RACE").map_err(|e| FixupError::SchemaError(e.to_string()))?;
        let target_schema = config
            .target_schema
            .as_deref()
            .ok_or_else(|| FixupError::Other("missing target schema in fixup config".into()))?;

        let mut report = FixupReport::empty();
        let mut changed_records = Vec::new();

        let race_fks = session
            .form_keys_of_sig(race_sig, mapper.interner)
            .map_err(|e| FixupError::HandleError(e.to_string()))?;

        if race_fks.is_empty() {
            return Ok(report);
        }

        for fk in &race_fks {
            let mut record = match session.record_decoded(fk, target_schema, mapper.interner) {
                Ok(r) => r,
                Err(e) => {
                    let w = mapper
                        .interner
                        .intern(&format!("strip_orphan_race_props_read_err:{e}"));
                    report.warnings.push(w);
                    continue;
                }
            };

            let stripped = apply_to_record(&mut record);
            if stripped > 0 {
                changed_records.push(record);
                report.records_changed += 1;
                report.records_dropped += stripped;
            }
        }

        session
            .replace_records(changed_records, target_schema, mapper.interner)
            .map_err(|e| FixupError::HandleError(e.to_string()))?;

        Ok(report)
    }
}

// ---------------------------------------------------------------------------
// Record-level mutation (extracted for unit-test access)
// ---------------------------------------------------------------------------

/// Strip PRPS rows whose actor-value FormID is zero; returns the number stripped.
/// PRPS decodes as `FieldValue::Bytes` (unknown-codec fallback). A payload that is
/// not a whole number of 8-byte rows is left untouched.
pub fn apply_to_record(record: &mut Record) -> u32 {
    let prps_sig = match SubrecordSig::from_str("PRPS") {
        Ok(s) => s,
        Err(_) => return 0,
    };

    let mut stripped: u32 = 0;
    let mut new_fields: smallvec::SmallVec<[FieldEntry; 8]> = smallvec::SmallVec::new();

    for entry in record.fields.drain(..) {
        if entry.sig != prps_sig {
            new_fields.push(entry);
            continue;
        }

        // Only touch PRPS fields stored as raw bytes.
        match entry.value {
            FieldValue::Bytes(ref data) => {
                let data = data.clone();

                // If the payload is not a multiple of PRPS_ROW_SIZE, keep it intact.
                if data.len() % PRPS_ROW_SIZE != 0 {
                    new_fields.push(entry);
                    continue;
                }

                // Filter out rows with a null FormID.
                let mut kept_data: smallvec::SmallVec<[u8; 32]> = smallvec::SmallVec::new();
                for chunk in data.chunks_exact(PRPS_ROW_SIZE) {
                    let form_id = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
                    if form_id == 0 {
                        stripped += 1;
                    } else {
                        kept_data.extend_from_slice(chunk);
                    }
                }

                if stripped > 0 || kept_data.len() != data.len() {
                    // Emit the filtered PRPS — even if empty, keep the subrecord
                    // so downstream consumers don't see a missing PRPS.
                    new_fields.push(FieldEntry {
                        sig: prps_sig,
                        value: FieldValue::Bytes(kept_data),
                    });
                } else {
                    new_fields.push(entry);
                }
            }
            // Non-bytes PRPS (typed decode) — keep unchanged.
            _ => {
                new_fields.push(entry);
            }
        }
    }

    record.fields = new_fields;
    stripped
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

    fn make_race(
        local: u32,
        plugin: &str,
        prps_rows: &[(u32, f32)],
        interner: &StringInterner,
    ) -> Record {
        let sig = SigCode::from_str("RACE").unwrap();
        let fk = FormKey {
            local,
            plugin: interner.intern(plugin),
        };

        let mut fields: smallvec::SmallVec<[FieldEntry; 8]> = smallvec::SmallVec::new();

        if !prps_rows.is_empty() {
            let prps_sig = SubrecordSig::from_str("PRPS").unwrap();
            let mut data: smallvec::SmallVec<[u8; 32]> = smallvec::SmallVec::new();
            for &(form_id, float_val) in prps_rows {
                data.extend_from_slice(&form_id.to_le_bytes());
                data.extend_from_slice(&float_val.to_le_bytes());
            }
            fields.push(FieldEntry {
                sig: prps_sig,
                value: FieldValue::Bytes(data),
            });
        }

        Record {
            sig,
            form_key: fk,
            eid: None,
            flags: RecordFlags::empty(),
            fields,
            warnings: smallvec::SmallVec::new(),
        }
    }

    #[test]
    fn non_prps_fields_and_malformed_prps_are_preserved() {
        let mut interner = StringInterner::new();
        let edid_sym = interner.intern("HumanRace");

        let mut record = make_race(0x000800, "Output.esp", &[], &mut interner);
        // Add an EDID field before PRPS (should survive).
        let edid_sig = SubrecordSig::from_str("EDID").unwrap();
        record.fields.push(FieldEntry {
            sig: edid_sig,
            value: FieldValue::String(edid_sym),
        });
        // Add an orphan PRPS entry.
        let prps_sig = SubrecordSig::from_str("PRPS").unwrap();
        let mut prps_data: smallvec::SmallVec<[u8; 32]> = smallvec::SmallVec::new();
        prps_data.extend_from_slice(&0u32.to_le_bytes()); // null formid
        prps_data.extend_from_slice(&1.0f32.to_le_bytes());
        record.fields.push(FieldEntry {
            sig: prps_sig,
            value: FieldValue::Bytes(prps_data),
        });

        let stripped = apply_to_record(&mut record);
        assert_eq!(stripped, 1);

        // EDID should still be first; PRPS second (now empty).
        assert_eq!(record.fields.len(), 2);
        assert_eq!(record.fields[0].sig.as_str(), "EDID");
        assert_eq!(record.fields[1].sig.as_str(), "PRPS");

        let interner = StringInterner::new();
        let prps_sig = SubrecordSig::from_str("PRPS").unwrap();
        let sig = SigCode::from_str("RACE").unwrap();
        let fk = FormKey {
            local: 0x000800,
            plugin: interner.intern("Output.esp"),
        };

        let mut bad_data: smallvec::SmallVec<[u8; 32]> = smallvec::SmallVec::new();
        // 5 bytes — not a multiple of 8.
        bad_data.extend_from_slice(&[0u8, 0, 0, 0, 0]);

        let mut record = Record {
            sig,
            form_key: fk,
            eid: None,
            flags: RecordFlags::empty(),
            fields: smallvec::smallvec![FieldEntry {
                sig: prps_sig,
                value: FieldValue::Bytes(bad_data),
            }],
            warnings: smallvec::SmallVec::new(),
        };

        let stripped = apply_to_record(&mut record);
        assert_eq!(stripped, 0, "malformed PRPS must not be stripped");
        assert_eq!(record.fields.len(), 1);
        if let FieldValue::Bytes(data) = &record.fields[0].value {
            assert_eq!(data.len(), 5, "malformed payload must be preserved");
        } else {
            panic!("expected Bytes");
        }
    }

    #[test]
    fn null_formid_rows_are_stripped_in_order() {
        let interner = StringInterner::new();
        let encoded = |rows: &[(u32, f32)]| {
            FieldValue::Bytes(
                rows.iter()
                    .flat_map(|&(form_id, value)| {
                        form_id.to_le_bytes().into_iter().chain(value.to_le_bytes())
                    })
                    .collect(),
            )
        };

        let mut record = make_race(0x000800, "Output.esp", &[], &interner);
        assert_eq!(apply_to_record(&mut record), 0);
        assert!(record.fields.is_empty());

        for (name, rows, kept) in [
            (
                "all valid",
                vec![(0x1234, 1.0), (0x5678, 2.0)],
                vec![(0x1234, 1.0), (0x5678, 2.0)],
            ),
            (
                "one null",
                vec![(0x1234, 1.0), (0, 0.5)],
                vec![(0x1234, 1.0)],
            ),
            ("all null", vec![(0, 1.0), (0, 2.0)], vec![]),
            (
                "mixed",
                vec![(0x1111, 1.0), (0, 0.0), (0x2222, 3.0), (0, 0.0)],
                vec![(0x1111, 1.0), (0x2222, 3.0)],
            ),
        ] {
            let mut record = make_race(0x000800, "Output.esp", &rows, &interner);
            let stripped = apply_to_record(&mut record);
            assert_eq!(stripped as usize, rows.len() - kept.len(), "{name}");
            assert_eq!(record.fields.len(), 1, "{name}");
            assert_eq!(record.fields[0].value, encoded(&kept), "{name}");
        }
    }
}
