use std::collections::BTreeSet;

use crate::record::{FieldValue, Record};
use crate::source_rig::{
    SourcePoweredRagdollEvidence, SourcePoweredRagdollFeedbackEvidence,
    SourcePoweredRagdollPoseEvidence,
};
use crate::sym::{StringInterner, Sym};

const LEGACY_RGDL_DATA_LEN: usize = 14;
const LEGACY_RGDL_DATA_LEN_WITH_TRAILING_BYTE: usize = 15;
const LEGACY_RGDL_RAFD_LEN: usize = 60;
const LEGACY_RGDL_RAPS_LEN: usize = 24;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LegacyRagdollSupport {
    pub source: Option<SourcePoweredRagdollEvidence>,
    pub reason_codes: Vec<String>,
}

impl LegacyRagdollSupport {
    pub(crate) fn is_ready(&self) -> bool {
        self.reason_codes.is_empty()
    }
}

pub(crate) fn classify_legacy_ragdoll(
    record: &Record,
    interner: &StringInterner,
) -> LegacyRagdollSupport {
    let source = match decode_legacy_powered_ragdoll_evidence(record, interner) {
        Ok(source) => source,
        Err(reason_code) => {
            return LegacyRagdollSupport {
                source: None,
                reason_codes: vec![reason_code],
            };
        }
    };
    let mut reasons = BTreeSet::new();
    if [
        source.unknown_byte_2,
        source.unknown_byte_3,
        source.unknown_byte_4,
        source.unknown_byte_5,
        source.unknown_byte_11,
    ]
    .into_iter()
    .any(|value| value != 0)
        || source.unknown_trailing_u8.is_some_and(|value| value != 0)
    {
        reasons.insert("legacy_rgdl_unknown_data_controls_unmapped".to_string());
    }
    if source.enabled_foot_ik || source.enabled_look_ik || source.enabled_grab_ik {
        reasons.insert("legacy_rgdl_legacy_ik_controls_unmapped".to_string());
    }
    if source.enabled_feedback {
        reasons.insert("legacy_rgdl_feedback_motor_controls_unmapped".to_string());
    }
    if source.enabled_pose_matching {
        reasons.insert("legacy_rgdl_pose_motor_controls_unmapped".to_string());
    }
    if source.death_pose_animation.is_some() {
        reasons.insert("legacy_rgdl_death_pose_animation_unmapped".to_string());
    }
    LegacyRagdollSupport {
        source: Some(source),
        reason_codes: reasons.into_iter().collect(),
    }
}

