//! FNV/FO3 PACK inventory plus the first verified FO4 template-backed lowerer.
//!
//! Legacy PACK records and FO4 PACK records share several subrecord signatures, but their
//! payloads are not ABI-compatible. In particular, both PKDT layouts are 12 bytes while assigning
//! those bytes different meanings. This module therefore classifies legacy records without
//! mutating them. Lowering is restricted to source shapes that have an audited FO4 base-game
//! package template; unsupported shapes remain fail-closed.

use std::collections::BTreeMap;

use serde::Serialize;
use smallvec::SmallVec;
use thiserror::Error;

use super::fnv_conditions::{LegacyConditionFamily, normalize_legacy_condition_scope};
use crate::formkey_mapper::FormKeyMapper;
use crate::ids::{FormKey, SigCode, SubrecordSig};
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

const FO4_TRAVEL_TEMPLATE_LOCAL: u32 = 0x002C_B0;
const FO4_PATROL_TEMPLATE_LOCAL: u32 = 0x002C_E0;
const FO4_FORCE_GREET_TEMPLATE_LOCAL: u32 = 0x017B_AB;
const FO4_FORCE_GREET_DONOR_LOCAL: u32 = 0x05B3_07;
const FO4_SHARED_LEGACY_GENERAL_FLAGS: u32 = 0x0026_06C5;
const FO4_FORCE_GREET_INPUT_TYPES: [&str; 24] = [
    "Topic",
    "Location",
    "Location",
    "Location",
    "Bool",
    "SingleRef",
    "Location",
    "Bool",
    "Bool",
    "Bool",
    "Bool",
    "Bool",
    "Bool",
    "Bool",
    "Bool",
    "Bool",
    "Bool",
    "Bool",
    "Bool",
    "Bool",
    "TargetSelector",
    "Float",
    "Int",
    "TargetSelector",
];
const FO4_FORCE_GREET_UNAM: [u8; 24] = [
    0x00, 0x01, 0x02, 0x05, 0x07, 0x04, 0x09, 0x0D, 0x0F, 0x10, 0x11, 0x12, 0x13, 0x14, 0x1A, 0x16,
    0x18, 0x1C, 0x1D, 0x1F, 0x20, 0x21, 0x22, 0x24,
];

