use std::collections::BTreeSet;

use crate::record::{FieldEntry, FieldValue, Record};
use crate::sym::StringInterner;
use smallvec::SmallVec;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LegacyImpactSupport {
    pub reason_codes: Vec<String>,
}

impl LegacyImpactSupport {
    pub(crate) fn is_ready(&self) -> bool {
        self.reason_codes.is_empty()
    }
}

pub(crate) fn legacy_impact_fallback_warnings(
    record: &Record,
    interner: &StringInterner,
) -> Vec<String> {
    if !classify_legacy_impact(record, interner).is_ready() {
        return Vec::new();
    }
    legacy_decal_fallback_warnings(record, interner, "legacy_impact")
}

pub(crate) fn legacy_texture_set_fallback_warnings(
    record: &Record,
    interner: &StringInterner,
) -> Vec<String> {
    if !classify_legacy_texture_set(record, interner).is_ready() {
        return Vec::new();
    }
    legacy_decal_fallback_warnings(record, interner, "legacy_texture_set")
}

pub(crate) fn classify_legacy_impact(
    record: &Record,
    interner: &StringInterner,
) -> LegacyImpactSupport {
    let mut reasons = BTreeSet::new();
    if record.sig.as_str() != "IPCT" {
        reasons.insert("legacy_impact_wrong_signature".to_string());
    }

    let mut data_count = 0usize;
    let mut decal_count = 0usize;
    for field in &record.fields {
        match field.sig.as_str() {
            "EDID" | "MODL" if matches!(field.value, FieldValue::String(_)) => {}
            "MODB" if scalar_f32(&field.value).is_some() => {}
            "MODT" if matches!(field.value, FieldValue::Bytes(_)) => {}
            "MODD" if scalar_u8(&field.value) == Some(0) => {}
            "DATA" => {
                data_count += 1;
                if let Err(reason) = legacy_impact_data_bytes(&field.value, interner) {
                    reasons.insert(reason);
                }
            }
            "DODT" => {
                decal_count += 1;
                if let Err(reason) = legacy_decal_data_bytes(&field.value, interner) {
                    reasons.insert(reason);
                }
            }
            "DNAM" | "SNAM" | "NAM1" if scalar_form_id_or_null(&field.value) => {}
            "MODS" => {
                reasons.insert("legacy_impact_alternate_texture_layout_unverified".to_string());
            }
            "MODD" => {
                reasons.insert("legacy_impact_model_flags_not_fo4_equivalent".to_string());
            }
            signature => {
                reasons.insert(format!(
                    "legacy_impact_{}_shape_unverified",
                    signature.to_ascii_lowercase()
                ));
            }
        }
    }
    if data_count != 1 {
        reasons.insert("legacy_impact_data_multiplicity_unverified".to_string());
    }
    if decal_count > 1 {
        reasons.insert("legacy_impact_decal_multiplicity_unverified".to_string());
    }
    LegacyImpactSupport {
        reason_codes: reasons.into_iter().collect(),
    }
}

pub(crate) fn lower_legacy_impact(record: &mut Record, interner: &StringInterner) -> bool {
    if !classify_legacy_impact(record, interner).is_ready() {
        return false;
    }

    let mut fields = SmallVec::new();
    for field in &record.fields {
        let replacement = match field.sig.as_str() {
            "MODB" | "MODT" | "MODD" => continue,
            "DATA" => legacy_impact_data_bytes(&field.value, interner)
                .ok()
                .map(|bytes| bytes.to_vec()),
            "DODT" => legacy_decal_data_bytes(&field.value, interner)
                .ok()
                .map(|bytes| bytes.to_vec()),
            _ => None,
        };
        fields.push(FieldEntry {
            sig: field.sig,
            value: replacement
                .map(|bytes| FieldValue::Bytes(SmallVec::from_vec(bytes)))
                .unwrap_or_else(|| field.value.clone()),
        });
    }
    record.fields = fields;
    true
}

