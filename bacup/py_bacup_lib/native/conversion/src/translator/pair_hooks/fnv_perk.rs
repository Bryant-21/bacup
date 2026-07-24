//! Semantic FNV/FO3 PERK conversion for FO4's serial mapper pass.
//!
//! Production calls this module exactly once per legacy PERK from the serial mapper pass. PERK
//! entries and condition companion rows are rebuilt as atomic scopes, and an unresolved source
//! reference is reported and removed with its owning scope instead of escaping as a source raw
//! FormID.

use smallvec::SmallVec;

use crate::formkey_mapper::FormKeyMapper;
use crate::ids::{FormKey, SubrecordSig};
use crate::record::{FieldEntry, FieldValue, Record};

pub const LEGACY_CTDA_LEN: usize = 28;
pub const FO4_CTDA_LEN: usize = 32;
pub const LEGACY_QUEST_DATA_LEN: usize = 8;
pub const FO4_QUEST_DATA_LEN: usize = 6;

const FO4_MAX_CONDITION_FUNCTION_ID: u16 = 817;

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
pub enum LegacyPerkFamily {
    Fnv,
    Fo3,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PerkReferenceOutcome {
    SourceNull,
    MappedRaw { source_raw: u32, target_raw: u32 },
    MappedTyped { source: FormKey, target: FormKey },
    UnmappedRaw { source_raw: u32 },
    UnmappedTyped { source: FormKey },
    UnsupportedValue,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PerkReferenceDecision {
    pub entry_index: Option<usize>,
    pub condition_index: Option<usize>,
    pub field: &'static str,
    pub outcome: PerkReferenceOutcome,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PerkDropReason {
    MissingEndMarker,
    MalformedHeader,
    MissingData,
    MalformedData,
    UnsupportedEntryType,
    UnsupportedEntryPoint,
    UnsupportedEntryFunction,
    UnsupportedScriptParameter,
    MalformedFunctionParameters,
    UnmappedRequiredReference,
    MalformedCondition,
    UnsupportedConditionFunction,
    UnmappedConditionReference,
    OrphanCompanion,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PerkDropDecision {
    pub entry_index: Option<usize>,
    pub condition_index: Option<usize>,
    pub reason: PerkDropReason,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PerkEnumDecision {
    pub entry_index: Option<usize>,
    pub field: &'static str,
    pub source: u32,
    pub target: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PerkNormalizeReport {
    pub converted_entries: usize,
    pub dropped_entries: usize,
    pub converted_conditions: usize,
    pub preserved_target_conditions: usize,
    pub dropped_conditions: usize,
    pub orphan_companions_dropped: usize,
    pub references: Vec<PerkReferenceDecision>,
    pub enums: Vec<PerkEnumDecision>,
    pub drops: Vec<PerkDropDecision>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum RowShape {
    Legacy,
    Target,
    Malformed,
}

#[derive(Clone, Copy)]
pub struct EntryPointTarget {
    pub value: u8,
    pub condition_tabs: u8,
}

/// Rebuild one FNV/FO3 PERK into FO4-compatible ordered scopes.
///
/// This is a single-pass legacy-source transform. PRKE and entry-point DATA are three bytes in
/// both the legacy and target formats, so a previously converted entry cannot be distinguished
/// from a legacy entry and must not be passed through this function again. Target-shaped quest
/// DATA and CTDA rows are preserved because their widths are unambiguous.
///
/// The mapper must contain every mapping that can be referenced by this record. A missing mapping
/// drops the smallest atomic owner (condition or entry) and is included in the returned report.
pub fn normalize_legacy_perk(
    record: &mut Record,
    family: LegacyPerkFamily,
    mapper: &mut FormKeyMapper<'_>,
) -> PerkNormalizeReport {
    let mut report = PerkNormalizeReport::default();
    if record.sig.0 != *b"PERK" {
        return report;
    }

    let input: Vec<FieldEntry> = record.fields.drain(..).collect();
    let mut output = Vec::with_capacity(input.len());
    let mut index = 0;
    let mut entry_index = 0;
    let mut top_condition_index = 0;
    while index < input.len() {
        match input[index].sig.0 {
            sig if sig == *b"PRKE" => {
                let next_entry = input[index + 1..]
                    .iter()
                    .position(|entry| entry.sig.0 == *b"PRKE")
                    .map(|offset| index + 1 + offset)
                    .unwrap_or(input.len());
                let end_marker = input[index..next_entry]
                    .iter()
                    .position(|entry| entry.sig.0 == *b"PRKF")
                    .map(|offset| index + offset);
                let end = end_marker.map_or(next_entry, |marker| marker + 1);
                if end_marker.is_none() {
                    drop_entry(entry_index, PerkDropReason::MissingEndMarker, &mut report);
                } else if let Some(converted) =
                    normalize_entry(&input[index..end], entry_index, family, mapper, &mut report)
                {
                    output.extend(converted);
                }
                entry_index += 1;
                index = end;
            }
            sig if sig == *b"CTDA" => {
                let end = condition_companion_end(&input, index);
                if let Some(value) = convert_condition(
                    &input[index].value,
                    None,
                    top_condition_index,
                    family,
                    mapper,
                    &mut report,
                ) {
                    output.push(FieldEntry {
                        sig: SubrecordSig(*b"CTDA"),
                        value,
                    });
                    output.extend_from_slice(&input[index + 1..end]);
                } else {
                    report.orphan_companions_dropped += end - index - 1;
                }
                top_condition_index += 1;
                index = end;
            }
            sig if is_entry_companion(sig)
                || matches!(sig, s if s == *b"CIS1" || s == *b"CIS2") =>
            {
                report.orphan_companions_dropped += 1;
                report.drops.push(PerkDropDecision {
                    entry_index: None,
                    condition_index: None,
                    reason: PerkDropReason::OrphanCompanion,
                });
                index += 1;
            }
            _ => {
                output.push(input[index].clone());
                index += 1;
            }
        }
    }

    record.fields = output.into_iter().collect();
    report
}

fn normalize_entry(
    group: &[FieldEntry],
    entry_index: usize,
    family: LegacyPerkFamily,
    mapper: &mut FormKeyMapper<'_>,
    report: &mut PerkNormalizeReport,
) -> Option<Vec<FieldEntry>> {
    let Some(data_index) = group.iter().position(|entry| entry.sig.0 == *b"DATA") else {
        drop_entry(entry_index, PerkDropReason::MissingData, report);
        return None;
    };
    if data_index != 1 {
        drop_entry(entry_index, PerkDropReason::MalformedHeader, report);
        return None;
    }
    let Some(entry_type) = entry_type(&group[0].value, mapper) else {
        drop_entry(entry_index, PerkDropReason::MalformedHeader, report);
        return None;
    };

    let mut output = vec![group[0].clone()];
    let data = match entry_type {
        0 => convert_quest_data(&group[data_index].value, entry_index, mapper, report),
        1 => remap_required_reference_value(
            &group[data_index].value,
            entry_index,
            "ability",
            mapper,
            report,
        ),
        2 => convert_entry_point_data(
            &group[data_index].value,
            entry_index,
            family,
            mapper,
            report,
        ),
        _ => {
            drop_entry(entry_index, PerkDropReason::UnsupportedEntryType, report);
            return None;
        }
    };
    let Some(data) = data else {
        if !report
            .drops
            .iter()
            .any(|drop| drop.entry_index == Some(entry_index))
        {
            drop_entry(entry_index, PerkDropReason::MalformedData, report);
        }
        return None;
    };
    output.push(FieldEntry {
        sig: SubrecordSig(*b"DATA"),
        value: data,
    });

    let marker_index = group.len() - 1;
    let function_start = group[data_index + 1..marker_index]
        .iter()
        .position(|entry| entry.sig.0 == *b"EPFT")
        .map(|offset| data_index + 1 + offset)
        .unwrap_or(marker_index);
    if !normalize_entry_conditions(
        &group[data_index + 1..function_start],
        entry_index,
        family,
        mapper,
        report,
        &mut output,
    ) {
        drop_entry(entry_index, PerkDropReason::MalformedCondition, report);
        return None;
    }
    if function_start < marker_index
        && !normalize_function_parameters(
            &group[function_start..marker_index],
            entry_index,
            mapper,
            report,
            &mut output,
        )
    {
        return None;
    }
    output.push(group[marker_index].clone());
    report.converted_entries += 1;
    Some(output)
}

fn normalize_entry_conditions(
    fields: &[FieldEntry],
    entry_index: usize,
    family: LegacyPerkFamily,
    mapper: &mut FormKeyMapper<'_>,
    report: &mut PerkNormalizeReport,
    output: &mut Vec<FieldEntry>,
) -> bool {
    let mut index = 0;
    let mut condition_index = 0;
    while index < fields.len() {
        if fields[index].sig.0 != *b"PRKC" {
            return false;
        }
        let run_on = fields[index].clone();
        index += 1;
        let start_len = output.len();
        output.push(run_on);
        let mut kept = 0;
        while index < fields.len() && fields[index].sig.0 != *b"PRKC" {
            if fields[index].sig.0 != *b"CTDA" {
                output.truncate(start_len);
                return false;
            }
            let end = condition_companion_end(fields, index);
            if let Some(value) = convert_condition(
                &fields[index].value,
                Some(entry_index),
                condition_index,
                family,
                mapper,
                report,
            ) {
                output.push(FieldEntry {
                    sig: SubrecordSig(*b"CTDA"),
                    value,
                });
                output.extend_from_slice(&fields[index + 1..end]);
                kept += 1;
            } else {
                report.orphan_companions_dropped += end - index - 1;
            }
            condition_index += 1;
            index = end;
        }
        if kept == 0 {
            output.truncate(start_len);
        }
    }
    true
}

fn normalize_function_parameters(
    fields: &[FieldEntry],
    entry_index: usize,
    mapper: &mut FormKeyMapper<'_>,
    report: &mut PerkNormalizeReport,
    output: &mut Vec<FieldEntry>,
) -> bool {
    if fields.is_empty() || fields[0].sig.0 != *b"EPFT" {
        drop_entry(
            entry_index,
            PerkDropReason::MalformedFunctionParameters,
            report,
        );
        return false;
    }
    let Some(source_type) = scalar_u8(&fields[0].value) else {
        drop_entry(
            entry_index,
            PerkDropReason::MalformedFunctionParameters,
            report,
        );
        return false;
    };
    if source_type == 4 {
        drop_entry(
            entry_index,
            PerkDropReason::UnsupportedScriptParameter,
            report,
        );
        return false;
    }
    let target_type = match source_type {
        0..=3 => source_type,
        5 => 8,
        _ => {
            drop_entry(
                entry_index,
                PerkDropReason::MalformedFunctionParameters,
                report,
            );
            return false;
        }
    };
    let mut epfd = None;
    let mut epfb = None;
    let mut epf2 = None;
    let mut epf3 = None;
    for entry in &fields[1..] {
        match entry.sig.0 {
            sig if sig == *b"EPFD" && epfd.is_none() => epfd = Some(entry.clone()),
            sig if sig == *b"EPFB" && epfb.is_none() => epfb = Some(entry.clone()),
            sig if sig == *b"EPF2" && epf2.is_none() => epf2 = Some(entry.clone()),
            sig if sig == *b"EPF3" && epf3.is_none() => epf3 = Some(entry.clone()),
            _ => {
                drop_entry(
                    entry_index,
                    PerkDropReason::MalformedFunctionParameters,
                    report,
                );
                return false;
            }
        }
    }
    let epfd_required = matches!(source_type, 1 | 2 | 3 | 5);
    if epfd_required != epfd.is_some() || epf2.is_some() || epf3.is_some() {
        drop_entry(
            entry_index,
            PerkDropReason::MalformedFunctionParameters,
            report,
        );
        return false;
    }
    if source_type == 3 {
        let Some(entry) = epfd.as_mut() else {
            unreachable!();
        };
        let Some(value) = remap_required_reference_value(
            &entry.value,
            entry_index,
            "leveled_item_parameter",
            mapper,
            report,
        ) else {
            drop_entry(
                entry_index,
                PerkDropReason::UnmappedRequiredReference,
                report,
            );
            return false;
        };
        entry.value = value;
    }
    if source_type == 5 {
        drop_entry(
            entry_index,
            PerkDropReason::MalformedFunctionParameters,
            report,
        );
        return false;
    }
    output.push(FieldEntry {
        sig: SubrecordSig(*b"EPFT"),
        value: scalar_u8_value(&fields[0].value, target_type),
    });
    if let Some(entry) = epfb {
        output.push(entry);
    }
    if let Some(entry) = epf2 {
        output.push(entry);
    }
    if let Some(entry) = epf3 {
        output.push(entry);
    }
    if let Some(entry) = epfd {
        output.push(entry);
    }
    if source_type != target_type {
        report.enums.push(PerkEnumDecision {
            entry_index: Some(entry_index),
            field: "function_parameter_type",
            source: u32::from(source_type),
            target: u32::from(target_type),
        });
    }
    true
}

fn convert_quest_data(
    value: &FieldValue,
    entry_index: usize,
    mapper: &mut FormKeyMapper<'_>,
    report: &mut PerkNormalizeReport,
) -> Option<FieldValue> {
    match value {
        FieldValue::Bytes(bytes) if bytes.len() == LEGACY_QUEST_DATA_LEN => {
            let mut target = vec![0_u8; FO4_QUEST_DATA_LEN];
            target[..4].copy_from_slice(&bytes[..4]);
            target[4..6].copy_from_slice(&u16::from(bytes[4]).to_le_bytes());
            if !remap_raw_reference_at(
                &mut target,
                0,
                Some(entry_index),
                None,
                "quest",
                mapper,
                report,
            ) {
                drop_entry(
                    entry_index,
                    PerkDropReason::UnmappedRequiredReference,
                    report,
                );
                return None;
            }
            Some(FieldValue::Bytes(SmallVec::from_vec(target)))
        }
        FieldValue::Bytes(bytes) if bytes.len() == FO4_QUEST_DATA_LEN => Some(value.clone()),
        FieldValue::Struct(_) => {
            let mut target = value.clone();
            if remap_typed_formkeys(
                &mut target,
                Some(entry_index),
                None,
                "quest",
                mapper,
                report,
            ) {
                Some(target)
            } else {
                drop_entry(
                    entry_index,
                    PerkDropReason::UnmappedRequiredReference,
                    report,
                );
                None
            }
        }
        _ => None,
    }
}

fn convert_entry_point_data(
    value: &FieldValue,
    entry_index: usize,
    family: LegacyPerkFamily,
    mapper: &FormKeyMapper<'_>,
    report: &mut PerkNormalizeReport,
) -> Option<FieldValue> {
    match value {
        FieldValue::Bytes(bytes) if bytes.len() == 3 => {
            let Some(target) = map_legacy_entry_point(family, bytes[0]) else {
                drop_entry(entry_index, PerkDropReason::UnsupportedEntryPoint, report);
                return None;
            };
            let Some(function) = map_entry_function(bytes[1]) else {
                drop_entry(
                    entry_index,
                    PerkDropReason::UnsupportedEntryFunction,
                    report,
                );
                return None;
            };
            report.enums.push(PerkEnumDecision {
                entry_index: Some(entry_index),
                field: "entry_point",
                source: u32::from(bytes[0]),
                target: u32::from(target.value),
            });
            report.enums.push(PerkEnumDecision {
                entry_index: Some(entry_index),
                field: "entry_function",
                source: u32::from(bytes[1]),
                target: u32::from(function),
            });
            Some(FieldValue::Bytes(SmallVec::from_slice(&[
                target.value,
                function,
                target.condition_tabs,
            ])))
        }
        FieldValue::Struct(fields) => {
            let source_entry = named_u8(fields, mapper, "entry_point_entry_point")?;
            let source_function = named_u8(fields, mapper, "entry_point_function")?;
            let target = map_legacy_entry_point(family, source_entry)?;
            let function = map_entry_function(source_function)?;
            let mut fields = fields.clone();
            set_named_number(&mut fields, mapper, "entry_point_entry_point", target.value)?;
            set_named_number(&mut fields, mapper, "entry_point_function", function)?;
            set_named_number(
                &mut fields,
                mapper,
                "entry_point_perk_condition_tab_count",
                target.condition_tabs,
            )?;
            report.enums.push(PerkEnumDecision {
                entry_index: Some(entry_index),
                field: "entry_point",
                source: u32::from(source_entry),
                target: u32::from(target.value),
            });
            report.enums.push(PerkEnumDecision {
                entry_index: Some(entry_index),
                field: "entry_function",
                source: u32::from(source_function),
                target: u32::from(function),
            });
            Some(FieldValue::Struct(fields))
        }
        _ => None,
    }
}

fn convert_condition(
    value: &FieldValue,
    entry_index: Option<usize>,
    condition_index: usize,
    family: LegacyPerkFamily,
    mapper: &mut FormKeyMapper<'_>,
    report: &mut PerkNormalizeReport,
) -> Option<FieldValue> {
    match condition_shape(value, mapper) {
        RowShape::Target => {
            report.preserved_target_conditions += 1;
            Some(value.clone())
        }
        RowShape::Malformed => {
            drop_condition(
                entry_index,
                condition_index,
                PerkDropReason::MalformedCondition,
                report,
            );
            None
        }
        RowShape::Legacy => match value {
            FieldValue::Bytes(bytes) => {
                let source_function = read_u16(bytes, 8);
                let Some(function) = translate_condition_function(family, source_function) else {
                    drop_condition(
                        entry_index,
                        condition_index,
                        PerkDropReason::UnsupportedConditionFunction,
                        report,
                    );
                    return None;
                };
                let mut target = bytes.to_vec();
                target[8..10].copy_from_slice(&function.to_le_bytes());
                target.extend_from_slice(&(-1_i32).to_le_bytes());
                if target[0] & 0x04 != 0
                    && !remap_raw_reference_at(
                        &mut target,
                        4,
                        entry_index,
                        Some(condition_index),
                        "condition_comparison_global",
                        mapper,
                        report,
                    )
                {
                    drop_condition(
                        entry_index,
                        condition_index,
                        PerkDropReason::UnmappedConditionReference,
                        report,
                    );
                    return None;
                }
                let (param1, param2) = condition_formid_functions(family);
                if param1.binary_search(&source_function).is_ok()
                    && !remap_raw_reference_at(
                        &mut target,
                        12,
                        entry_index,
                        Some(condition_index),
                        "condition_parameter_1",
                        mapper,
                        report,
                    )
                {
                    drop_condition(
                        entry_index,
                        condition_index,
                        PerkDropReason::UnmappedConditionReference,
                        report,
                    );
                    return None;
                }
                if param2.binary_search(&source_function).is_ok()
                    && !remap_raw_reference_at(
                        &mut target,
                        16,
                        entry_index,
                        Some(condition_index),
                        "condition_parameter_2",
                        mapper,
                        report,
                    )
                {
                    drop_condition(
                        entry_index,
                        condition_index,
                        PerkDropReason::UnmappedConditionReference,
                        report,
                    );
                    return None;
                }
                let run_on = read_u32(&target, 20);
                if run_on == 20 {
                    target[20..24].copy_from_slice(&0_u32.to_le_bytes());
                }
                if run_on == 2
                    && !remap_raw_reference_at(
                        &mut target,
                        24,
                        entry_index,
                        Some(condition_index),
                        "condition_run_on_reference",
                        mapper,
                        report,
                    )
                {
                    drop_condition(
                        entry_index,
                        condition_index,
                        PerkDropReason::UnmappedConditionReference,
                        report,
                    );
                    return None;
                }
                report.enums.push(PerkEnumDecision {
                    entry_index,
                    field: "condition_function",
                    source: u32::from(source_function),
                    target: u32::from(function),
                });
                report.converted_conditions += 1;
                Some(FieldValue::Bytes(SmallVec::from_vec(target)))
            }
            FieldValue::Struct(fields) => {
                let source_function = named_u16(fields, mapper, "function")?;
                let function = translate_condition_function(family, source_function)?;
                let mut target = FieldValue::Struct(fields.clone());
                if !remap_typed_formkeys(
                    &mut target,
                    entry_index,
                    Some(condition_index),
                    "condition_reference",
                    mapper,
                    report,
                ) {
                    drop_condition(
                        entry_index,
                        condition_index,
                        PerkDropReason::UnmappedConditionReference,
                        report,
                    );
                    return None;
                }
                let FieldValue::Struct(fields) = &mut target else {
                    unreachable!();
                };
                set_named_number_u16(fields, mapper, "function", function)?;
                if named_u32(fields, mapper, "run_on") == Some(20) {
                    set_named_number_u32(fields, mapper, "run_on", 0)?;
                }
                fields.push((mapper.interner.intern("parameter_3"), FieldValue::Int(-1)));
                report.enums.push(PerkEnumDecision {
                    entry_index,
                    field: "condition_function",
                    source: u32::from(source_function),
                    target: u32::from(function),
                });
                report.converted_conditions += 1;
                Some(target)
            }
            _ => unreachable!(),
        },
    }
}

pub const fn map_legacy_entry_point(
    family: LegacyPerkFamily,
    source: u8,
) -> Option<EntryPointTarget> {
    let mapped = match source {
        0 => (35, 3),
        1 => (1, 3),
        2 => (2, 3),
        3 => (79, 2),
        4 => (3, 2),
        6 => (115, 3),
        7 => (141, 2),
        8 => (123, 3),
        9 => (22, 1),
        11 => (5, 1),
        12 => (6, 1),
        15 => (7, 2),
        17 => (8, 2),
        21 => (9, 2),
        22 => (10, 1),
        23 => (11, 1),
        24 => (12, 1),
        25 => (13, 1),
        27 => (14, 2),
        29 => (155, 1),
        31 => (15, 1),
        32 => (16, 1),
        33 => (109, 1),
        34 => (134, 2),
        36 => (17, 3),
        38 if matches!(family, LegacyPerkFamily::Fnv) => (97, 2),
        40 if matches!(family, LegacyPerkFamily::Fnv) => (79, 2),
        56 if matches!(family, LegacyPerkFamily::Fnv) => (37, 3),
        57 if matches!(family, LegacyPerkFamily::Fnv) => (145, 2),
        59 if matches!(family, LegacyPerkFamily::Fnv) => (103, 2),
        72 if matches!(family, LegacyPerkFamily::Fnv) => (100, 1),
        _ => return None,
    };
    Some(EntryPointTarget {
        value: mapped.0,
        condition_tabs: mapped.1,
    })
}

pub const fn map_entry_function(source: u8) -> Option<u8> {
    match source {
        1..=9 => Some(source),
        _ => None,
    }
}

fn condition_shape(value: &FieldValue, mapper: &FormKeyMapper<'_>) -> RowShape {
    match value {
        FieldValue::Bytes(bytes) if bytes.len() == LEGACY_CTDA_LEN => RowShape::Legacy,
        FieldValue::Bytes(bytes) if bytes.len() == FO4_CTDA_LEN => RowShape::Target,
        FieldValue::Struct(fields) if named_field(fields, mapper, "parameter_3").is_some() => {
            RowShape::Target
        }
        FieldValue::Struct(fields) if named_field(fields, mapper, "function").is_some() => {
            RowShape::Legacy
        }
        _ => RowShape::Malformed,
    }
}

fn condition_formid_functions(family: LegacyPerkFamily) -> (&'static [u16], &'static [u16]) {
    match family {
        LegacyPerkFamily::Fnv => (
            FNV_CTDA_PARAM1_FORMID_FUNCTIONS,
            FNV_CTDA_PARAM2_FORMID_FUNCTIONS,
        ),
        LegacyPerkFamily::Fo3 => (
            FO3_CTDA_PARAM1_FORMID_FUNCTIONS,
            FO3_CTDA_PARAM2_FORMID_FUNCTIONS,
        ),
    }
}

pub const fn translate_condition_function(family: LegacyPerkFamily, source: u16) -> Option<u16> {
    if contains_u16(DROPPED_LEGACY_CONDITION_FUNCTIONS, source)
        || matches!(family, LegacyPerkFamily::Fnv) && matches!(source, 420 | 421)
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
        1030 if matches!(family, LegacyPerkFamily::Fnv) => 14,
        5993 if matches!(family, LegacyPerkFamily::Fnv) => 672,
        6013 if matches!(family, LegacyPerkFamily::Fnv) => 801,
        6204 if matches!(family, LegacyPerkFamily::Fnv) => 329,
        value if value <= FO4_MAX_CONDITION_FUNCTION_ID => value,
        _ => return None,
    };
    Some(target)
}

const fn contains_u16(values: &[u16], needle: u16) -> bool {
    let mut index = 0;
    while index < values.len() {
        if values[index] == needle {
            return true;
        }
        index += 1;
    }
    false
}

fn remap_required_reference_value(
    value: &FieldValue,
    entry_index: usize,
    field: &'static str,
    mapper: &mut FormKeyMapper<'_>,
    report: &mut PerkNormalizeReport,
) -> Option<FieldValue> {
    match value {
        FieldValue::FormKey(source) if source.local == 0 => {
            report.references.push(PerkReferenceDecision {
                entry_index: Some(entry_index),
                condition_index: None,
                field,
                outcome: PerkReferenceOutcome::SourceNull,
            });
            None
        }
        FieldValue::FormKey(source) => match mapper.lookup(*source) {
            Some(target) => {
                report.references.push(PerkReferenceDecision {
                    entry_index: Some(entry_index),
                    condition_index: None,
                    field,
                    outcome: PerkReferenceOutcome::MappedTyped {
                        source: *source,
                        target,
                    },
                });
                Some(FieldValue::FormKey(target))
            }
            None => {
                report.references.push(PerkReferenceDecision {
                    entry_index: Some(entry_index),
                    condition_index: None,
                    field,
                    outcome: PerkReferenceOutcome::UnmappedTyped { source: *source },
                });
                None
            }
        },
        FieldValue::Bytes(bytes) if bytes.len() == 4 => {
            let mut target = bytes.to_vec();
            if remap_raw_reference_at(
                &mut target,
                0,
                Some(entry_index),
                None,
                field,
                mapper,
                report,
            ) && read_u32(&target, 0) != 0
            {
                Some(FieldValue::Bytes(SmallVec::from_vec(target)))
            } else {
                None
            }
        }
        FieldValue::Struct(_) => {
            let mut target = value.clone();
            remap_typed_formkeys(&mut target, Some(entry_index), None, field, mapper, report)
                .then_some(target)
        }
        _ => {
            report.references.push(PerkReferenceDecision {
                entry_index: Some(entry_index),
                condition_index: None,
                field,
                outcome: PerkReferenceOutcome::UnsupportedValue,
            });
            None
        }
    }
}

fn remap_raw_reference_at(
    bytes: &mut [u8],
    offset: usize,
    entry_index: Option<usize>,
    condition_index: Option<usize>,
    field: &'static str,
    mapper: &mut FormKeyMapper<'_>,
    report: &mut PerkNormalizeReport,
) -> bool {
    let source_raw = read_u32(bytes, offset);
    if source_raw == 0 {
        report.references.push(PerkReferenceDecision {
            entry_index,
            condition_index,
            field,
            outcome: PerkReferenceOutcome::SourceNull,
        });
        return true;
    }
    match mapper.rewrite_raw_formid_at(bytes, offset) {
        Some(_) => {
            report.references.push(PerkReferenceDecision {
                entry_index,
                condition_index,
                field,
                outcome: PerkReferenceOutcome::MappedRaw {
                    source_raw,
                    target_raw: read_u32(bytes, offset),
                },
            });
            true
        }
        None => {
            report.references.push(PerkReferenceDecision {
                entry_index,
                condition_index,
                field,
                outcome: PerkReferenceOutcome::UnmappedRaw { source_raw },
            });
            false
        }
    }
}

fn remap_typed_formkeys(
    value: &mut FieldValue,
    entry_index: Option<usize>,
    condition_index: Option<usize>,
    field: &'static str,
    mapper: &mut FormKeyMapper<'_>,
    report: &mut PerkNormalizeReport,
) -> bool {
    match value {
        FieldValue::FormKey(source) if source.local == 0 => true,
        FieldValue::FormKey(source) => match mapper.lookup(*source) {
            Some(target) => {
                report.references.push(PerkReferenceDecision {
                    entry_index,
                    condition_index,
                    field,
                    outcome: PerkReferenceOutcome::MappedTyped {
                        source: *source,
                        target,
                    },
                });
                *source = target;
                true
            }
            None => {
                report.references.push(PerkReferenceDecision {
                    entry_index,
                    condition_index,
                    field,
                    outcome: PerkReferenceOutcome::UnmappedTyped { source: *source },
                });
                false
            }
        },
        FieldValue::List(items) => items.iter_mut().all(|item| {
            remap_typed_formkeys(item, entry_index, condition_index, field, mapper, report)
        }),
        FieldValue::Struct(fields) => fields.iter_mut().all(|(_, value)| {
            remap_typed_formkeys(value, entry_index, condition_index, field, mapper, report)
        }),
        _ => true,
    }
}

fn entry_type(value: &FieldValue, mapper: &FormKeyMapper<'_>) -> Option<u8> {
    match value {
        FieldValue::Bytes(bytes) if bytes.len() == 3 => Some(bytes[0]),
        FieldValue::Struct(fields) => named_u8(fields, mapper, "type"),
        _ => None,
    }
}

fn condition_companion_end(fields: &[FieldEntry], index: usize) -> usize {
    fields[index + 1..]
        .iter()
        .position(|entry| !matches!(entry.sig.0, sig if sig == *b"CIS1" || sig == *b"CIS2"))
        .map(|offset| index + 1 + offset)
        .unwrap_or(fields.len())
}

fn is_entry_companion(sig: [u8; 4]) -> bool {
    matches!(
        sig,
        s if s == *b"PRKC"
            || s == *b"EPFT"
            || s == *b"EPFB"
            || s == *b"EPFD"
            || s == *b"EPF2"
            || s == *b"EPF3"
            || s == *b"SCHR"
            || s == *b"SCDA"
            || s == *b"SCTX"
            || s == *b"SLSD"
            || s == *b"SCVR"
            || s == *b"SCRO"
            || s == *b"SCRV"
            || s == *b"PRKF"
    )
}

fn drop_entry(entry_index: usize, reason: PerkDropReason, report: &mut PerkNormalizeReport) {
    if report
        .drops
        .iter()
        .any(|drop| drop.entry_index == Some(entry_index) && drop.condition_index.is_none())
    {
        return;
    }
    report.dropped_entries += 1;
    report.drops.push(PerkDropDecision {
        entry_index: Some(entry_index),
        condition_index: None,
        reason,
    });
}

fn drop_condition(
    entry_index: Option<usize>,
    condition_index: usize,
    reason: PerkDropReason,
    report: &mut PerkNormalizeReport,
) {
    report.dropped_conditions += 1;
    report.drops.push(PerkDropDecision {
        entry_index,
        condition_index: Some(condition_index),
        reason,
    });
}

fn scalar_u8(value: &FieldValue) -> Option<u8> {
    match value {
        FieldValue::Bytes(bytes) if bytes.len() == 1 => Some(bytes[0]),
        FieldValue::Uint(value) => u8::try_from(*value).ok(),
        FieldValue::Int(value) => u8::try_from(*value).ok(),
        _ => None,
    }
}

fn scalar_u8_value(template: &FieldValue, value: u8) -> FieldValue {
    match template {
        FieldValue::Uint(_) => FieldValue::Uint(u64::from(value)),
        FieldValue::Int(_) => FieldValue::Int(i64::from(value)),
        _ => FieldValue::Bytes(SmallVec::from_slice(&[value])),
    }
}

fn named_field<'a>(
    fields: &'a [(crate::sym::Sym, FieldValue)],
    mapper: &FormKeyMapper<'_>,
    name: &str,
) -> Option<&'a FieldValue> {
    fields
        .iter()
        .find(|(key, _)| mapper.interner.resolve(*key) == Some(name))
        .map(|(_, value)| value)
}

fn named_u8(
    fields: &[(crate::sym::Sym, FieldValue)],
    mapper: &FormKeyMapper<'_>,
    name: &str,
) -> Option<u8> {
    match named_field(fields, mapper, name)? {
        FieldValue::Uint(value) => u8::try_from(*value).ok(),
        FieldValue::Int(value) => u8::try_from(*value).ok(),
        _ => None,
    }
}

fn named_u16(
    fields: &[(crate::sym::Sym, FieldValue)],
    mapper: &FormKeyMapper<'_>,
    name: &str,
) -> Option<u16> {
    match named_field(fields, mapper, name)? {
        FieldValue::Uint(value) => u16::try_from(*value).ok(),
        FieldValue::Int(value) => u16::try_from(*value).ok(),
        _ => None,
    }
}

fn named_u32(
    fields: &[(crate::sym::Sym, FieldValue)],
    mapper: &FormKeyMapper<'_>,
    name: &str,
) -> Option<u32> {
    match named_field(fields, mapper, name)? {
        FieldValue::Uint(value) => u32::try_from(*value).ok(),
        FieldValue::Int(value) => u32::try_from(*value).ok(),
        _ => None,
    }
}

fn set_named_number(
    fields: &mut [(crate::sym::Sym, FieldValue)],
    mapper: &FormKeyMapper<'_>,
    name: &str,
    value: u8,
) -> Option<()> {
    let (_, current) = fields
        .iter_mut()
        .find(|(key, _)| mapper.interner.resolve(*key) == Some(name))?;
    *current = match current {
        FieldValue::Uint(_) => FieldValue::Uint(u64::from(value)),
        FieldValue::Int(_) => FieldValue::Int(i64::from(value)),
        _ => return None,
    };
    Some(())
}

fn set_named_number_u16(
    fields: &mut [(crate::sym::Sym, FieldValue)],
    mapper: &FormKeyMapper<'_>,
    name: &str,
    value: u16,
) -> Option<()> {
    let (_, current) = fields
        .iter_mut()
        .find(|(key, _)| mapper.interner.resolve(*key) == Some(name))?;
    *current = match current {
        FieldValue::Uint(_) => FieldValue::Uint(u64::from(value)),
        FieldValue::Int(_) => FieldValue::Int(i64::from(value)),
        _ => return None,
    };
    Some(())
}

fn set_named_number_u32(
    fields: &mut [(crate::sym::Sym, FieldValue)],
    mapper: &FormKeyMapper<'_>,
    name: &str,
    value: u32,
) -> Option<()> {
    let (_, current) = fields
        .iter_mut()
        .find(|(key, _)| mapper.interner.resolve(*key) == Some(name))?;
    *current = match current {
        FieldValue::Uint(_) => FieldValue::Uint(u64::from(value)),
        FieldValue::Int(_) => FieldValue::Int(i64::from(value)),
        _ => return None,
    };
    Some(())
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
    use crate::formkey_mapper::{MapperOptions, ResolutionMode};
    use crate::ids::SigCode;
    use crate::sym::StringInterner;

    fn source_plugin(family: LegacyPerkFamily) -> &'static str {
        match family {
            LegacyPerkFamily::Fnv => "FalloutNV.esm",
            LegacyPerkFamily::Fo3 => "Fallout3.esm",
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
        family: LegacyPerkFamily,
        mappings: &[(u32, u32)],
    ) -> FormKeyMapper<'a> {
        let mut mapper = FormKeyMapper::new(
            std::iter::empty(),
            MapperOptions {
                output_plugin_name: "Converted.esm".into(),
                source_plugin_name: source_plugin(family).into(),
                target_master_names: vec!["Fallout4.esm".into()],
                resolution_mode: ResolutionMode::Strict,
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

    fn record(interner: &StringInterner) -> Record {
        Record::new(
            SigCode(*b"PERK"),
            form_key(interner, "Converted.esm", 0x800),
        )
    }

    fn field(sig: &[u8; 4], bytes: impl Into<Vec<u8>>) -> FieldEntry {
        FieldEntry {
            sig: SubrecordSig(*sig),
            value: FieldValue::Bytes(SmallVec::from_vec(bytes.into())),
        }
    }

    fn empty(sig: &[u8; 4]) -> FieldEntry {
        FieldEntry {
            sig: SubrecordSig(*sig),
            value: FieldValue::None,
        }
    }

    fn legacy_ctda(function: u16) -> Vec<u8> {
        let mut bytes = vec![0_u8; LEGACY_CTDA_LEN];
        bytes[8..10].copy_from_slice(&function.to_le_bytes());
        bytes
    }

    fn sigs(record: &Record) -> Vec<&str> {
        record
            .fields
            .iter()
            .map(|entry| entry.sig.as_str())
            .collect()
    }

    fn raw<'a>(record: &'a Record, sig: &[u8; 4]) -> Vec<&'a [u8]> {
        record
            .fields
            .iter()
            .filter(|entry| entry.sig.0 == *sig)
            .map(|entry| match &entry.value {
                FieldValue::Bytes(bytes) => bytes.as_slice(),
                other => panic!("expected raw field, got {other:?}"),
            })
            .collect()
    }

    #[test]
    fn splash_damage_golden_maps_entry_semantics_and_ctda_width() {
        let interner = StringInterner::new();
        let mut mapper = mapper(&interner, LegacyPerkFamily::Fnv, &[]);
        let mut perk = record(&interner);
        let mut condition = legacy_ctda(495);
        condition[0] = 0x60;
        condition[4..8].copy_from_slice(&70.0_f32.to_le_bytes());
        condition[12..16].copy_from_slice(&35_u32.to_le_bytes());
        perk.fields.extend([
            field(b"CTDA", condition),
            field(b"DATA", vec![0, 12, 1, 1, 0]),
            field(b"PRKE", vec![2, 0, 0]),
            field(b"DATA", vec![72, 3, 2]),
            field(b"EPFT", vec![1]),
            field(b"EPFD", 1.25_f32.to_le_bytes().to_vec()),
            empty(b"PRKF"),
        ]);

        let report = normalize_legacy_perk(&mut perk, LegacyPerkFamily::Fnv, &mut mapper);

        assert_eq!(
            sigs(&perk),
            vec!["CTDA", "DATA", "PRKE", "DATA", "EPFT", "EPFD", "PRKF"]
        );
        let ctda = raw(&perk, b"CTDA")[0];
        assert_eq!(ctda.len(), FO4_CTDA_LEN);
        assert_eq!(read_u16(ctda, 8), 494);
        assert_eq!(i32::from_le_bytes(ctda[28..32].try_into().unwrap()), -1);
        assert_eq!(raw(&perk, b"DATA")[1], &[100, 3, 1]);
        assert_eq!(report.converted_entries, 1);
        assert_eq!(report.converted_conditions, 1);
        assert_eq!(report.dropped_entries, 0);
    }

    #[test]
    fn quest_ability_and_leveled_item_references_are_mapped_without_raw_leaks() {
        let interner = StringInterner::new();
        let mut mapper = mapper(
            &interner,
            LegacyPerkFamily::Fo3,
            &[(0x100, 0x200), (0x101, 0x201), (0x102, 0x202)],
        );
        let mut perk = record(&interner);
        let mut quest = vec![0_u8; LEGACY_QUEST_DATA_LEN];
        quest[..4].copy_from_slice(&0x100_u32.to_le_bytes());
        quest[4] = 42;
        perk.fields.extend([
            field(b"PRKE", vec![0, 0, 0]),
            field(b"DATA", quest),
            empty(b"PRKF"),
            field(b"PRKE", vec![1, 0, 0]),
            field(b"DATA", 0x101_u32.to_le_bytes().to_vec()),
            empty(b"PRKF"),
            field(b"PRKE", vec![2, 0, 0]),
            field(b"DATA", vec![21, 8, 2]),
            field(b"EPFT", vec![3]),
            field(b"EPFD", 0x102_u32.to_le_bytes().to_vec()),
            empty(b"PRKF"),
        ]);

        let report = normalize_legacy_perk(&mut perk, LegacyPerkFamily::Fo3, &mut mapper);

        let data = raw(&perk, b"DATA");
        assert_eq!(data[0].len(), FO4_QUEST_DATA_LEN);
        assert_eq!(read_u32(data[0], 0), 0x200);
        assert_eq!(u16::from_le_bytes(data[0][4..6].try_into().unwrap()), 42);
        assert_eq!(read_u32(data[1], 0), 0x201);
        assert_eq!(read_u32(raw(&perk, b"EPFD")[0], 0), 0x202);
        assert_eq!(report.converted_entries, 3);
        assert_eq!(report.references.len(), 3);
    }

    #[test]
    fn unmapped_required_reference_drops_the_whole_entry() {
        let interner = StringInterner::new();
        let mut mapper = mapper(&interner, LegacyPerkFamily::Fnv, &[]);
        let mut perk = record(&interner);
        perk.fields.extend([
            field(b"PRKE", vec![1, 0, 0]),
            field(b"DATA", 0x1234_u32.to_le_bytes().to_vec()),
            empty(b"PRKF"),
            field(b"DATA", vec![0, 1, 1, 1, 0]),
        ]);

        let report = normalize_legacy_perk(&mut perk, LegacyPerkFamily::Fnv, &mut mapper);

        assert_eq!(sigs(&perk), vec!["DATA"]);
        assert_eq!(report.dropped_entries, 1);
        assert!(matches!(
            report.references[0].outcome,
            PerkReferenceOutcome::UnmappedRaw { source_raw: 0x1234 }
        ));
    }

    #[test]
    fn unmapped_condition_drops_ctda_and_cis_companions_but_keeps_entry() {
        let interner = StringInterner::new();
        let mut mapper = mapper(&interner, LegacyPerkFamily::Fnv, &[]);
        let mut perk = record(&interner);
        let mut condition = legacy_ctda(42);
        condition[12..16].copy_from_slice(&0x440_u32.to_le_bytes());
        perk.fields.extend([
            field(b"PRKE", vec![2, 0, 0]),
            field(b"DATA", vec![0, 3, 3]),
            field(b"PRKC", vec![0]),
            field(b"CTDA", condition),
            field(b"CIS1", b"name\0".to_vec()),
            field(b"EPFT", vec![1]),
            field(b"EPFD", 0.75_f32.to_le_bytes().to_vec()),
            empty(b"PRKF"),
        ]);

        let report = normalize_legacy_perk(&mut perk, LegacyPerkFamily::Fnv, &mut mapper);

        assert_eq!(sigs(&perk), vec!["PRKE", "DATA", "EPFT", "EPFD", "PRKF"]);
        assert_eq!(report.converted_entries, 1);
        assert_eq!(report.dropped_conditions, 1);
        assert_eq!(report.orphan_companions_dropped, 1);
    }

    #[test]
    fn target_sized_condition_is_preserved_byte_for_byte() {
        let interner = StringInterner::new();
        let mut mapper = mapper(&interner, LegacyPerkFamily::Fnv, &[]);
        let mut perk = record(&interner);
        let target = vec![0xA5; FO4_CTDA_LEN];
        perk.fields.push(field(b"CTDA", target.clone()));

        let report = normalize_legacy_perk(&mut perk, LegacyPerkFamily::Fnv, &mut mapper);

        assert_eq!(raw(&perk, b"CTDA")[0], target);
        assert_eq!(report.preserved_target_conditions, 1);
        assert_eq!(report.converted_conditions, 0);
    }

    #[test]
    fn script_parameter_entry_is_dropped_with_every_companion() {
        let interner = StringInterner::new();
        let mut mapper = mapper(&interner, LegacyPerkFamily::Fo3, &[]);
        let mut perk = record(&interner);
        perk.fields.extend([
            field(b"PRKE", vec![2, 0, 0]),
            field(b"DATA", vec![27, 9, 2]),
            field(b"EPFT", vec![4]),
            empty(b"EPFD"),
            field(b"EPF2", b"Open\0".to_vec()),
            field(b"EPF3", vec![1, 0]),
            field(b"SCHR", vec![0; 20]),
            field(b"SCRO", 0x123_u32.to_le_bytes().to_vec()),
            empty(b"PRKF"),
        ]);

        let report = normalize_legacy_perk(&mut perk, LegacyPerkFamily::Fo3, &mut mapper);

        assert!(perk.fields.is_empty());
        assert_eq!(report.dropped_entries, 1);
        assert_eq!(
            report.drops[0].reason,
            PerkDropReason::UnsupportedScriptParameter
        );
    }

    #[test]
    fn fnv_and_fo3_entry_and_condition_domains_diverge_explicitly() {
        let fnv = map_legacy_entry_point(LegacyPerkFamily::Fnv, 72).unwrap();
        assert_eq!((fnv.value, fnv.condition_tabs), (100, 1));
        assert!(map_legacy_entry_point(LegacyPerkFamily::Fo3, 72).is_none());
        assert_eq!(
            translate_condition_function(LegacyPerkFamily::Fnv, 5993),
            Some(672)
        );
        assert_eq!(
            translate_condition_function(LegacyPerkFamily::Fo3, 5993),
            None
        );
    }

    #[test]
    fn mapped_entry_domain_is_target_valid_and_corpus_values_are_accounted_for() {
        let fnv_corpus = [
            0, 1, 2, 4, 6, 8, 9, 10, 11, 12, 20, 23, 25, 27, 28, 29, 31, 32, 33, 34, 35, 36, 37,
            38, 39, 40, 41, 42, 43, 44, 46, 47, 48, 51, 52, 53, 54, 55, 56, 57, 58, 59, 61, 62, 64,
            65, 66, 67, 68, 69, 70, 71, 72,
        ];
        let fo3_corpus = [
            0, 2, 4, 6, 8, 9, 10, 11, 17, 20, 21, 23, 25, 26, 27, 28, 29, 31, 32, 33, 35, 36,
        ];
        for family in [LegacyPerkFamily::Fnv, LegacyPerkFamily::Fo3] {
            for source in 0..=73 {
                if let Some(target) = map_legacy_entry_point(family, source) {
                    assert!(target.value <= 157);
                    assert!((1..=3).contains(&target.condition_tabs));
                }
            }
        }
        assert!(fnv_corpus.iter().all(|source| *source <= 72));
        assert!(fo3_corpus.iter().all(|source| *source <= 36));
        assert!(
            fnv_corpus
                .iter()
                .any(|source| map_legacy_entry_point(LegacyPerkFamily::Fnv, *source).is_none())
        );
        assert!(
            fo3_corpus
                .iter()
                .any(|source| map_legacy_entry_point(LegacyPerkFamily::Fo3, *source).is_none())
        );
    }

    #[test]
    fn malformed_entry_does_not_consume_the_following_valid_entry() {
        let interner = StringInterner::new();
        let mut mapper = mapper(&interner, LegacyPerkFamily::Fnv, &[]);
        let mut perk = record(&interner);
        perk.fields.extend([
            field(b"PRKE", vec![2, 0, 0]),
            field(b"DATA", vec![72, 3, 2]),
            field(b"EPFT", vec![1]),
            field(b"EPFD", 1.25_f32.to_le_bytes().to_vec()),
            field(b"PRKE", vec![2, 0, 0]),
            field(b"DATA", vec![0, 3, 3]),
            field(b"EPFT", vec![1]),
            field(b"EPFD", 0.75_f32.to_le_bytes().to_vec()),
            empty(b"PRKF"),
        ]);

        let report = normalize_legacy_perk(&mut perk, LegacyPerkFamily::Fnv, &mut mapper);

        assert_eq!(report.dropped_entries, 1);
        assert_eq!(report.converted_entries, 1);
        assert_eq!(raw(&perk, b"DATA")[0], &[35, 3, 3]);
    }
}
