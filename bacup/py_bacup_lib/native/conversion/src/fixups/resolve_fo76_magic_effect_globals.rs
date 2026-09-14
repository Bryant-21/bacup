//! Materialize FO76 global-backed effect magnitude and duration values for FO4.

use std::collections::HashMap;

use crate::fixups::{Fixup, FixupConfig, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::ids::{FormKey, SigCode};
use crate::record::{FieldValue, Record};
use crate::session::PluginSession;
use crate::sym::StringInterner;

const TARGET_MASTER: &str = "Fallout4.esm";

#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct EffectGlobalValues {
    magnitude: Option<f32>,
    duration: Option<u32>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct EffectGlobalRefs {
    magnitude: Option<FormKey>,
    duration: Option<FormKey>,
}

pub struct ResolveFo76MagicEffectGlobalsFixup;

impl Fixup for ResolveFo76MagicEffectGlobalsFixup {
    fn name(&self) -> &'static str {
        "resolve_fo76_magic_effect_globals"
    }

    fn uses_session(&self) -> bool {
        true
    }

    fn applies_to_session(&self, session: &PluginSession, _config: &FixupConfig) -> bool {
        session
            .target_masters()
            .iter()
            .any(|master| master.eq_ignore_ascii_case(TARGET_MASTER))
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
        let source_schema = config
            .source_schema
            .as_deref()
            .ok_or_else(|| FixupError::Other("missing source schema in fixup config".into()))?;
        let Some(source_slot) = session.source_slot_opt() else {
            return Ok(FixupReport::empty());
        };
        let source_plugin_name = source_slot.parsed.plugin_name.clone();
        let source_master_names = source_slot.parsed.header.masters.clone();
        let source_plugin = mapper.interner.intern(&source_plugin_name);
        let mut global_cache = HashMap::new();
        let mut changed_records = Vec::new();

        for sig_str in ["ALCH", "ENCH", "SPEL"] {
            let sig = SigCode::from_str(sig_str).map_err(FixupError::SchemaError)?;
            let form_keys = session
                .form_keys_of_sig(sig, mapper.interner)
                .map_err(|error| FixupError::HandleError(error.to_string()))?;

            for target_form_key in form_keys {
                let source_form_key = FormKey {
                    plugin: source_plugin,
                    local: target_form_key.local,
                };
                let source_record = match session.source_record_decoded(
                    &source_form_key,
                    source_schema,
                    mapper.interner,
                ) {
                    Ok(record) => record,
                    Err(_) => continue,
                };
                let refs = source_effect_global_refs(
                    &source_record,
                    &source_master_names,
                    &source_plugin_name,
                    mapper.interner,
                );
                if !refs
                    .iter()
                    .any(|entry| entry.magnitude.is_some() || entry.duration.is_some())
                {
                    continue;
                }
                let values = resolve_effect_global_values(
                    session,
                    source_schema,
                    &refs,
                    mapper.interner,
                    &mut global_cache,
                );
                let mut target_record = match session.record_decoded(
                    &target_form_key,
                    target_schema,
                    mapper.interner,
                ) {
                    Ok(record) => record,
                    Err(_) => continue,
                };
                if apply_effect_global_values(&mut target_record, &values, mapper.interner) {
                    changed_records.push(target_record);
                }
            }
        }

        if changed_records.is_empty() {
            return Ok(FixupReport::empty());
        }
        let expected = changed_records.len();
        let replaced = session
            .replace_records_contents(changed_records, target_schema, mapper.interner)
            .map_err(|error| FixupError::HandleError(error.to_string()))?;
        if replaced != expected {
            return Err(FixupError::HandleError(format!(
                "resolve_fo76_magic_effect_globals replaced {replaced} of {expected} expected records"
            )));
        }

        let mut report = FixupReport::empty();
        report.records_changed = replaced.try_into().unwrap_or(u32::MAX);
        Ok(report)
    }
}

fn source_effect_global_refs(
    record: &Record,
    source_master_names: &[String],
    source_plugin_name: &str,
    interner: &StringInterner,
) -> Vec<EffectGlobalRefs> {
    if !matches!(record.sig.as_str(), "ALCH" | "ENCH" | "SPEL") {
        return Vec::new();
    }

    let mut refs = Vec::new();
    let mut effect_index = None;
    for field in &record.fields {
        match field.sig.as_str() {
            "EFID" => {
                refs.push(EffectGlobalRefs::default());
                effect_index = Some(refs.len() - 1);
            }
            "MAGG" => {
                if let Some(index) = effect_index {
                    refs[index].magnitude = source_form_key_from_value(
                        &field.value,
                        source_master_names,
                        source_plugin_name,
                        interner,
                    );
                }
            }
            "DURG" => {
                if let Some(index) = effect_index {
                    refs[index].duration = source_form_key_from_value(
                        &field.value,
                        source_master_names,
                        source_plugin_name,
                        interner,
                    );
                }
            }
            _ => {}
        }
    }
    refs
}