pub(crate) fn classify_legacy_texture_set(
    record: &Record,
    interner: &StringInterner,
) -> LegacyImpactSupport {
    let mut reasons = BTreeSet::new();
    if record.sig.as_str() != "TXST" {
        reasons.insert("legacy_texture_set_wrong_signature".to_string());
    }
    let mut decal_count = 0usize;
    let mut bounds_count = 0usize;
    for field in &record.fields {
        match field.sig.as_str() {
            "EDID" | "TX00" | "TX01" | "TX02" | "TX03" | "TX04" | "TX05"
                if matches!(field.value, FieldValue::String(_)) => {}
            "OBND"
                if matches!(&field.value, FieldValue::Bytes(bytes) if bytes.len() == 12)
                    || matches!(field.value, FieldValue::Struct(_)) =>
            {
                bounds_count += 1;
            }
            "DODT" => {
                decal_count += 1;
                if let Err(reason) = legacy_decal_data_bytes(&field.value, interner) {
                    reasons.insert(reason.replace("legacy_decal", "legacy_texture_set_decal"));
                }
            }
            "DNAM" if scalar_u16(&field.value).is_some_and(|flags| flags <= 1) => {}
            signature => {
                reasons.insert(format!(
                    "legacy_texture_set_{}_shape_unverified",
                    signature.to_ascii_lowercase()
                ));
            }
        }
    }
    if decal_count > 1 {
        reasons.insert("legacy_texture_set_decal_multiplicity_unverified".to_string());
    }
    if bounds_count != 1 {
        reasons.insert("legacy_texture_set_bounds_multiplicity_unverified".to_string());
    }
    LegacyImpactSupport {
        reason_codes: reasons.into_iter().collect(),
    }
}

pub(crate) fn lower_legacy_texture_set(record: &mut Record, interner: &StringInterner) -> bool {
    if !classify_legacy_texture_set(record, interner).is_ready() {
        return false;
    }
    for field in &mut record.fields {
        if field.sig.as_str() == "DODT" {
            let Ok(bytes) = legacy_decal_data_bytes(&field.value, interner) else {
                return false;
            };
            field.value = FieldValue::Bytes(bytes.as_slice().into());
        }
    }
    true
}

fn legacy_impact_data_bytes(
    value: &FieldValue,
    interner: &StringInterner,
) -> Result<[u8; 24], String> {
    let mut bytes = source_impact_data_bytes(value, interner)?;
    let flags = u32::from_le_bytes(bytes[20..24].try_into().unwrap());
    bytes[20] = (flags & 1) as u8;
    bytes[21..24].fill(0);
    Ok(bytes)
}

fn source_impact_data_bytes(
    value: &FieldValue,
    interner: &StringInterner,
) -> Result<[u8; 24], String> {
    let mut bytes = [0u8; 24];
    match value {
        FieldValue::Bytes(source) if source.len() == bytes.len() => {
            bytes.copy_from_slice(source);
        }
        FieldValue::Struct(fields) => {
            for (index, name) in ["effect_duration", "angle_threshold", "placement_radius"]
                .into_iter()
                .enumerate()
            {
                let offset = [0usize, 8, 12][index];
                bytes[offset..offset + 4].copy_from_slice(
                    &named_f32(fields, name, interner)
                        .ok_or_else(|| "legacy_impact_data_shape_unverified".to_string())?
                        .to_le_bytes(),
                );
            }
            bytes[4..8].copy_from_slice(
                &named_u32(fields, "effect_orientation", interner)
                    .ok_or_else(|| "legacy_impact_data_shape_unverified".to_string())?
                    .to_le_bytes(),
            );
            bytes[16..20].copy_from_slice(
                &named_u32(fields, "sound_level", interner)
                    .ok_or_else(|| "legacy_impact_data_shape_unverified".to_string())?
                    .to_le_bytes(),
            );
            bytes[20..24].copy_from_slice(
                &named_u32(fields, "flags", interner)
                    .ok_or_else(|| "legacy_impact_data_shape_unverified".to_string())?
                    .to_le_bytes(),
            );
        }
        _ => return Err("legacy_impact_data_shape_unverified".to_string()),
    }

    Ok(bytes)
}

