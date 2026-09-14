//! Canonical, read-only FNV/FO3 creature-family evidence recipes.
//!
//! This module deliberately has no phase or publication wiring. It converts an
//! explicit winning-record corpus and indexed source evidence into a stable
//! ledger that a later implementation can consume without rediscovering facts
//! from names or target records.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::record::Record;
use crate::source_rig::race_data::{
    Fo4RaceDataDerivation, Fo4RaceDataScalePolicy, RaceDataEvidenceField,
    SourceFamilyRaceDataEvidence, derive_fo4_race_data,
};
use crate::sym::StringInterner;

use super::creature_catalog::{
    BodyVariantKey, CreatureCatalogError, CreatureCatalogOptions, CreatureDisposition,
    CreatureProvenance, LegacyCreatureGame, LegacyRecordSource, ProxyBlocker, ProxyReadiness,
    RigFamilyKey, SpecialCreatureReason, StableFormKey, build_creature_corpus_plan,
};
use super::creature_motion::{
    BindingCompatibility, CreatureFamilyMotionSet, CreatureMotionCatalogError,
    CreatureMotionFamilyEvidence, KfParseEvidence, MotionCandidate, MotionRole, RoleReadiness,
    RootMotionEvidence, build_creature_motion_catalog,
};
use super::creature_race_data::{
    CreatureRaceDataEvidence, FamilyRaceDataSelection, LegacyMovementBaseSettings,
    LegacyRaceDataEvidenceInputs, RaceDataFidelityReceipt, build_family_race_data_selections,
    load_creature_record_race_data_evidence, load_legacy_movement_base_settings,
};

pub const CREATURE_RECIPE_SCHEMA_VERSION: u32 = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IndexedAssetKind {
    Skeleton,
    Body,
    Kf,
    Material,
    Texture,
    Ragdoll,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndexedSourceAssetEvidence {
    pub game: LegacyCreatureGame,
    pub path: String,
    pub kind: IndexedAssetKind,
    pub content_hash_blake3: String,
    pub dependencies: Vec<String>,
    /// Exact roots decoded from a skeleton NIF. Non-skeleton assets must leave
    /// this empty; a family skeleton must expose exactly one actual root.
    pub root_nodes: Vec<String>,
    /// Exact `hkaSkeleton.name` reconstructed from this skeleton. This is
    /// explicit source evidence and is never inferred from the asset path.
    pub skeleton_runtime_name: Option<String>,
    /// Exact ordered `hkaSkeleton.floatSlots` reconstructed from this skeleton.
    pub skeleton_float_slots: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CreatureGraphCapability {
    PassiveGround,
    GroundMelee,
    GroundRangedProjectile,
    GroundMeleeRanged,
    GroundSwim,
    GroundFly,
    Swim,
    Fly,
    StationaryTurret,
    RobotContinuousAttack,
    HumanoidWeaponOverlay,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreatureGraphContractEvidence {
    pub rig: RigFamilyKey,
    pub capability: CreatureGraphCapability,
    pub required_roles: Vec<MotionRole>,
    pub requires_ragdoll: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RagdollMode {
    EmbeddedSkeleton,
    ExternalAsset,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "capability", rename_all = "snake_case")]
pub enum RagdollCapabilityEvidence {
    Supported {
        rig: RigFamilyKey,
        mode: RagdollMode,
        owner_asset: String,
    },
    ExplicitlyUnsupported {
        rig: RigFamilyKey,
        reason: String,
    },
}

impl RagdollCapabilityEvidence {
    fn rig(&self) -> &RigFamilyKey {
        match self {
            Self::Supported { rig, .. } | Self::ExplicitlyUnsupported { rig, .. } => rig,
        }
    }
}

#[derive(Clone, Copy)]
pub struct CreatureRecipeBuildInput<'a> {
    pub winning_records: &'a [LegacyRecordSource<'a>],
    pub indexed_assets: &'a [IndexedSourceAssetEvidence],
    pub motion_families: &'a [CreatureMotionFamilyEvidence],
    pub graph_contracts: &'a [CreatureGraphContractEvidence],
    pub ragdoll_evidence: &'a [RagdollCapabilityEvidence],
    pub race_data: LegacyRaceDataEvidenceInputs<'a>,
    pub race_data_policy: &'a Fo4RaceDataScalePolicy,
    pub expected_creature_winners: Option<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "disposition", rename_all = "snake_case")]
