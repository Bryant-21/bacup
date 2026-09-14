use std::collections::BTreeSet;

use crate::record::{FieldValue, Record};
use crate::sym::StringInterner;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LegacyEyeSourceEvidence {
    pub editor_id: String,
    pub display_name: String,
    pub diffuse_texture: String,
    pub raw_flags: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LegacyHairSourceEvidence {
    pub editor_id: String,
    pub display_name: String,
    pub preview_texture: String,
    pub model: String,
    pub model_information: Vec<u8>,
    pub raw_flags: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LegacyAppearanceSupport {
    pub reason_codes: Vec<String>,
}

pub(crate) fn classify_legacy_eye(record: &Record) -> LegacyAppearanceSupport {
    let mut reasons = BTreeSet::new();
    if record.sig.as_str() != "EYES" {
        reasons.insert("legacy_eye_wrong_signature".to_string());
    }

    let mut counts = [0usize; 4];
    for field in &record.fields {
        match field.sig.as_str() {
            "EDID" if matches!(field.value, FieldValue::String(_)) => counts[0] += 1,
            "FULL" if matches!(field.value, FieldValue::String(_)) => counts[1] += 1,
            "ICON" if matches!(field.value, FieldValue::String(_)) => counts[2] += 1,
            "DATA" if matches!(field.value, FieldValue::Uint(value) if value <= u8::MAX.into()) => {
                counts[3] += 1
            }
            _ => {
                reasons.insert("legacy_eye_subrecord_shape_unverified".to_string());
            }
        }
    }
    if counts != [1, 1, 1, 1] {
        reasons.insert("legacy_eye_field_multiplicity_unverified".to_string());
    }
    if reasons.is_empty() {
        reasons.insert("legacy_eye_requires_owner_sex_race_hdpt_txst_projection".to_string());
    }

    LegacyAppearanceSupport {
        reason_codes: reasons.into_iter().collect(),
    }
}

pub(crate) fn classify_legacy_hair(record: &Record) -> LegacyAppearanceSupport {
    let mut reasons = BTreeSet::new();
    if record.sig.as_str() != "HAIR" {
        reasons.insert("legacy_hair_wrong_signature".to_string());
    }

    let mut counts = [0usize; 6];
    for field in &record.fields {
        match field.sig.as_str() {
            "EDID" if matches!(field.value, FieldValue::String(_)) => counts[0] += 1,
            "FULL" if matches!(field.value, FieldValue::String(_)) => counts[1] += 1,
            "ICON" if matches!(field.value, FieldValue::String(_)) => counts[2] += 1,
            "MODL" if matches!(field.value, FieldValue::String(_)) => counts[3] += 1,
            "MODT" if matches!(field.value, FieldValue::Bytes(_)) => counts[4] += 1,
            "DATA" if matches!(field.value, FieldValue::Uint(value) if value <= u8::MAX.into()) => {
                counts[5] += 1
            }
            _ => {
                reasons.insert("legacy_hair_subrecord_shape_unverified".to_string());
            }
        }
    }
    if counts != [1, 1, 1, 1, 1, 1] {
        reasons.insert("legacy_hair_field_multiplicity_unverified".to_string());
    }
    if reasons.is_empty() {
        reasons.insert("legacy_hair_requires_owner_sex_race_hdpt_asset_projection".to_string());
    }

    LegacyAppearanceSupport {
        reason_codes: reasons.into_iter().collect(),
    }
}

pub(crate) fn decode_legacy_eye(
    record: &Record,
    interner: &StringInterner,
) -> Result<LegacyEyeSourceEvidence, LegacyAppearanceSupport> {
    let support = classify_legacy_eye(record);
    if support.reason_codes
        != ["legacy_eye_requires_owner_sex_race_hdpt_txst_projection".to_string()]
    {
        return Err(support);
    }
    Ok(LegacyEyeSourceEvidence {
        editor_id: single_string(record, "EDID", interner),
        display_name: single_string(record, "FULL", interner),
        diffuse_texture: single_string(record, "ICON", interner),
        raw_flags: single_uint(record, "DATA"),
    })
}

pub(crate) fn decode_legacy_hair(
    record: &Record,
    interner: &StringInterner,
) -> Result<LegacyHairSourceEvidence, LegacyAppearanceSupport> {
    let support = classify_legacy_hair(record);
    if support.reason_codes
        != ["legacy_hair_requires_owner_sex_race_hdpt_asset_projection".to_string()]
    {
        return Err(support);
    }
    Ok(LegacyHairSourceEvidence {
        editor_id: single_string(record, "EDID", interner),
        display_name: single_string(record, "FULL", interner),
        preview_texture: single_string(record, "ICON", interner),
        model: single_string(record, "MODL", interner),
        model_information: record
            .fields
            .iter()
            .find_map(|field| {
                (field.sig.as_str() == "MODT").then(|| match &field.value {
                    FieldValue::Bytes(bytes) => bytes.to_vec(),
                    _ => unreachable!("shape classifier accepted non-bytes MODT"),
                })
            })
            .expect("shape classifier requires one MODT"),
        raw_flags: single_uint(record, "DATA"),
    })
}

fn single_string(record: &Record, signature: &str, interner: &StringInterner) -> String {
    record
        .fields
        .iter()
        .find_map(|field| {
            (field.sig.as_str() == signature).then(|| match field.value {
                FieldValue::String(value) => interner
                    .resolve(value)
                    .expect("shape-classified string must resolve")
                    .to_string(),
                _ => unreachable!("shape classifier accepted non-string field"),
            })
        })
        .expect("shape classifier requires one string field")
}

fn single_uint(record: &Record, signature: &str) -> u32 {
    record
        .fields
        .iter()
        .find_map(|field| {
            (field.sig.as_str() == signature).then(|| match field.value {
                FieldValue::Uint(value) => u32::try_from(value)
                    .expect("shape classifier restricts DATA to a one-byte unsigned value"),
                _ => unreachable!("shape classifier accepted non-uint field"),
            })
        })
        .expect("shape classifier requires one uint field")
}

#[cfg(test)]
mod tests {
    use crate::ids::{FormKey, SigCode, SubrecordSig};
    use crate::record::FieldEntry;
    use crate::sym::StringInterner;

    use super::*;

    fn field(sig: [u8; 4], value: FieldValue) -> FieldEntry {
        FieldEntry {
            sig: SubrecordSig(sig),
            value,
        }
    }

    fn record(signature: &str, interner: &StringInterner) -> Record {
        Record::new(
            SigCode::from_str(signature).unwrap(),
            FormKey {
                local: 0x800,
                plugin: interner.intern("FalloutNV.esm"),
            },
        )
    }

    #[test]
    fn exact_legacy_eye_shape_requires_owner_aware_fo4_projection() {
        let interner = StringInterner::new();
        let mut record = record("EYES", &interner);
        record.fields.extend([
            field(*b"EDID", FieldValue::String(interner.intern("EyeBlue"))),
            field(*b"FULL", FieldValue::String(interner.intern("Blue"))),
            field(
                *b"ICON",
                FieldValue::String(interner.intern("Characters\\Eyes\\EyeBlue.dds")),
            ),
            field(*b"DATA", FieldValue::Uint(1)),
        ]);

        assert_eq!(
            classify_legacy_eye(&record).reason_codes,
            ["legacy_eye_requires_owner_sex_race_hdpt_txst_projection"]
        );
        assert_eq!(
            decode_legacy_eye(&record, &interner).unwrap(),
            LegacyEyeSourceEvidence {
                editor_id: "EyeBlue".to_string(),
                display_name: "Blue".to_string(),
                diffuse_texture: "Characters\\Eyes\\EyeBlue.dds".to_string(),
                raw_flags: 1,
            }
        );
    }

    #[test]
    fn exact_legacy_hair_shape_requires_owner_aware_fo4_projection() {
        let interner = StringInterner::new();
        let mut record = record("HAIR", &interner);
        record.fields.extend([
            field(*b"EDID", FieldValue::String(interner.intern("HairWavy"))),
            field(*b"FULL", FieldValue::String(interner.intern("Smooth Wave"))),
            field(
                *b"ICON",
                FieldValue::String(interner.intern("Characters\\Hair\\HairWavy.dds")),
            ),
            field(
                *b"MODL",
                FieldValue::String(interner.intern("Characters\\Hair\\HairWavy.nif")),
            ),
            field(*b"MODT", FieldValue::Bytes(vec![0; 96].into())),
            field(*b"DATA", FieldValue::Uint(5)),
        ]);

        assert_eq!(
            classify_legacy_hair(&record).reason_codes,
            ["legacy_hair_requires_owner_sex_race_hdpt_asset_projection"]
        );
        assert_eq!(
            decode_legacy_hair(&record, &interner).unwrap(),
            LegacyHairSourceEvidence {
                editor_id: "HairWavy".to_string(),
                display_name: "Smooth Wave".to_string(),
                preview_texture: "Characters\\Hair\\HairWavy.dds".to_string(),
                model: "Characters\\Hair\\HairWavy.nif".to_string(),
                model_information: vec![0; 96],
                raw_flags: 5,
            }
        );
    }

    #[test]
    fn appearance_shape_drift_is_not_hidden_by_projection_blockers() {
        let interner = StringInterner::new();
        let mut eye = record("EYES", &interner);
        eye.fields.push(field(
            *b"ICON",
            FieldValue::String(interner.intern("Characters\\Eyes\\EyeBlue.dds")),
        ));
        assert_eq!(
            classify_legacy_eye(&eye).reason_codes,
            ["legacy_eye_field_multiplicity_unverified"]
        );

        let mut hair = record("HAIR", &interner);
        hair.fields.push(field(*b"MODT", FieldValue::Uint(1)));
        assert_eq!(
            classify_legacy_hair(&hair).reason_codes,
            [
                "legacy_hair_field_multiplicity_unverified",
                "legacy_hair_subrecord_shape_unverified",
            ]
        );
    }
}
