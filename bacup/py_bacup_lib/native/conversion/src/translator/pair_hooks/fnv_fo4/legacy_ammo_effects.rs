use crate::record::{FieldValue, Record};
use crate::sym::StringInterner;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum LegacyAmmoEffectChannel {
    Damage,
    Spread,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum LegacyAmmoEffectOperation {
    Add,
    Multiply,
    Subtract,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct LegacyAmmoEffectFold {
    pub channel: LegacyAmmoEffectChannel,
    pub operation: LegacyAmmoEffectOperation,
    pub value_bits: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum LegacyAmmoEffectSupport {
    Foldable(LegacyAmmoEffectFold),
    Blocked(Vec<String>),
}

pub(crate) fn classify_legacy_ammo_effect(
    record: &Record,
    interner: &StringInterner,
) -> LegacyAmmoEffectSupport {
    let mut reasons = Vec::new();
    if record.sig.as_str() != "AMEF" {
        reasons.push("legacy_ammo_effect_wrong_signature".to_string());
    }
    if record
        .fields
        .iter()
        .any(|field| !matches!(field.sig.0, sig if sig == *b"EDID" || sig == *b"FULL" || sig == *b"DATA"))
    {
        reasons.push("legacy_ammo_effect_unsupported_runtime_field".to_string());
    }
    let mut data = record
        .fields
        .iter()
        .filter(|field| field.sig.as_str() == "DATA");
    let parsed = data
        .next()
        .and_then(|field| parse_data(&field.value, interner));
    if data.next().is_some() {
        reasons.push("legacy_ammo_effect_field_multiplicity_data".to_string());
    }
    let Some((effect_type, operation, value)) = parsed else {
        reasons.push("legacy_ammo_effect_invalid_data".to_string());
        reasons.sort();
        reasons.dedup();
        return LegacyAmmoEffectSupport::Blocked(reasons);
    };
    if !value.is_finite() {
        reasons.push("legacy_ammo_effect_nonfinite_value".to_string());
    }
    let operation = match operation {
        0 => Some(LegacyAmmoEffectOperation::Add),
        1 => Some(LegacyAmmoEffectOperation::Multiply),
        2 => Some(LegacyAmmoEffectOperation::Subtract),
        _ => {
            reasons.push("legacy_ammo_effect_unknown_operation".to_string());
            None
        }
    };
    let channel = match effect_type {
        0 => Some(LegacyAmmoEffectChannel::Damage),
        1 => {
            reasons.push(
                "legacy_ammo_effect_damage_resistance_has_no_exact_fo4_ammo_semantics".to_string(),
            );
            None
        }
        2 => {
            reasons.push(
                "legacy_ammo_effect_damage_threshold_is_flat_not_fo4_armor_penetration".to_string(),
            );
            None
        }
        3 => Some(LegacyAmmoEffectChannel::Spread),
        4 => {
            reasons
                .push("legacy_ammo_effect_weapon_condition_has_no_fo4_runtime_system".to_string());
            None
        }
        5 => {
            reasons.push("legacy_ammo_effect_fatigue_is_not_fo4_action_points".to_string());
            None
        }
        _ => {
            reasons.push("legacy_ammo_effect_unknown_type".to_string());
            None
        }
    };
    if reasons.is_empty() {
        LegacyAmmoEffectSupport::Foldable(LegacyAmmoEffectFold {
            channel: channel.expect("validated channel"),
            operation: operation.expect("validated operation"),
            value_bits: value.to_bits(),
        })
    } else {
        reasons.sort();
        reasons.dedup();
        LegacyAmmoEffectSupport::Blocked(reasons)
    }
}

pub(crate) fn apply_legacy_ammo_effect_fold(input: f32, fold: LegacyAmmoEffectFold) -> Option<f32> {
    let value = f32::from_bits(fold.value_bits);
    let output = match fold.operation {
        LegacyAmmoEffectOperation::Add => input + value,
        LegacyAmmoEffectOperation::Multiply => input * value,
        LegacyAmmoEffectOperation::Subtract => input - value,
    };
    (output.is_finite() && output >= 0.0).then_some(output)
}

fn parse_data(value: &FieldValue, interner: &StringInterner) -> Option<(u32, u32, f32)> {
    match value {
        FieldValue::Bytes(bytes) if bytes.len() == 12 => Some((
            u32::from_le_bytes(bytes[0..4].try_into().ok()?),
            u32::from_le_bytes(bytes[4..8].try_into().ok()?),
            f32::from_le_bytes(bytes[8..12].try_into().ok()?),
        )),
        FieldValue::Struct(fields) => Some((
            named_u32(fields, "type", interner)?,
            named_u32(fields, "operation", interner)?,
            named_f32(fields, "value", interner)?,
        )),
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

#[cfg(test)]
mod tests {
    use crate::ids::{FormKey, SigCode, SubrecordSig};
    use crate::record::FieldEntry;

    use super::*;

    fn effect(effect_type: u32, operation: u32, value: f32, interner: &StringInterner) -> Record {
        let mut record = Record::new(
            SigCode::from_str("AMEF").unwrap(),
            FormKey {
                local: 0x100,
                plugin: interner.intern("FalloutNV.esm"),
            },
        );
        record.fields.push(FieldEntry {
            sig: SubrecordSig(*b"DATA"),
            value: FieldValue::Struct(vec![
                (
                    interner.intern("type"),
                    FieldValue::Uint(u64::from(effect_type)),
                ),
                (
                    interner.intern("operation"),
                    FieldValue::Uint(u64::from(operation)),
                ),
                (interner.intern("value"), FieldValue::Float(value)),
            ]),
        });
        record
    }

    #[test]
    fn damage_and_spread_operations_are_exact_numeric_folds() {
        let interner = StringInterner::new();
        for (effect_type, channel) in [
            (0, LegacyAmmoEffectChannel::Damage),
            (3, LegacyAmmoEffectChannel::Spread),
        ] {
            let LegacyAmmoEffectSupport::Foldable(fold) =
                classify_legacy_ammo_effect(&effect(effect_type, 1, 1.25, &interner), &interner)
            else {
                panic!("effect must be foldable");
            };
            assert_eq!(fold.channel, channel);
            assert_eq!(apply_legacy_ammo_effect_fold(8.0, fold), Some(10.0));
        }
    }

    #[test]
    fn non_equivalent_target_mechanics_remain_typed_blockers() {
        let interner = StringInterner::new();
        for (effect_type, reason) in [
            (
                1,
                "legacy_ammo_effect_damage_resistance_has_no_exact_fo4_ammo_semantics",
            ),
            (
                2,
                "legacy_ammo_effect_damage_threshold_is_flat_not_fo4_armor_penetration",
            ),
            (
                4,
                "legacy_ammo_effect_weapon_condition_has_no_fo4_runtime_system",
            ),
            (5, "legacy_ammo_effect_fatigue_is_not_fo4_action_points"),
        ] {
            let LegacyAmmoEffectSupport::Blocked(reasons) =
                classify_legacy_ammo_effect(&effect(effect_type, 1, 1.0, &interner), &interner)
            else {
                panic!("effect must remain blocked");
            };
            assert_eq!(reasons, [reason]);
        }
    }
}