pub enum CreatureRecordRecipeDisposition {
    VisualOwner {
        rig: RigFamilyKey,
        body: BodyVariantKey,
    },
    Proxy {
        rig: RigFamilyKey,
        terminal_visual_owners: Vec<StableFormKey>,
    },
    RejectedProxy {
        blockers: Vec<RecordRecipeBlocker>,
    },
    Special {
        reason: SpecialCreatureReason,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum RecordRecipeBlocker {
    Catalog(ProxyBlocker),
    MissingTerminalOwner(StableFormKey),
    TerminalOwnerIsNotVisual(StableFormKey),
    RequiresLeveledTemplateProjection(Vec<StableFormKey>),
    AmbiguousFamilies(Vec<RigFamilyKey>),
    NoResolvedFamily,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreatureRecordRecipeEntry {
    pub source: StableFormKey,
    pub editor_id: Option<String>,
    pub provenance: CreatureProvenance,
    pub record_dependencies: Vec<StableFormKey>,
    pub disposition: CreatureRecordRecipeDisposition,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MovementSettingsReceipt {
    pub game: LegacyCreatureGame,
    pub f_move_base_speed_source: StableFormKey,
    pub locomotion_multiplier_source: StableFormKey,
    pub f_move_base_speed_bits: u32,
    pub locomotion_multiplier_bits: u32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FamilyRaceDataRecipe {
    pub selection: FamilyRaceDataSelection,
    pub measured_evidence: SourceFamilyRaceDataEvidence,
    pub derivation: Fo4RaceDataDerivation,
    pub policy_receipts: Vec<RaceDataFidelityReceipt>,
    pub movement_settings: MovementSettingsReceipt,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RequiredMotionRoleRecipe {
    pub role: MotionRole,
    pub candidates: Vec<MotionCandidate>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecursiveAssetClaim {
    pub root: String,
    pub closure: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CanonicalAssetClaim {
    pub game: LegacyCreatureGame,
    pub path: String,
    pub kind: IndexedAssetKind,
    pub content_hash_blake3: String,
    pub dependencies: Vec<String>,
    pub root_nodes: Vec<String>,
    pub skeleton_runtime_name: Option<String>,
    pub skeleton_float_slots: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum FamilyRecipeBlocker {
    MissingGraphContract,
    DuplicateGraphContract,
    MissingRagdollEvidence,
    DuplicateRagdollEvidence,
    UnsupportedGraphCapability(CreatureGraphCapability),
    MissingCapabilityRole(MotionRole),
    UnexpectedCapabilityRole(MotionRole),
    DuplicateCapabilityRole(MotionRole),
    MissingMotionFamily,
    MotionFamilyNotReady(String),
    MissingRequiredRole(MotionRole),
    AmbiguousRequiredRole {
        role: MotionRole,
        candidates: Vec<String>,
    },
    FilenameOnlyRoleEvidence {
        role: MotionRole,
        source_kf: String,
    },
    InvalidMotionEvidence {
        source_kf: String,
        detail: String,
    },
    MissingIdleRecord(StableFormKey),
    WrongIdleSignature {
        source: StableFormKey,
        signature: String,
    },
    MissingAsset(String),
    AmbiguousAsset(String),
    InvalidAssetEvidence {
        path: String,
        detail: String,
    },
    AssetGameMismatch {
        path: String,
        expected: LegacyCreatureGame,
        actual: LegacyCreatureGame,
    },
    AssetKindMismatch {
        path: String,
        expected: IndexedAssetKind,
        actual: IndexedAssetKind,
    },
    MissingActualRoot(String),
    AmbiguousActualRoot {
        path: String,
        roots: Vec<String>,
    },
    MissingFamilySelection,
    DuplicateFamilySelection,
    MissingRigRaceData,
    DuplicateRigRaceData,
    MissingBodyRaceData,
    DuplicateBodyRaceData,
    MissingControllerRaceData,
    DuplicateControllerRaceData,
    MissingCreatureRaceData,
    DuplicateCreatureRaceData,
    MissingRaceDataFields(Vec<RaceDataEvidenceField>),
    InvalidRaceData(String),
    MissingCapsule,
    MissingMovementSettings(LegacyCreatureGame),
    AmbiguousMovementSettings(LegacyCreatureGame),
    InvalidMovementSettings(LegacyCreatureGame),
    MissingRagdollOwner(String),
    RagdollOwnerKindMismatch {
        path: String,
        actual: IndexedAssetKind,
    },
    RagdollRequiredButUnsupported,
    UnsupportedOverlayChannels(String),
    MissingScalarSlotEvidence(String),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "disposition", rename_all = "snake_case")]
pub enum FamilyRecipeDisposition {
    Ready,
    Rejected { blockers: Vec<FamilyRecipeBlocker> },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CreatureFamilyRecipeJob {
    pub rig: RigFamilyKey,
    pub source_plugins: Vec<String>,
    pub members: Vec<StableFormKey>,
    pub actual_root_node: Option<String>,
    pub body_variants: Vec<BodyVariantKey>,
    pub graph_contract: Option<CreatureGraphContractEvidence>,
    pub indexed_motion_evidence: Option<CreatureMotionFamilyEvidence>,
    pub motion_evidence: Option<CreatureFamilyMotionSet>,
    pub required_motion: Vec<RequiredMotionRoleRecipe>,
    pub ragdoll: Option<RagdollCapabilityEvidence>,
    pub race_data: Option<FamilyRaceDataRecipe>,
    pub recursive_asset_claims: Vec<RecursiveAssetClaim>,
    pub assets: Vec<CanonicalAssetClaim>,
    pub disposition: FamilyRecipeDisposition,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreatureRecipeAccounting {
    pub winning_creatures: usize,
    pub record_dispositions: usize,
    pub fnv_creatures: usize,
    pub fo3_creatures: usize,
    pub rig_families: usize,
    pub family_jobs: usize,
    pub ready_families: usize,
    pub rejected_families: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreatureFamilyRecipeLedger {
    pub schema_version: u32,
    pub records: Vec<CreatureRecordRecipeEntry>,
    pub families: Vec<CreatureFamilyRecipeJob>,
    pub accounting: CreatureRecipeAccounting,
    pub content_hash_blake3: String,
}

impl CreatureFamilyRecipeLedger {
    pub fn canonical_json(&self) -> Result<String, CreatureRecipeBuildError> {
        self.validate()?;
        serde_json::to_string(self)
            .map_err(|error| CreatureRecipeBuildError::Serialization(error.to_string()))
    }

    pub fn from_json(json: &str) -> Result<Self, CreatureRecipeBuildError> {
        let ledger: Self = serde_json::from_str(json)
            .map_err(|error| CreatureRecipeBuildError::Serialization(error.to_string()))?;
        ledger.validate()?;
        Ok(ledger)
    }

    pub fn validate(&self) -> Result<(), CreatureRecipeBuildError> {
        if self.schema_version != CREATURE_RECIPE_SCHEMA_VERSION {
            return Err(CreatureRecipeBuildError::InvalidLedger(format!(
                "unsupported schema version {}; expected {}",
                self.schema_version, CREATURE_RECIPE_SCHEMA_VERSION
            )));
        }
        if !strictly_sorted_unique(self.records.iter().map(|record| &record.source)) {
            return Err(CreatureRecipeBuildError::InvalidLedger(
                "record order is not canonical and unique".to_string(),
            ));
        }
        if !strictly_sorted_unique(self.families.iter().map(|family| &family.rig)) {
            return Err(CreatureRecipeBuildError::InvalidLedger(
                "family order is not canonical and unique".to_string(),
            ));
        }
        let expected_accounting = account(&self.records, &self.families);
        validate_accounting(&expected_accounting)?;
        if self.accounting != expected_accounting {
            return Err(CreatureRecipeBuildError::InvalidLedger(
                "accounting differs from record and family dispositions".to_string(),
            ));
        }
        validate_ledger_references_and_order(self)?;
        if !canonical_blake3_hash(&self.content_hash_blake3) {
            return Err(CreatureRecipeBuildError::InvalidLedger(
                "ledger content hash is not lowercase 64-hex BLAKE3".to_string(),
            ));
        }
        let actual = self.compute_content_hash_blake3()?;
        if self.content_hash_blake3 != actual {
            return Err(CreatureRecipeBuildError::HashMismatch {
                expected: self.content_hash_blake3.clone(),
                actual,
            });
        }
        Ok(())
    }

    pub fn canonical_hash_blake3(&self) -> Result<String, CreatureRecipeBuildError> {
        self.validate()?;
        Ok(self.content_hash_blake3.clone())
    }

    fn compute_content_hash_blake3(&self) -> Result<String, CreatureRecipeBuildError> {
        let mut unhashed = self.clone();
        unhashed.content_hash_blake3.clear();
        let bytes = serde_json::to_vec(&unhashed)
            .map_err(|error| CreatureRecipeBuildError::Serialization(error.to_string()))?;
        Ok(blake3::hash(&bytes).to_hex().to_string())
    }
}

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum CreatureRecipeBuildError {
    #[error(transparent)]
    Catalog(#[from] CreatureCatalogError),
    #[error(transparent)]
    Motion(#[from] CreatureMotionCatalogError),
    #[error("recipe accounting drifted: {0}")]
    Accounting(String),
    #[error("creature recipe ledger is invalid: {0}")]
    InvalidLedger(String),
    #[error("creature recipe content hash mismatch: expected {expected}, actual {actual}")]
    HashMismatch { expected: String, actual: String },
    #[error("recipe evidence is not JSON-serializable: {0}")]
    Serialization(String),
}

pub fn build_full_merged_creature_family_recipe(
    mut input: CreatureRecipeBuildInput<'_>,
    interner: &StringInterner,
) -> Result<CreatureFamilyRecipeLedger, CreatureRecipeBuildError> {
    input.expected_creature_winners =
        Some(super::creature_catalog::EXPECTED_FULL_MERGED_CREA_WINNERS);
    build_creature_family_recipe(input, interner)
}

pub fn build_creature_family_recipe(
    input: CreatureRecipeBuildInput<'_>,
    interner: &StringInterner,
) -> Result<CreatureFamilyRecipeLedger, CreatureRecipeBuildError> {
    let catalog = build_creature_corpus_plan(
        input.winning_records,
        CreatureCatalogOptions {
            mesh_root: None,
            expected_creature_winners: input.expected_creature_winners,
        },
        interner,
    )?;
    let motion_catalog = build_creature_motion_catalog(input.motion_families)?;
    let winning_records = select_explicit_winners(input.winning_records, interner);
    let source_crea = catalog
        .records
        .iter()
        .filter_map(|entry| {
            winning_records
                .get(&entry.source)
                .map(|winner| (*winner).clone())
        })
        .collect::<Vec<Record>>();
    let generated_selections;
    let selections = if input.race_data.selections.is_empty() {
        generated_selections = build_family_race_data_selections(&catalog);
        generated_selections.as_slice()
    } else {
        input.race_data.selections
    };
    let loaded_creatures =
        load_creature_record_race_data_evidence(&catalog, selections, &source_crea, interner);
    let movement = load_legacy_movement_base_settings(&catalog, input.winning_records, interner);
    let assets = AssetIndex::new(input.indexed_assets);

    let records = build_record_entries(&catalog);
    let graph_contracts = grouped_by_rig(input.graph_contracts, |entry| &entry.rig);
    let ragdolls = grouped_by_rig(input.ragdoll_evidence, RagdollCapabilityEvidence::rig);
    let motion_families = motion_catalog
        .families
        .iter()
        .map(|family| (family.rig.clone(), family))
        .collect::<BTreeMap<_, _>>();

    let mut families = Vec::with_capacity(catalog.rig_families.len());
    for family in &catalog.rig_families {
        let mut blockers = Vec::new();
        let mut graph_contract = exactly_one(
            graph_contracts.get(&family.key),
            &mut blockers,
            FamilyRecipeBlocker::MissingGraphContract,
            FamilyRecipeBlocker::DuplicateGraphContract,
        )
        .cloned();
        if let Some(contract) = &mut graph_contract {
            contract.required_roles.sort();
            for pair in contract.required_roles.windows(2) {
                if pair[0] == pair[1] {
                    blockers.push(FamilyRecipeBlocker::DuplicateCapabilityRole(pair[0]));
                }
            }
            contract.required_roles.dedup();
        }
        let ragdoll = exactly_one(
            ragdolls.get(&family.key),
            &mut blockers,
            FamilyRecipeBlocker::MissingRagdollEvidence,
            FamilyRecipeBlocker::DuplicateRagdollEvidence,
        )
        .cloned();
        if let Some(contract) = &graph_contract {
            validate_graph_contract(contract, &mut blockers);
        }

        let motion = motion_families.get(&family.key).copied();
        if motion.is_none() {
            blockers.push(FamilyRecipeBlocker::MissingMotionFamily);
        }
        let mut complete_motion = motion.cloned();
        if let Some(motion) = &mut complete_motion {
            normalize_and_validate_motion_family(
                family.key.game,
                motion,
                &winning_records,
                &mut blockers,
            );
        }
        let mut indexed_motion_evidence = input
            .motion_families
            .iter()
            .find(|evidence| evidence.rig == family.key)
            .cloned();
        if let Some(evidence) = &mut indexed_motion_evidence {
            normalize_and_validate_indexed_motion(evidence, &winning_records, &mut blockers);
        }
        let required_motion = match (&graph_contract, motion) {
            (Some(contract), Some(motion)) => build_required_motion(
                family.key.game,
                contract,
                motion,
                &winning_records,
                &mut blockers,
            ),
            _ => Vec::new(),
        };

        let family_bodies = catalog
            .body_variants
            .iter()
            .filter(|body| body.key.rig == family.key)
            .map(|body| body.key.clone())
            .collect::<Vec<_>>();
        let mut asset_roots = vec![(family.key.skeleton_path.clone(), IndexedAssetKind::Skeleton)];
        asset_roots.extend(
            family_bodies
                .iter()
                .flat_map(|body| body.body_paths.iter().cloned())
                .filter(|path| assets.unique(family.key.game, path).is_some())
                .map(|path| (path, IndexedAssetKind::Body)),
        );
        if let Some(evidence) = input
            .motion_families
            .iter()
            .find(|evidence| evidence.rig == family.key)
        {
            asset_roots.extend(
                evidence
                    .referenced_kfs
                    .iter()
                    .cloned()
                    .map(|path| (path, IndexedAssetKind::Kf)),
            );
        }
        if let Some(RagdollCapabilityEvidence::Supported {
            mode, owner_asset, ..
        }) = &ragdoll
        {
            let expected = match mode {
                RagdollMode::EmbeddedSkeleton => IndexedAssetKind::Skeleton,
                RagdollMode::ExternalAsset => IndexedAssetKind::Ragdoll,
            };
            asset_roots.push((owner_asset.clone(), expected));
        }
        let (actual_root_node, recursive_asset_claims, claimed_assets) = assets.claim_family(
            family.key.game,
            &family.key.skeleton_path,
            &asset_roots,
            &mut blockers,
        );
        validate_ragdoll(&ragdoll, &graph_contract, &assets, &mut blockers);

        let race_data = build_family_race_recipe(
            &family.key,
            selections,
            input.race_data,
            &loaded_creatures.creatures,
            &loaded_creatures.fidelity_receipts,
            &movement.settings,
            input.race_data_policy,
            &mut blockers,
        );
        blockers.sort();
        blockers.dedup();
        let source_plugins = family
            .members
            .iter()
            .filter_map(|member| catalog.record(member))
            .map(|record| record.provenance.source_plugin.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        families.push(CreatureFamilyRecipeJob {
            rig: family.key.clone(),
            source_plugins,
            members: family.members.clone(),
            actual_root_node,
            body_variants: family_bodies,
            graph_contract,
            indexed_motion_evidence,
            motion_evidence: complete_motion,
            required_motion,
            ragdoll,
            race_data,
            recursive_asset_claims,
            assets: claimed_assets,
            disposition: if blockers.is_empty() {
                FamilyRecipeDisposition::Ready
            } else {
                FamilyRecipeDisposition::Rejected { blockers }
            },
        });
    }
    families.sort_by(|left, right| left.rig.cmp(&right.rig));

    let accounting = account(&records, &families);
    validate_accounting(&accounting)?;
    let mut ledger = CreatureFamilyRecipeLedger {
        schema_version: CREATURE_RECIPE_SCHEMA_VERSION,
        records,
        families,
        accounting,
        content_hash_blake3: String::new(),
    };
    ledger.content_hash_blake3 = ledger.compute_content_hash_blake3()?;
    ledger.validate()?;
    Ok(ledger)
}

fn build_record_entries(
    catalog: &super::creature_catalog::CreatureCorpusPlan,
) -> Vec<CreatureRecordRecipeEntry> {
    let mut records = catalog
        .records
        .iter()
        .map(|record| {
            let disposition = match &record.disposition {
                CreatureDisposition::VisualOwner { rig, body } => {
                    CreatureRecordRecipeDisposition::VisualOwner {
                        rig: rig.clone(),
                        body: body.clone(),
                    }
                }
                CreatureDisposition::Special { reason } => {
                    CreatureRecordRecipeDisposition::Special {
                        reason: reason.clone(),
                    }
                }
                CreatureDisposition::Proxy {
                    terminal_visual_owners,
                    readiness,
                } => {
                    let mut blockers = match readiness {
                        ProxyReadiness::Ready => Vec::new(),
                        ProxyReadiness::Blocked(blockers) => blockers
                            .iter()
                            .cloned()
                            .map(RecordRecipeBlocker::Catalog)
                            .collect(),
                    };
                    if terminal_visual_owners.len() > 1 {
                        blockers.push(RecordRecipeBlocker::RequiresLeveledTemplateProjection(
                            terminal_visual_owners.clone(),
                        ));
                    }
                    let mut rigs = BTreeSet::new();
                    for owner in terminal_visual_owners {
                        match catalog.record(owner) {
                            None => blockers
                                .push(RecordRecipeBlocker::MissingTerminalOwner(owner.clone())),
                            Some(owner_record) => match &owner_record.disposition {
                                CreatureDisposition::VisualOwner { rig, .. } => {
                                    rigs.insert(rig.clone());
                                }
                                _ => blockers.push(RecordRecipeBlocker::TerminalOwnerIsNotVisual(
                                    owner.clone(),
                                )),
                            },
                        }
                    }
                    if blockers.is_empty() && rigs.len() == 1 {
                        CreatureRecordRecipeDisposition::Proxy {
                            rig: rigs.into_iter().next().expect("one proxy family"),
                            terminal_visual_owners: terminal_visual_owners.clone(),
                        }
                    } else {
                        if blockers.is_empty() {
                            if rigs.is_empty() {
                                blockers.push(RecordRecipeBlocker::NoResolvedFamily);
                            } else {
                                blockers.push(RecordRecipeBlocker::AmbiguousFamilies(
                                    rigs.into_iter().collect(),
                                ));
                            }
                        }
                        blockers.sort();
                        blockers.dedup();
                        CreatureRecordRecipeDisposition::RejectedProxy { blockers }
                    }
                }
            };
            CreatureRecordRecipeEntry {
                source: record.source.clone(),
                editor_id: record.editor_id.clone(),
                provenance: record.provenance.clone(),
                record_dependencies: record.record_dependencies.clone(),
                disposition,
            }
        })
        .collect::<Vec<_>>();
    records.sort_by(|left, right| left.source.cmp(&right.source));
    records
}

fn build_required_motion(
    game: LegacyCreatureGame,
    contract: &CreatureGraphContractEvidence,
    motion: &CreatureFamilyMotionSet,
    winning_records: &BTreeMap<StableFormKey, &Record>,
    blockers: &mut Vec<FamilyRecipeBlocker>,
) -> Vec<RequiredMotionRoleRecipe> {
    let mut recipes = Vec::new();
    for role in &contract.required_roles {
        let Some(role_evidence) = motion.role(*role) else {
            blockers.push(FamilyRecipeBlocker::MissingRequiredRole(*role));
            continue;
        };
        match &role_evidence.readiness {
            RoleReadiness::Missing => {
                blockers.push(FamilyRecipeBlocker::MissingRequiredRole(*role));
                continue;
            }
            RoleReadiness::Ready | RoleReadiness::Ambiguous(_) => {}
        }
        let mut candidates = role_evidence.candidates.clone();
        for candidate in &mut candidates {
            normalize_candidate(candidate);
            validate_candidate(
                game,
                &contract.rig.skeleton_path,
                *role,
                candidate,
                winning_records,
                blockers,
            );
        }
        recipes.push(RequiredMotionRoleRecipe {
            role: *role,
            candidates,
        });
    }
    recipes.sort_by_key(|recipe| recipe.role);
    recipes
}

fn validate_candidate(
    game: LegacyCreatureGame,
    skeleton_path: &str,
    role: MotionRole,
    candidate: &MotionCandidate,
    winning_records: &BTreeMap<StableFormKey, &Record>,
    blockers: &mut Vec<FamilyRecipeBlocker>,
) {
    if candidate.source_game != game {
        blockers.push(FamilyRecipeBlocker::InvalidMotionEvidence {
            source_kf: candidate.source_kf.clone(),
            detail: "KF provenance does not match the rig family".to_string(),
        });
    }
    if blank(&candidate.source_kf) || blank(&candidate.sequence_name) {
        blockers.push(FamilyRecipeBlocker::InvalidMotionEvidence {
            source_kf: candidate.source_kf.clone(),
            detail: "KF path and sequence name must be explicit".to_string(),
        });
    }
    if candidate.binding.compatibility != BindingCompatibility::Verified
        || blank(&candidate.binding.source_skeleton_path)
    {
        blockers.push(FamilyRecipeBlocker::InvalidMotionEvidence {
            source_kf: candidate.source_kf.clone(),
            detail: "skeleton binding is not explicitly verified".to_string(),
        });
    }
    if canonical_path(&candidate.binding.source_skeleton_path) != canonical_path(skeleton_path) {
        blockers.push(FamilyRecipeBlocker::InvalidMotionEvidence {
            source_kf: candidate.source_kf.clone(),
            detail: "KF binding skeleton does not match the rig family".to_string(),
        });
    }
    if candidate.binding.float_track_count > 0 && candidate.binding.required_float_slots.is_empty()
    {
        blockers.push(FamilyRecipeBlocker::MissingScalarSlotEvidence(
            candidate.source_kf.clone(),
        ));
    }
    match &candidate.root_motion {
        RootMotionEvidence::Unknown | RootMotionEvidence::Unsupported { .. } => {
            blockers.push(FamilyRecipeBlocker::InvalidMotionEvidence {
                source_kf: candidate.source_kf.clone(),
                detail: "root motion is missing or unsupported".to_string(),
            });
        }
        RootMotionEvidence::Planar {
            accum_root,
            distance,
            yaw_radians,
        } if blank(accum_root) || !distance.is_finite() || !yaw_radians.is_finite() => {
            blockers.push(FamilyRecipeBlocker::InvalidMotionEvidence {
                source_kf: candidate.source_kf.clone(),
                detail: "planar root motion is malformed".to_string(),
            });
        }
        _ => {}
    }
    let _ = (role, winning_records);
}

fn normalize_and_validate_motion_family(
    _game: LegacyCreatureGame,
    motion: &mut CreatureFamilyMotionSet,
    _winning_records: &BTreeMap<StableFormKey, &Record>,
    _blockers: &mut Vec<FamilyRecipeBlocker>,
) {
    motion
        .kf_accounting
        .sort_by(|left, right| left.source_kf.cmp(&right.source_kf));
    for role in &mut motion.roles {
        for candidate in &mut role.candidates {
            normalize_candidate(candidate);
        }
    }
    motion.roles.sort_by_key(|role| role.role);
}

fn normalize_and_validate_indexed_motion(
    evidence: &mut CreatureMotionFamilyEvidence,
    winning_records: &BTreeMap<StableFormKey, &Record>,
    blockers: &mut Vec<FamilyRecipeBlocker>,
) {
    evidence
        .referenced_kfs
        .sort_by_key(|path| canonical_path(path).unwrap_or_else(|| path.to_ascii_lowercase()));
    evidence.creature_traits.sort_by(|left, right| {
        left.motion_trait
            .cmp(&right.motion_trait)
            .then_with(|| left.source_creature.cmp(&right.source_creature))
            .then_with(|| left.field.cmp(&right.field))
            .then_with(|| left.value.cmp(&right.value))
    });
    for trait_evidence in &evidence.creature_traits {
        if blank(&trait_evidence.field)
            || blank(&trait_evidence.value)
            || !winning_records.contains_key(&trait_evidence.source_creature)
        {
            blockers.push(FamilyRecipeBlocker::InvalidMotionEvidence {
                source_kf: evidence.rig.skeleton_path.clone(),
                detail: "creature trait lacks an explicit source record, field, or value"
                    .to_string(),
            });
        }
    }
    for kf in &mut evidence.kf_evidence {
        kf.idle_claims
            .sort_by(|left, right| left.source_idle.cmp(&right.source_idle));
        if kf.source_game != evidence.rig.game || canonical_path(&kf.source_kf).is_none() {
            blockers.push(FamilyRecipeBlocker::InvalidMotionEvidence {
                source_kf: kf.source_kf.clone(),
                detail: "KF path or game provenance is invalid".to_string(),
            });
        }
        match &mut kf.sequence {
            KfParseEvidence::Parsed(sequence) => {
                sequence.text_keys.sort_by(|left, right| {
                    left.time
                        .total_cmp(&right.time)
                        .then_with(|| left.text.cmp(&right.text))
                });
                sequence.binding.controller_types.sort();
                sequence.binding.interpolator_types.sort();
                sequence.binding.target_names.sort();
                if blank(&sequence.name)
                    || !sequence.start_time.is_finite()
                    || !sequence.stop_time.is_finite()
                    || !sequence.frequency.is_finite()
                    || sequence.stop_time < sequence.start_time
                    || sequence.frequency <= 0.0
                {
                    blockers.push(FamilyRecipeBlocker::InvalidMotionEvidence {
                        source_kf: kf.source_kf.clone(),
                        detail: "sequence identity or timing is invalid".to_string(),
                    });
                }
            }
            KfParseEvidence::MissingAsset | KfParseEvidence::ParseFailed(_) => {}
        }
        for claim in &kf.idle_claims {
            for node in &claim.ancestry {
                match winning_records.get(&node.form_key) {
                    None => blockers.push(FamilyRecipeBlocker::MissingIdleRecord(
                        node.form_key.clone(),
                    )),
                    Some(record) if record.sig.as_str() != "IDLE" => {
                        blockers.push(FamilyRecipeBlocker::WrongIdleSignature {
                            source: node.form_key.clone(),
                            signature: record.sig.as_str().to_string(),
                        });
                    }
                    Some(_) => {}
                }
            }
        }
    }
    evidence.kf_evidence.sort_by(|left, right| {
        canonical_path(&left.source_kf)
            .cmp(&canonical_path(&right.source_kf))
            .then_with(|| left.source_kf.cmp(&right.source_kf))
    });
}

fn normalize_candidate(candidate: &mut MotionCandidate) {
    candidate.events.sort_by(|left, right| {
        left.time
            .total_cmp(&right.time)
            .then_with(|| left.raw_text.cmp(&right.raw_text))
    });
    candidate.evidence.sort_by(|left, right| {
        left.source
            .cmp(&right.source)
            .then_with(|| left.detail.cmp(&right.detail))
            .then_with(|| match (left.event_time, right.event_time) {
                (Some(left), Some(right)) => left.total_cmp(&right),
                (None, Some(_)) => std::cmp::Ordering::Less,
                (Some(_), None) => std::cmp::Ordering::Greater,
                (None, None) => std::cmp::Ordering::Equal,
            })
    });
    candidate.binding.controller_types.sort();
    candidate.binding.interpolator_types.sort();
    candidate.binding.overlay_targets.sort();
    candidate.ambiguous_with.sort();
}

fn validate_graph_contract(
    contract: &CreatureGraphContractEvidence,
    blockers: &mut Vec<FamilyRecipeBlocker>,
) {
    let required = capability_roles(contract.capability);
    let declared = contract
        .required_roles
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();
    for role in required {
        if !declared.contains(&role) {
            blockers.push(FamilyRecipeBlocker::MissingCapabilityRole(*role));
        }
    }
    let has_ranged = [
        MotionRole::RangedAttack,
        MotionRole::Fire,
        MotionRole::Projectile,
    ]
    .iter()
    .any(|role| declared.contains(role));
    let needs_ranged = matches!(
        contract.capability,
        CreatureGraphCapability::GroundRangedProjectile
            | CreatureGraphCapability::GroundMeleeRanged
    ) || (contract.capability == CreatureGraphCapability::StationaryTurret
        && !declared.contains(&MotionRole::MeleeAttack));
    if needs_ranged && !has_ranged {
        blockers.push(FamilyRecipeBlocker::MissingCapabilityRole(
            MotionRole::RangedAttack,
        ));
    }
    let mut seen = BTreeSet::new();
    for role in &contract.required_roles {
        if !seen.insert(*role) {
            blockers.push(FamilyRecipeBlocker::UnexpectedCapabilityRole(*role));
        }
    }
}

fn capability_roles(capability: CreatureGraphCapability) -> &'static [MotionRole] {
    use MotionRole::*;
    match capability {
        CreatureGraphCapability::PassiveGround => &[Idle],
        CreatureGraphCapability::GroundMelee => &[Idle, GroundLocomotion, MeleeAttack],
        CreatureGraphCapability::GroundRangedProjectile => &[Idle, GroundLocomotion],
        CreatureGraphCapability::GroundMeleeRanged => &[Idle, GroundLocomotion, MeleeAttack],
        CreatureGraphCapability::GroundSwim => &[Idle, GroundLocomotion, Swim],
        CreatureGraphCapability::GroundFly => &[Idle, GroundLocomotion, Fly],
        CreatureGraphCapability::Swim => &[Idle, Swim],
        CreatureGraphCapability::Fly => &[Idle, Fly],
        CreatureGraphCapability::StationaryTurret => &[Idle],
        CreatureGraphCapability::RobotContinuousAttack => {
            &[Idle, GroundLocomotion, ContinuousOrRobot]
        }
        CreatureGraphCapability::HumanoidWeaponOverlay => &[Idle, Overlay],
    }
}

fn build_family_race_recipe(
    rig: &RigFamilyKey,
    selections: &[FamilyRaceDataSelection],
    inputs: LegacyRaceDataEvidenceInputs<'_>,
    loaded_creatures: &[CreatureRaceDataEvidence],
    receipts: &[RaceDataFidelityReceipt],
    movement_settings: &[LegacyMovementBaseSettings],
    policy: &Fo4RaceDataScalePolicy,
    blockers: &mut Vec<FamilyRecipeBlocker>,
) -> Option<FamilyRaceDataRecipe> {
    let selection = one_for(
        selections,
        |selection| &selection.rig == rig,
        blockers,
        FamilyRecipeBlocker::MissingFamilySelection,
        FamilyRecipeBlocker::DuplicateFamilySelection,
    )?;
    let rig_evidence = one_for(
        inputs.rigs,
        |evidence| &evidence.rig == rig,
        blockers,
        FamilyRecipeBlocker::MissingRigRaceData,
        FamilyRecipeBlocker::DuplicateRigRaceData,
    )?;
    let body_evidence = one_for(
        inputs.bodies,
        |evidence| evidence.body == selection.source_body,
        blockers,
        FamilyRecipeBlocker::MissingBodyRaceData,
        FamilyRecipeBlocker::DuplicateBodyRaceData,
    )?;
    let controller = one_for(
        inputs.controllers,
        |evidence| &evidence.rig == rig,
        blockers,
        FamilyRecipeBlocker::MissingControllerRaceData,
        FamilyRecipeBlocker::DuplicateControllerRaceData,
    )?;
    let creature = one_for(
        loaded_creatures,
        |evidence| evidence.creature == selection.source_creature,
        blockers,
        FamilyRecipeBlocker::MissingCreatureRaceData,
        FamilyRecipeBlocker::DuplicateCreatureRaceData,
    )?;
    let settings = one_for(
        movement_settings,
        |settings| settings.game == rig.game,
        blockers,
        FamilyRecipeBlocker::MissingMovementSettings(rig.game),
        FamilyRecipeBlocker::AmbiguousMovementSettings(rig.game),
    )?;
    if controller.capsule.is_none() {
        blockers.push(FamilyRecipeBlocker::MissingCapsule);
    }
    let measured_evidence = SourceFamilyRaceDataEvidence {
        body_bounds: body_evidence.body_bounds.clone(),
        skeleton_bounds: rig_evidence.skeleton_bounds.clone(),
        up_axis: rig_evidence.up_axis,
        capsule: controller.capsule.clone(),
        geometry_scale_to_fo4: rig_evidence.geometry_scale_to_fo4,
        male_actor_scale: creature.male_actor_scale,
        female_actor_scale: creature.female_actor_scale,
        default_weights: creature.default_weights.clone(),
        movement: controller.movement.clone(),
        injured_health_percent: creature.injured_health_percent,
        body_biped_object: creature.body_biped_object,
        xp_value: creature.xp_value,
    };
    let derivation = match derive_fo4_race_data(&measured_evidence, policy) {
        Ok(derivation) => derivation,
        Err(error) => {
            blockers.push(FamilyRecipeBlocker::InvalidRaceData(error.to_string()));
            return None;
        }
    };
    if !derivation.missing_evidence.is_empty() {
        blockers.push(FamilyRecipeBlocker::MissingRaceDataFields(
            derivation.missing_evidence.clone(),
        ));
    }
    let mut policy_receipts = receipts
        .iter()
        .filter(|receipt| receipt_creature(receipt) == &selection.source_creature)
        .cloned()
        .collect::<Vec<_>>();
    policy_receipts.sort();
    policy_receipts.dedup();
    Some(FamilyRaceDataRecipe {
        selection: selection.clone(),
        measured_evidence,
        derivation,
        policy_receipts,
        movement_settings: MovementSettingsReceipt {
            game: settings.game,
            f_move_base_speed_source: settings.f_move_base_speed_source.clone(),
            locomotion_multiplier_source: settings.locomotion_multiplier_source.clone(),
            f_move_base_speed_bits: settings.f_move_base_speed.to_bits(),
            locomotion_multiplier_bits: settings.locomotion_multiplier.to_bits(),
        },
    })
}

fn receipt_creature(receipt: &RaceDataFidelityReceipt) -> &StableFormKey {
    match receipt {
        RaceDataFidelityReceipt::SchemaAbsentDefaulted { creature, .. }
        | RaceDataFidelityReceipt::LevelAsXpMvp { creature, .. } => creature,
    }
}

fn validate_ragdoll(
    ragdoll: &Option<RagdollCapabilityEvidence>,
    graph: &Option<CreatureGraphContractEvidence>,
    assets: &AssetIndex<'_>,
    blockers: &mut Vec<FamilyRecipeBlocker>,
) {
    match ragdoll {
        Some(RagdollCapabilityEvidence::Supported {
            rig,
            mode,
            owner_asset,
        }) => {
            let expected = match mode {
                RagdollMode::EmbeddedSkeleton => IndexedAssetKind::Skeleton,
                RagdollMode::ExternalAsset => IndexedAssetKind::Ragdoll,
            };
            match assets.unique(rig.game, owner_asset) {
                None => blockers.push(FamilyRecipeBlocker::MissingRagdollOwner(
                    owner_asset.clone(),
                )),
                Some(asset) if asset.kind != expected => {
                    blockers.push(FamilyRecipeBlocker::RagdollOwnerKindMismatch {
                        path: canonical_path(owner_asset).unwrap_or_else(|| owner_asset.clone()),
                        actual: asset.kind,
                    });
                }
                Some(_) => {}
            }
        }
        Some(RagdollCapabilityEvidence::ExplicitlyUnsupported { reason, .. }) => {
            if blank(reason) {
                blockers.push(FamilyRecipeBlocker::InvalidAssetEvidence {
                    path: "ragdoll".to_string(),
                    detail: "unsupported ragdoll evidence requires a reason".to_string(),
                });
            }
            if graph.as_ref().is_some_and(|graph| graph.requires_ragdoll) {
                blockers.push(FamilyRecipeBlocker::RagdollRequiredButUnsupported);
            }
        }
        None => {}
    }
}

struct AssetIndex<'a> {
    entries: BTreeMap<(LegacyCreatureGame, String), Vec<&'a IndexedSourceAssetEvidence>>,
}

impl<'a> AssetIndex<'a> {
    fn new(evidence: &'a [IndexedSourceAssetEvidence]) -> Self {
        let mut entries = BTreeMap::<(LegacyCreatureGame, String), Vec<_>>::new();
        for asset in evidence {
            let key =
                canonical_path(&asset.path).unwrap_or_else(|| asset.path.to_ascii_lowercase());
            entries.entry((asset.game, key)).or_default().push(asset);
        }
        Self { entries }
    }

    fn unique(
        &self,
        game: LegacyCreatureGame,
        path: &str,
    ) -> Option<&'a IndexedSourceAssetEvidence> {
        let key = canonical_path(path)?;
        match self.entries.get(&(game, key)).map(Vec::as_slice) {
            Some([asset]) => Some(*asset),
            _ => None,
        }
    }

    fn claim_family(
        &self,
        game: LegacyCreatureGame,
        skeleton_path: &str,
        roots: &[(String, IndexedAssetKind)],
        blockers: &mut Vec<FamilyRecipeBlocker>,
    ) -> (
        Option<String>,
        Vec<RecursiveAssetClaim>,
        Vec<CanonicalAssetClaim>,
    ) {
        let mut all_paths = BTreeSet::new();
        let mut claims = Vec::new();
        for (root, expected_kind) in roots {
            let Some(root_key) = canonical_path(root) else {
                blockers.push(FamilyRecipeBlocker::InvalidAssetEvidence {
                    path: root.clone(),
                    detail: "asset path is empty, absolute, or traverses upward".to_string(),
                });
                continue;
            };
            let mut closure = BTreeSet::new();
            self.visit(
                game,
                &root_key,
                Some(*expected_kind),
                &mut closure,
                blockers,
            );
            all_paths.extend(closure.iter().cloned());
            claims.push(RecursiveAssetClaim {
                root: root_key,
                closure: closure.into_iter().collect(),
            });
        }
        claims.sort_by(|left, right| left.root.cmp(&right.root));
        claims.dedup_by(|left, right| left.root == right.root);

        let skeleton_key = canonical_path(skeleton_path);
        let actual_root = skeleton_key
            .as_deref()
            .and_then(|path| self.unique(game, path).map(|asset| (path, asset)))
            .and_then(|(path, asset)| {
                let mut roots = asset
                    .root_nodes
                    .iter()
                    .filter(|root| !blank(root))
                    .cloned()
                    .collect::<Vec<_>>();
                roots.sort();
                roots.dedup();
                match roots.as_slice() {
                    [] => {
                        blockers.push(FamilyRecipeBlocker::MissingActualRoot(path.to_string()));
                        None
                    }
                    [root] => Some(root.clone()),
                    _ => {
                        blockers.push(FamilyRecipeBlocker::AmbiguousActualRoot {
                            path: path.to_string(),
                            roots,
                        });
                        None
                    }
                }
            });
        let assets = all_paths
            .into_iter()
            .filter_map(|path| self.unique(game, &path))
            .filter_map(canonical_asset_claim)
            .collect();
        (actual_root, claims, assets)
    }

    fn visit(
        &self,
        game: LegacyCreatureGame,
        path: &str,
        expected_kind: Option<IndexedAssetKind>,
        closure: &mut BTreeSet<String>,
        blockers: &mut Vec<FamilyRecipeBlocker>,
    ) {
        if closure.contains(path) {
            return;
        }
        let Some(candidates) = self.entries.get(&(game, path.to_string())) else {
            if expected_kind.is_none() && path.ends_with(".nif") {
                return;
            }
            closure.insert(path.to_string());
            blockers.push(FamilyRecipeBlocker::MissingAsset(path.to_string()));
            return;
        };
        closure.insert(path.to_string());
        if candidates.len() != 1 {
            blockers.push(FamilyRecipeBlocker::AmbiguousAsset(path.to_string()));
            return;
        }
        let asset = candidates[0];
        if asset.game != game {
            blockers.push(FamilyRecipeBlocker::AssetGameMismatch {
                path: path.to_string(),
                expected: game,
                actual: asset.game,
            });
        }
        if let Some(expected) = expected_kind
            && asset.kind != expected
        {
            blockers.push(FamilyRecipeBlocker::AssetKindMismatch {
                path: path.to_string(),
                expected,
                actual: asset.kind,
            });
        }
        if !canonical_blake3_hash(&asset.content_hash_blake3) {
            blockers.push(FamilyRecipeBlocker::InvalidAssetEvidence {
                path: path.to_string(),
                detail: "content hash must be a non-placeholder lowercase 64-hex BLAKE3 digest"
                    .to_string(),
            });
        }
        if asset.kind != IndexedAssetKind::Skeleton && !asset.root_nodes.is_empty() {
            blockers.push(FamilyRecipeBlocker::InvalidAssetEvidence {
                path: path.to_string(),
                detail: "only skeleton evidence may declare root nodes".to_string(),
            });
        }
        let skeleton_contract_valid = match asset.kind {
            IndexedAssetKind::Skeleton => {
                asset
                    .skeleton_runtime_name
                    .as_deref()
                    .is_some_and(|name| !blank(name))
                    && asset.skeleton_float_slots.iter().all(|slot| !blank(slot))
                    && asset
                        .skeleton_float_slots
                        .iter()
                        .collect::<BTreeSet<_>>()
                        .len()
                        == asset.skeleton_float_slots.len()
            }
            _ => asset.skeleton_runtime_name.is_none() && asset.skeleton_float_slots.is_empty(),
        };
        if !skeleton_contract_valid {
            blockers.push(FamilyRecipeBlocker::InvalidAssetEvidence {
                path: path.to_string(),
                detail: "skeleton runtime name/ordered float slots are missing or invalid"
                    .to_string(),
            });
        }
        let mut dependencies = asset.dependencies.clone();
        dependencies.sort_by_key(|path| path.to_ascii_lowercase());
        for dependency in dependencies {
            let Some(dependency) = canonical_path(&dependency) else {
                blockers.push(FamilyRecipeBlocker::InvalidAssetEvidence {
                    path: path.to_string(),
                    detail: "dependency path is invalid".to_string(),
                });
                continue;
            };
            self.visit(game, &dependency, None, closure, blockers);
        }
    }
}

fn canonical_asset_claim(asset: &IndexedSourceAssetEvidence) -> Option<CanonicalAssetClaim> {
    let skeleton_contract_valid = match asset.kind {
        IndexedAssetKind::Skeleton => {
            asset
                .skeleton_runtime_name
                .as_deref()
                .is_some_and(|name| !blank(name))
                && asset.skeleton_float_slots.iter().all(|slot| !blank(slot))
                && asset
                    .skeleton_float_slots
                    .iter()
                    .collect::<BTreeSet<_>>()
                    .len()
                    == asset.skeleton_float_slots.len()
        }
        _ => asset.skeleton_runtime_name.is_none() && asset.skeleton_float_slots.is_empty(),
    };
    if !canonical_blake3_hash(&asset.content_hash_blake3) || !skeleton_contract_valid {
        return None;
    }
    let mut dependencies = asset
        .dependencies
        .iter()
        .filter_map(|path| canonical_path(path))
        .collect::<Vec<_>>();
    dependencies.sort();
    dependencies.dedup();
    let mut root_nodes = asset.root_nodes.clone();
    root_nodes.sort();
    root_nodes.dedup();
    Some(CanonicalAssetClaim {
        game: asset.game,
        path: canonical_path(&asset.path).unwrap_or_else(|| asset.path.to_ascii_lowercase()),
        kind: asset.kind,
        content_hash_blake3: asset.content_hash_blake3.clone(),
        dependencies,
        root_nodes,
        skeleton_runtime_name: asset.skeleton_runtime_name.clone(),
        skeleton_float_slots: asset.skeleton_float_slots.clone(),
    })
}

fn canonical_path(path: &str) -> Option<String> {
    let path = path.trim().replace('\\', "/");
    if path.is_empty() || path.starts_with('/') || path.contains(':') {
        return None;
    }
    let mut parts = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => return None,
            part => parts.push(part.to_ascii_lowercase()),
        }
    }
    (!parts.is_empty()).then(|| parts.join("/"))
}

fn select_explicit_winners<'a>(
    records: &'a [LegacyRecordSource<'a>],
    interner: &StringInterner,
) -> BTreeMap<StableFormKey, &'a Record> {
    let mut winners = BTreeMap::<StableFormKey, (&Record, u32)>::new();
    for source in records {
        let Some(plugin) = interner.resolve(source.record.form_key.plugin) else {
            continue;
        };
        let key = StableFormKey {
            local: source.record.form_key.local,
            plugin: plugin.to_ascii_lowercase(),
        };
        if winners
            .get(&key)
            .is_none_or(|(_, precedence)| source.provenance.precedence > *precedence)
        {
            winners.insert(key, (source.record, source.provenance.precedence));
        }
    }
    winners
        .into_iter()
        .map(|(key, (record, _))| (key, record))
        .collect()
}

fn grouped_by_rig<'a, T, F>(values: &'a [T], rig: F) -> BTreeMap<RigFamilyKey, Vec<&'a T>>
where
    F: Fn(&T) -> &RigFamilyKey,
{
    let mut grouped = BTreeMap::<RigFamilyKey, Vec<&T>>::new();
    for value in values {
        grouped.entry(rig(value).clone()).or_default().push(value);
    }
    grouped
}

fn exactly_one<'a, T>(
    candidates: Option<&'a Vec<&T>>,
    blockers: &mut Vec<FamilyRecipeBlocker>,
    missing: FamilyRecipeBlocker,
    duplicate: FamilyRecipeBlocker,
) -> Option<&'a T> {
    match candidates.map(Vec::as_slice).unwrap_or(&[]) {
        [candidate] => Some(*candidate),
        [] => {
            blockers.push(missing);
            None
        }
        _ => {
            blockers.push(duplicate);
            None
        }
    }
}

fn one_for<'a, T, F>(
    values: &'a [T],
    predicate: F,
    blockers: &mut Vec<FamilyRecipeBlocker>,
    missing: FamilyRecipeBlocker,
    duplicate: FamilyRecipeBlocker,
) -> Option<&'a T>
where
    F: Fn(&T) -> bool,
{
    let matches = values
        .iter()
        .filter(|value| predicate(value))
        .collect::<Vec<_>>();
    match matches.as_slice() {
        [candidate] => Some(*candidate),
        [] => {
            blockers.push(missing);
            None
        }
        _ => {
            blockers.push(duplicate);
            None
        }
    }
}

fn validate_ledger_references_and_order(
    ledger: &CreatureFamilyRecipeLedger,
) -> Result<(), CreatureRecipeBuildError> {
    let family_ids = ledger
        .families
        .iter()
        .map(|family| &family.rig)
        .collect::<BTreeSet<_>>();
    let records = ledger
        .records
        .iter()
        .map(|record| (&record.source, record))
        .collect::<BTreeMap<_, _>>();

    for record in &ledger.records {
        if !strictly_sorted_unique(record.record_dependencies.iter()) {
            return invalid_ledger(format!(
                "record {:?} dependencies are not canonical and unique",
                record.source
            ));
        }
        match &record.disposition {
            CreatureRecordRecipeDisposition::VisualOwner { rig, body } => {
                if !family_ids.contains(rig) || &body.rig != rig {
                    return invalid_ledger(format!(
                        "record {:?} references an invalid visual family",
                        record.source
                    ));
                }
            }
            CreatureRecordRecipeDisposition::Proxy {
                rig,
                terminal_visual_owners,
            } => {
                if !family_ids.contains(rig)
                    || terminal_visual_owners.is_empty()
                    || !strictly_sorted_unique(terminal_visual_owners.iter())
                {
                    return invalid_ledger(format!(
                        "record {:?} has invalid proxy-family references",
                        record.source
                    ));
                }
                for owner in terminal_visual_owners {
                    let Some(owner) = records.get(owner) else {
                        return invalid_ledger(format!(
                            "record {:?} references an absent terminal owner",
                            record.source
                        ));
                    };
                    if !matches!(
                        &owner.disposition,
                        CreatureRecordRecipeDisposition::VisualOwner {
                            rig: owner_rig,
                            ..
                        } if owner_rig == rig
                    ) {
                        return invalid_ledger(format!(
                            "record {:?} terminal owner is not in its proxy family",
                            record.source
                        ));
                    }
                }
            }
            CreatureRecordRecipeDisposition::RejectedProxy { blockers } => {
                if blockers.is_empty() || !strictly_sorted_unique(blockers.iter()) {
                    return invalid_ledger(format!(
                        "record {:?} proxy blockers are not canonical and unique",
                        record.source
                    ));
                }
            }
            CreatureRecordRecipeDisposition::Special { .. } => {}
        }
    }

    for family in &ledger.families {
        if family.source_plugins.is_empty()
            || !strictly_sorted_unique(family.source_plugins.iter())
            || family.source_plugins.iter().any(|plugin| blank(plugin))
            || family.members.is_empty()
            || !strictly_sorted_unique(family.members.iter())
            || !strictly_sorted_unique(family.body_variants.iter())
            || !strictly_sorted_unique_by(&family.required_motion, |recipe| recipe.role)
            || !strictly_sorted_unique_by(&family.recursive_asset_claims, |claim| {
                claim.root.clone()
            })
            || !strictly_sorted_unique_by(&family.assets, |asset| (asset.game, asset.path.clone()))
        {
            return invalid_ledger(format!(
                "family {:?} collections are not canonical and unique",
                family.rig
            ));
        }
        if family
            .members
            .iter()
            .any(|member| !records.contains_key(member))
            || family
                .body_variants
                .iter()
                .any(|body| body.rig != family.rig)
        {
            return invalid_ledger(format!(
                "family {:?} contains a foreign or absent member",
                family.rig
            ));
        }
        if let Some(contract) = &family.graph_contract
            && (contract.rig != family.rig
                || !strictly_sorted_unique(contract.required_roles.iter()))
        {
            return invalid_ledger(format!(
                "family {:?} graph contract is not canonical",
                family.rig
            ));
        }
        if family
            .indexed_motion_evidence
            .as_ref()
            .is_some_and(|motion| motion.rig != family.rig)
            || family
                .motion_evidence
                .as_ref()
                .is_some_and(|motion| motion.rig != family.rig)
            || family
                .ragdoll
                .as_ref()
                .is_some_and(|ragdoll| ragdoll.rig() != &family.rig)
        {
            return invalid_ledger(format!("family {:?} contains foreign evidence", family.rig));
        }
        for claim in &family.recursive_asset_claims {
            if canonical_path(&claim.root).as_deref() != Some(claim.root.as_str())
                || !strictly_sorted_unique(claim.closure.iter())
                || claim
                    .closure
                    .iter()
                    .any(|path| canonical_path(path).as_deref() != Some(path.as_str()))
            {
                return invalid_ledger(format!(
                    "family {:?} recursive asset claim is not canonical",
                    family.rig
                ));
            }
        }
        for asset in &family.assets {
            let skeleton_contract_valid = match asset.kind {
                IndexedAssetKind::Skeleton => {
                    asset
                        .skeleton_runtime_name
                        .as_deref()
                        .is_some_and(|name| !blank(name))
                        && asset.skeleton_float_slots.iter().all(|slot| !blank(slot))
                        && asset
                            .skeleton_float_slots
                            .iter()
                            .collect::<BTreeSet<_>>()
                            .len()
                            == asset.skeleton_float_slots.len()
                }
                _ => asset.skeleton_runtime_name.is_none() && asset.skeleton_float_slots.is_empty(),
            };
            if asset.game != family.rig.game
                || canonical_path(&asset.path).as_deref() != Some(asset.path.as_str())
                || !canonical_blake3_hash(&asset.content_hash_blake3)
                || !strictly_sorted_unique(asset.dependencies.iter())
                || !strictly_sorted_unique(asset.root_nodes.iter())
                || asset
                    .dependencies
                    .iter()
                    .any(|path| canonical_path(path).as_deref() != Some(path.as_str()))
                || (asset.kind != IndexedAssetKind::Skeleton && !asset.root_nodes.is_empty())
                || !skeleton_contract_valid
            {
                return invalid_ledger(format!(
                    "family {:?} has invalid canonical asset evidence for {}",
                    family.rig, asset.path
                ));
            }
        }
        match &family.disposition {
            FamilyRecipeDisposition::Ready => {
                let assets = family
                    .assets
                    .iter()
                    .map(|asset| asset.path.as_str())
                    .collect::<BTreeSet<_>>();
                if family
                    .actual_root_node
                    .as_ref()
                    .is_none_or(|root| blank(root))
                    || family.recursive_asset_claims.iter().any(|claim| {
                        claim
                            .closure
                            .iter()
                            .any(|path| !assets.contains(path.as_str()))
                    })
                {
                    return invalid_ledger(format!(
                        "ready family {:?} has an incomplete asset closure",
                        family.rig
                    ));
                }
            }
            FamilyRecipeDisposition::Rejected { blockers } => {
                if blockers.is_empty() || !strictly_sorted_unique(blockers.iter()) {
                    return invalid_ledger(format!(
                        "family {:?} blockers are not canonical and unique",
                        family.rig
                    ));
                }
            }
        }
    }
    Ok(())
}

fn invalid_ledger<T>(detail: String) -> Result<T, CreatureRecipeBuildError> {
    Err(CreatureRecipeBuildError::InvalidLedger(detail))
}

fn strictly_sorted_unique<'a, T: Ord + 'a>(values: impl IntoIterator<Item = &'a T>) -> bool {
    let mut values = values.into_iter();
    let Some(mut previous) = values.next() else {
        return true;
    };
    for current in values {
        if previous >= current {
            return false;
        }
        previous = current;
    }
    true
}

fn strictly_sorted_unique_by<T, K: Ord>(values: &[T], key: impl Fn(&T) -> K) -> bool {
    values.windows(2).all(|pair| key(&pair[0]) < key(&pair[1]))
}

fn canonical_blake3_hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
        && value.bytes().any(|byte| byte != b'0')
}

fn account(
    records: &[CreatureRecordRecipeEntry],
    families: &[CreatureFamilyRecipeJob],
) -> CreatureRecipeAccounting {
    let mut accounting = CreatureRecipeAccounting {
        winning_creatures: records.len(),
        record_dispositions: records.len(),
        rig_families: families.len(),
        family_jobs: families.len(),
        ..CreatureRecipeAccounting::default()
    };
    for record in records {
        match record.provenance.game {
            LegacyCreatureGame::Fnv => accounting.fnv_creatures += 1,
            LegacyCreatureGame::Fo3 => accounting.fo3_creatures += 1,
        }
    }
    for family in families {
        match family.disposition {
            FamilyRecipeDisposition::Ready => accounting.ready_families += 1,
            FamilyRecipeDisposition::Rejected { .. } => accounting.rejected_families += 1,
        }
    }
    accounting
}

fn validate_accounting(
    accounting: &CreatureRecipeAccounting,
) -> Result<(), CreatureRecipeBuildError> {
    if accounting.winning_creatures != accounting.record_dispositions {
        return Err(CreatureRecipeBuildError::Accounting(
            "every winning CREA must have exactly one disposition".to_string(),
        ));
    }
    if accounting.fnv_creatures + accounting.fo3_creatures != accounting.winning_creatures {
        return Err(CreatureRecipeBuildError::Accounting(
            "FNV/FO3 provenance does not partition the winning CREA census".to_string(),
        ));
    }
    if accounting.rig_families != accounting.family_jobs
        || accounting.ready_families + accounting.rejected_families != accounting.family_jobs
    {
        return Err(CreatureRecipeBuildError::Accounting(
            "rig families are not represented by exactly one family job".to_string(),
        ));
    }
    Ok(())
}

fn blank(value: &str) -> bool {
    value.trim().is_empty()
}

#[cfg(test)]
pub(super) mod tests {
    use smallvec::SmallVec;

