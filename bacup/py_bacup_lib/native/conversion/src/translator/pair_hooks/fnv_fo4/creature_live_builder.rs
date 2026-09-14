//! Live, source-owned evidence construction for the merged FNV/FO3 creature MVP.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use havok_native::convert::creature_ragdoll::reconstruct_fo4_creature_ragdoll_packfile;
use nif_core_native::creature_ragdoll::{
    LegacyCreatureRagdollError, extract_fnv_fo3_creature_ragdoll,
};
use nif_core_native::model::NifFile;
use thiserror::Error;

use crate::record::Record;
use crate::source_rig::CreatureGraphTemplate;
use crate::source_rig::race_data::{Fo4RaceDataScalePolicy, MeasurementAxis};
use crate::sym::StringInterner;

use super::creature_catalog::{
    AttackProfile, CreatureCatalogError, CreatureCatalogOptions, CreatureCorpusPlan,
    LegacyCreatureGame, LegacyRecordSource, RigFamilyKey, build_creature_corpus_plan,
    game_mesh_root,
};
use super::creature_motion::{
    CreatureFamilyMotionSet, CreatureKfEvidence, CreatureKfSkeletonContract,
    CreatureMotionCatalogError, CreatureMotionFamilyEvidence, KfParseEvidence, MotionRole,
    build_creature_motion_catalog, parse_creature_kf_evidence,
};
use super::creature_race_data::{
    LegacyMovementGraphSelection, LegacyMovementMvpIssue, LegacyRaceDataEvidenceInputs,
    RigNifMeasurementContext, build_family_race_data_selections,
    build_legacy_movement_mvp_contexts, load_creature_record_race_data_evidence,
    load_legacy_movement_base_settings, load_nif_race_data_evidence_with_controllers,
};
use super::creature_recipe::{
    CreatureFamilyRecipeLedger, CreatureGraphCapability, CreatureGraphContractEvidence,
    CreatureRecipeBuildError, CreatureRecipeBuildInput, IndexedAssetKind,
    IndexedSourceAssetEvidence, RagdollCapabilityEvidence, RagdollMode,
    build_creature_family_recipe,
};

const TARGET_DEFAULT_AIM_TOLERANCE_DEGREES: f32 = 30.0;
const TARGET_DEFAULT_PITCH_LIMIT_DEGREES: f32 = 45.0;
const TARGET_DEFAULT_ROLL_LIMIT_DEGREES: f32 = 20.0;

#[derive(Clone, Copy)]
pub struct FnvFo3LiveRecipeBuildInput<'a> {
    pub winning_records: &'a [LegacyRecordSource<'a>],
    pub source_data_root: &'a Path,
    pub expected_creature_winners: Option<usize>,
}

#[derive(Clone, Debug)]
pub struct FnvFo3LiveRecipeEvidence {
    pub catalog: CreatureCorpusPlan,
    pub indexed_assets: Vec<IndexedSourceAssetEvidence>,
    pub motion_families: Vec<CreatureMotionFamilyEvidence>,
    pub motion_sets: Vec<CreatureFamilyMotionSet>,
    pub movement_issues: Vec<LegacyMovementMvpIssue>,
    pub graph_contracts: Vec<CreatureGraphContractEvidence>,
    pub ragdoll_evidence: Vec<RagdollCapabilityEvidence>,
    pub recipe_ledger: CreatureFamilyRecipeLedger,
}