pub(crate) fn decode_legacy_powered_ragdoll_evidence(
    record: &Record,
    interner: &StringInterner,
) -> Result<SourcePoweredRagdollEvidence, String> {
    if record.sig.as_str() != "RGDL" {
        return Err("legacy_rgdl_wrong_signature".to_string());
    }
    let mut data = None;
    let mut feedback = None;
    let mut feedback_bones = None;
    let mut pose_matching = None;
    let mut death_pose_animation = None;
    let mut counts = [0usize; 8];
    for field in &record.fields {
        match field.sig.as_str() {
            "EDID" if matches!(field.value, FieldValue::String(_)) => counts[0] += 1,
            "NVER" if scalar_u32(&field.value).is_some() => counts[1] += 1,
            "DATA" => {
                counts[2] += 1;
                data = Some(decode_data(&field.value, interner)?);
            }
            "XNAM" if matches!(field.value, FieldValue::FormKey(_)) => counts[3] += 1,
            "TNAM" if matches!(field.value, FieldValue::FormKey(_)) => counts[4] += 1,
            "RAFD" => {
                counts[5] += 1;
                feedback = Some(decode_feedback(&field.value, interner)?);
            }
            "RAFB" => {
                counts[6] += 1;
                feedback_bones = Some(decode_bones(&field.value, interner)?);
            }
            "RAPS" => {
                counts[7] += 1;
                pose_matching = Some(decode_pose(&field.value, interner)?);
            }
            "ANAM" => {
                let FieldValue::String(path) = &field.value else {
                    return Err("legacy_rgdl_death_pose_shape_unverified".to_string());
                };
                if death_pose_animation.is_some() {
                    return Err("legacy_rgdl_death_pose_multiplicity_unverified".to_string());
                }
                death_pose_animation = interner.resolve(*path).map(str::to_string);
                if death_pose_animation.is_none() {
                    return Err("legacy_rgdl_death_pose_shape_unverified".to_string());
                }
            }
            _ => return Err("legacy_rgdl_subrecord_shape_unverified".to_string()),
        }
    }
    if counts[2] != 1 || counts.iter().any(|count| *count > 1) {
        return Err("legacy_rgdl_subrecord_multiplicity_unverified".to_string());
    }
    let (dynamic_bone_count, bytes) =
        data.ok_or_else(|| "legacy_rgdl_subrecord_multiplicity_unverified".to_string())?;
    let enabled_feedback = bool_byte(bytes[4])?;
    let enabled_foot_ik = bool_byte(bytes[5])?;
    let enabled_look_ik = bool_byte(bytes[6])?;
    let enabled_grab_ik = bool_byte(bytes[7])?;
    let enabled_pose_matching = bool_byte(bytes[8])?;
    let feedback = match (feedback, feedback_bones) {
        (Some(mut feedback), bones) => {
            feedback.bones = bones.unwrap_or_default();
            Some(feedback)
        }
        (None, None) => None,
        (None, Some(_)) => {
            return Err("legacy_rgdl_feedback_subrecord_pair_unverified".to_string());
        }
    };
    if feedback
        .as_ref()
        .map_or(dynamic_bone_count != 0, |feedback| {
            feedback.bones.len() != dynamic_bone_count as usize
        })
    {
        return Err("legacy_rgdl_dynamic_bone_count_mismatch".to_string());
    }
    if enabled_feedback && feedback.is_none() {
        return Err("legacy_rgdl_feedback_enablement_mismatch".to_string());
    }
    if enabled_pose_matching && pose_matching.is_none() {
        return Err("legacy_rgdl_pose_enablement_mismatch".to_string());
    }
    Ok(SourcePoweredRagdollEvidence {
        dynamic_bone_count,
        unknown_byte_2: bytes[0],
        unknown_byte_3: bytes[1],
        unknown_byte_4: bytes[2],
        unknown_byte_5: bytes[3],
        enabled_feedback,
        enabled_foot_ik,
        enabled_look_ik,
        enabled_grab_ik,
        enabled_pose_matching,
        unknown_byte_11: bytes[9],
        unknown_trailing_u8: bytes.get(10).copied(),
        feedback,
        pose_matching,
        death_pose_animation,
    })
}

fn decode_data(value: &FieldValue, interner: &StringInterner) -> Result<(u32, Vec<u8>), String> {
    match value {
        FieldValue::Bytes(bytes)
            if matches!(
                bytes.len(),
                LEGACY_RGDL_DATA_LEN | LEGACY_RGDL_DATA_LEN_WITH_TRAILING_BYTE
            ) =>
        {
            Ok((
                u32::from_le_bytes(bytes[..4].try_into().unwrap()),
                bytes[4..].to_vec(),
            ))
        }
        FieldValue::Struct(fields) => {
            let count = named_u32(fields, "dynamic_bone_count", interner)
                .ok_or_else(|| "legacy_rgdl_data_shape_unverified".to_string())?;
            let mut bytes = Vec::with_capacity(11);
            for name in [
                "unknown_u8_1",
                "unknown_u8_2",
                "unknown_u8_3",
                "unknown_u8_4",
                "enabled_feedback",
                "enabled_foot_ik_broken_don_t_use",
                "enabled_look_ik_broken_don_t_use",
                "enabled_grab_ik_broken_don_t_use",
                "enabled_pose_matching",
                "unknown_u8_10",
            ] {
                bytes.push(
                    named_u8(fields, name, interner)
                        .ok_or_else(|| "legacy_rgdl_data_shape_unverified".to_string())?,
                );
            }
            if let Some(value) = named_u8(fields, "unknown_trailing_u8", interner) {
                bytes.push(value);
            }
            Ok((count, bytes))
        }
        _ => Err("legacy_rgdl_data_shape_unverified".to_string()),
    }
}