fn legacy_decal_data_bytes(
    value: &FieldValue,
    interner: &StringInterner,
) -> Result<[u8; 36], String> {
    let mut bytes = source_decal_data_bytes(value, interner)?;
    bytes[29] &= 0x06;
    bytes[30] = 0;
    bytes[31] = 0;
    bytes[35] = 0;
    Ok(bytes)
}

fn source_decal_data_bytes(
    value: &FieldValue,
    interner: &StringInterner,
) -> Result<[u8; 36], String> {
    let mut bytes = [0u8; 36];
    match value {
        FieldValue::Bytes(source) if source.len() == bytes.len() => {
            bytes.copy_from_slice(source);
        }
        FieldValue::Struct(fields) => {
            for (index, name) in [
                "min_width",
                "max_width",
                "min_height",
                "max_height",
                "depth",
                "shininess",
                "parallax_scale",
            ]
            .into_iter()
            .enumerate()
            {
                let offset = index * 4;
                bytes[offset..offset + 4].copy_from_slice(
                    &named_f32(fields, name, interner)
                        .ok_or_else(|| "legacy_decal_data_shape_unverified".to_string())?
                        .to_le_bytes(),
                );
            }
            for (offset, name) in [
                (28usize, "parallax_passes"),
                (29, "flags"),
                (30, "unknown_u8_9"),
                (31, "unknown_u8_10"),
                (32, "color_red"),
                (33, "color_green"),
                (34, "color_blue"),
                (35, "unknown_u8_14"),
            ] {
                bytes[offset] = named_u8(fields, name, interner)
                    .ok_or_else(|| "legacy_decal_data_shape_unverified".to_string())?;
            }
        }
        _ => return Err("legacy_decal_data_shape_unverified".to_string()),
    }

    Ok(bytes)
}

fn legacy_decal_fallback_warnings(
    record: &Record,
    interner: &StringInterner,
    prefix: &str,
) -> Vec<String> {
    let mut warnings = BTreeSet::new();
    for field in &record.fields {
        match field.sig.as_str() {
            "DATA" => {
                if let Ok(bytes) = source_impact_data_bytes(&field.value, interner) {
                    let flags = u32::from_le_bytes(bytes[20..24].try_into().unwrap());
                    if flags & !1 != 0 {
                        warnings.insert(format!("{prefix}_unmapped_data_flags_omitted"));
                    }
                }
            }
            "DODT" => {
                if let Ok(bytes) = source_decal_data_bytes(&field.value, interner) {
                    if bytes[29] & 1 != 0 {
                        warnings.insert(format!("{prefix}_decal_parallax_omitted"));
                    }
                    if bytes[29] & !0x07 != 0 {
                        warnings.insert(format!("{prefix}_decal_unmapped_flags_omitted"));
                    }
                    if bytes[30] != 0 || bytes[31] != 0 {
                        warnings.insert(format!("{prefix}_decal_unknown_alpha_threshold_omitted"));
                    }
                    if bytes[35] != 0 {
                        warnings.insert(format!("{prefix}_decal_trailing_field_omitted"));
                    }
                }
            }
            _ => {}
        }
    }
    warnings.into_iter().collect()
}

fn scalar_form_id_or_null(value: &FieldValue) -> bool {
    matches!(value, FieldValue::FormKey(_)) || scalar_u32(value) == Some(0)
}

fn scalar_f32(value: &FieldValue) -> Option<f32> {
    match value {
        FieldValue::Float(value) => Some(*value),
        FieldValue::Bytes(bytes) if bytes.len() == 4 => {
            Some(f32::from_le_bytes(bytes.as_slice().try_into().ok()?))
        }
        _ => None,
    }
}

fn scalar_u8(value: &FieldValue) -> Option<u8> {
    u8::try_from(scalar_u64(value)?).ok()
}