    use super::*;
    use crate::ids::{FormKey, SigCode, SubrecordSig};
    use crate::record::{FieldEntry, FieldValue};
    use crate::source_rig::race_data::{
        AngularMotionEvidence, CapsuleEvidence, ControllerArchitecture, LinearMotionEvidence,
        MeasuredBounds, MeasurementAxis, MovementArchitecture, MovementSemantics,
    };
    use crate::translator::pair_hooks::fnv_fo4::creature_motion::{
        BindingEvidence, CreatureKfEvidence, IdleAncestryNode, IdleClaimEvidence, KfParseEvidence,
        NiControllerSequenceEvidence, SequenceCycle, TextKeyEvidence,
    };
    use crate::translator::pair_hooks::fnv_fo4::creature_race_data::{
        BodyRaceDataEvidence, ControllerRaceDataEvidence, RigRaceDataEvidence,
    };

    fn form_key(local: u32, plugin: &str, interner: &StringInterner) -> FormKey {
        FormKey {
            local,
            plugin: interner.intern(plugin),
        }
    }

    fn field(signature: &str, value: FieldValue) -> FieldEntry {
        FieldEntry {
            sig: SubrecordSig::from_str(signature).unwrap(),
            value,
        }
    }

    fn record(
        signature: &str,
        local: u32,
        plugin: &str,
        editor_id: &str,
        fields: Vec<FieldEntry>,
        interner: &StringInterner,
    ) -> Record {
        let mut record = Record::new(
            SigCode::from_str(signature).unwrap(),
            form_key(local, plugin, interner),
        );
        record.eid = Some(interner.intern(editor_id));
        record.fields = SmallVec::from_vec(fields);
        record
    }

