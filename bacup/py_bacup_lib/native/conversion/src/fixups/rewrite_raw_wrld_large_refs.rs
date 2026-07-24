//! Fixup: rewrite raw WRLD large-reference (RNAM) table FormIDs.
//!
//! Each RNAM subrecord is a grid header (i16 gridY + i16 gridX + u32 count)
//! followed by rows of (u32 ref FormID, i16 cellY, i16 cellX). A `00xxxxxx`
//! source-local FormID left after FO4 masters are added resolves against
//! `Fallout4.esm`, so it is remapped here. Defensive: the translator normally
//! strips WRLD runtime tables (RNAM/OFST/CLSZ) — the shipped RNAM is produced by
//! `esp_authoring_core::worldspace_header`'s carry — so this only fires on a
//! WRLD.RNAM table that survives translation.

use crate::fixups::{Fixup, FixupConfig, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::ids::{FormKey, SigCode};
use crate::record::{FieldValue, Record};
use crate::session::PluginSession;
use crate::sym::Sym;
use rustc_hash::FxHashMap;

pub struct RewriteRawWrldLargeRefsFixup;

impl Fixup for RewriteRawWrldLargeRefsFixup {
    fn name(&self) -> &'static str {
        "rewrite_raw_wrld_large_refs"
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
        repair_wrld_large_refs(session, mapper, config)
    }
}

pub(crate) fn repair_wrld_large_refs(
    session: &mut PluginSession,
    mapper: &mut FormKeyMapper,
    config: &FixupConfig,
) -> Result<FixupReport, FixupError> {
    let schema_holder;
    let target_schema = if let Some(schema) = config.target_schema.as_deref() {
        schema
    } else {
        schema_holder = session
            .schema()
            .map_err(|e| FixupError::HandleError(e.to_string()))?;
        schema_holder.as_ref()
    };
    let wrld_sig = SigCode::from_str("WRLD").map_err(|e| FixupError::Other(e.to_string()))?;
    let wrld_fks = session
        .form_keys_of_sig(wrld_sig, mapper.interner)
        .map_err(|e| FixupError::HandleError(e.to_string()))?;

    let mut report = FixupReport::empty();
    for fk in wrld_fks {
        let mut record = match session.record_decoded(&fk, target_schema, mapper.interner) {
            Ok(record) => record,
            Err(e) => {
                let w = mapper
                    .interner
                    .intern(&format!("rewrite_raw_wrld_large_refs_read_err:{e}"));
                report.warnings.push(w);
                continue;
            }
        };

        if rewrite_wrld_large_ref_record(&mut record, mapper, &mut report.warnings) {
            session
                .replace_record_contents(record, target_schema, mapper.interner)
                .map_err(|e| FixupError::HandleError(e.to_string()))?;
            report.records_changed += 1;
        }
    }

    Ok(report)
}

fn rewrite_wrld_large_ref_record(
    record: &mut Record,
    mapper: &mut FormKeyMapper,
    warnings: &mut Vec<Sym>,
) -> bool {
    if record.sig.as_str() != "WRLD" {
        return false;
    }

    let target_by_source_local: FxHashMap<u32, FormKey> = mapper
        .source_to_target_iter()
        .map(|(source, target)| (source.local, target))
        .collect();

    let mut changed = false;
    for entry in record
        .fields
        .iter_mut()
        .filter(|entry| entry.sig.as_str() == "RNAM")
    {
        match &mut entry.value {
            FieldValue::Bytes(bytes) => {
                changed |= rewrite_rnam_bytes(bytes.as_mut_slice(), mapper, warnings);
            }
            FieldValue::List(_) | FieldValue::Struct(_) => {
                changed |= rewrite_decoded_rnam_refs(&mut entry.value, &target_by_source_local) > 0;
            }
            _ => {}
        }
    }
    changed
}

