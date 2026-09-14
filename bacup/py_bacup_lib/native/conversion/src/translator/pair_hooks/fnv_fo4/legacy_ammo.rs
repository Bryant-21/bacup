use crate::ids::SubrecordSig;
use crate::record::{FieldEntry, FieldValue, Record};
use crate::sym::StringInterner;
use smallvec::SmallVec;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LegacyAmmoSourceFamily {
    Fnv,
    Fo3,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LegacyAmmoSupport {
    pub reason_codes: Vec<String>,
}

impl LegacyAmmoSupport {
    pub(crate) fn is_ready(&self) -> bool {
        self.reason_codes.is_empty()
    }
}

pub(crate) fn legacy_ammo_fallback_warnings(
    record: &Record,
    source: LegacyAmmoSourceFamily,
    interner: &StringInterner,
) -> Vec<String> {
    let Ok((speed, flags, _, clip_rounds)) = source_data_from_record(record, interner) else {
        return Vec::new();
    };
    let mut warnings = Vec::new();
    if speed.to_bits() != 1.0_f32.to_bits() {
        warnings.push("legacy_ammo_speed_omitted_in_favor_of_projectile_defaults".to_string());
    }
    if flags & !0x03 != 0 {
        warnings.push("legacy_ammo_unmapped_flag_bits_omitted".to_string());
    }
    if clip_rounds != 0 {
        warnings.push("legacy_ammo_clip_rounds_omitted".to_string());
    }
    if source == LegacyAmmoSourceFamily::Fnv {
        let Ok((projectiles_per_shot, _, _, consumed, consumed_percentage)) =
            source_dat2_from_record(record, interner)
        else {
            return Vec::new();
        };
        if projectiles_per_shot != 1 {
            warnings.push("legacy_ammo_multi_projectile_count_uses_single_projectile".to_string());
        }
        if consumed.is_some() || consumed_percentage.to_bits() != 0.0_f32.to_bits() {
            warnings.push("legacy_ammo_consumption_chain_omitted".to_string());
        }
    }
    if record.fields.iter().any(|field| field.sig.0 == *b"SCRI") {
        warnings.push("legacy_ammo_script_omitted".to_string());
    }
    warnings.sort();
    warnings.dedup();
    warnings
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct LegacyAmmoProjection {
    value: u32,
    weight: f32,
    projectile: Option<crate::ids::FormKey>,
    flags: u8,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LegacyAmmoEmbeddedReference {
    pub form_key: crate::ids::FormKey,
    pub locator: &'static str,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct RawLegacyAmmoDat2 {
    projectiles_per_shot: u32,
    projectile: u32,
    weight: f32,
    consumed_ammo: u32,
    consumed_percentage: f32,
}

const PRESERVED_FIELDS: &[[u8; 4]] = &[
    *b"EDID", *b"OBND", *b"FULL", *b"MODL", *b"DEST", *b"DSTD", *b"DMDL", *b"DSTF", *b"YNAM",
    *b"ZNAM", *b"ONAM",
];

const NON_RUNTIME_SOURCE_FIELDS: &[[u8; 4]] = &[
    *b"MODB", *b"MODT", *b"MODS", *b"MODD", *b"ICON", *b"MICO", *b"DMDT", *b"QNAM",
];

const OWNER_FOLDED_SOURCE_FIELDS: &[[u8; 4]] = &[*b"RCIL"];

pub(crate) fn classify_legacy_ammo(
    record: &Record,
    source: LegacyAmmoSourceFamily,
    interner: &StringInterner,
) -> LegacyAmmoSupport {
    match ammo_projection(record, source, interner) {
        Ok(_) => LegacyAmmoSupport {
            reason_codes: Vec::new(),
        },
        Err(mut reason_codes) => {
            reason_codes.sort();
            reason_codes.dedup();
            LegacyAmmoSupport { reason_codes }
        }
    }
}

pub(crate) fn lower_legacy_ammo(
    record: &mut Record,
    source: LegacyAmmoSourceFamily,
    interner: &StringInterner,
) -> bool {
    if record.sig.as_str() != "AMMO" {
        return false;
    }
    let projection = ammo_projection(record, source, interner)
        .unwrap_or_else(|_| fallback_ammo_projection(record, source, interner));

    let mut fields = record
        .fields
        .iter()
        .filter(|field| PRESERVED_FIELDS.contains(&field.sig.0))
        .cloned()
        .collect::<Vec<_>>();
    fields.extend([
        field(
            "DATA",
            FieldValue::Struct(vec![
                (
                    interner.intern("value"),
                    FieldValue::Uint(u64::from(projection.value)),
                ),
                (
                    interner.intern("weight"),
                    FieldValue::Float(projection.weight),
                ),
            ]),
        ),
        field(
            "DNAM",
            FieldValue::Struct(vec![
                (
                    interner.intern("projectile"),
                    projection
                        .projectile
                        .map(FieldValue::FormKey)
                        .unwrap_or(FieldValue::Uint(0)),
                ),
                (
                    interner.intern("flags"),
                    FieldValue::Uint(u64::from(projection.flags)),
                ),
                (interner.intern("unknown_u8_2"), FieldValue::Uint(0)),
                (interner.intern("unknown_u8_3"), FieldValue::Uint(0)),
                (interner.intern("unknown_u8_4"), FieldValue::Uint(0)),
                (interner.intern("damage"), FieldValue::Float(0.0)),
                (interner.intern("health"), FieldValue::Uint(0)),
            ]),
        ),
    ]);
    record.fields = SmallVec::from_vec(fields);
    true
}

fn fallback_ammo_projection(
    record: &Record,
    source: LegacyAmmoSourceFamily,
    interner: &StringInterner,
) -> LegacyAmmoProjection {
    let (_, flags, value, _) = source_data_from_record(record, interner).unwrap_or((1.0, 0, 0, 0));
    let (weight, projectile) = match source {
        LegacyAmmoSourceFamily::Fnv => source_dat2_from_record(record, interner)
            .ok()
            .map(|(_, projectile, weight, _, _)| {
                (
                    if weight.is_finite() && weight >= 0.0 {
                        weight
                    } else {
                        0.0
                    },
                    projectile,
                )
            })
            .unwrap_or((0.0, None)),
        LegacyAmmoSourceFamily::Fo3 => (0.0, None),
    };
    LegacyAmmoProjection {
        value,
        weight,
        projectile,
        flags: flags & 0x03,
    }
}

pub(crate) fn legacy_ammo_embedded_references(
    record: &Record,
    source_plugin_name: &str,
    source_master_names: &[String],
    interner: &StringInterner,
) -> Result<Vec<LegacyAmmoEmbeddedReference>, String> {
    let mut dat2 = record
        .fields
        .iter()
        .filter(|field| field.sig.as_str() == "DAT2");
    let Some(field) = dat2.next() else {
        return Ok(Vec::new());
    };
    if dat2.next().is_some() {
        return Err("legacy_ammo_field_multiplicity_dat2".to_string());
    }
    let FieldValue::Bytes(bytes) = &field.value else {
        return Ok(Vec::new());
    };
    let raw = raw_legacy_ammo_dat2(bytes).ok_or_else(|| "legacy_ammo_invalid_dat2".to_string())?;
    let mut references = Vec::new();
    for (raw_form_id, locator) in [
        (raw.projectile, "DAT2[0].projectile@4"),
        (raw.consumed_ammo, "DAT2[0].consumed_ammo@12"),
    ] {
        if raw_form_id == 0 {
            continue;
        }
        let form_key = source_form_key_for_raw_form_id(
            raw_form_id,
            source_plugin_name,
            source_master_names,
            interner,
        )
        .ok_or_else(|| format!("legacy_ammo_unresolved_raw_formid_{raw_form_id:08x}"))?;
        references.push(LegacyAmmoEmbeddedReference { form_key, locator });
    }
    Ok(references)
}

pub(crate) fn decode_legacy_ammo_dat2(
    record: &mut Record,
    source_plugin_name: &str,
    source_master_names: &[String],
    interner: &StringInterner,
) -> Result<bool, String> {
    let mut matches = record
        .fields
        .iter()
        .enumerate()
        .filter(|(_, field)| field.sig.as_str() == "DAT2");
    let Some((index, field)) = matches.next() else {
        return Ok(false);
    };
    if matches.next().is_some() {
        return Err("legacy_ammo_field_multiplicity_dat2".to_string());
    }
    let FieldValue::Bytes(bytes) = &field.value else {
        return Ok(false);
    };
    let raw = raw_legacy_ammo_dat2(bytes).ok_or_else(|| "legacy_ammo_invalid_dat2".to_string())?;
    let projectile = typed_raw_form_id(
        raw.projectile,
        source_plugin_name,
        source_master_names,
        interner,
    )?;
    let consumed_ammo = typed_raw_form_id(
        raw.consumed_ammo,
        source_plugin_name,
        source_master_names,
        interner,
    )?;
    record.fields[index].value = FieldValue::Struct(vec![
        (
            interner.intern("proj_per_shot"),
            FieldValue::Uint(u64::from(raw.projectiles_per_shot)),
        ),
        (interner.intern("projectile"), projectile),
        (interner.intern("weight"), FieldValue::Float(raw.weight)),
        (interner.intern("consumed_ammo"), consumed_ammo),
        (
            interner.intern("consumed_percentage"),
            FieldValue::Float(raw.consumed_percentage),
        ),
    ]);
    Ok(true)
}

fn typed_raw_form_id(
    raw_form_id: u32,
    source_plugin_name: &str,
    source_master_names: &[String],
    interner: &StringInterner,
) -> Result<FieldValue, String> {
    if raw_form_id == 0 {
        return Ok(FieldValue::Uint(0));
    }
    source_form_key_for_raw_form_id(
        raw_form_id,
        source_plugin_name,
        source_master_names,
        interner,
    )
    .map(FieldValue::FormKey)
    .ok_or_else(|| format!("legacy_ammo_unresolved_raw_formid_{raw_form_id:08x}"))
}

pub(super) fn source_form_key_for_raw_form_id(
    raw_form_id: u32,
    source_plugin_name: &str,
    source_master_names: &[String],
    interner: &StringInterner,
) -> Option<crate::ids::FormKey> {
    let local = raw_form_id & 0x00ff_ffff;
    if local == 0 {
        return None;
    }
    let index = ((raw_form_id >> 24) & 0xff) as usize;
    let plugin = if index < source_master_names.len() {
        source_master_names.get(index)?.as_str()
    } else if index == source_master_names.len() {
        source_plugin_name
    } else {
        return None;
    };
    Some(crate::ids::FormKey {
        local,
        plugin: interner.intern(plugin),
    })
}

fn raw_legacy_ammo_dat2(bytes: &[u8]) -> Option<RawLegacyAmmoDat2> {
    if !matches!(bytes.len(), 12 | 20) {
        return None;
    }
    let (consumed_ammo, consumed_percentage) = if bytes.len() == 20 {
        (
            u32::from_le_bytes(bytes[12..16].try_into().ok()?),
            f32::from_le_bytes(bytes[16..20].try_into().ok()?),
        )
    } else {
        (0, 0.0)
    };
    Some(RawLegacyAmmoDat2 {
        projectiles_per_shot: u32::from_le_bytes(bytes[0..4].try_into().ok()?),
        projectile: u32::from_le_bytes(bytes[4..8].try_into().ok()?),
        weight: f32::from_le_bytes(bytes[8..12].try_into().ok()?),
        consumed_ammo,
        consumed_percentage,
    })
}

fn ammo_projection(
    record: &Record,
    source: LegacyAmmoSourceFamily,
    interner: &StringInterner,
) -> Result<LegacyAmmoProjection, Vec<String>> {
    let mut reasons = Vec::new();
    if record.sig.as_str() != "AMMO" {
        reasons.push("legacy_ammo_wrong_signature".to_string());
        return Err(reasons);
    }

    for field in &record.fields {
        if PRESERVED_FIELDS.contains(&field.sig.0)
            || NON_RUNTIME_SOURCE_FIELDS.contains(&field.sig.0)
            || OWNER_FOLDED_SOURCE_FIELDS.contains(&field.sig.0)
            || matches!(field.sig.0, field if field == *b"DATA" || field == *b"DAT2" || field == *b"SCRI")
        {
            continue;
        }
        reasons.push(format!(
            "legacy_ammo_unsupported_runtime_field_{}",
            field.sig.as_str().to_ascii_lowercase()
        ));
    }

    let data = exact_field(record, "DATA", &mut reasons)
        .and_then(|field| source_data(&field.value, interner));
    if data.is_none() {
        reasons.push("legacy_ammo_invalid_data".to_string());
    }
    let (_, flags, value, _) = data.unwrap_or((f32::NAN, u8::MAX, 0, u8::MAX));

    let (weight, projectile) = match source {
        LegacyAmmoSourceFamily::Fnv => {
            let dat2 = exact_field(record, "DAT2", &mut reasons)
                .and_then(|field| source_dat2(&field.value, interner));
            if dat2.is_none() {
                reasons.push("legacy_ammo_invalid_dat2".to_string());
            }
            let (projectiles_per_shot, projectile, weight, consumed, consumed_percentage) =
                dat2.unwrap_or((0, None, f32::NAN, None, f32::NAN));
            let _ = (projectiles_per_shot, consumed, consumed_percentage);
            (weight, projectile)
        }
        LegacyAmmoSourceFamily::Fo3 => {
            if record
                .fields
                .iter()
                .any(|field| field.sig.as_str() == "DAT2")
            {
                reasons.push("legacy_ammo_fo3_unexpected_dat2".to_string());
            }
            (0.0, None)
        }
    };
    if !weight.is_finite() || weight < 0.0 {
        reasons.push("legacy_ammo_invalid_weight".to_string());
    }

    if reasons.is_empty() {
        Ok(LegacyAmmoProjection {
            value,
            weight,
            projectile,
            flags: flags & 0x03,
        })
    } else {
        Err(reasons)
    }
}

fn source_data_from_record(
    record: &Record,
    interner: &StringInterner,
) -> Result<(f32, u8, u32, u8), ()> {
    let mut fields = record
        .fields
        .iter()
        .filter(|field| field.sig.as_str() == "DATA");
    let field = fields.next().ok_or(())?;
    if fields.next().is_some() {
        return Err(());
    }
    source_data(&field.value, interner).ok_or(())
}

fn source_dat2_from_record(record: &Record, interner: &StringInterner) -> Result<SourceDat2, ()> {
    let mut fields = record
        .fields
        .iter()
        .filter(|field| field.sig.as_str() == "DAT2");
    let field = fields.next().ok_or(())?;
    if fields.next().is_some() {
        return Err(());
    }
    source_dat2(&field.value, interner).ok_or(())
}

fn source_data(value: &FieldValue, interner: &StringInterner) -> Option<(f32, u8, u32, u8)> {
    match value {
        FieldValue::Bytes(bytes) if bytes.len() == 13 => {
            let speed = f32::from_le_bytes(bytes[0..4].try_into().ok()?);
            let flags = bytes[4];
            if bytes[5..8].iter().any(|byte| *byte != 0) {
                return None;
            }
            let value = u32::try_from(i32::from_le_bytes(bytes[8..12].try_into().ok()?)).ok()?;
            Some((speed, flags, value, bytes[12]))
        }
        FieldValue::Struct(fields)
            if ["unknown_u8_2", "unknown_u8_3", "unknown_u8_4"]
                .into_iter()
                .all(|name| named_u8(fields, name, interner) == Some(0)) =>
        {
            Some((
                named_f32(fields, "speed", interner)?,
                named_u8(fields, "flags", interner)?,
                named_u32(fields, "value", interner)?,
                named_u8(fields, "clip_rounds", interner)?,
            ))
        }
        _ => None,
    }
}

type SourceDat2 = (
    u32,
    Option<crate::ids::FormKey>,
    f32,
    Option<crate::ids::FormKey>,
    f32,
);

fn source_dat2(value: &FieldValue, interner: &StringInterner) -> Option<SourceDat2> {
    match value {
        FieldValue::Bytes(bytes) => {
            let raw = raw_legacy_ammo_dat2(bytes)?;
            if raw.projectile != 0 || raw.consumed_ammo != 0 {
                return None;
            }
            Some((
                raw.projectiles_per_shot,
                None,
                raw.weight,
                None,
                raw.consumed_percentage,
            ))
        }
        FieldValue::Struct(fields) => Some((
            named_u32(fields, "proj_per_shot", interner)?,
            named_form_key(fields, "projectile", interner),
            named_f32(fields, "weight", interner)?,
            named_form_key(fields, "consumed_ammo", interner),
            named_f32(fields, "consumed_percentage", interner)?,
        )),
        _ => None,
    }
}

fn exact_field<'a>(
    record: &'a Record,
    signature: &str,
    reasons: &mut Vec<String>,
) -> Option<&'a FieldEntry> {
    let mut fields = record
        .fields
        .iter()
        .filter(|field| field.sig.as_str() == signature);
    let field = fields.next();
    if fields.next().is_some() {
        reasons.push(format!(
            "legacy_ammo_field_multiplicity_{}",
            signature.to_ascii_lowercase()
        ));
        return None;
    }
    field
}

fn named_value<'a>(
    fields: &'a [(crate::sym::Sym, FieldValue)],
    name: &str,
    interner: &StringInterner,
) -> Option<&'a FieldValue> {
    fields
        .iter()
        .find_map(|(key, value)| (interner.resolve(*key) == Some(name)).then_some(value))
}

