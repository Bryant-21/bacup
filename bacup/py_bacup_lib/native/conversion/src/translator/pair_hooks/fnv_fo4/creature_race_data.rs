//! FNV/FO3 source-owned creature RACE DATA evidence adapter.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use nif_core_native::model::{NifFile, NifValue};
use nif_core_native::world_bounds::{
    WorldBounds, WorldBoundsError, aggregate_named_node_world_bounds, aggregate_render_world_bounds,
};
use serde::{Deserialize, Serialize};

use crate::record::{FieldValue, Record};
use crate::source_rig::CreatureGraphTemplate;
use crate::source_rig::race_data::{
    AngularMotionEvidence, CapsuleEvidence, ControllerArchitecture, DefaultWeightEvidence,
    Fo4RaceDataDerivation, Fo4RaceDataScalePolicy, LinearMotionEvidence, MeasuredBounds,
    MeasurementAxis, MovementArchitecture, MovementSemantics, RaceDataDerivationError,
    RaceDataEvidenceField, SourceFamilyRaceDataEvidence, derive_fo4_race_data,
};
use crate::sym::StringInterner;

use super::creature_catalog::{
    AssetReadiness, BodyReadiness, BodyVariantKey, CreatureCorpusPlan, CreatureDisposition,
    CreatureFamilyKind, CreatureProvenance, EXPECTED_FULL_MERGED_CREA_WINNERS, LegacyCreatureGame,
    LegacyRecordSource, ProxyBlocker, ProxyReadiness, RigFamilyKey, SpecialCreatureReason,
    StableFormKey,
};
use super::creature_motion::{
    BindingCompatibility, CreatureMotionFamilyEvidence, KfParseEvidence, RootMotionEvidence,
};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FamilyRaceDataSelection {
    pub rig: RigFamilyKey,
    pub source_creature: StableFormKey,
    pub source_body: BodyVariantKey,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RigRaceDataEvidence {
    pub rig: RigFamilyKey,
    pub skeleton_bounds: Option<MeasuredBounds>,
    pub up_axis: MeasurementAxis,
    pub geometry_scale_to_fo4: Option<f32>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BodyRaceDataEvidence {
    pub body: BodyVariantKey,
    pub body_bounds: Option<MeasuredBounds>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ControllerRaceDataEvidence {
    pub rig: RigFamilyKey,
    pub capsule: Option<CapsuleEvidence>,
    pub movement: Option<MovementSemantics>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CreatureRaceDataEvidence {
    pub creature: StableFormKey,
    pub male_actor_scale: Option<f32>,
    pub female_actor_scale: Option<f32>,
    pub default_weights: Option<DefaultWeightEvidence>,
    pub injured_health_percent: Option<f32>,
    pub body_biped_object: Option<i32>,
    pub xp_value: Option<i16>,
}

#[derive(Clone, Copy, Debug)]
pub struct LegacyRaceDataEvidenceInputs<'a> {
    pub selections: &'a [FamilyRaceDataSelection],
    pub rigs: &'a [RigRaceDataEvidence],
    pub bodies: &'a [BodyRaceDataEvidence],
    pub controllers: &'a [ControllerRaceDataEvidence],
    pub creatures: &'a [CreatureRaceDataEvidence],
}

impl Default for LegacyRaceDataEvidenceInputs<'_> {
    fn default() -> Self {
        Self {
            selections: &[],
            rigs: &[],
            bodies: &[],
            controllers: &[],
            creatures: &[],
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum FamilyRaceDataIssue {
    MissingFamilySelection,
    DuplicateFamilySelection,
    UnknownSourceCreature(StableFormKey),
    UnknownSourceBody(BodyVariantKey),
    SourceCreatureNotVisualOwner(StableFormKey),
    SourceCreatureRigMismatch {
        expected: RigFamilyKey,
        actual: RigFamilyKey,
    },
    SourceCreatureBodyMismatch {
        expected: BodyVariantKey,
        actual: BodyVariantKey,
    },
    SourceGameMismatch {
        rig_game: LegacyCreatureGame,
        creature_game: LegacyCreatureGame,
    },
    CatalogRigUnavailable(String),
    CatalogBodyUnavailable(String),
    MissingRigEvidence,
    DuplicateRigEvidence,
    MissingBodyEvidence,
    DuplicateBodyEvidence,
    MissingControllerEvidence,
    DuplicateControllerEvidence,
    MissingCreatureEvidence,
    DuplicateCreatureEvidence,
    InvalidEvidence {
        field: RaceDataEvidenceField,
        reason: String,
    },
    InvalidDerivedTarget(String),
}

#[derive(Clone, Debug, PartialEq)]
pub enum FamilyRaceDataStatus {
    Ready {
        evidence: SourceFamilyRaceDataEvidence,
        derivation: Fo4RaceDataDerivation,
    },
    Missing {
        fields: Vec<RaceDataEvidenceField>,
        issues: Vec<FamilyRaceDataIssue>,
    },
    Invalid {
        fields: Vec<RaceDataEvidenceField>,
        issues: Vec<FamilyRaceDataIssue>,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct FamilyRaceDataEntry {
    pub rig: RigFamilyKey,
    pub provenance: LegacyCreatureGame,
    pub family_kind: CreatureFamilyKind,
    pub selection: Option<FamilyRaceDataSelection>,
    pub status: FamilyRaceDataStatus,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ProxyRaceDataBlocker {
    Catalog(ProxyBlocker),
    MissingTerminalVisualOwner(StableFormKey),
    TerminalIsNotVisualOwner(StableFormKey),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RecordRaceDataDisposition {
    VisualOwner {
        rig: RigFamilyKey,
    },
    Proxy {
        rig: RigFamilyKey,
        terminal_visual_owners: Vec<StableFormKey>,
    },
    AmbiguousProxy {
        rigs: Vec<RigFamilyKey>,
        terminal_visual_owners: Vec<StableFormKey>,
    },
    UnavailableProxy {
        blockers: Vec<ProxyRaceDataBlocker>,
    },
    Special {
        reason: SpecialCreatureReason,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecordRaceDataEntry {
    pub source: StableFormKey,
    pub provenance: CreatureProvenance,
    pub disposition: RecordRaceDataDisposition,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RaceDataAdapterAccounting {
    pub catalog_records: usize,
    pub record_dispositions: usize,
    pub fnv_records: usize,
    pub fo3_records: usize,
    pub visual_owner_records: usize,
    pub proxy_records: usize,
    pub ambiguous_proxy_records: usize,
    pub unavailable_proxy_records: usize,
    pub special_records: usize,
    pub rig_families: usize,
    pub ready_families: usize,
    pub missing_families: usize,
    pub invalid_families: usize,
    pub records_with_ready_family: usize,
    pub records_with_missing_family: usize,
    pub records_with_invalid_family: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LegacyRaceDataAdapterPlan {
    pub families: Vec<FamilyRaceDataEntry>,
    pub records: Vec<RecordRaceDataEntry>,
    pub accounting: RaceDataAdapterAccounting,
}

#[derive(Clone, Debug, thiserror::Error, PartialEq, Eq)]
pub enum LegacyRaceDataAdapterError {
    #[error("invalid FO4 size policy: {0}")]
    InvalidSizePolicy(String),
    #[error("creature catalog accounting drifted: {0}")]
    CatalogAccounting(String),
    #[error("creature catalog contains duplicate record {0}")]
    DuplicateCatalogRecord(StableFormKey),
    #[error("creature catalog contains duplicate rig family {0:?}")]
    DuplicateCatalogRig(RigFamilyKey),
    #[error("creature catalog contains duplicate body variant {0:?}")]
    DuplicateCatalogBody(BodyVariantKey),
    #[error("creature catalog attack-set coverage drifted for {0:?}")]
    AttackSetCoverage(RigFamilyKey),
    #[error("full merged race-data census is {actual}, expected {expected}")]
    WinnerCountMismatch { expected: usize, actual: usize },
    #[error("race-data adapter accounting drifted: {0}")]
    AdapterAccounting(String),
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ProductionRaceDataRequirement {
    RigMeasurements {
        rig: RigFamilyKey,
        skeleton_path: String,
    },
    BodyMeasurements {
        body: BodyVariantKey,
        body_paths: Vec<String>,
    },
    ControllerMeasurements {
        rig: RigFamilyKey,
        skeleton_path: String,
    },
    CreatureSemantics {
        creature: StableFormKey,
        provenance: LegacyCreatureGame,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum RaceDataFidelityReceipt {
    SchemaAbsentDefaulted {
        creature: StableFormKey,
        field: RaceDataEvidenceField,
        policy: String,
    },
    LevelAsXpMvp {
        creature: StableFormKey,
        source_level: u16,
        xp_value: i16,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum CreatureRecordEvidenceIssue {
    MissingSourceRecord(StableFormKey),
    DuplicateSourceRecord(StableFormKey),
    WrongSourceSignature {
        creature: StableFormKey,
        signature: String,
    },
    MissingEvidence {
        creature: StableFormKey,
        field: RaceDataEvidenceField,
    },
    InvalidEvidence {
        creature: StableFormKey,
        field: RaceDataEvidenceField,
        reason: String,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct LoadedCreatureRecordRaceDataEvidence {
    pub creatures: Vec<CreatureRaceDataEvidence>,
    pub issues: Vec<CreatureRecordEvidenceIssue>,
    pub fidelity_receipts: Vec<RaceDataFidelityReceipt>,
    pub remaining_requirements: Vec<ProductionRaceDataRequirement>,
}

pub fn build_family_race_data_selections(
    catalog: &CreatureCorpusPlan,
) -> Vec<FamilyRaceDataSelection> {
    let ready_bodies = catalog
        .body_variants
        .iter()
        .filter(|body| matches!(body.readiness, BodyReadiness::Ready))
        .map(|body| &body.key)
        .collect::<BTreeSet<_>>();
    let mut selections = Vec::new();
    for family in &catalog.rig_families {
        let mut owners = catalog
            .records
            .iter()
            .filter_map(|record| match &record.disposition {
                CreatureDisposition::VisualOwner { rig, body } if rig == &family.key => {
                    Some((record.source.clone(), body.clone()))
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        owners.sort_by(|left, right| {
            ready_bodies
                .contains(&right.1)
                .cmp(&ready_bodies.contains(&left.1))
                .then_with(|| left.cmp(right))
        });
        if let Some((source_creature, source_body)) = owners.into_iter().next() {
            selections.push(FamilyRaceDataSelection {
                rig: family.key.clone(),
                source_creature,
                source_body,
            });
        }
    }
    selections.sort_by(|left, right| left.rig.cmp(&right.rig));
    selections
}

pub fn load_creature_record_race_data_evidence(
    catalog: &CreatureCorpusPlan,
    selections: &[FamilyRaceDataSelection],
    records: &[Record],
    interner: &StringInterner,
) -> LoadedCreatureRecordRaceDataEvidence {
    let mut records_by_key = BTreeMap::<StableFormKey, Vec<&Record>>::new();
    for record in records {
        let Some(plugin) = interner.resolve(record.form_key.plugin) else {
            continue;
        };
        records_by_key
            .entry(StableFormKey {
                local: record.form_key.local,
                plugin: plugin.to_ascii_lowercase(),
            })
            .or_default()
            .push(record);
    }
    let mut requirements = production_evidence_requirements(catalog)
        .into_iter()
        .collect::<BTreeSet<_>>();
    let provenance = catalog
        .records
        .iter()
        .map(|record| (record.source.clone(), record.provenance.game))
        .collect::<BTreeMap<_, _>>();
    let mut creatures = Vec::new();
    let mut issues = Vec::new();
    let mut fidelity_receipts = Vec::new();

    for selection in selections {
        let candidates = records_by_key
            .get(&selection.source_creature)
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        if candidates.is_empty() {
            issues.push(CreatureRecordEvidenceIssue::MissingSourceRecord(
                selection.source_creature.clone(),
            ));
            continue;
        }
        if candidates.len() != 1 {
            issues.push(CreatureRecordEvidenceIssue::DuplicateSourceRecord(
                selection.source_creature.clone(),
            ));
            continue;
        }
        let record = candidates[0];
        if record.sig.as_str() != "CREA" {
            issues.push(CreatureRecordEvidenceIssue::WrongSourceSignature {
                creature: selection.source_creature.clone(),
                signature: record.sig.as_str().to_string(),
            });
            continue;
        }
        let base_scale = scalar_f32(record, "BNAM", "float32_0", interner);
        if base_scale.is_none() {
            for field in [
                RaceDataEvidenceField::MaleActorScale,
                RaceDataEvidenceField::FemaleActorScale,
            ] {
                issues.push(CreatureRecordEvidenceIssue::MissingEvidence {
                    creature: selection.source_creature.clone(),
                    field,
                });
            }
        } else if base_scale.is_some_and(|value| !value.is_finite() || value <= 0.0) {
            for field in [
                RaceDataEvidenceField::MaleActorScale,
                RaceDataEvidenceField::FemaleActorScale,
            ] {
                issues.push(CreatureRecordEvidenceIssue::InvalidEvidence {
                    creature: selection.source_creature.clone(),
                    field,
                    reason: "BNAM base scale must be finite and positive".to_string(),
                });
            }
        }
        let source_level = struct_u16(record, "ACBS", "level", interner);
        if source_level.is_none() {
            issues.push(CreatureRecordEvidenceIssue::MissingEvidence {
                creature: selection.source_creature.clone(),
                field: RaceDataEvidenceField::XpValue,
            });
        }
        let xp_value = source_level.map(|level| i16::try_from(level.max(1)).unwrap_or(i16::MAX));
        let default_weights = DefaultWeightEvidence {
            male: [0.0, 0.0, 0.0],
            female: [0.0, 0.0, 0.0],
            ungendered: true,
        };
        fidelity_receipts.extend([
            RaceDataFidelityReceipt::SchemaAbsentDefaulted {
                creature: selection.source_creature.clone(),
                field: RaceDataEvidenceField::DefaultWeights,
                policy: "legacy_crea_ungendered_no_morph_channels_v1".to_string(),
            },
            RaceDataFidelityReceipt::SchemaAbsentDefaulted {
                creature: selection.source_creature.clone(),
                field: RaceDataEvidenceField::InjuredHealthPercent,
                policy: "disable_invented_injury_threshold_v1".to_string(),
            },
            RaceDataFidelityReceipt::SchemaAbsentDefaulted {
                creature: selection.source_creature.clone(),
                field: RaceDataEvidenceField::BodyBipedObject,
                policy: "legacy_crea_has_no_biped_object_slot_v1".to_string(),
            },
        ]);
        if let (Some(source_level), Some(xp_value)) = (source_level, xp_value) {
            fidelity_receipts.push(RaceDataFidelityReceipt::LevelAsXpMvp {
                creature: selection.source_creature.clone(),
                source_level,
                xp_value,
            });
        }
        creatures.push(CreatureRaceDataEvidence {
            creature: selection.source_creature.clone(),
            male_actor_scale: base_scale,
            female_actor_scale: base_scale,
            default_weights: Some(default_weights),
            injured_health_percent: Some(0.0),
            body_biped_object: Some(-1),
            xp_value,
        });
        if base_scale.is_some()
            && source_level.is_some()
            && let Some(game) = provenance.get(&selection.source_creature).copied()
        {
            requirements.remove(&ProductionRaceDataRequirement::CreatureSemantics {
                creature: selection.source_creature.clone(),
                provenance: game,
            });
        }
    }
    creatures.sort_by(|left, right| left.creature.cmp(&right.creature));
    issues.sort();
    issues.dedup();
    fidelity_receipts.sort();
    fidelity_receipts.dedup();
    LoadedCreatureRecordRaceDataEvidence {
        creatures,
        issues,
        fidelity_receipts,
        remaining_requirements: requirements.into_iter().collect(),
    }
}

fn scalar_f32(
    record: &Record,
    signature: &str,
    member: &str,
    interner: &StringInterner,
) -> Option<f32> {
    let value = record
        .fields
        .iter()
        .find(|field| field.sig.as_str() == signature)
        .map(|field| &field.value)?;
    match value {
        FieldValue::Float(value) => Some(*value as f32),
        FieldValue::Int(value) => Some(*value as f32),
        FieldValue::Uint(value) => Some(*value as f32),
        FieldValue::Bytes(bytes) if bytes.len() == 4 => {
            Some(f32::from_le_bytes(bytes.as_slice().try_into().ok()?))
        }
        FieldValue::Struct(fields) => fields
            .iter()
            .find(|(name, _)| interner.resolve(*name) == Some(member))
            .and_then(|(_, value)| match value {
                FieldValue::Float(value) => Some(*value as f32),
                FieldValue::Int(value) => Some(*value as f32),
                FieldValue::Uint(value) => Some(*value as f32),
                _ => None,
            }),
        _ => None,
    }
}

fn struct_u16(
    record: &Record,
    signature: &str,
    member: &str,
    interner: &StringInterner,
) -> Option<u16> {
    let value = record
        .fields
        .iter()
        .find(|field| field.sig.as_str() == signature)
        .map(|field| &field.value)?;
    match value {
        FieldValue::Struct(fields) => fields
            .iter()
            .find(|(name, _)| interner.resolve(*name) == Some(member))
            .and_then(|(_, value)| match value {
                FieldValue::Uint(value) => u16::try_from(*value).ok(),
                FieldValue::Int(value) => u16::try_from(*value).ok(),
                _ => None,
            }),
        FieldValue::Bytes(bytes) if signature == "ACBS" && bytes.len() == 24 => {
            let offset = match member {
                "level" => 8,
                "speed_multiplier" => 14,
                _ => return None,
            };
            Some(u16::from_le_bytes(
                bytes[offset..offset + 2].try_into().ok()?,
            ))
        }
        _ => None,
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct RigNifMeasurementContext {
    pub rig: RigFamilyKey,
    pub up_axis: MeasurementAxis,
    pub geometry_scale_to_fo4: Option<f32>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RigControllerRaceDataContext {
    pub rig: RigFamilyKey,
    pub architecture: ControllerArchitecture,
    pub movement: Option<MovementSemantics>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LegacyMovementBaseSettings {
    pub game: LegacyCreatureGame,
    pub f_move_base_speed_source: StableFormKey,
    pub locomotion_multiplier_source: StableFormKey,
    pub f_move_base_speed: f32,
    pub locomotion_multiplier: f32,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum LegacyMovementBaseSettingsIssue {
    MissingSetting {
        game: LegacyCreatureGame,
        editor_id: String,
    },
    WinningCollision {
        game: LegacyCreatureGame,
        editor_id: String,
        precedence: u32,
    },
    InvalidSettingValue {
        game: LegacyCreatureGame,
        editor_id: String,
        source: StableFormKey,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct LoadedLegacyMovementBaseSettings {
    pub settings: Vec<LegacyMovementBaseSettings>,
    pub issues: Vec<LegacyMovementBaseSettingsIssue>,
}

pub fn load_legacy_movement_base_settings(
    catalog: &CreatureCorpusPlan,
    sources: &[LegacyRecordSource<'_>],
    interner: &StringInterner,
) -> LoadedLegacyMovementBaseSettings {
    const BASE_SPEED: &str = "fmovebasespeed";
    const LOCOMOTION_MULTIPLIER: &str = "fmoverunmult";

    let required_games = catalog
        .rig_families
        .iter()
        .map(|family| family.key.game)
        .collect::<BTreeSet<_>>();
    let mut candidates =
        BTreeMap::<(LegacyCreatureGame, String), Vec<&LegacyRecordSource<'_>>>::new();
    for source in sources {
        if source.record.sig.as_str() != "GMST" {
            continue;
        }
        let Some(editor_id) = source.record.eid.and_then(|eid| interner.resolve(eid)) else {
            continue;
        };
        let normalized = editor_id.to_ascii_lowercase();
        if matches!(normalized.as_str(), BASE_SPEED | LOCOMOTION_MULTIPLIER) {
            candidates
                .entry((source.provenance.game, normalized))
                .or_default()
                .push(source);
        }
    }
    let mut values = BTreeMap::<(LegacyCreatureGame, String), (StableFormKey, f32)>::new();
    let mut issues = Vec::new();
    for game in required_games.iter().copied() {
        for editor_id in [BASE_SPEED, LOCOMOTION_MULTIPLIER] {
            let key = (game, editor_id.to_string());
            let Some(entries) = candidates.get(&key) else {
                issues.push(LegacyMovementBaseSettingsIssue::MissingSetting {
                    game,
                    editor_id: editor_id.to_string(),
                });
                continue;
            };
            let precedence = entries
                .iter()
                .map(|entry| entry.provenance.precedence)
                .max()
                .expect("candidate list is nonempty");
            let winners = entries
                .iter()
                .filter(|entry| entry.provenance.precedence == precedence)
                .copied()
                .collect::<Vec<_>>();
            if winners.len() != 1 {
                issues.push(LegacyMovementBaseSettingsIssue::WinningCollision {
                    game,
                    editor_id: editor_id.to_string(),
                    precedence,
                });
                continue;
            }
            let winner = winners[0];
            let source_key = stable_record_key(winner.record, interner);
            let Some(source_key) = source_key else {
                issues.push(LegacyMovementBaseSettingsIssue::InvalidSettingValue {
                    game,
                    editor_id: editor_id.to_string(),
                    source: StableFormKey {
                        plugin: winner.provenance.source_plugin.to_ascii_lowercase(),
                        local: winner.record.form_key.local,
                    },
                });
                continue;
            };
            let value = scalar_f32(winner.record, "DATA", "float_float", interner);
            if value.is_none_or(|value| !value.is_finite() || value <= 0.0) {
                issues.push(LegacyMovementBaseSettingsIssue::InvalidSettingValue {
                    game,
                    editor_id: editor_id.to_string(),
                    source: source_key,
                });
                continue;
            }
            values.insert(key, (source_key, value.expect("checked above")));
        }
    }
    let mut settings = Vec::new();
    for game in required_games {
        let Some((base_source, base_speed)) = values.get(&(game, BASE_SPEED.to_string())) else {
            continue;
        };
        let Some((multiplier_source, multiplier)) =
            values.get(&(game, LOCOMOTION_MULTIPLIER.to_string()))
        else {
            continue;
        };
        settings.push(LegacyMovementBaseSettings {
            game,
            f_move_base_speed_source: base_source.clone(),
            locomotion_multiplier_source: multiplier_source.clone(),
            f_move_base_speed: *base_speed,
            locomotion_multiplier: *multiplier,
        });
    }
    settings.sort_by_key(|settings| settings.game);
    issues.sort();
    issues.dedup();
    LoadedLegacyMovementBaseSettings { settings, issues }
}

fn stable_record_key(record: &Record, interner: &StringInterner) -> Option<StableFormKey> {
    Some(StableFormKey {
        local: record.form_key.local,
        plugin: interner
            .resolve(record.form_key.plugin)?
            .to_ascii_lowercase(),
    })
}

#[derive(Clone, Debug, PartialEq)]
pub struct LegacyMovementGraphSelection {
    pub rig: RigFamilyKey,
    pub graph_template: CreatureGraphTemplate,
    pub locomotion_source_kf: Option<String>,
    pub turn_source_kf: Option<String>,
    pub stationary_cycle_source_kf: Option<String>,
    pub uses_controller_speed: bool,
    pub aiming_capable: bool,
    pub aim_tolerance_degrees: Option<f32>,
    pub orientation_pitch_degrees: Option<f32>,
    pub orientation_roll_degrees: Option<f32>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum LegacyMovementMvpIssue {
    MissingGraphSelection(RigFamilyKey),
    DuplicateGraphSelection(RigFamilyKey),
    MissingBaseSettings(LegacyCreatureGame),
    DuplicateBaseSettings(LegacyCreatureGame),
    MissingFamilySelection(RigFamilyKey),
    MissingSourceRecord(StableFormKey),
    DuplicateSourceRecord(StableFormKey),
    MissingCreatureSpeedMultiplier(StableFormKey),
    InvalidCreatureSpeedMultiplier(StableFormKey),
    MissingCreatureTurningSpeed(StableFormKey),
    InvalidCreatureTurningSpeed(StableFormKey),
    MissingMotionFamily(RigFamilyKey),
    DuplicateMotionFamily(RigFamilyKey),
    MissingSelectedKf {
        rig: RigFamilyKey,
        role: String,
    },
    UnreferencedSelectedKf {
        rig: RigFamilyKey,
        role: String,
        source_kf: String,
    },
    DuplicateSelectedKf {
        rig: RigFamilyKey,
        source_kf: String,
    },
    UnavailableSelectedKf {
        rig: RigFamilyKey,
        source_kf: String,
        reason: String,
    },
    InvalidSelectedCycle {
        rig: RigFamilyKey,
        source_kf: String,
        reason: String,
    },
    MissingAimTolerance(RigFamilyKey),
    InvalidAimTolerance(RigFamilyKey),
    MissingOrientationLimits(RigFamilyKey),
    InvalidOrientationLimits(RigFamilyKey),
    InvalidBaseSettings(LegacyCreatureGame),
}

#[derive(Clone, Debug, PartialEq)]
pub struct LoadedLegacyMovementMvpEvidence {
    pub controllers: Vec<RigControllerRaceDataContext>,
    pub issues: Vec<LegacyMovementMvpIssue>,
    pub fidelity_receipts: Vec<RaceDataFidelityReceipt>,
}

pub fn build_legacy_movement_mvp_contexts(
    catalog: &CreatureCorpusPlan,
    family_selections: &[FamilyRaceDataSelection],
    graph_selections: &[LegacyMovementGraphSelection],
    base_settings: &[LegacyMovementBaseSettings],
    motion_families: &[CreatureMotionFamilyEvidence],
    records: &[Record],
    interner: &StringInterner,
) -> LoadedLegacyMovementMvpEvidence {
    let mut family_selection_by_rig =
        BTreeMap::<RigFamilyKey, Vec<&FamilyRaceDataSelection>>::new();
    for selection in family_selections {
        family_selection_by_rig
            .entry(selection.rig.clone())
            .or_default()
            .push(selection);
    }
    let mut graph_by_rig = BTreeMap::<RigFamilyKey, Vec<&LegacyMovementGraphSelection>>::new();
    for selection in graph_selections {
        graph_by_rig
            .entry(selection.rig.clone())
            .or_default()
            .push(selection);
    }
    let mut settings_by_game =
        BTreeMap::<LegacyCreatureGame, Vec<&LegacyMovementBaseSettings>>::new();
    for settings in base_settings {
        settings_by_game
            .entry(settings.game)
            .or_default()
            .push(settings);
    }
    let mut motion_by_rig = BTreeMap::<RigFamilyKey, Vec<&CreatureMotionFamilyEvidence>>::new();
    for family in motion_families {
        motion_by_rig
            .entry(family.rig.clone())
            .or_default()
            .push(family);
    }
    let mut records_by_key = BTreeMap::<StableFormKey, Vec<&Record>>::new();
    for record in records {
        let Some(plugin) = interner.resolve(record.form_key.plugin) else {
            continue;
        };
        records_by_key
            .entry(StableFormKey {
                plugin: plugin.to_ascii_lowercase(),
                local: record.form_key.local,
            })
            .or_default()
            .push(record);
    }
    let family_kind_by_rig = catalog
        .attack_sets
        .iter()
        .map(|attack| (attack.rig.clone(), attack.family_kind))
        .collect::<BTreeMap<_, _>>();

    let mut controllers = Vec::new();
    let mut issues = Vec::new();
    let mut fidelity_receipts = Vec::new();
    for family in &catalog.rig_families {
        let graph_candidates = graph_by_rig
            .get(&family.key)
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        let graph = match graph_candidates {
            [] => {
                issues.push(LegacyMovementMvpIssue::MissingGraphSelection(
                    family.key.clone(),
                ));
                continue;
            }
            [graph] => *graph,
            _ => {
                issues.push(LegacyMovementMvpIssue::DuplicateGraphSelection(
                    family.key.clone(),
                ));
                continue;
            }
        };
        let selection_candidates = family_selection_by_rig
            .get(&family.key)
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        let family_selection = match selection_candidates {
            [selection] => *selection,
            _ => {
                issues.push(LegacyMovementMvpIssue::MissingFamilySelection(
                    family.key.clone(),
                ));
                continue;
            }
        };
        let source_records = records_by_key
            .get(&family_selection.source_creature)
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        let source_record = match source_records {
            [] => {
                issues.push(LegacyMovementMvpIssue::MissingSourceRecord(
                    family_selection.source_creature.clone(),
                ));
                continue;
            }
            [record] => *record,
            _ => {
                issues.push(LegacyMovementMvpIssue::DuplicateSourceRecord(
                    family_selection.source_creature.clone(),
                ));
                continue;
            }
        };
        let settings_candidates = settings_by_game
            .get(&family.key.game)
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        let settings = match settings_candidates {
            [] => {
                issues.push(LegacyMovementMvpIssue::MissingBaseSettings(family.key.game));
                continue;
            }
            [settings] => *settings,
            _ => {
                issues.push(LegacyMovementMvpIssue::DuplicateBaseSettings(
                    family.key.game,
                ));
                continue;
            }
        };
        if !settings.f_move_base_speed.is_finite()
            || settings.f_move_base_speed <= 0.0
            || !settings.locomotion_multiplier.is_finite()
            || settings.locomotion_multiplier <= 0.0
            || settings.f_move_base_speed_source.plugin.trim().is_empty()
            || settings
                .locomotion_multiplier_source
                .plugin
                .trim()
                .is_empty()
        {
            issues.push(LegacyMovementMvpIssue::InvalidBaseSettings(family.key.game));
            continue;
        }
        let speed_multiplier = struct_u16(source_record, "ACBS", "speed_multiplier", interner);
        if speed_multiplier.is_none() {
            issues.push(LegacyMovementMvpIssue::MissingCreatureSpeedMultiplier(
                family_selection.source_creature.clone(),
            ));
            continue;
        }
        let speed_multiplier = speed_multiplier.expect("checked above");
        if speed_multiplier == 0 {
            issues.push(LegacyMovementMvpIssue::InvalidCreatureSpeedMultiplier(
                family_selection.source_creature.clone(),
            ));
            continue;
        }
        let turning_speed = scalar_f32(source_record, "TNAM", "float32_0", interner);
        if turning_speed.is_none() {
            issues.push(LegacyMovementMvpIssue::MissingCreatureTurningSpeed(
                family_selection.source_creature.clone(),
            ));
            continue;
        }
        let turning_speed = turning_speed.expect("checked above");
        if !turning_speed.is_finite() || turning_speed < 0.0 {
            issues.push(LegacyMovementMvpIssue::InvalidCreatureTurningSpeed(
                family_selection.source_creature.clone(),
            ));
            continue;
        }
        let motion_candidates = motion_by_rig
            .get(&family.key)
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        let motion = match motion_candidates {
            [] => {
                issues.push(LegacyMovementMvpIssue::MissingMotionFamily(
                    family.key.clone(),
                ));
                continue;
            }
            [motion] => *motion,
            _ => {
                issues.push(LegacyMovementMvpIssue::DuplicateMotionFamily(
                    family.key.clone(),
                ));
                continue;
            }
        };
        let architecture = if graph.graph_template == CreatureGraphTemplate::PassiveGround
            && graph.locomotion_source_kf.is_none()
        {
            MovementArchitecture::Stationary
        } else {
            movement_architecture(graph.graph_template)
        };
        let family_kind = family_kind_by_rig
            .get(&family.key)
            .copied()
            .unwrap_or(CreatureFamilyKind::Organic);
        let controller_architecture = controller_architecture(graph.graph_template, family_kind);
        let cycle_paths = if architecture == MovementArchitecture::Stationary {
            vec![("stationary", graph.stationary_cycle_source_kf.as_deref())]
        } else {
            let mut paths = vec![("locomotion", graph.locomotion_source_kf.as_deref())];
            if graph.turn_source_kf.is_some() {
                paths.push(("turn", graph.turn_source_kf.as_deref()));
            }
            paths
        };
        let mut cycles = BTreeMap::new();
        let mut cycle_failed = false;
        for (role, source_kf) in cycle_paths {
            let Some(source_kf) = source_kf else {
                issues.push(LegacyMovementMvpIssue::MissingSelectedKf {
                    rig: family.key.clone(),
                    role: role.to_string(),
                });
                cycle_failed = true;
                continue;
            };
            match selected_cycle(motion, source_kf) {
                Ok(cycle) => {
                    cycles.insert(role, cycle);
                }
                Err(issue) => {
                    issues.push(issue);
                    cycle_failed = true;
                }
            }
        }
        if cycle_failed {
            continue;
        }

        let (linear, angular_acceleration_seconds, max_yaw_speed) =
            if architecture == MovementArchitecture::Stationary {
                let cycle = cycles["stationary"];
                (None, cycle.duration_seconds, 0.0)
            } else {
                let locomotion = cycles["locomotion"];
                let turn = cycles.get("turn").copied();
                let root_speed = (locomotion.planar_distance > 0.0)
                    .then_some(locomotion.planar_distance / locomotion.duration_seconds);
                let controller_speed = settings.f_move_base_speed
                    * settings.locomotion_multiplier
                    * f32::from(speed_multiplier)
                    / 100.0;
                let max_speed = if graph.uses_controller_speed {
                    controller_speed
                } else {
                    root_speed.unwrap_or(controller_speed)
                };
                let yaw_speed = if turn.is_some_and(|turn| turn.yaw_degrees > 0.0) {
                    let turn = turn.expect("positive turn yaw was checked");
                    turn.yaw_degrees / turn.duration_seconds
                } else {
                    turning_speed
                };
                (
                    Some(LinearMotionEvidence {
                        max_speed,
                        seconds_to_full_speed: locomotion.duration_seconds,
                        seconds_to_stop: locomotion.duration_seconds,
                    }),
                    turn.map_or(locomotion.duration_seconds, |turn| turn.duration_seconds),
                    yaw_speed,
                )
            };

        let aim_tolerance = if graph.aiming_capable {
            let Some(value) = graph.aim_tolerance_degrees else {
                issues.push(LegacyMovementMvpIssue::MissingAimTolerance(
                    family.key.clone(),
                ));
                continue;
            };
            if !valid_angle(value) {
                issues.push(LegacyMovementMvpIssue::InvalidAimTolerance(
                    family.key.clone(),
                ));
                continue;
            }
            value
        } else {
            0.0
        };
        let (pitch_limit, roll_limit) = match architecture {
            MovementArchitecture::Flying
            | MovementArchitecture::GroundedFlying
            | MovementArchitecture::Swimming
            | MovementArchitecture::GroundedSwimming => {
                let (Some(pitch), Some(roll)) = (
                    graph.orientation_pitch_degrees,
                    graph.orientation_roll_degrees,
                ) else {
                    issues.push(LegacyMovementMvpIssue::MissingOrientationLimits(
                        family.key.clone(),
                    ));
                    continue;
                };
                if !valid_nonzero_angle(pitch) || !valid_nonzero_angle(roll) {
                    issues.push(LegacyMovementMvpIssue::InvalidOrientationLimits(
                        family.key.clone(),
                    ));
                    continue;
                }
                (pitch, roll)
            }
            MovementArchitecture::Grounded | MovementArchitecture::Stationary => (0.0, 0.0),
        };
        let mobile = architecture != MovementArchitecture::Stationary;
        let movement = MovementSemantics {
            architecture,
            linear,
            angular: Some(AngularMotionEvidence {
                max_yaw_speed_degrees_per_second: max_yaw_speed,
                seconds_to_full_yaw_speed: angular_acceleration_seconds,
                tolerance_degrees: 0.0,
                aim_tolerance_degrees: aim_tolerance,
                pitch_limit_degrees: pitch_limit,
                roll_limit_degrees: roll_limit,
            }),
            pushable: mobile,
            opens_doors: family_kind == CreatureFamilyKind::HumanoidWeaponOverlay,
            allow_ragdoll_collision: mobile,
            non_hostile: false,
            use_large_actor_pathing: false,
            use_subsegmented_damage: false,
        };
        controllers.push(RigControllerRaceDataContext {
            rig: family.key.clone(),
            architecture: controller_architecture,
            movement: Some(movement),
        });
        fidelity_receipts.extend([
            RaceDataFidelityReceipt::SchemaAbsentDefaulted {
                creature: family_selection.source_creature.clone(),
                field: RaceDataEvidenceField::MovementArchitecture,
                policy: "legacy_movement_mvp_v1:selected_graph_template_and_conservative_flags"
                    .to_string(),
            },
            RaceDataFidelityReceipt::SchemaAbsentDefaulted {
                creature: family_selection.source_creature.clone(),
                field: RaceDataEvidenceField::LinearMotion,
                policy: "legacy_movement_mvp_v1:measured_locomotion_cycle_accel_decel".to_string(),
            },
            RaceDataFidelityReceipt::SchemaAbsentDefaulted {
                creature: family_selection.source_creature.clone(),
                field: RaceDataEvidenceField::AngularMotion,
                policy: "legacy_movement_mvp_v1:measured_turn_cycle_zero_absent_tolerances"
                    .to_string(),
            },
        ]);
    }
    controllers.sort_by(|left, right| left.rig.cmp(&right.rig));
    issues.sort();
    issues.dedup();
    fidelity_receipts.sort();
    fidelity_receipts.dedup();
    LoadedLegacyMovementMvpEvidence {
        controllers,
        issues,
        fidelity_receipts,
    }
}

#[derive(Clone, Copy)]
struct SelectedCycle {
    duration_seconds: f32,
    planar_distance: f32,
    yaw_degrees: f32,
}

fn selected_cycle(
    family: &CreatureMotionFamilyEvidence,
    source_kf: &str,
) -> Result<SelectedCycle, LegacyMovementMvpIssue> {
    let normalized = normalize_asset_path(source_kf);
    if !family
        .referenced_kfs
        .iter()
        .any(|path| normalize_asset_path(path) == normalized)
    {
        return Err(LegacyMovementMvpIssue::UnreferencedSelectedKf {
            rig: family.rig.clone(),
            role: "selected_cycle".to_string(),
            source_kf: source_kf.to_string(),
        });
    }
    let candidates = family
        .kf_evidence
        .iter()
        .filter(|evidence| normalize_asset_path(&evidence.source_kf) == normalized)
        .collect::<Vec<_>>();
    let evidence = match candidates.as_slice() {
        [] => {
            return Err(LegacyMovementMvpIssue::UnavailableSelectedKf {
                rig: family.rig.clone(),
                source_kf: source_kf.to_string(),
                reason: "no parsed KF evidence".to_string(),
            });
        }
        [evidence] => *evidence,
        _ => {
            return Err(LegacyMovementMvpIssue::DuplicateSelectedKf {
                rig: family.rig.clone(),
                source_kf: source_kf.to_string(),
            });
        }
    };
    let sequence = match &evidence.sequence {
        KfParseEvidence::Parsed(sequence)
            if sequence.binding.compatibility == BindingCompatibility::Verified =>
        {
            sequence
        }
        KfParseEvidence::Parsed(_) => {
            return Err(LegacyMovementMvpIssue::UnavailableSelectedKf {
                rig: family.rig.clone(),
                source_kf: source_kf.to_string(),
                reason: "KF binding compatibility is not verified".to_string(),
            });
        }
        KfParseEvidence::MissingAsset => {
            return Err(LegacyMovementMvpIssue::UnavailableSelectedKf {
                rig: family.rig.clone(),
                source_kf: source_kf.to_string(),
                reason: "KF asset is missing".to_string(),
            });
        }
        KfParseEvidence::ParseFailed(reason) => {
            return Err(LegacyMovementMvpIssue::UnavailableSelectedKf {
                rig: family.rig.clone(),
                source_kf: source_kf.to_string(),
                reason: reason.clone(),
            });
        }
    };
    let source_span = sequence.stop_time - sequence.start_time;
    if !source_span.is_finite()
        || source_span <= 0.0
        || !sequence.frequency.is_finite()
        || sequence.frequency <= 0.0
    {
        return Err(LegacyMovementMvpIssue::InvalidSelectedCycle {
            rig: family.rig.clone(),
            source_kf: source_kf.to_string(),
            reason: "KF cycle span and frequency must be finite and positive".to_string(),
        });
    }
    let duration_seconds = (source_span / sequence.frequency) as f32;
    let (planar_distance, yaw_degrees) = match &sequence.root_motion {
        RootMotionEvidence::Planar {
            distance,
            yaw_radians,
            ..
        } => (distance.abs() as f32, yaw_radians.abs().to_degrees() as f32),
        RootMotionEvidence::None | RootMotionEvidence::Stationary { .. } => (0.0, 0.0),
        RootMotionEvidence::Unknown => {
            return Err(LegacyMovementMvpIssue::UnavailableSelectedKf {
                rig: family.rig.clone(),
                source_kf: source_kf.to_string(),
                reason: "KF root motion is unknown".to_string(),
            });
        }
        RootMotionEvidence::Unsupported { detail, .. } => {
            return Err(LegacyMovementMvpIssue::UnavailableSelectedKf {
                rig: family.rig.clone(),
                source_kf: source_kf.to_string(),
                reason: detail.clone(),
            });
        }
    };
    if !duration_seconds.is_finite() || duration_seconds <= 0.0 {
        return Err(LegacyMovementMvpIssue::InvalidSelectedCycle {
            rig: family.rig.clone(),
            source_kf: source_kf.to_string(),
            reason: "derived KF runtime duration must be finite and positive".to_string(),
        });
    }
    Ok(SelectedCycle {
        duration_seconds,
        planar_distance,
        yaw_degrees,
    })
}

fn movement_architecture(template: CreatureGraphTemplate) -> MovementArchitecture {
    match template {
        CreatureGraphTemplate::GroundMelee
        | CreatureGraphTemplate::PassiveGround
        | CreatureGraphTemplate::GroundRangedProjectile
        | CreatureGraphTemplate::GroundMeleeRanged
        | CreatureGraphTemplate::RobotContinuousAttack => MovementArchitecture::Grounded,
        CreatureGraphTemplate::Swim => MovementArchitecture::Swimming,
        CreatureGraphTemplate::GroundSwim => MovementArchitecture::GroundedSwimming,
        CreatureGraphTemplate::Fly => MovementArchitecture::Flying,
        CreatureGraphTemplate::GroundFly => MovementArchitecture::GroundedFlying,
        CreatureGraphTemplate::StationaryTurret => MovementArchitecture::Stationary,
    }
}

fn controller_architecture(
    template: CreatureGraphTemplate,
    family_kind: CreatureFamilyKind,
) -> ControllerArchitecture {
    match template {
        CreatureGraphTemplate::StationaryTurret => ControllerArchitecture::Fixed,
        CreatureGraphTemplate::GroundMelee if family_kind == CreatureFamilyKind::Organic => {
            ControllerArchitecture::Quadruped
        }
        _ => ControllerArchitecture::Standard,
    }
}

fn normalize_asset_path(path: &str) -> String {
    path.replace('/', "\\").to_ascii_lowercase()
}

fn valid_angle(value: f32) -> bool {
    value.is_finite() && (0.0..=180.0).contains(&value)
}

fn valid_nonzero_angle(value: f32) -> bool {
    value.is_finite() && (0.0..=180.0).contains(&value) && value > 0.0
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum NifEvidenceLoadIssue {
    MissingRigContext(RigFamilyKey),
    DuplicateRigContext(RigFamilyKey),
    CatalogRigUnavailable {
        rig: RigFamilyKey,
        readiness: String,
    },
    CatalogBodyUnavailable {
        body: BodyVariantKey,
        readiness: String,
    },
    InvalidAssetPath(String),
    SkeletonBounds {
        rig: RigFamilyKey,
        path: String,
        reason: String,
    },
    BodyBounds {
        body: BodyVariantKey,
        path: String,
        reason: String,
    },
    EmptyBodyPaths(BodyVariantKey),
    MissingControllerContext(RigFamilyKey),
    DuplicateControllerContext(RigFamilyKey),
    MissingControllerBound {
        rig: RigFamilyKey,
        path: String,
    },
    AmbiguousControllerBound {
        rig: RigFamilyKey,
        path: String,
        count: usize,
    },
    InvalidControllerBound {
        rig: RigFamilyKey,
        path: String,
        reason: String,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct LoadedNifRaceDataEvidence {
    pub rigs: Vec<RigRaceDataEvidence>,
    pub bodies: Vec<BodyRaceDataEvidence>,
    pub controllers: Vec<ControllerRaceDataEvidence>,
    pub issues: Vec<NifEvidenceLoadIssue>,
    pub remaining_requirements: Vec<ProductionRaceDataRequirement>,
}

pub fn load_nif_race_data_evidence(
    catalog: &CreatureCorpusPlan,
    mesh_root: &Path,
    rig_contexts: &[RigNifMeasurementContext],
) -> LoadedNifRaceDataEvidence {
    let mut contexts = BTreeMap::<RigFamilyKey, Vec<&RigNifMeasurementContext>>::new();
    for context in rig_contexts {
        contexts
            .entry(context.rig.clone())
            .or_default()
            .push(context);
    }
    let mut remaining_requirements = production_evidence_requirements(catalog)
        .into_iter()
        .collect::<BTreeSet<_>>();
    let mut rigs = Vec::new();
    let mut bodies = Vec::new();
    let mut issues = Vec::new();

    for family in &catalog.rig_families {
        let context_candidates = contexts.get(&family.key).map(Vec::as_slice).unwrap_or(&[]);
        if context_candidates.is_empty() {
            issues.push(NifEvidenceLoadIssue::MissingRigContext(family.key.clone()));
            continue;
        }
        if context_candidates.len() != 1 {
            issues.push(NifEvidenceLoadIssue::DuplicateRigContext(
                family.key.clone(),
            ));
            continue;
        }
        if !matches!(family.readiness, AssetReadiness::Ready) {
            issues.push(NifEvidenceLoadIssue::CatalogRigUnavailable {
                rig: family.key.clone(),
                readiness: format!("{:?}", family.readiness),
            });
            continue;
        }
        let skeleton_path = family.key.skeleton_path.clone();
        let family_mesh_root = super::creature_catalog::game_mesh_root(mesh_root, family.key.game);
        let Some(resolved) = explicit_asset_path(&family_mesh_root, &skeleton_path) else {
            issues.push(NifEvidenceLoadIssue::InvalidAssetPath(skeleton_path));
            continue;
        };
        match aggregate_named_node_world_bounds(&resolved) {
            Ok(bounds) => {
                let context = context_candidates[0];
                rigs.push(RigRaceDataEvidence {
                    rig: family.key.clone(),
                    skeleton_bounds: Some(measured(bounds)),
                    up_axis: context.up_axis,
                    geometry_scale_to_fo4: context.geometry_scale_to_fo4,
                });
                remaining_requirements.remove(&ProductionRaceDataRequirement::RigMeasurements {
                    rig: family.key.clone(),
                    skeleton_path,
                });
            }
            Err(error) => issues.push(NifEvidenceLoadIssue::SkeletonBounds {
                rig: family.key.clone(),
                path: skeleton_path,
                reason: error.to_string(),
            }),
        }
    }

    for body in &catalog.body_variants {
        if !matches!(body.readiness, BodyReadiness::Ready) {
            issues.push(NifEvidenceLoadIssue::CatalogBodyUnavailable {
                body: body.key.clone(),
                readiness: format!("{:?}", body.readiness),
            });
            continue;
        }
        if body.key.body_paths.is_empty() {
            issues.push(NifEvidenceLoadIssue::EmptyBodyPaths(body.key.clone()));
            continue;
        }
        let mut aggregate = None;
        let mut failed = false;
        let body_mesh_root = super::creature_catalog::game_mesh_root(mesh_root, body.key.rig.game);
        for body_path in &body.key.body_paths {
            let Some(resolved) = explicit_asset_path(&body_mesh_root, body_path) else {
                issues.push(NifEvidenceLoadIssue::InvalidAssetPath(body_path.clone()));
                failed = true;
                continue;
            };
            if !resolved.is_file() {
                continue;
            }
            match aggregate_render_world_bounds(&resolved) {
                Ok(bounds) => union_bounds(&mut aggregate, bounds),
                Err(error) if non_bounds_body_accessory(&error) => {}
                Err(error) => {
                    issues.push(NifEvidenceLoadIssue::BodyBounds {
                        body: body.key.clone(),
                        path: body_path.clone(),
                        reason: error.to_string(),
                    });
                    failed = true;
                }
            }
        }
        if !failed && let Some(bounds) = aggregate {
            bodies.push(BodyRaceDataEvidence {
                body: body.key.clone(),
                body_bounds: Some(measured(bounds)),
            });
            remaining_requirements.remove(&ProductionRaceDataRequirement::BodyMeasurements {
                body: body.key.clone(),
                body_paths: body.key.body_paths.clone(),
            });
        }
    }

    rigs.sort_by(|left, right| left.rig.cmp(&right.rig));
    bodies.sort_by(|left, right| left.body.cmp(&right.body));
    issues.sort();
    issues.dedup();
    LoadedNifRaceDataEvidence {
        rigs,
        bodies,
        controllers: Vec::new(),
        issues,
        remaining_requirements: remaining_requirements.into_iter().collect(),
    }
}

fn non_bounds_body_accessory(error: &WorldBoundsError) -> bool {
    match error {
        WorldBoundsError::EmptyGeometry => true,
        WorldBoundsError::UnsupportedGeometry { type_name, .. } => type_name == "NiParticleSystem",
        _ => false,
    }
}

pub fn load_nif_race_data_evidence_with_controllers(
    catalog: &CreatureCorpusPlan,
    mesh_root: &Path,
    rig_contexts: &[RigNifMeasurementContext],
    controller_contexts: &[RigControllerRaceDataContext],
) -> LoadedNifRaceDataEvidence {
    let mut loaded = load_nif_race_data_evidence(catalog, mesh_root, rig_contexts);
    let mut contexts = BTreeMap::<RigFamilyKey, Vec<&RigControllerRaceDataContext>>::new();
    for context in controller_contexts {
        contexts
            .entry(context.rig.clone())
            .or_default()
            .push(context);
    }
    let mut remaining = loaded
        .remaining_requirements
        .into_iter()
        .collect::<BTreeSet<_>>();
    for family in &catalog.rig_families {
        let candidates = contexts.get(&family.key).map(Vec::as_slice).unwrap_or(&[]);
        if candidates.is_empty() {
            loaded
                .issues
                .push(NifEvidenceLoadIssue::MissingControllerContext(
                    family.key.clone(),
                ));
            continue;
        }
        if candidates.len() != 1 {
            loaded
                .issues
                .push(NifEvidenceLoadIssue::DuplicateControllerContext(
                    family.key.clone(),
                ));
            continue;
        }
        let skeleton_path = family.key.skeleton_path.clone();
        let family_mesh_root = super::creature_catalog::game_mesh_root(mesh_root, family.key.game);
        let Some(path) = explicit_asset_path(&family_mesh_root, &skeleton_path) else {
            loaded
                .issues
                .push(NifEvidenceLoadIssue::InvalidAssetPath(skeleton_path));
            continue;
        };
        let context = candidates[0];
        match load_bsbound_capsule(&path, &family.key, context.architecture) {
            Ok(capsule) => {
                loaded.controllers.push(ControllerRaceDataEvidence {
                    rig: family.key.clone(),
                    capsule: Some(capsule),
                    movement: context.movement.clone(),
                });
                if context.movement.is_some() {
                    remaining.remove(&ProductionRaceDataRequirement::ControllerMeasurements {
                        rig: family.key.clone(),
                        skeleton_path,
                    });
                }
            }
            Err(issue) => loaded.issues.push(issue),
        }
    }
    loaded
        .controllers
        .sort_by(|left, right| left.rig.cmp(&right.rig));
    loaded.issues.sort();
    loaded.issues.dedup();
    loaded.remaining_requirements = remaining.into_iter().collect();
    loaded
}

fn load_bsbound_capsule(
    path: &Path,
    rig: &RigFamilyKey,
    architecture: crate::source_rig::race_data::ControllerArchitecture,
) -> Result<CapsuleEvidence, NifEvidenceLoadIssue> {
    let nif = NifFile::load(path.to_path_buf()).map_err(|error| {
        NifEvidenceLoadIssue::InvalidControllerBound {
            rig: rig.clone(),
            path: rig.skeleton_path.clone(),
            reason: error.to_string(),
        }
    })?;
    let bounds = nif
        .blocks
        .iter()
        .filter(|block| block.type_name == "BSBound")
        .collect::<Vec<_>>();
    if bounds.is_empty() {
        return Err(NifEvidenceLoadIssue::MissingControllerBound {
            rig: rig.clone(),
            path: rig.skeleton_path.clone(),
        });
    }
    if bounds.len() != 1 {
        return Err(NifEvidenceLoadIssue::AmbiguousControllerBound {
            rig: rig.clone(),
            path: rig.skeleton_path.clone(),
            count: bounds.len(),
        });
    }
    let dimensions = nif_vec3(bounds[0].get_field("Dimensions")).ok_or_else(|| {
        NifEvidenceLoadIssue::InvalidControllerBound {
            rig: rig.clone(),
            path: rig.skeleton_path.clone(),
            reason: "BSBound Dimensions are missing or malformed".to_string(),
        }
    })?;
    if !dimensions
        .iter()
        .all(|value| value.is_finite() && *value > 0.0)
    {
        return Err(NifEvidenceLoadIssue::InvalidControllerBound {
            rig: rig.clone(),
            path: rig.skeleton_path.clone(),
            reason: "BSBound Dimensions must be finite and positive".to_string(),
        });
    }
    let radius = dimensions[0].min(dimensions[1]).min(dimensions[2]);
    let total_height = dimensions[2] * 2.0;
    Ok(CapsuleEvidence {
        radius,
        total_height,
        architecture,
    })
}

fn nif_vec3(value: Option<&NifValue>) -> Option<[f32; 3]> {
    match value? {
        NifValue::Vec3(value) => Some(*value),
        NifValue::Struct(fields) => Some([
            nif_number(fields.get("x"))?,
            nif_number(fields.get("y"))?,
            nif_number(fields.get("z"))?,
        ]),
        _ => None,
    }
}

fn nif_number(value: Option<&NifValue>) -> Option<f32> {
    match value? {
        NifValue::Float(value) => Some(*value as f32),
        NifValue::Int(value) => Some(*value as f32),
        NifValue::UInt(value) => Some(*value as f32),
        _ => None,
    }
}

fn explicit_asset_path(mesh_root: &Path, relative: &str) -> Option<PathBuf> {
    let mut result = mesh_root.to_path_buf();
    let mut saw_part = false;
    for part in relative.split(['/', '\\']) {
        if part.is_empty() {
            continue;
        }
        if part == "." || part == ".." || part.ends_with(':') {
            return None;
        }
        result.push(part);
        saw_part = true;
    }
    saw_part.then_some(result)
}

fn measured(bounds: WorldBounds) -> MeasuredBounds {
    MeasuredBounds {
        min: bounds.min,
        max: bounds.max,
    }
}

fn union_bounds(aggregate: &mut Option<WorldBounds>, next: WorldBounds) {
    match aggregate {
        Some(current) => {
            for axis in 0..3 {
                current.min[axis] = current.min[axis].min(next.min[axis]);
                current.max[axis] = current.max[axis].max(next.max[axis]);
            }
        }
        None => *aggregate = Some(next),
    }
}

pub fn production_evidence_requirements(
    catalog: &CreatureCorpusPlan,
) -> Vec<ProductionRaceDataRequirement> {
    let mut requirements = BTreeSet::new();
    for family in &catalog.rig_families {
        requirements.insert(ProductionRaceDataRequirement::RigMeasurements {
            rig: family.key.clone(),
            skeleton_path: family.key.skeleton_path.clone(),
        });
        requirements.insert(ProductionRaceDataRequirement::ControllerMeasurements {
            rig: family.key.clone(),
            skeleton_path: family.key.skeleton_path.clone(),
        });
    }
    for body in &catalog.body_variants {
        requirements.insert(ProductionRaceDataRequirement::BodyMeasurements {
            body: body.key.clone(),
            body_paths: body.key.body_paths.clone(),
        });
    }
    for record in &catalog.records {
        if matches!(record.disposition, CreatureDisposition::VisualOwner { .. }) {
            requirements.insert(ProductionRaceDataRequirement::CreatureSemantics {
                creature: record.source.clone(),
                provenance: record.provenance.game,
            });
        }
    }
    requirements.into_iter().collect()
}

pub fn build_full_merged_race_data_adapter(
    catalog: &CreatureCorpusPlan,
    inputs: LegacyRaceDataEvidenceInputs<'_>,
    policy: &Fo4RaceDataScalePolicy,
) -> Result<LegacyRaceDataAdapterPlan, LegacyRaceDataAdapterError> {
    if catalog.records.len() != EXPECTED_FULL_MERGED_CREA_WINNERS {
        return Err(LegacyRaceDataAdapterError::WinnerCountMismatch {
            expected: EXPECTED_FULL_MERGED_CREA_WINNERS,
            actual: catalog.records.len(),
        });
    }
    build_race_data_adapter(catalog, inputs, policy)
}

pub fn build_race_data_adapter(
    catalog: &CreatureCorpusPlan,
    inputs: LegacyRaceDataEvidenceInputs<'_>,
    policy: &Fo4RaceDataScalePolicy,
) -> Result<LegacyRaceDataAdapterPlan, LegacyRaceDataAdapterError> {
    validate_policy(policy)?;
    validate_catalog(catalog)?;

    let mut records_by_source = BTreeMap::new();
    for record in &catalog.records {
        if records_by_source
            .insert(record.source.clone(), record)
            .is_some()
        {
            return Err(LegacyRaceDataAdapterError::DuplicateCatalogRecord(
                record.source.clone(),
            ));
        }
    }
    let mut bodies_by_key = BTreeMap::new();
    for body in &catalog.body_variants {
        if bodies_by_key.insert(body.key.clone(), body).is_some() {
            return Err(LegacyRaceDataAdapterError::DuplicateCatalogBody(
                body.key.clone(),
            ));
        }
    }
    let mut families_by_key = BTreeMap::new();
    for family in &catalog.rig_families {
        if families_by_key.insert(family.key.clone(), family).is_some() {
            return Err(LegacyRaceDataAdapterError::DuplicateCatalogRig(
                family.key.clone(),
            ));
        }
    }
    let mut family_kinds = BTreeMap::<RigFamilyKey, Vec<CreatureFamilyKind>>::new();
    for attack_set in &catalog.attack_sets {
        family_kinds
            .entry(attack_set.rig.clone())
            .or_default()
            .push(attack_set.family_kind);
    }
    for family in &catalog.rig_families {
        if family_kinds
            .get(&family.key)
            .is_none_or(|kinds| kinds.len() != 1)
        {
            return Err(LegacyRaceDataAdapterError::AttackSetCoverage(
                family.key.clone(),
            ));
        }
    }

    let mut selections = BTreeMap::<RigFamilyKey, Vec<&FamilyRaceDataSelection>>::new();
    for selection in inputs.selections {
        selections
            .entry(selection.rig.clone())
            .or_default()
            .push(selection);
    }
    let mut rig_evidence = BTreeMap::<RigFamilyKey, Vec<&RigRaceDataEvidence>>::new();
    for evidence in inputs.rigs {
        rig_evidence
            .entry(evidence.rig.clone())
            .or_default()
            .push(evidence);
    }
    let mut body_evidence = BTreeMap::<BodyVariantKey, Vec<&BodyRaceDataEvidence>>::new();
    for evidence in inputs.bodies {
        body_evidence
            .entry(evidence.body.clone())
            .or_default()
            .push(evidence);
    }
    let mut controller_evidence = BTreeMap::<RigFamilyKey, Vec<&ControllerRaceDataEvidence>>::new();
    for evidence in inputs.controllers {
        controller_evidence
            .entry(evidence.rig.clone())
            .or_default()
            .push(evidence);
    }
    let mut creature_evidence = BTreeMap::<StableFormKey, Vec<&CreatureRaceDataEvidence>>::new();
    for evidence in inputs.creatures {
        creature_evidence
            .entry(evidence.creature.clone())
            .or_default()
            .push(evidence);
    }

    let mut families = Vec::with_capacity(catalog.rig_families.len());
    for family in &catalog.rig_families {
        let family_kind = family_kinds[&family.key][0];
        let selection_candidates = selections
            .get(&family.key)
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        let (selection, status) = if selection_candidates.is_empty() {
            (
                None,
                FamilyRaceDataStatus::Missing {
                    fields: all_evidence_fields(),
                    issues: vec![FamilyRaceDataIssue::MissingFamilySelection],
                },
            )
        } else if selection_candidates.len() != 1 {
            (
                None,
                FamilyRaceDataStatus::Invalid {
                    fields: all_evidence_fields(),
                    issues: vec![FamilyRaceDataIssue::DuplicateFamilySelection],
                },
            )
        } else {
            let selection = (*selection_candidates[0]).clone();
            let status = build_family_status(
                family,
                &selection,
                &records_by_source,
                &bodies_by_key,
                &rig_evidence,
                &body_evidence,
                &controller_evidence,
                &creature_evidence,
                policy,
            );
            (Some(selection), status)
        };
        families.push(FamilyRaceDataEntry {
            rig: family.key.clone(),
            provenance: family.key.game,
            family_kind,
            selection,
            status,
        });
    }
    families.sort_by(|left, right| left.rig.cmp(&right.rig));

    let mut records = catalog
        .records
        .iter()
        .map(|record| RecordRaceDataEntry {
            source: record.source.clone(),
            provenance: record.provenance.clone(),
            disposition: record_disposition(record, &records_by_source),
        })
        .collect::<Vec<_>>();
    records.sort_by(|left, right| left.source.cmp(&right.source));

    let accounting = build_accounting(catalog, &families, &records)?;
    Ok(LegacyRaceDataAdapterPlan {
        families,
        records,
        accounting,
    })
}

#[allow(clippy::too_many_arguments)]
fn build_family_status(
    family: &super::creature_catalog::RigFamily,
    selection: &FamilyRaceDataSelection,
    records: &BTreeMap<StableFormKey, &super::creature_catalog::RecordVariant>,
    bodies: &BTreeMap<BodyVariantKey, &super::creature_catalog::BodyVariant>,
    rig_evidence: &BTreeMap<RigFamilyKey, Vec<&RigRaceDataEvidence>>,
    body_evidence: &BTreeMap<BodyVariantKey, Vec<&BodyRaceDataEvidence>>,
    controller_evidence: &BTreeMap<RigFamilyKey, Vec<&ControllerRaceDataEvidence>>,
    creature_evidence: &BTreeMap<StableFormKey, Vec<&CreatureRaceDataEvidence>>,
    policy: &Fo4RaceDataScalePolicy,
) -> FamilyRaceDataStatus {
    let mut invalid_fields = Vec::new();
    let mut invalid_issues = Vec::new();

    if !matches!(family.readiness, AssetReadiness::Ready) {
        invalid_fields.push(RaceDataEvidenceField::SkeletonBounds);
        invalid_issues.push(FamilyRaceDataIssue::CatalogRigUnavailable(format!(
            "{:?}",
            family.readiness
        )));
    }
    let selected_record = records.get(&selection.source_creature).copied();
    match selected_record {
        None => {
            invalid_fields.extend(creature_fields());
            invalid_issues.push(FamilyRaceDataIssue::UnknownSourceCreature(
                selection.source_creature.clone(),
            ));
        }
        Some(record) => match &record.disposition {
            CreatureDisposition::VisualOwner { rig, body } => {
                if rig != &family.key {
                    invalid_fields.extend(all_evidence_fields());
                    invalid_issues.push(FamilyRaceDataIssue::SourceCreatureRigMismatch {
                        expected: family.key.clone(),
                        actual: rig.clone(),
                    });
                }
                if body != &selection.source_body {
                    invalid_fields.push(RaceDataEvidenceField::BodyBounds);
                    invalid_issues.push(FamilyRaceDataIssue::SourceCreatureBodyMismatch {
                        expected: selection.source_body.clone(),
                        actual: body.clone(),
                    });
                }
                if record.provenance.game != family.key.game {
                    invalid_fields.extend(all_evidence_fields());
                    invalid_issues.push(FamilyRaceDataIssue::SourceGameMismatch {
                        rig_game: family.key.game,
                        creature_game: record.provenance.game,
                    });
                }
            }
            _ => {
                invalid_fields.extend(all_evidence_fields());
                invalid_issues.push(FamilyRaceDataIssue::SourceCreatureNotVisualOwner(
                    selection.source_creature.clone(),
                ));
            }
        },
    }

    match bodies.get(&selection.source_body).copied() {
        None => {
            invalid_fields.push(RaceDataEvidenceField::BodyBounds);
            invalid_issues.push(FamilyRaceDataIssue::UnknownSourceBody(
                selection.source_body.clone(),
            ));
        }
        Some(body) => {
            if body.key.rig != family.key {
                invalid_fields.push(RaceDataEvidenceField::BodyBounds);
                invalid_issues.push(FamilyRaceDataIssue::SourceCreatureBodyMismatch {
                    expected: selection.source_body.clone(),
                    actual: body.key.clone(),
                });
            }
            if !matches!(body.readiness, BodyReadiness::Ready) {
                invalid_fields.push(RaceDataEvidenceField::BodyBounds);
                invalid_issues.push(FamilyRaceDataIssue::CatalogBodyUnavailable(format!(
                    "{:?}",
                    body.readiness
                )));
            }
        }
    }

    let rig_candidates = rig_evidence
        .get(&family.key)
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    let body_candidates = body_evidence
        .get(&selection.source_body)
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    let controller_candidates = controller_evidence
        .get(&family.key)
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    let creature_candidates = creature_evidence
        .get(&selection.source_creature)
        .map(Vec::as_slice)
        .unwrap_or(&[]);

    let mut missing_fields = Vec::new();
    let mut missing_issues = Vec::new();
    match rig_candidates.len() {
        0 => {
            missing_fields.extend(rig_fields());
            missing_issues.push(FamilyRaceDataIssue::MissingRigEvidence);
        }
        1 => {}
        _ => {
            invalid_fields.extend(rig_fields());
            invalid_issues.push(FamilyRaceDataIssue::DuplicateRigEvidence);
        }
    }
    match body_candidates.len() {
        0 => {
            missing_fields.push(RaceDataEvidenceField::BodyBounds);
            missing_issues.push(FamilyRaceDataIssue::MissingBodyEvidence);
        }
        1 => {}
        _ => {
            invalid_fields.push(RaceDataEvidenceField::BodyBounds);
            invalid_issues.push(FamilyRaceDataIssue::DuplicateBodyEvidence);
        }
    }
    match controller_candidates.len() {
        0 => {
            missing_fields.extend(controller_fields());
            missing_issues.push(FamilyRaceDataIssue::MissingControllerEvidence);
        }
        1 => {}
        _ => {
            invalid_fields.extend(controller_fields());
            invalid_issues.push(FamilyRaceDataIssue::DuplicateControllerEvidence);
        }
    }
    match creature_candidates.len() {
        0 => {
            missing_fields.extend(creature_fields());
            missing_issues.push(FamilyRaceDataIssue::MissingCreatureEvidence);
        }
        1 => {}
        _ => {
            invalid_fields.extend(creature_fields());
            invalid_issues.push(FamilyRaceDataIssue::DuplicateCreatureEvidence);
        }
    }

    if !invalid_issues.is_empty() {
        sort_fields(&mut invalid_fields);
        invalid_issues.sort();
        invalid_issues.dedup();
        return FamilyRaceDataStatus::Invalid {
            fields: invalid_fields,
            issues: invalid_issues,
        };
    }
    if !missing_issues.is_empty() {
        sort_fields(&mut missing_fields);
        missing_issues.sort();
        missing_issues.dedup();
        return FamilyRaceDataStatus::Missing {
            fields: missing_fields,
            issues: missing_issues,
        };
    }

    let rig = rig_candidates[0];
    let body = body_candidates[0];
    let controller = controller_candidates[0];
    let creature = creature_candidates[0];
    let evidence = SourceFamilyRaceDataEvidence {
        body_bounds: body.body_bounds.clone(),
        skeleton_bounds: rig.skeleton_bounds.clone(),
        up_axis: rig.up_axis,
        capsule: controller.capsule.clone(),
        geometry_scale_to_fo4: rig.geometry_scale_to_fo4,
        male_actor_scale: creature.male_actor_scale,
        female_actor_scale: creature.female_actor_scale,
        default_weights: creature.default_weights.clone(),
        movement: controller.movement.clone(),
        injured_health_percent: creature.injured_health_percent,
        body_biped_object: creature.body_biped_object,
        xp_value: creature.xp_value,
    };
    match derive_fo4_race_data(&evidence, policy) {
        Ok(derivation) if derivation.missing_evidence.is_empty() => FamilyRaceDataStatus::Ready {
            evidence,
            derivation,
        },
        Ok(derivation) => FamilyRaceDataStatus::Missing {
            fields: derivation.missing_evidence,
            issues: Vec::new(),
        },
        Err(RaceDataDerivationError::InvalidEvidence { field, reason }) => {
            FamilyRaceDataStatus::Invalid {
                fields: vec![field],
                issues: vec![FamilyRaceDataIssue::InvalidEvidence { field, reason }],
            }
        }
        Err(RaceDataDerivationError::InvalidTarget(reason)) => FamilyRaceDataStatus::Invalid {
            fields: Vec::new(),
            issues: vec![FamilyRaceDataIssue::InvalidDerivedTarget(reason)],
        },
        Err(RaceDataDerivationError::InvalidSizePolicy(reason)) => FamilyRaceDataStatus::Invalid {
            fields: Vec::new(),
            issues: vec![FamilyRaceDataIssue::InvalidDerivedTarget(reason)],
        },
    }
}

fn record_disposition(
    record: &super::creature_catalog::RecordVariant,
    records: &BTreeMap<StableFormKey, &super::creature_catalog::RecordVariant>,
) -> RecordRaceDataDisposition {
    match &record.disposition {
        CreatureDisposition::VisualOwner { rig, .. } => {
            RecordRaceDataDisposition::VisualOwner { rig: rig.clone() }
        }
        CreatureDisposition::Special { reason } => RecordRaceDataDisposition::Special {
            reason: reason.clone(),
        },
        CreatureDisposition::Proxy {
            terminal_visual_owners,
            readiness,
        } => {
            let mut blockers = match readiness {
                ProxyReadiness::Ready => Vec::new(),
                ProxyReadiness::Blocked(blockers) => blockers
                    .iter()
                    .cloned()
                    .map(ProxyRaceDataBlocker::Catalog)
                    .collect(),
            };
            let mut rigs = BTreeSet::new();
            for owner in terminal_visual_owners {
                match records.get(owner).map(|record| &record.disposition) {
                    None => blockers.push(ProxyRaceDataBlocker::MissingTerminalVisualOwner(
                        owner.clone(),
                    )),
                    Some(CreatureDisposition::VisualOwner { rig, .. }) => {
                        rigs.insert(rig.clone());
                    }
                    Some(_) => blockers.push(ProxyRaceDataBlocker::TerminalIsNotVisualOwner(
                        owner.clone(),
                    )),
                }
            }
            blockers.sort();
            blockers.dedup();
            if !blockers.is_empty() || rigs.is_empty() {
                if blockers.is_empty() {
                    blockers.push(ProxyRaceDataBlocker::TerminalIsNotVisualOwner(
                        record.source.clone(),
                    ));
                }
                RecordRaceDataDisposition::UnavailableProxy { blockers }
            } else if rigs.len() == 1 {
                RecordRaceDataDisposition::Proxy {
                    rig: rigs.into_iter().next().expect("one proxy family"),
                    terminal_visual_owners: terminal_visual_owners.clone(),
                }
            } else {
                RecordRaceDataDisposition::AmbiguousProxy {
                    rigs: rigs.into_iter().collect(),
                    terminal_visual_owners: terminal_visual_owners.clone(),
                }
            }
        }
    }
}

fn build_accounting(
    catalog: &CreatureCorpusPlan,
    families: &[FamilyRaceDataEntry],
    records: &[RecordRaceDataEntry],
) -> Result<RaceDataAdapterAccounting, LegacyRaceDataAdapterError> {
    let mut accounting = RaceDataAdapterAccounting {
        catalog_records: catalog.records.len(),
        record_dispositions: records.len(),
        rig_families: families.len(),
        ..RaceDataAdapterAccounting::default()
    };
    let family_status = families
        .iter()
        .map(|family| (&family.rig, &family.status))
        .collect::<BTreeMap<_, _>>();
    for family in families {
        match family.status {
            FamilyRaceDataStatus::Ready { .. } => accounting.ready_families += 1,
            FamilyRaceDataStatus::Missing { .. } => accounting.missing_families += 1,
            FamilyRaceDataStatus::Invalid { .. } => accounting.invalid_families += 1,
        }
    }
    for record in records {
        match record.provenance.game {
            LegacyCreatureGame::Fnv => accounting.fnv_records += 1,
            LegacyCreatureGame::Fo3 => accounting.fo3_records += 1,
        }
        let family = match &record.disposition {
            RecordRaceDataDisposition::VisualOwner { rig } => {
                accounting.visual_owner_records += 1;
                Some(rig)
            }
            RecordRaceDataDisposition::Proxy { rig, .. } => {
                accounting.proxy_records += 1;
                Some(rig)
            }
            RecordRaceDataDisposition::AmbiguousProxy { .. } => {
                accounting.proxy_records += 1;
                accounting.ambiguous_proxy_records += 1;
                None
            }
            RecordRaceDataDisposition::UnavailableProxy { .. } => {
                accounting.proxy_records += 1;
                accounting.unavailable_proxy_records += 1;
                None
            }
            RecordRaceDataDisposition::Special { .. } => {
                accounting.special_records += 1;
                None
            }
        };
        if let Some(status) = family.and_then(|rig| family_status.get(rig).copied()) {
            match status {
                FamilyRaceDataStatus::Ready { .. } => accounting.records_with_ready_family += 1,
                FamilyRaceDataStatus::Missing { .. } => accounting.records_with_missing_family += 1,
                FamilyRaceDataStatus::Invalid { .. } => accounting.records_with_invalid_family += 1,
            }
        }
    }

    if accounting.record_dispositions != accounting.catalog_records {
        return Err(LegacyRaceDataAdapterError::AdapterAccounting(
            "record dispositions do not cover the catalog".to_string(),
        ));
    }
    if accounting.fnv_records + accounting.fo3_records != accounting.catalog_records {
        return Err(LegacyRaceDataAdapterError::AdapterAccounting(
            "source provenance does not cover the catalog".to_string(),
        ));
    }
    if accounting.visual_owner_records + accounting.proxy_records + accounting.special_records
        != accounting.catalog_records
    {
        return Err(LegacyRaceDataAdapterError::AdapterAccounting(
            "record disposition classes do not cover the catalog".to_string(),
        ));
    }
    if accounting.ready_families + accounting.missing_families + accounting.invalid_families
        != accounting.rig_families
    {
        return Err(LegacyRaceDataAdapterError::AdapterAccounting(
            "family statuses do not cover every rig".to_string(),
        ));
    }
    Ok(accounting)
}

fn validate_catalog(catalog: &CreatureCorpusPlan) -> Result<(), LegacyRaceDataAdapterError> {
    if catalog.records.len() != catalog.accounting.winning_creatures
        || catalog.records.len() != catalog.accounting.dispositions
    {
        return Err(LegacyRaceDataAdapterError::CatalogAccounting(
            "winning CREA and disposition counts differ".to_string(),
        ));
    }
    if catalog.rig_families.len() != catalog.accounting.rig_families
        || catalog.body_variants.len() != catalog.accounting.body_variants
        || catalog.attack_sets.len() != catalog.accounting.attack_sets
    {
        return Err(LegacyRaceDataAdapterError::CatalogAccounting(
            "family, body, or attack-set counts differ".to_string(),
        ));
    }
    Ok(())
}

fn validate_policy(policy: &Fo4RaceDataScalePolicy) -> Result<(), LegacyRaceDataAdapterError> {
    let values = [
        policy.small_max_stature,
        policy.medium_max_stature,
        policy.large_max_stature,
    ];
    if !values.iter().all(|value| value.is_finite() && *value > 0.0) {
        return Err(LegacyRaceDataAdapterError::InvalidSizePolicy(
            "thresholds must be finite and positive".to_string(),
        ));
    }
    if !(values[0] < values[1] && values[1] < values[2]) {
        return Err(LegacyRaceDataAdapterError::InvalidSizePolicy(
            "thresholds must be strictly increasing".to_string(),
        ));
    }
    Ok(())
}

fn all_evidence_fields() -> Vec<RaceDataEvidenceField> {
    vec![
        RaceDataEvidenceField::BodyBounds,
        RaceDataEvidenceField::SkeletonBounds,
        RaceDataEvidenceField::ControllerCapsule,
        RaceDataEvidenceField::GeometryScaleToFo4,
        RaceDataEvidenceField::MaleActorScale,
        RaceDataEvidenceField::FemaleActorScale,
        RaceDataEvidenceField::DefaultWeights,
        RaceDataEvidenceField::MovementArchitecture,
        RaceDataEvidenceField::LinearMotion,
        RaceDataEvidenceField::AngularMotion,
        RaceDataEvidenceField::InjuredHealthPercent,
        RaceDataEvidenceField::BodyBipedObject,
        RaceDataEvidenceField::XpValue,
    ]
}

fn rig_fields() -> Vec<RaceDataEvidenceField> {
    vec![
        RaceDataEvidenceField::SkeletonBounds,
        RaceDataEvidenceField::GeometryScaleToFo4,
    ]
}

fn controller_fields() -> Vec<RaceDataEvidenceField> {
    vec![
        RaceDataEvidenceField::ControllerCapsule,
        RaceDataEvidenceField::MovementArchitecture,
        RaceDataEvidenceField::LinearMotion,
        RaceDataEvidenceField::AngularMotion,
    ]
}

fn creature_fields() -> Vec<RaceDataEvidenceField> {
    vec![
        RaceDataEvidenceField::MaleActorScale,
        RaceDataEvidenceField::FemaleActorScale,
        RaceDataEvidenceField::DefaultWeights,
        RaceDataEvidenceField::InjuredHealthPercent,
        RaceDataEvidenceField::BodyBipedObject,
        RaceDataEvidenceField::XpValue,
    ]
}

fn sort_fields(fields: &mut Vec<RaceDataEvidenceField>) {
    fields.sort();
    fields.dedup();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::{FormKey, SigCode, SubrecordSig};
    use crate::record::FieldEntry;
    use crate::source_rig::race_data::{
        AngularMotionEvidence, ControllerArchitecture, LinearMotionEvidence, MovementArchitecture,
    };
    use crate::source_rig::{Fo4RaceDataTarget, Fo4RaceFlag, Fo4RaceFlag2, RaceDataMapping};
    use crate::translator::pair_hooks::fnv_fo4::creature_catalog::{
        AttackProfile, AttackSet, BodyVariant, CorpusAccounting, GraphReadiness, RecordVariant,
        RigFamily,
    };
    use indexmap::IndexMap;
    use nif_core_native::model::{NifFile, NifValue};
    use smallvec::SmallVec;

    #[derive(Default)]
    struct EvidenceSet {
        selections: Vec<FamilyRaceDataSelection>,
        rigs: Vec<RigRaceDataEvidence>,
        bodies: Vec<BodyRaceDataEvidence>,
        controllers: Vec<ControllerRaceDataEvidence>,
        creatures: Vec<CreatureRaceDataEvidence>,
    }

    impl EvidenceSet {
        fn inputs(&self) -> LegacyRaceDataEvidenceInputs<'_> {
            LegacyRaceDataEvidenceInputs {
                selections: &self.selections,
                rigs: &self.rigs,
                bodies: &self.bodies,
                controllers: &self.controllers,
                creatures: &self.creatures,
            }
        }

        fn reverse(&mut self) {
            self.selections.reverse();
            self.rigs.reverse();
            self.bodies.reverse();
            self.controllers.reverse();
            self.creatures.reverse();
        }
    }

    fn policy() -> Fo4RaceDataScalePolicy {
        Fo4RaceDataScalePolicy {
            small_max_stature: 32.0,
            medium_max_stature: 96.0,
            large_max_stature: 192.0,
        }
    }

    fn stable(local: u32, game: LegacyCreatureGame) -> StableFormKey {
        StableFormKey {
            local,
            plugin: match game {
                LegacyCreatureGame::Fnv => "falloutnv.esm",
                LegacyCreatureGame::Fo3 => "fallout3.esm",
            }
            .to_string(),
        }
    }

    fn provenance(game: LegacyCreatureGame) -> CreatureProvenance {
        CreatureProvenance {
            game,
            source_plugin: match game {
                LegacyCreatureGame::Fnv => "FalloutNV.esm",
                LegacyCreatureGame::Fo3 => "Fallout3.esm",
            }
            .to_string(),
            precedence: 0,
        }
    }

    fn add_family(
        index: u32,
        game: LegacyCreatureGame,
        family_kind: CreatureFamilyKind,
        movement_architecture: MovementArchitecture,
        controller_architecture: ControllerArchitecture,
        stature: f32,
        body_readiness: BodyReadiness,
        catalog: &mut CreatureCorpusPlan,
        evidence: &mut EvidenceSet,
    ) -> (RigFamilyKey, BodyVariantKey, StableFormKey) {
        let rig = RigFamilyKey {
            game,
            skeleton_path: format!("creatures\\family{index}\\skeleton.nif"),
        };
        let body = BodyVariantKey {
            rig: rig.clone(),
            body_paths: vec![format!("creatures\\family{index}\\body.nif")],
            nift: Vec::new(),
            nifz_supported: true,
            nift_supported: true,
        };
        let creature = stable(0x100 + index, game);
        catalog.rig_families.push(RigFamily {
            key: rig.clone(),
            members: vec![creature.clone()],
            direct_kf_paths: Vec::new(),
            recursive_kf_paths: Vec::new(),
            asset_dependencies: vec![rig.skeleton_path.clone()],
            readiness: AssetReadiness::Ready,
        });
        catalog.body_variants.push(BodyVariant {
            key: body.clone(),
            members: vec![creature.clone()],
            asset_dependencies: body.body_paths.clone(),
            readiness: body_readiness,
        });
        catalog.attack_sets.push(AttackSet {
            rig: rig.clone(),
            family_kind,
            profile: AttackProfile::Melee,
            animation_paths: Vec::new(),
            melee_candidates: Vec::new(),
            ranged_candidates: Vec::new(),
            looping_candidates: Vec::new(),
            graph_readiness: match family_kind {
                CreatureFamilyKind::Organic => GraphReadiness::NeedsMeleeGraph,
                CreatureFamilyKind::RobotOrTurret => GraphReadiness::NeedsRangedGraph,
                CreatureFamilyKind::HumanoidWeaponOverlay => {
                    GraphReadiness::NeedsWeaponOverlayGraph
                }
            },
        });
        catalog.records.push(RecordVariant {
            source: creature.clone(),
            editor_id: Some(format!("Creature{index}")),
            provenance: provenance(game),
            record_dependencies: Vec::new(),
            disposition: CreatureDisposition::VisualOwner {
                rig: rig.clone(),
                body: body.clone(),
            },
        });

        evidence.selections.push(FamilyRaceDataSelection {
            rig: rig.clone(),
            source_creature: creature.clone(),
            source_body: body.clone(),
        });
        evidence.rigs.push(RigRaceDataEvidence {
            rig: rig.clone(),
            skeleton_bounds: Some(MeasuredBounds {
                min: [-stature * 0.25, -stature * 0.25, 0.0],
                max: [stature * 0.25, stature * 0.25, stature],
            }),
            up_axis: MeasurementAxis::Z,
            geometry_scale_to_fo4: Some(1.0),
        });
        evidence.bodies.push(BodyRaceDataEvidence {
            body: body.clone(),
            body_bounds: Some(MeasuredBounds {
                min: [-stature * 0.3, -stature * 0.2, 0.0],
                max: [stature * 0.3, stature * 0.2, stature],
            }),
        });
        let mobile = movement_architecture != MovementArchitecture::Stationary;
        evidence.controllers.push(ControllerRaceDataEvidence {
            rig: rig.clone(),
            capsule: Some(CapsuleEvidence {
                radius: stature * 0.15,
                total_height: stature * 0.75,
                architecture: controller_architecture,
            }),
            movement: Some(MovementSemantics {
                architecture: movement_architecture,
                linear: mobile.then_some(LinearMotionEvidence {
                    max_speed: 120.0,
                    seconds_to_full_speed: 0.5,
                    seconds_to_stop: 0.25,
                }),
                angular: Some(AngularMotionEvidence {
                    max_yaw_speed_degrees_per_second: 120.0,
                    seconds_to_full_yaw_speed: 0.5,
                    tolerance_degrees: 15.0,
                    aim_tolerance_degrees: 30.0,
                    pitch_limit_degrees: 45.0,
                    roll_limit_degrees: 20.0,
                }),
                pushable: mobile,
                opens_doors: family_kind == CreatureFamilyKind::HumanoidWeaponOverlay,
                allow_ragdoll_collision: mobile,
                non_hostile: false,
                use_large_actor_pathing: stature > 192.0,
                use_subsegmented_damage: family_kind == CreatureFamilyKind::RobotOrTurret,
            }),
        });
        evidence.creatures.push(CreatureRaceDataEvidence {
            creature: creature.clone(),
            male_actor_scale: Some(1.0),
            female_actor_scale: Some(1.0),
            default_weights: Some(DefaultWeightEvidence {
                male: [0.0, 1.0, 0.0],
                female: [0.0, 1.0, 0.0],
                ungendered: true,
            }),
            injured_health_percent: Some(0.2),
            body_biped_object: Some(0),
            xp_value: Some(10),
        });
        (rig, body, creature)
    }

    fn empty_catalog() -> CreatureCorpusPlan {
        CreatureCorpusPlan {
            records: Vec::new(),
            rig_families: Vec::new(),
            body_variants: Vec::new(),
            attack_sets: Vec::new(),
            accounting: CorpusAccounting::default(),
        }
    }

    fn finish_catalog(catalog: &mut CreatureCorpusPlan) {
        let mut fnv = 0;
        let mut fo3 = 0;
        let mut visual = 0;
        let mut proxies = 0;
        let mut blocked = 0;
        let mut specials = 0;
        for record in &catalog.records {
            match record.provenance.game {
                LegacyCreatureGame::Fnv => fnv += 1,
                LegacyCreatureGame::Fo3 => fo3 += 1,
            }
            match &record.disposition {
                CreatureDisposition::VisualOwner { .. } => visual += 1,
                CreatureDisposition::Proxy { readiness, .. } => {
                    proxies += 1;
                    blocked += usize::from(matches!(readiness, ProxyReadiness::Blocked(_)));
                }
                CreatureDisposition::Special { .. } => specials += 1,
            }
        }
        catalog.accounting = CorpusAccounting {
            input_records: catalog.records.len(),
            winning_records: catalog.records.len(),
            winning_creatures: catalog.records.len(),
            dispositions: catalog.records.len(),
            fnv_creatures: fnv,
            fo3_creatures: fo3,
            visual_owners: visual,
            proxies,
            blocked_proxies: blocked,
            specials,
            rig_families: catalog.rig_families.len(),
            body_variants: catalog.body_variants.len(),
            attack_sets: catalog.attack_sets.len(),
        };
    }

    fn ready_target(entry: &FamilyRaceDataEntry) -> &Fo4RaceDataTarget {
        let FamilyRaceDataStatus::Ready { derivation, .. } = &entry.status else {
            panic!("family was not ready: {:?}", entry.status);
        };
        let RaceDataMapping::Mapped { target } = &derivation.mapping else {
            panic!("ready family did not map");
        };
        target
    }

    #[test]
    fn family_selection_prefers_a_ready_body_variant() {
        let mut catalog = empty_catalog();
        let mut evidence = EvidenceSet::default();
        let (rig, ready_body, _) = add_family(
            0,
            LegacyCreatureGame::Fnv,
            CreatureFamilyKind::Organic,
            MovementArchitecture::Grounded,
            ControllerArchitecture::Quadruped,
            80.0,
            BodyReadiness::Ready,
            &mut catalog,
            &mut evidence,
        );
        let missing_body = BodyVariantKey {
            rig: rig.clone(),
            body_paths: vec!["creatures/family0/missing.nif".to_string()],
            nift: Vec::new(),
            nifz_supported: true,
            nift_supported: true,
        };
        let earlier_source = stable(1, LegacyCreatureGame::Fnv);
        catalog.body_variants.push(BodyVariant {
            key: missing_body.clone(),
            members: vec![earlier_source.clone()],
            asset_dependencies: missing_body.body_paths.clone(),
            readiness: BodyReadiness::MissingAssets(missing_body.body_paths.clone()),
        });
        catalog.records.push(RecordVariant {
            source: earlier_source,
            editor_id: Some("EarlierMissingBody".to_string()),
            provenance: provenance(LegacyCreatureGame::Fnv),
            record_dependencies: Vec::new(),
            disposition: CreatureDisposition::VisualOwner {
                rig,
                body: missing_body,
            },
        });

        let selections = build_family_race_data_selections(&catalog);
        assert_eq!(selections.len(), 1);
        assert_eq!(selections[0].source_body, ready_body);
    }

    fn nif_fields<const N: usize>(values: [(&str, NifValue); N]) -> IndexMap<String, NifValue> {
        values
            .into_iter()
            .map(|(name, value)| (name.to_string(), value))
            .collect()
    }

    fn nif_vertex(position: [f32; 3]) -> NifValue {
        NifValue::Struct(nif_fields([
            ("Vertex", NifValue::Vec3(position)),
            ("Bitangent X", NifValue::Float(0.0)),
            (
                "UV",
                NifValue::Struct(nif_fields([
                    ("u", NifValue::Float(0.0)),
                    ("v", NifValue::Float(0.0)),
                ])),
            ),
            ("Normal", NifValue::Vec3([0.0, 0.0, 1.0])),
            ("Bitangent Y", NifValue::Float(1.0)),
            ("Tangent", NifValue::Vec3([1.0, 0.0, 0.0])),
            ("Bitangent Z", NifValue::Float(0.0)),
        ]))
    }

    fn set_nif_children(nif: &mut NifFile, block_id: usize, children: &[usize]) {
        let block = nif.blocks.get_mut(block_id).unwrap();
        block.set_field("Num Children", NifValue::UInt(children.len() as u64));
        block.set_field(
            "Children",
            NifValue::Array(
                children
                    .iter()
                    .map(|child| NifValue::Ref(*child as i32))
                    .collect(),
            ),
        );
    }

    fn write_bounds_fixtures(mesh_root: &Path) {
        let family_root = mesh_root.join("creatures/family0");
        std::fs::create_dir_all(&family_root).unwrap();

        let mut skeleton = NifFile::new("fo4");
        skeleton.blocks[0].set_field("Name", NifValue::String("Root".to_string()));
        let bone = skeleton.add_block(
            "NiNode",
            Some(nif_fields([("Name", NifValue::String("Bone".to_string()))])),
        );
        skeleton
            .blocks
            .get_mut(bone)
            .unwrap()
            .set_field("Translation", NifValue::Vec3([0.0, 0.0, 80.0]));
        let bound = skeleton.add_block(
            "BSBound",
            Some(nif_fields([
                ("Name", NifValue::String("BBX".to_string())),
                ("Center", NifValue::Vec3([0.0, 0.0, 40.0])),
                ("Dimensions", NifValue::Vec3([20.0, 30.0, 40.0])),
            ])),
        );
        set_nif_children(&mut skeleton, 0, &[bone, bound]);
        skeleton.header.footer_roots = vec![0];
        skeleton
            .save(Some(family_root.join("skeleton.nif")))
            .unwrap();

        let mut body = NifFile::new("fo4");
        body.blocks[0].set_field("Name", NifValue::String("Root".to_string()));
        let shape = body.add_block(
            "BSTriShape",
            Some(nif_fields([
                ("Name", NifValue::String("Body".to_string())),
                ("Vertex Desc", NifValue::Int(193_514_046_685_700)),
                ("Num Vertices", NifValue::UInt(2)),
                ("Num Triangles", NifValue::UInt(0)),
                ("Data Size", NifValue::UInt(0)),
                (
                    "Vertex Data",
                    NifValue::Array(vec![
                        nif_vertex([-10.0, -5.0, 0.0]),
                        nif_vertex([10.0, 5.0, 70.0]),
                    ]),
                ),
            ])),
        );
        set_nif_children(&mut body, 0, &[shape]);
        body.header.footer_roots = vec![0];
        body.save(Some(family_root.join("body.nif"))).unwrap();
    }

    #[test]
    fn loader_uses_render_body_bounds_and_named_node_skeleton_bounds() {
        let mut catalog = empty_catalog();
        let mut evidence = EvidenceSet::default();
        let (rig, body, _) = add_family(
            0,
            LegacyCreatureGame::Fnv,
            CreatureFamilyKind::Organic,
            MovementArchitecture::Grounded,
            ControllerArchitecture::Quadruped,
            80.0,
            BodyReadiness::Ready,
            &mut catalog,
            &mut evidence,
        );
        let mut body_with_accessory = body;
        body_with_accessory
            .body_paths
            .push("creatures\\family0\\accessory.nif".to_string());
        catalog.body_variants[0].key = body_with_accessory.clone();
        catalog.body_variants[0]
            .asset_dependencies
            .push("creatures\\family0\\accessory.nif".to_string());
        let CreatureDisposition::VisualOwner { body, .. } = &mut catalog.records[0].disposition
        else {
            panic!("fixture record must own its visual body");
        };
        *body = body_with_accessory;
        finish_catalog(&mut catalog);
        let temp = tempfile::tempdir().unwrap();
        write_bounds_fixtures(temp.path());
        let mut accessory = NifFile::new("fo4");
        accessory.blocks[0].set_field("Name", NifValue::String("Accessory".to_string()));
        accessory.header.footer_roots = vec![0];
        accessory
            .save(Some(temp.path().join("creatures/family0/accessory.nif")))
            .unwrap();

        let loaded = load_nif_race_data_evidence(
            &catalog,
            temp.path(),
            &[RigNifMeasurementContext {
                rig,
                up_axis: MeasurementAxis::Z,
                geometry_scale_to_fo4: Some(1.0),
            }],
        );
        assert!(loaded.issues.is_empty(), "{:?}", loaded.issues);
        assert_eq!(loaded.rigs.len(), 1);
        assert_eq!(loaded.bodies.len(), 1);
        assert_eq!(
            loaded.rigs[0].skeleton_bounds.as_ref().unwrap().max[2],
            80.0
        );
        assert_eq!(loaded.bodies[0].body_bounds.as_ref().unwrap().min[0], -10.0);
        assert_eq!(loaded.bodies[0].body_bounds.as_ref().unwrap().max[2], 70.0);
        assert_eq!(loaded.remaining_requirements.len(), 2);
        assert!(
            loaded
                .remaining_requirements
                .iter()
                .any(|requirement| matches!(
                    requirement,
                    ProductionRaceDataRequirement::ControllerMeasurements { .. }
                ))
        );
        assert!(
            loaded
                .remaining_requirements
                .iter()
                .any(|requirement| matches!(
                    requirement,
                    ProductionRaceDataRequirement::CreatureSemantics { .. }
                ))
        );
    }

    #[test]
    fn real_fnv_particle_accessories_are_separate_from_render_bounds() {
        let Some(repo_root) = Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .find(|path| path.join("extracted/fnv").is_dir())
        else {
            return;
        };
        let mesh_root = repo_root.join("extracted/fnv/meshes");
        for relative in [
            "creatures/nvsecuritron/nvsecuritron.nif",
            "creatures/nvsecuritron/nvsecuritronscreenstatic.nif",
            "creatures/nvsecuritron/nvsecuritronvoicebox.nif",
            "nvdlc01/creatures/ghosts/nvdlc01ghost.nif",
            "nvdlc01/creatures/ghosts/nvdlc01glovel.nif",
        ] {
            assert!(
                aggregate_render_world_bounds(mesh_root.join(relative)).is_ok(),
                "render body must provide measurable bounds: {relative}"
            );
        }
        for relative in [
            "creatures/nvsecuritron/nvsecuritronsmoketrail.nif",
            "nvdlc01/creatures/ghosts/nvdlc01_ghostbreathe.nif",
        ] {
            let error = aggregate_render_world_bounds(mesh_root.join(relative)).unwrap_err();
            assert!(
                non_bounds_body_accessory(&error),
                "particle accessory must be excluded from render bounds: {relative}: {error}"
            );
        }
    }

    #[test]
    fn controller_loader_uses_source_bsbound_and_explicit_motion_semantics() {
        let mut catalog = empty_catalog();
        let mut evidence = EvidenceSet::default();
        let (rig, _, _) = add_family(
            0,
            LegacyCreatureGame::Fnv,
            CreatureFamilyKind::Organic,
            MovementArchitecture::Grounded,
            ControllerArchitecture::Quadruped,
            80.0,
            BodyReadiness::Ready,
            &mut catalog,
            &mut evidence,
        );
        finish_catalog(&mut catalog);
        let temp = tempfile::tempdir().unwrap();
        write_bounds_fixtures(temp.path());
        let loaded = load_nif_race_data_evidence_with_controllers(
            &catalog,
            temp.path(),
            &[RigNifMeasurementContext {
                rig: rig.clone(),
                up_axis: MeasurementAxis::Z,
                geometry_scale_to_fo4: Some(1.0),
            }],
            &[RigControllerRaceDataContext {
                rig,
                architecture: ControllerArchitecture::Quadruped,
                movement: evidence.controllers[0].movement.clone(),
            }],
        );
        assert!(loaded.issues.is_empty(), "{:?}", loaded.issues);
        assert_eq!(loaded.controllers.len(), 1);
        let capsule = loaded.controllers[0].capsule.as_ref().unwrap();
        assert_eq!(capsule.radius, 20.0);
        assert_eq!(capsule.total_height, 80.0);
        assert_eq!(capsule.architecture, ControllerArchitecture::Quadruped);
        assert!(
            loaded
                .remaining_requirements
                .iter()
                .all(|requirement| !matches!(
                    requirement,
                    ProductionRaceDataRequirement::ControllerMeasurements { .. }
                ))
        );
    }

    fn source_creature_record(
        source: &StableFormKey,
        scale: Option<f32>,
        level: Option<u64>,
        interner: &StringInterner,
    ) -> Record {
        let mut record = Record::new(
            SigCode::from_str("CREA").unwrap(),
            FormKey {
                local: source.local,
                plugin: interner.intern(&source.plugin),
            },
        );
        let mut fields = Vec::new();
        if let Some(scale) = scale {
            fields.push(FieldEntry {
                sig: SubrecordSig::from_str("BNAM").unwrap(),
                value: FieldValue::Struct(vec![(
                    interner.intern("float32_0"),
                    FieldValue::Float(scale),
                )]),
            });
        }
        if let Some(level) = level {
            fields.push(FieldEntry {
                sig: SubrecordSig::from_str("ACBS").unwrap(),
                value: FieldValue::Struct(vec![(
                    interner.intern("level"),
                    FieldValue::Uint(level),
                )]),
            });
        }
        record.fields = SmallVec::from_vec(fields);
        record
    }

    fn movement_source_creature_record(
        source: &StableFormKey,
        speed_multiplier: u64,
        turning_speed: f32,
        interner: &StringInterner,
    ) -> Record {
        let mut record = source_creature_record(source, Some(1.0), Some(1), interner);
        let FieldValue::Struct(acbs) = &mut record
            .fields
            .iter_mut()
            .find(|field| field.sig.as_str() == "ACBS")
            .unwrap()
            .value
        else {
            panic!("ACBS fixture must be structured");
        };
        acbs.push((
            interner.intern("speed_multiplier"),
            FieldValue::Uint(speed_multiplier),
        ));
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("TNAM").unwrap(),
            value: FieldValue::Float(turning_speed),
        });
        record
    }

    fn parsed_motion(
        source_kf: &str,
        stop_time: f64,
        root_motion: RootMotionEvidence,
    ) -> crate::translator::pair_hooks::fnv_fo4::creature_motion::CreatureKfEvidence {
        use crate::translator::pair_hooks::fnv_fo4::creature_motion::{
            BindingEvidence, CreatureKfEvidence, NiControllerSequenceEvidence, SequenceCycle,
        };

        CreatureKfEvidence {
            source_game: LegacyCreatureGame::Fnv,
            source_kf: source_kf.to_string(),
            sequence: KfParseEvidence::Parsed(NiControllerSequenceEvidence {
                sequence_index: 0,
                name: source_kf.to_string(),
                cycle: SequenceCycle::Loop,
                start_time: 0.0,
                stop_time,
                frequency: 1.0,
                text_keys: Vec::new(),
                binding: BindingEvidence {
                    source_skeleton_path: "creatures\\family0\\skeleton.nif".to_string(),
                    transform_track_count: 1,
                    float_track_count: 0,
                    controller_types: Vec::new(),
                    interpolator_types: Vec::new(),
                    target_names: Vec::new(),
                    required_float_slots: Vec::new(),
                    compatibility: BindingCompatibility::Verified,
                    compatibility_detail: None,
                },
                root_motion,
            }),
            idle_claims: Vec::new(),
        }
    }

    fn movement_family(
        rig: &RigFamilyKey,
        clips: Vec<crate::translator::pair_hooks::fnv_fo4::creature_motion::CreatureKfEvidence>,
    ) -> CreatureMotionFamilyEvidence {
        CreatureMotionFamilyEvidence {
            rig: rig.clone(),
            referenced_kfs: clips.iter().map(|clip| clip.source_kf.clone()).collect(),
            kf_evidence: clips,
            creature_traits: Vec::new(),
        }
    }

    fn base_settings() -> LegacyMovementBaseSettings {
        LegacyMovementBaseSettings {
            game: LegacyCreatureGame::Fnv,
            f_move_base_speed_source: StableFormKey {
                plugin: "falloutnv.esm".to_string(),
                local: 0x014B91,
            },
            locomotion_multiplier_source: StableFormKey {
                plugin: "falloutnv.esm".to_string(),
                local: 0x014E37,
            },
            f_move_base_speed: 80.0,
            locomotion_multiplier: 1.5,
        }
    }

    fn gmst_record(local: u32, editor_id: &str, value: f32, interner: &StringInterner) -> Record {
        let mut record = Record::new(
            SigCode::from_str("GMST").unwrap(),
            FormKey {
                local,
                plugin: interner.intern("FalloutNV.esm"),
            },
        );
        record.eid = Some(interner.intern(editor_id));
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("DATA").unwrap(),
            value: FieldValue::Bytes(value.to_le_bytes().as_slice().into()),
        });
        record
    }

    #[test]
    fn movement_base_settings_loader_resolves_exact_winning_gmsts() {
        let mut catalog = empty_catalog();
        let mut evidence = EvidenceSet::default();
        add_family(
            0,
            LegacyCreatureGame::Fnv,
            CreatureFamilyKind::Organic,
            MovementArchitecture::Grounded,
            ControllerArchitecture::Quadruped,
            80.0,
            BodyReadiness::Ready,
            &mut catalog,
            &mut evidence,
        );
        finish_catalog(&mut catalog);
        let interner = StringInterner::new();
        let base = gmst_record(0x014B91, "fMoveBaseSpeed", 77.0, &interner);
        let run = gmst_record(0x014E37, "fMoveRunMult", 4.0, &interner);
        let sources = [
            LegacyRecordSource {
                record: &base,
                provenance: provenance(LegacyCreatureGame::Fnv),
            },
            LegacyRecordSource {
                record: &run,
                provenance: provenance(LegacyCreatureGame::Fnv),
            },
        ];
        let loaded = load_legacy_movement_base_settings(&catalog, &sources, &interner);
        assert!(loaded.issues.is_empty(), "{:?}", loaded.issues);
        assert_eq!(loaded.settings.len(), 1);
        assert_eq!(loaded.settings[0].f_move_base_speed, 77.0);
        assert_eq!(loaded.settings[0].locomotion_multiplier, 4.0);
        assert_eq!(
            loaded.settings[0].f_move_base_speed_source,
            StableFormKey {
                local: 0x014B91,
                plugin: "falloutnv.esm".to_string(),
            }
        );
        assert_eq!(
            loaded.settings[0].locomotion_multiplier_source,
            StableFormKey {
                local: 0x014E37,
                plugin: "falloutnv.esm".to_string(),
            }
        );
    }

    #[test]
    fn packed_crea_acbs_u16_fields_are_decoded() {
        let interner = StringInterner::new();
        let source = stable(0x123, LegacyCreatureGame::Fnv);
        let mut record = Record::new(
            SigCode::from_str("CREA").unwrap(),
            FormKey {
                local: source.local,
                plugin: interner.intern(&source.plugin),
            },
        );
        let mut acbs = vec![0_u8; 24];
        acbs[8..10].copy_from_slice(&23_u16.to_le_bytes());
        acbs[14..16].copy_from_slice(&125_u16.to_le_bytes());
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("ACBS").unwrap(),
            value: FieldValue::Bytes(acbs.into()),
        });

        assert_eq!(struct_u16(&record, "ACBS", "level", &interner), Some(23));
        assert_eq!(
            struct_u16(&record, "ACBS", "speed_multiplier", &interner),
            Some(125)
        );
    }

    #[test]
    fn crea_loader_distinguishes_schema_absent_fallbacks_from_missing_evidence() {
        let mut catalog = empty_catalog();
        let mut evidence = EvidenceSet::default();
        let (_, _, creature) = add_family(
            0,
            LegacyCreatureGame::Fnv,
            CreatureFamilyKind::Organic,
            MovementArchitecture::Grounded,
            ControllerArchitecture::Quadruped,
            80.0,
            BodyReadiness::Ready,
            &mut catalog,
            &mut evidence,
        );
        finish_catalog(&mut catalog);
        let interner = StringInterner::new();
        let selections = build_family_race_data_selections(&catalog);
        let complete = source_creature_record(&creature, Some(1.25), Some(7), &interner);
        let loaded =
            load_creature_record_race_data_evidence(&catalog, &selections, &[complete], &interner);
        assert!(loaded.issues.is_empty());
        assert_eq!(loaded.creatures.len(), 1);
        assert_eq!(loaded.creatures[0].male_actor_scale, Some(1.25));
        assert_eq!(loaded.creatures[0].female_actor_scale, Some(1.25));
        assert_eq!(loaded.creatures[0].injured_health_percent, Some(0.0));
        assert_eq!(loaded.creatures[0].body_biped_object, Some(-1));
        assert_eq!(loaded.creatures[0].xp_value, Some(7));
        assert_eq!(loaded.fidelity_receipts.len(), 4);
        assert!(loaded.fidelity_receipts.iter().any(|receipt| matches!(
            receipt,
            RaceDataFidelityReceipt::LevelAsXpMvp {
                source_level: 7,
                xp_value: 7,
                ..
            }
        )));
        assert!(
            loaded
                .remaining_requirements
                .iter()
                .all(|requirement| !matches!(
                    requirement,
                    ProductionRaceDataRequirement::CreatureSemantics { .. }
                ))
        );

        let incomplete = source_creature_record(&creature, None, Some(7), &interner);
        let missing = load_creature_record_race_data_evidence(
            &catalog,
            &selections,
            &[incomplete],
            &interner,
        );
        assert!(missing.issues.iter().any(|issue| matches!(
            issue,
            CreatureRecordEvidenceIssue::MissingEvidence {
                field: RaceDataEvidenceField::MaleActorScale,
                ..
            }
        )));
        assert!(missing.fidelity_receipts.iter().all(|receipt| !matches!(
            receipt,
            RaceDataFidelityReceipt::SchemaAbsentDefaulted {
                field: RaceDataEvidenceField::MaleActorScale,
                ..
            }
        )));
    }

    #[test]
    fn movement_mvp_prefers_measured_root_cycles_and_receipts_defaults() {
        let mut catalog = empty_catalog();
        let mut evidence = EvidenceSet::default();
        let (rig, _, creature) = add_family(
            0,
            LegacyCreatureGame::Fnv,
            CreatureFamilyKind::Organic,
            MovementArchitecture::Grounded,
            ControllerArchitecture::Quadruped,
            80.0,
            BodyReadiness::Ready,
            &mut catalog,
            &mut evidence,
        );
        finish_catalog(&mut catalog);
        let interner = StringInterner::new();
        let record = movement_source_creature_record(&creature, 125, 200.0, &interner);
        let locomotion = "creatures\\family0\\walkforward.kf";
        let turn = "creatures\\family0\\turnleft.kf";
        let motion = movement_family(
            &rig,
            vec![
                parsed_motion(
                    locomotion,
                    2.0,
                    RootMotionEvidence::Planar {
                        accum_root: "Bip01 NonAccum".to_string(),
                        distance: 120.0,
                        yaw_radians: 0.0,
                    },
                ),
                parsed_motion(
                    turn,
                    0.5,
                    RootMotionEvidence::Planar {
                        accum_root: "Bip01 NonAccum".to_string(),
                        distance: 0.0,
                        yaw_radians: std::f64::consts::FRAC_PI_2,
                    },
                ),
            ],
        );
        let loaded = build_legacy_movement_mvp_contexts(
            &catalog,
            &build_family_race_data_selections(&catalog),
            &[LegacyMovementGraphSelection {
                rig: rig.clone(),
                graph_template: CreatureGraphTemplate::GroundMelee,
                locomotion_source_kf: Some(locomotion.to_string()),
                turn_source_kf: Some(turn.to_string()),
                stationary_cycle_source_kf: None,
                uses_controller_speed: false,
                aiming_capable: false,
                aim_tolerance_degrees: None,
                orientation_pitch_degrees: None,
                orientation_roll_degrees: None,
            }],
            &[base_settings()],
            &[motion],
            &[record],
            &interner,
        );
        assert!(loaded.issues.is_empty(), "{:?}", loaded.issues);
        assert_eq!(loaded.controllers.len(), 1);
        assert_eq!(
            loaded.controllers[0].architecture,
            ControllerArchitecture::Quadruped
        );
        let movement = loaded.controllers[0].movement.as_ref().unwrap();
        let linear = movement.linear.as_ref().unwrap();
        assert_eq!(linear.max_speed, 60.0);
        assert_eq!(linear.seconds_to_full_speed, 2.0);
        assert_eq!(linear.seconds_to_stop, 2.0);
        let angular = movement.angular.as_ref().unwrap();
        assert_eq!(angular.max_yaw_speed_degrees_per_second, 180.0);
        assert_eq!(angular.seconds_to_full_yaw_speed, 0.5);
        assert_eq!(angular.aim_tolerance_degrees, 0.0);
        assert_eq!(angular.pitch_limit_degrees, 0.0);
        assert_eq!(angular.roll_limit_degrees, 0.0);
        assert_eq!(loaded.fidelity_receipts.len(), 3);
    }

    #[test]
    fn movement_mvp_uses_resolved_base_settings_for_in_place_controller_motion() {
        let mut catalog = empty_catalog();
        let mut evidence = EvidenceSet::default();
        let (rig, _, creature) = add_family(
            0,
            LegacyCreatureGame::Fnv,
            CreatureFamilyKind::Organic,
            MovementArchitecture::Grounded,
            ControllerArchitecture::Quadruped,
            80.0,
            BodyReadiness::Ready,
            &mut catalog,
            &mut evidence,
        );
        finish_catalog(&mut catalog);
        let interner = StringInterner::new();
        let record = movement_source_creature_record(&creature, 125, 200.0, &interner);
        let locomotion = "creatures\\family0\\walkforward.kf";
        let turn = "creatures\\family0\\turnleft.kf";
        let motion = movement_family(
            &rig,
            vec![
                parsed_motion(
                    locomotion,
                    0.75,
                    RootMotionEvidence::Stationary { accum_root: None },
                ),
                parsed_motion(
                    turn,
                    0.25,
                    RootMotionEvidence::Stationary { accum_root: None },
                ),
            ],
        );
        let loaded = build_legacy_movement_mvp_contexts(
            &catalog,
            &build_family_race_data_selections(&catalog),
            &[LegacyMovementGraphSelection {
                rig,
                graph_template: CreatureGraphTemplate::GroundMelee,
                locomotion_source_kf: Some(locomotion.to_string()),
                turn_source_kf: Some(turn.to_string()),
                stationary_cycle_source_kf: None,
                uses_controller_speed: true,
                aiming_capable: false,
                aim_tolerance_degrees: None,
                orientation_pitch_degrees: None,
                orientation_roll_degrees: None,
            }],
            &[base_settings()],
            &[motion],
            &[record],
            &interner,
        );
        assert!(loaded.issues.is_empty(), "{:?}", loaded.issues);
        let movement = loaded.controllers[0].movement.as_ref().unwrap();
        assert_eq!(movement.linear.as_ref().unwrap().max_speed, 150.0);
        assert_eq!(
            movement
                .angular
                .as_ref()
                .unwrap()
                .max_yaw_speed_degrees_per_second,
            200.0
        );
    }

    #[test]
    fn movement_mvp_rejects_missing_roles_and_non_ground_orientation() {
        let mut catalog = empty_catalog();
        let mut evidence = EvidenceSet::default();
        let (rig, _, creature) = add_family(
            0,
            LegacyCreatureGame::Fnv,
            CreatureFamilyKind::Organic,
            MovementArchitecture::Swimming,
            ControllerArchitecture::Standard,
            80.0,
            BodyReadiness::Ready,
            &mut catalog,
            &mut evidence,
        );
        finish_catalog(&mut catalog);
        let interner = StringInterner::new();
        let record = movement_source_creature_record(&creature, 100, 90.0, &interner);
        let locomotion = "creatures\\family0\\swimforward.kf";
        let motion = movement_family(
            &rig,
            vec![parsed_motion(
                locomotion,
                1.0,
                RootMotionEvidence::Planar {
                    accum_root: "Bip01 NonAccum".to_string(),
                    distance: 50.0,
                    yaw_radians: 0.0,
                },
            )],
        );
        let loaded = build_legacy_movement_mvp_contexts(
            &catalog,
            &build_family_race_data_selections(&catalog),
            &[LegacyMovementGraphSelection {
                rig,
                graph_template: CreatureGraphTemplate::GroundMelee,
                locomotion_source_kf: Some(locomotion.to_string()),
                turn_source_kf: None,
                stationary_cycle_source_kf: None,
                uses_controller_speed: false,
                aiming_capable: false,
                aim_tolerance_degrees: None,
                orientation_pitch_degrees: None,
                orientation_roll_degrees: None,
            }],
            &[base_settings()],
            &[motion],
            &[record],
            &interner,
        );
        assert!(loaded.issues.is_empty(), "{:?}", loaded.issues);
        let angular = loaded.controllers[0]
            .movement
            .as_ref()
            .unwrap()
            .angular
            .as_ref()
            .unwrap();
        assert_eq!(angular.max_yaw_speed_degrees_per_second, 90.0);
        assert_eq!(angular.seconds_to_full_yaw_speed, 1.0);

        let turn = "creatures\\family0\\swimturn.kf";
        let complete_motion = movement_family(
            &catalog.rig_families[0].key,
            vec![
                parsed_motion(
                    locomotion,
                    1.0,
                    RootMotionEvidence::Planar {
                        accum_root: "Bip01 NonAccum".to_string(),
                        distance: 50.0,
                        yaw_radians: 0.0,
                    },
                ),
                parsed_motion(
                    turn,
                    0.5,
                    RootMotionEvidence::Planar {
                        accum_root: "Bip01 NonAccum".to_string(),
                        distance: 0.0,
                        yaw_radians: std::f64::consts::FRAC_PI_2,
                    },
                ),
            ],
        );
        let missing_orientation = build_legacy_movement_mvp_contexts(
            &catalog,
            &build_family_race_data_selections(&catalog),
            &[LegacyMovementGraphSelection {
                rig: catalog.rig_families[0].key.clone(),
                graph_template: CreatureGraphTemplate::Swim,
                locomotion_source_kf: Some(locomotion.to_string()),
                turn_source_kf: Some(turn.to_string()),
                stationary_cycle_source_kf: None,
                uses_controller_speed: false,
                aiming_capable: false,
                aim_tolerance_degrees: None,
                orientation_pitch_degrees: None,
                orientation_roll_degrees: None,
            }],
            &[base_settings()],
            &[complete_motion],
            &[movement_source_creature_record(
                &creature, 100, 90.0, &interner,
            )],
            &interner,
        );
        assert!(missing_orientation.controllers.is_empty());
        assert!(
            missing_orientation
                .issues
                .iter()
                .any(|issue| matches!(issue, LegacyMovementMvpIssue::MissingOrientationLimits(_)))
        );
    }

    #[test]
    fn movement_mvp_stationary_template_emits_explicit_zero_motion() {
        let mut catalog = empty_catalog();
        let mut evidence = EvidenceSet::default();
        let (rig, _, creature) = add_family(
            0,
            LegacyCreatureGame::Fnv,
            CreatureFamilyKind::RobotOrTurret,
            MovementArchitecture::Stationary,
            ControllerArchitecture::Fixed,
            80.0,
            BodyReadiness::Ready,
            &mut catalog,
            &mut evidence,
        );
        finish_catalog(&mut catalog);
        let interner = StringInterner::new();
        let idle = "creatures\\family0\\idle.kf";
        let motion = movement_family(
            &rig,
            vec![parsed_motion(
                idle,
                1.25,
                RootMotionEvidence::Stationary { accum_root: None },
            )],
        );
        let loaded = build_legacy_movement_mvp_contexts(
            &catalog,
            &build_family_race_data_selections(&catalog),
            &[LegacyMovementGraphSelection {
                rig,
                graph_template: CreatureGraphTemplate::StationaryTurret,
                locomotion_source_kf: None,
                turn_source_kf: None,
                stationary_cycle_source_kf: Some(idle.to_string()),
                uses_controller_speed: false,
                aiming_capable: false,
                aim_tolerance_degrees: None,
                orientation_pitch_degrees: None,
                orientation_roll_degrees: None,
            }],
            &[base_settings()],
            &[motion],
            &[movement_source_creature_record(
                &creature, 100, 0.0, &interner,
            )],
            &interner,
        );
        assert!(loaded.issues.is_empty(), "{:?}", loaded.issues);
        assert_eq!(
            loaded.controllers[0].architecture,
            ControllerArchitecture::Fixed
        );
        let movement = loaded.controllers[0].movement.as_ref().unwrap();
        assert_eq!(movement.architecture, MovementArchitecture::Stationary);
        assert!(movement.linear.is_none());
        assert_eq!(
            movement
                .angular
                .as_ref()
                .unwrap()
                .max_yaw_speed_degrees_per_second,
            0.0
        );
    }

    #[test]
    fn covers_requested_family_classes_proxy_and_missing_body_without_donors() {
        let mut catalog = empty_catalog();
        let mut evidence = EvidenceSet::default();
        let (quadruped_rig, _, quadruped) = add_family(
            0,
            LegacyCreatureGame::Fnv,
            CreatureFamilyKind::Organic,
            MovementArchitecture::Grounded,
            ControllerArchitecture::Quadruped,
            80.0,
            BodyReadiness::Ready,
            &mut catalog,
            &mut evidence,
        );
        add_family(
            1,
            LegacyCreatureGame::Fnv,
            CreatureFamilyKind::Organic,
            MovementArchitecture::Grounded,
            ControllerArchitecture::Quadruped,
            12.0,
            BodyReadiness::Ready,
            &mut catalog,
            &mut evidence,
        );
        add_family(
            2,
            LegacyCreatureGame::Fnv,
            CreatureFamilyKind::Organic,
            MovementArchitecture::Flying,
            ControllerArchitecture::Standard,
            120.0,
            BodyReadiness::Ready,
            &mut catalog,
            &mut evidence,
        );
        add_family(
            3,
            LegacyCreatureGame::Fnv,
            CreatureFamilyKind::Organic,
            MovementArchitecture::Swimming,
            ControllerArchitecture::Standard,
            60.0,
            BodyReadiness::Ready,
            &mut catalog,
            &mut evidence,
        );
        add_family(
            4,
            LegacyCreatureGame::Fnv,
            CreatureFamilyKind::RobotOrTurret,
            MovementArchitecture::Stationary,
            ControllerArchitecture::Fixed,
            140.0,
            BodyReadiness::Ready,
            &mut catalog,
            &mut evidence,
        );
        add_family(
            5,
            LegacyCreatureGame::Fnv,
            CreatureFamilyKind::HumanoidWeaponOverlay,
            MovementArchitecture::Grounded,
            ControllerArchitecture::Standard,
            72.0,
            BodyReadiness::Ready,
            &mut catalog,
            &mut evidence,
        );
        add_family(
            6,
            LegacyCreatureGame::Fnv,
            CreatureFamilyKind::Organic,
            MovementArchitecture::Grounded,
            ControllerArchitecture::Quadruped,
            64.0,
            BodyReadiness::MissingAssets(vec!["missing-body.nif".to_string()]),
            &mut catalog,
            &mut evidence,
        );
        catalog.records.push(RecordVariant {
            source: stable(0x900, LegacyCreatureGame::Fnv),
            editor_id: Some("Proxy".to_string()),
            provenance: provenance(LegacyCreatureGame::Fnv),
            record_dependencies: vec![quadruped.clone()],
            disposition: CreatureDisposition::Proxy {
                terminal_visual_owners: vec![quadruped],
                readiness: ProxyReadiness::Ready,
            },
        });
        catalog.records.push(RecordVariant {
            source: stable(0x901, LegacyCreatureGame::Fnv),
            editor_id: Some("Special".to_string()),
            provenance: provenance(LegacyCreatureGame::Fnv),
            record_dependencies: Vec::new(),
            disposition: CreatureDisposition::Special {
                reason: SpecialCreatureReason::NoVisualTemplate,
            },
        });
        finish_catalog(&mut catalog);

        let plan = build_race_data_adapter(&catalog, evidence.inputs(), &policy()).unwrap();
        assert_eq!(plan.accounting.catalog_records, 9);
        assert_eq!(plan.accounting.record_dispositions, 9);
        assert_eq!(plan.accounting.ready_families, 6);
        assert_eq!(plan.accounting.invalid_families, 1);
        assert_eq!(plan.accounting.visual_owner_records, 7);
        assert_eq!(plan.accounting.proxy_records, 1);
        assert_eq!(plan.accounting.special_records, 1);
        assert_eq!(plan.accounting.records_with_ready_family, 7);
        assert_eq!(plan.accounting.records_with_invalid_family, 1);

        let quadruped = plan
            .families
            .iter()
            .find(|family| family.rig == quadruped_rig)
            .unwrap();
        assert!(
            ready_target(quadruped)
                .flags_2
                .contains(&Fo4RaceFlag2::UseQuadrupedController)
        );
        let flyer = plan
            .families
            .iter()
            .find(|family| {
                matches!(
                    &family.status,
                    FamilyRaceDataStatus::Ready { evidence, .. }
                        if evidence.movement.as_ref().is_some_and(|movement| movement.architecture == MovementArchitecture::Flying)
                )
            })
            .unwrap();
        assert!(ready_target(flyer).flags.contains(&Fo4RaceFlag::Flies));
        let swimmer = plan
            .families
            .iter()
            .find(|family| {
                matches!(
                    &family.status,
                    FamilyRaceDataStatus::Ready { evidence, .. }
                        if evidence.movement.as_ref().is_some_and(|movement| movement.architecture == MovementArchitecture::Swimming)
                )
            })
            .unwrap();
        assert!(ready_target(swimmer).flags.contains(&Fo4RaceFlag::Swims));
        let turret = plan
            .families
            .iter()
            .find(|family| family.family_kind == CreatureFamilyKind::RobotOrTurret)
            .unwrap();
        assert!(ready_target(turret).flags.contains(&Fo4RaceFlag::Immobile));
        assert!(matches!(
            plan.families
                .iter()
                .find(|family| family.rig.skeleton_path.contains("family6"))
                .unwrap()
                .status,
            FamilyRaceDataStatus::Invalid { ref fields, .. }
                if fields == &[RaceDataEvidenceField::BodyBounds]
        ));
        assert!(matches!(
            plan.records
                .iter()
                .find(|record| record.source.local == 0x900)
                .unwrap()
                .disposition,
            RecordRaceDataDisposition::Proxy { .. }
        ));
        assert_eq!(production_evidence_requirements(&catalog).len(), 28);
    }

    #[test]
    fn returns_exact_ordered_missing_and_invalid_fields() {
        let mut catalog = empty_catalog();
        let mut evidence = EvidenceSet::default();
        add_family(
            0,
            LegacyCreatureGame::Fnv,
            CreatureFamilyKind::Organic,
            MovementArchitecture::Grounded,
            ControllerArchitecture::Quadruped,
            80.0,
            BodyReadiness::Ready,
            &mut catalog,
            &mut evidence,
        );
        finish_catalog(&mut catalog);
        evidence.bodies.clear();
        evidence.controllers.clear();
        let plan = build_race_data_adapter(&catalog, evidence.inputs(), &policy()).unwrap();
        assert!(matches!(
            &plan.families[0].status,
            FamilyRaceDataStatus::Missing { fields, issues }
                if fields == &[
                    RaceDataEvidenceField::BodyBounds,
                    RaceDataEvidenceField::ControllerCapsule,
                    RaceDataEvidenceField::MovementArchitecture,
                    RaceDataEvidenceField::LinearMotion,
                    RaceDataEvidenceField::AngularMotion,
                ] && issues == &[
                    FamilyRaceDataIssue::MissingBodyEvidence,
                    FamilyRaceDataIssue::MissingControllerEvidence,
                ]
        ));

        let mut complete = EvidenceSet::default();
        let mut second_catalog = empty_catalog();
        add_family(
            0,
            LegacyCreatureGame::Fnv,
            CreatureFamilyKind::Organic,
            MovementArchitecture::Grounded,
            ControllerArchitecture::Quadruped,
            80.0,
            BodyReadiness::Ready,
            &mut second_catalog,
            &mut complete,
        );
        finish_catalog(&mut second_catalog);
        complete.controllers[0].capsule.as_mut().unwrap().radius = -1.0;
        let invalid =
            build_race_data_adapter(&second_catalog, complete.inputs(), &policy()).unwrap();
        assert!(matches!(
            &invalid.families[0].status,
            FamilyRaceDataStatus::Invalid { fields, issues }
                if fields == &[RaceDataEvidenceField::ControllerCapsule]
                    && matches!(&issues[..], [FamilyRaceDataIssue::InvalidEvidence {
                        field: RaceDataEvidenceField::ControllerCapsule,
                        ..
                    }])
        ));
    }

    #[test]
    fn same_paths_preserve_fnv_and_grafted_fo3_provenance() {
        let mut catalog = empty_catalog();
        let mut evidence = EvidenceSet::default();
        let (fnv_rig, _, _) = add_family(
            0,
            LegacyCreatureGame::Fnv,
            CreatureFamilyKind::Organic,
            MovementArchitecture::Grounded,
            ControllerArchitecture::Quadruped,
            80.0,
            BodyReadiness::Ready,
            &mut catalog,
            &mut evidence,
        );
        let (fo3_rig, _, _) = add_family(
            1,
            LegacyCreatureGame::Fo3,
            CreatureFamilyKind::Organic,
            MovementArchitecture::Grounded,
            ControllerArchitecture::Quadruped,
            80.0,
            BodyReadiness::Ready,
            &mut catalog,
            &mut evidence,
        );
        catalog.rig_families[1].key.skeleton_path = fnv_rig.skeleton_path.clone();
        catalog.attack_sets[1].rig.skeleton_path = fnv_rig.skeleton_path.clone();
        catalog.body_variants[1].key.rig.skeleton_path = fnv_rig.skeleton_path.clone();
        let fo3_body = catalog.body_variants[1].key.clone();
        if let CreatureDisposition::VisualOwner { rig, body } = &mut catalog.records[1].disposition
        {
            rig.skeleton_path = fnv_rig.skeleton_path.clone();
            *body = fo3_body.clone();
        }
        evidence.selections[1].rig.skeleton_path = fnv_rig.skeleton_path.clone();
        evidence.selections[1].source_body = fo3_body.clone();
        evidence.rigs[1].rig.skeleton_path = fnv_rig.skeleton_path.clone();
        evidence.controllers[1].rig.skeleton_path = fnv_rig.skeleton_path.clone();
        evidence.bodies[1].body = fo3_body;
        finish_catalog(&mut catalog);

        let plan = build_race_data_adapter(&catalog, evidence.inputs(), &policy()).unwrap();
        assert_eq!(plan.accounting.fnv_records, 1);
        assert_eq!(plan.accounting.fo3_records, 1);
        assert_eq!(plan.accounting.ready_families, 2);
        assert_eq!(
            plan.families[0].rig.skeleton_path,
            plan.families[1].rig.skeleton_path
        );
        assert_ne!(plan.families[0].provenance, plan.families[1].provenance);
        assert_eq!(fo3_rig.game, LegacyCreatureGame::Fo3);
    }

    #[test]
    fn full_merged_wrapper_accounts_for_every_expected_winner() {
        let mut catalog = empty_catalog();
        let mut evidence = EvidenceSet::default();
        add_family(
            0,
            LegacyCreatureGame::Fnv,
            CreatureFamilyKind::Organic,
            MovementArchitecture::Grounded,
            ControllerArchitecture::Quadruped,
            80.0,
            BodyReadiness::Ready,
            &mut catalog,
            &mut evidence,
        );
        for index in 0..(EXPECTED_FULL_MERGED_CREA_WINNERS - 1) {
            let game = if index % 2 == 0 {
                LegacyCreatureGame::Fnv
            } else {
                LegacyCreatureGame::Fo3
            };
            catalog.records.push(RecordVariant {
                source: stable(0x1000 + index as u32, game),
                editor_id: None,
                provenance: provenance(game),
                record_dependencies: Vec::new(),
                disposition: CreatureDisposition::Special {
                    reason: SpecialCreatureReason::NoVisualTemplate,
                },
            });
        }
        finish_catalog(&mut catalog);

        let plan =
            build_full_merged_race_data_adapter(&catalog, evidence.inputs(), &policy()).unwrap();
        assert_eq!(
            plan.accounting.record_dispositions,
            EXPECTED_FULL_MERGED_CREA_WINNERS
        );
        assert_eq!(plan.accounting.visual_owner_records, 1);
        assert_eq!(
            plan.accounting.special_records,
            EXPECTED_FULL_MERGED_CREA_WINNERS - 1
        );
        assert_eq!(
            plan.accounting.fnv_records + plan.accounting.fo3_records,
            EXPECTED_FULL_MERGED_CREA_WINNERS
        );
    }

    #[test]
    fn output_and_requirements_are_deterministic_under_input_reordering() {
        let mut catalog = empty_catalog();
        let mut evidence = EvidenceSet::default();
        add_family(
            0,
            LegacyCreatureGame::Fnv,
            CreatureFamilyKind::Organic,
            MovementArchitecture::Grounded,
            ControllerArchitecture::Quadruped,
            80.0,
            BodyReadiness::Ready,
            &mut catalog,
            &mut evidence,
        );
        add_family(
            1,
            LegacyCreatureGame::Fo3,
            CreatureFamilyKind::RobotOrTurret,
            MovementArchitecture::Stationary,
            ControllerArchitecture::Fixed,
            120.0,
            BodyReadiness::Ready,
            &mut catalog,
            &mut evidence,
        );
        finish_catalog(&mut catalog);
        let forward = build_race_data_adapter(&catalog, evidence.inputs(), &policy()).unwrap();
        let forward_requirements = production_evidence_requirements(&catalog);

        catalog.records.reverse();
        catalog.rig_families.reverse();
        catalog.body_variants.reverse();
        catalog.attack_sets.reverse();
        evidence.reverse();
        let reverse = build_race_data_adapter(&catalog, evidence.inputs(), &policy()).unwrap();
        assert_eq!(forward, reverse);
        assert_eq!(
            forward_requirements,
            production_evidence_requirements(&catalog)
        );
    }
}
