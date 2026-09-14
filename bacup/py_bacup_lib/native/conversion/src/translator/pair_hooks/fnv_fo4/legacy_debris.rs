use std::collections::BTreeSet;

use crate::record::{FieldValue, Record};
use crate::sym::StringInterner;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LegacyDebrisModelRow {
    pub percentage: u8,
    pub source_model_filename: String,
    pub target_model_filename: String,
    pub has_collision: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LegacyDebrisSupport {
    pub reason_codes: Vec<String>,
}

impl LegacyDebrisSupport {
    pub(crate) fn is_ready(&self) -> bool {
        self.reason_codes.is_empty()
    }
}

pub(crate) fn classify_legacy_debris(record: &Record) -> LegacyDebrisSupport {
    let mut reasons = BTreeSet::new();
    if record.sig.as_str() != "DEBR" {
        reasons.insert("legacy_debris_wrong_signature".to_string());
    }

    let mut editor_ids = 0usize;
    let mut data_rows = 0usize;
    let mut model_info_rows = 0usize;
    for field in &record.fields {
        match field.sig.as_str() {
            "EDID" if matches!(field.value, FieldValue::String(_)) => editor_ids += 1,
            "DATA" => {
                data_rows += 1;
                match &field.value {
                    FieldValue::Bytes(bytes) if decode_legacy_debris_data(bytes).is_some() => {}
                    _ => {
                        reasons.insert("legacy_debris_data_shape_unverified".to_string());
                    }
                }
            }
            "MODT" if matches!(field.value, FieldValue::Bytes(_)) => model_info_rows += 1,
            "EDID" | "MODT" => {
                reasons.insert("legacy_debris_subrecord_shape_unverified".to_string());
            }
            _ => {
                reasons.insert("legacy_debris_subrecord_unverified".to_string());
            }
        }
    }

    if editor_ids != 1 || data_rows == 0 || model_info_rows > data_rows {
        reasons.insert("legacy_debris_row_multiplicity_unverified".to_string());
    }

    LegacyDebrisSupport {
        reason_codes: reasons.into_iter().collect(),
    }
}

pub(crate) fn lower_legacy_debris(
    record: &mut Record,
    source_game: &str,
    interner: &StringInterner,
) -> Result<bool, String> {
    if record.sig.as_str() != "DEBR" {
        return Ok(false);
    }
    let namespace = legacy_debris_runtime_namespace(source_game, record, interner)?;
    for field in &mut record.fields {
        if field.sig.as_str() != "DATA" {
            continue;
        }
        let FieldValue::Bytes(bytes) = &field.value else {
            continue;
        };
        let Some((percentage, source_model_filename, has_collision)) =
            decode_legacy_debris_data(bytes)
        else {
            continue;
        };
        let target_model_filename =
            legacy_debris_target_model_filename(&namespace, source_model_filename);
        field.value = FieldValue::Bytes(
            [percentage]
                .into_iter()
                .chain(target_model_filename.bytes())
                .chain([0, u8::from(has_collision)])
                .collect::<Vec<_>>()
                .into(),
        );
    }
    record.fields.retain(|field| field.sig.as_str() != "MODT");
    Ok(true)
}

pub(crate) fn legacy_debris_model_rows(
    record: &Record,
    source_game: &str,
    interner: &StringInterner,
) -> Result<Vec<LegacyDebrisModelRow>, String> {
    if record.sig.as_str() != "DEBR" {
        return Ok(Vec::new());
    }
    let namespace = legacy_debris_runtime_namespace(source_game, record, interner)?;
    Ok(record
        .fields
        .iter()
        .filter(|field| field.sig.as_str() == "DATA")
        .filter_map(|field| match &field.value {
            FieldValue::Bytes(bytes) => decode_legacy_debris_data(bytes).map(
                |(percentage, source_model_filename, has_collision)| LegacyDebrisModelRow {
                    percentage,
                    source_model_filename: source_model_filename.to_string(),
                    target_model_filename: legacy_debris_target_model_filename(
                        &namespace,
                        source_model_filename,
                    ),
                    has_collision,
                },
            ),
            _ => None,
        })
        .collect())
}

fn legacy_debris_runtime_namespace(
    source_game: &str,
    record: &Record,
    interner: &StringInterner,
) -> Result<String, String> {
    let source_game = source_game.trim().to_ascii_lowercase();
    if !matches!(source_game.as_str(), "fnv" | "fo3") {
        return Err(format!(
            "unsupported legacy debris source game {source_game:?}"
        ));
    }
    let plugin = interner
        .resolve(record.form_key.plugin)
        .ok_or_else(|| "legacy debris source plugin is unresolved".to_string())?;
    let identity = format!(
        "{source_game}|{}|{:06x}",
        plugin.to_ascii_lowercase(),
        record.form_key.local
    );
    Ok(format!(
        "b21_fnvfo3_debr_{}",
        &blake3::hash(identity.as_bytes()).to_hex().as_str()[..16]
    ))
}

fn legacy_debris_target_model_filename(namespace: &str, source_model_filename: &str) -> String {
    format!(
        "{namespace}\\{}",
        source_model_filename.trim().replace('/', "\\")
    )
}

fn decode_legacy_debris_data(bytes: &[u8]) -> Option<(u8, &str, bool)> {
    if bytes.len() < 3 || bytes[0] > 100 {
        return None;
    }
    let terminator = bytes[1..].iter().position(|byte| *byte == 0)? + 1;
    if terminator + 2 != bytes.len() || !matches!(bytes[terminator + 1], 0 | 1) {
        return None;
    }
    let path = std::str::from_utf8(&bytes[1..terminator]).ok()?;
    let normalized = path.replace('/', "\\");
    if path.is_empty()
        || path.contains(':')
        || normalized.starts_with('\\')
        || !normalized.to_ascii_lowercase().ends_with(".nif")
    {
        return None;
    }
    Some((bytes[0], path, bytes[terminator + 1] != 0))
}

#[cfg(test)]
mod tests {
    use crate::ids::{FormKey, SigCode, SubrecordSig};
    use crate::record::FieldEntry;
    use crate::sym::StringInterner;

    use super::*;

    fn source_record(interner: &StringInterner) -> Record {
        let mut record = Record::new(
            SigCode::from_str("DEBR").unwrap(),
            FormKey {
                local: 0x800,
                plugin: interner.intern("FalloutNV.esm"),
            },
        );
        record.fields.extend([
            FieldEntry {
                sig: SubrecordSig(*b"EDID"),
                value: FieldValue::String(interner.intern("GoreBits")),
            },
            FieldEntry {
                sig: SubrecordSig(*b"DATA"),
                value: FieldValue::Bytes(
                    [25_u8]
                        .into_iter()
                        .chain(b"Gore\\MeatBit01.nif\0".iter().copied())
                        .chain([1])
                        .collect::<Vec<_>>()
                        .into(),
                ),
            },
            FieldEntry {
                sig: SubrecordSig(*b"MODT"),
                value: FieldValue::Bytes(vec![1, 2, 3].into()),
            },
        ]);
        record
    }

    #[test]
    fn accepts_only_exact_legacy_rows_and_exposes_embedded_model_paths() {
        let interner = StringInterner::new();
        let record = source_record(&interner);
        assert!(classify_legacy_debris(&record).is_ready());
        assert_eq!(
            legacy_debris_model_rows(&record, "fnv", &interner)
                .unwrap()
                .into_iter()
                .map(|row| row.source_model_filename)
                .collect::<Vec<_>>(),
            ["Gore\\MeatBit01.nif"]
        );

        let mut invalid = record.clone();
        let FieldValue::Bytes(bytes) = &mut invalid
            .fields
            .iter_mut()
            .find(|field| field.sig.as_str() == "DATA")
            .unwrap()
            .value
        else {
            panic!("fixture DATA must be bytes")
        };
        *bytes.last_mut().unwrap() = 2;
        assert_eq!(
            classify_legacy_debris(&invalid).reason_codes,
            ["legacy_debris_data_shape_unverified"]
        );
    }

    #[test]
    fn drops_only_source_model_info_for_target_regeneration() {
        let interner = StringInterner::new();
        let mut record = source_record(&interner);
        let expected_target = legacy_debris_model_rows(&record, "fnv", &interner).unwrap()[0]
            .target_model_filename
            .clone();
        assert!(lower_legacy_debris(&mut record, "fnv", &interner).unwrap());
        assert_eq!(
            record
                .fields
                .iter()
                .map(|field| field.sig.as_str())
                .collect::<Vec<_>>(),
            ["EDID", "DATA"]
        );
        let FieldValue::Bytes(bytes) = &record.fields[1].value else {
            panic!("lowered DATA must remain bytes")
        };
        assert_eq!(
            decode_legacy_debris_data(bytes),
            Some((25, expected_target.as_str(), true))
        );
    }

    #[test]
    fn model_texture_metadata_is_optional_per_debris_row() {
        let interner = StringInterner::new();
        let mut record = source_record(&interner);
        record.fields.push(FieldEntry {
            sig: SubrecordSig(*b"DATA"),
            value: FieldValue::Bytes(
                [75_u8]
                    .into_iter()
                    .chain(b"Gore\\MeatBit02.nif\0".iter().copied())
                    .chain([0])
                    .collect::<Vec<_>>()
                    .into(),
            ),
        });

        assert!(classify_legacy_debris(&record).is_ready());
        assert_eq!(
            legacy_debris_model_rows(&record, "fnv", &interner)
                .unwrap()
                .len(),
            2
        );
    }
}