fn scalar_u16(value: &FieldValue) -> Option<u16> {
    match value {
        FieldValue::Bytes(bytes) if bytes.len() == 2 => {
            Some(u16::from_le_bytes(bytes.as_slice().try_into().ok()?))
        }
        _ => u16::try_from(scalar_u64(value)?).ok(),
    }
}

fn scalar_u32(value: &FieldValue) -> Option<u32> {
    match value {
        FieldValue::Bytes(bytes) if bytes.len() == 4 => {
            Some(u32::from_le_bytes(bytes.as_slice().try_into().ok()?))
        }
        _ => u32::try_from(scalar_u64(value)?).ok(),
    }
}

fn scalar_u64(value: &FieldValue) -> Option<u64> {
    match value {
        FieldValue::Uint(value) => Some(*value),
        FieldValue::Int(value) => u64::try_from(*value).ok(),
        FieldValue::Bool(value) => Some(u64::from(*value)),
        FieldValue::Bytes(bytes) if bytes.len() == 1 => Some(u64::from(bytes[0])),
        _ => None,
    }
}

fn named_value<'a>(
    fields: &'a [(crate::sym::Sym, FieldValue)],
    name: &str,
    interner: &StringInterner,
) -> Option<&'a FieldValue> {
    fields
        .iter()
        .find(|(key, _)| interner.resolve(*key) == Some(name))
        .map(|(_, value)| value)
}

fn named_f32(
    fields: &[(crate::sym::Sym, FieldValue)],
    name: &str,
    interner: &StringInterner,
) -> Option<f32> {
    scalar_f32(named_value(fields, name, interner)?)
}

fn named_u8(
    fields: &[(crate::sym::Sym, FieldValue)],
    name: &str,
    interner: &StringInterner,
) -> Option<u8> {
    scalar_u8(named_value(fields, name, interner)?)
}

fn named_u32(
    fields: &[(crate::sym::Sym, FieldValue)],
    name: &str,
    interner: &StringInterner,
) -> Option<u32> {
    scalar_u32(named_value(fields, name, interner)?)
}

#[cfg(test)]
mod tests {
    use crate::ids::{FormKey, SigCode, SubrecordSig};
    use crate::schema::AuthoringSchema;
    use smallvec::smallvec;

    use super::*;

    fn record(signature: &str, interner: &StringInterner) -> Record {
        Record::new(
            SigCode::from_str(signature).unwrap(),
            FormKey {
                local: 0x800,
                plugin: interner.intern("FalloutNV.esm"),
            },
        )
    }

    fn field(signature: &[u8; 4], value: FieldValue) -> FieldEntry {
        FieldEntry {
            sig: SubrecordSig(*signature),
            value,
        }
    }

    fn legacy_data(flags: u32) -> Vec<u8> {
        let mut data = Vec::new();
        data.extend_from_slice(&0.25f32.to_le_bytes());
        data.extend_from_slice(&2u32.to_le_bytes());
        data.extend_from_slice(&15.0f32.to_le_bytes());
        data.extend_from_slice(&16.0f32.to_le_bytes());
        data.extend_from_slice(&1u32.to_le_bytes());
        data.extend_from_slice(&flags.to_le_bytes());
        data
    }

    fn legacy_decal(flags: u8) -> Vec<u8> {
        let mut data = Vec::new();
        for value in [1.0f32, 2.0, 3.0, 4.0, 5.0, 6.0, 0.0] {
            data.extend_from_slice(&value.to_le_bytes());
        }
        data.extend_from_slice(&[4, flags, 0, 0, 255, 128, 64, 0]);
        data
    }