fn decode_feedback(
    value: &FieldValue,
    interner: &StringInterner,
) -> Result<SourcePoweredRagdollFeedbackEvidence, String> {
    let mut bits = [0u32; 13];
    let mut integers = [0i32; 2];
    match value {
        FieldValue::Bytes(bytes) if bytes.len() == LEGACY_RGDL_RAFD_LEN => {
            for (index, chunk) in bytes[..52].chunks_exact(4).enumerate() {
                bits[index] = u32::from_le_bytes(chunk.try_into().unwrap());
                if !f32::from_bits(bits[index]).is_finite() {
                    return Err("legacy_rgdl_feedback_nonfinite".to_string());
                }
            }
            integers[0] = i32::from_le_bytes(bytes[52..56].try_into().unwrap());
            integers[1] = i32::from_le_bytes(bytes[56..60].try_into().unwrap());
        }
        FieldValue::Struct(fields) => {
            for (index, name) in [
                "dynamic_keyframe_blend_amount",
                "hierarchy_gain",
                "position_gain",
                "velocity_gain",
                "acceleration_gain",
                "snap_gain",
                "velocity_damping",
                "snap_max_settings_linear_velocity",
                "snap_max_settings_angular_velocity",
                "snap_max_settings_linear_distance",
                "snap_max_settings_angular_distance",
                "position_max_velocity_linear",
                "position_max_velocity_angular",
            ]
            .into_iter()
            .enumerate()
            {
                let value = named_f32(fields, name, interner)
                    .ok_or_else(|| "legacy_rgdl_feedback_shape_unverified".to_string())?;
                if !value.is_finite() {
                    return Err("legacy_rgdl_feedback_nonfinite".to_string());
                }
                bits[index] = value.to_bits();
            }
            integers[0] = named_i32(fields, "position_max_velocity_projectile", interner)
                .ok_or_else(|| "legacy_rgdl_feedback_shape_unverified".to_string())?;
            integers[1] = named_i32(fields, "position_max_velocity_melee", interner)
                .ok_or_else(|| "legacy_rgdl_feedback_shape_unverified".to_string())?;
        }
        _ => return Err("legacy_rgdl_feedback_shape_unverified".to_string()),
    }
    Ok(SourcePoweredRagdollFeedbackEvidence {
        dynamic_keyframe_blend_amount_bits: bits[0],
        hierarchy_gain_bits: bits[1],
        position_gain_bits: bits[2],
        velocity_gain_bits: bits[3],
        acceleration_gain_bits: bits[4],
        snap_gain_bits: bits[5],
        velocity_damping_bits: bits[6],
        snap_max_linear_velocity_bits: bits[7],
        snap_max_angular_velocity_bits: bits[8],
        snap_max_linear_distance_bits: bits[9],
        snap_max_angular_distance_bits: bits[10],
        position_max_velocity_linear_bits: bits[11],
        position_max_velocity_angular_bits: bits[12],
        position_max_velocity_projectile: integers[0],
        position_max_velocity_melee: integers[1],
        bones: Vec::new(),
    })
}

fn decode_bones(value: &FieldValue, interner: &StringInterner) -> Result<Vec<u16>, String> {
    match value {
        FieldValue::Bytes(bytes) if bytes.len() % 2 == 0 => Ok(bytes
            .chunks_exact(2)
            .map(|chunk| u16::from_le_bytes(chunk.try_into().unwrap()))
            .collect()),
        FieldValue::List(values) => values
            .iter()
            .map(|value| scalar_or_nested_u16(value, interner))
            .collect::<Option<Vec<_>>>()
            .ok_or_else(|| "legacy_rgdl_feedback_bones_shape_unverified".to_string()),
        _ => Err("legacy_rgdl_feedback_bones_shape_unverified".to_string()),
    }
}