#[derive(Debug, Error)]
pub enum FnvFo3LiveRecipeBuildError {
    #[error(transparent)]
    Catalog(#[from] CreatureCatalogError),
    #[error(transparent)]
    Motion(#[from] CreatureMotionCatalogError),
    #[error(transparent)]
    Recipe(#[from] CreatureRecipeBuildError),
    #[error("invalid live FNV/FO3 recipe input: {0}")]
    Input(String),
}

pub fn build_live_fnv_fo3_creature_recipe_evidence(
    input: FnvFo3LiveRecipeBuildInput<'_>,
    interner: &StringInterner,
) -> Result<FnvFo3LiveRecipeEvidence, FnvFo3LiveRecipeBuildError> {
    let mesh_root = extracted_mesh_root(input.source_data_root);
    validate_game_asset_roots(input.winning_records, &mesh_root)?;
    let catalog = build_creature_corpus_plan(
        input.winning_records,
        CreatureCatalogOptions {
            mesh_root: Some(&mesh_root),
            expected_creature_winners: input.expected_creature_winners,
        },
        interner,
    )?;
    let required_float_slots = collect_required_float_slots(&catalog, &mesh_root);
    let skeletons = reconstruct_skeleton_evidence(&catalog, &mesh_root, &required_float_slots);
    let motion_families = load_motion_evidence(&catalog, &mesh_root, &skeletons);
    let motion_catalog = build_creature_motion_catalog(&motion_families)?;
    let ragdoll_evidence = load_ragdoll_evidence(&catalog, &mesh_root);
    let graph_contracts =
        build_graph_contracts(&catalog, &motion_catalog.families, &ragdoll_evidence);
    let indexed_assets = index_source_assets(&catalog, &mesh_root, &skeletons);

    let selections = build_family_race_data_selections(&catalog);
    let source_records = input
        .winning_records
        .iter()
        .map(|source| source.record.clone())
        .collect::<Vec<Record>>();
    let creature_data =
        load_creature_record_race_data_evidence(&catalog, &selections, &source_records, interner);
    let base_settings =
        load_legacy_movement_base_settings(&catalog, input.winning_records, interner);
    let movement_selections = build_movement_selections(&graph_contracts, &motion_catalog.families);
    let movement = build_legacy_movement_mvp_contexts(
        &catalog,
        &selections,
        &movement_selections,
        &base_settings.settings,
        &motion_families,
        &source_records,
        interner,
    );
    let rig_contexts = catalog
        .rig_families
        .iter()
        .map(|family| RigNifMeasurementContext {
            rig: family.key.clone(),
            up_axis: MeasurementAxis::Z,
            geometry_scale_to_fo4: Some(1.0),
        })
        .collect::<Vec<_>>();
    let nif_data = load_nif_race_data_evidence_with_controllers(
        &catalog,
        &mesh_root,
        &rig_contexts,
        &movement.controllers,
    );
    let race_data = LegacyRaceDataEvidenceInputs {
        selections: &selections,
        rigs: &nif_data.rigs,
        bodies: &nif_data.bodies,
        controllers: &nif_data.controllers,
        creatures: &creature_data.creatures,
    };
    let policy = Fo4RaceDataScalePolicy {
        small_max_stature: 64.0,
        medium_max_stature: 128.0,
        large_max_stature: 256.0,
    };
    let recipe_ledger = build_creature_family_recipe(
        CreatureRecipeBuildInput {
            winning_records: input.winning_records,
            indexed_assets: &indexed_assets,
            motion_families: &motion_families,
            graph_contracts: &graph_contracts,
            ragdoll_evidence: &ragdoll_evidence,
            race_data,
            race_data_policy: &policy,
            expected_creature_winners: input.expected_creature_winners,
        },
        interner,
    )?;

    Ok(FnvFo3LiveRecipeEvidence {
        catalog,
        indexed_assets,
        motion_families,
        motion_sets: motion_catalog.families,
        movement_issues: movement.issues,
        graph_contracts,
        ragdoll_evidence,
        recipe_ledger,
    })
}

#[derive(Clone, Debug)]
struct LiveSkeletonEvidence {
    root_nodes: Vec<String>,
    runtime_name: String,
    ordered_bones: Vec<String>,
    float_slots: Vec<String>,
}

fn reconstruct_skeleton_evidence(
    catalog: &CreatureCorpusPlan,
    mesh_root: &Path,
    required_float_slots: &BTreeMap<RigFamilyKey, Vec<String>>,
) -> BTreeMap<RigFamilyKey, LiveSkeletonEvidence> {
    let mut skeletons = BTreeMap::new();
    for family in &catalog.rig_families {
        let family_mesh_root = game_mesh_root(mesh_root, family.key.game);
        let source = asset_path(&family_mesh_root, &family.key.skeleton_path);
        let runtime_name = source
            .file_stem()
            .and_then(|value| value.to_str())
            .filter(|value| !value.trim().is_empty())
            .unwrap_or("Skeleton");
        let Ok(receipt) = crate::phase::skeleton::convert_source_owned_skeleton_artifact(
            &source,
            "Actors\\B21_FnvFo3Evidence\\CharacterAssets\\Skeleton.hkx",
            runtime_name,
            required_float_slots
                .get(&family.key)
                .map(Vec::as_slice)
                .unwrap_or_default(),
        ) else {
            continue;
        };
        let root_nodes = receipt
            .ordered_bone_names
            .iter()
            .zip(&receipt.parent_indices)
            .filter_map(|(name, parent)| (*parent < 0).then_some(name.clone()))
            .collect();
        skeletons.insert(
            family.key.clone(),
            LiveSkeletonEvidence {
                root_nodes,
                runtime_name: receipt.skeleton_name,
                ordered_bones: receipt.ordered_bone_names,
                float_slots: receipt.float_slot_names,
            },
        );
    }
    skeletons
}

fn collect_required_float_slots(
    catalog: &CreatureCorpusPlan,
    mesh_root: &Path,
) -> BTreeMap<RigFamilyKey, Vec<String>> {
    catalog
        .rig_families
        .iter()
        .map(|family| {
            let family_mesh_root = game_mesh_root(mesh_root, family.key.game);
            let mut slots = BTreeMap::<String, String>::new();
            for source_kf in &family.recursive_kf_paths {
                let source = asset_path(&family_mesh_root, source_kf);
                let Ok(bytes) = fs::read(source) else {
                    continue;
                };
                let Ok(evidence) =
                    parse_creature_kf_evidence(&bytes, source_kf, family.key.game, None, None)
                else {
                    continue;
                };
                let KfParseEvidence::Parsed(sequence) = evidence.sequence else {
                    continue;
                };
                for slot in sequence.binding.required_float_slots {
                    merge_animation_slot(&mut slots, slot);
                }
            }
            (family.key.clone(), slots.into_values().collect())
        })
        .collect()
}

fn merge_animation_slot(slots: &mut BTreeMap<String, String>, slot: String) {
    let slot = slot.split_whitespace().collect::<Vec<_>>().join(" ");
    let normalized = normalize_animation_slot(&slot);
    slots
        .entry(normalized)
        .and_modify(|current| {
            if slot < *current {
                *current = slot.clone();
            }
        })
        .or_insert(slot);
}

fn normalize_animation_slot(name: &str) -> String {
    name.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase()
}

fn load_motion_evidence(
    catalog: &CreatureCorpusPlan,
    mesh_root: &Path,
    skeletons: &BTreeMap<RigFamilyKey, LiveSkeletonEvidence>,
) -> Vec<CreatureMotionFamilyEvidence> {
    catalog
        .rig_families
        .iter()
        .map(|family| {
            let family_mesh_root = game_mesh_root(mesh_root, family.key.game);
            let skeleton = skeletons.get(&family.key);
            let kf_evidence = family
                .recursive_kf_paths
                .iter()
                .map(|source_kf| {
                    let source = asset_path(&family_mesh_root, source_kf);
                    let bytes = match fs::read(&source) {
                        Ok(bytes) => bytes,
                        Err(_) => {
                            return CreatureKfEvidence {
                                source_game: family.key.game,
                                source_kf: source_kf.clone(),
                                sequence: KfParseEvidence::MissingAsset,
                                idle_claims: Vec::new(),
                            };
                        }
                    };
                    let contract = skeleton.map(|skeleton| CreatureKfSkeletonContract {
                        skeleton_path: &family.key.skeleton_path,
                        ordered_bone_names: &skeleton.ordered_bones,
                        ordered_float_slot_names: &skeleton.float_slots,
                    });
                    parse_creature_kf_evidence(&bytes, source_kf, family.key.game, None, contract)
                        .unwrap_or_else(|error| CreatureKfEvidence {
                            source_game: family.key.game,
                            source_kf: source_kf.clone(),
                            sequence: KfParseEvidence::ParseFailed(error.to_string()),
                            idle_claims: Vec::new(),
                        })
                })
                .collect();
            CreatureMotionFamilyEvidence {
                rig: family.key.clone(),
                referenced_kfs: family.recursive_kf_paths.clone(),
                kf_evidence,
                creature_traits: Vec::new(),
            }
        })
        .collect()
}

fn load_ragdoll_evidence(
    catalog: &CreatureCorpusPlan,
    mesh_root: &Path,
) -> Vec<RagdollCapabilityEvidence> {
    catalog
        .rig_families
        .iter()
        .map(|family| {
            let family_mesh_root = game_mesh_root(mesh_root, family.key.game);
            let source = asset_path(&family_mesh_root, &family.key.skeleton_path);
            match NifFile::load(&source)
                .map_err(|error| error.to_string())
                .and_then(|nif| {
                    extract_fnv_fo3_creature_ragdoll(&nif).map_err(|error| error.to_string())
                })
                .and_then(|ir| {
                    reconstruct_fo4_creature_ragdoll_packfile(&ir)
                        .map(|_| ())
                        .map_err(|error| error.to_string())
                }) {
                Ok(_) => RagdollCapabilityEvidence::Supported {
                    rig: family.key.clone(),
                    mode: RagdollMode::EmbeddedSkeleton,
                    owner_asset: family.key.skeleton_path.clone(),
                },
                Err(reason) if reason == LegacyCreatureRagdollError::MissingRagdoll.to_string() => {
                    RagdollCapabilityEvidence::ExplicitlyUnsupported {
                        rig: family.key.clone(),
                        reason: "source skeleton contains no articulated ragdoll".to_string(),
                    }
                }
                Err(reason) => RagdollCapabilityEvidence::ExplicitlyUnsupported {
                    rig: family.key.clone(),
                    reason: format!(
                        "source ragdoll is present but cannot be represented: {reason}"
                    ),
                },
            }
        })
        .collect()
}

fn build_graph_contracts(
    catalog: &CreatureCorpusPlan,
    motion: &[CreatureFamilyMotionSet],
    ragdolls: &[RagdollCapabilityEvidence],
) -> Vec<CreatureGraphContractEvidence> {
    let motion = motion
        .iter()
        .map(|family| (family.rig.clone(), family))
        .collect::<BTreeMap<_, _>>();
    let ragdolls = ragdolls
        .iter()
        .map(|evidence| {
            let rig = match evidence {
                RagdollCapabilityEvidence::Supported { rig, .. }
                | RagdollCapabilityEvidence::ExplicitlyUnsupported { rig, .. } => rig.clone(),
            };
            (rig, evidence)
        })
        .collect::<BTreeMap<_, _>>();
    let mut contracts = catalog
        .attack_sets
        .iter()
        .map(|attack| {
            let family_motion = motion.get(&attack.rig).copied();
            let capability = infer_capability(attack, family_motion);
            let required_roles = required_capability_roles(capability, attack, family_motion);
            let requires_ragdoll = matches!(
                ragdolls.get(&attack.rig),
                Some(RagdollCapabilityEvidence::Supported { .. })
            );
            CreatureGraphContractEvidence {
                rig: attack.rig.clone(),
                capability,
                required_roles,
                requires_ragdoll,
            }
        })
        .collect::<Vec<_>>();
    contracts.sort_by(|left, right| left.rig.cmp(&right.rig));
    contracts
}

fn infer_capability(
    attack: &super::creature_catalog::AttackSet,
    motion: Option<&CreatureFamilyMotionSet>,
) -> CreatureGraphCapability {
    let has = |role| {
        motion
            .and_then(|motion| motion.role(role))
            .is_some_and(|entry| !entry.candidates.is_empty())
    };
    let has_ground = has(MotionRole::GroundLocomotion);
    let has_mobility = has_ground || has(MotionRole::Fly) || has(MotionRole::Swim);
    let has_melee = has(MotionRole::MeleeAttack)
        || matches!(attack.profile, AttackProfile::Melee | AttackProfile::Mixed);
    let has_ranged = has(MotionRole::RangedAttack)
        || has(MotionRole::Projectile)
        || matches!(attack.profile, AttackProfile::Ranged | AttackProfile::Mixed);
    if matches!(attack.profile, AttackProfile::Undetermined)
        && !has_melee
        && !has_ranged
        && !has(MotionRole::ContinuousOrRobot)
    {
        CreatureGraphCapability::PassiveGround
    } else if has_looping_motion_candidate(motion, MotionRole::Overlay) {
        CreatureGraphCapability::HumanoidWeaponOverlay
    } else if has(MotionRole::Fly) && has_ground {
        CreatureGraphCapability::GroundFly
    } else if has(MotionRole::Swim) && has_ground {
        CreatureGraphCapability::GroundSwim
    } else if has(MotionRole::Fly) {
        CreatureGraphCapability::Fly
    } else if has(MotionRole::Swim) {
        CreatureGraphCapability::Swim
    } else if has(MotionRole::ContinuousOrRobot) {
        CreatureGraphCapability::RobotContinuousAttack
    } else if !has_mobility && has(MotionRole::Idle) && (has_melee || has_ranged) {
        CreatureGraphCapability::StationaryTurret
    } else if has(MotionRole::StationaryOrTurret)
        && (has(MotionRole::Fire) || has(MotionRole::Projectile))
    {
        CreatureGraphCapability::StationaryTurret
    } else if has_melee && has_ranged {
        CreatureGraphCapability::GroundMeleeRanged
    } else if has_ranged {
        CreatureGraphCapability::GroundRangedProjectile
    } else {
        CreatureGraphCapability::GroundMelee
    }
}

fn has_looping_motion_candidate(
    motion: Option<&CreatureFamilyMotionSet>,
    role: MotionRole,
) -> bool {
    motion
        .and_then(|motion| motion.role(role))
        .is_some_and(|entry| entry.candidates.iter().any(|candidate| candidate.looping))
}

fn required_capability_roles(
    capability: CreatureGraphCapability,
    _attack: &super::creature_catalog::AttackSet,
    motion: Option<&CreatureFamilyMotionSet>,
) -> Vec<MotionRole> {
    let has = |role| {
        motion
            .and_then(|motion| motion.role(role))
            .is_some_and(|entry| !entry.candidates.is_empty())
    };
    let mut roles = capability_roles(capability).to_vec();
    let melee = has(MotionRole::MeleeAttack);
    let ranged =
        has(MotionRole::RangedAttack) || has(MotionRole::Fire) || has(MotionRole::Projectile);
    if matches!(
        capability,
        CreatureGraphCapability::GroundRangedProjectile
            | CreatureGraphCapability::GroundMeleeRanged
    ) {
        roles.push(preferred_ranged_motion_role(motion));
    }
    if capability == CreatureGraphCapability::StationaryTurret {
        if ranged {
            roles.push(preferred_ranged_motion_role(motion));
        } else if melee {
            roles.push(MotionRole::MeleeAttack);
        }
    }
    if matches!(
        capability,
        CreatureGraphCapability::GroundSwim
            | CreatureGraphCapability::GroundFly
            | CreatureGraphCapability::Swim
            | CreatureGraphCapability::Fly
    ) {
        if melee {
            roles.push(MotionRole::MeleeAttack);
        }
        if ranged {
            roles.push(preferred_ranged_motion_role(motion));
        }
    }
    if capability == CreatureGraphCapability::HumanoidWeaponOverlay
        && has(MotionRole::GroundLocomotion)
    {
        roles.push(MotionRole::GroundLocomotion);
    }
    roles.sort();
    roles.dedup();
    roles
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

fn preferred_ranged_motion_role(motion: Option<&CreatureFamilyMotionSet>) -> MotionRole {
    [
        MotionRole::RangedAttack,
        MotionRole::Fire,
        MotionRole::Projectile,
    ]
    .into_iter()
    .find(|role| {
        motion
            .and_then(|motion| motion.role(*role))
            .is_some_and(|entry| !entry.candidates.is_empty())
    })
    .or_else(|| {
        motion
            .and_then(|motion| motion.role(MotionRole::MeleeAttack))
            .is_some_and(|entry| !entry.candidates.is_empty())
            .then_some(MotionRole::MeleeAttack)
    })
    .unwrap_or(MotionRole::RangedAttack)
}

fn build_movement_selections(
    contracts: &[CreatureGraphContractEvidence],
    motion: &[CreatureFamilyMotionSet],
) -> Vec<LegacyMovementGraphSelection> {
    let motion = motion
        .iter()
        .map(|family| (family.rig.clone(), family))
        .collect::<BTreeMap<_, _>>();
    let mut selections = contracts
        .iter()
        .map(|contract| {
            let family = motion.get(&contract.rig).copied();
            let first = |role| {
                family
                    .and_then(|family| family.role(role))
                    .and_then(|role| role.candidates.first())
                    .map(|candidate| candidate.source_kf.clone())
            };
            let locomotion_role = movement_locomotion_role(contract.capability);
            let locomotion_source_kf = locomotion_role.and_then(first);
            let stationary_cycle_source_kf =
                first(MotionRole::StationaryOrTurret).or_else(|| first(MotionRole::Idle));
            LegacyMovementGraphSelection {
                rig: contract.rig.clone(),
                graph_template: graph_template(contract.capability),
                locomotion_source_kf: locomotion_source_kf.clone(),
                turn_source_kf: first(MotionRole::Turn),
                stationary_cycle_source_kf,
                uses_controller_speed: locomotion_role == Some(MotionRole::GroundLocomotion)
                    && locomotion_source_kf.is_some(),
                aiming_capable: matches!(
                    contract.capability,
                    CreatureGraphCapability::GroundRangedProjectile
                        | CreatureGraphCapability::GroundMeleeRanged
                        | CreatureGraphCapability::StationaryTurret
                        | CreatureGraphCapability::RobotContinuousAttack
                ),
                aim_tolerance_degrees: Some(TARGET_DEFAULT_AIM_TOLERANCE_DEGREES),
                orientation_pitch_degrees: Some(TARGET_DEFAULT_PITCH_LIMIT_DEGREES),
                orientation_roll_degrees: Some(TARGET_DEFAULT_ROLL_LIMIT_DEGREES),
            }
        })
        .collect::<Vec<_>>();
    selections.sort_by(|left, right| left.rig.cmp(&right.rig));
    selections
}

fn movement_locomotion_role(capability: CreatureGraphCapability) -> Option<MotionRole> {
    match capability {
        CreatureGraphCapability::Swim => Some(MotionRole::Swim),
        CreatureGraphCapability::Fly => Some(MotionRole::Fly),
        CreatureGraphCapability::PassiveGround => Some(MotionRole::GroundLocomotion),
        CreatureGraphCapability::StationaryTurret => None,
        _ => Some(MotionRole::GroundLocomotion),
    }
}

fn graph_template(capability: CreatureGraphCapability) -> CreatureGraphTemplate {
    match capability {
        CreatureGraphCapability::PassiveGround => CreatureGraphTemplate::PassiveGround,
        CreatureGraphCapability::GroundMelee => CreatureGraphTemplate::GroundMelee,
        CreatureGraphCapability::GroundRangedProjectile => {
            CreatureGraphTemplate::GroundRangedProjectile
        }
        CreatureGraphCapability::GroundMeleeRanged => CreatureGraphTemplate::GroundMeleeRanged,
        CreatureGraphCapability::GroundSwim => CreatureGraphTemplate::GroundSwim,
        CreatureGraphCapability::GroundFly => CreatureGraphTemplate::GroundFly,
        CreatureGraphCapability::Swim => CreatureGraphTemplate::Swim,
        CreatureGraphCapability::Fly => CreatureGraphTemplate::Fly,
        CreatureGraphCapability::StationaryTurret => CreatureGraphTemplate::StationaryTurret,
        CreatureGraphCapability::RobotContinuousAttack => {
            CreatureGraphTemplate::RobotContinuousAttack
        }
        CreatureGraphCapability::HumanoidWeaponOverlay => CreatureGraphTemplate::PassiveGround,
    }
}

fn index_source_assets(
    catalog: &CreatureCorpusPlan,
    mesh_root: &Path,
    skeletons: &BTreeMap<RigFamilyKey, LiveSkeletonEvidence>,
) -> Vec<IndexedSourceAssetEvidence> {
    let mut claims =
        BTreeMap::<(LegacyCreatureGame, String), (IndexedAssetKind, BTreeSet<String>)>::new();
    for family in &catalog.rig_families {
        let skeleton_key = canonical_asset_path(&family.key.skeleton_path);
        let dependencies = family
            .asset_dependencies
            .iter()
            .map(|path| canonical_asset_path(path))
            .filter(|path| path != &skeleton_key)
            .collect::<BTreeSet<_>>();
        merge_claim(
            &mut claims,
            family.key.game,
            skeleton_key,
            IndexedAssetKind::Skeleton,
            dependencies,
        );
        for source_kf in &family.recursive_kf_paths {
            merge_claim(
                &mut claims,
                family.key.game,
                canonical_asset_path(source_kf),
                IndexedAssetKind::Kf,
                BTreeSet::new(),
            );
        }
    }
    for body in &catalog.body_variants {
        for body_path in &body.key.body_paths {
            let root = canonical_asset_path(body_path);
            let dependencies = body
                .asset_dependencies
                .iter()
                .map(|path| canonical_asset_path(path))
                .filter(|path| path != &root)
                .collect::<BTreeSet<_>>();
            merge_claim(
                &mut claims,
                body.key.rig.game,
                root,
                IndexedAssetKind::Body,
                dependencies,
            );
        }
        for dependency in &body.asset_dependencies {
            let path = canonical_asset_path(dependency);
            if body
                .key
                .body_paths
                .iter()
                .any(|body| canonical_asset_path(body) == path)
            {
                continue;
            }
            if let Some(kind) = dependency_kind(&path) {
                merge_claim(&mut claims, body.key.rig.game, path, kind, BTreeSet::new());
            }
        }
    }

    let mut assets = Vec::with_capacity(claims.len());
    for ((game, path), (kind, dependencies)) in claims {
        let game_mesh_root = game_mesh_root(mesh_root, game);
        let source = asset_path(&game_mesh_root, &path);
        let Ok(bytes) = fs::read(&source) else {
            continue;
        };
        let skeleton = catalog
            .rig_families
            .iter()
            .find(|family| {
                family.key.game == game && canonical_asset_path(&family.key.skeleton_path) == path
            })
            .and_then(|family| skeletons.get(&family.key));
        assets.push(IndexedSourceAssetEvidence {
            game,
            path,
            kind,
            content_hash_blake3: blake3::hash(&bytes).to_hex().to_string(),
            dependencies: dependencies.into_iter().collect(),
            root_nodes: skeleton
                .map(|skeleton| skeleton.root_nodes.clone())
                .unwrap_or_default(),
            skeleton_runtime_name: skeleton.map(|skeleton| skeleton.runtime_name.clone()),
            skeleton_float_slots: skeleton
                .map(|skeleton| skeleton.float_slots.clone())
                .unwrap_or_default(),
        });
    }
    assets
}

fn merge_claim(
    claims: &mut BTreeMap<(LegacyCreatureGame, String), (IndexedAssetKind, BTreeSet<String>)>,
    game: LegacyCreatureGame,
    path: String,
    kind: IndexedAssetKind,
    dependencies: BTreeSet<String>,
) {
    claims
        .entry((game, path))
        .and_modify(|(_, current)| current.extend(dependencies.iter().cloned()))
        .or_insert((kind, dependencies));
}

fn dependency_kind(path: &str) -> Option<IndexedAssetKind> {
    let extension = path.rsplit_once('.').map(|(_, extension)| extension)?;
    match extension {
        "nif" => Some(IndexedAssetKind::Body),
        "kf" => Some(IndexedAssetKind::Kf),
        "bgsm" | "bgem" => Some(IndexedAssetKind::Material),
        "dds" => Some(IndexedAssetKind::Texture),
        _ => None,
    }
}

fn extracted_mesh_root(source_data_root: &Path) -> PathBuf {
    for name in ["Meshes", "meshes"] {
        let candidate = source_data_root.join(name);
        if candidate.is_dir() {
            return candidate;
        }
    }
    source_data_root.to_path_buf()
}

fn validate_game_asset_roots(
    records: &[LegacyRecordSource<'_>],
    mesh_root: &Path,
) -> Result<(), FnvFo3LiveRecipeBuildError> {
    let games = records
        .iter()
        .filter(|source| source.record.sig.as_str() == "CREA")
        .map(|source| source.provenance.game)
        .collect::<BTreeSet<_>>();
    if games.contains(&LegacyCreatureGame::Fnv)
        && games.contains(&LegacyCreatureGame::Fo3)
        && game_mesh_root(mesh_root, LegacyCreatureGame::Fnv)
            == game_mesh_root(mesh_root, LegacyCreatureGame::Fo3)
    {
        return Err(FnvFo3LiveRecipeBuildError::Input(
            "merged FNV+FO3 assets require distinct fnv/ and fo3/ source roots".to_string(),
        ));
    }
    Ok(())
}

fn asset_path(mesh_root: &Path, relative: &str) -> PathBuf {
    canonical_asset_path(relative)
        .split('/')
        .fold(mesh_root.to_path_buf(), |path, component| {
            path.join(component)
        })
}

fn canonical_asset_path(path: &str) -> String {
    let normalized = path
        .trim()
        .replace('\\', "/")
        .trim_start_matches('/')
        .to_ascii_lowercase();
    normalized
        .strip_prefix("meshes/")
        .unwrap_or(&normalized)
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::super::creature_motion::{
        BindingCompatibility, BindingSummary, CandidateConfidence, FamilyMotionReadiness,
        MotionCandidate, MotionRoleCandidates, RoleReadiness, RootMotionEvidence, SequenceCycle,
    };
    use super::*;

    #[test]
    fn float_slot_union_is_deterministic_and_normalized() {
        let mut slots = BTreeMap::new();
        for slot in [" headtrack ", "HeadTrack", "Jaw Weight", "jaw   weight"] {
            merge_animation_slot(&mut slots, slot.to_string());
        }

        assert_eq!(
            slots.into_values().collect::<Vec<_>>(),
            vec!["HeadTrack".to_string(), "Jaw Weight".to_string()]
        );
    }

    #[test]
    fn movement_selection_uses_air_and_water_cycles_for_pure_capabilities() {
        assert_eq!(
            movement_locomotion_role(CreatureGraphCapability::Fly),
            Some(MotionRole::Fly)
        );
        assert_eq!(
            movement_locomotion_role(CreatureGraphCapability::Swim),
            Some(MotionRole::Swim)
        );
        assert_eq!(
            movement_locomotion_role(CreatureGraphCapability::GroundFly),
            Some(MotionRole::GroundLocomotion)
        );
        assert_eq!(
            movement_locomotion_role(CreatureGraphCapability::GroundSwim),
            Some(MotionRole::GroundLocomotion)
        );
        assert_eq!(
            movement_locomotion_role(CreatureGraphCapability::StationaryTurret),
            None
        );
        assert_eq!(
            movement_locomotion_role(CreatureGraphCapability::PassiveGround),
            Some(MotionRole::GroundLocomotion)
        );
    }

    #[test]
    fn non_looping_visibility_track_is_not_a_weapon_overlay_capability() {
        let candidate = MotionCandidate {
            source_game: LegacyCreatureGame::Fnv,
            source_kf: "creatures/alien/equip.kf".to_string(),
            sequence_index: 0,
            sequence_name: "Equip".to_string(),
            cycle: SequenceCycle::Clamp,
            looping: false,
            events: Vec::new(),
            idle_claims: Vec::new(),
            binding: BindingSummary {
                source_skeleton_path: "creatures/alien/skeleton.nif".to_string(),
                transform_track_count: 1,
                float_track_count: 0,
                controller_types: Vec::new(),
                interpolator_types: Vec::new(),
                overlay_targets: vec!["##VisCtrl".to_string()],
                required_float_slots: Vec::new(),
                compatibility: BindingCompatibility::Verified,
                compatibility_detail: None,
            },
            root_motion: RootMotionEvidence::Stationary { accum_root: None },
            confidence: CandidateConfidence::High,
            evidence: Vec::new(),
            ambiguous_with: Vec::new(),
        };
        let family = CreatureFamilyMotionSet {
            rig: RigFamilyKey {
                game: LegacyCreatureGame::Fnv,
                skeleton_path: "creatures/alien/skeleton.nif".to_string(),
            },
            roles: vec![MotionRoleCandidates {
                role: MotionRole::Overlay,
                candidates: vec![candidate.clone()],
                readiness: RoleReadiness::Ready,
            }],
            readiness: FamilyMotionReadiness::Ready,
            kf_accounting: Vec::new(),
        };

        assert!(!has_looping_motion_candidate(
            Some(&family),
            MotionRole::Overlay
        ));

        let mut looping = family;
        looping.roles[0].candidates[0] = MotionCandidate {
            cycle: SequenceCycle::Loop,
            looping: true,
            ..candidate
        };
        assert!(has_looping_motion_candidate(
            Some(&looping),
            MotionRole::Overlay
        ));
    }
}