    fn provenance(game: LegacyCreatureGame, plugin: &str) -> CreatureProvenance {
        CreatureProvenance {
            game,
            source_plugin: plugin.to_string(),
            precedence: 0,
        }
    }

    fn stable(local: u32, plugin: &str) -> StableFormKey {
        StableFormKey {
            local,
            plugin: plugin.to_ascii_lowercase(),
        }
    }

    #[derive(Clone)]
    struct GroundFixture {
        records: Vec<Record>,
        provenance: Vec<CreatureProvenance>,
        assets: Vec<IndexedSourceAssetEvidence>,
        motion: Vec<CreatureMotionFamilyEvidence>,
        graph: Vec<CreatureGraphContractEvidence>,
        ragdoll: Vec<RagdollCapabilityEvidence>,
        selections: Vec<FamilyRaceDataSelection>,
        rigs: Vec<RigRaceDataEvidence>,
        bodies: Vec<BodyRaceDataEvidence>,
        controllers: Vec<ControllerRaceDataEvidence>,
        policy: Fo4RaceDataScalePolicy,
    }

    impl GroundFixture {
        fn new(interner: &StringInterner) -> Self {
            let plugin = "FalloutNV.esm";
            let creature_key = stable(0x100, plugin);
            let idle_key = stable(0x200, plugin);
            let rig = RigFamilyKey {
                game: LegacyCreatureGame::Fnv,
                skeleton_path: "creatures/gecko/skeleton.nif".to_string(),
            };
            let creature = record(
                "CREA",
                creature_key.local,
                plugin,
                "Gecko",
                vec![
                    field(
                        "MODL",
                        FieldValue::String(interner.intern("creatures/gecko/skeleton.nif")),
                    ),
                    field(
                        "NIFZ",
                        FieldValue::Bytes(SmallVec::from_slice(b"gecko.nif\0")),
                    ),
                    field(
                        "NIFT",
                        FieldValue::Bytes(SmallVec::from_slice(&[0, 0, 0, 0])),
                    ),
                    field("BNAM", FieldValue::Float(1.0)),
                    field(
                        "ACBS",
                        FieldValue::Struct(vec![(interner.intern("level"), FieldValue::Uint(8))]),
                    ),
                ],
                interner,
            );
            let idle = record(
                "IDLE",
                idle_key.local,
                plugin,
                "GeckoIdle",
                vec![],
                interner,
            );
            let base_speed = record(
                "GMST",
                0x300,
                plugin,
                "fMoveBaseSpeed",
                vec![field("DATA", FieldValue::Float(77.0))],
                interner,
            );
            let run_mult = record(
                "GMST",
                0x301,
                plugin,
                "fMoveRunMult",
                vec![field("DATA", FieldValue::Float(2.0))],
                interner,
            );
            let body = BodyVariantKey {
                rig: rig.clone(),
                body_paths: vec!["creatures/gecko/gecko.nif".to_string()],
                nift: vec![0, 0, 0, 0],
                nifz_supported: true,
                nift_supported: true,
            };
            let idle_claim = IdleClaimEvidence {
                source_idle: idle_key.clone(),
                ancestry: vec![IdleAncestryNode {
                    form_key: idle_key,
                    editor_id: Some("GeckoIdle".to_string()),
                    model_path: None,
                    parent: None,
                    previous: None,
                    conditions: Vec::new(),
                }],
            };
            let clips = [
                ("idle.kf", "Idle", vec![idle_claim]),
                ("walk.kf", "WalkForward", Vec::new()),
                ("attack.kf", "AttackBite", Vec::new()),
                ("death.kf", "Death", Vec::new()),
            ]
            .into_iter()
            .map(|(file, sequence_name, idle_claims)| CreatureKfEvidence {
                source_game: LegacyCreatureGame::Fnv,
                source_kf: format!("creatures/gecko/{file}"),
                sequence: KfParseEvidence::Parsed(NiControllerSequenceEvidence {
                    sequence_index: 0,
                    name: sequence_name.to_string(),
                    cycle: if file == "idle.kf" || file == "walk.kf" {
                        SequenceCycle::Loop
                    } else {
                        SequenceCycle::Clamp
                    },
                    start_time: 0.0,
                    stop_time: 1.0,
                    frequency: 1.0,
                    text_keys: Vec::new(),
                    binding: BindingEvidence {
                        source_skeleton_path: rig.skeleton_path.clone(),
                        transform_track_count: 12,
                        float_track_count: 0,
                        controller_types: vec!["NiTransformController".to_string()],
                        interpolator_types: vec!["NiTransformInterpolator".to_string()],
                        target_names: vec!["Bip01".to_string()],
                        required_float_slots: Vec::new(),
                        compatibility: BindingCompatibility::Verified,
                        compatibility_detail: None,
                    },
                    root_motion: RootMotionEvidence::None,
                }),
                idle_claims,
            })
            .collect::<Vec<_>>();
            let referenced_kfs = clips.iter().map(|clip| clip.source_kf.clone()).collect();
            let material = "materials/creatures/gecko/gecko.bgsm";
            let texture = "textures/creatures/gecko/gecko_d.dds";
            let mut assets = vec![
                asset(
                    LegacyCreatureGame::Fnv,
                    &rig.skeleton_path,
                    IndexedAssetKind::Skeleton,
                    vec![material],
                    vec!["Bip01"],
                ),
                asset(
                    LegacyCreatureGame::Fnv,
                    &body.body_paths[0],
                    IndexedAssetKind::Body,
                    vec![material],
                    vec![],
                ),
                asset(
                    LegacyCreatureGame::Fnv,
                    material,
                    IndexedAssetKind::Material,
                    vec![texture],
                    vec![],
                ),
                asset(
                    LegacyCreatureGame::Fnv,
                    texture,
                    IndexedAssetKind::Texture,
                    vec![],
                    vec![],
                ),
            ];
            assets.extend(clips.iter().map(|clip| {
                asset(
                    LegacyCreatureGame::Fnv,
                    &clip.source_kf,
                    IndexedAssetKind::Kf,
                    vec![],
                    vec![],
                )
            }));
            Self {
                records: vec![creature, idle, base_speed, run_mult],
                provenance: vec![
                    provenance(LegacyCreatureGame::Fnv, plugin),
                    provenance(LegacyCreatureGame::Fnv, plugin),
                    provenance(LegacyCreatureGame::Fnv, plugin),
                    provenance(LegacyCreatureGame::Fnv, plugin),
                ],
                assets,
                motion: vec![CreatureMotionFamilyEvidence {
                    rig: rig.clone(),
                    referenced_kfs,
                    kf_evidence: clips,
                    creature_traits: Vec::new(),
                }],
                graph: vec![CreatureGraphContractEvidence {
                    rig: rig.clone(),
                    capability: CreatureGraphCapability::GroundMelee,
                    required_roles: vec![
                        MotionRole::Idle,
                        MotionRole::GroundLocomotion,
                        MotionRole::MeleeAttack,
                        MotionRole::Death,
                    ],
                    requires_ragdoll: true,
                }],
                ragdoll: vec![RagdollCapabilityEvidence::Supported {
                    rig: rig.clone(),
                    mode: RagdollMode::EmbeddedSkeleton,
                    owner_asset: rig.skeleton_path.clone(),
                }],
                selections: vec![FamilyRaceDataSelection {
                    rig: rig.clone(),
                    source_creature: creature_key,
                    source_body: body.clone(),
                }],
                rigs: vec![RigRaceDataEvidence {
                    rig: rig.clone(),
                    skeleton_bounds: Some(MeasuredBounds {
                        min: [-1.0, -1.0, 0.0],
                        max: [1.0, 1.0, 2.0],
                    }),
                    up_axis: MeasurementAxis::Z,
                    geometry_scale_to_fo4: Some(1.0),
                }],
                bodies: vec![BodyRaceDataEvidence {
                    body,
                    body_bounds: Some(MeasuredBounds {
                        min: [-1.0, -1.0, 0.0],
                        max: [1.0, 1.0, 2.0],
                    }),
                }],
                controllers: vec![ControllerRaceDataEvidence {
                    rig,
                    capsule: Some(CapsuleEvidence {
                        radius: 0.5,
                        total_height: 2.0,
                        architecture: ControllerArchitecture::Quadruped,
                    }),
                    movement: Some(MovementSemantics {
                        architecture: MovementArchitecture::Grounded,
                        linear: Some(LinearMotionEvidence {
                            max_speed: 154.0,
                            seconds_to_full_speed: 0.5,
                            seconds_to_stop: 0.25,
                        }),
                        angular: Some(AngularMotionEvidence {
                            max_yaw_speed_degrees_per_second: 90.0,
                            seconds_to_full_yaw_speed: 0.5,
                            tolerance_degrees: 5.0,
                            aim_tolerance_degrees: 10.0,
                            pitch_limit_degrees: 30.0,
                            roll_limit_degrees: 30.0,
                        }),
                        pushable: true,
                        opens_doors: false,
                        allow_ragdoll_collision: true,
                        non_hostile: false,
                        use_large_actor_pathing: false,
                        use_subsegmented_damage: false,
                    }),
                }],
                policy: Fo4RaceDataScalePolicy {
                    small_max_stature: 1.0,
                    medium_max_stature: 3.0,
                    large_max_stature: 10.0,
                },
            }
        }