fn decode_pose(
    value: &FieldValue,
    interner: &StringInterner,
) -> Result<SourcePoweredRagdollPoseEvidence, String> {
    match value {
        FieldValue::Bytes(bytes) if bytes.len() == LEGACY_RGDL_RAPS_LEN => {
            let matching_bones = [0usize, 2, 4].map(|offset| {
                let bone = u16::from_le_bytes(bytes[offset..offset + 2].try_into().unwrap());
                (bone != u16::MAX).then_some(bone)
            });
            let scalar = |offset: usize| {
                let bits = u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap());
                f32::from_bits(bits).is_finite().then_some(bits)
            };
            Ok(SourcePoweredRagdollPoseEvidence {
                matching_bones,
                flags: bytes[6],
                unknown: bytes[7],
                motors_strength_bits: scalar(8)
                    .ok_or_else(|| "legacy_rgdl_pose_nonfinite".to_string())?,
                pose_activation_delay_time_bits: scalar(12)
                    .ok_or_else(|| "legacy_rgdl_pose_nonfinite".to_string())?,
                match_error_allowance_bits: scalar(16)
                    .ok_or_else(|| "legacy_rgdl_pose_nonfinite".to_string())?,
                displacement_to_disable_bits: scalar(20)
                    .ok_or_else(|| "legacy_rgdl_pose_nonfinite".to_string())?,
            })
        }
        FieldValue::Struct(fields) => {
            let bones = named_value(fields, "match_bones", interner)
                .and_then(|value| u16_values(value, interner))
                .ok_or_else(|| "legacy_rgdl_pose_shape_unverified".to_string())?;
            let [bone_0, bone_1, bone_2] = bones.as_slice() else {
                return Err("legacy_rgdl_pose_bone_count_unverified".to_string());
            };
            let matching_bones =
                [*bone_0, *bone_1, *bone_2].map(|bone| (bone != u16::MAX).then_some(bone));
            let float_bits = |name: &str| {
                named_f32(fields, name, interner)
                    .filter(|value| value.is_finite())
                    .map(f32::to_bits)
                    .ok_or_else(|| "legacy_rgdl_pose_shape_unverified".to_string())
            };
            Ok(SourcePoweredRagdollPoseEvidence {
                matching_bones,
                flags: named_u8(fields, "flags", interner)
                    .or_else(|| named_u8(fields, "disable_on_move", interner))
                    .ok_or_else(|| "legacy_rgdl_pose_shape_unverified".to_string())?,
                unknown: named_u8(fields, "unknown_u8_2", interner)
                    .ok_or_else(|| "legacy_rgdl_pose_shape_unverified".to_string())?,
                motors_strength_bits: float_bits("motors_strength")?,
                pose_activation_delay_time_bits: float_bits("pose_activation_delay_time")?,
                match_error_allowance_bits: float_bits("match_error_allowance")?,
                displacement_to_disable_bits: float_bits("displacement_to_disable")?,
            })
        }
        _ => Err("legacy_rgdl_pose_shape_unverified".to_string()),
    }
}

fn bool_byte(value: u8) -> Result<bool, String> {
    match value {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err("legacy_rgdl_boolean_shape_unverified".to_string()),
    }
}

fn named_value<'a>(
    fields: &'a [(Sym, FieldValue)],
    name: &str,
    interner: &StringInterner,
) -> Option<&'a FieldValue> {
    fields
        .iter()
        .find(|(key, _)| interner.resolve(*key) == Some(name))
        .map(|(_, value)| value)
}

fn named_u8(fields: &[(Sym, FieldValue)], name: &str, interner: &StringInterner) -> Option<u8> {
    u8::try_from(scalar_u64(named_value(fields, name, interner)?)?).ok()
}

fn named_u32(fields: &[(Sym, FieldValue)], name: &str, interner: &StringInterner) -> Option<u32> {
    u32::try_from(scalar_u64(named_value(fields, name, interner)?)?).ok()
}

fn named_i32(fields: &[(Sym, FieldValue)], name: &str, interner: &StringInterner) -> Option<i32> {
    match named_value(fields, name, interner)? {
        FieldValue::Int(value) => i32::try_from(*value).ok(),
        FieldValue::Uint(value) => i32::try_from(*value).ok(),
        FieldValue::Bytes(bytes) if bytes.len() == 4 => {
            Some(i32::from_le_bytes(bytes.as_slice().try_into().ok()?))
        }
        _ => None,
    }
}

