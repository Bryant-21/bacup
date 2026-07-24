use super::common::struct_value;
use crate::ids::SubrecordSig;
use crate::record::{FieldEntry, FieldValue, Record};
use crate::sym::StringInterner;

const ARMA_MODEL_SIGS: &[[u8; 4]] = &[
    *b"MODL", *b"MOD2", *b"MOD3", *b"MOD4", *b"MOD5", *b"MODT", *b"MO2T", *b"MO3T", *b"MO4T",
    *b"MO5T", *b"MODS", *b"MO2S", *b"MO3S", *b"MO4S", *b"MO5S", *b"MODD", *b"MOSD",
];

fn collect_named_values(
    value: &FieldValue,
    name: &str,
    interner: &crate::sym::StringInterner,
    output: &mut Vec<FieldValue>,
) {
    match value {
        FieldValue::Struct(fields) => {
            for (key, value) in fields {
                if interner.resolve(*key) == Some(name) {
                    output.push(value.clone());
                } else {
                    collect_named_values(value, name, interner, output);
                }
            }
        }
        FieldValue::List(items) => {
            for item in items {
                collect_named_values(item, name, interner, output);
            }
        }
        _ => {}
    }
}

pub(super) fn relayout_arma_models(record: &mut Record, interner: &crate::sym::StringInterner) {
    // FNV MODL/MOD3 are male/female actor-biped meshes; MOD2/MOD4 are
    // ground-object meshes. FO4 biped MOD2/MOD3 receive the actor meshes.
    // Legacy armor has no distinct first-person source, so MOD4/MOD5 stay absent.
    let mut male = Vec::new();
    let mut female = Vec::new();
    let mut output = Vec::with_capacity(record.fields.len());
    let mut insert_at = None;
    let mut dropped_legacy_materials = false;

    for entry in record.fields.drain(..) {
        if ARMA_MODEL_SIGS.contains(&entry.sig.0) {
            insert_at.get_or_insert(output.len());
            if matches!(entry.sig.0, sig if sig == *b"MODS" || sig == *b"MO2S" || sig == *b"MO3S" || sig == *b"MO4S" || sig == *b"MO5S")
            {
                dropped_legacy_materials = true;
            }
            match entry.sig.0 {
                sig if sig == *b"MODL" => match &entry.value {
                    FieldValue::List(_) | FieldValue::Struct(_) => {
                        collect_named_values(&entry.value, "MODL", interner, &mut male);
                        collect_named_values(&entry.value, "MOD3", interner, &mut female);
                    }
                    _ => male.push(entry.value.clone()),
                },
                sig if sig == *b"MOD3" => match &entry.value {
                    FieldValue::List(_) | FieldValue::Struct(_) => {
                        collect_named_values(&entry.value, "MOD3", interner, &mut female);
                    }
                    _ => female.push(entry.value.clone()),
                },
                _ => {}
            }
        } else {
            output.push(entry);
        }
    }

    let Some(insert_at) = insert_at else {
        record.fields = output.into_iter().collect();
        return;
    };
    let mut replacements = Vec::with_capacity(male.len() + female.len());
    for value in male {
        replacements.push(FieldEntry {
            sig: SubrecordSig(*b"MOD2"),
            value,
        });
    }
    for value in female {
        replacements.push(FieldEntry {
            sig: SubrecordSig(*b"MOD3"),
            value,
        });
    }
    output.splice(insert_at..insert_at, replacements);
    record.fields = output.into_iter().collect();
    if dropped_legacy_materials {
        let warning = interner.intern("legacy_armor_alternate_textures_require_mswp_synthesis");
        if !record.warnings.contains(&warning) {
            record.warnings.push(warning);
        }
    }
}

#[derive(Clone, Copy)]
pub(super) enum LegacyArmorSource {
    Fnv,
    Fo3,
}