        fn build(
            &self,
            interner: &StringInterner,
        ) -> Result<CreatureFamilyRecipeLedger, CreatureRecipeBuildError> {
            let sources = self
                .records
                .iter()
                .zip(&self.provenance)
                .map(|(record, provenance)| LegacyRecordSource {
                    record,
                    provenance: provenance.clone(),
                })
                .collect::<Vec<_>>();
            build_creature_family_recipe(
                CreatureRecipeBuildInput {
                    winning_records: &sources,
                    indexed_assets: &self.assets,
                    motion_families: &self.motion,
                    graph_contracts: &self.graph,
                    ragdoll_evidence: &self.ragdoll,
                    race_data: LegacyRaceDataEvidenceInputs {
                        selections: &self.selections,
                        rigs: &self.rigs,
                        bodies: &self.bodies,
                        controllers: &self.controllers,
                        creatures: &[],
                    },
                    race_data_policy: &self.policy,
                    expected_creature_winners: Some(1),
                },
                interner,
            )
        }
    }

    pub(crate) fn source_rig_bridge_test_ledger(
        interner: &StringInterner,
        game: LegacyCreatureGame,
    ) -> CreatureFamilyRecipeLedger {
        let mut fixture = GroundFixture::new(interner);
        fixture.graph[0].requires_ragdoll = false;
        fixture.ragdoll[0] = RagdollCapabilityEvidence::ExplicitlyUnsupported {
            rig: fixture.graph[0].rig.clone(),
            reason: "source has no articulated ragdoll".to_string(),
        };
        for evidence in &mut fixture.motion[0].kf_evidence {
            let KfParseEvidence::Parsed(sequence) = &mut evidence.sequence else {
                continue;
            };
            sequence.binding.transform_track_count = 4;
            sequence.binding.target_names = (0..4).map(|index| format!("Bone{index}")).collect();
            sequence.text_keys = match sequence.name.as_str() {
                "WalkForward" => vec![TextKeyEvidence {
                    time: 0.0,
                    text: "startWalk".to_string(),
                }],
                "AttackBite" => vec![TextKeyEvidence {
                    time: 0.5,
                    text: "meleeGeckoBite".to_string(),
                }],
                _ => Vec::new(),
            };
        }
        let ledger = fixture.build(interner).unwrap();
        if game == LegacyCreatureGame::Fnv {
            return ledger;
        }
        fn replace(value: &mut serde_json::Value) {
            match value {
                serde_json::Value::String(text) => {
                    if text == "fnv" {
                        *text = "fo3".to_string();
                    } else if text.eq_ignore_ascii_case("falloutnv.esm") {
                        *text = "fallout3.esm".to_string();
                    }
                }
                serde_json::Value::Array(values) => values.iter_mut().for_each(replace),
                serde_json::Value::Object(values) => values.values_mut().for_each(replace),
                _ => {}
            }
        }
        let mut value = serde_json::to_value(ledger).unwrap();
        replace(&mut value);
        let mut ledger: CreatureFamilyRecipeLedger = serde_json::from_value(value).unwrap();
        ledger.accounting.fnv_creatures = 0;
        ledger.accounting.fo3_creatures = ledger.accounting.winning_creatures;
        ledger.content_hash_blake3.clear();
        ledger.content_hash_blake3 = ledger.compute_content_hash_blake3().unwrap();
        ledger.validate().unwrap();
        ledger
    }