fn resolve_effect_global_values(
    session: &mut PluginSession,
    source_schema: &crate::schema::AuthoringSchema,
    refs: &[EffectGlobalRefs],
    interner: &StringInterner,
    cache: &mut HashMap<FormKey, Option<f32>>,
) -> Vec<EffectGlobalValues> {
    refs.iter()
        .map(|entry| EffectGlobalValues {
            magnitude: entry
                .magnitude
                .and_then(|form_key| {
                    source_global_value(session, source_schema, form_key, interner, cache)
                })
                .filter(|value| value.is_finite()),
            duration: entry
                .duration
                .and_then(|form_key| {
                    source_global_value(session, source_schema, form_key, interner, cache)
                })
                .and_then(duration_from_global),
        })
        .collect()
}

fn source_global_value(
    session: &mut PluginSession,
    source_schema: &crate::schema::AuthoringSchema,
    form_key: FormKey,
    interner: &StringInterner,
    cache: &mut HashMap<FormKey, Option<f32>>,
) -> Option<f32> {
    if let Some(value) = cache.get(&form_key) {
        return *value;
    }
    let value = session
        .source_record_decoded(&form_key, source_schema, interner)
        .ok()
        .and_then(|record| global_value(&record));
    cache.insert(form_key, value);
    value
}

fn source_form_key_from_value(
    value: &FieldValue,
    source_master_names: &[String],
    source_plugin_name: &str,
    interner: &StringInterner,
) -> Option<FormKey> {
    match value {
        FieldValue::FormKey(form_key) => Some(*form_key),
        FieldValue::Bytes(bytes) if bytes.len() >= 4 => {
            let raw = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
            let master_index = ((raw >> 24) & 0xFF) as usize;
            let plugin_name = source_master_names
                .get(master_index)
                .map(String::as_str)
                .unwrap_or(source_plugin_name);
            Some(FormKey {
                plugin: interner.intern(plugin_name),
                local: raw & 0x00FF_FFFF,
            })
        }
        FieldValue::Struct(fields) => fields.iter().find_map(|(_, nested)| {
            source_form_key_from_value(nested, source_master_names, source_plugin_name, interner)
        }),
        FieldValue::List(values) => values.iter().find_map(|nested| {
            source_form_key_from_value(nested, source_master_names, source_plugin_name, interner)
        }),
        _ => None,
    }
}

fn global_value(record: &Record) -> Option<f32> {
    if record.sig.as_str() != "GLOB" {
        return None;
    }
    record
        .fields
        .iter()
        .find(|field| field.sig.as_str() == "FLTV")
        .and_then(|field| numeric_f32(&field.value))
}

fn duration_from_global(value: f32) -> Option<u32> {
    value
        .is_finite()
        .then(|| value.round().clamp(0.0, u32::MAX as f32) as u32)
}

fn apply_effect_global_values(
    record: &mut Record,
    values: &[EffectGlobalValues],
    interner: &StringInterner,
) -> bool {
    if !matches!(record.sig.as_str(), "ALCH" | "ENCH" | "SPEL") {
        return false;
    }

    let mut changed = false;
    let mut next_effect_index = 0usize;
    let mut effect_index = None;
    for field in &mut record.fields {
        if field.sig.as_str() == "EFID" {
            effect_index = Some(next_effect_index);
            next_effect_index += 1;
            continue;
        }
        if field.sig.as_str() != "EFIT" {
            continue;
        }
        let Some(overrides) = effect_index.and_then(|index| values.get(index)) else {
            continue;
        };
        if overrides.magnitude.is_none() && overrides.duration.is_none() {
            continue;
        }
        let Some((magnitude, area, duration)) = effect_data(&field.value, interner) else {
            continue;
        };
        let magnitude = overrides.magnitude.unwrap_or(magnitude);
        let duration = overrides.duration.unwrap_or(duration);
        let mut bytes = [0u8; 12];
        bytes[0..4].copy_from_slice(&magnitude.to_le_bytes());
        bytes[4..8].copy_from_slice(&area.to_le_bytes());
        bytes[8..12].copy_from_slice(&duration.to_le_bytes());
        let value = FieldValue::Bytes(smallvec::SmallVec::from_slice(&bytes));
        if field.value != value {
            field.value = value;
            changed = true;
        }
    }
    changed
}

