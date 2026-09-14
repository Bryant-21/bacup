use std::collections::{BTreeMap, BTreeSet, HashMap};

use serde::{Deserialize, Serialize};

use crate::phase::animations::{
    FnvExtractedMotionPolicy, FnvKfBindingCompatibility, FnvKfCycle, FnvKfRootMotionEvidence,
    FnvKfStageRequest, KfParseError, parse_fnv_creature_kf,
};
use crate::phase::skeleton::SourceRigSkeletonArtifactReceipt;

pub(crate) use crate::phase::animations::FnvKfSkeletonContract as CreatureKfSkeletonContract;

use super::creature_catalog::{LegacyCreatureGame, RigFamilyKey, StableFormKey};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CreatureMotionTrait {
    Walks,
    Swims,
    Flies,
    Robot,
    Stationary,
    Turret,
    Continuous,
    HumanoidWeaponOverlay,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreatureTraitEvidence {
    pub motion_trait: CreatureMotionTrait,
    pub source_creature: StableFormKey,
    pub field: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IdleConditionEvidence {
    pub signature: String,
    pub raw_hex: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IdleAncestryNode {
    pub form_key: StableFormKey,
    pub editor_id: Option<String>,
    pub model_path: Option<String>,
    pub parent: Option<StableFormKey>,
    pub previous: Option<StableFormKey>,
    pub conditions: Vec<IdleConditionEvidence>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IdleClaimEvidence {
    pub source_idle: StableFormKey,
    /// Leaf first, then the exact parent chain. Previous remains an edge on
    /// each node and is not rewritten into parent ancestry.
    pub ancestry: Vec<IdleAncestryNode>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SequenceCycle {
    Loop,
    Reverse,
    Clamp,
}

impl SequenceCycle {
    pub fn loops(self) -> bool {
        matches!(self, Self::Loop | Self::Reverse)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TextKeyEvidence {
    pub time: f64,
    pub text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BindingCompatibility {
    Verified,
    Unverified,
    Incompatible,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BindingEvidence {
    pub source_skeleton_path: String,
    pub transform_track_count: usize,
    pub float_track_count: usize,
    pub controller_types: Vec<String>,
    pub interpolator_types: Vec<String>,
    pub target_names: Vec<String>,
    pub required_float_slots: Vec<String>,
    pub compatibility: BindingCompatibility,
    pub compatibility_detail: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum RootMotionEvidence {
    Unknown,
    None,
    Stationary {
        accum_root: Option<String>,
    },
    Planar {
        accum_root: String,
        distance: f64,
        yaw_radians: f64,
    },
    Unsupported {
        accum_root: Option<String>,
        detail: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NiControllerSequenceEvidence {
    pub sequence_index: usize,
    pub name: String,
    pub cycle: SequenceCycle,
    pub start_time: f64,
    pub stop_time: f64,
    pub frequency: f64,
    pub text_keys: Vec<TextKeyEvidence>,
    pub binding: BindingEvidence,
    pub root_motion: RootMotionEvidence,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum KfParseEvidence {
    Parsed(NiControllerSequenceEvidence),
    MissingAsset,
    ParseFailed(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CreatureKfEvidence {
    pub source_game: LegacyCreatureGame,
    pub source_kf: String,
    pub sequence: KfParseEvidence,
    pub idle_claims: Vec<IdleClaimEvidence>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CreatureMotionFamilyEvidence {
    pub rig: RigFamilyKey,
    /// Complete KF set claimed by the family. Evidence is deliberately a
    /// separate collection so absent and failed files cannot disappear.
    pub referenced_kfs: Vec<String>,
    pub kf_evidence: Vec<CreatureKfEvidence>,
    pub creature_traits: Vec<CreatureTraitEvidence>,
}

pub(crate) fn parse_creature_kf_evidence(
    kf_bytes: &[u8],
    source_kf: &str,
    source_game: LegacyCreatureGame,
    selected_sequence_index: Option<usize>,
    skeleton_contract: Option<CreatureKfSkeletonContract<'_>>,
) -> Result<CreatureKfEvidence, KfParseError> {
    let parsed = parse_fnv_creature_kf(
        kf_bytes,
        source_kf,
        selected_sequence_index,
        skeleton_contract,
    )?;
    let compatibility = match parsed.binding.compatibility {
        FnvKfBindingCompatibility::Verified => (BindingCompatibility::Verified, None),
        FnvKfBindingCompatibility::Unverified { detail } => {
            (BindingCompatibility::Unverified, Some(detail))
        }
        FnvKfBindingCompatibility::Incompatible { detail } => {
            (BindingCompatibility::Incompatible, Some(detail))
        }
    };
    let root_motion = match parsed.root_motion {
        FnvKfRootMotionEvidence::None => RootMotionEvidence::None,
        FnvKfRootMotionEvidence::Stationary { accum_root } => {
            RootMotionEvidence::Stationary { accum_root }
        }
        FnvKfRootMotionEvidence::Planar {
            accum_root,
            distance,
            yaw_radians,
        } => RootMotionEvidence::Planar {
            accum_root,
            distance,
            yaw_radians,
        },
        FnvKfRootMotionEvidence::Unsupported { accum_root, detail } => {
            RootMotionEvidence::Unsupported { accum_root, detail }
        }
    };
    Ok(CreatureKfEvidence {
        source_game,
        source_kf: parsed.source_kf,
        sequence: KfParseEvidence::Parsed(NiControllerSequenceEvidence {
            sequence_index: parsed.sequence_index,
            name: parsed.sequence_name,
            cycle: match parsed.cycle {
                FnvKfCycle::Loop => SequenceCycle::Loop,
                FnvKfCycle::Reverse => SequenceCycle::Reverse,
                FnvKfCycle::Clamp => SequenceCycle::Clamp,
            },
            start_time: parsed.start_time,
            stop_time: parsed.stop_time,
            frequency: parsed.frequency,
            text_keys: parsed
                .text_keys
                .into_iter()
                .map(|event| TextKeyEvidence {
                    time: event.time,
                    text: event.text,
                })
                .collect(),
            binding: BindingEvidence {
                source_skeleton_path: parsed.binding.skeleton_path.unwrap_or_default(),
                transform_track_count: parsed.binding.transform_track_count,
                float_track_count: parsed.binding.float_track_count,
                controller_types: parsed.binding.controller_types,
                interpolator_types: parsed.binding.interpolator_types,
                target_names: parsed.binding.target_names,
                required_float_slots: parsed.binding.required_float_slots,
                compatibility: compatibility.0,
                compatibility_detail: compatibility.1,
            },
            root_motion,
        }),
        idle_claims: Vec::new(),
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MotionRole {
    Idle,
    GroundLocomotion,
    Swim,
    Fly,
    Turn,
    MeleeAttack,
    RangedAttack,
    Fire,
    Projectile,
    StationaryOrTurret,
    ContinuousOrRobot,
    Overlay,
    Hurt,
    Death,
}

const ALL_MOTION_ROLES: [MotionRole; 14] = [
    MotionRole::Idle,
    MotionRole::GroundLocomotion,
    MotionRole::Swim,
    MotionRole::Fly,
    MotionRole::Turn,
    MotionRole::MeleeAttack,
    MotionRole::RangedAttack,
    MotionRole::Fire,
    MotionRole::Projectile,
    MotionRole::StationaryOrTurret,
    MotionRole::ContinuousOrRobot,
    MotionRole::Overlay,
    MotionRole::Hurt,
    MotionRole::Death,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CandidateConfidence {
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoleSignalSource {
    SequenceName,
    TextKey,
    IdleAncestry,
    BindingTarget,
    RootMotion,
    CreatureField,
    Filename,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RoleSignalProvenance {
    pub source: RoleSignalSource,
    pub detail: String,
    pub event_time: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MotionEventKind {
    Begin,
    End,
    Hit,
    ProjectileRelease,
    Sound,
    Movement,
    Attachment,
    Other,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MotionEventProvenance {
    pub time: f64,
    pub raw_text: String,
    pub kind: MotionEventKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BindingSummary {
    pub source_skeleton_path: String,
    pub transform_track_count: usize,
    pub float_track_count: usize,
    pub controller_types: Vec<String>,
    pub interpolator_types: Vec<String>,
    pub overlay_targets: Vec<String>,
    pub required_float_slots: Vec<String>,
    pub compatibility: BindingCompatibility,
    pub compatibility_detail: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MotionCandidate {
    pub source_game: LegacyCreatureGame,
    pub source_kf: String,
    pub sequence_index: usize,
    pub sequence_name: String,
    pub cycle: SequenceCycle,
    pub looping: bool,
    pub events: Vec<MotionEventProvenance>,
    pub idle_claims: Vec<IdleClaimEvidence>,
    pub binding: BindingSummary,
    pub root_motion: RootMotionEvidence,
    pub confidence: CandidateConfidence,
    pub evidence: Vec<RoleSignalProvenance>,
    pub ambiguous_with: Vec<MotionRole>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub(crate) enum CreatureKfStageRequestError {
    #[error("motion candidate binding is not verified: {detail}")]
    BindingNotVerified { detail: String },
    #[error(
        "motion candidate skeleton path {candidate_path:?} does not match staged skeleton receipt {receipt_path:?}"
    )]
    SkeletonPathMismatch {
        candidate_path: String,
        receipt_path: String,
    },
    #[error("staged skeleton receipt is missing required float slot {slot:?}")]
    MissingFloatSlot { slot: String },
    #[error("motion candidate has no supported extracted-motion policy: {detail}")]
    UnsupportedMotion { detail: String },
}

pub(crate) fn stage_request_from_motion_candidate<'a>(
    candidate: &'a MotionCandidate,
    source_skeleton_path: &str,
    skeleton: &'a SourceRigSkeletonArtifactReceipt,
    output_clip_path: &'a str,
    event_map: &'a HashMap<String, String>,
    target_sample_rate_hz: Option<f64>,
) -> Result<FnvKfStageRequest<'a>, CreatureKfStageRequestError> {
    if candidate.binding.compatibility != BindingCompatibility::Verified {
        return Err(CreatureKfStageRequestError::BindingNotVerified {
            detail: candidate
                .binding
                .compatibility_detail
                .clone()
                .unwrap_or_else(|| format!("{:?}", candidate.binding.compatibility)),
        });
    }
    if canonical_motion_path(&candidate.binding.source_skeleton_path)
        != canonical_motion_path(source_skeleton_path)
    {
        return Err(CreatureKfStageRequestError::SkeletonPathMismatch {
            candidate_path: candidate.binding.source_skeleton_path.clone(),
            receipt_path: source_skeleton_path.to_string(),
        });
    }
    for required in &candidate.binding.required_float_slots {
        if !skeleton
            .float_slot_names
            .iter()
            .any(|slot| slot == required)
        {
            return Err(CreatureKfStageRequestError::MissingFloatSlot {
                slot: required.clone(),
            });
        }
    }
    let extracted_motion_policy = match &candidate.root_motion {
        RootMotionEvidence::None | RootMotionEvidence::Stationary { .. } => {
            FnvExtractedMotionPolicy::RejectNonzero
        }
        RootMotionEvidence::Planar { .. } => FnvExtractedMotionPolicy::ExtractPlanarReferenceFrame,
        RootMotionEvidence::Unknown => {
            return Err(CreatureKfStageRequestError::UnsupportedMotion {
                detail: "root motion is unknown".to_string(),
            });
        }
        RootMotionEvidence::Unsupported { detail, .. } => {
            return Err(CreatureKfStageRequestError::UnsupportedMotion {
                detail: detail.clone(),
            });
        }
    };
    Ok(FnvKfStageRequest {
        source_kf: &candidate.source_kf,
        output_clip_path,
        sequence_index: Some(candidate.sequence_index),
        skeleton: CreatureKfSkeletonContract {
            skeleton_path: &skeleton.runtime_path,
            ordered_bone_names: &skeleton.ordered_bone_names,
            ordered_float_slot_names: &skeleton.float_slot_names,
        },
        original_skeleton_name: &skeleton.skeleton_name,
        event_map,
        target_sample_rate_hz,
        extracted_motion_policy,
    })
}

fn canonical_motion_path(path: &str) -> String {
    path.trim().replace('\\', "/").to_ascii_lowercase()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoleReadiness {
    Missing,
    Ready,
    Ambiguous(Vec<String>),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MotionRoleCandidates {
    pub role: MotionRole,
    pub candidates: Vec<MotionCandidate>,
    pub readiness: RoleReadiness,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CoreMotionRequirement {
    Idle,
    MobilityOrStationary,
    Combat,
    Death,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FamilyMotionReadiness {
    Ready,
    Incomplete {
        missing: Vec<CoreMotionRequirement>,
    },
    Ambiguous {
        roles: Vec<MotionRole>,
        kfs: Vec<String>,
    },
    Unclassified {
        kfs: Vec<String>,
    },
    EvidenceIncomplete {
        kfs: Vec<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum KfAccountingDisposition {
    Classified(Vec<MotionRole>),
    Ambiguous(Vec<MotionRole>),
    Unclassified,
    MissingEvidence,
    MissingAsset,
    ParseFailed(String),
    BindingUnverified(Vec<MotionRole>),
    BindingIncompatible(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KfAccountingEntry {
    pub source_kf: String,
    pub disposition: KfAccountingDisposition,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MotionAccounting {
    pub families: usize,
    pub referenced_kfs: usize,
    pub classified_kfs: usize,
    pub ambiguous_kfs: usize,
    pub unclassified_kfs: usize,
    pub unavailable_kfs: usize,
    pub role_assignments: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CreatureFamilyMotionSet {
    pub rig: RigFamilyKey,
    pub roles: Vec<MotionRoleCandidates>,
    pub readiness: FamilyMotionReadiness,
    pub kf_accounting: Vec<KfAccountingEntry>,
}

impl CreatureFamilyMotionSet {
    pub fn role(&self, role: MotionRole) -> Option<&MotionRoleCandidates> {
        self.roles.iter().find(|entry| entry.role == role)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CreatureMotionCatalog {
    pub families: Vec<CreatureFamilyMotionSet>,
    pub accounting: MotionAccounting,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CreatureMotionCatalogError {
    #[error("duplicate motion family {rig:?}")]
    DuplicateFamily { rig: RigFamilyKey },
    #[error("family {rig:?} references KF path more than once: {path}")]
    DuplicateReferencedKf { rig: RigFamilyKey, path: String },
    #[error("family {rig:?} has duplicate evidence for KF path: {path}")]
    DuplicateKfEvidence { rig: RigFamilyKey, path: String },
    #[error("family {rig:?} has evidence for unreferenced KF path: {path}")]
    UnreferencedKfEvidence { rig: RigFamilyKey, path: String },
    #[error("family {rig:?} has a non-KF asset in its motion set: {path}")]
    InvalidKfPath { rig: RigFamilyKey, path: String },
    #[error("motion accounting drifted: {detail}")]
    AccountingDrift { detail: String },
}

pub fn build_creature_motion_catalog(
    families: &[CreatureMotionFamilyEvidence],
) -> Result<CreatureMotionCatalog, CreatureMotionCatalogError> {
    let mut ordered = families.iter().collect::<Vec<_>>();
    ordered.sort_by(|left, right| left.rig.cmp(&right.rig));
    for pair in ordered.windows(2) {
        if pair[0].rig == pair[1].rig {
            return Err(CreatureMotionCatalogError::DuplicateFamily {
                rig: pair[0].rig.clone(),
            });
        }
    }

    let mut catalog_families = Vec::with_capacity(ordered.len());
    let mut accounting = MotionAccounting {
        families: ordered.len(),
        ..MotionAccounting::default()
    };
    for family in ordered {
        let motion_set = build_family_motion_set(family)?;
        accumulate_accounting(&mut accounting, &motion_set);
        catalog_families.push(motion_set);
    }
    validate_accounting(&accounting, &catalog_families)?;
    Ok(CreatureMotionCatalog {
        families: catalog_families,
        accounting,
    })
}

fn build_family_motion_set(
    family: &CreatureMotionFamilyEvidence,
) -> Result<CreatureFamilyMotionSet, CreatureMotionCatalogError> {
    let referenced = indexed_paths(&family.rig, &family.referenced_kfs, true)?;
    let evidence_paths = family
        .kf_evidence
        .iter()
        .map(|evidence| evidence.source_kf.clone())
        .collect::<Vec<_>>();
    let evidence_index = indexed_paths(&family.rig, &evidence_paths, false)?;
    for (key, path) in &evidence_index {
        if !referenced.contains_key(key) {
            return Err(CreatureMotionCatalogError::UnreferencedKfEvidence {
                rig: family.rig.clone(),
                path: path.clone(),
            });
        }
    }
    let evidence_by_key = family
        .kf_evidence
        .iter()
        .map(|evidence| (path_key(&evidence.source_kf), evidence))
        .collect::<BTreeMap<_, _>>();
    let traits = family
        .creature_traits
        .iter()
        .map(|evidence| evidence.motion_trait)
        .collect::<BTreeSet<_>>();

    let mut role_candidates = ALL_MOTION_ROLES
        .into_iter()
        .map(|role| (role, Vec::new()))
        .collect::<BTreeMap<_, Vec<MotionCandidate>>>();
    let mut kf_accounting = Vec::with_capacity(referenced.len());
    for (key, source_kf) in referenced {
        let Some(evidence) = evidence_by_key.get(&key) else {
            kf_accounting.push(KfAccountingEntry {
                source_kf,
                disposition: KfAccountingDisposition::MissingEvidence,
            });
            continue;
        };
        match &evidence.sequence {
            KfParseEvidence::MissingAsset => kf_accounting.push(KfAccountingEntry {
                source_kf,
                disposition: KfAccountingDisposition::MissingAsset,
            }),
            KfParseEvidence::ParseFailed(detail) => kf_accounting.push(KfAccountingEntry {
                source_kf,
                disposition: KfAccountingDisposition::ParseFailed(detail.clone()),
            }),
            KfParseEvidence::Parsed(sequence) => {
                let classified = classify_sequence(family, evidence, sequence, &source_kf, &traits);
                let roles = classified.keys().copied().collect::<Vec<_>>();
                let ambiguous_roles = semantic_ambiguity(&roles);
                let disposition = match sequence.binding.compatibility {
                    BindingCompatibility::Incompatible => {
                        KfAccountingDisposition::BindingIncompatible(
                            sequence
                                .binding
                                .compatibility_detail
                                .clone()
                                .unwrap_or_else(|| "binding marked incompatible".to_string()),
                        )
                    }
                    BindingCompatibility::Unverified => {
                        KfAccountingDisposition::BindingUnverified(roles.clone())
                    }
                    BindingCompatibility::Verified if roles.is_empty() => {
                        KfAccountingDisposition::Unclassified
                    }
                    BindingCompatibility::Verified if !ambiguous_roles.is_empty() => {
                        KfAccountingDisposition::Ambiguous(roles.clone())
                    }
                    BindingCompatibility::Verified => {
                        KfAccountingDisposition::Classified(roles.clone())
                    }
                };
                if sequence.binding.compatibility == BindingCompatibility::Verified {
                    for (role, mut candidate) in classified {
                        candidate.ambiguous_with = ambiguous_roles
                            .iter()
                            .copied()
                            .filter(|other| *other != role)
                            .collect();
                        if motion_candidate_is_stageable(&candidate) {
                            role_candidates
                                .get_mut(&role)
                                .expect("all roles have buckets")
                                .push(candidate);
                        }
                    }
                }
                kf_accounting.push(KfAccountingEntry {
                    source_kf,
                    disposition,
                });
            }
        }
    }

    let roles = ALL_MOTION_ROLES
        .into_iter()
        .map(|role| {
            let mut candidates = role_candidates.remove(&role).unwrap_or_default();
            candidates.sort_by(candidate_order);
            let readiness = role_readiness(role, &candidates);
            MotionRoleCandidates {
                role,
                candidates,
                readiness,
            }
        })
        .collect::<Vec<_>>();
    let readiness = family_readiness(&roles, &kf_accounting);
    Ok(CreatureFamilyMotionSet {
        rig: family.rig.clone(),
        roles,
        readiness,
        kf_accounting,
    })
}

fn indexed_paths(
    rig: &RigFamilyKey,
    paths: &[String],
    referenced: bool,
) -> Result<BTreeMap<String, String>, CreatureMotionCatalogError> {
    let mut indexed = BTreeMap::new();
    for path in paths {
        if !path_key(path).ends_with(".kf") {
            return Err(CreatureMotionCatalogError::InvalidKfPath {
                rig: rig.clone(),
                path: path.clone(),
            });
        }
        let key = path_key(path);
        if indexed.insert(key, path.clone()).is_some() {
            return Err(if referenced {
                CreatureMotionCatalogError::DuplicateReferencedKf {
                    rig: rig.clone(),
                    path: path.clone(),
                }
            } else {
                CreatureMotionCatalogError::DuplicateKfEvidence {
                    rig: rig.clone(),
                    path: path.clone(),
                }
            });
        }
    }
    Ok(indexed)
}

fn classify_sequence(
    family: &CreatureMotionFamilyEvidence,
    evidence: &CreatureKfEvidence,
    sequence: &NiControllerSequenceEvidence,
    source_kf: &str,
    traits: &BTreeSet<CreatureMotionTrait>,
) -> BTreeMap<MotionRole, MotionCandidate> {
    let mut signals =
        BTreeMap::<MotionRole, Vec<(CandidateConfidence, RoleSignalProvenance)>>::new();
    let name = normalized_words(&sequence.name);
    let path = normalized_words(source_kf);
    let events = event_provenance(&sequence.text_keys);
    let event_text = sequence
        .text_keys
        .iter()
        .map(|event| normalized_words(&event.text))
        .collect::<Vec<_>>();
    let ancestry_text = evidence
        .idle_claims
        .iter()
        .flat_map(|claim| &claim.ancestry)
        .flat_map(|node| {
            node.editor_id
                .iter()
                .chain(node.model_path.iter())
                .map(|value| normalized_words(value))
        })
        .collect::<Vec<_>>();

    let sequence_signal = |role: MotionRole, detail: &str, signals: &mut BTreeMap<_, _>| {
        add_signal(
            signals,
            role,
            CandidateConfidence::High,
            RoleSignalSource::SequenceName,
            detail,
            None,
        );
    };
    let has_name = |tokens: &[&str]| tokens.iter().any(|token| name.contains(token));
    let has_event = |tokens: &[&str]| {
        event_text
            .iter()
            .any(|text| tokens.iter().any(|token| text.contains(token)))
    };
    let has_ancestry = |tokens: &[&str]| {
        ancestry_text
            .iter()
            .any(|text| tokens.iter().any(|token| text.contains(token)))
    };

    let death_name = has_name(&["death", "die", "dead", "unconscious"]);
    let death_ancestry = has_ancestry(&["death", "dead", "unconscious"]);
    let death = death_name || death_ancestry;
    let hurt_name = has_name(&["hurt", "recoil", "hitreaction", "criticalhit"]);
    let hurt_ancestry = has_ancestry(&["hitreaction", "criticalhit", "hurt"]);
    let hurt = hurt_name || hurt_ancestry;
    let attack_name = has_name(&["attack", "strike", "bite", "claw"]);
    let attack_ancestry = has_ancestry(&["attack"]);
    let attack = attack_name || attack_ancestry;
    let turn = has_name(&["turnleft", "turnright", "turn left", "turn right"]);
    let movement = has_name(&[
        "forward",
        "backward",
        "fastforward",
        "fastbackward",
        "strafe",
        "walk",
        "run",
    ]);
    let explicit_swim = has_name(&["swim"]);
    let explicit_fly = has_name(&["fly", "hover", "land", "takeoff"])
        || has_ancestry(&["fly", "hover", "land", "takeoff"]);
    let projectile = has_name(&["throw", "projectile", "spit", "dart", "missile"])
        || has_event(&["release", "eject", "projectile", "dart", "missile"]);
    let fire = has_name(&["fire", "shoot", "burst", "attackauto"])
        || has_event(&["fire", "gun", "laser", "plasma", "eject"]);

    if death {
        if death_name {
            sequence_signal(MotionRole::Death, &sequence.name, &mut signals);
        }
        if death_ancestry {
            add_ancestry_signals(
                &mut signals,
                MotionRole::Death,
                evidence,
                &["death", "dead", "unconscious"],
            );
        }
    } else if hurt {
        if hurt_name {
            sequence_signal(MotionRole::Hurt, &sequence.name, &mut signals);
        }
        if hurt_ancestry {
            add_ancestry_signals(
                &mut signals,
                MotionRole::Hurt,
                evidence,
                &["hitreaction", "criticalhit", "hurt"],
            );
        }
    } else if !attack && (has_name(&["idle"]) || has_ancestry(&["idle"])) {
        if has_name(&["idle"]) {
            sequence_signal(MotionRole::Idle, &sequence.name, &mut signals);
        }
        if has_ancestry(&["idle"]) {
            add_ancestry_signals(&mut signals, MotionRole::Idle, evidence, &["idle"]);
        }
    } else if !attack
        && sequence.cycle.loops()
        && has_name(&["aim"])
        && matches!(
            sequence.root_motion,
            RootMotionEvidence::None | RootMotionEvidence::Stationary { .. }
        )
        && !traits.contains(&CreatureMotionTrait::HumanoidWeaponOverlay)
        && !traits.contains(&CreatureMotionTrait::Robot)
        && !traits.contains(&CreatureMotionTrait::Continuous)
    {
        sequence_signal(MotionRole::Idle, &sequence.name, &mut signals);
    }
    if turn {
        sequence_signal(MotionRole::Turn, &sequence.name, &mut signals);
    }

    if movement {
        classify_mobility(
            family,
            source_kf,
            explicit_swim,
            explicit_fly,
            traits,
            &mut signals,
        );
    }

    if attack {
        if attack_ancestry {
            add_ancestry_signals(
                &mut signals,
                if projectile || fire {
                    MotionRole::RangedAttack
                } else {
                    MotionRole::MeleeAttack
                },
                evidence,
                &["attack"],
            );
        }
        if projectile || fire {
            if attack_name {
                sequence_signal(MotionRole::RangedAttack, &sequence.name, &mut signals);
            }
            if projectile {
                add_signal(
                    &mut signals,
                    MotionRole::Projectile,
                    CandidateConfidence::High,
                    if has_name(&["throw", "projectile", "spit", "dart", "missile"]) {
                        RoleSignalSource::SequenceName
                    } else {
                        RoleSignalSource::TextKey
                    },
                    "projectile release evidence",
                    first_event_time(
                        &sequence.text_keys,
                        &["release", "eject", "projectile", "dart", "missile"],
                    ),
                );
            }
            if fire {
                add_signal(
                    &mut signals,
                    MotionRole::Fire,
                    CandidateConfidence::High,
                    if has_name(&["fire", "shoot", "burst", "attackauto"]) {
                        RoleSignalSource::SequenceName
                    } else {
                        RoleSignalSource::TextKey
                    },
                    "fire evidence",
                    first_event_time(
                        &sequence.text_keys,
                        &["fire", "gun", "laser", "plasma", "eject"],
                    ),
                );
            }
        } else {
            if attack_name {
                sequence_signal(MotionRole::MeleeAttack, &sequence.name, &mut signals);
            }
        }
    }

    if let RootMotionEvidence::Planar {
        distance,
        yaw_radians,
        ..
    } = sequence.root_motion
    {
        if turn && yaw_radians.abs() > 1.0e-4 {
            add_signal(
                &mut signals,
                MotionRole::Turn,
                CandidateConfidence::High,
                RoleSignalSource::RootMotion,
                &format!("planar yaw {yaw_radians}"),
                None,
            );
        }
        if movement && distance.abs() > 1.0e-4 {
            for role in [
                MotionRole::GroundLocomotion,
                MotionRole::Swim,
                MotionRole::Fly,
            ] {
                if signals.contains_key(&role) {
                    add_signal(
                        &mut signals,
                        role,
                        CandidateConfidence::High,
                        RoleSignalSource::RootMotion,
                        &format!("planar distance {distance}"),
                        None,
                    );
                }
            }
        }
    }

    let overlay_targets = sequence
        .binding
        .target_names
        .iter()
        .filter(|target| target.starts_with("##"))
        .cloned()
        .collect::<Vec<_>>();
    for target in &overlay_targets {
        add_signal(
            &mut signals,
            MotionRole::Overlay,
            CandidateConfidence::High,
            RoleSignalSource::BindingTarget,
            target,
            None,
        );
    }

    if traits.contains(&CreatureMotionTrait::Stationary)
        || traits.contains(&CreatureMotionTrait::Turret)
    {
        if sequence.cycle.loops() || attack || fire {
            add_trait_signal(
                &mut signals,
                MotionRole::StationaryOrTurret,
                family,
                &[CreatureMotionTrait::Stationary, CreatureMotionTrait::Turret],
            );
        }
    }
    if traits.contains(&CreatureMotionTrait::Robot)
        || traits.contains(&CreatureMotionTrait::Continuous)
    {
        if sequence.cycle.loops() || has_name(&["auto", "loop", "continuous", "aim"]) {
            add_trait_signal(
                &mut signals,
                MotionRole::ContinuousOrRobot,
                family,
                &[CreatureMotionTrait::Robot, CreatureMotionTrait::Continuous],
            );
        }
    }
    if traits.contains(&CreatureMotionTrait::HumanoidWeaponOverlay) && !overlay_targets.is_empty() {
        add_trait_signal(
            &mut signals,
            MotionRole::Overlay,
            family,
            &[CreatureMotionTrait::HumanoidWeaponOverlay],
        );
    }

    add_secondary_filename_signal(source_kf, &path, &mut signals);

    let binding = binding_summary(&sequence.binding);
    let mut events = events;
    events.sort_by(|left, right| {
        left.time
            .total_cmp(&right.time)
            .then_with(|| left.raw_text.cmp(&right.raw_text))
    });
    signals
        .into_iter()
        .map(|(role, mut role_signals)| {
            role_signals.sort_by(|left, right| {
                right
                    .0
                    .cmp(&left.0)
                    .then_with(|| left.1.source.cmp(&right.1.source))
                    .then_with(|| left.1.detail.cmp(&right.1.detail))
            });
            let confidence = role_signals
                .iter()
                .map(|(confidence, _)| *confidence)
                .max()
                .unwrap_or(CandidateConfidence::Low);
            let provenance = role_signals
                .into_iter()
                .map(|(_, provenance)| provenance)
                .collect();
            (
                role,
                MotionCandidate {
                    source_game: evidence.source_game,
                    source_kf: source_kf.to_string(),
                    sequence_index: sequence.sequence_index,
                    sequence_name: sequence.name.clone(),
                    cycle: sequence.cycle,
                    looping: sequence.cycle.loops(),
                    events: events.clone(),
                    idle_claims: sorted_idle_claims(&evidence.idle_claims),
                    binding: binding.clone(),
                    root_motion: sequence.root_motion.clone(),
                    confidence,
                    evidence: provenance,
                    ambiguous_with: Vec::new(),
                },
            )
        })
        .collect()
}

fn classify_mobility(
    family: &CreatureMotionFamilyEvidence,
    source_kf: &str,
    explicit_swim: bool,
    explicit_fly: bool,
    traits: &BTreeSet<CreatureMotionTrait>,
    signals: &mut BTreeMap<MotionRole, Vec<(CandidateConfidence, RoleSignalProvenance)>>,
) {
    let path = path_key(source_kf);
    let path_swim = path.contains("swim");
    let path_fly = path.contains("fly") || path.contains("hover");
    if explicit_swim || (path_swim && traits.contains(&CreatureMotionTrait::Swims)) {
        add_trait_signal(
            signals,
            MotionRole::Swim,
            family,
            &[CreatureMotionTrait::Swims],
        );
        if path_swim && !explicit_swim {
            add_signal(
                signals,
                MotionRole::Swim,
                CandidateConfidence::Low,
                RoleSignalSource::Filename,
                source_kf,
                None,
            );
        }
        return;
    }
    if explicit_fly || path_fly || traits.contains(&CreatureMotionTrait::Flies) {
        add_trait_signal(
            signals,
            MotionRole::Fly,
            family,
            &[CreatureMotionTrait::Flies],
        );
        if path_fly && !explicit_fly {
            add_signal(
                signals,
                MotionRole::Fly,
                CandidateConfidence::Low,
                RoleSignalSource::Filename,
                source_kf,
                None,
            );
        }
        return;
    }
    if traits.contains(&CreatureMotionTrait::Walks) {
        add_trait_signal(
            signals,
            MotionRole::GroundLocomotion,
            family,
            &[CreatureMotionTrait::Walks],
        );
    } else {
        add_signal(
            signals,
            MotionRole::GroundLocomotion,
            CandidateConfidence::Medium,
            RoleSignalSource::SequenceName,
            "generic locomotion sequence",
            None,
        );
    }
}

fn add_ancestry_signals(
    signals: &mut BTreeMap<MotionRole, Vec<(CandidateConfidence, RoleSignalProvenance)>>,
    role: MotionRole,
    evidence: &CreatureKfEvidence,
    tokens: &[&str],
) {
    for claim in &evidence.idle_claims {
        for node in &claim.ancestry {
            let matches = node
                .editor_id
                .iter()
                .chain(node.model_path.iter())
                .map(|value| normalized_words(value))
                .any(|value| tokens.iter().any(|token| value.contains(token)));
            if matches {
                add_signal(
                    signals,
                    role,
                    CandidateConfidence::High,
                    RoleSignalSource::IdleAncestry,
                    &format!(
                        "{} editor={:?} model={:?} parent={:?} previous={:?} conditions={}",
                        node.form_key,
                        node.editor_id,
                        node.model_path,
                        node.parent,
                        node.previous,
                        node.conditions.len()
                    ),
                    None,
                );
            }
        }
    }
}

fn add_trait_signal(
    signals: &mut BTreeMap<MotionRole, Vec<(CandidateConfidence, RoleSignalProvenance)>>,
    role: MotionRole,
    family: &CreatureMotionFamilyEvidence,
    accepted: &[CreatureMotionTrait],
) {
    for evidence in family
        .creature_traits
        .iter()
        .filter(|evidence| accepted.contains(&evidence.motion_trait))
    {
        add_signal(
            signals,
            role,
            CandidateConfidence::High,
            RoleSignalSource::CreatureField,
            &format!(
                "{} {}={}",
                evidence.source_creature, evidence.field, evidence.value
            ),
            None,
        );
    }
}

fn add_secondary_filename_signal(
    source_kf: &str,
    normalized_path: &str,
    signals: &mut BTreeMap<MotionRole, Vec<(CandidateConfidence, RoleSignalProvenance)>>,
) {
    let role = if normalized_path.contains("death") {
        Some(MotionRole::Death)
    } else if normalized_path.contains("hurt") || normalized_path.contains("recoil") {
        Some(MotionRole::Hurt)
    } else if normalized_path.contains("idle") {
        Some(MotionRole::Idle)
    } else {
        None
    };
    if let Some(role) = role {
        add_signal(
            signals,
            role,
            CandidateConfidence::Low,
            RoleSignalSource::Filename,
            source_kf,
            None,
        );
    }
}

fn add_signal(
    signals: &mut BTreeMap<MotionRole, Vec<(CandidateConfidence, RoleSignalProvenance)>>,
    role: MotionRole,
    confidence: CandidateConfidence,
    source: RoleSignalSource,
    detail: &str,
    event_time: Option<f64>,
) {
    signals.entry(role).or_default().push((
        confidence,
        RoleSignalProvenance {
            source,
            detail: detail.to_string(),
            event_time,
        },
    ));
}

fn event_provenance(events: &[TextKeyEvidence]) -> Vec<MotionEventProvenance> {
    events
        .iter()
        .map(|event| {
            let text = normalized_words(&event.text);
            let kind = if text == "start" {
                MotionEventKind::Begin
            } else if text == "end" {
                MotionEventKind::End
            } else if text == "hit" || text.starts_with("hit ") {
                MotionEventKind::Hit
            } else if ["release", "eject", "projectile", "missile"]
                .iter()
                .any(|token| text.contains(token))
            {
                MotionEventKind::ProjectileRelease
            } else if text.starts_with("sound") {
                MotionEventKind::Sound
            } else if text.starts_with("enum") || text.starts_with("m ") || text.starts_with("a ") {
                MotionEventKind::Movement
            } else if ["attach", "detach", "prn"]
                .iter()
                .any(|token| text.starts_with(token))
            {
                MotionEventKind::Attachment
            } else {
                MotionEventKind::Other
            };
            MotionEventProvenance {
                time: event.time,
                raw_text: event.text.clone(),
                kind,
            }
        })
        .collect()
}

fn first_event_time(events: &[TextKeyEvidence], tokens: &[&str]) -> Option<f64> {
    events.iter().find_map(|event| {
        let text = normalized_words(&event.text);
        tokens
            .iter()
            .any(|token| text.contains(token))
            .then_some(event.time)
    })
}

fn binding_summary(binding: &BindingEvidence) -> BindingSummary {
    let mut controller_types = binding.controller_types.clone();
    sort_case_insensitive(&mut controller_types);
    controller_types.dedup_by(|left, right| left.eq_ignore_ascii_case(right));
    let mut interpolator_types = binding.interpolator_types.clone();
    sort_case_insensitive(&mut interpolator_types);
    interpolator_types.dedup_by(|left, right| left.eq_ignore_ascii_case(right));
    let mut overlay_targets = binding
        .target_names
        .iter()
        .filter(|target| target.starts_with("##"))
        .cloned()
        .collect::<Vec<_>>();
    sort_case_insensitive(&mut overlay_targets);
    overlay_targets.dedup();
    BindingSummary {
        source_skeleton_path: binding.source_skeleton_path.clone(),
        transform_track_count: binding.transform_track_count,
        float_track_count: binding.float_track_count,
        controller_types,
        interpolator_types,
        overlay_targets,
        required_float_slots: binding.required_float_slots.clone(),
        compatibility: binding.compatibility,
        compatibility_detail: binding.compatibility_detail.clone(),
    }
}

fn sorted_idle_claims(claims: &[IdleClaimEvidence]) -> Vec<IdleClaimEvidence> {
    let mut claims = claims.to_vec();
    claims.sort_by(|left, right| left.source_idle.cmp(&right.source_idle));
    claims
}

fn semantic_ambiguity(roles: &[MotionRole]) -> Vec<MotionRole> {
    let base_roles = roles
        .iter()
        .copied()
        .filter(|role| {
            matches!(
                role,
                MotionRole::Idle
                    | MotionRole::GroundLocomotion
                    | MotionRole::Swim
                    | MotionRole::Fly
                    | MotionRole::MeleeAttack
                    | MotionRole::RangedAttack
                    | MotionRole::Hurt
                    | MotionRole::Death
            )
        })
        .collect::<Vec<_>>();
    let categories = base_roles
        .iter()
        .map(|role| match role {
            MotionRole::Idle | MotionRole::Hurt | MotionRole::Death => 0,
            MotionRole::GroundLocomotion | MotionRole::Swim | MotionRole::Fly => 1,
            MotionRole::MeleeAttack | MotionRole::RangedAttack => 2,
            _ => unreachable!("filtered base roles"),
        })
        .collect::<BTreeSet<_>>();
    if categories.len() > 1
        || [
            MotionRole::GroundLocomotion,
            MotionRole::Swim,
            MotionRole::Fly,
        ]
        .iter()
        .filter(|role| base_roles.contains(role))
        .count()
            > 1
        || (base_roles.contains(&MotionRole::MeleeAttack)
            && base_roles.contains(&MotionRole::RangedAttack))
    {
        base_roles
    } else {
        Vec::new()
    }
}

fn role_readiness(role: MotionRole, candidates: &[MotionCandidate]) -> RoleReadiness {
    if candidates.is_empty() {
        return RoleReadiness::Missing;
    }
    if candidates
        .iter()
        .any(|candidate| !candidate.ambiguous_with.is_empty())
    {
        return RoleReadiness::Ambiguous(
            candidates
                .iter()
                .filter(|candidate| !candidate.ambiguous_with.is_empty())
                .map(|candidate| candidate.source_kf.clone())
                .collect(),
        );
    }
    if role == MotionRole::Idle {
        let best = candidates
            .iter()
            .map(|candidate| candidate.confidence)
            .max()
            .expect("non-empty candidates");
        let best_paths = candidates
            .iter()
            .filter(|candidate| candidate.confidence == best)
            .map(|candidate| candidate.source_kf.clone())
            .collect::<Vec<_>>();
        if best_paths.len() > 1 {
            return RoleReadiness::Ambiguous(best_paths);
        }
    }
    RoleReadiness::Ready
}

fn family_readiness(
    roles: &[MotionRoleCandidates],
    _accounting: &[KfAccountingEntry],
) -> FamilyMotionReadiness {
    let has = |role: MotionRole| {
        roles
            .iter()
            .find(|candidates| candidates.role == role)
            .is_some_and(|candidates| !candidates.candidates.is_empty())
    };
    let mut missing = Vec::new();
    if !has(MotionRole::Idle) {
        missing.push(CoreMotionRequirement::Idle);
    }
    if ![
        MotionRole::GroundLocomotion,
        MotionRole::Swim,
        MotionRole::Fly,
        MotionRole::StationaryOrTurret,
    ]
    .iter()
    .copied()
    .any(has)
    {
        missing.push(CoreMotionRequirement::MobilityOrStationary);
    }
    if ![
        MotionRole::MeleeAttack,
        MotionRole::RangedAttack,
        MotionRole::Fire,
        MotionRole::Projectile,
    ]
    .iter()
    .copied()
    .any(has)
    {
        missing.push(CoreMotionRequirement::Combat);
    }
    if missing.is_empty() {
        FamilyMotionReadiness::Ready
    } else {
        FamilyMotionReadiness::Incomplete { missing }
    }
}

fn candidate_order(left: &MotionCandidate, right: &MotionCandidate) -> std::cmp::Ordering {
    right
        .confidence
        .cmp(&left.confidence)
        .then_with(|| path_key(&left.source_kf).cmp(&path_key(&right.source_kf)))
        .then_with(|| left.source_kf.cmp(&right.source_kf))
}

fn motion_candidate_is_stageable(candidate: &MotionCandidate) -> bool {
    candidate.binding.compatibility == BindingCompatibility::Verified
        && !candidate.binding.source_skeleton_path.trim().is_empty()
        && (candidate.binding.float_track_count == 0
            || !candidate.binding.required_float_slots.is_empty())
        && match &candidate.root_motion {
            RootMotionEvidence::None | RootMotionEvidence::Stationary { .. } => true,
            RootMotionEvidence::Planar {
                accum_root,
                distance,
                yaw_radians,
            } => !accum_root.trim().is_empty() && distance.is_finite() && yaw_radians.is_finite(),
            RootMotionEvidence::Unknown | RootMotionEvidence::Unsupported { .. } => false,
        }
}

fn accumulate_accounting(accounting: &mut MotionAccounting, family: &CreatureFamilyMotionSet) {
    accounting.referenced_kfs += family.kf_accounting.len();
    accounting.role_assignments += family
        .roles
        .iter()
        .map(|role| role.candidates.len())
        .sum::<usize>();
    for entry in &family.kf_accounting {
        match &entry.disposition {
            KfAccountingDisposition::Classified(_) => accounting.classified_kfs += 1,
            KfAccountingDisposition::Ambiguous(_) => accounting.ambiguous_kfs += 1,
            KfAccountingDisposition::Unclassified => accounting.unclassified_kfs += 1,
            KfAccountingDisposition::BindingUnverified(_) => accounting.ambiguous_kfs += 1,
            KfAccountingDisposition::MissingEvidence
            | KfAccountingDisposition::MissingAsset
            | KfAccountingDisposition::ParseFailed(_)
            | KfAccountingDisposition::BindingIncompatible(_) => accounting.unavailable_kfs += 1,
        }
    }
}

fn validate_accounting(
    accounting: &MotionAccounting,
    families: &[CreatureFamilyMotionSet],
) -> Result<(), CreatureMotionCatalogError> {
    let dispositions = accounting.classified_kfs
        + accounting.ambiguous_kfs
        + accounting.unclassified_kfs
        + accounting.unavailable_kfs;
    if dispositions != accounting.referenced_kfs {
        return Err(CreatureMotionCatalogError::AccountingDrift {
            detail: format!(
                "{dispositions} KF dispositions for {} references",
                accounting.referenced_kfs
            ),
        });
    }
    if families.len() != accounting.families {
        return Err(CreatureMotionCatalogError::AccountingDrift {
            detail: format!(
                "{} emitted families for {} inputs",
                families.len(),
                accounting.families
            ),
        });
    }
    Ok(())
}

fn normalized_words(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn path_key(path: &str) -> String {
    path.trim().replace('\\', "/").to_ascii_lowercase()
}

fn sort_case_insensitive(values: &mut [String]) {
    values.sort_by(|left, right| {
        left.to_ascii_lowercase()
            .cmp(&right.to_ascii_lowercase())
            .then_with(|| left.cmp(right))
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};

    fn key(local: u32, plugin: &str) -> StableFormKey {
        StableFormKey {
            local,
            plugin: plugin.to_string(),
        }
    }

    fn rig(game: LegacyCreatureGame, skeleton_path: &str) -> RigFamilyKey {
        RigFamilyKey {
            game,
            skeleton_path: skeleton_path.to_string(),
        }
    }

    fn creature_trait(
        motion_trait: CreatureMotionTrait,
        local: u32,
        plugin: &str,
        value: &str,
    ) -> CreatureTraitEvidence {
        CreatureTraitEvidence {
            motion_trait,
            source_creature: key(local, plugin),
            field: if motion_trait == CreatureMotionTrait::Robot {
                "DATA.Type".to_string()
            } else {
                "Configuration.Flags".to_string()
            },
            value: value.to_string(),
        }
    }

    fn event(time: f64, text: &str) -> TextKeyEvidence {
        TextKeyEvidence {
            time,
            text: text.to_string(),
        }
    }

    fn parsed(
        source_game: LegacyCreatureGame,
        path: &str,
        source_skeleton_path: &str,
        name: &str,
        cycle: SequenceCycle,
        text_keys: Vec<TextKeyEvidence>,
        root_motion: RootMotionEvidence,
        targets: &[&str],
        compact_spline: bool,
    ) -> CreatureKfEvidence {
        CreatureKfEvidence {
            source_game,
            source_kf: path.to_string(),
            sequence: KfParseEvidence::Parsed(NiControllerSequenceEvidence {
                sequence_index: 0,
                name: name.to_string(),
                cycle,
                start_time: 0.0,
                stop_time: text_keys.last().map_or(1.0, |event| event.time),
                frequency: 1.0,
                text_keys,
                binding: BindingEvidence {
                    source_skeleton_path: source_skeleton_path.to_string(),
                    transform_track_count: 1,
                    float_track_count: 0,
                    controller_types: vec!["NiTransformController".to_string()],
                    interpolator_types: vec![if compact_spline {
                        "NiBSplineCompTransformInterpolator".to_string()
                    } else {
                        "NiTransformInterpolator".to_string()
                    }],
                    target_names: targets.iter().map(|target| (*target).to_string()).collect(),
                    required_float_slots: Vec::new(),
                    compatibility: BindingCompatibility::Verified,
                    compatibility_detail: None,
                },
                root_motion,
            }),
            idle_claims: Vec::new(),
        }
    }

    fn role(family: &CreatureFamilyMotionSet, role: MotionRole) -> &MotionRoleCandidates {
        family
            .roles
            .iter()
            .find(|candidates| candidates.role == role)
            .unwrap()
    }

    #[test]
    fn looping_stationary_creature_aim_is_a_viable_idle() {
        let skeleton = "creatures\\centaur\\skeleton.nif";
        let aim = "creatures\\centaur\\h2haim.kf";
        let family = CreatureMotionFamilyEvidence {
            rig: rig(LegacyCreatureGame::Fnv, skeleton),
            referenced_kfs: vec![aim.to_string()],
            kf_evidence: vec![parsed(
                LegacyCreatureGame::Fnv,
                aim,
                skeleton,
                "Aim",
                SequenceCycle::Loop,
                vec![event(0.0, "start"), event(2.0, "end")],
                RootMotionEvidence::Stationary {
                    accum_root: Some("Bip01".to_string()),
                },
                &["Bip01 Pelvis"],
                false,
            )],
            creature_traits: vec![creature_trait(
                CreatureMotionTrait::Walks,
                0x00_0188,
                "FalloutNV.esm",
                "Walks",
            )],
        };

        let catalog = build_creature_motion_catalog(&[family]).unwrap();
        let idle = role(&catalog.families[0], MotionRole::Idle);
        assert_eq!(idle.readiness, RoleReadiness::Ready);
        assert_eq!(idle.candidates[0].source_kf, aim);
        assert!(idle.candidates[0].looping);
    }

    fn legacy_asset(game: &str, relative_path: &str) -> Option<PathBuf> {
        let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .find(|path| path.join("extracted").join(game).is_dir())?;
        let path = repo_root.join("extracted").join(game).join(relative_path);
        path.is_file().then_some(path)
    }

    #[test]
    fn real_fnv_gecko_kf_adapter_is_unverified_without_and_verified_with_rig_contract() {
        let source_kf = "creatures\\nvgecko\\mtidle.kf";
        let Some(kf_path) = legacy_asset("fnv", "meshes/creatures/nvgecko/mtidle.kf") else {
            return;
        };
        let Some(skeleton_path) = legacy_asset("fnv", "meshes/creatures/nvgecko/skeleton.nif")
        else {
            return;
        };
        let bytes = std::fs::read(kf_path).unwrap();
        let unverified =
            parse_creature_kf_evidence(&bytes, source_kf, LegacyCreatureGame::Fnv, None, None)
                .unwrap();
        let KfParseEvidence::Parsed(sequence) = &unverified.sequence else {
            panic!("real Gecko KF was not parsed")
        };
        assert_eq!(unverified.source_game, LegacyCreatureGame::Fnv);
        assert_eq!(sequence.sequence_index, 0);
        assert_eq!(sequence.name, "Idle");
        assert_eq!(sequence.cycle, SequenceCycle::Loop);
        assert_eq!(sequence.start_time, 0.0);
        assert!((sequence.stop_time - 13.333_333_969_116_211).abs() < 1.0e-9);
        assert_eq!(sequence.frequency, 1.0);
        assert_eq!(sequence.binding.transform_track_count, 84);
        assert_eq!(sequence.binding.float_track_count, 0);
        assert!(sequence.binding.required_float_slots.is_empty());
        assert_eq!(
            sequence.binding.compatibility,
            BindingCompatibility::Unverified
        );
        assert!(
            sequence
                .binding
                .compatibility_detail
                .as_deref()
                .is_some_and(
                    |detail| detail.contains("missing preserve-source-rig skeleton contract")
                )
        );
        assert!(matches!(
            sequence.root_motion,
            RootMotionEvidence::Stationary {
                accum_root: Some(ref root)
            } if root == "Bip01"
        ));
        let unverified_catalog = build_creature_motion_catalog(&[CreatureMotionFamilyEvidence {
            rig: rig(LegacyCreatureGame::Fnv, "creatures\\nvgecko\\skeleton.nif"),
            referenced_kfs: vec![source_kf.to_string()],
            kf_evidence: vec![unverified.clone()],
            creature_traits: Vec::new(),
        }])
        .unwrap();
        assert!(matches!(
            unverified_catalog.families[0].readiness,
            FamilyMotionReadiness::Incomplete { .. }
        ));
        assert!(matches!(
            unverified_catalog.families[0].kf_accounting[0].disposition,
            KfAccountingDisposition::BindingUnverified(_)
        ));

        let emitted_skeleton_path = "Actors\\B21_FNVGecko\\CharacterAssets\\Skeleton.hkx";
        let skeleton_receipt = crate::phase::skeleton::convert_source_owned_skeleton_artifact(
            &skeleton_path,
            emitted_skeleton_path,
            "NVGecko",
            &[],
        )
        .unwrap();
        assert_eq!(skeleton_receipt.ordered_bone_names.len(), 87);
        let verified = parse_creature_kf_evidence(
            &bytes,
            source_kf,
            LegacyCreatureGame::Fnv,
            None,
            Some(CreatureKfSkeletonContract {
                skeleton_path: emitted_skeleton_path,
                ordered_bone_names: &skeleton_receipt.ordered_bone_names,
                ordered_float_slot_names: &[],
            }),
        )
        .unwrap();
        let verified_catalog = build_creature_motion_catalog(&[CreatureMotionFamilyEvidence {
            rig: rig(LegacyCreatureGame::Fnv, "creatures\\nvgecko\\skeleton.nif"),
            referenced_kfs: vec![source_kf.to_string()],
            kf_evidence: vec![verified.clone()],
            creature_traits: Vec::new(),
        }])
        .unwrap();
        let candidate = &role(&verified_catalog.families[0], MotionRole::Idle).candidates[0];
        let event_map = HashMap::new();
        let request = stage_request_from_motion_candidate(
            candidate,
            emitted_skeleton_path,
            &skeleton_receipt,
            "Actors\\B21_FNVGecko\\Animations\\Idle.hkx",
            &event_map,
            None,
        )
        .unwrap();
        assert_eq!(request.sequence_index, Some(0));
        assert_eq!(request.skeleton.skeleton_path, emitted_skeleton_path);
        assert_eq!(
            request.extracted_motion_policy,
            FnvExtractedMotionPolicy::RejectNonzero
        );
        let KfParseEvidence::Parsed(sequence) = verified.sequence else {
            panic!("real Gecko KF was not parsed")
        };
        assert_eq!(
            sequence.binding.compatibility,
            BindingCompatibility::Verified
        );
        assert_eq!(sequence.binding.source_skeleton_path, emitted_skeleton_path);
        assert!(sequence.binding.compatibility_detail.is_none());
    }

    #[test]
    fn real_fo3_mirelurk_kf_adapter_preserves_sequence_events_and_binding() {
        let source_kf = "creatures\\mirelurk\\locomotion\\mtforward.kf";
        let Some(kf_path) =
            legacy_asset("fo3", "meshes/creatures/mirelurk/locomotion/mtforward.kf")
        else {
            return;
        };
        let Some(skeleton_path) = legacy_asset("fo3", "meshes/creatures/mirelurk/skeleton.nif")
        else {
            return;
        };
        let bytes = std::fs::read(kf_path).unwrap();
        let unverified =
            parse_creature_kf_evidence(&bytes, source_kf, LegacyCreatureGame::Fo3, None, None)
                .unwrap();
        let KfParseEvidence::Parsed(sequence) = &unverified.sequence else {
            panic!("real Mirelurk KF was not parsed")
        };
        assert_eq!(unverified.source_game, LegacyCreatureGame::Fo3);
        assert_eq!(sequence.sequence_index, 0);
        assert_eq!(sequence.name, "Forward");
        assert_eq!(sequence.cycle, SequenceCycle::Loop);
        assert_eq!(sequence.start_time, 0.0);
        assert!((sequence.stop_time - 1.666_666_746_139_526_4).abs() < 1.0e-9);
        assert_eq!(sequence.frequency, 1.0);
        assert_eq!(sequence.binding.transform_track_count, 45);
        assert_eq!(
            sequence
                .text_keys
                .iter()
                .map(|event| (event.time, event.text.as_str()))
                .collect::<Vec<_>>(),
            vec![
                (0.0, "start"),
                (0.166_666_671_633_720_4, "m:R"),
                (0.5, "Enum: Right"),
                (1.000_000_119_209_289_6, "m:L"),
                (1.333_333_373_069_763_2, "Enum: Left"),
                (1.666_666_746_139_526_4, "end"),
            ]
        );
        assert!(matches!(
            sequence.root_motion,
            RootMotionEvidence::Planar { distance, .. } if distance > 1.0e-4
        ));

        let ordered_bone_names =
            crate::phase::animations::load_ordered_source_skeleton_names(&skeleton_path).unwrap();
        let emitted_skeleton_path = "Actors\\B21_FO3Mirelurk\\CharacterAssets\\Skeleton.hkx";
        let verified = parse_creature_kf_evidence(
            &bytes,
            source_kf,
            LegacyCreatureGame::Fo3,
            Some(0),
            Some(CreatureKfSkeletonContract {
                skeleton_path: emitted_skeleton_path,
                ordered_bone_names: &ordered_bone_names,
                ordered_float_slot_names: &[],
            }),
        )
        .unwrap();
        let KfParseEvidence::Parsed(sequence) = verified.sequence else {
            panic!("real Mirelurk KF was not parsed")
        };
        assert_eq!(
            sequence.binding.compatibility,
            BindingCompatibility::Verified
        );
        assert_eq!(sequence.binding.source_skeleton_path, emitted_skeleton_path);
    }

    #[test]
    fn real_gecko_animal_roles_use_sequence_events_and_root_motion() {
        let skeleton = "creatures\\NVGecko\\skeleton.nif";
        let idle = "creatures\\nvgecko\\mtidle.kf";
        let forward = "creatures\\nvgecko\\locomotion\\mtforward.kf";
        let turn = "creatures\\nvgecko\\mtturnleft.kf";
        let attack = "creatures\\nvgecko\\h2hattackright.kf";
        let hurt = "creatures\\nvgecko\\idleanims\\hitreaction_torso.kf";
        let family = CreatureMotionFamilyEvidence {
            rig: rig(LegacyCreatureGame::Fnv, skeleton),
            referenced_kfs: vec![
                attack.to_string(),
                forward.to_string(),
                hurt.to_string(),
                idle.to_string(),
                turn.to_string(),
            ],
            kf_evidence: vec![
                parsed(
                    LegacyCreatureGame::Fnv,
                    idle,
                    skeleton,
                    "Idle",
                    SequenceCycle::Loop,
                    vec![event(0.0, "start"), event(13.333_334, "end")],
                    RootMotionEvidence::Stationary {
                        accum_root: Some("Bip01".to_string()),
                    },
                    &["Bip01 Pelvis", "Bip01 NonAccum"],
                    true,
                ),
                parsed(
                    LegacyCreatureGame::Fnv,
                    forward,
                    skeleton,
                    "Forward",
                    SequenceCycle::Loop,
                    vec![
                        event(0.0, "start"),
                        event(0.333_333, "Enum: Auxiliary1"),
                        event(0.666_667, "Enum: Left"),
                        event(1.0, "Enum: Right"),
                        event(1.333_333, "end"),
                    ],
                    RootMotionEvidence::Planar {
                        accum_root: "Bip01".to_string(),
                        distance: 91.0,
                        yaw_radians: 0.0,
                    },
                    &["Bip01 Pelvis"],
                    false,
                ),
                parsed(
                    LegacyCreatureGame::Fnv,
                    turn,
                    skeleton,
                    "TurnLeft",
                    SequenceCycle::Loop,
                    vec![event(0.0, "start"), event(1.0, "end")],
                    RootMotionEvidence::Planar {
                        accum_root: "Bip01".to_string(),
                        distance: 0.0,
                        yaw_radians: -1.570_796,
                    },
                    &["Bip01 Pelvis"],
                    true,
                ),
                parsed(
                    LegacyCreatureGame::Fnv,
                    attack,
                    skeleton,
                    "AttackRight",
                    SequenceCycle::Clamp,
                    vec![
                        event(0.0, "start"),
                        event(0.933_333_4, "Hit"),
                        event(1.333_333, "a:L"),
                        event(1.666_667, "end"),
                    ],
                    RootMotionEvidence::Stationary {
                        accum_root: Some("Bip01".to_string()),
                    },
                    &["Bip01 Pelvis"],
                    false,
                ),
                parsed(
                    LegacyCreatureGame::Fnv,
                    hurt,
                    skeleton,
                    "SpecialIdle_HitReactionTorso",
                    SequenceCycle::Clamp,
                    vec![event(0.0, "start"), event(2.0, "end")],
                    RootMotionEvidence::Stationary {
                        accum_root: Some("Bip01".to_string()),
                    },
                    &["Bip01 Pelvis"],
                    false,
                ),
            ],
            creature_traits: vec![creature_trait(
                CreatureMotionTrait::Walks,
                0x10_CD73,
                "FalloutNV.esm",
                "Walks",
            )],
        };

        let catalog = build_creature_motion_catalog(&[family]).unwrap();
        let family = &catalog.families[0];
        assert_eq!(family.readiness, FamilyMotionReadiness::Ready);
        assert_eq!(role(family, MotionRole::Idle).candidates[0].source_kf, idle);
        assert_eq!(
            role(family, MotionRole::GroundLocomotion).candidates[0].source_kf,
            forward
        );
        assert_eq!(role(family, MotionRole::Turn).candidates[0].source_kf, turn);
        let attack = &role(family, MotionRole::MeleeAttack).candidates[0];
        assert_eq!(attack.source_kf, "creatures\\nvgecko\\h2hattackright.kf");
        assert!(attack.events.iter().any(|event| {
            event.kind == MotionEventKind::Hit
                && event.raw_text == "Hit"
                && event.time == 0.933_333_4
        }));
        assert_eq!(role(family, MotionRole::Hurt).candidates[0].source_kf, hurt);
        assert_eq!(catalog.accounting.referenced_kfs, 5);
        assert_eq!(catalog.accounting.classified_kfs, 5);
    }

    #[test]
    fn real_securitron_robot_keeps_ranged_projectile_and_hash_overlays() {
        let skeleton = "creatures\\NVSecuritron\\Skeleton.nif";
        let attack = "creatures\\nvsecuritron\\1hpattackleft.kf";
        let aim = "creatures\\nvsecuritron\\1hpaim.kf";
        let idle = "creatures\\nvsecuritron\\mtidle.kf";
        let death = "creatures\\nvsecuritron\\death.kf";
        let family = CreatureMotionFamilyEvidence {
            rig: rig(LegacyCreatureGame::Fnv, skeleton),
            referenced_kfs: vec![
                attack.to_string(),
                aim.to_string(),
                death.to_string(),
                idle.to_string(),
            ],
            kf_evidence: vec![
                parsed(
                    LegacyCreatureGame::Fnv,
                    attack,
                    skeleton,
                    "AttackLeft",
                    SequenceCycle::Clamp,
                    vec![
                        event(0.0, "start"),
                        event(0.2, "Hit"),
                        event(0.3, "Blend:1"),
                        event(0.4, "Eject"),
                        event(0.7, "end"),
                    ],
                    RootMotionEvidence::Stationary {
                        accum_root: Some("Bip01".to_string()),
                    },
                    &["Bip01 Pelvis"],
                    true,
                ),
                parsed(
                    LegacyCreatureGame::Fnv,
                    aim,
                    skeleton,
                    "AIM",
                    SequenceCycle::Loop,
                    vec![event(0.0, "start"), event(2.0, "end")],
                    RootMotionEvidence::Stationary {
                        accum_root: Some("Bip01".to_string()),
                    },
                    &["##VoiceBox_talk", "##VoiceBox_talk:0", "Bip01 Pelvis"],
                    true,
                ),
                parsed(
                    LegacyCreatureGame::Fnv,
                    idle,
                    skeleton,
                    "Idle",
                    SequenceCycle::Loop,
                    vec![event(0.0, "start"), event(2.0, "end")],
                    RootMotionEvidence::Stationary {
                        accum_root: Some("Bip01".to_string()),
                    },
                    &["Bip01 Pelvis"],
                    true,
                ),
                parsed(
                    LegacyCreatureGame::Fnv,
                    death,
                    skeleton,
                    "Death",
                    SequenceCycle::Clamp,
                    vec![event(0.0, "start"), event(2.0, "end")],
                    RootMotionEvidence::Stationary {
                        accum_root: Some("Bip01".to_string()),
                    },
                    &["Bip01 Pelvis"],
                    true,
                ),
            ],
            creature_traits: vec![
                creature_trait(
                    CreatureMotionTrait::Walks,
                    0x15_8646,
                    "FalloutNV.esm",
                    "Walks",
                ),
                creature_trait(
                    CreatureMotionTrait::Robot,
                    0x15_8646,
                    "FalloutNV.esm",
                    "Robot",
                ),
            ],
        };

        let catalog = build_creature_motion_catalog(&[family]).unwrap();
        let family = &catalog.families[0];
        assert_eq!(role(family, MotionRole::RangedAttack).candidates.len(), 1);
        assert_eq!(role(family, MotionRole::Projectile).candidates.len(), 1);
        assert_eq!(role(family, MotionRole::Fire).candidates.len(), 1);
        assert_eq!(role(family, MotionRole::Idle).candidates[0].source_kf, idle);
        assert_eq!(
            role(family, MotionRole::Death).candidates[0].source_kf,
            death
        );
        assert!(
            role(family, MotionRole::ContinuousOrRobot)
                .candidates
                .iter()
                .any(|candidate| candidate.source_kf == aim)
        );
        assert_eq!(
            role(family, MotionRole::Overlay).candidates[0]
                .binding
                .overlay_targets,
            vec!["##VoiceBox_talk", "##VoiceBox_talk:0"]
        );
        assert!(matches!(
            family.readiness,
            FamilyMotionReadiness::Incomplete { .. }
        ));
    }

    #[test]
    fn real_immobile_spore_plant_is_stationary_without_fake_locomotion() {
        let skeleton = "creatures\\NVSporePlant\\skeleton.nif";
        let idle = "creatures\\nvsporeplant\\mtidle.kf";
        let attack = "creatures\\nvsporeplant\\2hrattackleft.kf";
        let family = CreatureMotionFamilyEvidence {
            rig: rig(LegacyCreatureGame::Fnv, skeleton),
            referenced_kfs: vec![attack.to_string(), idle.to_string()],
            kf_evidence: vec![
                parsed(
                    LegacyCreatureGame::Fnv,
                    idle,
                    skeleton,
                    "Idle",
                    SequenceCycle::Loop,
                    vec![event(0.0, "start"), event(2.0, "end")],
                    RootMotionEvidence::Stationary {
                        accum_root: Some("Bip01".to_string()),
                    },
                    &["Bip01 Pelvis"],
                    true,
                ),
                parsed(
                    LegacyCreatureGame::Fnv,
                    attack,
                    skeleton,
                    "AttackLeft",
                    SequenceCycle::Clamp,
                    vec![
                        event(0.0, "start"),
                        event(0.2, "Sound: NPCSporePlantMvmt"),
                        event(0.5, "Sound: NPCSporePlantAttack"),
                        event(0.8, "Hit"),
                        event(1.0, "a:L"),
                        event(1.2, "end"),
                    ],
                    RootMotionEvidence::Stationary {
                        accum_root: Some("Bip01".to_string()),
                    },
                    &["Bip01 Pelvis"],
                    false,
                ),
            ],
            creature_traits: vec![creature_trait(
                CreatureMotionTrait::Stationary,
                0x0F_E036,
                "FalloutNV.esm",
                "Immobile",
            )],
        };

        let catalog = build_creature_motion_catalog(&[family]).unwrap();
        let family = &catalog.families[0];
        assert_eq!(
            role(family, MotionRole::StationaryOrTurret)
                .candidates
                .len(),
            2
        );
        assert!(
            role(family, MotionRole::GroundLocomotion)
                .candidates
                .is_empty()
        );
        assert_eq!(
            role(family, MotionRole::MeleeAttack).candidates[0].source_kf,
            attack
        );
        assert_eq!(family.readiness, FamilyMotionReadiness::Ready);
    }

    #[test]
    fn real_flying_and_swimming_families_use_crea_traits_before_filenames() {
        let blowfly_skeleton = "Creatures\\Blowfly\\Skeleton.nif";
        let fly = "creatures\\blowfly\\mtforward.kf";
        let mirelurk_skeleton = "creatures\\mirelurkking\\skeleton.nif";
        let ground = "creatures\\mirelurkking\\locomotion\\mtforward.kf";
        let swim = "creatures\\mirelurkking\\swimmtforward.kf";
        let blowfly = CreatureMotionFamilyEvidence {
            rig: rig(LegacyCreatureGame::Fnv, blowfly_skeleton),
            referenced_kfs: vec![fly.to_string()],
            kf_evidence: vec![parsed(
                LegacyCreatureGame::Fnv,
                fly,
                blowfly_skeleton,
                "Forward",
                SequenceCycle::Loop,
                vec![
                    event(0.0, "start"),
                    event(0.2, "Blend:10"),
                    event(0.8, "end"),
                ],
                RootMotionEvidence::Planar {
                    accum_root: "Bip01".to_string(),
                    distance: 80.0,
                    yaw_radians: 0.0,
                },
                &["Bip01 Pelvis"],
                false,
            )],
            creature_traits: vec![
                creature_trait(
                    CreatureMotionTrait::Flies,
                    0x09_189C,
                    "FalloutNV.esm",
                    "Flies",
                ),
                creature_trait(
                    CreatureMotionTrait::Walks,
                    0x09_189C,
                    "FalloutNV.esm",
                    "Walks",
                ),
            ],
        };
        let mirelurk = CreatureMotionFamilyEvidence {
            rig: rig(LegacyCreatureGame::Fnv, mirelurk_skeleton),
            referenced_kfs: vec![ground.to_string(), swim.to_string()],
            kf_evidence: vec![
                parsed(
                    LegacyCreatureGame::Fnv,
                    ground,
                    mirelurk_skeleton,
                    "Forward",
                    SequenceCycle::Loop,
                    vec![event(0.0, "start"), event(1.0, "end")],
                    RootMotionEvidence::Planar {
                        accum_root: "Bip01".to_string(),
                        distance: 75.0,
                        yaw_radians: 0.0,
                    },
                    &["Bip01 Pelvis"],
                    true,
                ),
                parsed(
                    LegacyCreatureGame::Fnv,
                    swim,
                    mirelurk_skeleton,
                    "Forward",
                    SequenceCycle::Loop,
                    vec![
                        event(0.0, "start"),
                        event(0.3, "Blend: 30"),
                        event(1.0, "end"),
                    ],
                    RootMotionEvidence::Planar {
                        accum_root: "Bip01".to_string(),
                        distance: 60.0,
                        yaw_radians: 0.0,
                    },
                    &["Bip01 Pelvis"],
                    true,
                ),
            ],
            creature_traits: vec![
                creature_trait(
                    CreatureMotionTrait::Walks,
                    0x10_0001,
                    "FalloutNV.esm",
                    "Walks",
                ),
                creature_trait(
                    CreatureMotionTrait::Swims,
                    0x10_0001,
                    "FalloutNV.esm",
                    "Swims",
                ),
            ],
        };

        let catalog = build_creature_motion_catalog(&[mirelurk, blowfly]).unwrap();
        let blowfly = catalog
            .families
            .iter()
            .find(|family| {
                family
                    .rig
                    .skeleton_path
                    .eq_ignore_ascii_case(blowfly_skeleton)
            })
            .unwrap();
        assert_eq!(role(blowfly, MotionRole::Fly).candidates[0].source_kf, fly);
        assert!(
            role(blowfly, MotionRole::GroundLocomotion)
                .candidates
                .is_empty()
        );
        let mirelurk = catalog
            .families
            .iter()
            .find(|family| family.rig.skeleton_path == mirelurk_skeleton)
            .unwrap();
        assert_eq!(
            role(mirelurk, MotionRole::GroundLocomotion).candidates[0].source_kf,
            ground
        );
        assert_eq!(
            role(mirelurk, MotionRole::Swim).candidates[0].source_kf,
            swim
        );
    }

    #[test]
    fn real_ghoul_projectile_overlay_preserves_exact_hash_binding() {
        let skeleton = "creatures\\ghoul\\skeleton.nif";
        let throw = "creatures\\ghoul\\1gtattackthrow.kf";
        let family = CreatureMotionFamilyEvidence {
            rig: rig(LegacyCreatureGame::Fnv, skeleton),
            referenced_kfs: vec![throw.to_string()],
            kf_evidence: vec![parsed(
                LegacyCreatureGame::Fnv,
                throw,
                skeleton,
                "AttackThrow",
                SequenceCycle::Clamp,
                vec![
                    event(0.0, "start"),
                    event(0.4, "Sound: NPCFeralGhoulReaverThrow"),
                    event(0.8, "Hold"),
                    event(1.1, "Release"),
                    event(1.4, "Attach"),
                    event(2.0, "end"),
                ],
                RootMotionEvidence::Stationary {
                    accum_root: Some("Bip01".to_string()),
                },
                &["##DLC03GoreGrenade", "Bip01 R Hand"],
                true,
            )],
            creature_traits: vec![creature_trait(
                CreatureMotionTrait::HumanoidWeaponOverlay,
                0x01_0001,
                "FalloutNV.esm",
                "weapon overlay",
            )],
        };

        let catalog = build_creature_motion_catalog(&[family]).unwrap();
        let family = &catalog.families[0];
        for expected_role in [
            MotionRole::RangedAttack,
            MotionRole::Projectile,
            MotionRole::Overlay,
        ] {
            assert_eq!(role(family, expected_role).candidates[0].source_kf, throw);
        }
        assert_eq!(
            role(family, MotionRole::Overlay).candidates[0]
                .binding
                .overlay_targets,
            vec!["##DLC03GoreGrenade"]
        );
    }

    #[test]
    fn fo3_graft_and_idle_ancestry_are_exact_and_fully_accounted() {
        let target_skeleton = "creatures\\mirelurkking\\skeleton.nif";
        let fo3_forward = "creatures\\mirelurk\\locomotion\\mtforward.kf";
        let fo3_attack = "creatures\\mirelurk\\h2hattackright.kf";
        let fo3_feed = "creatures\\mirelurk\\idleanims\\specialidle_feed.kf";
        let missing = "creatures\\mirelurk\\idleanims\\specialidle_missing.kf";
        let attack = parsed(
            LegacyCreatureGame::Fo3,
            fo3_attack,
            "creatures\\mirelurk\\skeleton.nif",
            "AttackRight",
            SequenceCycle::Clamp,
            vec![
                event(0.0, "start"),
                event(0.2, "Blend:6"),
                event(0.4, "Sound: NPCMirelurkAttack"),
                event(0.6, "Hit"),
                event(0.8, "a:L"),
                event(1.0, "end"),
            ],
            RootMotionEvidence::Stationary {
                accum_root: Some("Bip01".to_string()),
            },
            &["Bip01 Pelvis"],
            true,
        );
        let mut feed = parsed(
            LegacyCreatureGame::Fo3,
            fo3_feed,
            "creatures\\mirelurk\\skeleton.nif",
            "SpecialIdle_Feed",
            SequenceCycle::Clamp,
            vec![event(0.0, "start"), event(4.0, "end")],
            RootMotionEvidence::Stationary {
                accum_root: Some("Bip01".to_string()),
            },
            &["Bip01 Pelvis"],
            true,
        );
        feed.idle_claims.push(IdleClaimEvidence {
            source_idle: key(0x07_2DC0, "Fallout3.esm"),
            ancestry: vec![
                IdleAncestryNode {
                    form_key: key(0x07_2DC0, "Fallout3.esm"),
                    editor_id: Some("MFeed".to_string()),
                    model_path: Some(
                        "Creatures\\Mirelurk\\IdleAnims\\SpecialIdle_Feed.kf".to_string(),
                    ),
                    parent: Some(key(0x03_5E51, "Fallout3.esm")),
                    previous: Some(key(0x07_2DC1, "Fallout3.esm")),
                    conditions: vec![
                        IdleConditionEvidence {
                            signature: "CTDA".to_string(),
                            raw_hex: "010000000000803F5B00000000000000000000000000000000000000"
                                .to_string(),
                        },
                        IdleConditionEvidence {
                            signature: "CTDA".to_string(),
                            raw_hex: "A00000000000A0414D00000000000000000000000000000000000000"
                                .to_string(),
                        },
                    ],
                },
                IdleAncestryNode {
                    form_key: key(0x03_5E51, "Fallout3.esm"),
                    editor_id: Some("MIdles".to_string()),
                    model_path: Some("Creatures\\Mirelurk\\IdleAnims".to_string()),
                    parent: None,
                    previous: Some(key(0x03_5E52, "Fallout3.esm")),
                    conditions: vec![
                        IdleConditionEvidence {
                            signature: "CTDA".to_string(),
                            raw_hex: "00000000000000002101000000000000000000000000000000000000"
                                .to_string(),
                        },
                        IdleConditionEvidence {
                            signature: "CTDA".to_string(),
                            raw_hex: "00000000000000001900000000000000000000000000000000000000"
                                .to_string(),
                        },
                        IdleConditionEvidence {
                            signature: "CTDA".to_string(),
                            raw_hex: "00000000000000006500000000000000000000000000000000000000"
                                .to_string(),
                        },
                    ],
                },
            ],
        });
        let family = CreatureMotionFamilyEvidence {
            rig: rig(LegacyCreatureGame::Fnv, target_skeleton),
            referenced_kfs: vec![
                fo3_attack.to_string(),
                fo3_feed.to_string(),
                fo3_forward.to_string(),
                missing.to_string(),
            ],
            kf_evidence: vec![
                parsed(
                    LegacyCreatureGame::Fo3,
                    fo3_forward,
                    "creatures\\mirelurk\\skeleton.nif",
                    "Forward",
                    SequenceCycle::Loop,
                    vec![
                        event(0.0, "start"),
                        event(0.2, "m:R"),
                        event(0.4, "Enum: Right"),
                        event(0.6, "m:L"),
                        event(0.8, "Enum: Left"),
                        event(1.0, "end"),
                    ],
                    RootMotionEvidence::Planar {
                        accum_root: "Bip01".to_string(),
                        distance: 70.0,
                        yaw_radians: 0.0,
                    },
                    &["Bip01 Pelvis"],
                    true,
                ),
                attack,
                feed,
            ],
            creature_traits: vec![creature_trait(
                CreatureMotionTrait::Walks,
                0x01_CF83,
                "Fallout3.esm",
                "Walks",
            )],
        };

        let catalog = build_creature_motion_catalog(&[family]).unwrap();
        let family = &catalog.families[0];
        assert_eq!(family.readiness, FamilyMotionReadiness::Ready);
        assert_eq!(family.kf_accounting.len(), 4);
        assert_eq!(catalog.accounting.referenced_kfs, 4);
        assert_eq!(catalog.accounting.classified_kfs, 3);
        assert_eq!(catalog.accounting.unavailable_kfs, 1);
        let attack = &role(family, MotionRole::MeleeAttack).candidates[0];
        assert_eq!(attack.source_game, LegacyCreatureGame::Fo3);
        let feed = &role(family, MotionRole::Idle).candidates[0];
        assert_eq!(
            feed.idle_claims[0].ancestry[0].previous,
            Some(key(0x07_2DC1, "Fallout3.esm"))
        );
        assert_eq!(
            feed.idle_claims[0].ancestry[0].conditions[1].raw_hex,
            "A00000000000A0414D00000000000000000000000000000000000000"
        );
        assert!(feed.evidence.iter().any(|signal| {
            signal.source == RoleSignalSource::IdleAncestry
                && signal.detail.contains("035E51@Fallout3.esm")
        }));
    }

    #[test]
    fn equal_default_idle_candidates_remain_typed_ambiguity() {
        let skeleton = "creatures\\nvgecko\\skeleton.nif";
        let default_idle = "creatures\\nvgecko\\mtidle.kf";
        let combat_idle = "creatures\\nvgecko\\idleanims\\combatidle_hiss.kf";
        let family = CreatureMotionFamilyEvidence {
            rig: rig(LegacyCreatureGame::Fnv, skeleton),
            referenced_kfs: vec![default_idle.to_string(), combat_idle.to_string()],
            kf_evidence: vec![
                parsed(
                    LegacyCreatureGame::Fnv,
                    default_idle,
                    skeleton,
                    "Idle",
                    SequenceCycle::Loop,
                    vec![event(0.0, "start"), event(2.0, "end")],
                    RootMotionEvidence::Stationary {
                        accum_root: Some("Bip01".to_string()),
                    },
                    &["Bip01 Pelvis"],
                    true,
                ),
                parsed(
                    LegacyCreatureGame::Fnv,
                    combat_idle,
                    skeleton,
                    "CombatIdle_Hiss",
                    SequenceCycle::Clamp,
                    vec![event(0.0, "start"), event(1.0, "end")],
                    RootMotionEvidence::Stationary {
                        accum_root: Some("Bip01".to_string()),
                    },
                    &["Bip01 Pelvis"],
                    false,
                ),
            ],
            creature_traits: vec![creature_trait(
                CreatureMotionTrait::Walks,
                0x10_CD73,
                "FalloutNV.esm",
                "Walks",
            )],
        };

        let catalog = build_creature_motion_catalog(&[family]).unwrap();
        let family = &catalog.families[0];
        assert_eq!(
            role(family, MotionRole::Idle).readiness,
            RoleReadiness::Ambiguous(vec![combat_idle.to_string(), default_idle.to_string(),])
        );
        assert!(matches!(
            family.readiness,
            FamilyMotionReadiness::Incomplete { .. }
        ));
    }

    #[test]
    fn unreferenced_or_duplicate_kf_evidence_is_rejected() {
        let family = CreatureMotionFamilyEvidence {
            rig: rig(LegacyCreatureGame::Fnv, "creatures\\dog\\skeleton.nif"),
            referenced_kfs: vec![
                "creatures\\dog\\mtidle.kf".to_string(),
                "CREATURES/dog/MTIDLE.KF".to_string(),
            ],
            kf_evidence: Vec::new(),
            creature_traits: Vec::new(),
        };
        assert!(matches!(
            build_creature_motion_catalog(&[family]),
            Err(CreatureMotionCatalogError::DuplicateReferencedKf { .. })
        ));
    }
}