    #[test]
    fn impact_data_and_shared_decal_bits_pack_as_exact_fo4_layouts() {
        let interner = StringInterner::new();
        let mut impact = record("IPCT", &interner);
        impact.fields = smallvec![
            field(b"EDID", FieldValue::String(interner.intern("LegacyImpact"))),
            field(b"MODB", FieldValue::Float(12.0)),
            field(b"MODT", FieldValue::Bytes([1, 2, 3].as_slice().into())),
            field(b"DATA", FieldValue::Bytes(legacy_data(1).into())),
            field(b"DODT", FieldValue::Bytes(legacy_decal(0x06).into())),
        ];

        assert!(classify_legacy_impact(&impact, &interner).is_ready());
        assert!(lower_legacy_impact(&mut impact, &interner));
        assert!(
            !impact
                .fields
                .iter()
                .any(|field| matches!(field.sig.0, sig if sig == *b"MODB" || sig == *b"MODT"))
        );

        let schema = AuthoringSchema::for_game("fo4").unwrap();
        let definition = schema.record_def("IPCT");
        let data = impact
            .fields
            .iter()
            .find(|field| field.sig.0 == *b"DATA")
            .unwrap();
        let decal = impact
            .fields
            .iter()
            .find(|field| field.sig.0 == *b"DODT")
            .unwrap();
        let packed_data =
            crate::target_write::encode_field_pub(data, definition, &interner).unwrap();
        let packed_decal =
            crate::target_write::encode_field_pub(decal, definition, &interner).unwrap();
        assert_eq!(&packed_data[20..24], &[1, 0, 0, 0]);
        assert_eq!(packed_decal, legacy_decal(0x06));
    }

    #[test]
    fn source_only_impact_and_decal_semantics_use_safe_fallbacks() {
        let interner = StringInterner::new();
        let mut impact = record("IPCT", &interner);
        let mut decal = legacy_decal(1);
        decal[30] = 7;
        impact.fields = smallvec![
            field(b"DATA", FieldValue::Bytes(legacy_data(0).into())),
            field(b"DODT", FieldValue::Bytes(decal.into())),
        ];
        assert!(classify_legacy_impact(&impact, &interner).is_ready());
        assert_eq!(
            legacy_impact_fallback_warnings(&impact, &interner),
            [
                "legacy_impact_decal_parallax_omitted",
                "legacy_impact_decal_unknown_alpha_threshold_omitted",
            ]
        );
        assert!(lower_legacy_impact(&mut impact, &interner));
        let lowered_decal = impact
            .fields
            .iter()
            .find(|field| field.sig.as_str() == "DODT")
            .unwrap();
        assert_eq!(
            lowered_decal.value,
            FieldValue::Bytes(legacy_decal(0).into())
        );

        let mut decal = legacy_decal(0);
        decal[30] = 7;
        impact.fields[1].value = FieldValue::Bytes(decal.into());
        assert!(classify_legacy_impact(&impact, &interner).is_ready());
        assert_eq!(
            legacy_impact_fallback_warnings(&impact, &interner),
            ["legacy_impact_decal_unknown_alpha_threshold_omitted"]
        );
    }

    #[test]
    fn texture_set_decal_relayout_preserves_only_schema_identical_bits() {
        let interner = StringInterner::new();
        let mut texture = record("TXST", &interner);
        texture.fields = smallvec![
            field(
                b"EDID",
                FieldValue::String(interner.intern("ImpactTexture"))
            ),
            field(b"OBND", FieldValue::Bytes([0; 12].as_slice().into())),
            field(
                b"TX00",
                FieldValue::String(interner.intern("textures\\impact_d.dds"))
            ),
            field(
                b"TX01",
                FieldValue::String(interner.intern("textures\\impact_n.dds"))
            ),
            field(b"DODT", FieldValue::Bytes(legacy_decal(2).into())),
            field(b"DNAM", FieldValue::Uint(1)),
        ];

        assert!(classify_legacy_texture_set(&texture, &interner).is_ready());
        assert!(lower_legacy_texture_set(&mut texture, &interner));
        let schema = AuthoringSchema::for_game("fo4").unwrap();
        let decal = texture
            .fields
            .iter()
            .find(|field| field.sig.0 == *b"DODT")
            .unwrap();
        assert_eq!(
            crate::target_write::encode_field_pub(decal, schema.record_def("TXST"), &interner)
                .unwrap(),
            legacy_decal(2)
        );
    }
}
