//! Semantic FNV/FO3 magic-item conversion for FO4.
//!
//! This conversion runs from the serial mapper pass, after eligible source records have been
//! preallocated. Effect rows are rebuilt atomically so a rejected condition never leaves CIS
//! strings behind and a rejected base effect never leaves an EFIT/CTDA tail behind.

use smallvec::SmallVec;

use crate::formkey_mapper::FormKeyMapper;
use crate::ids::{FormKey, SubrecordSig};
use crate::record::{FieldEntry, FieldValue, Record};

pub const LEGACY_EFIT_LEN: usize = 20;
pub const FO4_EFIT_LEN: usize = 12;
pub const LEGACY_CTDA_LEN: usize = 28;
pub const FO4_CTDA_LEN: usize = 32;
pub const LEGACY_ALCH_ENIT_LEN: usize = 20;
pub const FO4_ALCH_ENIT_LEN: usize = 20;
pub const LEGACY_ENCH_ENIT_LEN: usize = 16;
pub const FO4_ENCH_ENIT_LEN: usize = 36;
pub const LEGACY_SPEL_SPIT_LEN: usize = 16;
pub const FO4_SPEL_SPIT_LEN: usize = 36;

const FO4_MAX_CONDITION_FUNCTION_ID: u16 = 817;
const FO4_ALCH_MEDICINE_FLAG: u32 = 1 << 16;
const FO4_SPEL_PC_START_FLAG: u32 = 1 << 17;
const FO4_SPEL_IGNORE_LOS_FLAG: u32 = 1 << 19;
const FO4_SPEL_IGNORE_RESISTANCE_FLAG: u32 = 1 << 20;
const FO4_SPEL_NO_ABSORB_REFLECT_FLAG: u32 = 1 << 21;
const FO4_ENCHANTMENT_TYPE: u32 = 6;
const FO4_STAFF_ENCHANTMENT_TYPE: u32 = 12;

const FNV_CTDA_PARAM1_FORMID_FUNCTIONS: &[u16] = &[
    1, 27, 32, 42, 43, 44, 45, 47, 53, 56, 58, 59, 60, 66, 67, 68, 69, 71, 72, 73, 74, 76, 79, 84,
    99, 122, 129, 130, 132, 136, 149, 161, 162, 163, 172, 180, 182, 193, 195, 197, 199, 214, 223,
    228, 230, 246, 278, 280, 310, 370, 372, 382, 399, 409, 410, 411, 415, 420, 421, 427, 446, 449,
    450, 451, 464, 478, 515, 518, 519, 520, 521, 525, 526, 527, 528, 546, 555, 573, 574, 575, 607,
    610, 612, 614,
];
const FNV_CTDA_PARAM2_FORMID_FUNCTIONS: &[u16] = &[60, 230, 280, 411];
const FO3_CTDA_PARAM1_FORMID_FUNCTIONS: &[u16] = &[
    1, 27, 32, 42, 43, 44, 45, 47, 53, 56, 58, 59, 60, 66, 67, 68, 69, 71, 72, 73, 74, 76, 79, 84,
    99, 122, 129, 130, 132, 136, 149, 161, 162, 163, 172, 180, 182, 193, 195, 197, 199, 214, 223,
    228, 230, 246, 278, 280, 310, 370, 372, 382, 399, 409, 410, 411, 415, 427, 446, 449, 450, 451,
    464, 478, 515, 518, 519, 520, 521, 525, 526, 527, 528, 546, 555,
];
const FO3_CTDA_PARAM2_FORMID_FUNCTIONS: &[u16] = &[60, 230, 280, 411];

