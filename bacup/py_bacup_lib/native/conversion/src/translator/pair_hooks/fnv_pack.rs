//! Read-only FNV/FO3 PACK inventory for a future verified FO4 lowerer.
//!
//! Legacy PACK records and FO4 PACK records share several subrecord signatures, but their
//! payloads are not ABI-compatible. In particular, both PKDT layouts are 12 bytes while assigning
//! those bytes different meanings. This module therefore classifies legacy records without
//! mutating them and deliberately does not synthesize FO4 PKCU, XNAM, or procedure-tree data.

use std::collections::BTreeMap;

use serde::Serialize;

use crate::record::{FieldEntry, FieldValue, Record};
use crate::sym::StringInterner;

pub const LEGACY_PKDT_LEN: usize = 12;
pub const LEGACY_PSDT_LEN: usize = 8;
pub const LEGACY_CTDA_LEN: usize = 28;
pub const LEGACY_OLD_CTDA_LEN: usize = 20;
pub const LEGACY_LOCATION_LEN: usize = 12;
pub const LEGACY_TARGET_LEN: usize = 16;
pub const LEGACY_PKW3_LEN: usize = 24;
pub const LEGACY_SCHR_LEN: usize = 20;

pub const AUDITED_FNV_PACK_COUNT: usize = 4_888;
pub const AUDITED_FO3_PACK_COUNT: usize = 4_567;
pub const AUDITED_LEGACY_PACK_COUNT: usize = AUDITED_FNV_PACK_COUNT + AUDITED_FO3_PACK_COUNT;