fn named_u32(
    fields: &[(crate::sym::Sym, FieldValue)],
    name: &str,
    interner: &StringInterner,
) -> Option<u32> {
    match named_value(fields, name, interner)? {
        FieldValue::Uint(value) => u32::try_from(*value).ok(),
        FieldValue::Int(value) => u32::try_from(*value).ok(),
        _ => None,
    }
}

fn named_u8(
    fields: &[(crate::sym::Sym, FieldValue)],
    name: &str,
    interner: &StringInterner,
) -> Option<u8> {
    named_u32(fields, name, interner).and_then(|value| u8::try_from(value).ok())
}

fn named_f32(
    fields: &[(crate::sym::Sym, FieldValue)],
    name: &str,
    interner: &StringInterner,
) -> Option<f32> {
    match named_value(fields, name, interner)? {
        FieldValue::Float(value) => Some(*value),
        _ => None,
    }
}

fn named_form_key(
    fields: &[(crate::sym::Sym, FieldValue)],
    name: &str,
    interner: &StringInterner,
) -> Option<crate::ids::FormKey> {
    match named_value(fields, name, interner) {
        Some(FieldValue::FormKey(form_key)) if form_key.local != 0 => Some(*form_key),
        _ => None,
    }
}

fn field(signature: &str, value: FieldValue) -> FieldEntry {
    FieldEntry {
        sig: SubrecordSig::from_str(signature).expect("valid subrecord signature"),
        value,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::{FormKey, SigCode};
    use crate::schema::AuthoringSchema;
    use crate::target_normalize::{TargetRecordNormalization, TargetRecordNormalizer};
    use crate::target_write::encode_field_pub;

    fn fixture(source: LegacyAmmoSourceFamily, interner: &StringInterner) -> Record {
        let plugin = interner.intern(match source {
            LegacyAmmoSourceFamily::Fnv => "FalloutNV.esm",
            LegacyAmmoSourceFamily::Fo3 => "Fallout3.esm",
        });
        let mut record = Record::new(
            SigCode::from_str("AMMO").unwrap(),
            FormKey {
                local: 0x123,
                plugin,
            },
        );
        record.fields.extend([
            field(
                "EDID",
                FieldValue::String(interner.intern("AmmoCreatureFixture")),
            ),
            field(
                "DATA",
                FieldValue::Struct(vec![
                    (interner.intern("speed"), FieldValue::Float(1.0)),
                    (interner.intern("flags"), FieldValue::Uint(2)),
                    (interner.intern("unknown_u8_2"), FieldValue::Uint(0)),
                    (interner.intern("unknown_u8_3"), FieldValue::Uint(0)),
                    (interner.intern("unknown_u8_4"), FieldValue::Uint(0)),
                    (interner.intern("value"), FieldValue::Uint(3)),
                    (interner.intern("clip_rounds"), FieldValue::Uint(0)),
                ]),
            ),
        ]);
        if source == LegacyAmmoSourceFamily::Fnv {
            record.fields.push(field(
                "DAT2",
                FieldValue::Struct(vec![
                    (interner.intern("proj_per_shot"), FieldValue::Uint(1)),
                    (
                        interner.intern("projectile"),
                        FieldValue::FormKey(FormKey {
                            local: 0x456,
                            plugin,
                        }),
                    ),
                    (interner.intern("weight"), FieldValue::Float(0.1)),
                    (interner.intern("consumed_ammo"), FieldValue::Uint(0)),
                    (
                        interner.intern("consumed_percentage"),
                        FieldValue::Float(0.0),
                    ),
                ]),
            ));
        }
        record
    }

    #[test]
    fn fnv_ammo_projection_normalizes_to_exact_fo4_payload_widths() {
        let interner = StringInterner::new();
        let mut record = fixture(LegacyAmmoSourceFamily::Fnv, &interner);
        assert!(classify_legacy_ammo(&record, LegacyAmmoSourceFamily::Fnv, &interner).is_ready());
        assert!(lower_legacy_ammo(
            &mut record,
            LegacyAmmoSourceFamily::Fnv,
            &interner
        ));

        let source_schema = AuthoringSchema::for_game("fnv").unwrap();
        let target_schema = AuthoringSchema::for_game("fo4").unwrap();
        let normalizer = TargetRecordNormalizer {
            target_schema: &target_schema,
            source_record_def: source_schema.record_def("AMMO"),
            interner: Some(&interner),
        };
        let TargetRecordNormalization::Keep(record) = normalizer.normalize(record) else {
            panic!("projected AMMO must remain target-supported");
        };
        let data = record
            .fields
            .iter()
            .find(|field| field.sig.as_str() == "DATA")
            .unwrap();
        let dnam = record
            .fields
            .iter()
            .find(|field| field.sig.as_str() == "DNAM")
            .unwrap();
        let target_record_def = target_schema.record_def("AMMO");
        assert_eq!(
            encode_field_pub(data, target_record_def, &interner)
                .unwrap()
                .len(),
            8
        );
        assert_eq!(
            encode_field_pub(dnam, target_record_def, &interner)
                .unwrap()
                .len(),
            16
        );
    }

    #[test]
    fn source_only_ammo_semantics_use_a_safe_single_projectile_fallback() {
        let interner = StringInterner::new();
        let mut record = fixture(LegacyAmmoSourceFamily::Fnv, &interner);
        let FieldValue::Struct(data) = &mut record.fields[1].value else {
            panic!("fixture DATA");
        };
        data.iter_mut()
            .find(|(name, _)| interner.resolve(*name) == Some("speed"))
            .unwrap()
            .1 = FieldValue::Float(1.5);
        data.iter_mut()
            .find(|(name, _)| interner.resolve(*name) == Some("clip_rounds"))
            .unwrap()
            .1 = FieldValue::Uint(3);
        let FieldValue::Struct(dat2) = &mut record.fields[2].value else {
            panic!("fixture DAT2");
        };
        dat2.iter_mut()
            .find(|(name, _)| interner.resolve(*name) == Some("proj_per_shot"))
            .unwrap()
            .1 = FieldValue::Uint(2);
        record.fields.push(field("SCRI", FieldValue::Uint(1)));
        let support = classify_legacy_ammo(&record, LegacyAmmoSourceFamily::Fnv, &interner);
        assert!(support.is_ready());
        assert_eq!(
            legacy_ammo_fallback_warnings(&record, LegacyAmmoSourceFamily::Fnv, &interner),
            vec![
                "legacy_ammo_clip_rounds_omitted",
                "legacy_ammo_multi_projectile_count_uses_single_projectile",
                "legacy_ammo_script_omitted",
                "legacy_ammo_speed_omitted_in_favor_of_projectile_defaults",
            ]
        );
        assert!(lower_legacy_ammo(
            &mut record,
            LegacyAmmoSourceFamily::Fnv,
            &interner
        ));
        assert!(
            record
                .fields
                .iter()
                .all(|field| field.sig.as_str() != "SCRI")
        );
    }

    #[test]
    fn malformed_optional_dat2_uses_target_defaults_instead_of_blocking_ammo() {
        let interner = StringInterner::new();
        let mut record = fixture(LegacyAmmoSourceFamily::Fnv, &interner);
        record
            .fields
            .iter_mut()
            .find(|field| field.sig.as_str() == "DAT2")
            .unwrap()
            .value = FieldValue::Bytes(vec![1, 2, 3].into());

        assert!(!classify_legacy_ammo(&record, LegacyAmmoSourceFamily::Fnv, &interner).is_ready());
        assert!(lower_legacy_ammo(
            &mut record,
            LegacyAmmoSourceFamily::Fnv,
            &interner
        ));
        assert!(
            record
                .fields
                .iter()
                .all(|field| field.sig.as_str() != "DAT2")
        );
        let FieldValue::Struct(data) = &record
            .fields
            .iter()
            .find(|field| field.sig.as_str() == "DATA")
            .unwrap()
            .value
        else {
            panic!("fallback DATA must be structured")
        };
        assert_eq!(named_f32(data, "weight", &interner), Some(0.0));
    }

    #[test]
    fn embedded_dat2_formids_use_exact_source_master_order() {
        let interner = StringInterner::new();
        let mut record = Record::new(
            SigCode::from_str("AMMO").unwrap(),
            FormKey {
                local: 0x100,
                plugin: interner.intern("DeadMoney.esm"),
            },
        );
        let mut dat2 = Vec::new();
        dat2.extend_from_slice(&1u32.to_le_bytes());
        dat2.extend_from_slice(&0x0012_3456u32.to_le_bytes());
        dat2.extend_from_slice(&0.2f32.to_le_bytes());
        dat2.extend_from_slice(&0x0100_0abcu32.to_le_bytes());
        dat2.extend_from_slice(&0.25f32.to_le_bytes());
        record
            .fields
            .push(field("DAT2", FieldValue::Bytes(dat2.into())));
        let masters = vec!["FalloutNV.esm".to_string()];

        let references =
            legacy_ammo_embedded_references(&record, "DeadMoney.esm", &masters, &interner).unwrap();
        assert_eq!(
            references,
            [
                LegacyAmmoEmbeddedReference {
                    form_key: FormKey {
                        local: 0x123456,
                        plugin: interner.intern("FalloutNV.esm"),
                    },
                    locator: "DAT2[0].projectile@4",
                },
                LegacyAmmoEmbeddedReference {
                    form_key: FormKey {
                        local: 0xabc,
                        plugin: interner.intern("DeadMoney.esm"),
                    },
                    locator: "DAT2[0].consumed_ammo@12",
                },
            ]
        );
        assert!(
            decode_legacy_ammo_dat2(&mut record, "DeadMoney.esm", &masters, &interner).unwrap()
        );
        let FieldValue::Struct(fields) = &record.fields[0].value else {
            panic!("DAT2 must become mapper-visible typed fields");
        };
        assert_eq!(
            named_form_key(fields, "projectile", &interner),
            Some(references[0].form_key)
        );
        assert_eq!(
            named_form_key(fields, "consumed_ammo", &interner),
            Some(references[1].form_key)
        );
    }

    #[test]
    fn short_dat2_layout_has_no_phantom_consumed_ammo() {
        let interner = StringInterner::new();
        let mut record = Record::new(
            SigCode::from_str("AMMO").unwrap(),
            FormKey {
                local: 0x100,
                plugin: interner.intern("FalloutNV.esm"),
            },
        );
        let mut dat2 = Vec::new();
        dat2.extend_from_slice(&1u32.to_le_bytes());
        dat2.extend_from_slice(&0x0000_0456u32.to_le_bytes());
        dat2.extend_from_slice(&0.1f32.to_le_bytes());
        record
            .fields
            .push(field("DAT2", FieldValue::Bytes(dat2.into())));

        let references =
            legacy_ammo_embedded_references(&record, "FalloutNV.esm", &[], &interner).unwrap();
        assert_eq!(references.len(), 1);
        assert_eq!(references[0].locator, "DAT2[0].projectile@4");
        assert!(decode_legacy_ammo_dat2(&mut record, "FalloutNV.esm", &[], &interner).unwrap());
        let FieldValue::Struct(fields) = &record.fields[0].value else {
            panic!("DAT2 must become mapper-visible typed fields");
        };
        assert_eq!(
            named_form_key(fields, "projectile", &interner),
            Some(references[0].form_key)
        );
        assert_eq!(named_form_key(fields, "consumed_ammo", &interner), None);
    }
}