fn rewrite_rnam_bytes(
    bytes: &mut [u8],
    mapper: &mut FormKeyMapper,
    warnings: &mut Vec<Sym>,
) -> bool {
    // RNAM layout: i16 gridY + i16 gridX + u32 row count, then count rows of
    // (u32 ref formid, i16 cellY, i16 cellX). The grid header must be skipped —
    // treating it as a row would rewrite the coords/count as a formid.
    const RNAM_HEADER_SIZE: usize = 8;
    const RNAM_ROW_SIZE: usize = 8;
    let count = (bytes.len() >= RNAM_HEADER_SIZE)
        .then(|| u32::from_le_bytes(bytes[4..8].try_into().expect("four-byte count")) as usize);
    if count.is_none_or(|c| bytes.len() != RNAM_HEADER_SIZE + c * RNAM_ROW_SIZE) {
        let w = mapper.interner.intern(&format!(
            "rewrite_raw_wrld_large_refs_bad_rnam_len:{}",
            bytes.len()
        ));
        warnings.push(w);
        return false;
    }

    let mut changed = false;
    for row_start in (RNAM_HEADER_SIZE..bytes.len()).step_by(RNAM_ROW_SIZE) {
        let raw = u32::from_le_bytes(
            bytes[row_start..row_start + 4]
                .try_into()
                .expect("row has four-byte formid"),
        );
        if raw == 0 || raw >> 24 != 0 {
            continue;
        }
        match mapper.rewrite_raw_formid_at(bytes, row_start) {
            Some(row_changed) => changed |= row_changed,
            None => {
                let w = mapper
                    .interner
                    .intern(&format!("rewrite_raw_wrld_large_refs_unmapped:{raw:08X}"));
                warnings.push(w);
            }
        }
    }
    changed
}