fn named_f32(fields: &[(Sym, FieldValue)], name: &str, interner: &StringInterner) -> Option<f32> {
    match named_value(fields, name, interner)? {
        FieldValue::Float(value) => Some(*value),
        FieldValue::Bytes(bytes) if bytes.len() == 4 => {
            Some(f32::from_le_bytes(bytes.as_slice().try_into().ok()?))
        }
        _ => None,
    }
}

fn scalar_u32(value: &FieldValue) -> Option<u32> {
    u32::try_from(scalar_u64(value)?).ok()
}

fn scalar_u64(value: &FieldValue) -> Option<u64> {
    match value {
        FieldValue::Uint(value) => Some(*value),
        FieldValue::Int(value) => u64::try_from(*value).ok(),
        FieldValue::Bool(value) => Some(u64::from(*value)),
        FieldValue::Bytes(bytes) if matches!(bytes.len(), 1 | 2 | 4 | 8) => {
            let mut buffer = [0u8; 8];
            buffer[..bytes.len()].copy_from_slice(bytes);
            Some(u64::from_le_bytes(buffer))
        }
        _ => None,
    }
}

fn scalar_or_nested_u16(value: &FieldValue, interner: &StringInterner) -> Option<u16> {
    match value {
        FieldValue::Struct(fields) if fields.len() == 1 => {
            scalar_or_nested_u16(&fields[0].1, interner)
        }
        _ => u16::try_from(scalar_u64(value)?).ok(),
    }
}

fn u16_values(value: &FieldValue, interner: &StringInterner) -> Option<Vec<u16>> {
    match value {
        FieldValue::List(values) => values
            .iter()
            .map(|value| scalar_or_nested_u16(value, interner))
            .collect(),
        FieldValue::Bytes(bytes) if bytes.len() % 2 == 0 => Some(
            bytes
                .chunks_exact(2)
                .map(|chunk| u16::from_le_bytes(chunk.try_into().unwrap()))
                .collect(),
        ),
        _ => scalar_or_nested_u16(value, interner).map(|value| vec![value]),
    }
}

#[cfg(test)]
mod tests {
    use smallvec::smallvec;

    use crate::ids::{FormKey, SigCode, SubrecordSig};
    use crate::record::FieldEntry;

    use super::*;

    fn raw_field(signature: &[u8; 4], bytes: &[u8]) -> FieldEntry {
        FieldEntry {
            sig: SubrecordSig(*signature),
            value: FieldValue::Bytes(bytes.into()),
        }
    }

    fn dog_ragdoll(interner: &StringInterner) -> Record {
        let mut record = Record::new(
            SigCode::from_str("RGDL").unwrap(),
            FormKey {
                local: 0x0658d0,
                plugin: interner.intern("FalloutNV.esm"),
            },
        );
        record.fields = smallvec![
            FieldEntry {
                sig: SubrecordSig(*b"EDID"),
                value: FieldValue::String(interner.intern("DogRagdoll")),
            },
            raw_field(b"NVER", &1u32.to_le_bytes()),
            raw_field(b"DATA", &hex_bytes("0700000000000000010000000131")),
            FieldEntry {
                sig: SubrecordSig(*b"XNAM"),
                value: FieldValue::FormKey(FormKey {
                    local: 0x01cf9c,
                    plugin: interner.intern("FalloutNV.esm"),
                }),
            },
            FieldEntry {
                sig: SubrecordSig(*b"TNAM"),
                value: FieldValue::FormKey(FormKey {
                    local: 0x02931e,
                    plugin: interner.intern("FalloutNV.esm"),
                }),
            },
            raw_field(
                b"RAFD",
                &hex_bytes(
                    "CDCC4C3F7B142E3EAE47613E9A99193F0000803FCDCCCC3D00000000000040409A99993E9A99993E0000404000006041000090418813000010270000"
                ),
            ),
            raw_field(b"RAFB", &hex_bytes("050009000A000C00100012000100")),
            raw_field(
                b"RAPS",
                &hex_bytes("000001001200013F0040674500000040295C8F3E00000040"),
            ),
            FieldEntry {
                sig: SubrecordSig(*b"ANAM"),
                value: FieldValue::String(
                    interner.intern("Creatures\\Dog\\IdleAnims\\DeathPose.psa"),
                ),
            },
        ];
        record
    }

