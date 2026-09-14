//! Shared raw FNV/FO3 condition (`CTDA`) lowering for FO4.
//!
//! A condition owns the immediately following CIS1/CIS2 rows.  Callers must pass that complete
//! scope so a failed conversion cannot leave companion strings behind.

use smallvec::SmallVec;

use crate::formkey_mapper::FormKeyMapper;
use crate::ids::SubrecordSig;
use crate::record::{FieldEntry, FieldValue};

pub const LEGACY_CTDA_LEN: usize = 28;
pub const LEGACY_OLD_CTDA_LEN: usize = 20;
pub const FO4_CTDA_LEN: usize = 32;

const FO4_MAX_CONDITION_FUNCTION_ID: u16 = 817;

const FNV_PARAM1_FORMID_FUNCTIONS: &[u16] = &[
    1, 27, 32, 42, 43, 44, 45, 47, 53, 56, 58, 59, 60, 66, 67, 68, 69, 71, 72, 73, 74, 76, 79, 84,
    99, 122, 129, 130, 132, 136, 149, 161, 162, 163, 172, 180, 182, 193, 195, 197, 199, 214, 223,
    228, 230, 246, 278, 280, 310, 370, 372, 382, 399, 409, 410, 411, 415, 420, 421, 427, 446, 449,
    450, 451, 464, 478, 515, 518, 519, 520, 521, 525, 526, 527, 528, 546, 555, 573, 574, 575, 607,
    610, 612, 614,
];
const FNV_PARAM2_FORMID_FUNCTIONS: &[u16] = &[60, 230, 280, 411];
const FO3_PARAM1_FORMID_FUNCTIONS: &[u16] = &[
    1, 27, 32, 42, 43, 44, 45, 47, 53, 56, 58, 59, 60, 66, 67, 68, 69, 71, 72, 73, 74, 76, 79, 84,
    99, 122, 129, 130, 132, 136, 149, 161, 162, 163, 172, 180, 182, 193, 195, 197, 199, 214, 223,
    228, 230, 246, 278, 280, 310, 370, 372, 382, 399, 409, 410, 411, 415, 427, 446, 449, 450, 451,
    464, 478, 515, 518, 519, 520, 521, 525, 526, 527, 528, 546, 555,
];
const FO3_PARAM2_FORMID_FUNCTIONS: &[u16] = &[60, 230, 280, 411];
const DROPPED_FUNCTIONS: &[u16] = &[
    36, 53, 76, 81, 98, 116, 117, 128, 129, 130, 131, 132, 160, 180, 219, 226, 258, 259, 264, 274,
    313, 323, 339, 382, 403, 430, 435, 436, 460, 462, 500, 503, 573, 574, 575, 586, 601, 607, 610,
    612, 614, 619,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LegacyConditionFamily {
    Fnv,
    Fo3,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegacyConditionReferenceSlot {
    pub field: &'static str,
    pub offset: usize,
    pub source_raw: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConditionReferenceOutcome {
    SourceNull,
    MappedRaw { source_raw: u32, target_raw: u32 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConditionReferenceDecision {
    pub field: &'static str,
    pub outcome: ConditionReferenceOutcome,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ConditionNormalizeReport {
    pub converted: bool,
    pub preserved_target: bool,
    pub source_function: Option<u16>,
    pub target_function: Option<u16>,
    pub source_run_on: Option<u32>,
    pub target_run_on: Option<u32>,
    pub references: Vec<ConditionReferenceDecision>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConditionNormalizeError {
    MissingCondition,
    OrphanCompanion,
    MalformedRow,
    UnsupportedFunction {
        source: u16,
    },
    UnsupportedRunOn {
        source: u32,
    },
    UnmappedRequiredReference {
        field: &'static str,
        source_raw: u32,
    },
}

/// Convert one complete `[CTDA, CIS1*, CIS2*]` scope.
///
/// On error the caller must discard the entire supplied scope.  This is intentionally raw-only:
/// it is the binary FNV/FO3 contract used by QUST, DIAL/INFO, SPEL/ENCH, and PERK records.
pub fn normalize_legacy_condition_scope(
    scope: &[FieldEntry],
    family: LegacyConditionFamily,
    mapper: &mut FormKeyMapper<'_>,
) -> Result<(Vec<FieldEntry>, ConditionNormalizeReport), ConditionNormalizeError> {
    let Some(condition) = scope.first() else {
        return Err(ConditionNormalizeError::MissingCondition);
    };
    if condition.sig.0 != *b"CTDA" {
        return Err(ConditionNormalizeError::MissingCondition);
    }
    if scope[1..]
        .iter()
        .any(|entry| !matches!(entry.sig.0, sig if sig == *b"CIS1" || sig == *b"CIS2"))
    {
        return Err(ConditionNormalizeError::OrphanCompanion);
    }

    let (value, report) = normalize_legacy_condition_value(&condition.value, family, mapper)?;
    let mut output = Vec::with_capacity(scope.len());
    output.push(FieldEntry {
        sig: SubrecordSig(*b"CTDA"),
        value,
    });
    output.extend_from_slice(&scope[1..]);
    Ok((output, report))
}

pub fn normalize_legacy_condition_value(
    value: &FieldValue,
    family: LegacyConditionFamily,
    mapper: &mut FormKeyMapper<'_>,
) -> Result<(FieldValue, ConditionNormalizeReport), ConditionNormalizeError> {
    let FieldValue::Bytes(bytes) = value else {
        return Err(ConditionNormalizeError::MalformedRow);
    };
    if bytes.len() == FO4_CTDA_LEN {
        return Ok((
            value.clone(),
            ConditionNormalizeReport {
                preserved_target: true,
                ..ConditionNormalizeReport::default()
            },
        ));
    }
    if bytes.len() != LEGACY_CTDA_LEN {
        return Err(ConditionNormalizeError::MalformedRow);
    }

    let source_function = read_u16(bytes, 8);
    let target_function = translate_condition_function(family, source_function).ok_or(
        ConditionNormalizeError::UnsupportedFunction {
            source: source_function,
        },
    )?;
    let source_run_on = read_u32(bytes, 20);
    let target_run_on =
        normalize_run_on(source_run_on).ok_or(ConditionNormalizeError::UnsupportedRunOn {
            source: source_run_on,
        })?;

    let mut target = bytes.to_vec();
    target[8..10].copy_from_slice(&target_function.to_le_bytes());
    target[20..24].copy_from_slice(&target_run_on.to_le_bytes());
    let mut report = ConditionNormalizeReport {
        converted: true,
        source_function: Some(source_function),
        target_function: Some(target_function),
        source_run_on: Some(source_run_on),
        target_run_on: Some(target_run_on),
        ..ConditionNormalizeReport::default()
    };

    if target[0] & 0x04 != 0 {
        remap_required_raw(
            &mut target,
            4,
            "condition_comparison_global",
            mapper,
            &mut report,
        )?;
    }
    let (param1, param2) = condition_formid_functions(family);
    if param1.binary_search(&source_function).is_ok() {
        remap_required_raw(
            &mut target,
            12,
            "condition_parameter_1",
            mapper,
            &mut report,
        )?;
    }
    if param2.binary_search(&source_function).is_ok() {
        remap_required_raw(
            &mut target,
            16,
            "condition_parameter_2",
            mapper,
            &mut report,
        )?;
    }
    if source_run_on == 2 {
        remap_required_raw(
            &mut target,
            24,
            "condition_run_on_reference",
            mapper,
            &mut report,
        )?;
    }
    target.extend_from_slice(&(-1_i32).to_le_bytes());
    Ok((FieldValue::Bytes(SmallVec::from_vec(target)), report))
}

pub fn legacy_condition_reference_slots(
    value: &FieldValue,
    family: LegacyConditionFamily,
) -> Result<Vec<LegacyConditionReferenceSlot>, ConditionNormalizeError> {
    let FieldValue::Bytes(bytes) = value else {
        return Err(ConditionNormalizeError::MalformedRow);
    };
    if !matches!(bytes.len(), LEGACY_OLD_CTDA_LEN | LEGACY_CTDA_LEN) {
        return Err(ConditionNormalizeError::MalformedRow);
    }

    let source_function = read_u16(bytes, 8);
    let (param1, param2) = condition_formid_functions(family);
    let mut slots = Vec::with_capacity(4);
    if bytes[0] & 0x04 != 0 {
        slots.push(condition_reference_slot(
            bytes,
            4,
            "condition_comparison_global",
        ));
    }
    if param1.binary_search(&source_function).is_ok() {
        slots.push(condition_reference_slot(bytes, 12, "condition_parameter_1"));
    }
    if param2.binary_search(&source_function).is_ok() {
        slots.push(condition_reference_slot(bytes, 16, "condition_parameter_2"));
    }
    if bytes.len() == LEGACY_CTDA_LEN && read_u32(bytes, 20) == 2 {
        slots.push(condition_reference_slot(
            bytes,
            24,
            "condition_run_on_reference",
        ));
    }
    Ok(slots)
}

fn condition_reference_slot(
    bytes: &[u8],
    offset: usize,
    field: &'static str,
) -> LegacyConditionReferenceSlot {
    LegacyConditionReferenceSlot {
        field,
        offset,
        source_raw: read_u32(bytes, offset),
    }
}

fn remap_required_raw(
    bytes: &mut [u8],
    offset: usize,
    field: &'static str,
    mapper: &mut FormKeyMapper<'_>,
    report: &mut ConditionNormalizeReport,
) -> Result<(), ConditionNormalizeError> {
    let source_raw = read_u32(bytes, offset);
    if source_raw == 0 {
        report.references.push(ConditionReferenceDecision {
            field,
            outcome: ConditionReferenceOutcome::SourceNull,
        });
        return Ok(());
    }
    if mapper.rewrite_raw_formid_at(bytes, offset).is_none() {
        return Err(ConditionNormalizeError::UnmappedRequiredReference { field, source_raw });
    }
    report.references.push(ConditionReferenceDecision {
        field,
        outcome: ConditionReferenceOutcome::MappedRaw {
            source_raw,
            target_raw: read_u32(bytes, offset),
        },
    });
    Ok(())
}

fn condition_formid_functions(family: LegacyConditionFamily) -> (&'static [u16], &'static [u16]) {
    match family {
        LegacyConditionFamily::Fnv => (FNV_PARAM1_FORMID_FUNCTIONS, FNV_PARAM2_FORMID_FUNCTIONS),
        LegacyConditionFamily::Fo3 => (FO3_PARAM1_FORMID_FUNCTIONS, FO3_PARAM2_FORMID_FUNCTIONS),
    }
}

fn normalize_run_on(source: u32) -> Option<u32> {
    match source {
        0..=7 => Some(source),
        20 => Some(0),
        _ => None,
    }
}

pub fn translate_condition_function(family: LegacyConditionFamily, source: u16) -> Option<u16> {
    if DROPPED_FUNCTIONS.binary_search(&source).is_ok()
        || matches!(family, LegacyConditionFamily::Fnv) && matches!(source, 420 | 421)
    {
        return None;
    }
    Some(match source {
        40 => 226,
        79 => 629,
        101 => 263,
        142 => 623,
        362 => 339,
        391 => 390,
        392 => 391,
        397 => 396,
        398 => 397,
        399 => 398,
        408 => 407,
        409 => 408,
        410 => 409,
        411 => 410,
        415 => 414,
        416 => 415,
        417 => 416,
        427 => 426,
        428 => 427,
        431 => 430,
        433 => 432,
        438 => 437,
        446 => 445,
        449 => 448,
        450 => 449,
        451 => 450,
        454 => 453,
        455 => 454,
        459 => 458,
        464 => 463,
        471 => 470,
        474 => 473,
        478 => 477,
        480 => 479,
        489 => 488,
        492 => 491,
        495 => 494,
        496 => 495,
        510 => 508,
        515 => 513,
        518 => 515,
        519 => 516,
        520 => 517,
        521 => 518,
        522 => 519,
        523 => 520,
        524 => 521,
        525 => 522,
        526 => 523,
        527 => 524,
        528 => 525,
        531 => 528,
        533 => 530,
        546 => 543,
        550 => 547,
        555 => 552,
        557 => 554,
        558 => 555,
        1030 if matches!(family, LegacyConditionFamily::Fnv) => 14,
        5993 if matches!(family, LegacyConditionFamily::Fnv) => 672,
        6013 if matches!(family, LegacyConditionFamily::Fnv) => 801,
        6204 if matches!(family, LegacyConditionFamily::Fnv) => 329,
        value if value <= FO4_MAX_CONDITION_FUNCTION_ID => value,
        _ => return None,
    })
}

fn read_u16(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes(bytes[offset..offset + 2].try_into().unwrap())
}

fn read_u32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formkey_mapper::MapperOptions;
    use crate::ids::FormKey;
    use crate::sym::StringInterner;

    fn scope(bytes: Vec<u8>, companions: &[&[u8]]) -> Vec<FieldEntry> {
        let mut fields = vec![FieldEntry {
            sig: SubrecordSig(*b"CTDA"),
            value: FieldValue::Bytes(SmallVec::from_vec(bytes)),
        }];
        fields.extend(
            companions
                .iter()
                .enumerate()
                .map(|(index, value)| FieldEntry {
                    sig: SubrecordSig(if index == 0 { *b"CIS1" } else { *b"CIS2" }),
                    value: FieldValue::Bytes(SmallVec::from_slice(value)),
                }),
        );
        fields
    }

    fn mapper<'a>(interner: &'a StringInterner) -> FormKeyMapper<'a> {
        FormKeyMapper::new(
            std::iter::empty(),
            MapperOptions {
                output_plugin_name: "Converted.esm".into(),
                source_plugin_name: "FalloutNV.esm".into(),
                target_master_names: vec!["Fallout4.esm".into()],
                ..MapperOptions::default()
            },
            interner,
        )
    }

    #[test]
    fn scope_remaps_known_function_reference_and_preserves_companions() {
        let interner = StringInterner::new();
        let mut mapper = mapper(&interner);
        mapper.add_mapping(
            FormKey {
                local: 0x400,
                plugin: interner.intern("FalloutNV.esm"),
            },
            FormKey {
                local: 0x1400,
                plugin: interner.intern("Converted.esm"),
            },
        );
        let mut condition = vec![0; LEGACY_CTDA_LEN];
        condition[8..10].copy_from_slice(&79_u16.to_le_bytes());
        condition[12..16].copy_from_slice(&0x400_u32.to_le_bytes());
        condition[20..24].copy_from_slice(&20_u32.to_le_bytes());
        let input = scope(condition, &[b"one\0", b"two\0"]);

        let (output, report) =
            normalize_legacy_condition_scope(&input, LegacyConditionFamily::Fnv, &mut mapper)
                .unwrap();
        let FieldValue::Bytes(bytes) = &output[0].value else {
            panic!("raw CTDA expected");
        };
        assert_eq!(bytes.len(), FO4_CTDA_LEN);
        assert_eq!(read_u16(bytes, 8), 629);
        assert_eq!(read_u32(bytes, 12), 0x0100_1400);
        assert_eq!(read_u32(bytes, 20), 0);
        assert_eq!(&output[1..], &input[1..]);
        assert!(report.converted);
        assert_eq!(report.references.len(), 1);
    }

    #[test]
    fn invalid_semantics_fail_before_any_companion_can_escape() {
        let interner = StringInterner::new();
        let mut mapper = mapper(&interner);
        let mut unknown_function = vec![0; LEGACY_CTDA_LEN];
        unknown_function[8..10].copy_from_slice(&900_u16.to_le_bytes());
        let input = scope(unknown_function, &[b"must_drop\0"]);
        assert_eq!(
            normalize_legacy_condition_scope(&input, LegacyConditionFamily::Fnv, &mut mapper),
            Err(ConditionNormalizeError::UnsupportedFunction { source: 900 })
        );

        let mut unknown_run_on = vec![0; LEGACY_CTDA_LEN];
        unknown_run_on[8..10].copy_from_slice(&79_u16.to_le_bytes());
        unknown_run_on[20..24].copy_from_slice(&99_u32.to_le_bytes());
        let input = scope(unknown_run_on, &[b"must_drop\0"]);
        assert_eq!(
            normalize_legacy_condition_scope(&input, LegacyConditionFamily::Fnv, &mut mapper),
            Err(ConditionNormalizeError::UnsupportedRunOn { source: 99 })
        );
    }

    #[test]
    fn reference_slots_are_family_typed_and_preserve_exact_offsets() {
        let mut condition = vec![0; LEGACY_CTDA_LEN];
        condition[0] = 0x04;
        condition[4..8].copy_from_slice(&0x0000_0100_u32.to_le_bytes());
        condition[8..10].copy_from_slice(&420_u16.to_le_bytes());
        condition[12..16].copy_from_slice(&0x0000_0200_u32.to_le_bytes());
        condition[20..24].copy_from_slice(&2_u32.to_le_bytes());
        condition[24..28].copy_from_slice(&0x0000_0300_u32.to_le_bytes());
        let value = FieldValue::Bytes(condition.into());

        assert_eq!(
            legacy_condition_reference_slots(&value, LegacyConditionFamily::Fnv).unwrap(),
            [
                LegacyConditionReferenceSlot {
                    field: "condition_comparison_global",
                    offset: 4,
                    source_raw: 0x100,
                },
                LegacyConditionReferenceSlot {
                    field: "condition_parameter_1",
                    offset: 12,
                    source_raw: 0x200,
                },
                LegacyConditionReferenceSlot {
                    field: "condition_run_on_reference",
                    offset: 24,
                    source_raw: 0x300,
                },
            ]
        );
        assert_eq!(
            legacy_condition_reference_slots(&value, LegacyConditionFamily::Fo3).unwrap(),
            [
                LegacyConditionReferenceSlot {
                    field: "condition_comparison_global",
                    offset: 4,
                    source_raw: 0x100,
                },
                LegacyConditionReferenceSlot {
                    field: "condition_run_on_reference",
                    offset: 24,
                    source_raw: 0x300,
                },
            ]
        );
    }

    #[test]
    fn reference_slots_reject_nonlegacy_rows() {
        assert_eq!(
            legacy_condition_reference_slots(
                &FieldValue::Bytes(vec![0; FO4_CTDA_LEN].into()),
                LegacyConditionFamily::Fnv,
            ),
            Err(ConditionNormalizeError::MalformedRow)
        );
    }

    #[test]
    fn old_reference_slots_preserve_typed_parameters_without_a_run_on_slot() {
        let mut condition = vec![0; LEGACY_OLD_CTDA_LEN];
        condition[8..10].copy_from_slice(&60_u16.to_le_bytes());
        condition[12..16].copy_from_slice(&0x100_u32.to_le_bytes());
        condition[16..20].copy_from_slice(&0x200_u32.to_le_bytes());

        assert_eq!(
            legacy_condition_reference_slots(
                &FieldValue::Bytes(condition.into()),
                LegacyConditionFamily::Fnv,
            )
            .unwrap(),
            [
                LegacyConditionReferenceSlot {
                    field: "condition_parameter_1",
                    offset: 12,
                    source_raw: 0x100,
                },
                LegacyConditionReferenceSlot {
                    field: "condition_parameter_2",
                    offset: 16,
                    source_raw: 0x200,
                },
            ]
        );
    }
}