fn rewrite_decoded_rnam_refs(
    value: &mut FieldValue,
    target_by_source_local: &FxHashMap<u32, FormKey>,
) -> u32 {
    match value {
        FieldValue::FormKey(fk) => {
            if fk.local == 0 {
                return 0;
            }
            let Some(target) = target_by_source_local.get(&fk.local) else {
                return 0;
            };
            if *fk == *target {
                return 0;
            }
            *fk = *target;
            1
        }
        FieldValue::List(items) => items
            .iter_mut()
            .map(|item| rewrite_decoded_rnam_refs(item, target_by_source_local))
            .sum(),
        FieldValue::Struct(fields) => fields
            .iter_mut()
            .map(|(_, child)| rewrite_decoded_rnam_refs(child, target_by_source_local))
            .sum(),
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formkey_mapper::MapperOptions;
    use crate::ids::{FormKey, SubrecordSig};
    use crate::record::FieldEntry;
    use crate::sym::StringInterner;
    use smallvec::SmallVec;

    fn fo76_to_fo4_mapper(interner: &mut StringInterner) -> FormKeyMapper<'_> {
        FormKeyMapper::new(
            [],
            MapperOptions {
                output_plugin_name: "SeventySix.esm".to_string(),
                source_plugin_name: "SeventySix.esm".to_string(),
                target_master_names: vec![
                    "Fallout4.esm".to_string(),
                    "DLCRobot.esm".to_string(),
                    "DLCworkshop01.esm".to_string(),
                    "DLCCoast.esm".to_string(),
                    "DLCworkshop02.esm".to_string(),
                    "DLCworkshop03.esm".to_string(),
                    "DLCNukaWorld.esm".to_string(),
                ],
                preserve_source_ids: true,
                ..Default::default()
            },
            interner,
        )
    }

    #[test]
    fn rewrites_wrld_rnam_source_local_refs_to_output_plugin_load_order() {
        let mut interner = StringInterner::new();
        let mut mapper = fo76_to_fo4_mapper(&mut interner);
        let source_ref = FormKey::parse("026616@SeventySix.esm", mapper.interner).unwrap();
        let target_ref = FormKey::parse("026616@SeventySix.esm", mapper.interner).unwrap();
        mapper.add_mapping(source_ref, target_ref);

        let mut record = Record::new(
            SigCode::from_str("WRLD").unwrap(),
            FormKey::parse("00DC6C@SeventySix.esm", mapper.interner).unwrap(),
        );
        let mut raw = Vec::new();
        raw.extend_from_slice(&16_i16.to_le_bytes());
        raw.extend_from_slice(&42_i16.to_le_bytes());
        raw.extend_from_slice(&1_u32.to_le_bytes());
        raw.extend_from_slice(&0x0002_6616_u32.to_le_bytes());
        raw.extend_from_slice(&16_i16.to_le_bytes());
        raw.extend_from_slice(&42_i16.to_le_bytes());
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("RNAM").unwrap(),
            value: FieldValue::Bytes(SmallVec::from_vec(raw)),
        });

        let mut warnings = Vec::new();
        assert!(rewrite_wrld_large_ref_record(
            &mut record,
            &mut mapper,
            &mut warnings
        ));
        assert!(warnings.is_empty());

        let FieldValue::Bytes(bytes) = &record.fields[0].value else {
            panic!("expected raw RNAM bytes");
        };
        // Grid header untouched.
        assert_eq!(i16::from_le_bytes(bytes[0..2].try_into().unwrap()), 16);
        assert_eq!(i16::from_le_bytes(bytes[2..4].try_into().unwrap()), 42);
        assert_eq!(u32::from_le_bytes(bytes[4..8].try_into().unwrap()), 1);
        // Row formid remapped, cell coords untouched.
        assert_eq!(
            u32::from_le_bytes(bytes[8..12].try_into().unwrap()),
            0x0702_6616
        );
        assert_eq!(i16::from_le_bytes(bytes[12..14].try_into().unwrap()), 16);
        assert_eq!(i16::from_le_bytes(bytes[14..16].try_into().unwrap()), 42);
    }

    #[test]
    fn warns_on_rnam_bytes_shorter_than_declared_row_count() {
        let mut interner = StringInterner::new();
        let mut mapper = fo76_to_fo4_mapper(&mut interner);
        let mut raw = Vec::new();
        raw.extend_from_slice(&0_i16.to_le_bytes());
        raw.extend_from_slice(&0_i16.to_le_bytes());
        raw.extend_from_slice(&2_u32.to_le_bytes()); // claims 2 rows, holds 1
        raw.extend_from_slice(&0x0002_6616_u32.to_le_bytes());
        raw.extend_from_slice(&0_i16.to_le_bytes());
        raw.extend_from_slice(&0_i16.to_le_bytes());
        let before = raw.clone();

        let mut warnings = Vec::new();
        assert!(!rewrite_rnam_bytes(
            raw.as_mut_slice(),
            &mut mapper,
            &mut warnings
        ));
        assert_eq!(warnings.len(), 1);
        assert_eq!(raw, before);
    }

    #[test]
    fn leaves_already_target_encoded_wrld_rnam_refs_unchanged() {
        let mut interner = StringInterner::new();
        let mut mapper = fo76_to_fo4_mapper(&mut interner);
        let mut raw = Vec::new();
        raw.extend_from_slice(&16_i16.to_le_bytes());
        raw.extend_from_slice(&42_i16.to_le_bytes());
        raw.extend_from_slice(&1_u32.to_le_bytes());
        raw.extend_from_slice(&0x0702_6616_u32.to_le_bytes());
        raw.extend_from_slice(&16_i16.to_le_bytes());
        raw.extend_from_slice(&42_i16.to_le_bytes());

        let mut warnings = Vec::new();
        assert!(!rewrite_rnam_bytes(
            raw.as_mut_slice(),
            &mut mapper,
            &mut warnings
        ));
        assert!(warnings.is_empty());
        assert_eq!(
            u32::from_le_bytes(raw[8..12].try_into().unwrap()),
            0x0702_6616
        );
    }

    #[test]
    fn rewrites_decoded_wrld_rnam_refs_to_output_plugin() {
        let mut interner = StringInterner::new();
        let mut mapper = fo76_to_fo4_mapper(&mut interner);
        let source_ref = FormKey::parse("026616@SeventySix.esm", mapper.interner).unwrap();
        let target_ref = FormKey::parse("026616@SeventySix.esm", mapper.interner).unwrap();
        let stale_ref = FormKey::parse("026616@Fallout4.esm", mapper.interner).unwrap();
        mapper.add_mapping(source_ref, target_ref);

        let cell_key = mapper.interner.intern("Cell");
        let references_key = mapper.interner.intern("References");
        let ref_key = mapper.interner.intern("Ref");
        let mut record = Record::new(
            SigCode::from_str("WRLD").unwrap(),
            FormKey::parse("00DC6C@SeventySix.esm", mapper.interner).unwrap(),
        );
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("RNAM").unwrap(),
            value: FieldValue::Struct(vec![(
                cell_key,
                FieldValue::Struct(vec![(
                    references_key,
                    FieldValue::List(vec![FieldValue::Struct(vec![(
                        ref_key,
                        FieldValue::FormKey(stale_ref),
                    )])]),
                )]),
            )]),
        });

        let mut warnings = Vec::new();
        assert!(rewrite_wrld_large_ref_record(
            &mut record,
            &mut mapper,
            &mut warnings
        ));
        assert!(warnings.is_empty());

        let FieldValue::Struct(cell_fields) = &record.fields[0].value else {
            panic!("expected decoded RNAM struct");
        };
        let FieldValue::Struct(reference_fields) = &cell_fields[0].1 else {
            panic!("expected Cell struct");
        };
        let FieldValue::List(rows) = &reference_fields[0].1 else {
            panic!("expected References list");
        };
        let FieldValue::Struct(row_fields) = &rows[0] else {
            panic!("expected reference row");
        };
        assert_eq!(row_fields[0].1, FieldValue::FormKey(target_ref));
    }
}