    pub(crate) fn source_rig_bridge_ranged_test_ledger(
        interner: &StringInterner,
    ) -> CreatureFamilyRecipeLedger {
        let mut ledger = source_rig_bridge_test_ledger(interner, LegacyCreatureGame::Fnv);
        let family = &mut ledger.families[0];
        let contract = family.graph_contract.as_mut().unwrap();
        contract.capability = CreatureGraphCapability::GroundRangedProjectile;
        contract.required_roles = vec![
            MotionRole::Idle,
            MotionRole::GroundLocomotion,
            MotionRole::RangedAttack,
            MotionRole::Projectile,
            MotionRole::Death,
        ];
        contract.required_roles.sort();
        let melee = family
            .required_motion
            .iter_mut()
            .find(|required| required.role == MotionRole::MeleeAttack)
            .unwrap();
        melee.role = MotionRole::RangedAttack;
        let mut projectile = family
            .required_motion
            .iter()
            .find(|required| required.role == MotionRole::Death)
            .unwrap()
            .clone();
        projectile.role = MotionRole::Projectile;
        family.required_motion.push(projectile);
        family.required_motion.sort_by_key(|required| required.role);
        ledger.content_hash_blake3.clear();
        ledger.content_hash_blake3 = ledger.compute_content_hash_blake3().unwrap();
        ledger.validate().unwrap();
        ledger
    }