const TYPE_SPECIFIC_SIGS: [[u8; 4]; 8] = [
    *b"PKED", *b"PKE2", *b"PKFD", *b"PKPT", *b"PKW3", *b"PUID", *b"PKAM", *b"PKDD",
];
const SCRIPT_MARKERS: [[u8; 4]; 3] = [*b"POBA", *b"POEA", *b"POCA"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LegacyPackSourceFamily {
    Fnv,
    Fo3,
    Fo76,
    Fo4,
}

impl LegacyPackSourceFamily {
    fn is_legacy(self) -> bool {
        matches!(self, Self::Fnv | Self::Fo3)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LegacyPackType {
    Find,
    Follow,
    Escort,
    Eat,
    Sleep,
    Wander,
    Travel,
    Accompany,
    UseItemAt,
    Ambush,
    FleeNotCombat,
    PackageType11,
    Sandbox,
    Patrol,
    Guard,
    Dialogue,
    UseWeapon,
}

impl LegacyPackType {
    pub const fn from_code(code: u8) -> Option<Self> {
        Some(match code {
            0 => Self::Find,
            1 => Self::Follow,
            2 => Self::Escort,
            3 => Self::Eat,
            4 => Self::Sleep,
            5 => Self::Wander,
            6 => Self::Travel,
            7 => Self::Accompany,
            8 => Self::UseItemAt,
            9 => Self::Ambush,
            10 => Self::FleeNotCombat,
            11 => Self::PackageType11,
            12 => Self::Sandbox,
            13 => Self::Patrol,
            14 => Self::Guard,
            15 => Self::Dialogue,
            16 => Self::UseWeapon,
            _ => return None,
        })
    }

    pub const fn code(self) -> u8 {
        match self {
            Self::Find => 0,
            Self::Follow => 1,
            Self::Escort => 2,
            Self::Eat => 3,
            Self::Sleep => 4,
            Self::Wander => 5,
            Self::Travel => 6,
            Self::Accompany => 7,
            Self::UseItemAt => 8,
            Self::Ambush => 9,
            Self::FleeNotCombat => 10,
            Self::PackageType11 => 11,
            Self::Sandbox => 12,
            Self::Patrol => 13,
            Self::Guard => 14,
            Self::Dialogue => 15,
            Self::UseWeapon => 16,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LegacyPackPkdt {
    pub observed_size: usize,
    pub general_flags: u32,
    pub package_type_code: u8,
    pub package_type: LegacyPackType,
    pub fallout_behavior_flags: u16,
    pub type_specific_flags: u16,
    pub unused_bytes_nonzero: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LegacyPackSchedule {
    pub observed_size: usize,
    pub month: i8,
    pub day_of_week: i8,
    pub date: i8,
    pub hour: i8,
    pub duration_hours: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LegacyPackLocationType {
    NearReference,
    InCell,
    NearCurrentLocation,
    NearEditorLocation,
    ObjectId,
    ObjectType,
    NearLinkedReference,
    AtPackageLocation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LegacyPackTargetType {
    SpecificReference,
    ObjectId,
    ObjectType,
    LinkedReference,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "domain", content = "kind", rename_all = "snake_case")]
pub enum LegacyPackUnionKind {
    Location(LegacyPackLocationType),
    Target(LegacyPackTargetType),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LegacyPackUnionPayload {
    Reference { present: bool },
    ObjectType { value: u32 },
    Implicit,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LegacyPackUnionInventory {
    pub sig: String,
    pub observed_size: usize,
    pub type_code: u32,
    pub union_kind: LegacyPackUnionKind,
    pub payload: LegacyPackUnionPayload,
    pub radius_or_distance: i32,
    pub trailing_unknown_nonzero: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LegacyPackConditionGroup {
    pub condition_index: usize,
    pub observed_size: usize,
    pub cis1_present: bool,
    pub cis2_present: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LegacyPackScriptEvent {
    OnBegin,
    OnEnd,
    OnChange,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LegacyPackScriptInventory {
    pub event: LegacyPackScriptEvent,
    pub header_size: usize,
    pub declared_reference_count: u32,
    pub declared_compiled_size: u32,
    pub declared_variable_count: u32,
    pub compiled_payload_size: usize,
    pub source_payload_size: usize,
    pub local_variable_rows: usize,
    pub named_local_variables: usize,
    pub global_references: usize,
    pub local_references: usize,
    pub idle_present: bool,
    pub topic_present: bool,
    pub compiled_size_matches: bool,
    pub reference_count_matches: bool,
    pub variable_count_matches: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LegacyPackSubrecordInventory {
    pub sig: String,
    pub count: usize,
    pub observed_sizes: Vec<usize>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LegacyPackUseWeaponData {
    pub observed_size: usize,
    pub flags: u32,
    pub fire_rate: u8,
    pub fire_count: u8,
    pub number_of_bursts: u16,
    pub shots_per_volley_min: u16,
    pub shots_per_volley_max: u16,
    pub pause_between_volleys_min: f32,
    pub pause_between_volleys_max: f32,
    pub unused_bytes_nonzero: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "reason", rename_all = "snake_case")]
pub enum LegacyPackLoweringBlocker {
    NoVerifiedFo4ProcedureBlueprint { package_type_code: u8 },
    LegacyConditionsRequireSemanticLowering,
    LegacyEventScriptsRequirePort,
    EncodedReferencesRequireMapper,
    ScriptAccountingMismatch { event: LegacyPackScriptEvent },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LegacyPackSupport {
    pub classification_supported: bool,
    pub lowering_supported: bool,
    pub lowering_blockers: Vec<LegacyPackLoweringBlocker>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LegacyPackInventory {
    pub source: LegacyPackSourceFamily,
    pub form_key: String,
    pub editor_id: Option<String>,
    pub package_type_code: u8,
    pub package_type: LegacyPackType,
    pub pkdt: LegacyPackPkdt,
    pub schedule: LegacyPackSchedule,
    pub conditions: Vec<LegacyPackConditionGroup>,
    pub unions: Vec<LegacyPackUnionInventory>,
    pub scripts: Vec<LegacyPackScriptInventory>,
    pub type_specific_subrecords: Vec<LegacyPackSubrecordInventory>,
    pub use_weapon_data: Option<LegacyPackUseWeaponData>,
    pub support: LegacyPackSupport,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LegacyPackClassificationStatus {
    NotApplicable,
    Accepted,
    Rejected,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "reason", rename_all = "snake_case")]
pub enum LegacyPackRejectionReason {
    UnresolvedRecordIdentity {
        field: String,
    },
    MissingRequiredSubrecord {
        sig: String,
    },
    DuplicateSubrecord {
        sig: String,
        count: usize,
    },
    MalformedSubrecord {
        sig: String,
        field_index: usize,
        expected_sizes: Vec<usize>,
        observed_size: Option<usize>,
    },
    UnknownPackageType {
        value: u64,
    },
    UnknownUnionType {
        sig: String,
        field_index: usize,
        value: u64,
    },
    DuplicateConditionCompanion {
        sig: String,
        condition_index: usize,
    },
    OrphanConditionCompanion {
        sig: String,
        field_index: usize,
    },
    MalformedScriptBlock {
        event: LegacyPackScriptEvent,
        issue: String,
    },
    OrphanScriptSubrecord {
        sig: String,
        field_index: usize,
    },
    UnknownSubrecord {
        sig: String,
        field_index: usize,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LegacyPackClassificationReport {
    pub source: LegacyPackSourceFamily,
    pub status: LegacyPackClassificationStatus,
    pub inventory: Option<LegacyPackInventory>,
    pub rejection_reasons: Vec<LegacyPackRejectionReason>,
}

impl LegacyPackClassificationReport {
    fn not_applicable(source: LegacyPackSourceFamily) -> Self {
        Self {
            source,
            status: LegacyPackClassificationStatus::NotApplicable,
            inventory: None,
            rejection_reasons: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LegacyPackCorpusReport {
    pub total_records: usize,
    pub fnv_records: usize,
    pub fo3_records: usize,
    pub accepted_records: usize,
    pub rejected_records: usize,
    pub by_type: BTreeMap<u8, usize>,
    pub exact_audited_coverage: bool,
}

/// Contract for a future lowerer once every FO4 procedure-tree blueprint is byte-verified.
///
/// Implementations must consume only an accepted inventory. This classifier intentionally has no
/// implementation of this trait because inventing PKCU, XNAM, or procedure-tree rows would turn a
/// safe audit into an unverified conversion.
pub trait LegacyPackLowerer {
    type Error;

    fn lower_supported_legacy_pack(
        &self,
        inventory: &LegacyPackInventory,
    ) -> Result<Record, Self::Error>;
}

/// Classify one source-shaped PACK without changing it or retaining raw payload bytes.
pub fn classify_legacy_pack(
    record: &Record,
    source: LegacyPackSourceFamily,
    interner: &StringInterner,
) -> LegacyPackClassificationReport {
    if record.sig.0 != *b"PACK" || !source.is_legacy() {
        return LegacyPackClassificationReport::not_applicable(source);
    }

    let mut rejections = Vec::new();
    for (field_index, field) in record.fields.iter().enumerate() {
        if !is_known_subrecord(field.sig.0) {
            rejections.push(LegacyPackRejectionReason::UnknownSubrecord {
                sig: field.sig.as_str().to_string(),
                field_index,
            });
        }
    }

    let form_key = if let Some(plugin) = interner.resolve(record.form_key.plugin) {
        format!("{:06X}@{plugin}", record.form_key.local)
    } else {
        rejections.push(LegacyPackRejectionReason::UnresolvedRecordIdentity {
            field: "form_key_plugin".to_string(),
        });
        String::new()
    };
    let editor_id = record
        .eid
        .and_then(|eid| interner.resolve(eid))
        .map(str::to_owned);
    if record.eid.is_some() && editor_id.is_none() {
        rejections.push(LegacyPackRejectionReason::UnresolvedRecordIdentity {
            field: "editor_id".to_string(),
        });
    }

    let pkdt = parse_unique_required(record, b"PKDT", interner, &mut rejections, parse_pkdt);
    let schedule =
        parse_unique_required(record, b"PSDT", interner, &mut rejections, parse_schedule);
    let conditions = classify_conditions(record, interner, &mut rejections);
    let unions = classify_unions(record, interner, &mut rejections);
    let (scripts, script_owned) = classify_scripts(record, interner, &mut rejections);
    reject_orphan_script_fields(record, &script_owned, &mut rejections);
    let (type_specific_subrecords, use_weapon_data) =
        classify_type_specific(record, interner, &mut rejections);

    if !rejections.is_empty() || pkdt.is_none() || schedule.is_none() {
        return LegacyPackClassificationReport {
            source,
            status: LegacyPackClassificationStatus::Rejected,
            inventory: None,
            rejection_reasons: rejections,
        };
    }

    let pkdt = pkdt.expect("checked above");
    let mut lowering_blockers = vec![LegacyPackLoweringBlocker::NoVerifiedFo4ProcedureBlueprint {
        package_type_code: pkdt.package_type_code,
    }];
    if !conditions.is_empty() {
        lowering_blockers.push(LegacyPackLoweringBlocker::LegacyConditionsRequireSemanticLowering);
    }
    if !scripts.is_empty() {
        lowering_blockers.push(LegacyPackLoweringBlocker::LegacyEventScriptsRequirePort);
    }
    if unions.iter().any(|union| {
        matches!(
            union.payload,
            LegacyPackUnionPayload::Reference { present: true }
        )
    }) {
        lowering_blockers.push(LegacyPackLoweringBlocker::EncodedReferencesRequireMapper);
    }
    for script in &scripts {
        if !(script.compiled_size_matches
            && script.reference_count_matches
            && script.variable_count_matches)
        {
            lowering_blockers.push(LegacyPackLoweringBlocker::ScriptAccountingMismatch {
                event: script.event,
            });
        }
    }

    let inventory = LegacyPackInventory {
        source,
        form_key,
        editor_id,
        package_type_code: pkdt.package_type_code,
        package_type: pkdt.package_type,
        pkdt,
        schedule: schedule.expect("checked above"),
        conditions,
        unions,
        scripts,
        type_specific_subrecords,
        use_weapon_data,
        support: LegacyPackSupport {
            classification_supported: true,
            lowering_supported: false,
            lowering_blockers,
        },
    };
    LegacyPackClassificationReport {
        source,
        status: LegacyPackClassificationStatus::Accepted,
        inventory: Some(inventory),
        rejection_reasons: Vec::new(),
    }
}

pub fn legacy_pack_type_hint(
    record: &Record,
    interner: &StringInterner,
) -> Option<(u8, LegacyPackType)> {
    let mut fields = record
        .fields
        .iter()
        .enumerate()
        .filter(|(_, field)| field.sig.0 == *b"PKDT");
    let (field_index, field) = fields.next()?;
    if fields.next().is_some() {
        return None;
    }
    let pkdt = parse_pkdt(&field.value, field_index, interner).ok()?;
    Some((pkdt.package_type_code, pkdt.package_type))
}

pub fn summarize_legacy_pack_reports<'a>(
    reports: impl IntoIterator<Item = &'a LegacyPackClassificationReport>,
) -> LegacyPackCorpusReport {
    let mut summary = LegacyPackCorpusReport {
        total_records: 0,
        fnv_records: 0,
        fo3_records: 0,
        accepted_records: 0,
        rejected_records: 0,
        by_type: BTreeMap::new(),
        exact_audited_coverage: false,
    };
    for report in reports {
        if report.status == LegacyPackClassificationStatus::NotApplicable {
            continue;
        }
        summary.total_records += 1;
        match report.source {
            LegacyPackSourceFamily::Fnv => summary.fnv_records += 1,
            LegacyPackSourceFamily::Fo3 => summary.fo3_records += 1,
            LegacyPackSourceFamily::Fo76 | LegacyPackSourceFamily::Fo4 => {}
        }
        match report.status {
            LegacyPackClassificationStatus::Accepted => {
                summary.accepted_records += 1;
                if let Some(inventory) = &report.inventory {
                    *summary
                        .by_type
                        .entry(inventory.package_type_code)
                        .or_default() += 1;
                }
            }
            LegacyPackClassificationStatus::Rejected => summary.rejected_records += 1,
            LegacyPackClassificationStatus::NotApplicable => {}
        }
    }
    summary.exact_audited_coverage = summary.total_records == AUDITED_LEGACY_PACK_COUNT
        && summary.fnv_records == AUDITED_FNV_PACK_COUNT
        && summary.fo3_records == AUDITED_FO3_PACK_COUNT;
    summary
}

fn parse_unique_required<T>(
    record: &Record,
    sig: &[u8; 4],
    interner: &StringInterner,
    rejections: &mut Vec<LegacyPackRejectionReason>,
    parse: fn(&FieldValue, usize, &StringInterner) -> Result<T, LegacyPackRejectionReason>,
) -> Option<T> {
    let fields = record
        .fields
        .iter()
        .enumerate()
        .filter(|(_, field)| field.sig.0 == *sig)
        .collect::<Vec<_>>();
    match fields.as_slice() {
        [] => {
            rejections.push(LegacyPackRejectionReason::MissingRequiredSubrecord {
                sig: sig_text(*sig),
            });
            None
        }
        [(field_index, field)] => match parse(&field.value, *field_index, interner) {
            Ok(value) => Some(value),
            Err(reason) => {
                rejections.push(reason);
                None
            }
        },
        _ => {
            rejections.push(LegacyPackRejectionReason::DuplicateSubrecord {
                sig: sig_text(*sig),
                count: fields.len(),
            });
            None
        }
    }
}

fn parse_pkdt(
    value: &FieldValue,
    field_index: usize,
    interner: &StringInterner,
) -> Result<LegacyPackPkdt, LegacyPackRejectionReason> {
    let (general_flags, type_code, fallout_behavior_flags, type_specific_flags, unused_nonzero) =
        match value {
            FieldValue::Bytes(bytes) if bytes.len() == LEGACY_PKDT_LEN => (
                read_u32(bytes, 0),
                bytes[4],
                read_u16(bytes, 6),
                read_u16(bytes, 8),
                bytes[5] != 0 || read_u16(bytes, 10) != 0,
            ),
            FieldValue::Struct(fields) => {
                let general_flags = struct_u32(fields, "general_flags", interner);
                let type_code = struct_package_type(fields, interner);
                let fallout_behavior_flags = struct_u16(fields, "fallout_behavior_flags", interner);
                let type_specific_flags = struct_u16(fields, "type_specific_flags", interner);
                let Some((general_flags, type_code, fallout_behavior_flags, type_specific_flags)) =
                    general_flags
                        .zip(type_code)
                        .zip(fallout_behavior_flags)
                        .zip(type_specific_flags)
                        .map(|(((a, b), c), d)| (a, b, c, d))
                else {
                    return Err(malformed("PKDT", field_index, &[LEGACY_PKDT_LEN], None));
                };
                let unused_nonzero = struct_u64(fields, "unused_1", interner).unwrap_or(0) != 0
                    || struct_u64(fields, "unused_2", interner).unwrap_or(0) != 0;
                (
                    general_flags,
                    type_code,
                    fallout_behavior_flags,
                    type_specific_flags,
                    unused_nonzero,
                )
            }
            other => {
                return Err(malformed(
                    "PKDT",
                    field_index,
                    &[LEGACY_PKDT_LEN],
                    value_size(other, interner),
                ));
            }
        };
    let Some(package_type) = LegacyPackType::from_code(type_code) else {
        return Err(LegacyPackRejectionReason::UnknownPackageType {
            value: u64::from(type_code),
        });
    };
    Ok(LegacyPackPkdt {
        observed_size: LEGACY_PKDT_LEN,
        general_flags,
        package_type_code: type_code,
        package_type,
        fallout_behavior_flags,
        type_specific_flags,
        unused_bytes_nonzero: unused_nonzero,
    })
}

fn parse_schedule(
    value: &FieldValue,
    field_index: usize,
    interner: &StringInterner,
) -> Result<LegacyPackSchedule, LegacyPackRejectionReason> {
    let (month, day_of_week, date, hour, duration_hours) = match value {
        FieldValue::Bytes(bytes) if bytes.len() == LEGACY_PSDT_LEN => (
            bytes[0] as i8,
            bytes[1] as i8,
            bytes[2] as i8,
            bytes[3] as i8,
            read_i32(bytes, 4),
        ),
        FieldValue::Struct(fields) => {
            let decoded = struct_i8(fields, "month", interner)
                .zip(struct_i8(fields, "day_of_week", interner))
                .zip(struct_i8(fields, "date", interner))
                .zip(struct_i8(fields, "time", interner))
                .zip(struct_i32(fields, "duration_hours", interner));
            let Some((month, day_of_week, date, hour, duration_hours)) = decoded
                .map(|((((month, day), date), hour), duration)| (month, day, date, hour, duration))
            else {
                return Err(malformed("PSDT", field_index, &[LEGACY_PSDT_LEN], None));
            };
            (month, day_of_week, date, hour, duration_hours)
        }
        other => {
            return Err(malformed(
                "PSDT",
                field_index,
                &[LEGACY_PSDT_LEN],
                value_size(other, interner),
            ));
        }
    };
    Ok(LegacyPackSchedule {
        observed_size: LEGACY_PSDT_LEN,
        month,
        day_of_week,
        date,
        hour,
        duration_hours,
    })
}

fn classify_conditions(
    record: &Record,
    interner: &StringInterner,
    rejections: &mut Vec<LegacyPackRejectionReason>,
) -> Vec<LegacyPackConditionGroup> {
    let mut conditions = Vec::new();
    let mut index = 0;
    while index < record.fields.len() {
        match &record.fields[index].sig.0 {
            b"CTDA" => {
                let observed_size = value_size(&record.fields[index].value, interner);
                if !matches!(observed_size, Some(LEGACY_OLD_CTDA_LEN | LEGACY_CTDA_LEN)) {
                    rejections.push(malformed(
                        "CTDA",
                        index,
                        &[LEGACY_OLD_CTDA_LEN, LEGACY_CTDA_LEN],
                        observed_size,
                    ));
                }
                let condition_index = conditions.len();
                let mut cis1_present = false;
                let mut cis2_present = false;
                index += 1;
                while index < record.fields.len()
                    && matches!(&record.fields[index].sig.0, b"CIS1" | b"CIS2")
                {
                    let companion = &record.fields[index];
                    let present = if companion.sig.0 == *b"CIS1" {
                        &mut cis1_present
                    } else {
                        &mut cis2_present
                    };
                    if *present {
                        rejections.push(LegacyPackRejectionReason::DuplicateConditionCompanion {
                            sig: companion.sig.as_str().to_string(),
                            condition_index,
                        });
                    }
                    if !is_text_payload(&companion.value, interner) {
                        rejections.push(malformed(
                            companion.sig.as_str(),
                            index,
                            &[],
                            value_size(&companion.value, interner),
                        ));
                    }
                    *present = true;
                    index += 1;
                }
                conditions.push(LegacyPackConditionGroup {
                    condition_index,
                    observed_size: observed_size.unwrap_or(0),
                    cis1_present,
                    cis2_present,
                });
                continue;
            }
            b"CIS1" | b"CIS2" => {
                rejections.push(LegacyPackRejectionReason::OrphanConditionCompanion {
                    sig: record.fields[index].sig.as_str().to_string(),
                    field_index: index,
                });
            }
            _ => {}
        }
        index += 1;
    }
    conditions
}

fn classify_unions(
    record: &Record,
    interner: &StringInterner,
    rejections: &mut Vec<LegacyPackRejectionReason>,
) -> Vec<LegacyPackUnionInventory> {
    let mut unions = Vec::new();
    for (field_index, field) in record.fields.iter().enumerate() {
        if !matches!(&field.sig.0, b"PLDT" | b"PLD2" | b"PTDT" | b"PTD2") {
            continue;
        }
        match parse_union(field, field_index, interner) {
            Ok(union) => unions.push(union),
            Err(reason) => rejections.push(reason),
        }
    }
    unions
}

fn parse_union(
    field: &FieldEntry,
    field_index: usize,
    interner: &StringInterner,
) -> Result<LegacyPackUnionInventory, LegacyPackRejectionReason> {
    let is_location = matches!(&field.sig.0, b"PLDT" | b"PLD2");
    let expected_size = if is_location {
        LEGACY_LOCATION_LEN
    } else {
        LEGACY_TARGET_LEN
    };
    let (type_code, payload_value, payload_is_reference, radius_or_distance, trailing_nonzero) =
        match &field.value {
            FieldValue::Bytes(bytes) if bytes.len() == expected_size => (
                read_u32(bytes, 0),
                read_u32(bytes, 4),
                false,
                read_i32(bytes, 8),
                !is_location && read_u32(bytes, 12) != 0,
            ),
            FieldValue::Struct(fields) => {
                let type_code = struct_u32(fields, "type", interner);
                let payload = struct_field(fields, "location", interner)
                    .or_else(|| struct_field(fields, "target", interner));
                let radius = struct_i32(fields, "radius", interner)
                    .or_else(|| struct_i32(fields, "count_distance", interner));
                let Some((type_code, payload, radius_or_distance)) = type_code
                    .zip(payload)
                    .zip(radius)
                    .map(|((a, b), c)| (a, b, c))
                else {
                    return Err(malformed(
                        field.sig.as_str(),
                        field_index,
                        &[expected_size],
                        None,
                    ));
                };
                let payload_is_reference = matches!(payload, FieldValue::FormKey(_));
                let payload_value =
                    numeric_value(payload).unwrap_or(u64::from(payload_is_reference));
                let trailing_nonzero =
                    struct_field(fields, "unknown", interner).is_some_and(value_nonzero);
                (
                    type_code,
                    u32::try_from(payload_value).unwrap_or(u32::MAX),
                    payload_is_reference,
                    radius_or_distance,
                    trailing_nonzero,
                )
            }
            other => {
                return Err(malformed(
                    field.sig.as_str(),
                    field_index,
                    &[expected_size],
                    value_size(other, interner),
                ));
            }
        };

    let (union_kind, payload) = if is_location {
        let location_type = match type_code {
            0 => LegacyPackLocationType::NearReference,
            1 => LegacyPackLocationType::InCell,
            2 => LegacyPackLocationType::NearCurrentLocation,
            3 => LegacyPackLocationType::NearEditorLocation,
            4 => LegacyPackLocationType::ObjectId,
            5 => LegacyPackLocationType::ObjectType,
            6 => LegacyPackLocationType::NearLinkedReference,
            7 => LegacyPackLocationType::AtPackageLocation,
            value => {
                return Err(LegacyPackRejectionReason::UnknownUnionType {
                    sig: field.sig.as_str().to_string(),
                    field_index,
                    value: u64::from(value),
                });
            }
        };
        let payload = match location_type {
            LegacyPackLocationType::NearReference
            | LegacyPackLocationType::InCell
            | LegacyPackLocationType::ObjectId => LegacyPackUnionPayload::Reference {
                present: payload_is_reference || payload_value != 0,
            },
            LegacyPackLocationType::ObjectType => LegacyPackUnionPayload::ObjectType {
                value: payload_value,
            },
            LegacyPackLocationType::NearCurrentLocation
            | LegacyPackLocationType::NearEditorLocation
            | LegacyPackLocationType::NearLinkedReference
            | LegacyPackLocationType::AtPackageLocation => LegacyPackUnionPayload::Implicit,
        };
        (LegacyPackUnionKind::Location(location_type), payload)
    } else {
        let target_type = match type_code {
            0 => LegacyPackTargetType::SpecificReference,
            1 => LegacyPackTargetType::ObjectId,
            2 => LegacyPackTargetType::ObjectType,
            3 => LegacyPackTargetType::LinkedReference,
            value => {
                return Err(LegacyPackRejectionReason::UnknownUnionType {
                    sig: field.sig.as_str().to_string(),
                    field_index,
                    value: u64::from(value),
                });
            }
        };
        let payload = match target_type {
            LegacyPackTargetType::SpecificReference | LegacyPackTargetType::ObjectId => {
                LegacyPackUnionPayload::Reference {
                    present: payload_is_reference || payload_value != 0,
                }
            }
            LegacyPackTargetType::ObjectType => LegacyPackUnionPayload::ObjectType {
                value: payload_value,
            },
            LegacyPackTargetType::LinkedReference => LegacyPackUnionPayload::Implicit,
        };
        (LegacyPackUnionKind::Target(target_type), payload)
    };
    Ok(LegacyPackUnionInventory {
        sig: field.sig.as_str().to_string(),
        observed_size: expected_size,
        type_code,
        union_kind,
        payload,
        radius_or_distance,
        trailing_unknown_nonzero: trailing_nonzero,
    })
}

fn classify_scripts(
    record: &Record,
    interner: &StringInterner,
    rejections: &mut Vec<LegacyPackRejectionReason>,
) -> (Vec<LegacyPackScriptInventory>, Vec<bool>) {
    let markers = record
        .fields
        .iter()
        .enumerate()
        .filter(|(_, field)| SCRIPT_MARKERS.contains(&field.sig.0))
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    let mut owned = vec![false; record.fields.len()];
    let mut scripts = Vec::new();
    for (marker_position, start) in markers.iter().copied().enumerate() {
        let end = markers
            .get(marker_position + 1)
            .copied()
            .unwrap_or(record.fields.len());
        owned[start..end].fill(true);
        match parse_script_block(&record.fields[start..end], interner) {
            Ok(script) => scripts.push(script),
            Err(reason) => rejections.push(reason),
        }
    }
    (scripts, owned)
}

fn parse_script_block(
    fields: &[FieldEntry],
    interner: &StringInterner,
) -> Result<LegacyPackScriptInventory, LegacyPackRejectionReason> {
    let marker = &fields[0];
    let event = match &marker.sig.0 {
        b"POBA" => LegacyPackScriptEvent::OnBegin,
        b"POEA" => LegacyPackScriptEvent::OnEnd,
        b"POCA" => LegacyPackScriptEvent::OnChange,
        _ => unreachable!("caller starts on a script marker"),
    };
    if value_size(&marker.value, interner) != Some(0) {
        return Err(script_error(event, "non_empty_event_marker"));
    }
    for field in &fields[1..] {
        if !matches!(
            &field.sig.0,
            b"INAM" | b"SCHR" | b"SCDA" | b"SCTX" | b"SLSD" | b"SCVR" | b"SCRO" | b"SCRV" | b"TNAM"
        ) {
            return Err(script_error(event, "unexpected_subrecord"));
        }
        let valid = match &field.sig.0 {
            b"INAM" | b"TNAM" | b"SCRO" => reference_value_has_valid_shape(&field.value),
            b"SCDA" => matches!(&field.value, FieldValue::Bytes(_)),
            b"SCTX" => matches!(&field.value, FieldValue::Bytes(_) | FieldValue::String(_)),
            b"SLSD" => {
                matches!(&field.value, FieldValue::Bytes(bytes) if bytes.len() == 24)
                    || matches!(&field.value, FieldValue::Struct(_))
            }
            b"SCVR" => is_text_payload(&field.value, interner),
            b"SCRV" => {
                numeric_value(&field.value).is_some()
                    || matches!(&field.value, FieldValue::Bytes(bytes) if bytes.len() == 4)
            }
            b"SCHR" => true,
            _ => unreachable!("signature was validated above"),
        };
        if !valid {
            return Err(script_error(
                event,
                &format!("malformed_{}", field.sig.as_str().to_ascii_lowercase()),
            ));
        }
    }
    let headers = fields
        .iter()
        .filter(|field| field.sig.0 == *b"SCHR")
        .collect::<Vec<_>>();
    if headers.len() != 1 {
        return Err(script_error(event, "expected_one_schr"));
    }
    let (declared_reference_count, declared_compiled_size, declared_variable_count) =
        parse_script_header(&headers[0].value, interner)
            .ok_or_else(|| script_error(event, "malformed_schr"))?;
    let compiled = fields
        .iter()
        .filter(|field| field.sig.0 == *b"SCDA")
        .collect::<Vec<_>>();
    let sources = fields
        .iter()
        .filter(|field| field.sig.0 == *b"SCTX")
        .collect::<Vec<_>>();
    if compiled.len() > 1 || sources.len() > 1 {
        return Err(script_error(event, "duplicate_script_payload"));
    }
    let compiled_payload_size = compiled
        .first()
        .and_then(|field| value_size(&field.value, interner))
        .unwrap_or(0);
    let source_payload_size = sources
        .first()
        .and_then(|field| value_size(&field.value, interner))
        .unwrap_or(0);
    let local_variable_rows = fields
        .iter()
        .filter(|field| field.sig.0 == *b"SLSD")
        .count();
    let named_local_variables = fields
        .iter()
        .filter(|field| field.sig.0 == *b"SCVR")
        .count();
    let global_references = fields
        .iter()
        .filter(|field| field.sig.0 == *b"SCRO")
        .count();
    let local_references = fields
        .iter()
        .filter(|field| field.sig.0 == *b"SCRV")
        .count();
    Ok(LegacyPackScriptInventory {
        event,
        header_size: LEGACY_SCHR_LEN,
        declared_reference_count,
        declared_compiled_size,
        declared_variable_count,
        compiled_payload_size,
        source_payload_size,
        local_variable_rows,
        named_local_variables,
        global_references,
        local_references,
        idle_present: fields.iter().any(|field| field.sig.0 == *b"INAM"),
        topic_present: fields.iter().any(|field| field.sig.0 == *b"TNAM"),
        compiled_size_matches: declared_compiled_size as usize == compiled_payload_size,
        reference_count_matches: declared_reference_count as usize
            == global_references + local_references,
        variable_count_matches: declared_variable_count as usize == local_variable_rows
            && local_variable_rows == named_local_variables,
    })
}

fn parse_script_header(value: &FieldValue, interner: &StringInterner) -> Option<(u32, u32, u32)> {
    match value {
        FieldValue::Bytes(bytes) if bytes.len() == LEGACY_SCHR_LEN => {
            Some((read_u32(bytes, 4), read_u32(bytes, 8), read_u32(bytes, 12)))
        }
        FieldValue::Struct(fields) => Some((
            struct_u32(fields, "ref_count", interner)?,
            struct_u32(fields, "compiled_size", interner)?,
            struct_u32(fields, "variable_count", interner)?,
        )),
        _ => None,
    }
}

fn reject_orphan_script_fields(
    record: &Record,
    owned: &[bool],
    rejections: &mut Vec<LegacyPackRejectionReason>,
) {
    for (field_index, field) in record.fields.iter().enumerate() {
        if is_script_member(field.sig.0) && !owned[field_index] {
            rejections.push(LegacyPackRejectionReason::OrphanScriptSubrecord {
                sig: field.sig.as_str().to_string(),
                field_index,
            });
        }
    }
}

fn classify_type_specific(
    record: &Record,
    interner: &StringInterner,
    rejections: &mut Vec<LegacyPackRejectionReason>,
) -> (
    Vec<LegacyPackSubrecordInventory>,
    Option<LegacyPackUseWeaponData>,
) {
    let mut inventory = Vec::new();
    let mut use_weapon = None;
    for sig in TYPE_SPECIFIC_SIGS {
        let matches = record
            .fields
            .iter()
            .enumerate()
            .filter(|(_, field)| field.sig.0 == sig)
            .collect::<Vec<_>>();
        if matches.is_empty() {
            continue;
        }
        if matches.len() > 1 {
            rejections.push(LegacyPackRejectionReason::DuplicateSubrecord {
                sig: sig_text(sig),
                count: matches.len(),
            });
        }
        let expected = expected_type_specific_size(sig);
        let mut observed_sizes = Vec::new();
        for (field_index, field) in &matches {
            let observed = if matches!(&field.value, FieldValue::Struct(_)) {
                Some(expected)
            } else {
                value_size(&field.value, interner)
            };
            if observed != Some(expected) {
                rejections.push(malformed(
                    field.sig.as_str(),
                    *field_index,
                    &[expected],
                    observed,
                ));
            }
            observed_sizes.push(observed.unwrap_or(0));
        }
        if sig == *b"PKW3" && matches.len() == 1 {
            match parse_use_weapon(&matches[0].1.value, matches[0].0, interner) {
                Ok(decoded) => use_weapon = Some(decoded),
                Err(reason) => rejections.push(reason),
            }
        }
        inventory.push(LegacyPackSubrecordInventory {
            sig: sig_text(sig),
            count: matches.len(),
            observed_sizes,
        });
    }
    (inventory, use_weapon)
}

fn parse_use_weapon(
    value: &FieldValue,
    field_index: usize,
    interner: &StringInterner,
) -> Result<LegacyPackUseWeaponData, LegacyPackRejectionReason> {
    let decoded = match value {
        FieldValue::Bytes(bytes) if bytes.len() == LEGACY_PKW3_LEN => LegacyPackUseWeaponData {
            observed_size: LEGACY_PKW3_LEN,
            flags: read_u32(bytes, 0),
            fire_rate: bytes[4],
            fire_count: bytes[5],
            number_of_bursts: read_u16(bytes, 6),
            shots_per_volley_min: read_u16(bytes, 8),
            shots_per_volley_max: read_u16(bytes, 10),
            pause_between_volleys_min: f32::from_bits(read_u32(bytes, 12)),
            pause_between_volleys_max: f32::from_bits(read_u32(bytes, 16)),
            unused_bytes_nonzero: bytes[20..24].iter().any(|byte| *byte != 0),
        },
        FieldValue::Struct(fields) => {
            let Some((flags, fire_rate, fire_count, bursts, min, max, pause_min, pause_max)) =
                struct_u32(fields, "flags", interner)
                    .zip(struct_u8(fields, "fire_rate", interner))
                    .zip(struct_u8(fields, "fire_count", interner))
                    .zip(struct_u16(fields, "number_of_bursts", interner))
                    .zip(struct_u16(fields, "shoots_per_volleys_min", interner))
                    .zip(struct_u16(fields, "shoots_per_volleys_max", interner))
                    .zip(struct_f32(fields, "pause_between_volleys_min", interner))
                    .zip(struct_f32(fields, "pause_between_volleys_max", interner))
                    .map(|(((((((a, b), c), d), e), f), g), h)| (a, b, c, d, e, f, g, h))
            else {
                return Err(malformed("PKW3", field_index, &[LEGACY_PKW3_LEN], None));
            };
            LegacyPackUseWeaponData {
                observed_size: LEGACY_PKW3_LEN,
                flags,
                fire_rate,
                fire_count,
                number_of_bursts: bursts,
                shots_per_volley_min: min,
                shots_per_volley_max: max,
                pause_between_volleys_min: pause_min,
                pause_between_volleys_max: pause_max,
                unused_bytes_nonzero: false,
            }
        }
        other => {
            return Err(malformed(
                "PKW3",
                field_index,
                &[LEGACY_PKW3_LEN],
                value_size(other, interner),
            ));
        }
    };
    Ok(decoded)
}

fn struct_package_type(
    fields: &[(crate::sym::Sym, FieldValue)],
    interner: &StringInterner,
) -> Option<u8> {
    let value = struct_field(fields, "type", interner)?;
    if let Some(value) = numeric_value(value) {
        return u8::try_from(value).ok();
    }
    let FieldValue::String(name) = value else {
        return None;
    };
    let normalized = interner
        .resolve(*name)?
        .bytes()
        .filter(|byte| byte.is_ascii_alphanumeric())
        .map(|byte| byte.to_ascii_lowercase())
        .collect::<Vec<_>>();
    Some(match normalized.as_slice() {
        b"find" => 0,
        b"follow" => 1,
        b"escort" => 2,
        b"eat" => 3,
        b"sleep" => 4,
        b"wander" => 5,
        b"travel" => 6,
        b"accompany" => 7,
        b"useitemat" => 8,
        b"ambush" => 9,
        b"fleenotcombat" => 10,
        b"packagetype11" => 11,
        b"sandbox" => 12,
        b"patrol" => 13,
        b"guard" => 14,
        b"dialogue" => 15,
        b"useweapon" => 16,
        _ => return None,
    })
}

fn struct_field<'a>(
    fields: &'a [(crate::sym::Sym, FieldValue)],
    name: &str,
    interner: &StringInterner,
) -> Option<&'a FieldValue> {
    fields
        .iter()
        .find_map(|(key, value)| (interner.resolve(*key) == Some(name)).then_some(value))
}

fn struct_u64(
    fields: &[(crate::sym::Sym, FieldValue)],
    name: &str,
    interner: &StringInterner,
) -> Option<u64> {
    numeric_value(struct_field(fields, name, interner)?)
}

fn struct_u32(
    fields: &[(crate::sym::Sym, FieldValue)],
    name: &str,
    interner: &StringInterner,
) -> Option<u32> {
    u32::try_from(struct_u64(fields, name, interner)?).ok()
}

fn struct_u16(
    fields: &[(crate::sym::Sym, FieldValue)],
    name: &str,
    interner: &StringInterner,
) -> Option<u16> {
    u16::try_from(struct_u64(fields, name, interner)?).ok()
}

fn struct_u8(
    fields: &[(crate::sym::Sym, FieldValue)],
    name: &str,
    interner: &StringInterner,
) -> Option<u8> {
    u8::try_from(struct_u64(fields, name, interner)?).ok()
}

fn struct_i32(
    fields: &[(crate::sym::Sym, FieldValue)],
    name: &str,
    interner: &StringInterner,
) -> Option<i32> {
    signed_value(struct_field(fields, name, interner)?).and_then(|value| i32::try_from(value).ok())
}

fn struct_i8(
    fields: &[(crate::sym::Sym, FieldValue)],
    name: &str,
    interner: &StringInterner,
) -> Option<i8> {
    signed_value(struct_field(fields, name, interner)?).and_then(|value| i8::try_from(value).ok())
}

fn struct_f32(
    fields: &[(crate::sym::Sym, FieldValue)],
    name: &str,
    interner: &StringInterner,
) -> Option<f32> {
    match struct_field(fields, name, interner)? {
        FieldValue::Float(value) => Some(*value),
        _ => None,
    }
}

fn numeric_value(value: &FieldValue) -> Option<u64> {
    match value {
        FieldValue::Uint(value) => Some(*value),
        FieldValue::Int(value) => u64::try_from(*value).ok(),
        FieldValue::Bool(value) => Some(u64::from(*value)),
        FieldValue::None => Some(0),
        _ => None,
    }
}

fn signed_value(value: &FieldValue) -> Option<i64> {
    match value {
        FieldValue::Int(value) => Some(*value),
        FieldValue::Uint(value) => i64::try_from(*value).ok(),
        _ => None,
    }
}

fn value_nonzero(value: &FieldValue) -> bool {
    match value {
        FieldValue::None => false,
        FieldValue::Bool(value) => *value,
        FieldValue::Int(value) => *value != 0,
        FieldValue::Uint(value) => *value != 0,
        FieldValue::Float(value) => *value != 0.0,
        FieldValue::Bytes(bytes) => bytes.iter().any(|byte| *byte != 0),
        FieldValue::FormKey(form_key) => form_key.local != 0,
        FieldValue::String(_) | FieldValue::List(_) | FieldValue::Struct(_) => true,
    }
}

fn reference_value_has_valid_shape(value: &FieldValue) -> bool {
    matches!(
        value,
        FieldValue::None | FieldValue::FormKey(_) | FieldValue::Uint(_) | FieldValue::Int(_)
    ) || matches!(value, FieldValue::Bytes(bytes) if bytes.len() == 4)
}

fn value_size(value: &FieldValue, interner: &StringInterner) -> Option<usize> {
    match value {
        FieldValue::None => Some(0),
        FieldValue::Bool(_) => Some(1),
        FieldValue::Int(_)
        | FieldValue::Uint(_)
        | FieldValue::Float(_)
        | FieldValue::FormKey(_) => Some(4),
        FieldValue::String(value) => interner.resolve(*value).map(|value| value.len() + 1),
        FieldValue::Bytes(bytes) => Some(bytes.len()),
        FieldValue::List(_) | FieldValue::Struct(_) => None,
    }
}

fn is_text_payload(value: &FieldValue, interner: &StringInterner) -> bool {
    match value {
        FieldValue::String(value) => interner.resolve(*value).is_some(),
        FieldValue::Bytes(bytes) => {
            bytes.last() == Some(&0) && std::str::from_utf8(&bytes[..bytes.len() - 1]).is_ok()
        }
        _ => false,
    }
}

fn malformed(
    sig: &str,
    field_index: usize,
    expected_sizes: &[usize],
    observed_size: Option<usize>,
) -> LegacyPackRejectionReason {
    LegacyPackRejectionReason::MalformedSubrecord {
        sig: sig.to_string(),
        field_index,
        expected_sizes: expected_sizes.to_vec(),
        observed_size,
    }
}

fn script_error(event: LegacyPackScriptEvent, issue: &str) -> LegacyPackRejectionReason {
    LegacyPackRejectionReason::MalformedScriptBlock {
        event,
        issue: issue.to_string(),
    }
}

fn expected_type_specific_size(sig: [u8; 4]) -> usize {
    match &sig {
        b"PKED" | b"PUID" | b"PKAM" => 0,
        b"PKPT" => 2,
        b"PKW3" => LEGACY_PKW3_LEN,
        b"PKDD" => 24,
        b"PKE2" | b"PKFD" => 4,
        _ => unreachable!("type-specific signature table is exhaustive"),
    }
}

fn is_known_subrecord(sig: [u8; 4]) -> bool {
    matches!(
        &sig,
        b"EDID"
            | b"PKDT"
            | b"PLDT"
            | b"PLD2"
            | b"PSDT"
            | b"PTDT"
            | b"PTD2"
            | b"CTDA"
            | b"CIS1"
            | b"CIS2"
            | b"IDLF"
            | b"IDLC"
            | b"IDLT"
            | b"IDLA"
            | b"IDLB"
            | b"CNAM"
            | b"PKED"
            | b"PKE2"
            | b"PKFD"
            | b"PKPT"
            | b"PKW3"
            | b"PUID"
            | b"PKAM"
            | b"PKDD"
            | b"POBA"
            | b"POEA"
            | b"POCA"
            | b"INAM"
            | b"SCHR"
            | b"SCDA"
            | b"SCTX"
            | b"SLSD"
            | b"SCVR"
            | b"SCRO"
            | b"SCRV"
            | b"TNAM"
    )
}

fn is_script_member(sig: [u8; 4]) -> bool {
    matches!(
        &sig,
        b"INAM" | b"SCHR" | b"SCDA" | b"SCTX" | b"SLSD" | b"SCVR" | b"SCRO" | b"SCRV" | b"TNAM"
    )
}

fn sig_text(sig: [u8; 4]) -> String {
    std::str::from_utf8(&sig)
        .expect("subrecord signatures are ASCII")
        .to_string()
}

fn read_u16(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes(
        bytes[offset..offset + 2]
            .try_into()
            .expect("validated width"),
    )
}

fn read_u32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(
        bytes[offset..offset + 4]
            .try_into()
            .expect("validated width"),
    )
}

fn read_i32(bytes: &[u8], offset: usize) -> i32 {
    i32::from_le_bytes(
        bytes[offset..offset + 4]
            .try_into()
            .expect("validated width"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::{FormKey, SigCode, SubrecordSig};
    use smallvec::SmallVec;

    fn record(interner: &StringInterner, local: u32, plugin: &str, eid: &str) -> Record {
        let mut record = Record::new(
            SigCode(*b"PACK"),
            FormKey {
                local,
                plugin: interner.intern(plugin),
            },
        );
        record.eid = Some(interner.intern(eid));
        record
    }

    fn field(sig: &[u8; 4], bytes: &[u8]) -> FieldEntry {
        FieldEntry {
            sig: SubrecordSig(*sig),
            value: FieldValue::Bytes(SmallVec::from_slice(bytes)),
        }
    }

    fn pkdt(package_type: u8) -> FieldEntry {
        let mut bytes = [0_u8; LEGACY_PKDT_LEN];
        bytes[..4].copy_from_slice(&0x0080_1004_u32.to_le_bytes());
        bytes[4] = package_type;
        bytes[6..8].copy_from_slice(&0x0045_u16.to_le_bytes());
        bytes[8..10].copy_from_slice(&0x0102_u16.to_le_bytes());
        field(b"PKDT", &bytes)
    }

    fn psdt() -> FieldEntry {
        let mut bytes = [0_u8; LEGACY_PSDT_LEN];
        bytes[..4].copy_from_slice(&[0xFF, 0xFF, 15, 8]);
        bytes[4..].copy_from_slice(&6_i32.to_le_bytes());
        field(b"PSDT", &bytes)
    }

    fn location(sig: &[u8; 4], kind: u32, value: u32, radius: i32) -> FieldEntry {
        let mut bytes = [0_u8; LEGACY_LOCATION_LEN];
        bytes[..4].copy_from_slice(&kind.to_le_bytes());
        bytes[4..8].copy_from_slice(&value.to_le_bytes());
        bytes[8..12].copy_from_slice(&radius.to_le_bytes());
        field(sig, &bytes)
    }

    fn target(sig: &[u8; 4], kind: u32, value: u32, distance: i32) -> FieldEntry {
        let mut bytes = [0_u8; LEGACY_TARGET_LEN];
        bytes[..4].copy_from_slice(&kind.to_le_bytes());
        bytes[4..8].copy_from_slice(&value.to_le_bytes());
        bytes[8..12].copy_from_slice(&distance.to_le_bytes());
        field(sig, &bytes)
    }

    fn empty_script(marker: &[u8; 4]) -> [FieldEntry; 2] {
        [field(marker, &[]), field(b"SCHR", &[0; LEGACY_SCHR_LEN])]
    }

    fn base_pack(
        interner: &StringInterner,
        source: LegacyPackSourceFamily,
        package_type: LegacyPackType,
    ) -> Record {
        let (plugin, local) = match source {
            LegacyPackSourceFamily::Fnv => ("FalloutNV.esm", 0x100000),
            LegacyPackSourceFamily::Fo3 => ("Fallout3.esm", 0x200000),
            _ => ("target.esm", 0x300000),
        };
        let mut record = record(
            interner,
            local + u32::from(package_type.code()),
            plugin,
            "GoldenPack",
        );
        record.fields.extend([pkdt(package_type.code()), psdt()]);
        record
    }

    #[test]
    fn fnv_pack_generated_enum_gap_12_through_16_is_explicitly_classified() {
        let cases = [
            (12, LegacyPackType::Sandbox, "\"sandbox\""),
            (13, LegacyPackType::Patrol, "\"patrol\""),
            (14, LegacyPackType::Guard, "\"guard\""),
            (15, LegacyPackType::Dialogue, "\"dialogue\""),
            (16, LegacyPackType::UseWeapon, "\"use_weapon\""),
        ];
        for (code, expected, serialized) in cases {
            assert_eq!(LegacyPackType::from_code(code), Some(expected));
            assert_eq!(expected.code(), code);
            assert_eq!(serde_json::to_string(&expected).unwrap(), serialized);
        }
        assert_eq!(LegacyPackType::from_code(17), None);
    }

    #[test]
    fn fnv_pack_families_classify_travel_patrol_follow_sandbox_and_use_weapon_goldens() {
        let interner = StringInterner::new();
        let types = [
            LegacyPackType::Travel,
            LegacyPackType::Patrol,
            LegacyPackType::Follow,
            LegacyPackType::Sandbox,
            LegacyPackType::UseWeapon,
        ];
        for source in [LegacyPackSourceFamily::Fnv, LegacyPackSourceFamily::Fo3] {
            for package_type in types {
                let mut record = base_pack(&interner, source, package_type);
                match package_type {
                    LegacyPackType::Travel => {
                        record.fields.push(location(b"PLDT", 3, 0, 0));
                    }
                    LegacyPackType::Patrol => {
                        record.fields.push(location(b"PLDT", 6, 0, 0));
                        record.fields.push(field(b"PKPT", &[1, 0]));
                    }
                    LegacyPackType::Follow => {
                        record.fields.push(target(b"PTDT", 3, 0, 128));
                        record.fields.push(field(b"PKFD", &0_f32.to_le_bytes()));
                    }
                    LegacyPackType::Sandbox => {
                        record.fields.push(location(b"PLDT", 3, 0, 1024));
                        record.fields.push(location(b"PLD2", 7, 0, 256));
                    }
                    LegacyPackType::UseWeapon => {
                        record.fields.push(target(b"PTDT", 2, 23, 1));
                        record.fields.push(field(b"PKW3", &[0; LEGACY_PKW3_LEN]));
                        record.fields.push(target(b"PTD2", 0, 0x0012_3456, 0));
                    }
                    _ => unreachable!(),
                }
                record.fields.extend(empty_script(b"POBA"));
                let report = classify_legacy_pack(&record, source, &interner);
                assert_eq!(report.status, LegacyPackClassificationStatus::Accepted);
                let inventory = report.inventory.expect("accepted inventory");
                assert_eq!(inventory.source, source);
                assert_eq!(inventory.package_type, package_type);
                assert_eq!(inventory.pkdt.observed_size, LEGACY_PKDT_LEN);
                assert_eq!(inventory.schedule.observed_size, LEGACY_PSDT_LEN);
                assert_eq!(inventory.scripts.len(), 1);
                assert!(!inventory.support.lowering_supported);
                if package_type == LegacyPackType::UseWeapon {
                    assert!(inventory.use_weapon_data.is_some());
                    assert!(inventory.unions.iter().any(|union| union.sig == "PTD2"));
                }
                let json = serde_json::to_string(&inventory).unwrap();
                assert!(!json.contains("raw_hex"));
            }
        }
    }

    #[test]
    fn fnv_pack_script_inventory_accounts_for_payloads_locals_and_references() {
        let interner = StringInterner::new();
        let mut record = base_pack(
            &interner,
            LegacyPackSourceFamily::Fnv,
            LegacyPackType::Travel,
        );
        let mut header = [0_u8; LEGACY_SCHR_LEN];
        header[4..8].copy_from_slice(&2_u32.to_le_bytes());
        header[8..12].copy_from_slice(&3_u32.to_le_bytes());
        header[12..16].copy_from_slice(&1_u32.to_le_bytes());
        record.fields.extend([
            field(b"POBA", &[]),
            field(b"INAM", &[0; 4]),
            field(b"SCHR", &header),
            field(b"SCDA", &[1, 2, 3]),
            field(b"SCTX", b"script source"),
            field(b"SLSD", &[0; 24]),
            FieldEntry {
                sig: SubrecordSig(*b"SCVR"),
                value: FieldValue::String(interner.intern("local")),
            },
            field(b"SCRO", &[1, 0, 0, 0]),
            FieldEntry {
                sig: SubrecordSig(*b"SCRV"),
                value: FieldValue::Uint(0),
            },
            field(b"TNAM", &[0; 4]),
        ]);

        let report = classify_legacy_pack(&record, LegacyPackSourceFamily::Fnv, &interner);
        assert_eq!(report.status, LegacyPackClassificationStatus::Accepted);
        let script = &report.inventory.unwrap().scripts[0];
        assert_eq!(script.compiled_payload_size, 3);
        assert_eq!(script.source_payload_size, 13);
        assert_eq!(script.local_variable_rows, 1);
        assert_eq!(script.named_local_variables, 1);
        assert_eq!(script.global_references, 1);
        assert_eq!(script.local_references, 1);
        assert!(script.compiled_size_matches);
        assert!(script.reference_count_matches);
        assert!(script.variable_count_matches);
    }

    #[test]
    fn fnv_pack_type_specific_inventory_covers_audited_legacy_shapes() {
        let interner = StringInterner::new();
        let mut record = base_pack(
            &interner,
            LegacyPackSourceFamily::Fo3,
            LegacyPackType::Dialogue,
        );
        record.fields.extend([
            field(b"PKED", &[]),
            field(b"PKE2", &[0; 4]),
            field(b"PKFD", &[0; 4]),
            field(b"PKPT", &[0; 2]),
            field(b"PKW3", &[0; LEGACY_PKW3_LEN]),
            field(b"PUID", &[]),
            field(b"PKAM", &[]),
            field(b"PKDD", &[0; 24]),
        ]);

        let report = classify_legacy_pack(&record, LegacyPackSourceFamily::Fo3, &interner);
        assert_eq!(report.status, LegacyPackClassificationStatus::Accepted);
        let inventory = report.inventory.unwrap();
        assert_eq!(
            inventory.type_specific_subrecords.len(),
            TYPE_SPECIFIC_SIGS.len()
        );
        assert!(inventory.use_weapon_data.is_some());
        assert!(
            inventory
                .type_specific_subrecords
                .iter()
                .all(|entry| entry.count == 1)
        );
    }

    #[test]
    fn fnv_pack_conditions_keep_20_and_28_byte_rows_with_atomic_cis_companions() {
        let interner = StringInterner::new();
        let mut record = base_pack(
            &interner,
            LegacyPackSourceFamily::Fo3,
            LegacyPackType::Follow,
        );
        record.fields.extend([
            field(b"CTDA", &[0; LEGACY_OLD_CTDA_LEN]),
            FieldEntry {
                sig: SubrecordSig(*b"CIS1"),
                value: FieldValue::String(interner.intern("parameter one")),
            },
            field(b"CTDA", &[0; LEGACY_CTDA_LEN]),
            FieldEntry {
                sig: SubrecordSig(*b"CIS2"),
                value: FieldValue::String(interner.intern("parameter two")),
            },
        ]);

        let report = classify_legacy_pack(&record, LegacyPackSourceFamily::Fo3, &interner);
        assert_eq!(report.status, LegacyPackClassificationStatus::Accepted);
        let conditions = report.inventory.unwrap().conditions;
        assert_eq!(conditions.len(), 2);
        assert_eq!(conditions[0].observed_size, LEGACY_OLD_CTDA_LEN);
        assert!(conditions[0].cis1_present);
        assert_eq!(conditions[1].observed_size, LEGACY_CTDA_LEN);
        assert!(conditions[1].cis2_present);
    }

    #[test]
    fn fnv_pack_malformed_and_unknown_records_are_rejected_without_raw_leaks() {
        let interner = StringInterner::new();
        let mut malformed = base_pack(
            &interner,
            LegacyPackSourceFamily::Fnv,
            LegacyPackType::Travel,
        );
        malformed.fields[0] = field(b"PKDT", b"SECRET_BYTES!");
        malformed.fields.push(field(b"ZZZZ", b"MORE_SECRET"));

        let report = classify_legacy_pack(&malformed, LegacyPackSourceFamily::Fnv, &interner);
        assert_eq!(report.status, LegacyPackClassificationStatus::Rejected);
        assert!(report.inventory.is_none());
        let json = serde_json::to_string(&report).unwrap();
        assert!(!json.contains("SECRET"));
        assert!(!json.contains("raw"));
        assert!(report.rejection_reasons.iter().any(|reason| matches!(
            reason,
            LegacyPackRejectionReason::UnknownSubrecord { sig, .. } if sig == "ZZZZ"
        )));

        let mut unknown_type = base_pack(
            &interner,
            LegacyPackSourceFamily::Fnv,
            LegacyPackType::Travel,
        );
        unknown_type.fields[0] = pkdt(17);
        let report = classify_legacy_pack(&unknown_type, LegacyPackSourceFamily::Fnv, &interner);
        assert_eq!(report.status, LegacyPackClassificationStatus::Rejected);
        assert!(report.rejection_reasons.iter().any(|reason| matches!(
            reason,
            LegacyPackRejectionReason::UnknownPackageType { value: 17 }
        )));
    }

    #[test]
    fn fnv_pack_unresolved_record_identity_is_rejected_without_panicking() {
        let record_interner = StringInterner::new();
        let report_interner = StringInterner::new();
        let mut record = Record::new(
            SigCode(*b"PACK"),
            FormKey {
                local: 1,
                plugin: record_interner.intern("FalloutNV.esm"),
            },
        );
        record.fields.extend([pkdt(6), psdt()]);

        let report = classify_legacy_pack(&record, LegacyPackSourceFamily::Fnv, &report_interner);
        assert_eq!(report.status, LegacyPackClassificationStatus::Rejected);
        assert!(report.rejection_reasons.iter().any(|reason| matches!(
            reason,
            LegacyPackRejectionReason::UnresolvedRecordIdentity { field }
                if field == "form_key_plugin"
        )));
    }

    #[test]
    fn fnv_pack_orphan_condition_companion_is_rejected() {
        let interner = StringInterner::new();
        let mut record = base_pack(
            &interner,
            LegacyPackSourceFamily::Fnv,
            LegacyPackType::Travel,
        );
        record.fields.push(FieldEntry {
            sig: SubrecordSig(*b"CIS1"),
            value: FieldValue::String(interner.intern("orphan")),
        });

        let report = classify_legacy_pack(&record, LegacyPackSourceFamily::Fnv, &interner);
        assert_eq!(report.status, LegacyPackClassificationStatus::Rejected);
        assert!(report.rejection_reasons.iter().any(|reason| matches!(
            reason,
            LegacyPackRejectionReason::OrphanConditionCompanion { .. }
        )));
    }

    #[test]
    fn fnv_pack_fo4_and_fo76_are_true_noops() {
        let interner = StringInterner::new();
        let record = base_pack(
            &interner,
            LegacyPackSourceFamily::Fnv,
            LegacyPackType::Travel,
        );
        for source in [LegacyPackSourceFamily::Fo4, LegacyPackSourceFamily::Fo76] {
            let before = format!("{:?}", record.fields);
            let report = classify_legacy_pack(&record, source, &interner);
            assert_eq!(report.status, LegacyPackClassificationStatus::NotApplicable);
            assert!(report.inventory.is_none());
            assert_eq!(format!("{:?}", record.fields), before);
        }
    }

    #[test]
    fn fnv_pack_corpus_report_reproduces_the_authoritative_9455_upper_census() {
        let interner = StringInterner::new();
        let fnv_record = base_pack(
            &interner,
            LegacyPackSourceFamily::Fnv,
            LegacyPackType::Travel,
        );
        let fo3_record = base_pack(
            &interner,
            LegacyPackSourceFamily::Fo3,
            LegacyPackType::Patrol,
        );
        let fnv = classify_legacy_pack(&fnv_record, LegacyPackSourceFamily::Fnv, &interner);
        let fo3 = classify_legacy_pack(&fo3_record, LegacyPackSourceFamily::Fo3, &interner);
        let mut reports = Vec::with_capacity(AUDITED_LEGACY_PACK_COUNT);
        reports.extend(std::iter::repeat_n(fnv, AUDITED_FNV_PACK_COUNT));
        reports.extend(std::iter::repeat_n(fo3, AUDITED_FO3_PACK_COUNT));

        let summary = summarize_legacy_pack_reports(&reports);
        assert_eq!(summary.fnv_records, 4_888);
        assert_eq!(summary.fo3_records, 4_567);
        assert_eq!(summary.total_records, 9_455);
        assert_eq!(summary.accepted_records + summary.rejected_records, 9_455);
        assert!(summary.exact_audited_coverage);
        assert_eq!(9_455 - 9_152, 33 + 270, "group-index undercount");
    }
}