const AUDITED_SOURCE_PLUGIN: &str = "FalloutNV.esm";
const TEC_MINE_HOSTAGE_TRAVEL_LOCAL: u32 = 0x1231_B7;
const TEC_MINE_HOSTAGE_ESCAPE_LOCAL: u32 = 0x1231_B6;
const RENOLDS_DIALOGUE_PACK_LOCAL: u32 = 0x1328_9E;
const RENOLDS_PATROL_PACK_LOCAL: u32 = 0x133F_3E;
const VTECHATTICUP_QUEST_LOCAL: u32 = 0x11F9_35;
const TEC_MINE_HOSTAGE_ESCAPE_SOURCE: &str = "ref hostage\r\nhostage.disable";
const TEC_MINE_HOSTAGE_ESCAPE_BYTECODE: [u8; 10] = [0x1C, 0, 1, 0, 0x22, 0x10, 2, 0, 0, 0];
const RENOLDS_GET_STAGE_CTDA: [u8; LEGACY_CTDA_LEN] = [
    0x80, 0, 0, 0, 0, 0, 0x20, 0x41, 0x3A, 0, 0, 0, 0x35, 0xF9, 0x11, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0,
];

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
    pub unused_1: u8,
    pub unused_2: u16,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LegacyPackUnionPayload {
    Reference {
        present: bool,
        source_form_key: Option<String>,
    },
    ObjectType {
        value: u32,
    },
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
    pub raw_ctda: Option<Vec<u8>>,
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
    pub requires_port: bool,
    pub compiled_payload: Vec<u8>,
    pub source_text: Option<String>,
    pub compiled_size_matches: bool,
    pub reference_count_matches: bool,
    pub variable_count_matches: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LegacyPackPatrolData {
    pub repeatable: bool,
    pub unused_byte_nonzero: bool,
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
    pub patrol_data: Option<LegacyPackPatrolData>,
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

/// Contract for a lowerer whose supported FO4 procedure-tree blueprints are byte-verified.
///
/// Implementations must consume only an accepted inventory and reject every shape they do not
/// explicitly support.
pub trait LegacyPackLowerer {
    type Error;

    fn lower_supported_legacy_pack(
        &self,
        inventory: &LegacyPackInventory,
    ) -> Result<Record, Self::Error>;
}

pub trait LegacyPackReferenceMapper {
    fn remap_legacy_pack_reference(&self, source_form_key: &str) -> Option<FormKey>;
}

impl<F> LegacyPackReferenceMapper for F
where
    F: Fn(&str) -> Option<FormKey>,
{
    fn remap_legacy_pack_reference(&self, source_form_key: &str) -> Option<FormKey> {
        self(source_form_key)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error, Serialize)]
#[serde(tag = "reason", rename_all = "snake_case")]
pub enum LegacyPackLoweringError {
    #[error("legacy PACK conditions require semantic lowering ({count} rows)")]
    ConditionsRequirePort { count: usize },
    #[error("legacy PACK event scripts require porting ({count} events)")]
    EventScriptsRequirePort { count: usize },
    #[error("legacy PACK type {package_type_code} has no verified FO4 blueprint")]
    UnsupportedPackageType { package_type_code: u8 },
    #[error("legacy PACK {package_type_code} has unsupported semantic shape: {issue}")]
    UnsupportedPackageShape {
        package_type_code: u8,
        issue: String,
    },
    #[error("legacy PACK reference payload in {sig} was not decoded to a FormKey")]
    UnresolvedEncodedReference { sig: String },
    #[error("legacy PACK reference remap failed for {source_form_key}")]
    ReferenceRemapFailed { source_form_key: String },
    #[error("legacy PACK record identity is invalid: {identity}")]
    InvalidRecordIdentity { identity: String },
    #[error("legacy PACK condition normalization failed: {issue}")]
    ConditionNormalizeFailed { issue: String },
    #[error("verified FO4 ForceGreet donor is invalid: {issue}")]
    InvalidForceGreetDonor { issue: String },
}

impl LegacyPackLoweringError {
    pub const fn code(&self) -> &'static str {
        match self {
            Self::ConditionsRequirePort { .. } => "legacy_conditions_require_semantic_lowering",
            Self::EventScriptsRequirePort { .. } => "legacy_event_scripts_require_port",
            Self::UnsupportedPackageType { .. } => "no_verified_fo4_procedure_blueprint",
            Self::UnsupportedPackageShape { .. } => "unsupported_verified_package_shape",
            Self::UnresolvedEncodedReference { .. } => "unresolved_encoded_reference",
            Self::ReferenceRemapFailed { .. } => "reference_remap_failed",
            Self::InvalidRecordIdentity { .. } => "invalid_record_identity",
            Self::ConditionNormalizeFailed { .. } => "condition_normalize_failed",
            Self::InvalidForceGreetDonor { .. } => "invalid_force_greet_donor",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LegacyPackLoweringSummary {
    pub attempted_records: usize,
    pub lowered_records: usize,
    pub blocked_records: usize,
    pub blockers: BTreeMap<String, usize>,
}

pub fn summarize_legacy_pack_lowering_results<'a>(
    results: impl IntoIterator<Item = &'a Result<Record, LegacyPackLoweringError>>,
) -> LegacyPackLoweringSummary {
    let mut summary = LegacyPackLoweringSummary {
        attempted_records: 0,
        lowered_records: 0,
        blocked_records: 0,
        blockers: BTreeMap::new(),
    };
    for result in results {
        summary.attempted_records += 1;
        match result {
            Ok(_) => summary.lowered_records += 1,
            Err(error) => {
                summary.blocked_records += 1;
                *summary
                    .blockers
                    .entry(error.code().to_string())
                    .or_default() += 1;
            }
        }
    }
    summary
}

pub struct VerifiedFo4LegacyPackLowerer<'a, M> {
    interner: &'a StringInterner,
    reference_mapper: M,
}

impl<'a, M> VerifiedFo4LegacyPackLowerer<'a, M> {
    pub fn new(interner: &'a StringInterner, reference_mapper: M) -> Self {
        Self {
            interner,
            reference_mapper,
        }
    }
}

impl<M> VerifiedFo4LegacyPackLowerer<'_, M>
where
    M: LegacyPackReferenceMapper,
{
    /// Lower the one audited Renolds dialogue package by cloning the verified
    /// Fallout4.esm ForceGreet package rather than reconstructing its procedure
    /// tree. The caller must load `05B307:Fallout4.esm` through the FO4 schema.
    pub fn lower_audited_renolds_dialogue_with_donor(
        &self,
        inventory: &LegacyPackInventory,
        mapper: &mut FormKeyMapper<'_>,
        donor: &Record,
    ) -> Result<Record, LegacyPackLoweringError> {
        validate_audited_renolds_dialogue(inventory)?;
        validate_force_greet_donor(donor, self.interner)?;

        let condition = inventory.conditions.first().ok_or_else(|| {
            unsupported_shape(inventory, "renolds_dialogue_requires_getstage_condition")
        })?;
        let raw_ctda = condition
            .raw_ctda
            .as_ref()
            .ok_or_else(|| unsupported_shape(inventory, "renolds_dialogue_requires_raw_ctda"))?;
        let source_scope = [FieldEntry {
            sig: SubrecordSig(*b"CTDA"),
            value: FieldValue::Bytes(SmallVec::from_vec(raw_ctda.clone())),
        }];
        let (normalized_scope, _) =
            normalize_legacy_condition_scope(&source_scope, LegacyConditionFamily::Fnv, mapper)
                .map_err(|error| LegacyPackLoweringError::ConditionNormalizeFailed {
                    issue: format!("{error:?}"),
                })?;

        let source_owner = source_local_reference(inventory, VTECHATTICUP_QUEST_LOCAL)?;
        let target_owner = self
            .reference_mapper
            .remap_legacy_pack_reference(&source_owner)
            .ok_or_else(|| LegacyPackLoweringError::ReferenceRemapFailed {
                source_form_key: source_owner,
            })?;

        let mut output = donor.clone();
        output.form_key = FormKey::parse(&inventory.form_key, self.interner).map_err(|_| {
            LegacyPackLoweringError::InvalidRecordIdentity {
                identity: inventory.form_key.clone(),
            }
        })?;
        set_serialized_editor_id(&mut output, inventory.editor_id.as_deref(), self.interner);
        canonicalize_force_greet_union_fields(&mut output)?;
        let counter = output
            .fields
            .iter_mut()
            .find(|field| field.sig.0 == *b"PKCU")
            .ok_or_else(|| LegacyPackLoweringError::InvalidForceGreetDonor {
                issue: "missing_pkcu".to_string(),
            })?;
        counter.value = pack_counter_value(24, FO4_FORCE_GREET_TEMPLATE_LOCAL, 11, self.interner);
        output
            .fields
            .retain(|field| !matches!(&field.sig.0, b"CTDA" | b"CIS1" | b"CIS2" | b"QNAM"));
        let insert_at = output
            .fields
            .iter()
            .position(|field| field.sig.0 == *b"PKCU")
            .ok_or_else(|| LegacyPackLoweringError::InvalidForceGreetDonor {
                issue: "missing_pkcu".to_string(),
            })?;
        let mut replacement = normalized_scope;
        replacement.push(FieldEntry {
            sig: SubrecordSig(*b"QNAM"),
            value: FieldValue::FormKey(target_owner),
        });
        for field in replacement.into_iter().rev() {
            output.fields.insert(insert_at, field);
        }
        Ok(output)
    }
}

impl<M> LegacyPackLowerer for VerifiedFo4LegacyPackLowerer<'_, M>
where
    M: LegacyPackReferenceMapper,
{
    type Error = LegacyPackLoweringError;

    fn lower_supported_legacy_pack(
        &self,
        inventory: &LegacyPackInventory,
    ) -> Result<Record, Self::Error> {
        if !inventory.conditions.is_empty() {
            return Err(LegacyPackLoweringError::ConditionsRequirePort {
                count: inventory.conditions.len(),
            });
        }
        let audited_hostage_escape = is_exact_record(
            inventory,
            TEC_MINE_HOSTAGE_ESCAPE_LOCAL,
            "TecMineHostageEscape",
        );
        if audited_hostage_escape {
            validate_audited_hostage_escape(inventory)?;
        }
        let script_count = if audited_hostage_escape {
            0
        } else {
            inventory
                .scripts
                .iter()
                .filter(|script| script.requires_port)
                .count()
        };
        if script_count != 0 {
            return Err(LegacyPackLoweringError::EventScriptsRequirePort {
                count: script_count,
            });
        }
        if let Some(unresolved) = inventory.unions.iter().find(|union| {
            matches!(
                &union.payload,
                LegacyPackUnionPayload::Reference {
                    present: true,
                    source_form_key: None,
                }
            )
        }) {
            return Err(LegacyPackLoweringError::UnresolvedEncodedReference {
                sig: unresolved.sig.clone(),
            });
        }

        match inventory.package_type {
            LegacyPackType::Travel => lower_verified_travel(inventory, self.interner),
            LegacyPackType::Patrol => {
                lower_verified_patrol(inventory, self.interner, &self.reference_mapper)
            }
            _ => Err(LegacyPackLoweringError::UnsupportedPackageType {
                package_type_code: inventory.package_type_code,
            }),
        }
    }
}

fn has_all_day_schedule(inventory: &LegacyPackInventory) -> bool {
    inventory.schedule.month == -1
        && inventory.schedule.day_of_week == -1
        && inventory.schedule.date == 0
        && inventory.schedule.hour == -1
        && inventory.schedule.duration_hours == 0
}

fn has_three_empty_event_scripts(inventory: &LegacyPackInventory) -> bool {
    let events = inventory
        .scripts
        .iter()
        .map(|script| script.event)
        .collect::<Vec<_>>();
    events
        == [
            LegacyPackScriptEvent::OnBegin,
            LegacyPackScriptEvent::OnEnd,
            LegacyPackScriptEvent::OnChange,
        ]
        && inventory.scripts.iter().all(|script| {
            script.header_size == LEGACY_SCHR_LEN
                && script.declared_reference_count == 0
                && script.declared_compiled_size == 0
                && script.declared_variable_count == 0
                && script.compiled_payload.is_empty()
                && script.source_text.is_none()
                && script.local_variable_rows == 0
                && script.named_local_variables == 0
                && script.global_references == 0
                && script.local_references == 0
                && !script.idle_present
                && !script.topic_present
                && !script.requires_port
                && script.compiled_size_matches
                && script.reference_count_matches
                && script.variable_count_matches
        })
}

fn validate_audited_hostage_travel(
    inventory: &LegacyPackInventory,
) -> Result<(), LegacyPackLoweringError> {
    let [location] = inventory.unions.as_slice() else {
        return Err(unsupported_shape(
            inventory,
            "hostage_travel_union_count_changed",
        ));
    };
    let exact_shape = is_exact_record(
        inventory,
        TEC_MINE_HOSTAGE_TRAVEL_LOCAL,
        "TecMineHostagePackage",
    ) && inventory.package_type == LegacyPackType::Travel
        && inventory.pkdt.general_flags == 0x0400_1206
        && inventory.pkdt.fallout_behavior_flags == 0x20
        && inventory.pkdt.type_specific_flags == 55
        && inventory.pkdt.unused_1 == 0
        && inventory.pkdt.unused_2 == 877
        && inventory.pkdt.unused_bytes_nonzero
        && has_all_day_schedule(inventory)
        && location.sig == "PLDT"
        && location.union_kind
            == LegacyPackUnionKind::Location(LegacyPackLocationType::NearLinkedReference)
        && location.payload == LegacyPackUnionPayload::Implicit
        && location.radius_or_distance == 0
        && !location.trailing_unknown_nonzero
        && inventory.type_specific_subrecords.is_empty()
        && has_three_empty_event_scripts(inventory);
    if exact_shape {
        Ok(())
    } else {
        Err(unsupported_shape(
            inventory,
            "audited_hostage_travel_shape_changed",
        ))
    }
}

fn validate_audited_renolds_patrol(
    inventory: &LegacyPackInventory,
) -> Result<(), LegacyPackLoweringError> {
    let [path_start] = inventory.unions.as_slice() else {
        return Err(unsupported_shape(
            inventory,
            "renolds_patrol_union_count_changed",
        ));
    };
    let exact_reference = matches!(
        &path_start.payload,
        LegacyPackUnionPayload::Reference {
            present: true,
            source_form_key: Some(source),
        } if source_local(source) == Some(0x133F_3D)
    );
    let exact_shape = is_exact_record(
        inventory,
        RENOLDS_PATROL_PACK_LOCAL,
        "TechaticupNCRRenoldsPatrolPackage",
    ) && inventory.package_type == LegacyPackType::Patrol
        && inventory.pkdt.general_flags == 0
        && inventory.pkdt.fallout_behavior_flags == 0
        && inventory.pkdt.type_specific_flags == 0
        && inventory.pkdt.unused_1 == 0
        && inventory.pkdt.unused_2 == 3
        && inventory.pkdt.unused_bytes_nonzero
        && has_all_day_schedule(inventory)
        && path_start.sig == "PLDT"
        && path_start.union_kind
            == LegacyPackUnionKind::Location(LegacyPackLocationType::NearReference)
        && exact_reference
        && path_start.radius_or_distance == 0
        && !path_start.trailing_unknown_nonzero
        && inventory
            .patrol_data
            .as_ref()
            .is_some_and(|data| !data.repeatable && !data.unused_byte_nonzero)
        && has_three_empty_event_scripts(inventory);
    if exact_shape {
        Ok(())
    } else {
        Err(unsupported_shape(
            inventory,
            "audited_renolds_patrol_shape_changed",
        ))
    }
}

fn validate_audited_hostage_escape(
    inventory: &LegacyPackInventory,
) -> Result<(), LegacyPackLoweringError> {
    if !is_exact_record(
        inventory,
        TEC_MINE_HOSTAGE_ESCAPE_LOCAL,
        "TecMineHostageEscape",
    ) {
        return Err(unsupported_shape(
            inventory,
            "not_audited_hostage_escape_identity",
        ));
    }
    let [path_start] = inventory.unions.as_slice() else {
        return Err(unsupported_shape(
            inventory,
            "hostage_escape_union_count_changed",
        ));
    };
    let exact_path = matches!(
        &path_start.payload,
        LegacyPackUnionPayload::Reference {
            present: true,
            source_form_key: Some(source),
        } if source_local(source) == Some(0x0E70_CA)
    );
    let exact_scripts = inventory.scripts.iter().all(|script| {
        if script.event == LegacyPackScriptEvent::OnEnd {
            script.header_size == LEGACY_SCHR_LEN
                && script.declared_reference_count == 1
                && script.declared_compiled_size == TEC_MINE_HOSTAGE_ESCAPE_BYTECODE.len() as u32
                && script.declared_variable_count == 1
                && script.compiled_payload == TEC_MINE_HOSTAGE_ESCAPE_BYTECODE
                && script.source_text.as_deref() == Some(TEC_MINE_HOSTAGE_ESCAPE_SOURCE)
                && script.local_variable_rows == 1
                && script.named_local_variables == 1
                && script.global_references == 0
                && script.local_references == 1
                && !script.idle_present
                && !script.topic_present
                && script.requires_port
                && script.compiled_size_matches
                && script.reference_count_matches
                && script.variable_count_matches
        } else {
            !script.requires_port
                && script.compiled_size_matches
                && script.reference_count_matches
                && script.variable_count_matches
        }
    }) && inventory.scripts.len() == 3;
    let exact_shape = inventory.source == LegacyPackSourceFamily::Fnv
        && inventory.package_type == LegacyPackType::Patrol
        && inventory.pkdt.general_flags == 0x0408_3206
        && inventory.pkdt.fallout_behavior_flags == 0x20
        && inventory.pkdt.type_specific_flags == 0
        && inventory.pkdt.unused_1 == 103
        && inventory.pkdt.unused_2 == 21_572
        && inventory.pkdt.unused_bytes_nonzero
        && inventory.schedule.month == -1
        && inventory.schedule.day_of_week == -1
        && inventory.schedule.date == 0
        && inventory.schedule.hour == -1
        && inventory.schedule.duration_hours == 0
        && path_start.sig == "PLDT"
        && path_start.union_kind
            == LegacyPackUnionKind::Location(LegacyPackLocationType::NearReference)
        && exact_path
        && path_start.radius_or_distance == 0
        && !path_start.trailing_unknown_nonzero
        && inventory
            .patrol_data
            .as_ref()
            .is_some_and(|data| !data.repeatable && !data.unused_byte_nonzero)
        && exact_scripts;
    if exact_shape {
        Ok(())
    } else {
        Err(unsupported_shape(
            inventory,
            "audited_hostage_escape_shape_changed",
        ))
    }
}

fn validate_audited_renolds_dialogue(
    inventory: &LegacyPackInventory,
) -> Result<(), LegacyPackLoweringError> {
    if !is_exact_record(
        inventory,
        RENOLDS_DIALOGUE_PACK_LOCAL,
        "TechaticupNCRRenoldsDialoguePackage",
    ) {
        return Err(unsupported_shape(
            inventory,
            "not_audited_renolds_dialogue_identity",
        ));
    }
    let [target] = inventory.unions.as_slice() else {
        return Err(unsupported_shape(inventory, "renolds_target_count_changed"));
    };
    let [condition] = inventory.conditions.as_slice() else {
        return Err(unsupported_shape(
            inventory,
            "renolds_condition_count_changed",
        ));
    };
    let exact_shape = inventory.source == LegacyPackSourceFamily::Fnv
        && inventory.package_type == LegacyPackType::Dialogue
        && inventory.pkdt.general_flags == 0x2000
        && inventory.pkdt.fallout_behavior_flags == 0
        && inventory.pkdt.type_specific_flags == 0
        && inventory.pkdt.unused_1 == 0
        && inventory.pkdt.unused_2 == 0
        && !inventory.pkdt.unused_bytes_nonzero
        && inventory.schedule.month == -1
        && inventory.schedule.day_of_week == -1
        && inventory.schedule.date == 0
        && inventory.schedule.hour == -1
        && inventory.schedule.duration_hours == 0
        && target.sig == "PTDT"
        && target.union_kind
            == LegacyPackUnionKind::Target(LegacyPackTargetType::SpecificReference)
        && matches!(
            &target.payload,
            LegacyPackUnionPayload::Reference {
                present: true,
                source_form_key: Some(source),
            } if source_local(source) == Some(0x0000_14)
        )
        && target.radius_or_distance == 128
        && !target.trailing_unknown_nonzero
        && condition.observed_size == LEGACY_CTDA_LEN
        && !condition.cis1_present
        && !condition.cis2_present
        && condition.raw_ctda.as_deref() == Some(RENOLDS_GET_STAGE_CTDA.as_slice())
        && inventory.type_specific_subrecords.as_slice()
            == [LegacyPackSubrecordInventory {
                sig: "PKDD".to_string(),
                count: 1,
                observed_sizes: vec![24],
            }]
        && inventory.scripts.iter().all(|script| !script.requires_port);
    if exact_shape {
        Ok(())
    } else {
        Err(unsupported_shape(
            inventory,
            "audited_renolds_dialogue_shape_changed",
        ))
    }
}

fn validate_force_greet_donor(
    donor: &Record,
    interner: &StringInterner,
) -> Result<(), LegacyPackLoweringError> {
    let plugin = interner.resolve(donor.form_key.plugin).unwrap_or_default();
    let editor_id = donor.eid.and_then(|eid| interner.resolve(eid));
    if donor.sig.0 != *b"PACK"
        || donor.form_key.local != FO4_FORCE_GREET_DONOR_LOCAL
        || !plugin.eq_ignore_ascii_case("Fallout4.esm")
        || editor_id != Some("RETravelCC01_Forcegreet")
    {
        return Err(LegacyPackLoweringError::InvalidForceGreetDonor {
            issue: "identity_mismatch".to_string(),
        });
    }
    let counters = donor
        .fields
        .iter()
        .filter(|field| field.sig.0 == *b"PKCU")
        .collect::<Vec<_>>();
    let [counter] = counters.as_slice() else {
        return Err(LegacyPackLoweringError::InvalidForceGreetDonor {
            issue: "expected_one_pkcu".to_string(),
        });
    };
    let exact_counter = match &counter.value {
        FieldValue::Struct(fields) => {
            let template = struct_field(fields, "package_template", interner);
            struct_u32(fields, "data_input_count", interner) == Some(24)
                && struct_u32(fields, "version_counter_autoincremented", interner) == Some(11)
                && matches!(
                    template,
                    Some(FieldValue::FormKey(FormKey { local, plugin }))
                        if *local == FO4_FORCE_GREET_TEMPLATE_LOCAL
                            && interner.resolve(*plugin).is_some_and(|name| name.eq_ignore_ascii_case("Fallout4.esm"))
                )
        }
        FieldValue::Bytes(bytes) if bytes.len() == 12 => {
            read_u32(bytes, 0) == 24
                && read_u32(bytes, 4) == FO4_FORCE_GREET_TEMPLATE_LOCAL
                && read_u32(bytes, 8) == 11
        }
        _ => false,
    };
    let input_types = donor
        .fields
        .iter()
        .filter_map(|field| {
            (field.sig.0 == *b"ANAM").then(|| match &field.value {
                FieldValue::String(value) => interner.resolve(*value),
                _ => None,
            })
        })
        .collect::<Option<Vec<_>>>();
    let exact_inputs = input_types.as_deref() == Some(FO4_FORCE_GREET_INPUT_TYPES.as_slice());
    let unam = donor
        .fields
        .iter()
        .filter_map(|field| {
            (field.sig.0 == *b"UNAM").then(|| match &field.value {
                FieldValue::Bytes(bytes) if bytes.len() == 1 => Some(bytes[0]),
                value => numeric_value(value).and_then(|value| u8::try_from(value).ok()),
            })
        })
        .collect::<Option<Vec<_>>>();
    let exact_unam = unam.as_deref() == Some(FO4_FORCE_GREET_UNAM.as_slice());
    let union_signatures = donor
        .fields
        .iter()
        .filter(|field| matches!(&field.sig.0, b"PLDT" | b"PTDA"))
        .map(|field| field.sig.0)
        .collect::<Vec<_>>();
    let exact_unions = union_signatures
        == [
            *b"PLDT", *b"PLDT", *b"PLDT", *b"PTDA", *b"PLDT", *b"PTDA", *b"PTDA",
        ];
    let exact_tree_marker = donor.fields.iter().any(|field| {
        field.sig.0 == *b"XNAM"
            && matches!(&field.value, FieldValue::Bytes(bytes) if bytes.as_slice() == [0x0C])
    });
    if exact_counter && exact_inputs && exact_unam && exact_unions && exact_tree_marker {
        Ok(())
    } else {
        Err(LegacyPackLoweringError::InvalidForceGreetDonor {
            issue: "template_or_input_shape_mismatch".to_string(),
        })
    }
}

fn canonicalize_force_greet_union_fields(
    record: &mut Record,
) -> Result<(), LegacyPackLoweringError> {
    let canonical = canonical_force_greet_union_fields();
    let union_fields = record
        .fields
        .iter_mut()
        .filter(|field| matches!(&field.sig.0, b"PLDT" | b"PTDA"))
        .collect::<Vec<_>>();
    if union_fields.len() != canonical.len()
        || union_fields
            .iter()
            .zip(canonical.iter())
            .any(|(field, (sig, _))| field.sig.0 != *sig)
    {
        return Err(LegacyPackLoweringError::InvalidForceGreetDonor {
            issue: "union_shape_mismatch".to_string(),
        });
    }
    for (field, (_, bytes)) in union_fields.into_iter().zip(canonical) {
        field.value = FieldValue::Bytes(SmallVec::from_slice(bytes));
    }
    Ok(())
}

fn canonical_force_greet_union_fields() -> [([u8; 4], &'static [u8]); 7] {
    [
        (
            *b"PLDT",
            &[0x0C, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0],
        ),
        (
            *b"PLDT",
            &[0x0C, 0, 0, 0, 0, 0, 0, 0, 0xB8, 0x0B, 0, 0, 0, 0, 0, 0],
        ),
        (
            *b"PLDT",
            &[0, 0, 0, 0, 0x14, 0, 0, 0, 0x80, 0, 0, 0, 0, 0, 0, 0],
        ),
        (*b"PTDA", &[0, 0, 0, 0, 0x14, 0, 0, 0, 0, 0, 0, 0]),
        (
            *b"PLDT",
            &[0, 0, 0, 0, 0x14, 0, 0, 0, 0x88, 0x13, 0, 0, 0, 0, 0, 0],
        ),
        (*b"PTDA", &[2, 0, 0, 0, 0x14, 0, 0, 0, 0, 0, 0, 0]),
        (*b"PTDA", &[2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]),
    ]
}

fn source_local_reference(
    inventory: &LegacyPackInventory,
    local: u32,
) -> Result<String, LegacyPackLoweringError> {
    let Some((_, plugin)) = inventory.form_key.split_once('@') else {
        return Err(LegacyPackLoweringError::InvalidRecordIdentity {
            identity: inventory.form_key.clone(),
        });
    };
    Ok(format!("{local:06X}@{plugin}"))
}

fn source_local(source: &str) -> Option<u32> {
    u32::from_str_radix(source.split(['@', ':']).next()?, 16).ok()
}

fn is_exact_identity(
    source: LegacyPackSourceFamily,
    form_key: &str,
    editor_id: Option<&str>,
    local: u32,
    expected_editor_id: &str,
) -> bool {
    let plugin_matches = form_key
        .split_once('@')
        .is_some_and(|(_, plugin)| plugin.eq_ignore_ascii_case(AUDITED_SOURCE_PLUGIN));
    source == LegacyPackSourceFamily::Fnv
        && plugin_matches
        && source_local(form_key) == Some(local)
        && editor_id.is_some_and(|actual| actual.eq_ignore_ascii_case(expected_editor_id))
}

fn is_exact_record(inventory: &LegacyPackInventory, local: u32, editor_id: &str) -> bool {
    is_exact_identity(
        inventory.source,
        &inventory.form_key,
        inventory.editor_id.as_deref(),
        local,
        editor_id,
    )
}

fn lower_verified_travel(
    inventory: &LegacyPackInventory,
    interner: &StringInterner,
) -> Result<Record, LegacyPackLoweringError> {
    validate_audited_hostage_travel(inventory)?;
    let [location] = inventory.unions.as_slice() else {
        return Err(unsupported_shape(inventory, "travel_requires_one_location"));
    };
    if location.sig != "PLDT"
        || location.union_kind
            != LegacyPackUnionKind::Location(LegacyPackLocationType::NearLinkedReference)
        || location.payload != LegacyPackUnionPayload::Implicit
        || !inventory.type_specific_subrecords.is_empty()
    {
        return Err(unsupported_shape(
            inventory,
            "travel_requires_near_linked_reference_without_type_specific_data",
        ));
    }

    let mut output = target_record(inventory, interner)?;
    push_common_header(&mut output, inventory);
    push_pack_counter(&mut output, 4, FO4_TRAVEL_TEMPLATE_LOCAL, 1, interner);
    push_string(&mut output, b"ANAM", "Location", interner);
    let mut location_data = Vec::with_capacity(16);
    location_data.extend_from_slice(&6_i32.to_le_bytes());
    location_data.extend_from_slice(&0_u32.to_le_bytes());
    location_data.extend_from_slice(&location.radius_or_distance.to_le_bytes());
    location_data.extend_from_slice(&0_u32.to_le_bytes());
    push_bytes(&mut output, b"PLDT", location_data);
    for _ in 0..3 {
        push_string(&mut output, b"ANAM", "Bool", interner);
        push_bytes(&mut output, b"CNAM", vec![0]);
    }
    for index in [1_u8, 3, 5, 7] {
        push_bytes(&mut output, b"UNAM", vec![index]);
    }
    push_bytes(&mut output, b"XNAM", vec![0x08]);
    push_empty_fo4_event_blocks(&mut output);
    Ok(output)
}

fn lower_verified_patrol<M>(
    inventory: &LegacyPackInventory,
    interner: &StringInterner,
    reference_mapper: &M,
) -> Result<Record, LegacyPackLoweringError>
where
    M: LegacyPackReferenceMapper,
{
    if is_exact_record(
        inventory,
        RENOLDS_PATROL_PACK_LOCAL,
        "TechaticupNCRRenoldsPatrolPackage",
    ) {
        validate_audited_renolds_patrol(inventory)?;
    } else if is_exact_record(
        inventory,
        TEC_MINE_HOSTAGE_ESCAPE_LOCAL,
        "TecMineHostageEscape",
    ) {
        validate_audited_hostage_escape(inventory)?;
    } else {
        return Err(unsupported_shape(
            inventory,
            "not_an_audited_vtechatticup_patrol_identity",
        ));
    }
    let [path_start] = inventory.unions.as_slice() else {
        return Err(unsupported_shape(
            inventory,
            "patrol_requires_one_path_start",
        ));
    };
    let LegacyPackUnionPayload::Reference {
        present: true,
        source_form_key: Some(source_form_key),
    } = &path_start.payload
    else {
        return Err(unsupported_shape(
            inventory,
            "patrol_requires_specific_reference_path_start",
        ));
    };
    if path_start.sig != "PLDT"
        || path_start.union_kind
            != LegacyPackUnionKind::Location(LegacyPackLocationType::NearReference)
        || inventory.type_specific_subrecords.as_slice()
            != [LegacyPackSubrecordInventory {
                sig: "PKPT".to_string(),
                count: 1,
                observed_sizes: vec![2],
            }]
    {
        return Err(unsupported_shape(
            inventory,
            "patrol_requires_pldt_reference_and_one_pkpt",
        ));
    }
    let patrol = inventory
        .patrol_data
        .as_ref()
        .ok_or_else(|| unsupported_shape(inventory, "patrol_data_missing"))?;
    let target_path_start = reference_mapper
        .remap_legacy_pack_reference(source_form_key)
        .ok_or_else(|| LegacyPackLoweringError::ReferenceRemapFailed {
            source_form_key: source_form_key.clone(),
        })?;

    let mut output = target_record(inventory, interner)?;
    push_common_header(&mut output, inventory);
    push_pack_counter(&mut output, 7, FO4_PATROL_TEMPLATE_LOCAL, 2, interner);
    push_string(&mut output, b"ANAM", "SingleRef", interner);
    push_struct(
        &mut output,
        b"PTDA",
        [
            ("target_data_type", FieldValue::Int(0)),
            ("target_data_target", FieldValue::FormKey(target_path_start)),
            ("target_data_count_distance", FieldValue::Int(0)),
        ],
        interner,
    );
    push_string(&mut output, b"ANAM", "Float", interner);
    push_value(
        &mut output,
        b"CNAM",
        FieldValue::Float(path_start.radius_or_distance.max(0) as f32),
    );
    for value in [patrol.repeatable, false, false, false] {
        push_string(&mut output, b"ANAM", "Bool", interner);
        push_value(&mut output, b"CNAM", FieldValue::Bool(value));
    }
    push_string(&mut output, b"ANAM", "Float", interner);
    push_value(&mut output, b"CNAM", FieldValue::Float(0.0));
    for index in [0_u8, 1, 2, 4, 6, 8, 10] {
        push_bytes(&mut output, b"UNAM", vec![index]);
    }
    push_bytes(&mut output, b"XNAM", vec![0x0B]);
    push_empty_fo4_event_blocks(&mut output);
    Ok(output)
}

fn unsupported_shape(inventory: &LegacyPackInventory, issue: &str) -> LegacyPackLoweringError {
    LegacyPackLoweringError::UnsupportedPackageShape {
        package_type_code: inventory.package_type_code,
        issue: issue.to_string(),
    }
}

fn target_record(
    inventory: &LegacyPackInventory,
    interner: &StringInterner,
) -> Result<Record, LegacyPackLoweringError> {
    let form_key = FormKey::parse(&inventory.form_key, interner).map_err(|_| {
        LegacyPackLoweringError::InvalidRecordIdentity {
            identity: inventory.form_key.clone(),
        }
    })?;
    let mut output = Record::new(SigCode(*b"PACK"), form_key);
    set_serialized_editor_id(&mut output, inventory.editor_id.as_deref(), interner);
    Ok(output)
}

fn set_serialized_editor_id(
    record: &mut Record,
    editor_id: Option<&str>,
    interner: &StringInterner,
) {
    record.fields.retain(|field| field.sig.0 != *b"EDID");
    record.eid = editor_id.map(|value| interner.intern(value));
    if let Some(editor_id) = record.eid {
        record.fields.insert(
            0,
            FieldEntry {
                sig: SubrecordSig(*b"EDID"),
                value: FieldValue::String(editor_id),
            },
        );
    }
}

fn push_common_header(output: &mut Record, inventory: &LegacyPackInventory) {
    let mut pack_data = [0_u8; 12];
    pack_data[..4].copy_from_slice(
        &(inventory.pkdt.general_flags & FO4_SHARED_LEGACY_GENERAL_FLAGS).to_le_bytes(),
    );
    pack_data[4] = 18;
    pack_data[6] = 2;
    pack_data[8..10].copy_from_slice(&inventory.pkdt.fallout_behavior_flags.to_le_bytes());
    push_bytes(output, b"PKDT", pack_data.to_vec());

    let mut schedule = [0_u8; 12];
    schedule[0] = inventory.schedule.month as u8;
    schedule[1] = inventory.schedule.day_of_week as u8;
    schedule[2] = inventory.schedule.date.max(0) as u8;
    schedule[3] = inventory.schedule.hour as u8;
    schedule[4] = u8::MAX;
    schedule[8..12].copy_from_slice(&inventory.schedule.duration_hours.to_le_bytes());
    push_bytes(output, b"PSDT", schedule.to_vec());
}

fn push_pack_counter(
    output: &mut Record,
    data_input_count: u32,
    template_local: u32,
    version_counter: u32,
    interner: &StringInterner,
) {
    push_value(
        output,
        b"PKCU",
        pack_counter_value(data_input_count, template_local, version_counter, interner),
    );
}

fn pack_counter_value(
    data_input_count: u32,
    template_local: u32,
    version_counter: u32,
    interner: &StringInterner,
) -> FieldValue {
    let template = FormKey {
        local: template_local,
        plugin: interner.intern("Fallout4.esm"),
    };
    FieldValue::Struct(
        [
            (
                interner.intern("data_input_count"),
                FieldValue::Uint(u64::from(data_input_count)),
            ),
            (
                interner.intern("package_template"),
                FieldValue::FormKey(template),
            ),
            (
                interner.intern("version_counter_autoincremented"),
                FieldValue::Uint(u64::from(version_counter)),
            ),
        ]
        .into_iter()
        .collect(),
    )
}

fn push_empty_fo4_event_blocks(output: &mut Record) {
    for marker in [b"POBA", b"POEA", b"POCA"] {
        push_value(output, marker, FieldValue::None);
        push_bytes(output, b"INAM", vec![0; 4]);
        push_bytes(output, b"PDTO", vec![0; 8]);
    }
}

fn push_struct<const N: usize>(
    output: &mut Record,
    sig: &[u8; 4],
    fields: [(&str, FieldValue); N],
    interner: &StringInterner,
) {
    push_value(
        output,
        sig,
        FieldValue::Struct(
            fields
                .into_iter()
                .map(|(name, value)| (interner.intern(name), value))
                .collect(),
        ),
    );
}

fn push_string(output: &mut Record, sig: &[u8; 4], value: &str, interner: &StringInterner) {
    push_value(output, sig, FieldValue::String(interner.intern(value)));
}

fn push_bytes(output: &mut Record, sig: &[u8; 4], bytes: Vec<u8>) {
    push_value(output, sig, FieldValue::Bytes(SmallVec::from_vec(bytes)));
}

fn push_value(output: &mut Record, sig: &[u8; 4], value: FieldValue) {
    output.fields.push(FieldEntry {
        sig: SubrecordSig(*sig),
        value,
    });
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
    let unions = classify_unions(record, source, interner, &mut rejections);
    let (scripts, script_owned) = classify_scripts(record, interner, &mut rejections);
    reject_orphan_script_fields(record, &script_owned, &mut rejections);
    let (type_specific_subrecords, patrol_data, use_weapon_data) =
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
    let mut lowering_blockers = Vec::new();
    if !has_verified_fo4_blueprint(
        source,
        &form_key,
        editor_id.as_deref(),
        pkdt.package_type,
        &unions,
        &type_specific_subrecords,
        patrol_data.as_ref(),
    ) {
        lowering_blockers.push(LegacyPackLoweringBlocker::NoVerifiedFo4ProcedureBlueprint {
            package_type_code: pkdt.package_type_code,
        });
    }
    if !conditions.is_empty() {
        lowering_blockers.push(LegacyPackLoweringBlocker::LegacyConditionsRequireSemanticLowering);
    }
    if scripts.iter().any(|script| script.requires_port) {
        lowering_blockers.push(LegacyPackLoweringBlocker::LegacyEventScriptsRequirePort);
    }
    if unions.iter().any(|union| {
        matches!(
            &union.payload,
            LegacyPackUnionPayload::Reference {
                present: true,
                source_form_key: None,
            }
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

    let mut inventory = LegacyPackInventory {
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
        patrol_data,
        use_weapon_data,
        support: LegacyPackSupport {
            classification_supported: true,
            lowering_supported: lowering_blockers.is_empty(),
            lowering_blockers,
        },
    };
    let audited_shape_verified = match inventory.package_type {
        LegacyPackType::Travel => validate_audited_hostage_travel(&inventory).is_ok(),
        LegacyPackType::Patrol => {
            validate_audited_renolds_patrol(&inventory).is_ok()
                || validate_audited_hostage_escape(&inventory).is_ok()
        }
        _ => true,
    };
    if !audited_shape_verified
        && !inventory.support.lowering_blockers.iter().any(|blocker| {
            matches!(
                blocker,
                LegacyPackLoweringBlocker::NoVerifiedFo4ProcedureBlueprint { .. }
            )
        })
    {
        inventory.support.lowering_blockers.push(
            LegacyPackLoweringBlocker::NoVerifiedFo4ProcedureBlueprint {
                package_type_code: inventory.package_type_code,
            },
        );
        inventory.support.lowering_supported = false;
    }
    if is_exact_record(
        &inventory,
        TEC_MINE_HOSTAGE_ESCAPE_LOCAL,
        "TecMineHostageEscape",
    ) && validate_audited_hostage_escape(&inventory).is_ok()
    {
        inventory
            .support
            .lowering_blockers
            .retain(|blocker| *blocker != LegacyPackLoweringBlocker::LegacyEventScriptsRequirePort);
        inventory.support.lowering_supported = inventory.support.lowering_blockers.is_empty();
    }
    LegacyPackClassificationReport {
        source,
        status: LegacyPackClassificationStatus::Accepted,
        inventory: Some(inventory),
        rejection_reasons: Vec::new(),
    }
}

fn has_verified_fo4_blueprint(
    source: LegacyPackSourceFamily,
    form_key: &str,
    editor_id: Option<&str>,
    package_type: LegacyPackType,
    unions: &[LegacyPackUnionInventory],
    type_specific: &[LegacyPackSubrecordInventory],
    patrol: Option<&LegacyPackPatrolData>,
) -> bool {
    match package_type {
        LegacyPackType::Travel => {
            is_exact_identity(
                source,
                form_key,
                editor_id,
                TEC_MINE_HOSTAGE_TRAVEL_LOCAL,
                "TecMineHostagePackage",
            ) && matches!(
                unions,
                [LegacyPackUnionInventory {
                    sig,
                    union_kind: LegacyPackUnionKind::Location(
                        LegacyPackLocationType::NearLinkedReference
                    ),
                    payload: LegacyPackUnionPayload::Implicit,
                    ..
                }] if sig == "PLDT" && type_specific.is_empty()
            )
        }
        LegacyPackType::Patrol => {
            let exact_identity = is_exact_identity(
                source,
                form_key,
                editor_id,
                RENOLDS_PATROL_PACK_LOCAL,
                "TechaticupNCRRenoldsPatrolPackage",
            ) || is_exact_identity(
                source,
                form_key,
                editor_id,
                TEC_MINE_HOSTAGE_ESCAPE_LOCAL,
                "TecMineHostageEscape",
            );
            exact_identity
                && matches!(
                    unions,
                    [LegacyPackUnionInventory {
                        sig,
                        union_kind: LegacyPackUnionKind::Location(
                            LegacyPackLocationType::NearReference
                        ),
                        payload: LegacyPackUnionPayload::Reference {
                            present: true,
                            source_form_key: Some(_),
                        },
                        ..
                    }] if sig == "PLDT"
                        && type_specific == [LegacyPackSubrecordInventory {
                            sig: "PKPT".to_string(),
                            count: 1,
                            observed_sizes: vec![2],
                        }]
                        && patrol.is_some_and(|data| !data.unused_byte_nonzero)
                )
        }
        _ => false,
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
    let (general_flags, type_code, fallout_behavior_flags, type_specific_flags, unused_1, unused_2) =
        match value {
            FieldValue::Bytes(bytes) if bytes.len() == LEGACY_PKDT_LEN => (
                read_u32(bytes, 0),
                bytes[4],
                read_u16(bytes, 6),
                read_u16(bytes, 8),
                bytes[5],
                read_u16(bytes, 10),
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
                let unused_1 = struct_u64(fields, "unused_1", interner).unwrap_or(0);
                let unused_2 = struct_u64(fields, "unused_2", interner).unwrap_or(0);
                let (Ok(unused_1), Ok(unused_2)) =
                    (u8::try_from(unused_1), u16::try_from(unused_2))
                else {
                    return Err(malformed("PKDT", field_index, &[LEGACY_PKDT_LEN], None));
                };
                (
                    general_flags,
                    type_code,
                    fallout_behavior_flags,
                    type_specific_flags,
                    unused_1,
                    unused_2,
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
        unused_1,
        unused_2,
        unused_bytes_nonzero: unused_1 != 0 || unused_2 != 0,
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
                let condition_field_index = index;
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
                    raw_ctda: match &record.fields[condition_field_index].value {
                        FieldValue::Bytes(bytes) => Some(bytes.to_vec()),
                        _ => None,
                    },
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
    source: LegacyPackSourceFamily,
    interner: &StringInterner,
    rejections: &mut Vec<LegacyPackRejectionReason>,
) -> Vec<LegacyPackUnionInventory> {
    let raw_reference_plugin = audited_raw_reference_plugin(record, source, interner);
    let mut unions = Vec::new();
    for (field_index, field) in record.fields.iter().enumerate() {
        if !matches!(&field.sig.0, b"PLDT" | b"PLD2" | b"PTDT" | b"PTD2") {
            continue;
        }
        match parse_union(field, field_index, raw_reference_plugin, interner) {
            Ok(union) => unions.push(union),
            Err(reason) => rejections.push(reason),
        }
    }
    unions
}

fn audited_raw_reference_plugin<'a>(
    record: &Record,
    source: LegacyPackSourceFamily,
    interner: &'a StringInterner,
) -> Option<&'a str> {
    if source != LegacyPackSourceFamily::Fnv {
        return None;
    }
    let plugin = interner.resolve(record.form_key.plugin)?;
    if !plugin.eq_ignore_ascii_case(AUDITED_SOURCE_PLUGIN) {
        return None;
    }
    let editor_id = record.eid.and_then(|eid| interner.resolve(eid))?;
    let exact_patrol = record.form_key.local == RENOLDS_PATROL_PACK_LOCAL
        && editor_id.eq_ignore_ascii_case("TechaticupNCRRenoldsPatrolPackage");
    let exact_escape = record.form_key.local == TEC_MINE_HOSTAGE_ESCAPE_LOCAL
        && editor_id.eq_ignore_ascii_case("TecMineHostageEscape");
    let exact_dialogue = record.form_key.local == RENOLDS_DIALOGUE_PACK_LOCAL
        && editor_id.eq_ignore_ascii_case("TechaticupNCRRenoldsDialoguePackage");
    (exact_patrol || exact_escape || exact_dialogue).then_some(plugin)
}

fn parse_union(
    field: &FieldEntry,
    field_index: usize,
    raw_reference_plugin: Option<&str>,
    interner: &StringInterner,
) -> Result<LegacyPackUnionInventory, LegacyPackRejectionReason> {
    let is_location = matches!(&field.sig.0, b"PLDT" | b"PLD2");
    let expected_size = if is_location {
        LEGACY_LOCATION_LEN
    } else {
        LEGACY_TARGET_LEN
    };
    let (
        type_code,
        payload_value,
        payload_is_reference,
        source_form_key,
        radius_or_distance,
        trailing_nonzero,
    ) = match &field.value {
        FieldValue::Bytes(bytes) if bytes.len() == expected_size => {
            let type_code = read_u32(bytes, 0);
            let payload_value = read_u32(bytes, 4);
            let reference_typed = if is_location {
                matches!(type_code, 0 | 1 | 4)
            } else {
                matches!(type_code, 0 | 1)
            };
            let source_form_key = raw_reference_plugin
                .filter(|_| {
                    reference_typed && payload_value != 0 && payload_value & 0xFF00_0000 == 0
                })
                .map(|plugin| format!("{payload_value:06X}@{plugin}"));
            (
                type_code,
                payload_value,
                source_form_key.is_some(),
                source_form_key,
                read_i32(bytes, 8),
                !is_location && read_u32(bytes, 12) != 0,
            )
        }
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
            let source_form_key = match payload {
                FieldValue::FormKey(form_key) if form_key.local != 0 => {
                    Some(form_key.format(interner))
                }
                _ => None,
            };
            let payload_value = numeric_value(payload).unwrap_or(u64::from(payload_is_reference));
            let trailing_nonzero =
                struct_field(fields, "unknown", interner).is_some_and(value_nonzero);
            (
                type_code,
                u32::try_from(payload_value).unwrap_or(u32::MAX),
                payload_is_reference,
                source_form_key,
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
                source_form_key,
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
                    source_form_key,
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
    let compiled_payload = compiled
        .first()
        .and_then(|field| match &field.value {
            FieldValue::Bytes(bytes) => Some(bytes.to_vec()),
            _ => None,
        })
        .unwrap_or_default();
    let source_payload_size = sources
        .first()
        .and_then(|field| value_size(&field.value, interner))
        .unwrap_or(0);
    let source_text = sources.first().and_then(|field| match &field.value {
        FieldValue::Bytes(bytes) => std::str::from_utf8(bytes).ok().map(str::to_owned),
        FieldValue::String(value) => interner.resolve(*value).map(str::to_owned),
        _ => None,
    });
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
    let idle_present = fields
        .iter()
        .any(|field| field.sig.0 == *b"INAM" && value_nonzero(&field.value));
    let topic_present = fields
        .iter()
        .any(|field| field.sig.0 == *b"TNAM" && value_nonzero(&field.value));
    let requires_port = compiled_payload_size != 0
        || source_payload_size != 0
        || local_variable_rows != 0
        || named_local_variables != 0
        || global_references != 0
        || local_references != 0
        || idle_present
        || topic_present;
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
        idle_present,
        topic_present,
        requires_port,
        compiled_payload,
        source_text,
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
    Option<LegacyPackPatrolData>,
    Option<LegacyPackUseWeaponData>,
) {
    let mut inventory = Vec::new();
    let mut patrol = None;
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
        if sig == *b"PKPT" && matches.len() == 1 {
            match parse_patrol_data(&matches[0].1.value, matches[0].0, interner) {
                Ok(decoded) => patrol = Some(decoded),
                Err(reason) => rejections.push(reason),
            }
        }
        inventory.push(LegacyPackSubrecordInventory {
            sig: sig_text(sig),
            count: matches.len(),
            observed_sizes,
        });
    }
    (inventory, patrol, use_weapon)
}

fn parse_patrol_data(
    value: &FieldValue,
    field_index: usize,
    interner: &StringInterner,
) -> Result<LegacyPackPatrolData, LegacyPackRejectionReason> {
    match value {
        FieldValue::Bytes(bytes) if bytes.len() == 2 => Ok(LegacyPackPatrolData {
            repeatable: bytes[0] != 0,
            unused_byte_nonzero: bytes[1] != 0,
        }),
        FieldValue::Struct(fields) => {
            let Some(repeatable) = struct_u8(fields, "repeatable", interner) else {
                return Err(malformed("PKPT", field_index, &[2], None));
            };
            Ok(LegacyPackPatrolData {
                repeatable: repeatable != 0,
                unused_byte_nonzero: struct_u8(fields, "unknown_u8_1", interner).unwrap_or(0) != 0,
            })
        }
        other => Err(malformed(
            "PKPT",
            field_index,
            &[2],
            value_size(other, interner),
        )),
    }
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
    use crate::formkey_mapper::MapperOptions;
    use crate::ids::{FormKey, SigCode, SubrecordSig};
    use crate::schema::AuthoringSchema;
    use esp_authoring_core::plugin_runtime::{
        ParsedItem, ParsedRecord, plugin_handle_add_master_native, plugin_handle_close_native,
        plugin_handle_load_no_py, plugin_handle_new_no_py, plugin_handle_save_no_py,
        plugin_handle_store_ref,
    };
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

    fn exact_pkdt(
        package_type: LegacyPackType,
        general_flags: u32,
        behavior_flags: u16,
        type_specific_flags: u16,
        unused_1: u8,
        unused_2: u16,
    ) -> FieldEntry {
        let mut bytes = [0_u8; LEGACY_PKDT_LEN];
        bytes[..4].copy_from_slice(&general_flags.to_le_bytes());
        bytes[4] = package_type.code();
        bytes[5] = unused_1;
        bytes[6..8].copy_from_slice(&behavior_flags.to_le_bytes());
        bytes[8..10].copy_from_slice(&type_specific_flags.to_le_bytes());
        bytes[10..12].copy_from_slice(&unused_2.to_le_bytes());
        field(b"PKDT", &bytes)
    }

    fn all_day_schedule() -> FieldEntry {
        let mut bytes = [0_u8; LEGACY_PSDT_LEN];
        bytes[..4].copy_from_slice(&[u8::MAX, u8::MAX, 0, u8::MAX]);
        field(b"PSDT", &bytes)
    }

    fn push_empty_legacy_events(record: &mut Record) {
        for marker in [b"POBA", b"POEA", b"POCA"] {
            record.fields.extend(empty_script(marker));
        }
    }

    fn push_force_greet_inputs(record: &mut Record, interner: &StringInterner) {
        let mut push_input = |kind: &str, sig: &[u8; 4], data: &[u8]| {
            push_string(record, b"ANAM", kind, interner);
            record.fields.push(field(sig, data));
        };
        push_input("Topic", b"PDTO", &[1, 0, 0, 0, b'H', b'E', b'L', b'O']);
        push_input(
            "Location",
            b"PLDT",
            &[0x0C, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0],
        );
        push_input(
            "Location",
            b"PLDT",
            &[0x0C, 0, 0, 0, 0, 0, 0, 0, 0xB8, 0x0B, 0, 0, 0, 0, 0, 0],
        );
        push_input(
            "Location",
            b"PLDT",
            &[0, 0, 0, 0, 0x14, 0, 0, 0, 0x80, 0, 0, 0, 0, 0, 0, 0],
        );
        push_input("Bool", b"CNAM", &[1]);
        push_input(
            "SingleRef",
            b"PTDA",
            &[0, 0, 0, 0, 0x14, 0, 0, 0, 0, 0, 0, 0],
        );
        push_input(
            "Location",
            b"PLDT",
            &[0, 0, 0, 0, 0x14, 0, 0, 0, 0x88, 0x13, 0, 0, 0, 0, 0],
        );
        for value in [1_u8, 1, 0, 1, 0, 0, 1, 0, 0, 1, 0, 0, 0] {
            push_input("Bool", b"CNAM", &[value]);
        }
        push_input(
            "TargetSelector",
            b"PTDA",
            &[2, 0, 0, 0, 0x14, 0, 0, 0, 0, 0, 0, 0],
        );
        push_input("Float", b"CNAM", &[0; 4]);
        push_input("Int", b"CNAM", &[0; 4]);
        push_input(
            "TargetSelector",
            b"PTDA",
            &[2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
        );
        for index in FO4_FORCE_GREET_UNAM {
            record.fields.push(field(b"UNAM", &[index]));
        }
        record.fields.push(field(b"XNAM", &[0x0C]));
    }

    fn signature_order(record: &Record) -> Vec<String> {
        record
            .fields
            .iter()
            .map(|field| field.sig.as_str().to_string())
            .collect()
    }

    fn string_field_values(
        record: &Record,
        sig: &[u8; 4],
        interner: &StringInterner,
    ) -> Vec<String> {
        record
            .fields
            .iter()
            .filter_map(|field| {
                if field.sig.0 != *sig {
                    return None;
                }
                let FieldValue::String(value) = &field.value else {
                    return None;
                };
                interner.resolve(*value).map(str::to_owned)
            })
            .collect()
    }

    fn one_byte_field_values(record: &Record, sig: &[u8; 4]) -> Vec<u8> {
        record
            .fields
            .iter()
            .filter_map(|field| {
                if field.sig.0 != *sig {
                    return None;
                }
                let FieldValue::Bytes(bytes) = &field.value else {
                    return None;
                };
                (bytes.len() == 1).then_some(bytes[0])
            })
            .collect()
    }

    fn find_parsed_record<'a>(
        items: &'a [ParsedItem],
        signature: &str,
        local: u32,
    ) -> Option<&'a ParsedRecord> {
        for item in items {
            match item {
                ParsedItem::Record(record)
                    if record.signature.as_str() == signature
                        && (record.form_id & 0x00FF_FFFF) == local =>
                {
                    return Some(record);
                }
                ParsedItem::Group(group) => {
                    if let Some(record) = find_parsed_record(&group.children, signature, local) {
                        return Some(record);
                    }
                }
                _ => {}
            }
        }
        None
    }

    fn assert_pack_editor_ids_survive_save_reopen(
        records: &[(&Record, &str, u32)],
        interner: &StringInterner,
    ) {
        let schema = AuthoringSchema::for_game("fo4").expect("FO4 schema loads");
        let handle = plugin_handle_new_no_py(AUDITED_SOURCE_PLUGIN, Some("fo4"));
        let masters = [
            "Fallout4.esm",
            "DLCRobot.esm",
            "DLCworkshop01.esm",
            "DLCCoast.esm",
            "DLCworkshop02.esm",
            "DLCworkshop03.esm",
            "DLCNukaWorld.esm",
        ];
        for master in masters {
            plugin_handle_add_master_native(handle, master, None).expect("add FO4 target master");
        }
        for (record, expected_editor_id, expected_template_local) in records {
            assert_eq!(record.fields[0].sig.0, *b"EDID");
            assert!(matches!(
                &record.fields[0].value,
                FieldValue::String(value) if interner.resolve(*value) == Some(*expected_editor_id)
            ));
            let counter = record
                .fields
                .iter()
                .find(|field| field.sig.0 == *b"PKCU")
                .expect("typed PKCU");
            let FieldValue::Struct(counter) = &counter.value else {
                panic!("PKCU must retain typed FormKey ownership before save");
            };
            assert!(matches!(
                struct_field(counter, "package_template", interner),
                Some(FieldValue::FormKey(FormKey { local, plugin }))
                    if *local == *expected_template_local
                        && interner.resolve(*plugin).is_some_and(|name| name.eq_ignore_ascii_case("Fallout4.esm"))
            ));
            crate::target_write::add_record_native(handle, (*record).clone(), &schema, interner)
                .expect("insert PACK");
        }

        let directory = tempfile::tempdir().expect("temp output");
        let path = directory.path().join(AUDITED_SOURCE_PLUGIN);
        plugin_handle_save_no_py(handle, path.to_str().unwrap()).expect("save PACK plugin");
        assert!(plugin_handle_close_native(handle));
        let reopened =
            plugin_handle_load_no_py(path.to_str().unwrap(), Some("fo4"), None, None, true)
                .expect("reopen PACK plugin");
        let store = plugin_handle_store_ref().lock().unwrap();
        let slot = store.get(&reopened).expect("reopened slot");
        assert_eq!(
            slot.parsed
                .header
                .masters
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            masters
        );
        for (record, expected_editor_id, expected_template_local) in records {
            let parsed = find_parsed_record(&slot.parsed.root_items, "PACK", record.form_key.local)
                .expect("saved PACK");
            assert_eq!(
                parsed
                    .subrecords
                    .iter()
                    .filter(|subrecord| subrecord.signature.as_str() == "EDID")
                    .count(),
                1
            );
            let first = parsed.subrecords.first().expect("first PACK subrecord");
            assert_eq!(first.signature.as_str(), "EDID");
            let bytes = first.data.as_ref();
            assert_eq!(
                std::str::from_utf8(bytes.strip_suffix(&[0]).unwrap_or(bytes)).unwrap(),
                *expected_editor_id
            );
            let counter = parsed
                .subrecords
                .iter()
                .find(|subrecord| subrecord.signature.as_str() == "PKCU")
                .expect("saved PKCU");
            assert_eq!(counter.data.len(), 12);
            assert_eq!(read_u32(counter.data.as_ref(), 4), *expected_template_local);
            if *expected_template_local == FO4_FORCE_GREET_TEMPLATE_LOCAL {
                let saved_unions = parsed
                    .subrecords
                    .iter()
                    .filter(|subrecord| matches!(subrecord.signature.as_str(), "PLDT" | "PTDA"))
                    .collect::<Vec<_>>();
                let canonical = canonical_force_greet_union_fields();
                assert_eq!(saved_unions.len(), canonical.len());
                for (saved, (expected_sig, expected_data)) in
                    saved_unions.into_iter().zip(canonical)
                {
                    assert_eq!(saved.signature.as_str().as_bytes(), expected_sig);
                    assert_eq!(saved.data.as_ref(), expected_data);
                }
                let saved_unam = parsed
                    .subrecords
                    .iter()
                    .filter(|subrecord| subrecord.signature.as_str() == "UNAM")
                    .map(|subrecord| subrecord.data.as_ref())
                    .collect::<Vec<_>>();
                assert_eq!(saved_unam.len(), FO4_FORCE_GREET_UNAM.len());
                assert!(
                    saved_unam
                        .iter()
                        .zip(FO4_FORCE_GREET_UNAM)
                        .all(|(saved, expected)| *saved == [expected])
                );
            }
        }
        drop(store);
        assert!(plugin_handle_close_native(reopened));
    }

    fn inventory(record: &Record, interner: &StringInterner) -> LegacyPackInventory {
        let report = classify_legacy_pack(record, LegacyPackSourceFamily::Fnv, interner);
        assert_eq!(report.status, LegacyPackClassificationStatus::Accepted);
        report.inventory.expect("accepted inventory")
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
                assert!(!inventory.scripts[0].requires_port);
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
            field(b"CTDA", &RENOLDS_GET_STAGE_CTDA),
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
    fn vtechatticup_pack_goldens_lower_only_verified_travel_and_patrol_shapes() {
        let interner = StringInterner::new();
        let merged = "FalloutNV.esm";

        let mut hostage_travel = record(&interner, 0x1231B7, merged, "TecMineHostagePackage");
        hostage_travel.fields.extend([
            exact_pkdt(LegacyPackType::Travel, 0x0400_1206, 0x20, 55, 0, 877),
            location(b"PLDT", 6, 0, 0),
            all_day_schedule(),
        ]);
        push_empty_legacy_events(&mut hostage_travel);

        let mut renolds_patrol = record(
            &interner,
            0x133F3E,
            merged,
            "TechaticupNCRRenoldsPatrolPackage",
        );
        renolds_patrol.fields.extend([
            exact_pkdt(LegacyPackType::Patrol, 0, 0, 0, 0, 3),
            location(b"PLDT", 0, 0x133F3D, 0),
            all_day_schedule(),
            field(b"PKPT", &[0, 0]),
        ]);
        push_empty_legacy_events(&mut renolds_patrol);

        let mut hostage_escape = record(&interner, 0x1231B6, merged, "TecMineHostageEscape");
        hostage_escape.fields.extend([
            exact_pkdt(LegacyPackType::Patrol, 0x0408_3206, 0x20, 0, 103, 21_572),
            location(b"PLDT", 0, 0x0E70CA, 0),
            all_day_schedule(),
            field(b"PKPT", &[0, 0]),
        ]);
        hostage_escape.fields.extend(empty_script(b"POBA"));
        let mut header = [0_u8; LEGACY_SCHR_LEN];
        header[4..8].copy_from_slice(&1_u32.to_le_bytes());
        header[8..12].copy_from_slice(&10_u32.to_le_bytes());
        header[12..16].copy_from_slice(&1_u32.to_le_bytes());
        hostage_escape.fields.extend([
            field(b"POEA", &[]),
            field(b"SCHR", &header),
            field(b"SCDA", &[0x1C, 0, 1, 0, 0x22, 0x10, 2, 0, 0, 0]),
            field(b"SCTX", b"ref hostage\r\nhostage.disable"),
            field(b"SLSD", &[0; 24]),
            FieldEntry {
                sig: SubrecordSig(*b"SCVR"),
                value: FieldValue::String(interner.intern("hostage")),
            },
            FieldEntry {
                sig: SubrecordSig(*b"SCRV"),
                value: FieldValue::Uint(1),
            },
        ]);
        hostage_escape.fields.extend(empty_script(b"POCA"));

        let mut renolds_dialogue = record(
            &interner,
            0x13289E,
            merged,
            "TechaticupNCRRenoldsDialoguePackage",
        );
        renolds_dialogue.fields.extend([
            exact_pkdt(LegacyPackType::Dialogue, 0x2000, 0, 0, 0, 0),
            all_day_schedule(),
            target(b"PTDT", 0, 0x0000_14, 128),
            field(b"CTDA", &[0; LEGACY_CTDA_LEN]),
            field(b"PKDD", &[0; 24]),
        ]);
        push_empty_legacy_events(&mut renolds_dialogue);

        let inventories = [
            inventory(&hostage_travel, &interner),
            inventory(&renolds_patrol, &interner),
            inventory(&hostage_escape, &interner),
            inventory(&renolds_dialogue, &interner),
        ];
        assert!(inventories[0].support.lowering_supported);
        assert!(inventories[1].support.lowering_supported);
        assert!(inventories[2].support.lowering_supported);
        assert!(!inventories[3].support.lowering_supported);
        assert_eq!(inventories[0].pkdt.unused_1, 0);
        assert_eq!(inventories[0].pkdt.unused_2, 877);
        assert_eq!(inventories[1].pkdt.unused_1, 0);
        assert_eq!(inventories[1].pkdt.unused_2, 3);
        assert_eq!(inventories[2].pkdt.unused_1, 103);
        assert_eq!(inventories[2].pkdt.unused_2, 21_572);
        assert!(matches!(
            &inventories[1].unions[0].payload,
            LegacyPackUnionPayload::Reference {
                present: true,
                source_form_key: Some(source),
            } if source == "133F3D@FalloutNV.esm"
        ));
        assert!(matches!(
            &inventories[2].unions[0].payload,
            LegacyPackUnionPayload::Reference {
                present: true,
                source_form_key: Some(source),
            } if source == "0E70CA@FalloutNV.esm"
        ));
        assert!(
            inventories
                .iter()
                .all(|inventory| inventory.schedule.date == 0)
        );
        let mut wrong_plugin_record = hostage_travel.clone();
        wrong_plugin_record.form_key.plugin = interner.intern("FalloutNV.esm");
        assert!(
            !inventory(&wrong_plugin_record, &interner)
                .support
                .lowering_supported
        );
        assert!(
            inventories[2]
                .scripts
                .iter()
                .any(|script| script.requires_port)
        );
        assert_eq!(
            inventories[1].patrol_data.as_ref().unwrap().repeatable,
            false
        );

        let remapped_patrol_start = FormKey {
            local: 0x00AB_CDEF,
            plugin: interner.intern("FalloutNV.esm"),
        };
        let remapped_escape_start = FormKey {
            local: 0x00AB_CDF0,
            plugin: interner.intern("FalloutNV.esm"),
        };
        let mapper = |source: &str| match source {
            "133F3D@FalloutNV.esm" => Some(remapped_patrol_start),
            "0E70CA@FalloutNV.esm" => Some(remapped_escape_start),
            _ => None,
        };
        let lowerer = VerifiedFo4LegacyPackLowerer::new(&interner, mapper);
        let results = inventories
            .iter()
            .map(|inventory| lowerer.lower_supported_legacy_pack(inventory))
            .collect::<Vec<_>>();

        let summary = summarize_legacy_pack_lowering_results(&results);
        assert_eq!(summary.attempted_records, 4);
        assert_eq!(summary.lowered_records, 3);
        assert_eq!(summary.blocked_records, 1);
        assert_eq!(
            summary.blockers,
            BTreeMap::from([("legacy_conditions_require_semantic_lowering".to_string(), 1)])
        );

        let travel = results[0].as_ref().expect("verified travel lowers");
        let travel_pkdt = travel
            .fields
            .iter()
            .find(|field| field.sig.0 == *b"PKDT")
            .expect("PKDT");
        let FieldValue::Bytes(travel_pkdt) = &travel_pkdt.value else {
            panic!("target PKDT is raw target-layout bytes");
        };
        assert_eq!(read_u32(travel_pkdt, 0), 0x204);
        assert_eq!(travel_pkdt[4], 18);
        assert_eq!(travel_pkdt[5], 0);
        assert_eq!(travel_pkdt[6], 2);
        assert_eq!(read_u16(travel_pkdt, 8), 0x20);
        assert_eq!(read_u16(travel_pkdt, 10), 0);
        let travel_schedule = travel
            .fields
            .iter()
            .find(|field| field.sig.0 == *b"PSDT")
            .expect("PSDT");
        let FieldValue::Bytes(travel_schedule) = &travel_schedule.value else {
            panic!("target PSDT is raw target-layout bytes");
        };
        assert_eq!(travel_schedule[2], 0);
        assert_eq!(
            travel
                .fields
                .iter()
                .filter(|field| field.sig.0 == *b"PDTO")
                .count(),
            3
        );
        assert_eq!(
            string_field_values(travel, b"ANAM", &interner),
            ["Location", "Bool", "Bool", "Bool"]
        );
        assert_eq!(one_byte_field_values(travel, b"UNAM"), [1, 3, 5, 7]);
        assert_eq!(one_byte_field_values(travel, b"XNAM"), [0x08]);
        assert_eq!(
            signature_order(travel),
            [
                "EDID", "PKDT", "PSDT", "PKCU", "ANAM", "PLDT", "ANAM", "CNAM", "ANAM", "CNAM",
                "ANAM", "CNAM", "UNAM", "UNAM", "UNAM", "UNAM", "XNAM", "POBA", "INAM", "PDTO",
                "POEA", "INAM", "PDTO", "POCA", "INAM", "PDTO",
            ]
        );

        let schema = AuthoringSchema::for_game("fo4").expect("FO4 schema loads");
        let travel_counter = travel
            .fields
            .iter()
            .find(|field| field.sig.0 == *b"PKCU")
            .expect("travel PKCU");
        let encoded_travel_counter = crate::target_write::encode_field_pub(
            travel_counter,
            schema.record_def("PACK"),
            &interner,
        )
        .expect("travel PKCU encodes");
        assert_eq!(encoded_travel_counter.len(), 12);
        assert_eq!(read_u32(&encoded_travel_counter, 0), 4);
        assert_eq!(
            read_u32(&encoded_travel_counter, 4),
            FO4_TRAVEL_TEMPLATE_LOCAL
        );
        assert_eq!(read_u32(&encoded_travel_counter, 8), 1);

        let patrol = results[1].as_ref().expect("verified patrol lowers");
        let patrol_target = patrol
            .fields
            .iter()
            .find(|field| field.sig.0 == *b"PTDA")
            .expect("patrol PTDA");
        let encoded_patrol_target = crate::target_write::encode_field_pub(
            patrol_target,
            schema.record_def("PACK"),
            &interner,
        )
        .expect("patrol PTDA encodes");
        assert_eq!(encoded_patrol_target.len(), 12);
        assert_eq!(read_u32(&encoded_patrol_target, 0), 0);
        assert_eq!(
            read_u32(&encoded_patrol_target, 4),
            remapped_patrol_start.local
        );
        assert_eq!(
            patrol
                .fields
                .iter()
                .filter_map(|field| match &field.value {
                    FieldValue::Bool(value) => Some(*value),
                    _ => None,
                })
                .collect::<Vec<_>>(),
            vec![false, false, false, false]
        );
        assert_eq!(
            string_field_values(patrol, b"ANAM", &interner),
            [
                "SingleRef",
                "Float",
                "Bool",
                "Bool",
                "Bool",
                "Bool",
                "Float",
            ]
        );
        assert_eq!(
            one_byte_field_values(patrol, b"UNAM"),
            [0, 1, 2, 4, 6, 8, 10]
        );
        assert_eq!(one_byte_field_values(patrol, b"XNAM"), [0x0B]);
        assert_eq!(
            signature_order(patrol),
            [
                "EDID", "PKDT", "PSDT", "PKCU", "ANAM", "PTDA", "ANAM", "CNAM", "ANAM", "CNAM",
                "ANAM", "CNAM", "ANAM", "CNAM", "ANAM", "CNAM", "ANAM", "CNAM", "UNAM", "UNAM",
                "UNAM", "UNAM", "UNAM", "UNAM", "UNAM", "XNAM", "POBA", "INAM", "PDTO", "POEA",
                "INAM", "PDTO", "POCA", "INAM", "PDTO",
            ]
        );

        let patrol_counter = patrol
            .fields
            .iter()
            .find(|field| field.sig.0 == *b"PKCU")
            .expect("patrol PKCU");
        let encoded_patrol_counter = crate::target_write::encode_field_pub(
            patrol_counter,
            schema.record_def("PACK"),
            &interner,
        )
        .expect("patrol PKCU encodes");
        assert_eq!(read_u32(&encoded_patrol_counter, 0), 7);
        assert_eq!(
            read_u32(&encoded_patrol_counter, 4),
            FO4_PATROL_TEMPLATE_LOCAL
        );
        assert_eq!(read_u32(&encoded_patrol_counter, 8), 2);

        let escape = results[2]
            .as_ref()
            .expect("audited hostage escape lowers with alias OnPackageEnd contract");
        assert_eq!(
            escape
                .fields
                .iter()
                .filter(|field| field.sig.0 == *b"POEA")
                .count(),
            1
        );
        assert_pack_editor_ids_survive_save_reopen(
            &[
                (travel, "TecMineHostagePackage", FO4_TRAVEL_TEMPLATE_LOCAL),
                (
                    patrol,
                    "TechaticupNCRRenoldsPatrolPackage",
                    FO4_PATROL_TEMPLATE_LOCAL,
                ),
                (escape, "TecMineHostageEscape", FO4_PATROL_TEMPLATE_LOCAL),
            ],
            &interner,
        );
        assert!(matches!(
            &results[3],
            Err(LegacyPackLoweringError::ConditionsRequirePort { count: 1 })
        ));

        let no_mapper = VerifiedFo4LegacyPackLowerer::new(&interner, |_source: &str| None);
        assert!(matches!(
            no_mapper.lower_supported_legacy_pack(&inventories[1]),
            Err(LegacyPackLoweringError::ReferenceRemapFailed { .. })
        ));
        let mut unresolved_patrol = inventories[1].clone();
        unresolved_patrol.unions[0].payload = LegacyPackUnionPayload::Reference {
            present: true,
            source_form_key: None,
        };
        assert!(matches!(
            lowerer.lower_supported_legacy_pack(&unresolved_patrol),
            Err(LegacyPackLoweringError::UnresolvedEncodedReference { .. })
        ));

        let mut wrong_plugin = inventories[0].clone();
        wrong_plugin.form_key = "1231B7@FalloutNV.esm".to_string();
        assert!(matches!(
            lowerer.lower_supported_legacy_pack(&wrong_plugin),
            Err(LegacyPackLoweringError::UnsupportedPackageShape { .. })
        ));

        let mut ambiguous_reference_record = renolds_patrol.clone();
        ambiguous_reference_record
            .fields
            .iter_mut()
            .find(|field| field.sig.0 == *b"PLDT")
            .expect("raw PLDT")
            .value = location(b"PLDT", 0, 0x0113_3F3D, 0).value;
        let ambiguous_reference = inventory(&ambiguous_reference_record, &interner);
        assert!(matches!(
            &ambiguous_reference.unions[0].payload,
            LegacyPackUnionPayload::Reference {
                present: true,
                source_form_key: None,
            }
        ));
        assert!(matches!(
            lowerer.lower_supported_legacy_pack(&ambiguous_reference),
            Err(LegacyPackLoweringError::UnresolvedEncodedReference { .. })
        ));
    }

    #[test]
    fn renolds_dialogue_clones_verified_force_greet_and_normalizes_getstage() {
        let interner = StringInterner::new();
        let merged = "FalloutNV.esm";
        let mut source = record(
            &interner,
            RENOLDS_DIALOGUE_PACK_LOCAL,
            merged,
            "TechaticupNCRRenoldsDialoguePackage",
        );
        source.fields.extend([
            exact_pkdt(LegacyPackType::Dialogue, 0x2000, 0, 0, 0, 0),
            all_day_schedule(),
            target(b"PTDT", 0, 0x0000_14, 128),
            field(b"CTDA", &RENOLDS_GET_STAGE_CTDA),
            field(b"PKDD", &[0; 24]),
        ]);
        push_empty_legacy_events(&mut source);
        let mut ambiguous_source = source.clone();
        ambiguous_source
            .fields
            .iter_mut()
            .find(|field| field.sig.0 == *b"PTDT")
            .expect("raw PTDT")
            .value = target(b"PTDT", 0, 0x0100_0014, 128).value;
        let ambiguous_source = inventory(&ambiguous_source, &interner);
        assert!(matches!(
            &ambiguous_source.unions[0].payload,
            LegacyPackUnionPayload::Reference {
                present: true,
                source_form_key: None,
            }
        ));
        assert!(validate_audited_renolds_dialogue(&ambiguous_source).is_err());
        let source = inventory(&source, &interner);
        assert!(matches!(
            &source.unions[0].payload,
            LegacyPackUnionPayload::Reference {
                present: true,
                source_form_key: Some(player),
            } if player == "000014@FalloutNV.esm"
        ));

        let mut donor = record(
            &interner,
            FO4_FORCE_GREET_DONOR_LOCAL,
            "Fallout4.esm",
            "RETravelCC01_Forcegreet",
        );
        push_string(&mut donor, b"EDID", "RETravelCC01_Forcegreet", &interner);
        let mut donor_counter = Vec::with_capacity(12);
        donor_counter.extend_from_slice(&24_u32.to_le_bytes());
        donor_counter.extend_from_slice(&FO4_FORCE_GREET_TEMPLATE_LOCAL.to_le_bytes());
        donor_counter.extend_from_slice(&11_u32.to_le_bytes());
        push_bytes(&mut donor, b"PKCU", donor_counter);
        push_force_greet_inputs(&mut donor, &interner);
        for field in donor
            .fields
            .iter_mut()
            .filter(|field| field.sig.0 == *b"UNAM")
        {
            let FieldValue::Bytes(bytes) = &field.value else {
                panic!("fixture UNAM starts raw");
            };
            field.value = FieldValue::Int(i64::from(bytes[0]));
        }
        push_empty_fo4_event_blocks(&mut donor);
        let mut ambiguous_donor = donor.clone();
        ambiguous_donor
            .fields
            .iter_mut()
            .find(|field| field.sig.0 == *b"PKCU")
            .expect("raw PKCU")
            .value = {
            let mut bytes = Vec::with_capacity(12);
            bytes.extend_from_slice(&24_u32.to_le_bytes());
            bytes.extend_from_slice(&0x0101_7BAB_u32.to_le_bytes());
            bytes.extend_from_slice(&11_u32.to_le_bytes());
            FieldValue::Bytes(SmallVec::from_vec(bytes))
        };
        assert!(matches!(
            validate_force_greet_donor(&ambiguous_donor, &interner),
            Err(LegacyPackLoweringError::InvalidForceGreetDonor { .. })
        ));
        let mut wrong_union_donor = donor.clone();
        wrong_union_donor
            .fields
            .iter_mut()
            .find(|field| field.sig.0 == *b"PLDT")
            .expect("donor PLDT")
            .sig = SubrecordSig(*b"PLD2");
        assert!(matches!(
            validate_force_greet_donor(&wrong_union_donor, &interner),
            Err(LegacyPackLoweringError::InvalidForceGreetDonor { .. })
        ));

        let target_owner = FormKey {
            local: 0x002000,
            plugin: interner.intern("Converted.esm"),
        };
        let reference_mapper =
            |source: &str| (source == "11F935@FalloutNV.esm").then_some(target_owner);
        let lowerer = VerifiedFo4LegacyPackLowerer::new(&interner, reference_mapper);
        let mut condition_mapper = FormKeyMapper::new(
            std::iter::empty(),
            MapperOptions {
                output_plugin_name: "Converted.esm".into(),
                source_plugin_name: merged.into(),
                target_master_names: vec!["Fallout4.esm".into()],
                ..MapperOptions::default()
            },
            &interner,
        );
        condition_mapper.add_mapping(
            FormKey {
                local: VTECHATTICUP_QUEST_LOCAL,
                plugin: interner.intern(merged),
            },
            target_owner,
        );

        let output = lowerer
            .lower_audited_renolds_dialogue_with_donor(&source, &mut condition_mapper, &donor)
            .expect("audited ForceGreet lowers");
        assert_eq!(output.form_key.local, RENOLDS_DIALOGUE_PACK_LOCAL);
        assert_eq!(output.fields[0].sig.0, *b"EDID");
        assert!(matches!(
            &output.fields[0].value,
            FieldValue::String(value)
                if interner.resolve(*value) == Some("TechaticupNCRRenoldsDialoguePackage")
        ));
        assert_eq!(
            output
                .fields
                .iter()
                .filter(|field| field.sig.0 == *b"EDID")
                .count(),
            1
        );
        assert_pack_editor_ids_survive_save_reopen(
            &[(
                &output,
                "TechaticupNCRRenoldsDialoguePackage",
                FO4_FORCE_GREET_TEMPLATE_LOCAL,
            )],
            &interner,
        );
        assert_eq!(
            output
                .fields
                .iter()
                .filter(|field| field.sig.0 == *b"ANAM")
                .count(),
            24
        );
        assert!(output.fields.iter().any(|field| {
            field.sig.0 == *b"QNAM" && field.value == FieldValue::FormKey(target_owner)
        }));
        let normalized = output
            .fields
            .iter()
            .find(|field| field.sig.0 == *b"CTDA")
            .expect("normalized CTDA");
        let FieldValue::Bytes(bytes) = &normalized.value else {
            panic!("normalized CTDA must remain raw");
        };
        assert_eq!(bytes.len(), super::super::fnv_conditions::FO4_CTDA_LEN);
        assert_eq!(read_u16(bytes, 8), 58);
        assert_ne!(read_u32(bytes, 12), VTECHATTICUP_QUEST_LOCAL);

        let generic = lowerer.lower_supported_legacy_pack(&source);
        assert!(matches!(
            generic,
            Err(LegacyPackLoweringError::ConditionsRequirePort { count: 1 })
        ));
        donor.form_key.local += 1;
        assert!(matches!(
            lowerer.lower_audited_renolds_dialogue_with_donor(
                &source,
                &mut condition_mapper,
                &donor,
            ),
            Err(LegacyPackLoweringError::InvalidForceGreetDonor { .. })
        ));
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