const DROPPED_LEGACY_CONDITION_FUNCTIONS: &[u16] = &[
    36, 53, 76, 81, 98, 116, 117, 128, 129, 130, 131, 132, 160, 180, 219, 226, 258, 259, 264, 274,
    313, 323, 339, 382, 403, 430, 435, 436, 460, 462, 500, 503, 573, 574, 575, 586, 601, 607, 610,
    612, 614, 619,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LegacyMagicFamily {
    Fnv,
    Fo3,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MagicReferenceOutcome {
    SourceNull,
    MappedRaw { source_raw: u32, target_raw: u32 },
    MappedTyped { source: FormKey, target: FormKey },
    UnmappedRaw { source_raw: u32 },
    UnmappedTyped { source: FormKey },
    UnsupportedValue,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MagicReferenceDecision {
    pub effect_index: Option<usize>,
    pub field: &'static str,
    pub outcome: MagicReferenceOutcome,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MagicEnumDecision {
    pub field: &'static str,
    pub source: u32,
    pub target: u32,
    pub dropped_bits: u32,
    pub used_default: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MagicEffectsNormalizeReport {
    pub converted_effects: usize,
    pub preserved_target_effects: usize,
    pub dropped_effects: usize,
    pub converted_conditions: usize,
    pub preserved_target_conditions: usize,
    pub dropped_conditions: usize,
    pub orphan_condition_strings_dropped: usize,
    pub converted_metadata_rows: usize,
    pub preserved_target_metadata_rows: usize,
    pub dropped_metadata_rows: usize,
    pub references: Vec<MagicReferenceDecision>,
    pub enums: Vec<MagicEnumDecision>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum RowShape {
    Legacy,
    Target,
    Malformed,
}

/// Rebuild the FNV/FO3 ALCH, ENCH, or SPEL magic contract for FO4.
///
/// The caller must supply the run's serial `FormKeyMapper`. Legacy EFID rows whose base MGEF is
/// not mapped are removed with their whole effect. A condition with any unresolved typed/raw
/// reference is removed with its own CIS1/CIS2 rows. Target-sized rows are preserved.
pub fn normalize_legacy_magic_effects(
    record: &mut Record,
    family: LegacyMagicFamily,
    mapper: &mut FormKeyMapper<'_>,
) -> MagicEffectsNormalizeReport {
    let mut report = MagicEffectsNormalizeReport::default();
    if !matches!(record.sig.0, sig if sig == *b"ALCH" || sig == *b"ENCH" || sig == *b"SPEL") {
        return report;
    }

    let input: Vec<FieldEntry> = record.fields.drain(..).collect();
    let mut output = Vec::with_capacity(input.len());
    let mut index = 0;
    let mut effect_index = 0;
    while index < input.len() {
        if input[index].sig.0 == *b"EFID" {
            let end = input[index + 1..]
                .iter()
                .position(|entry| entry.sig.0 == *b"EFID")
                .map(|offset| index + 1 + offset)
                .unwrap_or(input.len());
            normalize_effect_group(
                &input[index..end],
                effect_index,
                family,
                mapper,
                &mut report,
                &mut output,
            );
            effect_index += 1;
            index = end;
            continue;
        }

        if matches!(input[index].sig.0, sig if sig == *b"EFIT" || sig == *b"CTDA") {
            if input[index].sig.0 == *b"CTDA" {
                report.dropped_conditions += 1;
            }
            index += 1;
            continue;
        }
        if matches!(input[index].sig.0, sig if sig == *b"CIS1" || sig == *b"CIS2") {
            report.orphan_condition_strings_dropped += 1;
            index += 1;
            continue;
        }

        if let Some(entry) =
            normalize_metadata_row(&input[index], record.sig.0, mapper, &mut report)
        {
            output.push(entry);
        }
        index += 1;
    }

    record.fields = output.into_iter().collect();
    report
}

fn normalize_effect_group(
    group: &[FieldEntry],
    effect_index: usize,
    family: LegacyMagicFamily,
    mapper: &mut FormKeyMapper<'_>,
    report: &mut MagicEffectsNormalizeReport,
    output: &mut Vec<FieldEntry>,
) {
    let efit_positions = group
        .iter()
        .enumerate()
        .filter_map(|(index, entry)| (entry.sig.0 == *b"EFIT").then_some(index))
        .collect::<Vec<_>>();
    if efit_positions != [1] {
        drop_effect_group(group, report, output);
        return;
    }

    let efit_shape = effect_row_shape(&group[1].value, mapper.interner);
    let (efid, efit) = match efit_shape {
        RowShape::Target => {
            report.preserved_target_effects += 1;
            (group[0].clone(), group[1].clone())
        }
        RowShape::Legacy => {
            let Some(efid_value) = remap_reference_value(
                &group[0].value,
                Some(effect_index),
                "base_effect",
                mapper,
                report,
            ) else {
                drop_effect_group(group, report, output);
                return;
            };
            let Some(efit_value) = convert_effect_row(&group[1].value, mapper.interner) else {
                drop_effect_group(group, report, output);
                return;
            };
            report.converted_effects += 1;
            (
                FieldEntry {
                    sig: SubrecordSig(*b"EFID"),
                    value: efid_value,
                },
                FieldEntry {
                    sig: SubrecordSig(*b"EFIT"),
                    value: efit_value,
                },
            )
        }
        RowShape::Malformed => {
            drop_effect_group(group, report, output);
            return;
        }
    };

    output.push(efid);
    output.push(efit);
    normalize_condition_payload(&group[2..], effect_index, family, mapper, report, output);
}

fn drop_effect_group(
    group: &[FieldEntry],
    report: &mut MagicEffectsNormalizeReport,
    output: &mut Vec<FieldEntry>,
) {
    report.dropped_effects += 1;
    for entry in group {
        match entry.sig.0 {
            sig if sig == *b"CTDA" => report.dropped_conditions += 1,
            sig if sig == *b"CIS1" || sig == *b"CIS2" => {
                report.orphan_condition_strings_dropped += 1;
            }
            sig if sig == *b"EFID" || sig == *b"EFIT" => {}
            _ => output.push(entry.clone()),
        }
    }
}

fn normalize_condition_payload(
    fields: &[FieldEntry],
    effect_index: usize,
    family: LegacyMagicFamily,
    mapper: &mut FormKeyMapper<'_>,
    report: &mut MagicEffectsNormalizeReport,
    output: &mut Vec<FieldEntry>,
) {
    let mut index = 0;
    while index < fields.len() {
        if fields[index].sig.0 != *b"CTDA" {
            if matches!(fields[index].sig.0, sig if sig == *b"CIS1" || sig == *b"CIS2") {
                report.orphan_condition_strings_dropped += 1;
            } else {
                output.push(fields[index].clone());
            }
            index += 1;
            continue;
        }

        let strings_end = fields[index + 1..]
            .iter()
            .position(|entry| !matches!(entry.sig.0, sig if sig == *b"CIS1" || sig == *b"CIS2"))
            .map(|offset| index + 1 + offset)
            .unwrap_or(fields.len());
        match convert_condition_row(&fields[index].value, effect_index, family, mapper, report) {
            Some(value) => {
                output.push(FieldEntry {
                    sig: SubrecordSig(*b"CTDA"),
                    value,
                });
                output.extend_from_slice(&fields[index + 1..strings_end]);
            }
            None => {
                report.dropped_conditions += 1;
                report.orphan_condition_strings_dropped += strings_end - index - 1;
            }
        }
        index = strings_end;
    }
}

fn effect_row_shape(value: &FieldValue, interner: &crate::sym::StringInterner) -> RowShape {
    match value {
        FieldValue::Bytes(bytes) if bytes.len() == LEGACY_EFIT_LEN => RowShape::Legacy,
        FieldValue::Bytes(bytes) if bytes.len() == FO4_EFIT_LEN => RowShape::Target,
        FieldValue::Struct(fields) => {
            if has_named_field(fields, interner, "actor_value")
                || has_named_field(fields, interner, "type")
            {
                RowShape::Legacy
            } else if has_named_field(fields, interner, "magnitude")
                || has_named_field(fields, interner, "area")
                || has_named_field(fields, interner, "duration")
            {
                RowShape::Target
            } else {
                RowShape::Malformed
            }
        }
        _ => RowShape::Malformed,
    }
}

fn convert_effect_row(
    value: &FieldValue,
    interner: &crate::sym::StringInterner,
) -> Option<FieldValue> {
    match value {
        FieldValue::Bytes(bytes) if bytes.len() == LEGACY_EFIT_LEN => {
            let magnitude = read_u32(bytes, 0) as f32;
            let mut target = Vec::with_capacity(FO4_EFIT_LEN);
            target.extend_from_slice(&magnitude.to_le_bytes());
            target.extend_from_slice(&bytes[4..12]);
            Some(FieldValue::Bytes(SmallVec::from_vec(target)))
        }
        FieldValue::Struct(fields) => {
            let magnitude = named_number(fields, interner, "magnitude")? as f32;
            let area = named_u32(fields, interner, "area").unwrap_or(0);
            let duration = named_u32(fields, interner, "duration").unwrap_or(0);
            Some(FieldValue::Struct(vec![
                (interner.intern("magnitude"), FieldValue::Float(magnitude)),
                (interner.intern("area"), FieldValue::Uint(u64::from(area))),
                (
                    interner.intern("duration"),
                    FieldValue::Uint(u64::from(duration)),
                ),
            ]))
        }
        _ => None,
    }
}

fn convert_condition_row(
    value: &FieldValue,
    effect_index: usize,
    family: LegacyMagicFamily,
    mapper: &mut FormKeyMapper<'_>,
    report: &mut MagicEffectsNormalizeReport,
) -> Option<FieldValue> {
    match condition_row_shape(value, mapper.interner) {
        RowShape::Target => {
            report.preserved_target_conditions += 1;
            Some(value.clone())
        }
        RowShape::Malformed => None,
        RowShape::Legacy => match value {
            FieldValue::Bytes(bytes) => {
                let source_function = read_u16(bytes, 8);
                let function = translate_condition_function(family, source_function)?;
                let mut target = bytes.to_vec();
                target[8..10].copy_from_slice(&function.to_le_bytes());
                target.extend_from_slice(&(-1_i32).to_le_bytes());
                report.enums.push(MagicEnumDecision {
                    field: "condition_function",
                    source: u32::from(source_function),
                    target: u32::from(function),
                    dropped_bits: 0,
                    used_default: false,
                });
                if target[0] & 0x04 != 0
                    && !remap_raw_reference_at(
                        &mut target,
                        4,
                        Some(effect_index),
                        "condition_comparison_global",
                        mapper,
                        report,
                    )
                {
                    return None;
                }
                let (param1_functions, param2_functions) = condition_formid_functions(family);
                if param1_functions.binary_search(&source_function).is_ok()
                    && !remap_raw_reference_at(
                        &mut target,
                        12,
                        Some(effect_index),
                        "condition_parameter_1",
                        mapper,
                        report,
                    )
                {
                    return None;
                }
                if param2_functions.binary_search(&source_function).is_ok()
                    && !remap_raw_reference_at(
                        &mut target,
                        16,
                        Some(effect_index),
                        "condition_parameter_2",
                        mapper,
                        report,
                    )
                {
                    return None;
                }
                let source_run_on = read_u32(&target, 20);
                if source_run_on == 20 {
                    target[20..24].copy_from_slice(&0_u32.to_le_bytes());
                }
                if source_run_on == 2
                    && !remap_raw_reference_at(
                        &mut target,
                        24,
                        Some(effect_index),
                        "condition_run_on_reference",
                        mapper,
                        report,
                    )
                {
                    return None;
                }
                report.converted_conditions += 1;
                Some(FieldValue::Bytes(SmallVec::from_vec(target)))
            }
            FieldValue::Struct(fields) => {
                let source_function = named_u32(fields, mapper.interner, "function")
                    .and_then(|function| u16::try_from(function).ok())?;
                let function = translate_condition_function(family, source_function)?;
                let mut target = FieldValue::Struct(fields.clone());
                if !remap_typed_formkeys(
                    &mut target,
                    Some(effect_index),
                    "condition_reference",
                    mapper,
                    report,
                ) {
                    return None;
                }
                let FieldValue::Struct(fields) = &mut target else {
                    unreachable!();
                };
                let function_value = fields
                    .iter_mut()
                    .find(|(key, _)| mapper.interner.resolve(*key) == Some("function"))
                    .map(|(_, value)| value)?;
                *function_value = match function_value {
                    FieldValue::Int(_) => FieldValue::Int(i64::from(function)),
                    FieldValue::Uint(_) => FieldValue::Uint(u64::from(function)),
                    FieldValue::Float(_) => FieldValue::Float(f32::from(function)),
                    _ => return None,
                };
                if named_u32(fields, mapper.interner, "run_on") == Some(20) {
                    let run_on = fields
                        .iter_mut()
                        .find(|(key, _)| mapper.interner.resolve(*key) == Some("run_on"))
                        .map(|(_, value)| value)?;
                    *run_on = match run_on {
                        FieldValue::Int(_) => FieldValue::Int(0),
                        FieldValue::Uint(_) => FieldValue::Uint(0),
                        FieldValue::Float(_) => FieldValue::Float(0.0),
                        _ => return None,
                    };
                }
                report.enums.push(MagicEnumDecision {
                    field: "condition_function",
                    source: u32::from(source_function),
                    target: u32::from(function),
                    dropped_bits: 0,
                    used_default: false,
                });
                fields.push((mapper.interner.intern("parameter_3"), FieldValue::Int(-1)));
                report.converted_conditions += 1;
                Some(target)
            }
            _ => None,
        },
    }
}

fn condition_row_shape(value: &FieldValue, interner: &crate::sym::StringInterner) -> RowShape {
    match value {
        FieldValue::Bytes(bytes) if bytes.len() == LEGACY_CTDA_LEN => RowShape::Legacy,
        FieldValue::Bytes(bytes) if bytes.len() == FO4_CTDA_LEN => RowShape::Target,
        FieldValue::Struct(fields) if has_named_field(fields, interner, "parameter_3") => {
            RowShape::Target
        }
        FieldValue::Struct(fields) if has_named_field(fields, interner, "function") => {
            RowShape::Legacy
        }
        _ => RowShape::Malformed,
    }
}

fn condition_formid_functions(family: LegacyMagicFamily) -> (&'static [u16], &'static [u16]) {
    match family {
        LegacyMagicFamily::Fnv => (
            FNV_CTDA_PARAM1_FORMID_FUNCTIONS,
            FNV_CTDA_PARAM2_FORMID_FUNCTIONS,
        ),
        LegacyMagicFamily::Fo3 => (
            FO3_CTDA_PARAM1_FORMID_FUNCTIONS,
            FO3_CTDA_PARAM2_FORMID_FUNCTIONS,
        ),
    }
}

fn translate_condition_function(family: LegacyMagicFamily, source: u16) -> Option<u16> {
    if DROPPED_LEGACY_CONDITION_FUNCTIONS
        .binary_search(&source)
        .is_ok()
        || matches!(family, LegacyMagicFamily::Fnv) && matches!(source, 420 | 421)
    {
        return None;
    }

    let target = match source {
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
        1030 if matches!(family, LegacyMagicFamily::Fnv) => 14,
        5993 if matches!(family, LegacyMagicFamily::Fnv) => 672,
        6013 if matches!(family, LegacyMagicFamily::Fnv) => 801,
        6204 if matches!(family, LegacyMagicFamily::Fnv) => 329,
        source if source <= FO4_MAX_CONDITION_FUNCTION_ID => source,
        _ => return None,
    };
    Some(target)
}

fn normalize_metadata_row(
    entry: &FieldEntry,
    record_sig: [u8; 4],
    mapper: &mut FormKeyMapper<'_>,
    report: &mut MagicEffectsNormalizeReport,
) -> Option<FieldEntry> {
    let expected_sig = match record_sig {
        sig if sig == *b"ALCH" || sig == *b"ENCH" => *b"ENIT",
        sig if sig == *b"SPEL" => *b"SPIT",
        _ => return Some(entry.clone()),
    };
    if entry.sig.0 != expected_sig {
        return Some(entry.clone());
    }

    let value = match record_sig {
        sig if sig == *b"ALCH" => normalize_alch_enit(&entry.value, mapper, report),
        sig if sig == *b"ENCH" => normalize_ench_enit(&entry.value, mapper.interner, report),
        sig if sig == *b"SPEL" => normalize_spel_spit(&entry.value, mapper.interner, report),
        _ => unreachable!(),
    };
    match value {
        Some(value) => Some(FieldEntry {
            sig: SubrecordSig(expected_sig),
            value,
        }),
        None => {
            report.dropped_metadata_rows += 1;
            None
        }
    }
}

fn normalize_alch_enit(
    value: &FieldValue,
    mapper: &mut FormKeyMapper<'_>,
    report: &mut MagicEffectsNormalizeReport,
) -> Option<FieldValue> {
    match value {
        FieldValue::Bytes(bytes) if bytes.len() == LEGACY_ALCH_ENIT_LEN => {
            let source_flags = u32::from(bytes[4]);
            let target_flags = translate_alch_flags(source_flags);
            report.enums.push(MagicEnumDecision {
                field: "alch_flags",
                source: source_flags,
                target: target_flags,
                dropped_bits: source_flags & !0x07,
                used_default: false,
            });
            let mut target = vec![0_u8; FO4_ALCH_ENIT_LEN];
            target[0..4].copy_from_slice(&bytes[0..4]);
            target[4..8].copy_from_slice(&target_flags.to_le_bytes());
            target[8..12].copy_from_slice(&bytes[8..12]);
            target[12..16].copy_from_slice(&bytes[12..16]);
            target[16..20].copy_from_slice(&bytes[16..20]);
            null_unmapped_raw_reference_at(&mut target, 8, "alch_addiction", mapper, report);
            null_unmapped_raw_reference_at(&mut target, 16, "alch_sound_consume", mapper, report);
            report.converted_metadata_rows += 1;
            Some(FieldValue::Bytes(SmallVec::from_vec(target)))
        }
        FieldValue::Struct(fields) if has_named_field(fields, mapper.interner, "addiction") => {
            report.preserved_target_metadata_rows += 1;
            Some(value.clone())
        }
        FieldValue::Struct(fields)
            if has_named_field(fields, mapper.interner, "withdrawal_effect") =>
        {
            let source_flags = named_u32(fields, mapper.interner, "flags").unwrap_or(0);
            let target_flags = translate_alch_flags(source_flags);
            report.enums.push(MagicEnumDecision {
                field: "alch_flags",
                source: source_flags,
                target: target_flags,
                dropped_bits: source_flags & !0x07,
                used_default: false,
            });
            let addiction = named_field(fields, mapper.interner, "withdrawal_effect")
                .and_then(|value| {
                    remap_optional_reference_value(value, "alch_addiction", mapper, report)
                })
                .unwrap_or(FieldValue::Uint(0));
            let sound = named_field(fields, mapper.interner, "sound_consume")
                .and_then(|value| {
                    remap_optional_reference_value(value, "alch_sound_consume", mapper, report)
                })
                .unwrap_or(FieldValue::Uint(0));
            let target = FieldValue::Struct(vec![
                (
                    mapper.interner.intern("value"),
                    FieldValue::Int(named_i64(fields, mapper.interner, "value").unwrap_or(0)),
                ),
                (
                    mapper.interner.intern("flags"),
                    FieldValue::Uint(u64::from(target_flags)),
                ),
                (mapper.interner.intern("addiction"), addiction),
                (
                    mapper.interner.intern("addiction_chance"),
                    FieldValue::Float(
                        named_number(fields, mapper.interner, "addiction_chance").unwrap_or(0.0)
                            as f32,
                    ),
                ),
                (mapper.interner.intern("sound_consume"), sound),
            ]);
            report.converted_metadata_rows += 1;
            Some(target)
        }
        _ => None,
    }
}

fn normalize_ench_enit(
    value: &FieldValue,
    interner: &crate::sym::StringInterner,
    report: &mut MagicEffectsNormalizeReport,
) -> Option<FieldValue> {
    let (source_type, source_flags, raw) = match value {
        FieldValue::Bytes(bytes) if bytes.len() == FO4_ENCH_ENIT_LEN => {
            report.preserved_target_metadata_rows += 1;
            return Some(value.clone());
        }
        FieldValue::Bytes(bytes) if bytes.len() == LEGACY_ENCH_ENIT_LEN => {
            (read_u32(bytes, 0), u32::from(bytes[12]), true)
        }
        FieldValue::Struct(fields) if has_named_field(fields, interner, "enchant_type") => {
            report.preserved_target_metadata_rows += 1;
            return Some(value.clone());
        }
        FieldValue::Struct(fields) if has_named_field(fields, interner, "type") => (
            named_u32(fields, interner, "type").unwrap_or(0),
            named_u32(fields, interner, "flags").unwrap_or(0),
            false,
        ),
        _ => return None,
    };
    let target_flags = source_flags & 1;
    let (enchant_type, used_default) = match source_type {
        2 => (FO4_ENCHANTMENT_TYPE, false),
        3 => (FO4_STAFF_ENCHANTMENT_TYPE, false),
        _ => (FO4_ENCHANTMENT_TYPE, true),
    };
    report.enums.extend([
        MagicEnumDecision {
            field: "ench_flags",
            source: source_flags,
            target: target_flags,
            dropped_bits: source_flags & !1,
            used_default: false,
        },
        MagicEnumDecision {
            field: "ench_type",
            source: source_type,
            target: enchant_type,
            dropped_bits: 0,
            used_default,
        },
    ]);
    report.converted_metadata_rows += 1;

    if raw {
        let mut target = vec![0_u8; FO4_ENCH_ENIT_LEN];
        target[4..8].copy_from_slice(&target_flags.to_le_bytes());
        target[20..24].copy_from_slice(&enchant_type.to_le_bytes());
        Some(FieldValue::Bytes(SmallVec::from_vec(target)))
    } else {
        Some(FieldValue::Struct(vec![
            (interner.intern("enchantment_cost"), FieldValue::Int(0)),
            (
                interner.intern("flags"),
                FieldValue::Uint(u64::from(target_flags)),
            ),
            (interner.intern("cast_type"), FieldValue::Uint(0)),
            (interner.intern("enchantment_amount"), FieldValue::Int(0)),
            (interner.intern("target_type"), FieldValue::Uint(0)),
            (
                interner.intern("enchant_type"),
                FieldValue::Uint(u64::from(enchant_type)),
            ),
            (interner.intern("charge_time"), FieldValue::Float(0.0)),
            (interner.intern("base_enchantment"), FieldValue::Uint(0)),
            (interner.intern("worn_restrictions"), FieldValue::Uint(0)),
        ]))
    }
}

fn normalize_spel_spit(
    value: &FieldValue,
    interner: &crate::sym::StringInterner,
    report: &mut MagicEffectsNormalizeReport,
) -> Option<FieldValue> {
    let (source_type, source_cost, source_flags, raw) = match value {
        FieldValue::Bytes(bytes) if bytes.len() == FO4_SPEL_SPIT_LEN => {
            report.preserved_target_metadata_rows += 1;
            return Some(value.clone());
        }
        FieldValue::Bytes(bytes) if bytes.len() == LEGACY_SPEL_SPIT_LEN => (
            read_u32(bytes, 0),
            read_u32(bytes, 4),
            u32::from(bytes[12]),
            true,
        ),
        FieldValue::Struct(fields) if has_named_field(fields, interner, "base_cost") => {
            report.preserved_target_metadata_rows += 1;
            return Some(value.clone());
        }
        FieldValue::Struct(fields) if has_named_field(fields, interner, "cost_unused") => (
            named_u32(fields, interner, "type").unwrap_or(0),
            named_u32(fields, interner, "cost_unused").unwrap_or(0),
            named_u32(fields, interner, "flags").unwrap_or(0),
            false,
        ),
        _ => return None,
    };
    let (target_type, used_default) = if source_type <= 10 {
        (source_type, false)
    } else {
        (0, true)
    };
    let target_flags = translate_spel_flags(source_flags);
    report.enums.extend([
        MagicEnumDecision {
            field: "spel_type",
            source: source_type,
            target: target_type,
            dropped_bits: 0,
            used_default,
        },
        MagicEnumDecision {
            field: "spel_flags",
            source: source_flags,
            target: target_flags,
            dropped_bits: source_flags & !0x75,
            used_default: false,
        },
    ]);
    report.converted_metadata_rows += 1;

    if raw {
        let mut target = vec![0_u8; FO4_SPEL_SPIT_LEN];
        target[0..4].copy_from_slice(&source_cost.to_le_bytes());
        target[4..8].copy_from_slice(&target_flags.to_le_bytes());
        target[8..12].copy_from_slice(&target_type.to_le_bytes());
        Some(FieldValue::Bytes(SmallVec::from_vec(target)))
    } else {
        Some(FieldValue::Struct(vec![
            (
                interner.intern("base_cost"),
                FieldValue::Uint(u64::from(source_cost)),
            ),
            (
                interner.intern("flags"),
                FieldValue::Uint(u64::from(target_flags)),
            ),
            (
                interner.intern("type"),
                FieldValue::Uint(u64::from(target_type)),
            ),
            (interner.intern("charge_time"), FieldValue::Float(0.0)),
            (interner.intern("cast_type"), FieldValue::Uint(0)),
            (interner.intern("target_type"), FieldValue::Uint(0)),
            (interner.intern("cast_duration"), FieldValue::Float(0.0)),
            (interner.intern("range"), FieldValue::Float(0.0)),
            (interner.intern("casting_perk"), FieldValue::Uint(0)),
        ]))
    }
}

fn translate_alch_flags(source: u32) -> u32 {
    (source & 0x03) | ((source & 0x04 != 0) as u32 * FO4_ALCH_MEDICINE_FLAG)
}

fn translate_spel_flags(source: u32) -> u32 {
    let mut target = source & 0x01;
    if source & 0x04 != 0 {
        target |= FO4_SPEL_PC_START_FLAG;
    }
    if source & 0x10 != 0 {
        target |= FO4_SPEL_IGNORE_LOS_FLAG;
    }
    if source & 0x20 != 0 {
        target |= FO4_SPEL_IGNORE_RESISTANCE_FLAG;
    }
    if source & 0x40 != 0 {
        target |= FO4_SPEL_NO_ABSORB_REFLECT_FLAG;
    }
    target
}

fn remap_reference_value(
    value: &FieldValue,
    effect_index: Option<usize>,
    field: &'static str,
    mapper: &mut FormKeyMapper<'_>,
    report: &mut MagicEffectsNormalizeReport,
) -> Option<FieldValue> {
    match value {
        FieldValue::FormKey(source) if source.local == 0 => {
            report.references.push(MagicReferenceDecision {
                effect_index,
                field,
                outcome: MagicReferenceOutcome::SourceNull,
            });
            None
        }
        FieldValue::FormKey(source) => match mapper.lookup(*source) {
            Some(target) => {
                report.references.push(MagicReferenceDecision {
                    effect_index,
                    field,
                    outcome: MagicReferenceOutcome::MappedTyped {
                        source: *source,
                        target,
                    },
                });
                Some(FieldValue::FormKey(target))
            }
            None => {
                report.references.push(MagicReferenceDecision {
                    effect_index,
                    field,
                    outcome: MagicReferenceOutcome::UnmappedTyped { source: *source },
                });
                None
            }
        },
        FieldValue::Bytes(bytes) if bytes.len() == 4 => {
            let source_raw = read_u32(bytes, 0);
            if source_raw == 0 {
                report.references.push(MagicReferenceDecision {
                    effect_index,
                    field,
                    outcome: MagicReferenceOutcome::SourceNull,
                });
                return None;
            }
            let mut target = bytes.to_vec();
            match mapper.rewrite_raw_formid_at(&mut target, 0) {
                Some(_) => {
                    let target_raw = read_u32(&target, 0);
                    report.references.push(MagicReferenceDecision {
                        effect_index,
                        field,
                        outcome: MagicReferenceOutcome::MappedRaw {
                            source_raw,
                            target_raw,
                        },
                    });
                    Some(FieldValue::Bytes(SmallVec::from_vec(target)))
                }
                None => {
                    report.references.push(MagicReferenceDecision {
                        effect_index,
                        field,
                        outcome: MagicReferenceOutcome::UnmappedRaw { source_raw },
                    });
                    None
                }
            }
        }
        _ => {
            report.references.push(MagicReferenceDecision {
                effect_index,
                field,
                outcome: MagicReferenceOutcome::UnsupportedValue,
            });
            None
        }
    }
}

fn remap_optional_reference_value(
    value: &FieldValue,
    field: &'static str,
    mapper: &mut FormKeyMapper<'_>,
    report: &mut MagicEffectsNormalizeReport,
) -> Option<FieldValue> {
    match value {
        FieldValue::Uint(0) | FieldValue::Int(0) => {
            report.references.push(MagicReferenceDecision {
                effect_index: None,
                field,
                outcome: MagicReferenceOutcome::SourceNull,
            });
            Some(FieldValue::Uint(0))
        }
        FieldValue::Bytes(bytes) if bytes.len() == 4 && read_u32(bytes, 0) == 0 => {
            report.references.push(MagicReferenceDecision {
                effect_index: None,
                field,
                outcome: MagicReferenceOutcome::SourceNull,
            });
            Some(FieldValue::Uint(0))
        }
        _ => remap_reference_value(value, None, field, mapper, report),
    }
}

fn remap_raw_reference_at(
    bytes: &mut [u8],
    offset: usize,
    effect_index: Option<usize>,
    field: &'static str,
    mapper: &mut FormKeyMapper<'_>,
    report: &mut MagicEffectsNormalizeReport,
) -> bool {
    let source_raw = read_u32(bytes, offset);
    if source_raw == 0 {
        report.references.push(MagicReferenceDecision {
            effect_index,
            field,
            outcome: MagicReferenceOutcome::SourceNull,
        });
        return true;
    }
    match mapper.rewrite_raw_formid_at(bytes, offset) {
        Some(_) => {
            report.references.push(MagicReferenceDecision {
                effect_index,
                field,
                outcome: MagicReferenceOutcome::MappedRaw {
                    source_raw,
                    target_raw: read_u32(bytes, offset),
                },
            });
            true
        }
        None => {
            report.references.push(MagicReferenceDecision {
                effect_index,
                field,
                outcome: MagicReferenceOutcome::UnmappedRaw { source_raw },
            });
            false
        }
    }
}

fn null_unmapped_raw_reference_at(
    bytes: &mut [u8],
    offset: usize,
    field: &'static str,
    mapper: &mut FormKeyMapper<'_>,
    report: &mut MagicEffectsNormalizeReport,
) {
    if !remap_raw_reference_at(bytes, offset, None, field, mapper, report) {
        bytes[offset..offset + 4].fill(0);
    }
}

fn remap_typed_formkeys(
    value: &mut FieldValue,
    effect_index: Option<usize>,
    field: &'static str,
    mapper: &mut FormKeyMapper<'_>,
    report: &mut MagicEffectsNormalizeReport,
) -> bool {
    match value {
        FieldValue::FormKey(source) if source.local == 0 => true,
        FieldValue::FormKey(source) => match mapper.lookup(*source) {
            Some(target) => {
                report.references.push(MagicReferenceDecision {
                    effect_index,
                    field,
                    outcome: MagicReferenceOutcome::MappedTyped {
                        source: *source,
                        target,
                    },
                });
                *source = target;
                true
            }
            None => {
                report.references.push(MagicReferenceDecision {
                    effect_index,
                    field,
                    outcome: MagicReferenceOutcome::UnmappedTyped { source: *source },
                });
                false
            }
        },
        FieldValue::List(items) => items
            .iter_mut()
            .all(|item| remap_typed_formkeys(item, effect_index, field, mapper, report)),
        FieldValue::Struct(fields) => fields
            .iter_mut()
            .all(|(_, value)| remap_typed_formkeys(value, effect_index, field, mapper, report)),
        _ => true,
    }
}

fn has_named_field(
    fields: &[(crate::sym::Sym, FieldValue)],
    interner: &crate::sym::StringInterner,
    name: &str,
) -> bool {
    named_field(fields, interner, name).is_some()
}

fn named_field<'a>(
    fields: &'a [(crate::sym::Sym, FieldValue)],
    interner: &crate::sym::StringInterner,
    name: &str,
) -> Option<&'a FieldValue> {
    fields
        .iter()
        .find(|(key, _)| interner.resolve(*key) == Some(name))
        .map(|(_, value)| value)
}

fn named_number(
    fields: &[(crate::sym::Sym, FieldValue)],
    interner: &crate::sym::StringInterner,
    name: &str,
) -> Option<f64> {
    match named_field(fields, interner, name)? {
        FieldValue::Float(value) => Some(f64::from(*value)),
        FieldValue::Uint(value) => Some(*value as f64),
        FieldValue::Int(value) => Some(*value as f64),
        _ => None,
    }
}

fn named_u32(
    fields: &[(crate::sym::Sym, FieldValue)],
    interner: &crate::sym::StringInterner,
    name: &str,
) -> Option<u32> {
    match named_field(fields, interner, name)? {
        FieldValue::Uint(value) => u32::try_from(*value).ok(),
        FieldValue::Int(value) => u32::try_from(*value).ok(),
        FieldValue::Float(value) if value.is_finite() && *value >= 0.0 => Some(*value as u32),
        _ => None,
    }
}

fn named_i64(
    fields: &[(crate::sym::Sym, FieldValue)],
    interner: &crate::sym::StringInterner,
    name: &str,
) -> Option<i64> {
    match named_field(fields, interner, name)? {
        FieldValue::Int(value) => Some(*value),
        FieldValue::Uint(value) => i64::try_from(*value).ok(),
        FieldValue::Float(value) if value.is_finite() => Some(*value as i64),
        _ => None,
    }
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
    use crate::ids::SigCode;
    use crate::sym::StringInterner;

    fn source_plugin(family: LegacyMagicFamily) -> &'static str {
        match family {
            LegacyMagicFamily::Fnv => "FalloutNV.esm",
            LegacyMagicFamily::Fo3 => "Fallout3.esm",
        }
    }

    fn form_key(interner: &StringInterner, plugin: &str, local: u32) -> FormKey {
        FormKey {
            local,
            plugin: interner.intern(plugin),
        }
    }

    fn mapper<'a>(
        interner: &'a StringInterner,
        family: LegacyMagicFamily,
        mappings: &[(u32, u32)],
    ) -> FormKeyMapper<'a> {
        let mut mapper = FormKeyMapper::new(
            std::iter::empty(),
            MapperOptions {
                output_plugin_name: "Converted.esm".into(),
                source_plugin_name: source_plugin(family).into(),
                target_master_names: vec!["Fallout4.esm".into()],
                ..MapperOptions::default()
            },
            interner,
        );
        for (source, target) in mappings {
            mapper.add_mapping(
                form_key(interner, source_plugin(family), *source),
                form_key(interner, "Fallout4.esm", *target),
            );
        }
        mapper
    }

    fn record(interner: &StringInterner, sig: &[u8; 4]) -> Record {
        Record::new(SigCode(*sig), form_key(interner, "Converted.esm", 0x800))
    }

    fn field(sig: &[u8; 4], bytes: Vec<u8>) -> FieldEntry {
        FieldEntry {
            sig: SubrecordSig(*sig),
            value: FieldValue::Bytes(SmallVec::from_vec(bytes)),
        }
    }

    fn efid(raw: u32) -> FieldEntry {
        field(b"EFID", raw.to_le_bytes().to_vec())
    }

    fn legacy_efit(magnitude: u32, area: u32, duration: u32) -> FieldEntry {
        let mut bytes = vec![0_u8; LEGACY_EFIT_LEN];
        set_u32(&mut bytes, 0, magnitude);
        set_u32(&mut bytes, 4, area);
        set_u32(&mut bytes, 8, duration);
        field(b"EFIT", bytes)
    }

    fn legacy_ctda(function: u16) -> Vec<u8> {
        let mut bytes = vec![0_u8; LEGACY_CTDA_LEN];
        bytes[8..10].copy_from_slice(&function.to_le_bytes());
        bytes
    }

    fn raw_fields<'a>(record: &'a Record, sig: &[u8; 4]) -> Vec<&'a [u8]> {
        record
            .fields
            .iter()
            .filter(|entry| entry.sig.0 == *sig)
            .map(|entry| match &entry.value {
                FieldValue::Bytes(bytes) => bytes.as_slice(),
                other => panic!("expected raw {sig:?}, got {other:?}"),
            })
            .collect()
    }

    fn sigs(record: &Record) -> Vec<&str> {
        record
            .fields
            .iter()
            .map(|entry| entry.sig.as_str())
            .collect()
    }

    #[test]
    fn cook_cooks_fiend_stew_golden_preserves_three_ordered_effects_for_fnv_and_fo3() {
        for family in [LegacyMagicFamily::Fnv, LegacyMagicFamily::Fo3] {
            let interner = StringInterner::new();
            let mut mapper = mapper(
                &interner,
                family,
                &[
                    (0x00014E, 0x00397E),
                    (0x01515C, 0x04B268),
                    (0x162BCC, 0x0A7922),
                ],
            );
            let mut alch = record(&interner, b"ALCH");
            let mut enit = vec![0_u8; LEGACY_ALCH_ENIT_LEN];
            set_u32(&mut enit, 0, 25);
            enit[4] = 2;
            alch.fields.extend([
                field(b"ENIT", enit),
                efid(0x00014E),
                legacy_efit(2, 0, 60),
                efid(0x01515C),
                legacy_efit(1, 0, 120),
                efid(0x162BCC),
                legacy_efit(80, 0, 0),
                field(b"CTDA", legacy_ctda(12)),
            ]);

            let report = normalize_legacy_magic_effects(&mut alch, family, &mut mapper);

            assert_eq!(
                sigs(&alch),
                vec![
                    "ENIT", "EFID", "EFIT", "EFID", "EFIT", "EFID", "EFIT", "CTDA"
                ]
            );
            assert_eq!(report.converted_effects, 3);
            assert_eq!(report.converted_conditions, 1);
            assert_eq!(report.dropped_effects, 0);
            let effects = raw_fields(&alch, b"EFIT");
            assert_eq!(effects.len(), 3);
            assert_eq!(
                effects
                    .iter()
                    .map(|bytes| f32::from_le_bytes(bytes[0..4].try_into().unwrap()))
                    .collect::<Vec<_>>(),
                vec![2.0, 1.0, 80.0]
            );
            assert!(effects.iter().all(|bytes| bytes.len() == FO4_EFIT_LEN));
            let condition = raw_fields(&alch, b"CTDA")[0];
            assert_eq!(condition.len(), FO4_CTDA_LEN);
            assert_eq!(read_u16(condition, 8), 12);
            assert_eq!(
                i32::from_le_bytes(condition[28..32].try_into().unwrap()),
                -1
            );
            let enit = raw_fields(&alch, b"ENIT")[0];
            assert_eq!(enit.len(), FO4_ALCH_ENIT_LEN);
            assert_eq!(read_u32(enit, 0), 25);
            assert_eq!(read_u32(enit, 4), 2);
        }
    }

    #[test]
    fn caesars_armor_golden_builds_apparel_enchantment_contract_for_fnv_and_fo3() {
        for family in [LegacyMagicFamily::Fnv, LegacyMagicFamily::Fo3] {
            let interner = StringInterner::new();
            let mut mapper = mapper(
                &interner,
                family,
                &[(0x031D74, 0x04B268), (0x134B25, 0x03693A)],
            );
            let mut ench = record(&interner, b"ENCH");
            let mut enit = vec![0_u8; LEGACY_ENCH_ENIT_LEN];
            set_u32(&mut enit, 0, 3);
            ench.fields.extend([
                field(b"ENIT", enit),
                efid(0x031D74),
                legacy_efit(5, 0, 0),
                efid(0x134B25),
                legacy_efit(5, 0, 0),
            ]);

            let report = normalize_legacy_magic_effects(&mut ench, family, &mut mapper);

            assert_eq!(report.converted_effects, 2);
            let enit = raw_fields(&ench, b"ENIT")[0];
            assert_eq!(enit.len(), FO4_ENCH_ENIT_LEN);
            assert_eq!(read_u32(enit, 4), 0);
            assert_eq!(read_u32(enit, 8), 0);
            assert_eq!(read_u32(enit, 16), 0);
            assert_eq!(read_u32(enit, 20), FO4_STAFF_ENCHANTMENT_TYPE);
            assert_eq!(raw_fields(&ench, b"EFIT").len(), 2);

            let mut weapon_enchantment = record(&interner, b"ENCH");
            let mut weapon_enit = vec![0_u8; LEGACY_ENCH_ENIT_LEN];
            set_u32(&mut weapon_enit, 0, 2);
            weapon_enchantment.fields.push(field(b"ENIT", weapon_enit));
            normalize_legacy_magic_effects(&mut weapon_enchantment, family, &mut mapper);
            assert_eq!(
                read_u32(raw_fields(&weapon_enchantment, b"ENIT")[0], 20),
                FO4_ENCHANTMENT_TYPE
            );
        }
    }

    #[test]
    fn alch_20_byte_legacy_enit_is_relaid_out_and_remapped() {
        for family in [LegacyMagicFamily::Fnv, LegacyMagicFamily::Fo3] {
            let interner = StringInterner::new();
            let mut mapper = mapper(&interner, family, &[(0x500, 0x1500), (0x600, 0x1600)]);
            let mut alch = record(&interner, b"ALCH");
            let mut source = vec![0_u8; LEGACY_ALCH_ENIT_LEN];
            set_u32(&mut source, 0, 25);
            source[4] = 0x07;
            set_u32(&mut source, 8, 0x500);
            source[12..16].copy_from_slice(&0.25_f32.to_le_bytes());
            set_u32(&mut source, 16, 0x600);
            alch.fields.push(field(b"ENIT", source));

            let report = normalize_legacy_magic_effects(&mut alch, family, &mut mapper);

            let target = raw_fields(&alch, b"ENIT")[0];
            assert_eq!(target.len(), FO4_ALCH_ENIT_LEN);
            assert_eq!(read_u32(target, 0), 25);
            assert_eq!(read_u32(target, 4), 0x0001_0003);
            assert_eq!(read_u32(target, 8), 0x1500);
            assert_eq!(f32::from_le_bytes(target[12..16].try_into().unwrap()), 0.25);
            assert_eq!(read_u32(target, 16), 0x1600);
            assert_eq!(report.converted_metadata_rows, 1);
            assert_eq!(report.preserved_target_metadata_rows, 0);
        }
    }

    #[test]
    fn doctor_limb_restoration_golden_expands_zero_spit_for_fnv_and_fo3() {
        for family in [LegacyMagicFamily::Fnv, LegacyMagicFamily::Fo3] {
            let interner = StringInterner::new();
            let mut mapper = mapper(&interner, family, &[(0x0CB05D, 0x00397E)]);
            let mut spell = record(&interner, b"SPEL");
            spell.fields.extend([
                field(b"SPIT", vec![0; LEGACY_SPEL_SPIT_LEN]),
                efid(0x0CB05D),
                legacy_efit(10, 0, 0),
            ]);

            let report = normalize_legacy_magic_effects(&mut spell, family, &mut mapper);

            assert_eq!(report.converted_metadata_rows, 1);
            assert_eq!(report.converted_effects, 1);
            assert_eq!(raw_fields(&spell, b"SPIT")[0], &[0; FO4_SPEL_SPIT_LEN]);
            assert_eq!(raw_fields(&spell, b"EFIT")[0].len(), FO4_EFIT_LEN);
        }
    }

    #[test]
    fn condition_references_use_mapper_and_keep_cis_rows_in_lockstep() {
        let family = LegacyMagicFamily::Fnv;
        let interner = StringInterner::new();
        let mut mapper = mapper(
            &interner,
            family,
            &[
                (0x100, 0x1100),
                (0x200, 0x1200),
                (0x300, 0x1300),
                (0x400, 0x1400),
            ],
        );
        let mut condition = legacy_ctda(72);
        condition[0] = 0x04;
        set_u32(&mut condition, 4, 0x100);
        set_u32(&mut condition, 12, 0x200);
        set_u32(&mut condition, 20, 2);
        set_u32(&mut condition, 24, 0x300);
        let mut spell = record(&interner, b"SPEL");
        spell.fields.extend([
            efid(0x400),
            legacy_efit(1, 0, 0),
            field(b"CTDA", condition),
            field(b"CIS1", b"first\0".to_vec()),
            field(b"CIS2", b"second\0".to_vec()),
        ]);

        let report = normalize_legacy_magic_effects(&mut spell, family, &mut mapper);

        assert_eq!(sigs(&spell), vec!["EFID", "EFIT", "CTDA", "CIS1", "CIS2"]);
        let condition = raw_fields(&spell, b"CTDA")[0];
        assert_eq!(read_u32(condition, 4), 0x1100);
        assert_eq!(read_u32(condition, 12), 0x1200);
        assert_eq!(read_u32(condition, 24), 0x1300);
        assert_eq!(report.converted_conditions, 1);
        assert!(report.references.iter().any(|decision| {
            decision.field == "condition_run_on_reference"
                && matches!(
                    decision.outcome,
                    MagicReferenceOutcome::MappedRaw {
                        source_raw: 0x300,
                        target_raw: 0x1300
                    }
                )
        }));
    }

    #[test]
    fn condition_function_ids_and_legacy_run_on_are_translated() {
        for (family, source_function, target_function) in [
            (LegacyMagicFamily::Fnv, 79, 629),
            (LegacyMagicFamily::Fo3, 391, 390),
            (LegacyMagicFamily::Fnv, 1030, 14),
        ] {
            let interner = StringInterner::new();
            let mut mapper = mapper(&interner, family, &[(0x200, 0x1200), (0x400, 0x1400)]);
            let mut condition = legacy_ctda(source_function);
            if source_function == 79 {
                set_u32(&mut condition, 12, 0x200);
            }
            set_u32(&mut condition, 20, 20);
            let mut spell = record(&interner, b"SPEL");
            spell
                .fields
                .extend([efid(0x400), legacy_efit(1, 0, 0), field(b"CTDA", condition)]);

            let report = normalize_legacy_magic_effects(&mut spell, family, &mut mapper);

            let condition = raw_fields(&spell, b"CTDA")[0];
            assert_eq!(read_u16(condition, 8), target_function);
            assert_eq!(read_u32(condition, 20), 0);
            if source_function == 79 {
                assert_eq!(read_u32(condition, 12), 0x1200);
            }
            assert_eq!(report.converted_conditions, 1);
            assert!(report.enums.iter().any(|decision| {
                decision.field == "condition_function"
                    && decision.source == u32::from(source_function)
                    && decision.target == u32::from(target_function)
            }));
        }
    }

    #[test]
    fn fo3_condition_parameter_two_uses_fo3_formid_contract() {
        let family = LegacyMagicFamily::Fo3;
        let interner = StringInterner::new();
        let mut mapper = mapper(
            &interner,
            family,
            &[(0x200, 0x1200), (0x300, 0x1300), (0x400, 0x1400)],
        );
        let mut condition = legacy_ctda(60);
        set_u32(&mut condition, 12, 0x200);
        set_u32(&mut condition, 16, 0x300);
        let mut spell = record(&interner, b"SPEL");
        spell
            .fields
            .extend([efid(0x400), legacy_efit(1, 0, 0), field(b"CTDA", condition)]);

        normalize_legacy_magic_effects(&mut spell, family, &mut mapper);

        let condition = raw_fields(&spell, b"CTDA")[0];
        assert_eq!(read_u32(condition, 12), 0x1200);
        assert_eq!(read_u32(condition, 16), 0x1300);
    }

    #[test]
    fn typed_condition_normalizes_function_run_on_and_parameter_three() {
        let family = LegacyMagicFamily::Fnv;
        let interner = StringInterner::new();
        let mut mapper = mapper(&interner, family, &[(0x400, 0x1400)]);
        let mut spell = record(&interner, b"SPEL");
        spell.fields.extend([
            efid(0x400),
            legacy_efit(1, 0, 0),
            FieldEntry {
                sig: SubrecordSig(*b"CTDA"),
                value: FieldValue::Struct(vec![
                    (interner.intern("function"), FieldValue::Uint(391)),
                    (interner.intern("run_on"), FieldValue::Uint(20)),
                ]),
            },
        ]);

        normalize_legacy_magic_effects(&mut spell, family, &mut mapper);

        let FieldValue::Struct(condition) = &spell
            .fields
            .iter()
            .find(|entry| entry.sig.0 == *b"CTDA")
            .expect("condition")
            .value
        else {
            panic!("expected typed condition");
        };
        assert_eq!(named_u32(condition, &interner, "function"), Some(390));
        assert_eq!(named_u32(condition, &interner, "run_on"), Some(0));
        assert_eq!(named_i64(condition, &interner, "parameter_3"), Some(-1));
    }

    #[test]
    fn unmapped_or_malformed_rows_drop_only_their_atomic_scope() {
        let family = LegacyMagicFamily::Fnv;
        let interner = StringInterner::new();
        let mut mapper = mapper(&interner, family, &[(0x400, 0x1400)]);
        let mut condition = legacy_ctda(72);
        set_u32(&mut condition, 12, 0xDEAD);
        let mut spell = record(&interner, b"SPEL");
        spell.fields.extend([
            efid(0x401),
            legacy_efit(1, 0, 0),
            efid(0x400),
            field(b"EFIT", vec![0; LEGACY_EFIT_LEN - 1]),
            efid(0x400),
            legacy_efit(2, 0, 0),
            field(b"CTDA", condition),
            field(b"CIS1", b"drop\0".to_vec()),
            field(b"FULL", b"boundary\0".to_vec()),
            field(b"CIS2", b"orphan\0".to_vec()),
        ]);

        let report = normalize_legacy_magic_effects(&mut spell, family, &mut mapper);

        assert_eq!(sigs(&spell), vec!["EFID", "EFIT", "FULL"]);
        assert_eq!(report.dropped_effects, 2);
        assert_eq!(report.dropped_conditions, 1);
        assert_eq!(report.orphan_condition_strings_dropped, 2);
    }

    #[test]
    fn dropped_effect_preserves_non_effect_tail_fields() {
        let family = LegacyMagicFamily::Fnv;
        let interner = StringInterner::new();
        let mut mapper = mapper(&interner, family, &[]);
        let mut spell = record(&interner, b"SPEL");
        spell.fields.extend([
            efid(0x401),
            legacy_efit(1, 0, 0),
            field(b"CTDA", legacy_ctda(12)),
            field(b"CIS1", b"drop\0".to_vec()),
            field(b"FULL", b"keep\0".to_vec()),
        ]);

        let report = normalize_legacy_magic_effects(&mut spell, family, &mut mapper);

        assert_eq!(sigs(&spell), vec!["FULL"]);
        assert_eq!(report.dropped_effects, 1);
        assert_eq!(report.dropped_conditions, 1);
        assert_eq!(report.orphan_condition_strings_dropped, 1);
    }

    #[test]
    fn proto_dropped_condition_function_removes_its_cis_rows_only() {
        let family = LegacyMagicFamily::Fnv;
        let interner = StringInterner::new();
        let mut mapper = mapper(&interner, family, &[(0x400, 0x1400)]);
        let mut spell = record(&interner, b"SPEL");
        spell.fields.extend([
            efid(0x400),
            legacy_efit(1, 0, 0),
            field(b"CTDA", legacy_ctda(607)),
            field(b"CIS1", b"drop\0".to_vec()),
            field(b"FULL", b"keep\0".to_vec()),
        ]);

        let report = normalize_legacy_magic_effects(&mut spell, family, &mut mapper);

        assert_eq!(sigs(&spell), vec!["EFID", "EFIT", "FULL"]);
        assert_eq!(report.dropped_conditions, 1);
        assert_eq!(report.orphan_condition_strings_dropped, 1);
    }

    #[test]
    fn target_sized_rows_are_preserved_and_malformed_metadata_is_removed() {
        let family = LegacyMagicFamily::Fnv;
        let interner = StringInterner::new();
        let mut mapper = mapper(&interner, family, &[]);
        for (sig, metadata_sig, target_len) in [
            (*b"ENCH", *b"ENIT", FO4_ENCH_ENIT_LEN),
            (*b"SPEL", *b"SPIT", FO4_SPEL_SPIT_LEN),
        ] {
            let mut target = record(&interner, &sig);
            target.fields.extend([
                field(&metadata_sig, vec![0xA5; target_len]),
                efid(0xDEAD),
                field(b"EFIT", vec![0x5A; FO4_EFIT_LEN]),
                field(b"CTDA", vec![0xC3; FO4_CTDA_LEN]),
                field(b"CIS1", b"keep\0".to_vec()),
            ]);
            let before = target.fields.clone();

            let report = normalize_legacy_magic_effects(&mut target, family, &mut mapper);

            assert_eq!(target.fields, before);
            assert_eq!(report.preserved_target_metadata_rows, 1);
            assert_eq!(report.preserved_target_effects, 1);
            assert_eq!(report.preserved_target_conditions, 1);
        }

        let mut target_alch = record(&interner, b"ALCH");
        target_alch.fields.push(FieldEntry {
            sig: SubrecordSig(*b"ENIT"),
            value: FieldValue::Struct(vec![
                (interner.intern("value"), FieldValue::Int(25)),
                (interner.intern("flags"), FieldValue::Uint(1)),
                (interner.intern("addiction"), FieldValue::Uint(0)),
                (interner.intern("addiction_chance"), FieldValue::Float(0.0)),
                (interner.intern("sound_consume"), FieldValue::Uint(0)),
            ]),
        });
        let before = target_alch.fields.clone();
        let report = normalize_legacy_magic_effects(&mut target_alch, family, &mut mapper);
        assert_eq!(target_alch.fields, before);
        assert_eq!(report.preserved_target_metadata_rows, 1);

        let mut malformed = record(&interner, b"ALCH");
        malformed
            .fields
            .push(field(b"ENIT", vec![0; LEGACY_ALCH_ENIT_LEN - 1]));
        let report = normalize_legacy_magic_effects(&mut malformed, family, &mut mapper);
        assert!(malformed.fields.is_empty());
        assert_eq!(report.dropped_metadata_rows, 1);
    }

    #[test]
    fn domains_and_reference_function_tables_stay_target_safe() {
        assert_eq!(LEGACY_EFIT_LEN, 20);
        assert_eq!(FO4_EFIT_LEN, 12);
        assert_eq!(LEGACY_CTDA_LEN, 28);
        assert_eq!(FO4_CTDA_LEN, 32);
        for table in [
            FNV_CTDA_PARAM1_FORMID_FUNCTIONS,
            FNV_CTDA_PARAM2_FORMID_FUNCTIONS,
            FO3_CTDA_PARAM1_FORMID_FUNCTIONS,
            FO3_CTDA_PARAM2_FORMID_FUNCTIONS,
        ] {
            assert!(table.windows(2).all(|pair| pair[0] < pair[1]));
            assert!(
                table
                    .iter()
                    .all(|function| *function <= FO4_MAX_CONDITION_FUNCTION_ID)
            );
        }
        for source in 0..=u8::MAX {
            let alch = translate_alch_flags(u32::from(source));
            assert_eq!(alch & !(0x03 | FO4_ALCH_MEDICINE_FLAG), 0);
            let spell = translate_spel_flags(u32::from(source));
            assert_eq!(
                spell
                    & !(1
                        | FO4_SPEL_PC_START_FLAG
                        | FO4_SPEL_IGNORE_LOS_FLAG
                        | FO4_SPEL_IGNORE_RESISTANCE_FLAG
                        | FO4_SPEL_NO_ABSORB_REFLECT_FLAG),
                0
            );
        }
        assert!(FNV_CTDA_PARAM1_FORMID_FUNCTIONS.binary_search(&42).is_ok());
        assert!(FO3_CTDA_PARAM1_FORMID_FUNCTIONS.binary_search(&42).is_ok());
        assert!(FO3_CTDA_PARAM2_FORMID_FUNCTIONS.binary_search(&60).is_ok());
        assert_eq!(
            translate_condition_function(LegacyMagicFamily::Fnv, 79),
            Some(629)
        );
        assert_eq!(
            translate_condition_function(LegacyMagicFamily::Fo3, 391),
            Some(390)
        );
        assert_eq!(
            translate_condition_function(LegacyMagicFamily::Fnv, 1030),
            Some(14)
        );
        assert_eq!(
            translate_condition_function(LegacyMagicFamily::Fo3, 1030),
            None
        );
        assert_eq!(
            translate_condition_function(LegacyMagicFamily::Fnv, 420),
            None
        );
        assert_eq!(
            translate_condition_function(LegacyMagicFamily::Fnv, 607),
            None
        );
    }

    fn set_u32(bytes: &mut [u8], offset: usize, value: u32) {
        bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }
}