    fn hex_bytes(hex: &str) -> Vec<u8> {
        hex.as_bytes()
            .chunks_exact(2)
            .map(|digits| u8::from_str_radix(std::str::from_utf8(digits).unwrap(), 16).unwrap())
            .collect()
    }

    #[test]
    fn decodes_official_dog_rgdl_bit_exact_with_ordered_nullable_bones() {
        let interner = StringInterner::new();
        let source =
            decode_legacy_powered_ragdoll_evidence(&dog_ragdoll(&interner), &interner).unwrap();
        assert_eq!(source.dynamic_bone_count, 7);
        assert!(source.enabled_feedback);
        assert!(source.enabled_pose_matching);
        assert_eq!(source.unknown_byte_11, 0x31);
        assert_eq!(source.unknown_trailing_u8, None);
        assert_eq!(
            source.feedback.as_ref().unwrap().bones,
            [5, 9, 10, 12, 16, 18, 1]
        );
        assert_eq!(
            source.pose_matching.as_ref().unwrap().matching_bones,
            [Some(0), Some(1), Some(18)]
        );
        assert_eq!(
            source.pose_matching.as_ref().unwrap().motors_strength_bits,
            3700.0_f32.to_bits()
        );
        assert_eq!(
            source.death_pose_animation.as_deref(),
            Some("Creatures\\Dog\\IdleAnims\\DeathPose.psa")
        );
    }

    #[test]
    fn reports_only_unbound_runtime_semantics_after_source_evidence_decodes() {
        let interner = StringInterner::new();
        let support = classify_legacy_ragdoll(&dog_ragdoll(&interner), &interner);
        assert!(support.source.is_some());
        assert_eq!(
            support.reason_codes,
            [
                "legacy_rgdl_death_pose_animation_unmapped",
                "legacy_rgdl_feedback_motor_controls_unmapped",
                "legacy_rgdl_pose_motor_controls_unmapped",
                "legacy_rgdl_unknown_data_controls_unmapped",
            ]
        );
    }

    #[test]
    fn mismatched_dynamic_bone_count_is_not_silently_accepted() {
        let interner = StringInterner::new();
        let mut record = dog_ragdoll(&interner);
        let FieldValue::Bytes(data) = &mut record
            .fields
            .iter_mut()
            .find(|field| field.sig.as_str() == "DATA")
            .unwrap()
            .value
        else {
            unreachable!()
        };
        data[..4].copy_from_slice(&6u32.to_le_bytes());
        assert_eq!(
            decode_legacy_powered_ragdoll_evidence(&record, &interner),
            Err("legacy_rgdl_dynamic_bone_count_mismatch".to_string())
        );
    }

    #[test]
    fn sparse_disabled_feedback_override_preserves_controls_without_requiring_bones() {
        let interner = StringInterner::new();
        let mut record = Record::new(
            SigCode::from_str("RGDL").unwrap(),
            FormKey {
                local: 0x001ab6,
                plugin: interner.intern("DeadMoney.esm"),
            },
        );
        record.fields = smallvec![
            raw_field(b"NVER", &1u32.to_le_bytes()),
            raw_field(b"DATA", &hex_bytes("000000000000000000000000009B")),
            FieldEntry {
                sig: SubrecordSig(*b"TNAM"),
                value: FieldValue::FormKey(FormKey {
                    local: 0x02931e,
                    plugin: interner.intern("FalloutNV.esm"),
                }),
            },
            raw_field(
                b"RAFD",
                &hex_bytes(
                    "CDCC4C3F7B142E3EAE47613E9A99193F0000803FCDCCCC3D00000000000040409A99993E9A99993E0000404000006041000090418813000010270000"
                ),
            ),
            raw_field(
                b"RAPS",
                &hex_bytes("FFFFFFFFFFFF000000000000000000000000000000000000"),
            ),
        ];

        let source = decode_legacy_powered_ragdoll_evidence(&record, &interner).unwrap();
        assert_eq!(source.dynamic_bone_count, 0);
        assert!(!source.enabled_feedback);
        assert!(source.feedback.as_ref().unwrap().bones.is_empty());
        assert_eq!(source.unknown_byte_11, 0x9b);
    }
}