pub(super) fn relayout_armo_loader_fields(
    record: &mut Record,
    source: LegacyArmorSource,
    interner: &crate::sym::StringInterner,
) {
    let dnam = record
        .fields
        .iter()
        .find(|entry| entry.sig.0 == *b"DNAM")
        .map(|entry| entry.value.clone());
    let had_legacy_etyp = record.fields.iter().any(|entry| entry.sig.0 == *b"ETYP");
    let dropped_materials = record.fields.iter().any(|entry| {
        matches!(entry.sig.0, sig if sig == *b"MODS" || sig == *b"MO2S" || sig == *b"MO3S" || sig == *b"MO4S" || sig == *b"MO5S")
    });

    record.fields.retain(|entry| {
        !matches!(entry.sig.0, sig if sig == *b"DNAM" || sig == *b"ETYP" || sig == *b"MODL" || sig == *b"MODS" || sig == *b"MO2S" || sig == *b"MO3S" || sig == *b"MO4S" || sig == *b"MO5S")
    });

    if let Some(dnam) = dnam {
        if let Some(armor_rating) = legacy_armo_rating(&dnam, interner)
            && armor_rating > 0
        {
            let mut fnam = vec![0_u8; 8];
            fnam[0..2].copy_from_slice(&armor_rating.to_le_bytes());
            record.fields.push(FieldEntry {
                sig: SubrecordSig(*b"FNAM"),
                value: FieldValue::Bytes(smallvec::SmallVec::from_vec(fnam)),
            });
        }
        if matches!(source, LegacyArmorSource::Fnv)
            && let Some(dt) = legacy_armo_dt(&dnam, interner)
            && dt.is_finite()
            && dt > 0.0
        {
            let amount = dt.round().clamp(0.0, u32::MAX as f32) as u32;
            record.fields.push(FieldEntry {
                sig: SubrecordSig(*b"DAMA"),
                value: FieldValue::List(vec![FieldValue::Struct(vec![
                    (
                        interner.intern("resistances_type"),
                        FieldValue::FormKey(crate::ids::FormKey {
                            local: 0x0006_0A87,
                            plugin: interner.intern("Fallout4.esm"),
                        }),
                    ),
                    (
                        interner.intern("resistances_amount"),
                        FieldValue::Uint(u64::from(amount)),
                    ),
                ])]),
            });
        }
    }

    if had_legacy_etyp {
        push_armor_warning(record, interner, "legacy_armor_numeric_etyp_dropped");
    }
    if dropped_materials {
        push_armor_warning(
            record,
            interner,
            "legacy_armor_alternate_textures_require_mswp_synthesis",
        );
    }
}

fn legacy_armo_rating(value: &FieldValue, interner: &crate::sym::StringInterner) -> Option<u16> {
    let rating = match value {
        FieldValue::Bytes(bytes) if bytes.len() >= 2 => {
            i16::from_le_bytes(bytes[0..2].try_into().ok()?) as i32
        }
        _ => first_named_number(value, &["dr", "ar"], interner)? as i32,
    };
    Some(rating.clamp(0, i32::from(u16::MAX)) as u16)
}

fn legacy_armo_dt(value: &FieldValue, interner: &crate::sym::StringInterner) -> Option<f32> {
    match value {
        FieldValue::Bytes(bytes) if bytes.len() >= 8 => {
            Some(f32::from_le_bytes(bytes[4..8].try_into().ok()?))
        }
        _ => first_named_number(value, &["dt"], interner).map(|value| value as f32),
    }
}

fn first_named_number(
    value: &FieldValue,
    names: &[&str],
    interner: &crate::sym::StringInterner,
) -> Option<f64> {
    match value {
        FieldValue::Struct(fields) => fields.iter().find_map(|(key, value)| {
            if interner
                .resolve(*key)
                .is_some_and(|name| names.contains(&name))
            {
                field_number(value)
            } else {
                first_named_number(value, names, interner)
            }
        }),
        FieldValue::List(items) => items
            .iter()
            .find_map(|value| first_named_number(value, names, interner)),
        _ => None,
    }
}

fn field_number(value: &FieldValue) -> Option<f64> {
    match value {
        FieldValue::Float(value) => Some(f64::from(*value)),
        FieldValue::Int(value) => Some(*value as f64),
        FieldValue::Uint(value) => Some(*value as f64),
        _ => None,
    }
}

fn push_armor_warning(record: &mut Record, interner: &crate::sym::StringInterner, warning: &str) {
    let warning = interner.intern(warning);
    if !record.warnings.contains(&warning) {
        record.warnings.push(warning);
    }
}