    pub(crate) fn source_rig_bridge_bind_nif_hashes(
        ledger: &mut CreatureFamilyRecipeLedger,
        hashes: &[(&str, &str)],
    ) {
        for (path, hash) in hashes {
            let asset = ledger.families[0]
                .assets
                .iter_mut()
                .find(|asset| asset.path == *path)
                .unwrap();
            asset.content_hash_blake3 = (*hash).to_string();
        }
        ledger.content_hash_blake3.clear();
        ledger.content_hash_blake3 = ledger.compute_content_hash_blake3().unwrap();
        ledger.validate().unwrap();
    }

    fn asset(
        game: LegacyCreatureGame,
        path: &str,
        kind: IndexedAssetKind,
        dependencies: Vec<&str>,
        root_nodes: Vec<&str>,
    ) -> IndexedSourceAssetEvidence {
        let hash_input = format!("{game:?}:{kind:?}:{path}");
        IndexedSourceAssetEvidence {
            game,
            path: path.to_string(),
            kind,
            content_hash_blake3: blake3::hash(hash_input.as_bytes()).to_hex().to_string(),
            dependencies: dependencies.into_iter().map(str::to_string).collect(),
            root_nodes: root_nodes.into_iter().map(str::to_string).collect(),
            skeleton_runtime_name: (kind == IndexedAssetKind::Skeleton)
                .then(|| "NVGecko".to_string()),
            skeleton_float_slots: Vec::new(),
        }
    }

    fn family_blockers(ledger: &CreatureFamilyRecipeLedger) -> &[FamilyRecipeBlocker] {
        match &ledger.families[0].disposition {
            FamilyRecipeDisposition::Ready => &[],
            FamilyRecipeDisposition::Rejected { blockers } => blockers,
        }
    }

    #[test]
    fn builds_complete_gecko_like_ground_melee_recipe() {
        let interner = StringInterner::new();
        let ledger = GroundFixture::new(&interner).build(&interner).unwrap();
        assert_eq!(ledger.schema_version, CREATURE_RECIPE_SCHEMA_VERSION);
        assert_eq!(ledger.accounting.record_dispositions, 1);
        assert_eq!(ledger.accounting.ready_families, 1);
        assert_eq!(
            ledger.families[0].actual_root_node.as_deref(),
            Some("Bip01")
        );
        assert_eq!(ledger.families[0].required_motion.len(), 4);
        assert!(ledger.families[0].assets.iter().any(|asset| {
            asset.kind == IndexedAssetKind::Texture
                && asset.path == "textures/creatures/gecko/gecko_d.dds"
        }));
        assert_eq!(
            ledger.families[0]
                .race_data
                .as_ref()
                .unwrap()
                .policy_receipts
                .len(),
            4
        );
    }

    #[test]
    fn missing_optional_recursive_nif_is_not_claimed_in_a_ready_asset_closure() {
        let interner = StringInterner::new();
        let mut fixture = GroundFixture::new(&interner);
        fixture
            .assets
            .iter_mut()
            .find(|asset| asset.kind == IndexedAssetKind::Skeleton)
            .unwrap()
            .dependencies
            .push("creatures/gecko/optional_attachment.nif".to_string());

        let ledger = fixture.build(&interner).unwrap();

        assert_eq!(ledger.accounting.ready_families, 1);
        assert!(
            ledger.families[0]
                .recursive_asset_claims
                .iter()
                .all(|claim| {
                    !claim
                        .closure
                        .iter()
                        .any(|path| path == "creatures/gecko/optional_attachment.nif")
                })
        );
    }

    #[test]
    fn robot_and_overlay_capabilities_require_their_typed_roles() {
        let interner = StringInterner::new();
        for (capability, missing_role) in [
            (
                CreatureGraphCapability::RobotContinuousAttack,
                MotionRole::ContinuousOrRobot,
            ),
            (
                CreatureGraphCapability::HumanoidWeaponOverlay,
                MotionRole::Overlay,
            ),
        ] {
            let mut fixture = GroundFixture::new(&interner);
            fixture.graph[0].capability = capability;
            let ledger = fixture.build(&interner).unwrap();
            assert!(
                family_blockers(&ledger)
                    .contains(&FamilyRecipeBlocker::MissingCapabilityRole(missing_role))
            );
        }
    }