fn effect_data(value: &FieldValue, interner: &StringInterner) -> Option<(f32, u32, u32)> {
    match value {
        FieldValue::Bytes(bytes) if bytes.len() >= 12 => Some((
            f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]),
            u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]),
            u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]),
        )),
        FieldValue::Struct(fields) => {
            let mut magnitude = 0.0;
            let mut area = 0;
            let mut duration = 0;
            for (name, field_value) in fields {
                let Some(name) = interner.resolve(*name) else {
                    continue;
                };
                if name.eq_ignore_ascii_case("magnitude") {
                    magnitude = numeric_f32(field_value)?;
                } else if name.eq_ignore_ascii_case("area") {
                    area = numeric_u32(field_value)?;
                } else if name.eq_ignore_ascii_case("duration") {
                    duration = numeric_u32(field_value)?;
                }
            }
            Some((magnitude, area, duration))
        }
        _ => None,
    }
}

fn numeric_f32(value: &FieldValue) -> Option<f32> {
    match value {
        FieldValue::Float(value) => Some(*value),
        FieldValue::Uint(value) => Some(*value as f32),
        FieldValue::Int(value) => Some(*value as f32),
        FieldValue::Bytes(bytes) if bytes.len() >= 4 => {
            Some(f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
        }
        _ => None,
    }
}

fn numeric_u32(value: &FieldValue) -> Option<u32> {
    match value {
        FieldValue::Uint(value) => (*value).try_into().ok(),
        FieldValue::Int(value) => (*value).try_into().ok(),
        FieldValue::Float(value) => duration_from_global(*value),
        FieldValue::Bytes(bytes) if bytes.len() >= 4 => {
            Some(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use smallvec::{SmallVec, smallvec};

    use super::*;
    use crate::ids::SubrecordSig;
    use crate::record::{FieldEntry, RecordFlags};

    fn field(sig: &str, value: FieldValue) -> FieldEntry {
        FieldEntry {
            sig: SubrecordSig::from_str(sig).unwrap(),
            value,
        }
    }

    fn record(sig: &str, fields: Vec<FieldEntry>, interner: &StringInterner) -> Record {
        Record {
            sig: SigCode::from_str(sig).unwrap(),
            form_key: FormKey {
                plugin: interner.intern("SeventySix.esm"),
                local: 0x5EDEE5,
            },
            eid: Some(interner.intern("ATX_BuffAgility")),
            flags: RecordFlags::empty(),
            fields: SmallVec::from_vec(fields),
            warnings: SmallVec::new(),
        }
    }

    #[test]
    fn reads_fo76_magnitude_and_duration_global_refs_by_effect() {
        let interner = StringInterner::new();
        let source = record(
            "SPEL",
            vec![
                field("EFID", FieldValue::None),
                field("DURG", FieldValue::Bytes(smallvec![0x5E, 0x01, 0x65, 0x00])),
                field("MAGG", FieldValue::Bytes(smallvec![0x5F, 0x01, 0x65, 0x00])),
            ],
            &interner,
        );

        let refs = source_effect_global_refs(&source, &[], "SeventySix.esm", &interner);

        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].duration.unwrap().local, 0x65015E);
        assert_eq!(refs[0].magnitude.unwrap().local, 0x65015F);
    }

    #[test]
    fn writes_global_values_into_fo4_efit_bytes() {
        let interner = StringInterner::new();
        let magnitude = interner.intern("Magnitude");
        let area = interner.intern("Area");
        let duration = interner.intern("Duration");
        let mut target = record(
            "SPEL",
            vec![
                field("EFID", FieldValue::None),
                field(
                    "EFIT",
                    FieldValue::Struct(vec![
                        (magnitude, FieldValue::Float(0.0)),
                        (area, FieldValue::Uint(0)),
                        (duration, FieldValue::Uint(1800)),
                    ]),
                ),
            ],
            &interner,
        );

        assert!(apply_effect_global_values(
            &mut target,
            &[EffectGlobalValues {
                magnitude: Some(2.0),
                duration: Some(1800),
            }],
            &interner,
        ));

        let FieldValue::Bytes(bytes) = &target.fields[1].value else {
            panic!("EFIT must be serialized bytes");
        };
        assert_eq!(f32::from_le_bytes(bytes[0..4].try_into().unwrap()), 2.0);
        assert_eq!(u32::from_le_bytes(bytes[8..12].try_into().unwrap()), 1800);
        assert!(!apply_effect_global_values(
            &mut target,
            &[EffectGlobalValues {
                magnitude: Some(2.0),
                duration: Some(1800),
            }],
            &interner,
        ));
    }

    #[test]
    fn leaves_effects_without_global_overrides_unchanged() {
        let interner = StringInterner::new();
        let original = smallvec![0u8; 12];
        let mut target = record(
            "SPEL",
            vec![
                field("EFID", FieldValue::None),
                field("EFIT", FieldValue::Bytes(original.clone())),
            ],
            &interner,
        );

        assert!(!apply_effect_global_values(
            &mut target,
            &[EffectGlobalValues::default()],
            &interner,
        ));
        assert_eq!(target.fields[1].value, FieldValue::Bytes(original));
    }
}