    #[test]
    fn missing_asset_role_gmst_root_and_capsule_are_never_defaulted() {
        let interner = StringInterner::new();

        let mut fixture = GroundFixture::new(&interner);
        fixture
            .assets
            .retain(|asset| asset.kind != IndexedAssetKind::Texture);
        let ledger = fixture.build(&interner).unwrap();
        assert!(family_blockers(&ledger).iter().any(
            |blocker| matches!(blocker, FamilyRecipeBlocker::MissingAsset(path) if path.ends_with("gecko_d.dds"))
        ));

        let mut fixture = GroundFixture::new(&interner);
        fixture.graph[0]
            .required_roles
            .retain(|role| *role != MotionRole::Death);
        fixture.motion[0]
            .referenced_kfs
            .retain(|path| !path.ends_with("death.kf"));
        fixture.motion[0]
            .kf_evidence
            .retain(|evidence| !evidence.source_kf.ends_with("death.kf"));
        let ledger = fixture.build(&interner).unwrap();
        assert!(
            !family_blockers(&ledger)
                .contains(&FamilyRecipeBlocker::MissingRequiredRole(MotionRole::Death))
        );

        let mut fixture = GroundFixture::new(&interner);
        fixture
            .records
            .retain(|record| record.sig.as_str() != "GMST");
        fixture.provenance.truncate(fixture.records.len());
        let ledger = fixture.build(&interner).unwrap();
        assert!(
            family_blockers(&ledger).contains(&FamilyRecipeBlocker::MissingMovementSettings(
                LegacyCreatureGame::Fnv
            ))
        );

        let mut fixture = GroundFixture::new(&interner);
        fixture.assets[0].root_nodes.clear();
        let ledger = fixture.build(&interner).unwrap();
        assert!(
            family_blockers(&ledger)
                .iter()
                .any(|blocker| matches!(blocker, FamilyRecipeBlocker::MissingActualRoot(_)))
        );

        let mut fixture = GroundFixture::new(&interner);
        fixture
            .assets
            .iter_mut()
            .find(|asset| asset.kind == IndexedAssetKind::Skeleton)
            .unwrap()
            .skeleton_runtime_name = None;
        let ledger = fixture.build(&interner).unwrap();
        assert!(family_blockers(&ledger).iter().any(|blocker| matches!(
            blocker,
            FamilyRecipeBlocker::InvalidAssetEvidence { detail, .. }
                if detail.contains("skeleton runtime name")
        )));

        let mut fixture = GroundFixture::new(&interner);
        fixture.controllers[0].capsule = None;
        let ledger = fixture.build(&interner).unwrap();
        assert!(family_blockers(&ledger).contains(&FamilyRecipeBlocker::MissingCapsule));
    }

    #[test]
    fn proxy_maps_to_terminal_family_and_provenance_survives_both_games() {
        let interner = StringInterner::new();
        let mut fixture = GroundFixture::new(&interner);
        let owner_key = fixture.records[0].form_key;
        fixture.records.push(record(
            "CREA",
            0x101,
            "FalloutNV.esm",
            "GeckoProxy",
            vec![field("TPLT", FieldValue::FormKey(owner_key))],
            &interner,
        ));
        fixture
            .provenance
            .push(provenance(LegacyCreatureGame::Fnv, "FalloutNV.esm"));
        let fo3 = record(
            "CREA",
            0x500,
            "Fallout3.esm",
            "Fo3Special",
            Vec::new(),
            &interner,
        );
        fixture.records.push(fo3);
        fixture
            .provenance
            .push(provenance(LegacyCreatureGame::Fo3, "Fallout3.esm"));
        let sources = fixture
            .records
            .iter()
            .zip(&fixture.provenance)
            .map(|(record, provenance)| LegacyRecordSource {
                record,
                provenance: provenance.clone(),
            })
            .collect::<Vec<_>>();
        let ledger = build_creature_family_recipe(
            CreatureRecipeBuildInput {
                winning_records: &sources,
                indexed_assets: &fixture.assets,
                motion_families: &fixture.motion,
                graph_contracts: &fixture.graph,
                ragdoll_evidence: &fixture.ragdoll,
                race_data: LegacyRaceDataEvidenceInputs {
                    selections: &fixture.selections,
                    rigs: &fixture.rigs,
                    bodies: &fixture.bodies,
                    controllers: &fixture.controllers,
                    creatures: &[],
                },
                race_data_policy: &fixture.policy,
                expected_creature_winners: Some(3),
            },
            &interner,
        )
        .unwrap();
        assert_eq!(ledger.accounting.fnv_creatures, 2);
        assert_eq!(ledger.accounting.fo3_creatures, 1);
        assert!(matches!(
            ledger
                .records
                .iter()
                .find(|entry| entry.source.local == 0x101)
                .unwrap()
                .disposition,
            CreatureRecordRecipeDisposition::Proxy { .. }
        ));
    }

    #[test]
    fn same_winner_collision_is_rejected_instead_of_erasing_game_provenance() {
        let interner = StringInterner::new();
        let first = record(
            "CREA",
            0x100,
            "Collision.esm",
            "First",
            Vec::new(),
            &interner,
        );
        let second = record(
            "CREA",
            0x100,
            "Collision.esm",
            "Second",
            Vec::new(),
            &interner,
        );
        let policy = Fo4RaceDataScalePolicy {
            small_max_stature: 1.0,
            medium_max_stature: 2.0,
            large_max_stature: 3.0,
        };
        let sources = [
            LegacyRecordSource {
                record: &first,
                provenance: provenance(LegacyCreatureGame::Fnv, "FalloutNV.esm"),
            },
            LegacyRecordSource {
                record: &second,
                provenance: provenance(LegacyCreatureGame::Fo3, "Fallout3.esm"),
            },
        ];
        let error = build_creature_family_recipe(
            CreatureRecipeBuildInput {
                winning_records: &sources,
                indexed_assets: &[],
                motion_families: &[],
                graph_contracts: &[],
                ragdoll_evidence: &[],
                race_data: LegacyRaceDataEvidenceInputs::default(),
                race_data_policy: &policy,
                expected_creature_winners: None,
            },
            &interner,
        )
        .unwrap_err();
        assert!(matches!(
            error,
            CreatureRecipeBuildError::Catalog(CreatureCatalogError::WinnerCollision { .. })
        ));
    }

    #[test]
    fn canonical_json_hash_and_roundtrip_ignore_input_order() {
        let interner = StringInterner::new();
        let fixture = GroundFixture::new(&interner);
        let first = fixture.build(&interner).unwrap();
        let mut reordered = fixture.clone();
        reordered.records.reverse();
        reordered.provenance.reverse();
        reordered.assets.reverse();
        reordered.motion[0].referenced_kfs.reverse();
        reordered.motion[0].kf_evidence.reverse();
        reordered.graph[0].required_roles.reverse();
        let second = reordered.build(&interner).unwrap();
        assert_eq!(
            first.canonical_json().unwrap(),
            second.canonical_json().unwrap()
        );
        assert_eq!(
            first.canonical_hash_blake3().unwrap(),
            second.canonical_hash_blake3().unwrap()
        );
        assert!(canonical_blake3_hash(&first.content_hash_blake3));
        let reopened =
            CreatureFamilyRecipeLedger::from_json(&first.canonical_json().unwrap()).unwrap();
        assert_eq!(first, reopened);
    }

    #[test]
    fn asset_hashes_require_real_canonical_blake3_digests() {
        let interner = StringInterner::new();
        let fixture = GroundFixture::new(&interner);
        assert!(
            fixture
                .assets
                .iter()
                .all(|asset| canonical_blake3_hash(&asset.content_hash_blake3))
        );

        let valid = fixture.assets[1].content_hash_blake3.clone();
        for invalid in [
            "hash-placeholder".to_string(),
            valid.to_ascii_uppercase(),
            "0".repeat(64),
        ] {
            let mut invalid_fixture = fixture.clone();
            let invalid_path = invalid_fixture.assets[1].path.clone();
            invalid_fixture.assets[1].content_hash_blake3 = invalid;
            let ledger = invalid_fixture.build(&interner).unwrap();
            assert!(family_blockers(&ledger).iter().any(|blocker| {
                matches!(
                    blocker,
                    FamilyRecipeBlocker::InvalidAssetEvidence { path, detail }
                        if path == &invalid_path && detail.contains("lowercase 64-hex")
                )
            }));
            assert!(
                ledger.families[0]
                    .assets
                    .iter()
                    .all(|asset| asset.path != invalid_path)
            );
        }
    }

    #[test]
    fn strict_validation_rejects_tamper_accounting_hash_and_order_drift() {
        let interner = StringInterner::new();
        let ledger = GroundFixture::new(&interner).build(&interner).unwrap();

        let mut tampered = ledger.clone();
        tampered.families[0].actual_root_node = Some("TamperedRoot".to_string());
        assert!(matches!(
            tampered.validate(),
            Err(CreatureRecipeBuildError::HashMismatch { .. })
        ));
        let tampered_json = serde_json::to_string(&tampered).unwrap();
        assert!(matches!(
            CreatureFamilyRecipeLedger::from_json(&tampered_json),
            Err(CreatureRecipeBuildError::HashMismatch { .. })
        ));

        let mut accounting_drift = ledger.clone();
        accounting_drift.accounting.winning_creatures += 1;
        assert!(matches!(
            accounting_drift.validate(),
            Err(CreatureRecipeBuildError::InvalidLedger(_))
        ));

        let mut order_drift = ledger.clone();
        order_drift.families[0].assets.reverse();
        assert!(matches!(
            order_drift.validate(),
            Err(CreatureRecipeBuildError::InvalidLedger(_))
        ));

        let mut noncanonical_ledger_hash = ledger.clone();
        noncanonical_ledger_hash
            .content_hash_blake3
            .make_ascii_uppercase();
        assert!(matches!(
            noncanonical_ledger_hash.validate(),
            Err(CreatureRecipeBuildError::InvalidLedger(_))
        ));

        let mut noncanonical_asset_hash = ledger;
        noncanonical_asset_hash.families[0].assets[0]
            .content_hash_blake3
            .make_ascii_uppercase();
        assert!(matches!(
            noncanonical_asset_hash.validate(),
            Err(CreatureRecipeBuildError::InvalidLedger(_))
        ));

        let mut unknown_field = serde_json::to_value(&noncanonical_asset_hash).unwrap();
        unknown_field
            .as_object_mut()
            .unwrap()
            .insert("unexpected".to_string(), serde_json::Value::Bool(true));
        assert!(matches!(
            CreatureFamilyRecipeLedger::from_json(&unknown_field.to_string()),
            Err(CreatureRecipeBuildError::Serialization(_))
        ));
    }

    #[test]
    fn full_census_gate_accounts_for_every_expected_merged_winner() {
        let interner = StringInterner::new();
        let records = (0..super::super::creature_catalog::EXPECTED_FULL_MERGED_CREA_WINNERS)
            .map(|offset| {
                record(
                    "CREA",
                    0x1000 + offset as u32,
                    "FalloutNV.esm",
                    &format!("Creature{offset}"),
                    Vec::new(),
                    &interner,
                )
            })
            .collect::<Vec<_>>();
        let sources = records
            .iter()
            .map(|record| LegacyRecordSource {
                record,
                provenance: provenance(LegacyCreatureGame::Fnv, "FalloutNV.esm"),
            })
            .collect::<Vec<_>>();
        let policy = Fo4RaceDataScalePolicy {
            small_max_stature: 1.0,
            medium_max_stature: 2.0,
            large_max_stature: 3.0,
        };
        let ledger = build_full_merged_creature_family_recipe(
            CreatureRecipeBuildInput {
                winning_records: &sources,
                indexed_assets: &[],
                motion_families: &[],
                graph_contracts: &[],
                ragdoll_evidence: &[],
                race_data: LegacyRaceDataEvidenceInputs::default(),
                race_data_policy: &policy,
                expected_creature_winners: None,
            },
            &interner,
        )
        .unwrap();
        assert_eq!(
            ledger.accounting.record_dispositions,
            super::super::creature_catalog::EXPECTED_FULL_MERGED_CREA_WINNERS
        );
        assert_eq!(ledger.families.len(), 0);
    }
}
