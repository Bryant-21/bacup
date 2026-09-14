use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use havok_native::convert::creature_ragdoll::{
    extract_skyrim_2010_creature_ragdoll, lower_primitive_shapes_to_fo4_convex_hulls,
    reconstruct_fo4_creature_ragdoll_packfile,
};
use havok_native::hkx::HkxFile;
use havok_native::hkx::model::{HkxMember, HkxObject};
use havok_native::hkx::types::HkxValue;
use nif_core_native::creature_closure::{
    CreatureClosureReceipt, CreatureClosureRequest, CreatureNifInput, CreatureNifRole,
    refresh_staged_creature_nif_artifact, seal_embedded_creature_collision,
    stage_creature_nif_closure,
};
use nif_core_native::creature_ragdoll::{
    install_fo4_creature_controller_collision, install_fo4_creature_ragdoll_collision,
};
use nif_core_native::model::NifFile;
use thiserror::Error;

use crate::formkey_mapper::MapperState;
use crate::ids::FormKey;
use crate::record::Record;
use crate::run::{
    CreatureActorActionReservationReceipt, CreatureActorActionReservationRequest,
    SkyrimCreatureAncillaryRecordKind, SkyrimCreatureAncillaryReservationReceipt,
    SkyrimCreatureAncillaryReservationRequest, SkyrimCreatureRecordReservation,
    SkyrimCreatureReservedRecordKind,
};
use crate::source_rig::race_data::{ControllerArchitecture, MeasurementAxis};
use crate::source_rig::{
    ATTACK_STAGGER_OFFSET_ZERO_PROOF_SCHEMA, ATTACK_STAMINA_TO_ACTION_POINTS_PROOF_SCHEMA,
    ATTACK_TYPE_RUNTIME_INERT_PROOF_SCHEMA, BoneDecl, Capsule, ClipBinding, ClipDecl,
    CreatureActorActionRecordPlan, CreatureAttackRecordProjection, CreatureAttackRecordVariant,
    CreatureAttackSourceDataReceipt, CreatureAttackSpellPolicy, CreatureAttackStaggerOffsetPolicy,
    CreatureAttackStaminaPolicy, CreatureAttackTargetData, CreatureAttackTypePolicy,
    CreatureBodyNifRecordPart, CreatureBodyPartProjection, CreatureControllerDecl,
    CreatureDegradationReceipt, CreatureFallbackDisposition, CreatureGraphTemplate,
    CreatureManifest, CreatureMeleeRecordKeyPlan, CreatureNpcInventoryEntry,
    CreatureNpcInventoryOwnership, CreatureNpcRecordKeyPlan, CreatureNpcRecordVariant,
    CreaturePrimaryRecordMapping, CreatureRecordEditorIds, CreatureRecordFormKeys,
    CreatureRecordKeyPlan, CreatureRecordManifest, CreatureRecordProjectionManifest,
    CreatureSemanticProofReceipt, CreatureSourceRecordReference, CreatureTargetRecordReference,
    GraphDeclarations, NoRagdollReason, RAGDOLL_ENTER_EVENTS, RAGDOLL_TRANSITION_EVENTS,
    RaceDataMapping, RagdollDisposition, ScaffoldPaths, SkeletonDecl, SourceCreatureIdentity,
    SourceOwnedRagdollReceipt, SourceRigArtifactProvenance, SourceRigArtifactReceipt,
    SourceRigArtifactRole, SourceRigConvertedArtifactRequestReceipt, SourceRigExecutableRecipe,
    SourceRigFieldDecision, SourceRigRecordBatchIntent, TargetFormKey, projection_dependencies,
    required_actor_action_records,
};
use crate::sym::StringInterner;

use super::creature_catalog::{CreatureCorpusPlan, CreatureRacePlan, build_creature_corpus_plan};
use super::creature_motion::{
    SkyrimBoundMotionSamples, SkyrimControllerArchitectureEvidence,
    SkyrimControllerLayoutEvidence as MotionControllerLayout, SkyrimCreatureFamilyInventory,
    SkyrimCreatureMotionCatalog, SkyrimTimedMotionSample, load_skyrim_bound_motion_samples,
};
use super::creature_mvp_adapter::{
    SkyrimCreatureConvertedArtifact, SkyrimCreatureDependencyCandidate,
    SkyrimCreatureDependencyCandidateDisposition, SkyrimCreatureDependencyLedger,
    SkyrimCreatureSourceFieldDecision, SkyrimCreatureSourceFieldReceipt,
    SkyrimCreatureTargetDependencyProjection, SkyrimRecordLoweringDisposition,
    SkyrimRecordLoweringKind, embedded_form_ids, resolve_embedded_form_id,
    skyrim_creature_target_signature,
};
use super::creature_race_data::DecodedCreatureFamilyEvidence;
use super::creature_recipe::{
    SkyrimAssetClaimDisposition, SkyrimAssetRole, SkyrimControllerAxesEvidence,
    SkyrimControllerCapsuleReceipt, SkyrimControllerLayoutEvidence,
    SkyrimCreatureBodyVariantEvidence, SkyrimCreatureCandidateDisposition,
    SkyrimCreatureClipReceipt, SkyrimCreatureControllerReceipt,
    SkyrimCreatureControllerRecipeEvidence, SkyrimCreatureFamilyAssetEvidence,
    SkyrimCreatureFamilyDisposition, SkyrimCreatureFamilyJob, SkyrimCreatureGraphVariantEvidence,
    SkyrimCreatureRecipeError, SkyrimCreatureRecipeExpectations, SkyrimCreatureRecipeInput,
    SkyrimCreatureRecipeLedger, SkyrimCreatureSex, SkyrimExplicitCapabilityEvidence,
    SkyrimRaceAttackDataReceipt, SkyrimRaceDataPolicyReceipt, SkyrimRagdollCapabilityEvidence,
    SkyrimRecursiveAssetClaim, SkyrimRootMotionLocatorReceipt, SkyrimRootMotionReceipt,
    SkyrimRootMotionSourceReceipt, build_skyrim_creature_recipe_ledger,
};
use super::source_rig_bridge::{
    SkyrimSourceRigBridgeInput, SkyrimSourceRigFamilySelection, SkyrimSourceRigFieldDecision,
    build_skyrim_source_rig_executable_recipe, graph_manifest,
    required_skyrim_source_rig_field_names,
};

const FO4_CREATURE_RANGED_FALLBACK_WEAPON: u32 = 0x03175B;

#[derive(Clone, Copy)]
pub struct SkyrimCreatureLiveRecipeBuildInput<'a> {
    pub winning_records: &'a [Record],
    pub motion: &'a SkyrimCreatureMotionCatalog,
    pub decoded_race_data: &'a [DecodedCreatureFamilyEvidence],
    pub race_data_policy: &'a SkyrimRaceDataPolicyReceipt,
    pub dependency: &'a SkyrimCreatureDependencyLedger,
    pub source_data_root: &'a Path,
}

#[derive(Clone, Debug)]
pub struct SkyrimCreatureLiveRecipeEvidence {
    pub catalog: CreatureCorpusPlan,
    pub assets: Vec<SkyrimCreatureFamilyAssetEvidence>,
    pub recipe_ledger: SkyrimCreatureRecipeLedger,
}

#[derive(Debug, Error)]
pub enum SkyrimCreatureLiveRecipeBuildError {
    #[error(transparent)]
    Recipe(#[from] SkyrimCreatureRecipeError),
    #[error("Skyrim live creature input is inconsistent: {0}")]
    Input(String),
}

#[derive(Clone, Copy)]
pub(crate) struct SkyrimCreatureLiveMvpPreparationBuildInput<'a> {
    pub winning_records: &'a [Record],
    pub motion: &'a SkyrimCreatureMotionCatalog,
    pub decoded_race_data: &'a [DecodedCreatureFamilyEvidence],
    pub race_data_policy: &'a SkyrimRaceDataPolicyReceipt,
    pub dependency: &'a SkyrimCreatureDependencyLedger,
    pub source_data_root: &'a Path,
    pub private_staging_root: &'a Path,
    pub reservations: &'a [SkyrimCreatureRecordReservation],
    pub ancillary_reservations: &'a [SkyrimCreatureAncillaryReservationReceipt],
    pub actor_action_reservations: &'a [CreatureActorActionReservationReceipt],
    pub mapper_state: &'a MapperState,
    pub editor_id_prefix: &'a str,
}

pub(crate) fn enumerate_skyrim_creature_ancillary_reservation_requests(
    catalog: &CreatureCorpusPlan,
    motion: &SkyrimCreatureMotionCatalog,
    interner: &StringInterner,
) -> Result<
    Vec<SkyrimCreatureAncillaryReservationRequest>,
    SkyrimCreatureLiveMvpPreparationBuildError,
> {
    let mut requests = Vec::new();
    let mut variant_sources = catalog
        .npcs
        .iter()
        .map(|npc| {
            (
                npc.source_npc,
                SkyrimCreatureReservedRecordKind::Npc,
                SkyrimCreatureAncillaryRecordKind::NpcVariant,
            )
        })
        .chain(catalog.races.iter().flat_map(|race| {
            race.skin
                .into_iter()
                .map(|source| {
                    (
                        source,
                        SkyrimCreatureReservedRecordKind::Armor,
                        SkyrimCreatureAncillaryRecordKind::ArmorVariant,
                    )
                })
                .chain(race.armor_addons.iter().copied().map(|source| {
                    (
                        source,
                        SkyrimCreatureReservedRecordKind::ArmorAddon,
                        SkyrimCreatureAncillaryRecordKind::ArmorAddonVariant,
                    )
                }))
                .chain(race.body_part_data.into_iter().map(|source| {
                    (
                        source,
                        SkyrimCreatureReservedRecordKind::BodyPartData,
                        SkyrimCreatureAncillaryRecordKind::BodyPartDataVariant,
                    )
                }))
        }))
        .collect::<Vec<_>>();
    variant_sources.sort_by_key(|(source, kind, _)| (form_key_sort_key(*source, interner), *kind));
    variant_sources.dedup();
    for (source, source_kind, variant_kind) in variant_sources {
        let owner_races = projected_creature_record_owner_races(catalog, source, source_kind);
        if owner_races.len() < 2 {
            continue;
        }
        let event = record_variant_reservation_event(source, interner)
            .map_err(SkyrimCreatureLiveMvpPreparationBuildError::Input)?;
        for source_race in owner_races.into_iter().skip(1) {
            requests.push(SkyrimCreatureAncillaryReservationRequest {
                source_race,
                event: event.clone(),
                ordinal: source.local,
                kind: variant_kind,
            });
        }
    }
    for race in &catalog.races {
        if catalog.npcs_for_race(race.source_race).next().is_none() {
            requests.push(SkyrimCreatureAncillaryReservationRequest {
                source_race: race.source_race,
                event: "synthetic_npc".to_string(),
                ordinal: 0,
                kind: SkyrimCreatureAncillaryRecordKind::SyntheticNpc,
            });
        }
        for attack in &race.attack_data {
            requests.push(SkyrimCreatureAncillaryReservationRequest {
                source_race: race.source_race,
                event: attack.event.clone(),
                ordinal: u32::try_from(attack.ordinal).map_err(|_| {
                    SkyrimCreatureLiveMvpPreparationBuildError::Input(format!(
                        "RACE attack ordinal {} exceeds u32",
                        attack.ordinal
                    ))
                })?,
                kind: SkyrimCreatureAncillaryRecordKind::Weapon,
            });
        }
        if race.attack_data.is_empty() {
            let family = catalog
                .families
                .iter()
                .find(|family| family.races.contains(&race.source_race))
                .ok_or_else(|| {
                    SkyrimCreatureLiveMvpPreparationBuildError::Input(format!(
                        "RACE {:06X} has no creature family",
                        race.source_race.local
                    ))
                })?;
            if motion.family(&family.family_id).is_none() {
                continue;
            }
            let graph = motion
                .capability_graph_manifest(&family.family_id)
                .map_err(|error| {
                    SkyrimCreatureLiveMvpPreparationBuildError::Input(format!(
                        "derive graph for {}: {error}",
                        family.family_id
                    ))
                })?;
            if graph.template != CreatureGraphTemplate::PassiveGround {
                if synthetic_projectile_attack_event(
                    &race.attack_events,
                    &race.attack_contract,
                    &graph.roles,
                    &graph.explicit_events,
                )
                .is_some()
                {
                    continue;
                }
                let event = synthetic_melee_attack_event(
                    &race.source_plugin,
                    race.source_race.local,
                    &race.attack_events,
                    &race.attack_contract,
                    &graph.roles,
                    &graph.explicit_events,
                    &graph.candidate_attack_bindings,
                )
                .ok_or_else(|| {
                    SkyrimCreatureLiveMvpPreparationBuildError::Input(format!(
                        "RACE {:06X} has no source-backed melee event for its active graph",
                        race.source_race.local
                    ))
                })?;
                requests.push(SkyrimCreatureAncillaryReservationRequest {
                    source_race: race.source_race,
                    event,
                    ordinal: 0,
                    kind: SkyrimCreatureAncillaryRecordKind::Weapon,
                });
            }
        }
    }
    let mut keys = Vec::new();
    for request in &requests {
        let key = (
            request.source_race,
            request.event.to_ascii_lowercase(),
            request.ordinal,
            request.kind,
        );
        if keys.contains(&key) {
            return Err(SkyrimCreatureLiveMvpPreparationBuildError::Input(format!(
                "duplicate ancillary request for RACE {:06X} event {:?} ordinal {}",
                request.source_race.local, request.event, request.ordinal
            )));
        }
        keys.push(key);
    }
    Ok(requests)
}

fn synthetic_melee_attack_event(
    _source_plugin: &str,
    _source_local: u32,
    attack_events: &[String],
    attack_contract: &[String],
    roles: &[crate::source_rig::CapabilityClipRole],
    explicit_events: &[crate::source_rig::EventDecl],
    _candidate_bindings: &[crate::source_rig::CapabilityCandidateAttackBinding],
) -> Option<String> {
    let eligible = roles
        .iter()
        .filter(|role| role.role == crate::source_rig::CreatureClipRole::MeleeAttack)
        .flat_map(|role| role.trigger_event.iter().chain(&role.trigger_aliases))
        .filter(|event| {
            explicit_events.iter().any(|declared| {
                declared.name.eq_ignore_ascii_case(event)
                    && matches!(
                        declared.usage,
                        crate::source_rig::EventUsage::MeleeAttack
                            | crate::source_rig::EventUsage::Generic
                    )
            })
        })
        .cloned()
        .collect::<Vec<_>>();
    attack_events
        .iter()
        .chain(attack_contract)
        .find_map(|source_event| {
            eligible
                .iter()
                .find(|event| event.eq_ignore_ascii_case(source_event))
                .cloned()
        })
        .or_else(|| eligible.into_iter().next())
}

fn synthetic_projectile_attack_event(
    attack_events: &[String],
    attack_contract: &[String],
    roles: &[crate::source_rig::CapabilityClipRole],
    explicit_events: &[crate::source_rig::EventDecl],
) -> Option<String> {
    let eligible = roles
        .iter()
        .filter(|role| role.role == crate::source_rig::CreatureClipRole::ProjectileAttack)
        .flat_map(|role| role.trigger_event.iter().chain(&role.trigger_aliases))
        .filter(|event| {
            explicit_events.iter().any(|declared| {
                declared.name.eq_ignore_ascii_case(event)
                    && declared.usage == crate::source_rig::EventUsage::Generic
            })
        })
        .cloned()
        .collect::<Vec<_>>();
    attack_events
        .iter()
        .chain(attack_contract)
        .find_map(|source_event| {
            eligible
                .iter()
                .find(|event| event.eq_ignore_ascii_case(source_event))
                .cloned()
        })
        .or_else(|| eligible.into_iter().next())
}

pub(crate) fn enumerate_skyrim_creature_actor_action_reservation_requests(
    input: SkyrimCreatureLiveRecipeBuildInput<'_>,
    editor_id_prefix: &str,
    interner: &StringInterner,
) -> Result<Vec<CreatureActorActionReservationRequest>, SkyrimCreatureLiveMvpPreparationBuildError>
{
    let evidence = build_live_skyrim_creature_recipe_evidence(input, interner)?;
    actor_action_reservation_requests(&evidence.recipe_ledger, editor_id_prefix)
}

fn actor_action_reservation_requests(
    ledger: &SkyrimCreatureRecipeLedger,
    editor_id_prefix: &str,
) -> Result<Vec<CreatureActorActionReservationRequest>, SkyrimCreatureLiveMvpPreparationBuildError>
{
    if editor_id_prefix.trim().is_empty()
        || !editor_id_prefix
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '_')
    {
        return Err(SkyrimCreatureLiveMvpPreparationBuildError::Input(
            "editor_id_prefix is not a valid Actor Action EditorID prefix".to_string(),
        ));
    }
    let mut requests = Vec::new();
    for family in &ledger.family_jobs {
        let graph = graph_manifest(family).map_err(|error| {
            SkyrimCreatureLiveMvpPreparationBuildError::Input(format!(
                "derive Actor Action graph for {}: {error}",
                family.family_id
            ))
        })?;
        let root_behavior = skyrim_family_root_behavior_path(&family.family_id);
        for requirement in required_actor_action_records(&graph, &root_behavior) {
            let identity = format!(
                "{}:{}:{:?}:{}:{}",
                family.family_id,
                requirement.parent_form_id,
                requirement.kind,
                requirement.behavior_path,
                requirement.animation_event,
            );
            let editor_id = format!(
                "{editor_id_prefix}SkyrimActorAction_{}",
                &blake3::hash(identity.as_bytes()).to_hex()[..16]
            );
            requests.push(CreatureActorActionReservationRequest {
                family_id: family.family_id.clone(),
                requirement,
                editor_id,
            });
        }
    }
    requests.sort_by(|left, right| {
        left.family_id
            .to_ascii_lowercase()
            .cmp(&right.family_id.to_ascii_lowercase())
            .then_with(|| left.requirement.cmp(&right.requirement))
    });
    if requests.windows(2).any(|pair| {
        pair[0].family_id.eq_ignore_ascii_case(&pair[1].family_id)
            && pair[0].requirement == pair[1].requirement
    }) {
        return Err(SkyrimCreatureLiveMvpPreparationBuildError::Input(
            "a Skyrim family has duplicate Actor Action requirements".to_string(),
        ));
    }
    Ok(requests)
}

fn skyrim_family_root_behavior_path(family_id: &str) -> String {
    format!(
        "{}\\Behaviors\\SkyrimRootBehavior.hkx",
        skyrim_family_runtime_root(family_id)
    )
}

fn skyrim_family_runtime_root(family_id: &str) -> String {
    let hash = blake3::hash(family_id.as_bytes()).to_hex().to_string();
    format!("Actors\\B21_SkyrimCreature_{}", &hash[..16])
}

fn validate_ancillary_reservations(
    requests: &[SkyrimCreatureAncillaryReservationRequest],
    receipts: &[SkyrimCreatureAncillaryReservationReceipt],
    interner: &StringInterner,
) -> Result<(), SkyrimCreatureLiveMvpPreparationBuildError> {
    if requests.len() != receipts.len() {
        return Err(SkyrimCreatureLiveMvpPreparationBuildError::Input(format!(
            "expected {} ancillary reservations, found {}",
            requests.len(),
            receipts.len()
        )));
    }
    let mut targets = Vec::new();
    for request in requests {
        let matches = receipts
            .iter()
            .filter(|receipt| {
                receipt.source_race == request.source_race
                    && receipt.event == request.event
                    && receipt.ordinal == request.ordinal
                    && receipt.kind == request.kind
            })
            .collect::<Vec<_>>();
        let [receipt] = matches.as_slice() else {
            return Err(SkyrimCreatureLiveMvpPreparationBuildError::Input(format!(
                "ancillary request {}:{:?}:{}:{:?} has {} receipts",
                request.source_race.format(interner),
                request.event,
                request.ordinal,
                request.kind,
                matches.len()
            )));
        };
        if receipt.target.local == 0 {
            return Err(SkyrimCreatureLiveMvpPreparationBuildError::Input(format!(
                "ancillary receipt {}:{:?}:{}:{:?} has a null target",
                request.source_race.format(interner),
                request.event,
                request.ordinal,
                request.kind
            )));
        }
        if targets.contains(&receipt.target) {
            return Err(SkyrimCreatureLiveMvpPreparationBuildError::Input(format!(
                "ancillary target {} is assigned more than once",
                receipt.target.format(interner)
            )));
        }
        targets.push(receipt.target);
    }
    Ok(())
}

fn actor_action_record_plans(
    requests: &[CreatureActorActionReservationRequest],
    receipts: &[CreatureActorActionReservationReceipt],
    interner: &StringInterner,
) -> Result<
    BTreeMap<String, Vec<CreatureActorActionRecordPlan>>,
    SkyrimCreatureLiveMvpPreparationBuildError,
> {
    if requests.len() != receipts.len() {
        return Err(SkyrimCreatureLiveMvpPreparationBuildError::Input(format!(
            "expected {} Actor Action reservations, found {}",
            requests.len(),
            receipts.len()
        )));
    }
    let mut plans = BTreeMap::<String, Vec<CreatureActorActionRecordPlan>>::new();
    let mut targets = HashSet::new();
    for request in requests {
        let matches = receipts
            .iter()
            .filter(|receipt| {
                receipt.family_id == request.family_id
                    && receipt.requirement == request.requirement
                    && receipt.editor_id == request.editor_id
            })
            .collect::<Vec<_>>();
        let [receipt] = matches.as_slice() else {
            return Err(SkyrimCreatureLiveMvpPreparationBuildError::Input(format!(
                "Actor Action request {}:{} has {} receipts",
                request.family_id,
                request.requirement.animation_event,
                matches.len()
            )));
        };
        if receipt.target.local == 0 || !targets.insert(receipt.target) {
            return Err(SkyrimCreatureLiveMvpPreparationBuildError::Input(format!(
                "Actor Action receipt {}:{} has a null or duplicated target",
                request.family_id, request.requirement.animation_event
            )));
        }
        plans.entry(receipt.family_id.clone()).or_default().push(
            receipt.record_plan(interner).map_err(|error| {
                SkyrimCreatureLiveMvpPreparationBuildError::Input(format!(
                    "Actor Action receipt {}:{} is invalid: {error}",
                    request.family_id, request.requirement.animation_event
                ))
            })?,
        );
    }
    for family_plans in plans.values_mut() {
        family_plans.sort_by(|left, right| left.requirement.cmp(&right.requirement));
    }
    Ok(plans)
}

fn ancillary_target(
    receipts: &[SkyrimCreatureAncillaryReservationReceipt],
    source_race: FormKey,
    event: &str,
    ordinal: usize,
    kind: SkyrimCreatureAncillaryRecordKind,
    target_plugin: &str,
    interner: &StringInterner,
) -> Result<TargetFormKey, String> {
    let ordinal = u32::try_from(ordinal)
        .map_err(|_| format!("ATKD ordinal {ordinal} exceeds ancillary receipt width"))?;
    let matches = receipts
        .iter()
        .filter(|receipt| {
            receipt.source_race == source_race
                && receipt.event == event
                && receipt.ordinal == ordinal
                && receipt.kind == kind
        })
        .collect::<Vec<_>>();
    let [receipt] = matches.as_slice() else {
        return Err(format!(
            "ancillary {}:{event:?}:{ordinal}:{kind:?} has {} receipts",
            source_race.format(interner),
            matches.len()
        ));
    };
    let plugin = interner
        .resolve(receipt.target.plugin)
        .filter(|plugin| plugin.eq_ignore_ascii_case(target_plugin))
        .ok_or_else(|| {
            format!(
                "ancillary target {} is outside target plugin {target_plugin:?}",
                receipt.target.format(interner)
            )
        })?;
    if receipt.target.local == 0 || receipt.target.local > 0x00ff_ffff {
        return Err(format!(
            "ancillary target {} has an invalid local ID",
            receipt.target.format(interner)
        ));
    }
    Ok(TargetFormKey::new(receipt.target.local, plugin))
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct SkyrimCreatureLivePreparationBlocker {
    pub code: String,
    pub detail: String,
}

#[derive(Clone, Debug)]
pub enum SkyrimCreatureLivePreparedFamilyDisposition {
    Ready {
        asset_recipe: SourceRigExecutableRecipe,
        candidate_recipes: Vec<SourceRigExecutableRecipe>,
        closure: CreatureClosureReceipt,
        closure_staged_data_root: PathBuf,
        converted_artifacts: Vec<SkyrimCreatureConvertedArtifact>,
        dependency_projections: Vec<SkyrimCreatureTargetDependencyProjection>,
        source_field_receipts: Vec<SkyrimCreatureSourceFieldReceipt>,
        warnings: Vec<CreatureDegradationReceipt>,
    },
    Blocked {
        blockers: Vec<SkyrimCreatureLivePreparationBlocker>,
    },
}

#[derive(Clone, Debug)]
pub struct SkyrimCreatureLivePreparedFamily {
    pub family_id: String,
    pub project_path: String,
    pub member_source_keys: Vec<String>,
    pub disposition: SkyrimCreatureLivePreparedFamilyDisposition,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SkyrimCreatureLivePreparationAccounting {
    pub candidates: usize,
    pub families: usize,
    pub ready_candidates: usize,
    pub blocked_candidates: usize,
    pub ready_families: usize,
    pub blocked_families: usize,
}

#[derive(Clone, Debug)]
pub struct SkyrimCreatureLivePreparationLedger {
    pub families: Vec<SkyrimCreatureLivePreparedFamily>,
    pub accounting: SkyrimCreatureLivePreparationAccounting,
}

#[derive(Debug, Error)]
pub enum SkyrimCreatureLiveMvpPreparationBuildError {
    #[error(transparent)]
    Recipe(#[from] SkyrimCreatureLiveRecipeBuildError),
    #[error("invalid live Skyrim creature MVP preparation input: {0}")]
    Input(String),
}

#[derive(Clone)]
struct PreparedSkyrimLiveFamilyAssets {
    closure: CreatureClosureReceipt,
    closure_staged_data_root: PathBuf,
    rig: CreatureManifest,
    artifact_requests: Vec<SourceRigConvertedArtifactRequestReceipt>,
    artifact_receipts: Vec<SourceRigArtifactReceipt>,
    converted_artifacts: Vec<SkyrimCreatureConvertedArtifact>,
    body_runtime_paths: BTreeMap<String, String>,
    selection_by_race: BTreeMap<String, SkyrimSourceRigFamilySelection>,
}

pub fn build_live_skyrim_creature_recipe_evidence(
    input: SkyrimCreatureLiveRecipeBuildInput<'_>,
    interner: &StringInterner,
) -> Result<SkyrimCreatureLiveRecipeEvidence, SkyrimCreatureLiveRecipeBuildError> {
    let catalog = build_creature_corpus_plan(input.winning_records, interner);
    if catalog.races.len() != super::creature_recipe::SKYRIM_CREATURE_CANDIDATE_COUNT {
        return Err(SkyrimCreatureLiveRecipeBuildError::Input(format!(
            "expected {} candidates, found {}",
            super::creature_recipe::SKYRIM_CREATURE_CANDIDATE_COUNT,
            catalog.races.len()
        )));
    }
    if input.motion.families.len() != super::creature_recipe::SKYRIM_CREATURE_FAMILY_COUNT {
        return Err(SkyrimCreatureLiveRecipeBuildError::Input(format!(
            "expected {} motion families, found {}",
            super::creature_recipe::SKYRIM_CREATURE_FAMILY_COUNT,
            input.motion.families.len()
        )));
    }
    let assets = build_live_family_assets(
        &catalog,
        input.motion,
        input.dependency,
        input.source_data_root,
        interner,
    );
    let recipe_ledger = build_skyrim_creature_recipe_ledger(SkyrimCreatureRecipeInput {
        winning_records: input.winning_records,
        motion: input.motion,
        decoded_race_data: input.decoded_race_data,
        assets: &assets,
        race_data_policy: input.race_data_policy,
        expectations: SkyrimCreatureRecipeExpectations::OFFICIAL,
        interner,
    })?;
    Ok(SkyrimCreatureLiveRecipeEvidence {
        catalog,
        assets,
        recipe_ledger,
    })
}

pub(crate) fn build_live_skyrim_creature_mvp_families(
    input: SkyrimCreatureLiveMvpPreparationBuildInput<'_>,
    interner: &StringInterner,
) -> Result<SkyrimCreatureLivePreparationLedger, SkyrimCreatureLiveMvpPreparationBuildError> {
    if input.editor_id_prefix.trim().is_empty() {
        return Err(SkyrimCreatureLiveMvpPreparationBuildError::Input(
            "editor_id_prefix is empty".to_string(),
        ));
    }
    if input.private_staging_root == input.source_data_root {
        return Err(SkyrimCreatureLiveMvpPreparationBuildError::Input(
            "private staging root aliases the immutable source data root".to_string(),
        ));
    }
    let evidence = build_live_skyrim_creature_recipe_evidence(
        SkyrimCreatureLiveRecipeBuildInput {
            winning_records: input.winning_records,
            motion: input.motion,
            decoded_race_data: input.decoded_race_data,
            race_data_policy: input.race_data_policy,
            dependency: input.dependency,
            source_data_root: input.source_data_root,
        },
        interner,
    )?;
    let ancillary_requests = enumerate_skyrim_creature_ancillary_reservation_requests(
        &evidence.catalog,
        input.motion,
        interner,
    )?;
    validate_ancillary_reservations(&ancillary_requests, input.ancillary_reservations, interner)?;
    let actor_action_requests =
        actor_action_reservation_requests(&evidence.recipe_ledger, input.editor_id_prefix)?;
    let mut actor_action_plans = actor_action_record_plans(
        &actor_action_requests,
        input.actor_action_reservations,
        interner,
    )?;
    let records = input
        .winning_records
        .iter()
        .map(|record| (record.form_key, record))
        .collect::<HashMap<_, _>>();
    let reservation_by_source = input
        .reservations
        .iter()
        .map(|reservation| (reservation.source, reservation))
        .collect::<HashMap<_, _>>();
    if reservation_by_source.len() != input.reservations.len() {
        return Err(SkyrimCreatureLiveMvpPreparationBuildError::Input(
            "reservation ledger repeats a source FormKey".to_string(),
        ));
    }
    let mut family_members = BTreeMap::<String, Vec<FormKey>>::new();
    for reservation in input
        .reservations
        .iter()
        .filter(|reservation| reservation.kind == SkyrimCreatureReservedRecordKind::Race)
    {
        let [owner] = reservation.owners.as_slice() else {
            return Err(SkyrimCreatureLiveMvpPreparationBuildError::Input(format!(
                "RACE reservation {} has {} owners",
                reservation.source.format(interner),
                reservation.owners.len()
            )));
        };
        if owner.source_race != reservation.source {
            return Err(SkyrimCreatureLiveMvpPreparationBuildError::Input(format!(
                "RACE reservation {} is not self-owned",
                reservation.source.format(interner)
            )));
        }
        family_members
            .entry(owner.motion_family_id.clone())
            .or_default()
            .push(reservation.source);
    }
    let candidate_total = family_members.values().map(Vec::len).sum::<usize>();
    if candidate_total != super::creature_recipe::SKYRIM_CREATURE_CANDIDATE_COUNT {
        return Err(SkyrimCreatureLiveMvpPreparationBuildError::Input(format!(
            "expected {} reserved RACE candidates, found {candidate_total}",
            super::creature_recipe::SKYRIM_CREATURE_CANDIDATE_COUNT
        )));
    }

    let dependency_candidates = input
        .dependency
        .candidates
        .iter()
        .map(|candidate| (candidate.source_race, candidate))
        .collect::<HashMap<_, _>>();
    let recipe_candidates = evidence
        .recipe_ledger
        .candidates
        .iter()
        .filter_map(|candidate| {
            parse_form_key(&candidate.source_race, interner).map(|key| (key, candidate))
        })
        .collect::<HashMap<_, _>>();
    let target_plugin = input.mapper_state.options.output_plugin_name.clone();
    let mut families = Vec::with_capacity(evidence.recipe_ledger.family_jobs.len());
    let mut ready_candidates = 0usize;
    for family in &evidence.recipe_ledger.family_jobs {
        let mut members = family_members.remove(&family.family_id).unwrap_or_default();
        let family_actor_action_records = actor_action_plans
            .remove(&family.family_id)
            .unwrap_or_default();
        members.sort_by_key(|key| form_key_sort_key(*key, interner));
        let member_source_keys = members
            .iter()
            .map(|source| source_key(*source, interner))
            .collect::<Vec<_>>();
        let mut blockers = Vec::new();
        if members.is_empty() {
            blockers.push(live_blocker(
                "missing_family_members",
                "no reserved RACE candidate owns this exact motion family",
            ));
        }
        if family_actor_action_records.is_empty() {
            blockers.push(live_blocker(
                "missing_actor_action_records",
                "family has no reserved Actor Action records",
            ));
        }
        if !matches!(
            family.disposition,
            SkyrimCreatureFamilyDisposition::Ready { .. }
        ) {
            blockers.push(live_blocker(
                "family_recipe_not_ready",
                format!("{:?}", family.disposition),
            ));
        }
        for source_race in &members {
            let Some(dependency_candidate) = dependency_candidates.get(source_race) else {
                blockers.push(live_blocker(
                    "missing_dependency_candidate",
                    source_race.format(interner),
                ));
                continue;
            };
            if !matches!(
                dependency_candidate.disposition,
                SkyrimCreatureDependencyCandidateDisposition::Ready
            ) {
                blockers.push(live_blocker(
                    "dependency_candidate_not_ready",
                    format!(
                        "{}: {:?}",
                        source_race.format(interner),
                        dependency_candidate.disposition
                    ),
                ));
            }
            let Some(recipe_candidate) = recipe_candidates.get(source_race) else {
                blockers.push(live_blocker(
                    "missing_recipe_candidate",
                    source_race.format(interner),
                ));
                continue;
            };
            match &recipe_candidate.disposition {
                SkyrimCreatureCandidateDisposition::Ready { family_job_ids }
                    if family_job_ids.len() == 1 && family_job_ids[0] == family.family_id => {}
                disposition => blockers.push(live_blocker(
                    "candidate_recipe_not_ready",
                    format!("{}: {disposition:?}", source_race.format(interner)),
                )),
            }
        }
        canonical_live_blockers(&mut blockers);

        let disposition = if blockers.is_empty() {
            match prepare_live_skyrim_family_assets(
                family,
                &evidence,
                input.source_data_root,
                input.private_staging_root,
            ) {
                Ok(assets) => {
                    let mut recipes = Vec::with_capacity(members.len());
                    let mut dependency_projections = Vec::new();
                    let mut source_field_receipts = Vec::new();
                    let mut warnings = Vec::new();
                    for (candidate_index, source_race) in members.iter().enumerate() {
                        match prepare_live_skyrim_candidate_recipe(
                            *source_race,
                            family,
                            &assets,
                            &evidence,
                            input.dependency,
                            &records,
                            &reservation_by_source,
                            input.ancillary_reservations,
                            if candidate_index == 0 {
                                &family_actor_action_records
                            } else {
                                &[]
                            },
                            input.mapper_state,
                            &target_plugin,
                            input.editor_id_prefix,
                            interner,
                        ) {
                            Ok(prepared) => {
                                recipes.push(prepared.recipe);
                                dependency_projections.extend(prepared.dependency_projections);
                                source_field_receipts.extend(prepared.source_field_receipts);
                                warnings.extend(prepared.warnings);
                            }
                            Err(detail) => blockers.push(live_blocker(
                                "candidate_projection",
                                format!("{}: {detail}", source_race.format(interner)),
                            )),
                        }
                    }
                    canonical_live_blockers(&mut blockers);
                    if blockers.is_empty() {
                        ready_candidates += recipes.len();
                        let asset_recipe = recipes[0].clone();
                        canonical_degradation_receipts(&mut warnings)
                            .map_err(SkyrimCreatureLiveMvpPreparationBuildError::Input)?;
                        SkyrimCreatureLivePreparedFamilyDisposition::Ready {
                            asset_recipe,
                            candidate_recipes: recipes,
                            closure: assets.closure,
                            closure_staged_data_root: assets.closure_staged_data_root,
                            converted_artifacts: assets.converted_artifacts,
                            dependency_projections: canonical_dependency_projections(
                                dependency_projections,
                                interner,
                            ),
                            source_field_receipts: canonical_source_field_receipts(
                                source_field_receipts,
                                interner,
                            ),
                            warnings,
                        }
                    } else {
                        SkyrimCreatureLivePreparedFamilyDisposition::Blocked { blockers }
                    }
                }
                Err(detail) => SkyrimCreatureLivePreparedFamilyDisposition::Blocked {
                    blockers: vec![live_blocker("asset_preparation", detail)],
                },
            }
        } else {
            SkyrimCreatureLivePreparedFamilyDisposition::Blocked { blockers }
        };
        families.push(SkyrimCreatureLivePreparedFamily {
            family_id: family.family_id.clone(),
            project_path: family.project_path.clone(),
            member_source_keys,
            disposition,
        });
    }
    if !family_members.is_empty() {
        return Err(SkyrimCreatureLiveMvpPreparationBuildError::Input(format!(
            "reservations reference unknown motion families: {}",
            family_members.keys().cloned().collect::<Vec<_>>().join(",")
        )));
    }
    if !actor_action_plans.is_empty() {
        return Err(SkyrimCreatureLiveMvpPreparationBuildError::Input(format!(
            "Actor Action reservations reference unknown motion families: {}",
            actor_action_plans
                .keys()
                .cloned()
                .collect::<Vec<_>>()
                .join(",")
        )));
    }
    families.sort_by(|left, right| left.family_id.cmp(&right.family_id));
    if families.len() != super::creature_recipe::SKYRIM_CREATURE_FAMILY_COUNT {
        return Err(SkyrimCreatureLiveMvpPreparationBuildError::Input(format!(
            "expected {} terminal families, found {}",
            super::creature_recipe::SKYRIM_CREATURE_FAMILY_COUNT,
            families.len()
        )));
    }
    let ready_families = families
        .iter()
        .filter(|family| {
            matches!(
                family.disposition,
                SkyrimCreatureLivePreparedFamilyDisposition::Ready { .. }
            )
        })
        .count();
    Ok(SkyrimCreatureLivePreparationLedger {
        accounting: SkyrimCreatureLivePreparationAccounting {
            candidates: candidate_total,
            families: families.len(),
            ready_candidates,
            blocked_candidates: candidate_total.saturating_sub(ready_candidates),
            ready_families,
            blocked_families: families.len().saturating_sub(ready_families),
        },
        families,
    })
}

struct PreparedSkyrimCandidateRecipe {
    recipe: SourceRigExecutableRecipe,
    dependency_projections: Vec<SkyrimCreatureTargetDependencyProjection>,
    source_field_receipts: Vec<SkyrimCreatureSourceFieldReceipt>,
    warnings: Vec<CreatureDegradationReceipt>,
}

fn attach_actor_action_records(
    recipe: SourceRigExecutableRecipe,
    actor_action_records: &[CreatureActorActionRecordPlan],
) -> Result<SourceRigExecutableRecipe, String> {
    if actor_action_records.is_empty() {
        return Ok(recipe);
    }
    let SourceRigExecutableRecipe {
        rig,
        graph,
        projection,
        key_plan,
        batch_intent,
        bridge_receipt,
        ..
    } = recipe;
    let batch_intent = batch_intent.with_actor_action_dependencies(actor_action_records);
    let bridge_receipt = bridge_receipt
        .ok_or_else(|| "Skyrim source-rig recipe lost its bridge receipt".to_string())?;
    let field_receipts =
        SourceRigExecutableRecipe::required_field_receipts_from_policy_for_intent_with_actor_actions(
            &rig,
            &graph,
            &projection,
            &key_plan,
            actor_action_records,
            &batch_intent,
            Some(&bridge_receipt),
            "skyrim_live_creature_actor_actions_v1",
        );
    SourceRigExecutableRecipe::new_bridged_with_actor_actions(
        rig,
        graph,
        projection,
        key_plan,
        actor_action_records.to_vec(),
        batch_intent,
        bridge_receipt,
        field_receipts,
    )
    .map_err(|error| error.to_string())
}

fn candidate_attack_warnings(
    candidate: &super::creature_recipe::SkyrimCreatureCandidateRecipe,
    source_race: FormKey,
    dependency_projections: &[SkyrimCreatureTargetDependencyProjection],
    interner: &StringInterner,
) -> Result<Vec<CreatureDegradationReceipt>, String> {
    let source_key = source_key(source_race, interner);
    let mut warnings = Vec::new();
    for attack in &candidate.attack_data {
        if attack.attack_type.is_some() {
            warnings.push(CreatureDegradationReceipt {
                code: "skyrim_attack_type_runtime_inert".to_string(),
                detail: format!(
                    "ATKD[{}] event {:?} KYWD is retained only as a runtime-inert FO4 policy",
                    attack.ordinal, attack.event
                ),
                policy_id: "skyrim_creature_attack_policy_v1".to_string(),
                disposition: CreatureFallbackDisposition::Omitted,
                affected_source_keys: vec![source_key.clone()],
                source_signature: Some("KYWD".to_string()),
            });
        }
        match attack
            .attack_spell
            .as_ref()
            .map(|spell| spell.signature.as_str())
        {
            Some("SHOU") => warnings.push(CreatureDegradationReceipt {
                code: "skyrim_shout_attack_replaced".to_string(),
                detail: format!(
                    "ATKD[{}] event {:?} SHOU is represented by a graph-compatible FO4 fallback attack",
                    attack.ordinal, attack.event
                ),
                policy_id: "skyrim_creature_attack_policy_v1".to_string(),
                disposition: CreatureFallbackDisposition::ReplacedByGeneratedRuntime,
                affected_source_keys: vec![source_key.clone()],
                source_signature: Some("SHOU".to_string()),
            }),
            Some("SPEL")
                if attack.attack_spell.as_ref().is_some_and(|spell| {
                    parse_form_key(&spell.source, interner).is_some_and(|source| {
                        !dependency_projections
                            .iter()
                            .any(|projection| projection.source.form_key == source)
                    })
                }) =>
            {
                warnings.push(CreatureDegradationReceipt {
                    code: "skyrim_unsupported_spell_attack_replaced".to_string(),
                    detail: format!(
                        "ATKD[{}] event {:?} unsupported SPEL is represented by a graph-compatible FO4 fallback attack",
                        attack.ordinal, attack.event
                    ),
                    policy_id: "skyrim_creature_attack_policy_v1".to_string(),
                    disposition: CreatureFallbackDisposition::ReplacedByGeneratedRuntime,
                    affected_source_keys: vec![source_key.clone()],
                    source_signature: Some("SPEL".to_string()),
                })
            }
            None => warnings.push(CreatureDegradationReceipt {
                code: "skyrim_unarmed_attack_default".to_string(),
                detail: format!(
                    "ATKD[{}] event {:?} has no spell and receives a generated FO4 unarmed attack",
                    attack.ordinal, attack.event
                ),
                policy_id: "skyrim_creature_attack_policy_v1".to_string(),
                disposition: CreatureFallbackDisposition::TargetDefault,
                affected_source_keys: vec![source_key.clone()],
                source_signature: Some("ATKD".to_string()),
            }),
            _ => {}
        }
    }
    canonical_degradation_receipts(&mut warnings)?;
    Ok(warnings)
}

fn canonical_degradation_receipts(
    warnings: &mut Vec<CreatureDegradationReceipt>,
) -> Result<(), String> {
    for warning in warnings.iter_mut() {
        warning.canonicalize();
        warning.validate()?;
    }
    warnings.sort();
    warnings.dedup();
    Ok(())
}

fn prepare_live_skyrim_family_assets(
    family: &SkyrimCreatureFamilyJob,
    _evidence: &SkyrimCreatureLiveRecipeEvidence,
    source_data_root: &Path,
    private_staging_root: &Path,
) -> Result<PreparedSkyrimLiveFamilyAssets, String> {
    let graph = graph_manifest(family).map_err(|error| error.to_string())?;
    let [variant] = family.graph_variants.as_slice() else {
        return Err(format!(
            "family {} has {} graph variants; stage2 requires one exact project/character/skeleton owner",
            family.family_id,
            family.graph_variants.len()
        ));
    };
    let controllers = family
        .controllers
        .iter()
        .filter(|controller| same_runtime_path(&controller.character_path, &variant.character_path))
        .collect::<Vec<_>>();
    let [controller] = controllers.as_slice() else {
        return Err("selected graph variant has no unique controller receipt".to_string());
    };
    let capsule = controller
        .capsule
        .as_ref()
        .ok_or_else(|| "selected controller has no exact capsule".to_string())?;
    let controller_decl = controller_decl_from_receipt(controller)?;
    let namespace_hash = blake3::hash(family.family_id.as_bytes())
        .to_hex()
        .to_string();
    let namespace = format!("b21_skyrim_{}", &namespace_hash[..16]);
    let runtime_root = skyrim_family_runtime_root(&family.family_id);
    let creature_name = runtime_root
        .strip_prefix("Actors\\")
        .expect("Skyrim family runtime root is actor-relative")
        .to_string();
    let family_staging_root = private_staging_root.join("families").join(&namespace);
    let closure_staging_root = family_staging_root.join("closure");
    let mut closure_inputs = vec![CreatureNifInput {
        role: CreatureNifRole::Skeleton,
        source_data_relative_path: data_relative_mesh_path(&variant.skeleton_path),
        source_owner: family.family_id.clone(),
        body_variant: None,
    }];
    let mut body_paths = family
        .body_variants
        .iter()
        .map(|body| canonical_runtime_path(&body.body_nif))
        .collect::<BTreeSet<_>>();
    if body_paths.is_empty() {
        return Err("Ready family has no source body NIF".to_string());
    }
    closure_inputs.extend(
        body_paths
            .iter()
            .enumerate()
            .map(|(index, path)| CreatureNifInput {
                role: CreatureNifRole::Body,
                source_data_relative_path: data_relative_mesh_path(path),
                source_owner: family.family_id.clone(),
                body_variant: Some(format!("body_{index:04}")),
            }),
    );
    let mut closure = stage_creature_nif_closure(&CreatureClosureRequest {
        source_game: "skyrimse".to_string(),
        source_data_root: source_data_root.to_path_buf(),
        private_staging_root: closure_staging_root.clone(),
        target_namespace: runtime_root.clone(),
        texture_fallbacks: std::collections::BTreeMap::new(),
        inputs: closure_inputs,
    })
    .map_err(|error| format!("creature NIF closure failed: {error}"))?;
    let closure_staged_data_root = closure_staging_root.join("data");
    let visual_skeleton_input = closure
        .inputs
        .iter()
        .find(|input| input.role == CreatureNifRole::Skeleton)
        .ok_or_else(|| "creature closure omitted the visual skeleton NIF".to_string())?;
    let visual_skeleton_target = visual_skeleton_input.target_data_relative_path.clone();
    let visual_skeleton_nif = runtime_nif_path(&visual_skeleton_target);
    let visual_skeleton_path = closure_staged_data_root
        .join(visual_skeleton_target.replace('/', std::path::MAIN_SEPARATOR_STR));
    let mut visual_skeleton = NifFile::load(&visual_skeleton_path).map_err(|error| {
        format!(
            "load staged visual skeleton {}: {error}",
            visual_skeleton_path.display()
        )
    })?;
    install_fo4_creature_controller_collision(
        &mut visual_skeleton,
        capsule.total_height,
        capsule.radius,
        [
            controller_decl.model_up_ms[0],
            controller_decl.model_up_ms[1],
            controller_decl.model_up_ms[2],
        ],
        controller_decl.collision_filter_info,
    )
    .map_err(|error| format!("install FO4 CharacterController collision: {error}"))?;
    visual_skeleton
        .save(Some(visual_skeleton_path.clone()))
        .map_err(|error| format!("save controller-bearing visual skeleton: {error}"))?;
    refresh_staged_creature_nif_artifact(
        &mut closure,
        &closure_staged_data_root,
        &visual_skeleton_target,
    )
    .map_err(|error| format!("refresh controller-bearing NIF receipt: {error}"))?;
    let mut body_runtime_paths = BTreeMap::new();
    for input in closure
        .inputs
        .iter()
        .filter(|input| input.role == CreatureNifRole::Body)
    {
        body_runtime_paths.insert(
            path_key(&input.source_data_relative_path),
            runtime_nif_path(&input.target_data_relative_path),
        );
    }
    if body_paths
        .iter()
        .any(|path| !body_runtime_paths.contains_key(&path_key(path)))
    {
        let missing = body_paths
            .iter()
            .filter(|path| !body_runtime_paths.contains_key(&path_key(path)))
            .cloned()
            .collect::<Vec<_>>();
        return Err(format!(
            "creature closure did not retain body variants: {}",
            missing.join(", ")
        ));
    }

    let selected_clip_ids = graph
        .roles
        .iter()
        .flat_map(|role| {
            role.generator
                .clip_names(&role.clip_name)
                .into_iter()
                .map(str::to_string)
        })
        .chain(
            graph
                .overlays
                .iter()
                .map(|overlay| overlay.clip_name.clone()),
        )
        .collect::<BTreeSet<_>>();
    let selected_clips = selected_clip_ids
        .iter()
        .map(|clip_id| {
            family
                .clips
                .iter()
                .find(|clip| clip.clip_id == *clip_id)
                .ok_or_else(|| format!("graph references missing clip receipt {clip_id}"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut skeleton_names = BTreeSet::new();
    for clip in &selected_clips {
        let bytes = fs::read(asset_path(source_data_root, &clip.clip_path))
            .map_err(|error| format!("read clip {}: {error}", clip.clip_path))?;
        skeleton_names.insert(animation_binding(&bytes)?.original_skeleton_name);
    }
    let skeleton_names = skeleton_names.into_iter().collect::<Vec<_>>();
    let [skeleton_runtime_name] = skeleton_names.as_slice() else {
        return Err(
            "selected clips do not share one exact hkaAnimationBinding.originalSkeletonName"
                .to_string(),
        );
    };
    let skeleton_runtime_path = format!("{runtime_root}\\CharacterAssets\\Skeleton.hkx");
    let [animation_skeleton_path] = family.inventory.animation_skeleton_paths.as_slice() else {
        return Err(format!(
            "family has {} animation skeletons; stage2 requires one exact source owner",
            family.inventory.animation_skeleton_paths.len()
        ));
    };
    let skeleton_source_path = asset_path(source_data_root, animation_skeleton_path);
    let skeleton = crate::phase::skeleton::convert_skyrim_source_owned_skeleton_artifact(
        &skeleton_source_path,
        &skeleton_runtime_path,
    )
    .map_err(|error| format!("source-owned skeleton conversion failed: {error}"))?;
    if skeleton.skeleton_name != *skeleton_runtime_name {
        return Err(format!(
            "source animation skeleton name {:?} does not match selected clip binding {:?}",
            skeleton.skeleton_name, skeleton_runtime_name
        ));
    }
    let converted_root = family_staging_root.join("converted");
    let mut artifact_requests = Vec::new();
    let mut artifact_receipts = Vec::new();
    let mut converted_artifacts = Vec::new();
    let skeleton_source_hash = exact_asset_hash(
        family,
        animation_skeleton_path,
        SkyrimAssetRole::AnimationSkeleton,
    )?;
    stage_skyrim_converted_artifact(
        &converted_root,
        SourceRigArtifactRole::AnimationSkeleton,
        SourceRigArtifactProvenance::ReconstructedSourceSkeleton,
        &skeleton_runtime_path,
        skeleton_source_hash,
        &skeleton.hkx_bytes,
        &mut artifact_requests,
        &mut artifact_receipts,
        &mut converted_artifacts,
    )?;

    let mut clip_declarations = Vec::new();
    for clip in selected_clips {
        let source_path = asset_path(source_data_root, &clip.clip_path);
        let source_bytes = fs::read(&source_path)
            .map_err(|error| format!("read source clip {}: {error}", source_path.display()))?;
        let binding = animation_binding(&source_bytes)?;
        let converted =
            havok_native::api::havok_reemit_skyrim_2010_animation_asset_to_fo4(&source_bytes)
                .map_err(|error| format!("convert Skyrim clip {}: {error}", clip.clip_path))?;
        let converted = preserve_bound_root_motion(&converted, clip)?;
        let runtime_path = format!("{runtime_root}\\Animations\\{}.hkx", clip.clip_id);
        let clip_binding = bind_clip_to_skeleton(binding, &skeleton, &skeleton_runtime_path)?;
        let looping = clip_is_semantically_looping(&graph, &clip.clip_id);
        clip_declarations.push(ClipDecl {
            name: clip.clip_id.clone(),
            path: runtime_path.clone(),
            binding: clip_binding,
            looping,
        });
        stage_skyrim_converted_artifact(
            &converted_root,
            SourceRigArtifactRole::AnimationClip {
                clip_name: clip.clip_id.clone(),
            },
            SourceRigArtifactProvenance::ConvertedSourceClip,
            &runtime_path,
            exact_asset_hash(family, &clip.clip_path, SkyrimAssetRole::AnimationClip)?,
            &converted,
            &mut artifact_requests,
            &mut artifact_receipts,
            &mut converted_artifacts,
        )?;
    }
    clip_declarations.sort_by_key(|clip| clip.name.to_ascii_lowercase());

    let ragdoll = match &family.ragdoll {
        SkyrimRagdollCapabilityEvidence::Present { paths } => {
            let [source_path] = paths.as_slice() else {
                return Err(format!(
                    "family has {} ragdoll paths; stage2 requires one exact source owner",
                    paths.len()
                ));
            };
            let source = asset_path(source_data_root, source_path);
            let bytes = fs::read(&source)
                .map_err(|error| format!("read ragdoll {}: {error}", source.display()))?;
            let hkx = HkxFile::read(&bytes)
                .map_err(|error| format!("decode ragdoll {}: {error}", source.display()))?;
            let ir = extract_skyrim_2010_creature_ragdoll(&hkx)
                .map_err(|error| format!("extract Skyrim ragdoll: {error}"))?;
            let lowered = lower_primitive_shapes_to_fo4_convex_hulls(&ir, 12)
                .map_err(|error| format!("lower Skyrim ragdoll shapes: {error}"))?;
            let mut visual_skeleton = NifFile::load(&visual_skeleton_path).map_err(|error| {
                format!(
                    "reload staged visual skeleton {}: {error}",
                    visual_skeleton_path.display()
                )
            })?;
            let ragdoll_body_count =
                install_fo4_creature_ragdoll_collision(&mut visual_skeleton, &lowered)
                    .map_err(|error| format!("install embedded FO4 ragdoll collision: {error}"))?;
            visual_skeleton
                .save(Some(visual_skeleton_path.clone()))
                .map_err(|error| format!("save ragdoll-bearing visual skeleton: {error}"))?;
            seal_embedded_creature_collision(
                &mut closure,
                &closure_staged_data_root,
                &visual_skeleton_target,
                ragdoll_body_count,
            )
            .map_err(|error| format!("seal embedded ragdoll NIF receipt: {error}"))?;
            let converted = reconstruct_fo4_creature_ragdoll_packfile(&lowered)
                .map_err(|error| format!("reconstruct FO4 ragdoll: {error}"))?;
            let runtime_path = format!("{runtime_root}\\CharacterAssets\\Ragdoll.hkx");
            stage_skyrim_converted_artifact(
                &converted_root,
                SourceRigArtifactRole::Ragdoll,
                SourceRigArtifactProvenance::ReconstructedSourceRagdoll,
                &runtime_path,
                exact_asset_hash(family, source_path, SkyrimAssetRole::Ragdoll)?,
                &converted,
                &mut artifact_requests,
                &mut artifact_receipts,
                &mut converted_artifacts,
            )?;
            RagdollDisposition::SourceOwned {
                runtime_path,
                receipt: SourceOwnedRagdollReceipt {
                    byte_len: converted.len() as u64,
                    blake3: blake3::hash(&converted).to_hex().to_string(),
                    powered_ragdoll: None,
                },
            }
        }
        SkyrimRagdollCapabilityEvidence::NotApplicable { .. } => {
            let reason = if closure.inputs.iter().any(|entry| {
                matches!(
                    entry.collision,
                    nif_core_native::creature_closure::CreatureCollisionDisposition::ArticulatedDeferredForHkx { .. }
                )
            }) {
                NoRagdollReason::UnsupportedSourceRagdoll
            } else {
                NoRagdollReason::SourceHasNoRagdoll
            };
            RagdollDisposition::NoRagdoll { reason }
        }
        disposition => return Err(format!("ragdoll is not terminal: {disposition:?}")),
    };

    let declarations = GraphDeclarations {
        events: graph.explicit_events.clone(),
        variables: family.graph.variables.clone(),
        character_properties: Vec::new(),
    };
    let mut root_declarations = declarations.clone();
    if matches!(ragdoll, RagdollDisposition::SourceOwned { .. }) {
        for name in RAGDOLL_TRANSITION_EVENTS
            .into_iter()
            .chain(RAGDOLL_ENTER_EVENTS)
        {
            if root_declarations
                .events
                .iter()
                .all(|event| event.name != name)
            {
                root_declarations.events.push(crate::source_rig::EventDecl {
                    name: name.to_string(),
                    usage: crate::source_rig::EventUsage::Generic,
                    flags: 0,
                });
            }
        }
        root_declarations
            .events
            .sort_by_key(|event| event.name.to_ascii_lowercase());
    }
    let rig = CreatureManifest {
        creature_name,
        visual_skeleton_nif,
        animation_skeleton: SkeletonDecl {
            path: skeleton_runtime_path,
            runtime_name: skeleton.skeleton_name.clone(),
            bones: skeleton
                .ordered_bone_names
                .iter()
                .zip(&skeleton.parent_indices)
                .map(|(name, parent)| BoneDecl {
                    name: name.clone(),
                    parent_index: usize::try_from(*parent).ok(),
                })
                .collect(),
            float_slots: skeleton.float_slot_names.clone(),
        },
        controller: controller_decl,
        ragdoll,
        clips: clip_declarations,
        idle_clip: graph
            .roles
            .iter()
            .find(|role| {
                role.trigger_event.is_none()
                    && match graph.template {
                        CreatureGraphTemplate::Swim => {
                            role.role == crate::source_rig::CreatureClipRole::SwimIdle
                        }
                        CreatureGraphTemplate::Fly => {
                            role.role == crate::source_rig::CreatureClipRole::FlyIdle
                        }
                        CreatureGraphTemplate::StationaryTurret => {
                            role.role == crate::source_rig::CreatureClipRole::StationaryIdle
                        }
                        _ => role.role == crate::source_rig::CreatureClipRole::Idle,
                    }
            })
            .map(|role| role.clip_name.clone())
            .ok_or_else(|| "graph has no source-proven zero-trigger idle role".to_string())?,
        capsule: Capsule {
            height: capsule.total_height,
            radius: capsule.radius,
        },
        paths: ScaffoldPaths {
            project: format!("{runtime_root}\\SkyrimProject.hkx"),
            character: format!("{runtime_root}\\Characters\\SkyrimCharacter.hkx"),
            root_behavior: format!("{runtime_root}\\Behaviors\\SkyrimRootBehavior.hkx"),
            core_behavior: format!("{runtime_root}\\Behaviors\\SkyrimCoreBehavior.hkx"),
        },
        root: root_declarations,
        core: declarations,
    };
    rig.validate_capability_graph(&graph)
        .map_err(|error| format!("live source-rig graph validation failed: {error}"))?;
    artifact_requests.sort_by_key(|request| {
        (
            request.runtime_path.to_ascii_lowercase(),
            request.role.clone(),
        )
    });
    artifact_receipts.sort_by_key(|receipt| {
        (
            receipt.runtime_path.to_ascii_lowercase(),
            receipt.role.clone(),
        )
    });
    converted_artifacts.sort_by_key(|artifact| artifact.runtime_path.to_ascii_lowercase());

    let mut selection_by_race = BTreeMap::new();
    for body in &family.body_variants {
        selection_by_race
            .entry(body.source_race.to_ascii_lowercase())
            .or_insert_with(|| SkyrimSourceRigFamilySelection {
                sex: body.sex,
                project_path: variant.project_path.clone(),
                character_path: variant.character_path.clone(),
                skeleton_path: variant.skeleton_path.clone(),
                body_nif_path: body.body_nif.clone(),
            });
    }
    body_paths.clear();
    Ok(PreparedSkyrimLiveFamilyAssets {
        closure,
        closure_staged_data_root,
        rig,
        artifact_requests,
        artifact_receipts,
        converted_artifacts,
        body_runtime_paths,
        selection_by_race,
    })
}

#[allow(clippy::too_many_arguments)]
fn prepare_live_skyrim_candidate_recipe(
    source_race: FormKey,
    family: &SkyrimCreatureFamilyJob,
    assets: &PreparedSkyrimLiveFamilyAssets,
    evidence: &SkyrimCreatureLiveRecipeEvidence,
    dependency: &SkyrimCreatureDependencyLedger,
    records: &HashMap<FormKey, &Record>,
    reservations: &HashMap<FormKey, &SkyrimCreatureRecordReservation>,
    ancillary_reservations: &[SkyrimCreatureAncillaryReservationReceipt],
    actor_action_records: &[CreatureActorActionRecordPlan],
    mapper_state: &MapperState,
    target_plugin: &str,
    editor_id_prefix: &str,
    interner: &StringInterner,
) -> Result<PreparedSkyrimCandidateRecipe, String> {
    let source_race_key = source_race.format(interner);
    let candidate = evidence
        .recipe_ledger
        .candidates
        .iter()
        .find(|candidate| candidate.source_race.eq_ignore_ascii_case(&source_race_key))
        .ok_or_else(|| "recipe ledger omitted the source RACE".to_string())?;
    let dependency_candidate = dependency
        .candidates
        .iter()
        .find(|candidate| candidate.source_race == source_race)
        .ok_or_else(|| "dependency ledger omitted the source RACE".to_string())?;
    let primary_source_identity = source_identity(source_race, interner)?;
    let race_target = reserved_target(
        source_race,
        SkyrimCreatureReservedRecordKind::Race,
        reservations,
        target_plugin,
    )?;
    let skin_source = candidate
        .source_skin
        .as_deref()
        .and_then(|value| parse_form_key(value, interner))
        .ok_or_else(|| "candidate has no exact source ARMO skin".to_string())?;
    let (skin_target, _) = projected_creature_record_target(
        &evidence.catalog,
        source_race,
        skin_source,
        SkyrimCreatureReservedRecordKind::Armor,
        SkyrimCreatureAncillaryRecordKind::ArmorVariant,
        reservations,
        ancillary_reservations,
        target_plugin,
        interner,
    )?;
    let mut armor_addon_sources = candidate
        .source_armor_addons
        .iter()
        .filter_map(|value| parse_form_key(value, interner))
        .collect::<Vec<_>>();
    armor_addon_sources.sort_by_key(|source| form_key_sort_key(*source, interner));
    armor_addon_sources.dedup();
    if armor_addon_sources.is_empty() {
        return Err("candidate has no source ARMA records".to_string());
    }
    let armor_addon_targets = armor_addon_sources
        .iter()
        .map(|source| {
            projected_creature_record_target(
                &evidence.catalog,
                source_race,
                *source,
                SkyrimCreatureReservedRecordKind::ArmorAddon,
                SkyrimCreatureAncillaryRecordKind::ArmorAddonVariant,
                reservations,
                ancillary_reservations,
                target_plugin,
                interner,
            )
            .map(|(target, _)| target)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let body_part_source = candidate
        .source_body_part_data
        .as_deref()
        .and_then(|value| parse_form_key(value, interner))
        .ok_or_else(|| "candidate has no exact source BPTD".to_string())?;
    let (body_part_target, _) = projected_creature_record_target(
        &evidence.catalog,
        source_race,
        body_part_source,
        SkyrimCreatureReservedRecordKind::BodyPartData,
        SkyrimCreatureAncillaryRecordKind::BodyPartDataVariant,
        reservations,
        ancillary_reservations,
        target_plugin,
        interner,
    )?;
    let mut dependency_projections = target_dependency_projections(
        dependency_candidate,
        dependency,
        reservations,
        mapper_state,
        target_plugin,
        interner,
    )?;
    let race_spells = candidate
        .attack_spells
        .iter()
        .map(|source| {
            parse_form_key(source, interner)
                .ok_or_else(|| format!("invalid candidate spell locator {source}"))
        })
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .filter_map(|source| {
            dependency_projections
                .iter()
                .find(|projection| {
                    projection.source.form_key == source
                        && projection.source.signature == "SPEL"
                        && projection.target.signature == "SPEL"
                })
                .map(|projection| projection.target.clone())
        })
        .collect::<Vec<_>>();

    let npc_sources = projected_creature_npc_sources(dependency_candidate, interner);
    let mut npc_variants = Vec::with_capacity(npc_sources.len().max(1));
    let mut npc_key_plan = Vec::with_capacity(npc_sources.len().max(1));
    if npc_sources.is_empty() {
        let target = ancillary_target(
            ancillary_reservations,
            source_race,
            "synthetic_npc",
            0,
            SkyrimCreatureAncillaryRecordKind::SyntheticNpc,
            target_plugin,
            interner,
        )?;
        let editor_id = projected_editor_id(
            editor_id_prefix,
            candidate.editor_id.as_deref().unwrap_or("CreatureNPC"),
            source_race.local,
        );
        npc_variants.push(CreatureNpcRecordVariant {
            source_identity: primary_source_identity.clone(),
            form_key: target.clone(),
            editor_id,
            display_name: candidate
                .editor_id
                .clone()
                .unwrap_or_else(|| "Skyrim Creature".to_string()),
            primary: true,
            level: 1,
            health: 50,
            action_points: 50,
            npc_inventory: Some(Vec::new()),
            npc_equipment: None,
            npc_spells: Some(race_spells.clone()),
            npc_death_item: None,
        });
        npc_key_plan.push(CreatureNpcRecordKeyPlan {
            source_identity: primary_source_identity.clone(),
            form_key: target,
            primary: true,
        });
    }
    for (index, source_npc) in npc_sources.iter().copied().enumerate() {
        let (target, cloned_for_race) = projected_creature_record_target(
            &evidence.catalog,
            source_race,
            source_npc,
            SkyrimCreatureReservedRecordKind::Npc,
            SkyrimCreatureAncillaryRecordKind::NpcVariant,
            reservations,
            ancillary_reservations,
            target_plugin,
            interner,
        )?;
        let record = records
            .get(&source_npc)
            .copied()
            .ok_or_else(|| format!("missing winning NPC_ {}", source_npc.format(interner)))?;
        let stats_record = effective_npc_template_record(record, 0x0002, records, interner)?;
        let inventory_record = effective_npc_template_record(record, 0x0100, records, interner)?;
        let spell_record = effective_npc_template_record(record, 0x0008, records, interner)?;
        let stats = skyrim_npc_runtime_stats(stats_record, interner)?;
        let identity = if index == 0 {
            primary_source_identity.clone()
        } else {
            source_identity(source_npc, interner)?
        };
        let mut editor_id = projected_editor_id(
            editor_id_prefix,
            record_editor_id(record, interner)
                .as_deref()
                .unwrap_or("CreatureNPC"),
            source_npc.local,
        );
        if cloned_for_race {
            editor_id.push_str(&format!("_R{:06X}", source_race.local));
        }
        let primary = index == 0;
        let npc_inventory = source_npc_inventory(
            inventory_record,
            dependency,
            &dependency_projections,
            records,
            reservations,
            target_plugin,
            interner,
        )?;
        let mut npc_spells = source_record_target_references(
            spell_record.form_key,
            "SPLO",
            "SPEL",
            dependency,
            &dependency_projections,
        )?;
        for spell in &race_spells {
            if !npc_spells.contains(spell) {
                npc_spells.push(spell.clone());
            }
        }
        npc_variants.push(CreatureNpcRecordVariant {
            source_identity: identity.clone(),
            form_key: target.clone(),
            editor_id,
            display_name: record_display_name(record, interner)
                .or_else(|| candidate.editor_id.clone())
                .unwrap_or_else(|| "Skyrim Creature".to_string()),
            primary,
            level: stats.level,
            health: stats.health,
            action_points: stats.action_points,
            npc_inventory: Some(npc_inventory),
            npc_equipment: None,
            npc_spells: Some(npc_spells),
            npc_death_item: None,
        });
        npc_key_plan.push(CreatureNpcRecordKeyPlan {
            source_identity: identity,
            form_key: target,
            primary,
        });
    }
    let primary_npc_target = npc_key_plan[0].form_key.clone();
    let editor_stem = projected_editor_id(
        editor_id_prefix,
        candidate.editor_id.as_deref().unwrap_or("Creature"),
        source_race.local,
    );
    let editor_id = |suffix: &str| format!("{editor_stem}{suffix}");
    let fallback_spell = npc_variants
        .iter()
        .find(|variant| variant.primary)
        .and_then(|variant| variant.npc_spells.as_ref())
        .and_then(|spells| spells.first());
    let attacks = live_attack_projections(
        candidate,
        family,
        &dependency_projections,
        reservations,
        ancillary_reservations,
        fallback_spell,
        target_plugin,
        &editor_stem,
        interner,
    )?;
    let melee_attacks = melee_record_key_plan(&attacks);
    let (unarmed_weapon, unarmed_weapon_editor_id) =
        base_unarmed_weapon_identity(&attacks, target_plugin, &editor_id("Unarmed"));
    let mut body_nif_parts = Vec::new();
    let mut projected_armor_addon_sources = HashSet::new();
    for body in family
        .body_variants
        .iter()
        .filter(|body| body.source_race.eq_ignore_ascii_case(&source_race_key))
    {
        let source = parse_form_key(&body.armor_addon, interner)
            .ok_or_else(|| format!("invalid body ARMA locator {}", body.armor_addon))?;
        if !projected_armor_addon_sources.insert(source) {
            continue;
        }
        let index = armor_addon_sources
            .iter()
            .position(|candidate| *candidate == source)
            .ok_or_else(|| "body variant references an unreserved ARMA".to_string())?;
        let body_nif = assets
            .body_runtime_paths
            .get(&path_key(&body.body_nif))
            .cloned()
            .ok_or_else(|| "body variant NIF is absent from closure receipt".to_string())?;
        body_nif_parts.push(CreatureBodyNifRecordPart {
            body_nif,
            armor_addon_form_key: armor_addon_targets[index].clone(),
            armor_addon_editor_id: format!("{editor_stem}BodyAA_{index:02}"),
        });
    }
    if body_nif_parts.is_empty() {
        return Err("candidate has no exact body NIF/ARMA parts".to_string());
    }
    let base = CreatureRecordManifest {
        target_plugin: target_plugin.to_string(),
        editor_id_prefix: editor_id_prefix.to_string(),
        form_keys: CreatureRecordFormKeys {
            race: race_target,
            npc: primary_npc_target.clone(),
            skin: skin_target,
            armor_addon: body_nif_parts[0].armor_addon_form_key.clone(),
            body_part_data: body_part_target,
            unarmed_weapon,
        },
        editor_ids: CreatureRecordEditorIds {
            race: editor_id("Race"),
            npc: npc_variants[0].editor_id.clone(),
            skin: editor_id("Skin"),
            armor_addon: body_nif_parts[0].armor_addon_editor_id.clone(),
            body_part_data: editor_id("BodyPartData"),
            unarmed_weapon: unarmed_weapon_editor_id,
        },
        display_name: candidate
            .editor_id
            .clone()
            .unwrap_or_else(|| "Skyrim Creature".to_string()),
        body_nif: body_nif_parts[0].body_nif.clone(),
    };
    let race_data = candidate
        .race_data
        .derivation
        .as_ref()
        .and_then(|derivation| match &derivation.mapping {
            RaceDataMapping::Mapped { target } => Some(target.clone()),
            RaceDataMapping::Missing { .. } => None,
        })
        .ok_or_else(|| "candidate has no complete FO4 RACE.DATA derivation".to_string())?;
    let selected_variant = family
        .graph_variants
        .first()
        .ok_or_else(|| "family has no graph variant".to_string())?;
    let projected_armor_addons = body_nif_parts
        .iter()
        .map(|part| part.armor_addon_form_key.clone())
        .collect::<Vec<_>>();
    let projection = CreatureRecordProjectionManifest {
        base: base.clone(),
        source_primary_identity: primary_source_identity.clone(),
        race_data,
        body_parts: CreatureBodyPartProjection::root_only_32(
            selected_variant.actual_root_bone.clone(),
        ),
        body_nif_parts,
        variants: npc_variants,
        npc_inventory: Vec::new(),
        npc_equipment: Vec::new(),
        npc_spells: Vec::new(),
        npc_death_item: None,
        attacks,
    };
    let required_target_records = projection_dependencies(&projection);
    let key_plan = CreatureRecordKeyPlan {
        planned_source_identity: primary_source_identity.clone(),
        base: base.form_keys.clone(),
        armor_addons: projected_armor_addons,
        npc_variants: npc_key_plan,
        melee_attacks,
    };
    let batch_intent = SourceRigRecordBatchIntent {
        family_id: family.family_id.clone(),
        primary_mapping: CreaturePrimaryRecordMapping {
            source: primary_source_identity,
            target: primary_npc_target,
        },
        required_target_records,
    };
    let selection = assets
        .selection_by_race
        .get(&source_race_key.to_ascii_lowercase())
        .cloned()
        .ok_or_else(|| "family body selection omitted candidate RACE".to_string())?;
    let mut bridge = SkyrimSourceRigBridgeInput {
        ledger: evidence.recipe_ledger.clone(),
        family_id: family.family_id.clone(),
        selection,
        creature_closure: assets.closure.clone(),
        creature_closure_request_blake3: assets.closure.request_blake3.clone(),
        artifact_requests: assets.artifact_requests.clone(),
        artifact_receipts: assets.artifact_receipts.clone(),
        rig: assets.rig.clone(),
        projection,
        key_plan,
        batch_intent,
        field_decisions: Vec::new(),
    };
    let required_fields = required_skyrim_source_rig_field_names(bridge.clone())
        .map_err(|error| error.to_string())?;
    bridge.field_decisions = required_fields
        .into_iter()
        .map(|field| SkyrimSourceRigFieldDecision {
            field: field.clone(),
            decision: SourceRigFieldDecision::Derived {
                source_games: vec!["skyrimse".to_string()],
                source_fields: vec![field],
                policy_id: "skyrim_live_creature_projection_v1".to_string(),
            },
        })
        .collect();
    let recipe =
        build_skyrim_source_rig_executable_recipe(bridge).map_err(|error| error.to_string())?;
    let recipe = attach_actor_action_records(recipe, actor_action_records)?;
    let source_field_receipts = source_projection_field_receipts(dependency_candidate, dependency);
    dependency_projections.sort_by_key(|projection| {
        (
            projection.target.form_key.plugin.to_ascii_lowercase(),
            projection.target.form_key.local,
            projection.target.signature.clone(),
        )
    });
    let warnings =
        candidate_attack_warnings(candidate, source_race, &dependency_projections, interner)?;
    Ok(PreparedSkyrimCandidateRecipe {
        recipe,
        dependency_projections,
        source_field_receipts,
        warnings,
    })
}

fn projected_creature_npc_sources(
    candidate: &SkyrimCreatureDependencyCandidate,
    interner: &StringInterner,
) -> Vec<FormKey> {
    let mut sources = candidate.npc_sources.clone();
    sources.sort_by_key(|source| form_key_sort_key(*source, interner));
    sources.dedup();
    sources
}

fn projected_creature_record_owner_races(
    catalog: &CreatureCorpusPlan,
    source: FormKey,
    kind: SkyrimCreatureReservedRecordKind,
) -> Vec<FormKey> {
    match kind {
        SkyrimCreatureReservedRecordKind::Npc => {
            let Some(npc) = catalog.npcs.iter().find(|npc| npc.source_npc == source) else {
                return Vec::new();
            };
            catalog
                .races
                .iter()
                .map(|race| race.source_race)
                .filter(|source_race| npc.effective_races.contains(source_race))
                .collect()
        }
        SkyrimCreatureReservedRecordKind::Armor => catalog
            .races
            .iter()
            .filter(|race| race.skin == Some(source))
            .map(|race| race.source_race)
            .collect(),
        SkyrimCreatureReservedRecordKind::ArmorAddon => catalog
            .races
            .iter()
            .filter(|race| race.armor_addons.contains(&source))
            .map(|race| race.source_race)
            .collect(),
        SkyrimCreatureReservedRecordKind::BodyPartData => catalog
            .races
            .iter()
            .filter(|race| race.body_part_data == Some(source))
            .map(|race| race.source_race)
            .collect(),
        SkyrimCreatureReservedRecordKind::Race => Vec::new(),
    }
}

fn record_variant_reservation_event(
    source: FormKey,
    interner: &StringInterner,
) -> Result<String, String> {
    let plugin = interner
        .resolve(source.plugin)
        .filter(|plugin| !plugin.trim().is_empty())
        .ok_or_else(|| "record variant source plugin is unavailable".to_string())?;
    Ok(format!("record_variant:{}", plugin.to_ascii_lowercase()))
}

#[allow(clippy::too_many_arguments)]
fn projected_creature_record_target(
    catalog: &CreatureCorpusPlan,
    source_race: FormKey,
    source: FormKey,
    source_kind: SkyrimCreatureReservedRecordKind,
    variant_kind: SkyrimCreatureAncillaryRecordKind,
    reservations: &HashMap<FormKey, &SkyrimCreatureRecordReservation>,
    ancillary_reservations: &[SkyrimCreatureAncillaryReservationReceipt],
    target_plugin: &str,
    interner: &StringInterner,
) -> Result<(TargetFormKey, bool), String> {
    let owner_races = projected_creature_record_owner_races(catalog, source, source_kind);
    if !owner_races.contains(&source_race) {
        return Err(format!(
            "source {:?} {} is not owned by creature RACE {}",
            source_kind,
            source.format(interner),
            source_race.format(interner)
        ));
    }
    if owner_races.first() == Some(&source_race) {
        return reserved_target(source, source_kind, reservations, target_plugin)
            .map(|target| (target, false));
    }
    let event = record_variant_reservation_event(source, interner)?;
    ancillary_target(
        ancillary_reservations,
        source_race,
        &event,
        source.local as usize,
        variant_kind,
        target_plugin,
        interner,
    )
    .map(|target| (target, true))
}

struct AnimationBindingEvidence {
    original_skeleton_name: String,
    transform_tracks: usize,
    transform_indices: Vec<usize>,
    float_tracks: usize,
    float_indices: Vec<usize>,
}

fn preserve_bound_root_motion(
    converted: &[u8],
    clip: &SkyrimCreatureClipReceipt,
) -> Result<Vec<u8>, String> {
    let expected_sample_count = match &clip.root_motion {
        SkyrimRootMotionReceipt::Sampled {
            source: SkyrimRootMotionSourceReceipt::BoundAnims,
            sample_count,
            ..
        } => *sample_count,
        _ => return Ok(converted.to_vec()),
    };
    let bound_locators = clip
        .root_motion_locators
        .iter()
        .filter_map(|locator| match locator {
            SkyrimRootMotionLocatorReceipt::BoundAnims {
                source_path,
                project_stem,
                animation_index,
            } => Some((source_path, project_stem, *animation_index)),
            SkyrimRootMotionLocatorReceipt::HavokReferenceFrame { .. } => None,
        })
        .collect::<Vec<_>>();
    if bound_locators.is_empty() {
        return Err(format!(
            "sampled BoundAnims clip {} has no exact BoundAnims locator",
            clip.clip_id
        ));
    }
    let mut matching = Vec::new();
    let mut observed = Vec::new();
    for (source_path, project_stem, animation_index) in bound_locators {
        match load_skyrim_bound_motion_samples(
            Path::new(source_path),
            project_stem,
            animation_index,
        ) {
            Ok(samples) => {
                let count = samples.translations.len().max(samples.rotations.len());
                observed.push(format!("{project_stem}:{animation_index}={count}"));
                if bound_motion_matches_receipt(&samples, &clip.root_motion) {
                    matching.push(samples);
                }
            }
            Err(error) => observed.push(format!("{project_stem}:{animation_index}=error({error})")),
        }
    }
    let Some(samples) = matching.pop() else {
        return Err(format!(
            "no BoundAnims locator for {} matches its {}-sample receipt; observed {}",
            clip.clip_id,
            expected_sample_count,
            observed.join(", ")
        ));
    };
    let reference_frame_samples = planar_reference_frame_samples(&samples)?;
    for alternative in matching {
        if planar_reference_frame_samples(&alternative)? != reference_frame_samples {
            return Err(format!(
                "multiple BoundAnims locators for {} match its summary but contain different samples",
                clip.clip_id
            ));
        }
    }
    let mut hkx = HkxFile::read(converted)
        .map_err(|error| format!("decode converted clip {}: {error}", clip.clip_id))?;
    let animation_indices = hkx
        .objects()
        .iter()
        .enumerate()
        .filter_map(|(index, object)| {
            matches!(
                object.class_name.as_str(),
                "hkaLosslessCompressedAnimation"
                    | "hkaSplineCompressedAnimation"
                    | "hkaInterleavedUncompressedAnimation"
            )
            .then_some(index)
        })
        .collect::<Vec<_>>();
    let [animation_index] = animation_indices.as_slice() else {
        return Err(format!(
            "converted clip {} contains {} animation objects",
            clip.clip_id,
            animation_indices.len()
        ));
    };
    if hkx
        .objects()
        .iter()
        .any(|object| object.class_name == "hkaDefaultAnimatedReferenceFrame")
    {
        return Err(format!(
            "converted BoundAnims clip {} already contains a reference-frame object",
            clip.clip_id
        ));
    }
    let extracted_motion = hkx.objects()[*animation_index]
        .members
        .iter()
        .find(|member| member.name == "extractedMotion")
        .ok_or_else(|| {
            format!(
                "converted clip {} animation has no extractedMotion member",
                clip.clip_id
            )
        })?;
    if !matches!(extracted_motion.value, HkxValue::Pointer(None)) {
        return Err(format!(
            "converted BoundAnims clip {} already has embedded extracted motion",
            clip.clip_id
        ));
    }
    let reference_frame_index = hkx.push_object(HkxObject {
        name: None,
        offset: 0,
        signature: 0x60f8_e0b8,
        class_name: "hkaDefaultAnimatedReferenceFrame".to_string(),
        members: vec![
            HkxMember {
                name: "up".to_string(),
                value: HkxValue::F32List(vec![0.0, 0.0, 1.0, 0.0]),
            },
            HkxMember {
                name: "forward".to_string(),
                value: HkxValue::F32List(vec![0.0, 1.0, 0.0, 0.0]),
            },
            HkxMember {
                name: "duration".to_string(),
                value: HkxValue::F32(samples.duration),
            },
            HkxMember {
                name: "referenceFrameSamples".to_string(),
                value: HkxValue::Array(
                    reference_frame_samples
                        .into_iter()
                        .map(|sample| HkxValue::F32List(sample.to_vec()))
                        .collect(),
                ),
            },
        ],
    });
    let extracted_motion = hkx.objects_mut()[*animation_index]
        .members
        .iter_mut()
        .find(|member| member.name == "extractedMotion")
        .expect("checked extractedMotion above");
    extracted_motion.value = HkxValue::Pointer(Some(reference_frame_index));
    Ok(hkx.save())
}

fn bound_motion_matches_receipt(
    motion: &SkyrimBoundMotionSamples,
    receipt: &SkyrimRootMotionReceipt,
) -> bool {
    let SkyrimRootMotionReceipt::Sampled {
        source: SkyrimRootMotionSourceReceipt::BoundAnims,
        sample_count,
        translation_delta,
        rotation_start,
        rotation_end,
    } = receipt
    else {
        return false;
    };
    if motion.translations.len().max(motion.rotations.len()) != *sample_count {
        return false;
    }
    let actual_translation_delta = motion
        .translations
        .first()
        .zip(motion.translations.last())
        .map(|(first, last)| {
            std::array::from_fn(|component| last.value[component] - first.value[component])
        })
        .unwrap_or([0.0; 3]);
    if !actual_translation_delta
        .iter()
        .zip(translation_delta)
        .all(|(actual, expected)| (actual - expected).abs() <= 1.0e-5)
    {
        return false;
    }
    optional_quaternion_matches(
        motion.rotations.first().map(|sample| sample.value),
        *rotation_start,
    ) && optional_quaternion_matches(
        motion.rotations.last().map(|sample| sample.value),
        *rotation_end,
    )
}

fn optional_quaternion_matches(actual: Option<[f32; 4]>, expected: Option<[f32; 4]>) -> bool {
    match (actual, expected) {
        (None, None) => true,
        (Some(actual), Some(expected)) => {
            let same = actual
                .iter()
                .zip(expected)
                .map(|(actual, expected)| (actual - expected) * (actual - expected))
                .sum::<f32>()
                .sqrt();
            let negated = actual
                .iter()
                .zip(expected)
                .map(|(actual, expected)| (actual + expected) * (actual + expected))
                .sum::<f32>()
                .sqrt();
            same.min(negated) <= 1.0e-5
        }
        _ => false,
    }
}

fn planar_reference_frame_samples(
    motion: &SkyrimBoundMotionSamples,
) -> Result<Vec<[f32; 4]>, String> {
    let sample_count = motion.translations.len().max(motion.rotations.len());
    if sample_count == 0 || !motion.duration.is_finite() || motion.duration < 0.0 {
        return Err("BoundAnims motion has no samples or an invalid duration".to_string());
    }
    let mut samples = Vec::with_capacity(sample_count);
    let mut previous_yaw = None;
    for index in 0..sample_count {
        let translation = sample_linear_track(
            &motion.translations,
            index,
            sample_count,
            motion.duration,
            [0.0, 0.0, 0.0, 0.0],
        );
        let rotation =
            sample_quaternion_track(&motion.rotations, index, sample_count, motion.duration);
        if !translation.iter().all(|value| value.is_finite())
            || !rotation.iter().all(|value| value.is_finite())
        {
            return Err("BoundAnims motion contains a non-finite sample".to_string());
        }
        let mut yaw = quaternion_yaw(rotation);
        if let Some(previous) = previous_yaw {
            while yaw - previous > std::f32::consts::PI {
                yaw -= std::f32::consts::TAU;
            }
            while yaw - previous < -std::f32::consts::PI {
                yaw += std::f32::consts::TAU;
            }
        }
        previous_yaw = Some(yaw);
        samples.push([translation[0], translation[1], translation[2], yaw]);
    }
    Ok(samples)
}

fn sample_linear_track(
    track: &[SkyrimTimedMotionSample],
    index: usize,
    sample_count: usize,
    duration: f32,
    fallback: [f32; 4],
) -> [f32; 4] {
    if track.is_empty() {
        return fallback;
    }
    if track.len() == sample_count {
        return track[index].value;
    }
    let target_time = sample_time(index, sample_count, duration);
    let (left, right, fraction) = sample_interval(track, target_time);
    std::array::from_fn(|component| {
        left.value[component] + fraction * (right.value[component] - left.value[component])
    })
}

fn sample_quaternion_track(
    track: &[SkyrimTimedMotionSample],
    index: usize,
    sample_count: usize,
    duration: f32,
) -> [f32; 4] {
    if track.is_empty() {
        return [0.0, 0.0, 0.0, 1.0];
    }
    if track.len() == sample_count {
        return normalize_quaternion(track[index].value);
    }
    let target_time = sample_time(index, sample_count, duration);
    let (left, right, fraction) = sample_interval(track, target_time);
    slerp_quaternion(left.value, right.value, fraction)
}

fn sample_time(index: usize, sample_count: usize, duration: f32) -> f32 {
    if sample_count <= 1 {
        0.0
    } else {
        duration * index as f32 / (sample_count - 1) as f32
    }
}

fn sample_interval(
    track: &[SkyrimTimedMotionSample],
    target_time: f32,
) -> (SkyrimTimedMotionSample, SkyrimTimedMotionSample, f32) {
    if target_time <= track[0].time {
        return (track[0], track[0], 0.0);
    }
    for pair in track.windows(2) {
        if target_time <= pair[1].time {
            let span = pair[1].time - pair[0].time;
            let fraction = if span.abs() <= f32::EPSILON {
                0.0
            } else {
                ((target_time - pair[0].time) / span).clamp(0.0, 1.0)
            };
            return (pair[0], pair[1], fraction);
        }
    }
    let last = *track.last().expect("non-empty track");
    (last, last, 0.0)
}

fn normalize_quaternion(value: [f32; 4]) -> [f32; 4] {
    let length = value
        .iter()
        .map(|component| component * component)
        .sum::<f32>()
        .sqrt();
    if length <= f32::EPSILON || !length.is_finite() {
        [0.0, 0.0, 0.0, 1.0]
    } else {
        value.map(|component| component / length)
    }
}

fn slerp_quaternion(left: [f32; 4], right: [f32; 4], fraction: f32) -> [f32; 4] {
    let left = normalize_quaternion(left);
    let mut right = normalize_quaternion(right);
    let mut dot = left
        .iter()
        .zip(right)
        .map(|(left, right)| left * right)
        .sum::<f32>();
    if dot < 0.0 {
        right = right.map(|component| -component);
        dot = -dot;
    }
    if dot > 0.9995 {
        return normalize_quaternion(std::array::from_fn(|component| {
            left[component] + fraction * (right[component] - left[component])
        }));
    }
    let angle = dot.clamp(-1.0, 1.0).acos();
    let denominator = angle.sin();
    if denominator.abs() <= f32::EPSILON {
        return left;
    }
    let left_weight = ((1.0 - fraction) * angle).sin() / denominator;
    let right_weight = (fraction * angle).sin() / denominator;
    normalize_quaternion(std::array::from_fn(|component| {
        left_weight * left[component] + right_weight * right[component]
    }))
}

fn quaternion_yaw(value: [f32; 4]) -> f32 {
    let [x, y, z, w] = normalize_quaternion(value);
    (2.0 * (w * z + x * y)).atan2(1.0 - 2.0 * (y * y + z * z))
}

fn animation_binding(bytes: &[u8]) -> Result<AnimationBindingEvidence, String> {
    let file = HkxFile::read(bytes).map_err(|error| format!("decode animation HKX: {error}"))?;
    let bindings = file
        .objects()
        .iter()
        .filter(|object| object.class_name == "hkaAnimationBinding")
        .collect::<Vec<_>>();
    let [binding] = bindings.as_slice() else {
        return Err(format!(
            "animation contains {} hkaAnimationBinding objects",
            bindings.len()
        ));
    };
    let original_skeleton_name = binding
        .members
        .iter()
        .find(|member| member.name == "originalSkeletonName")
        .and_then(|member| match &member.value {
            HkxValue::String {
                value,
                is_null: false,
            } if !value.trim().is_empty() => Some(value.clone()),
            _ => None,
        })
        .ok_or_else(|| "animation binding has no exact originalSkeletonName".to_string())?;
    let mut transform_indices = hkx_usize_array(binding, "transformTrackToBoneIndices")?;
    let mut float_indices = hkx_usize_array(binding, "floatTrackToFloatSlotIndices")?;
    let animation = file
        .objects()
        .iter()
        .find(|object| object.class_name.ends_with("Animation"))
        .ok_or_else(|| "animation HKX has no hka*Animation object".to_string())?;
    let transform_tracks =
        hkx_usize_member(animation, "numberOfTransformTracks").unwrap_or(transform_indices.len());
    let float_tracks =
        hkx_usize_member(animation, "numberOfFloatTracks").unwrap_or(float_indices.len());
    if transform_indices.is_empty() && transform_tracks > 0 {
        transform_indices = (0..transform_tracks).collect();
    }
    if float_indices.is_empty() && float_tracks > 0 {
        float_indices = (0..float_tracks).collect();
    }
    if transform_indices.len() != transform_tracks || float_indices.len() != float_tracks {
        return Err(format!(
            "animation binding track cardinality mismatch transform={}/{} float={}/{}",
            transform_indices.len(),
            transform_tracks,
            float_indices.len(),
            float_tracks
        ));
    }
    Ok(AnimationBindingEvidence {
        original_skeleton_name,
        transform_tracks,
        transform_indices,
        float_tracks,
        float_indices,
    })
}

fn hkx_usize_array(
    object: &havok_native::hkx::model::HkxObject,
    name: &str,
) -> Result<Vec<usize>, String> {
    let Some(value) = object
        .members
        .iter()
        .find(|member| member.name == name)
        .map(|member| &member.value)
    else {
        return Ok(Vec::new());
    };
    let HkxValue::Array(values) = value else {
        return Err(format!("{name} is not an array"));
    };
    values
        .iter()
        .map(|value| {
            hkx_integer(value)
                .and_then(|value| usize::try_from(value).ok())
                .ok_or_else(|| format!("{name} contains a negative/non-integer index"))
        })
        .collect()
}

fn hkx_usize_member(object: &havok_native::hkx::model::HkxObject, name: &str) -> Option<usize> {
    object
        .members
        .iter()
        .find(|member| member.name == name)
        .and_then(|member| hkx_integer(&member.value))
        .and_then(|value| usize::try_from(value).ok())
}

fn hkx_integer(value: &HkxValue) -> Option<i64> {
    match value {
        HkxValue::I8(value) => Some(i64::from(*value)),
        HkxValue::U8(value) => Some(i64::from(*value)),
        HkxValue::I16(value) => Some(i64::from(*value)),
        HkxValue::U16(value) => Some(i64::from(*value)),
        HkxValue::I32(value) => Some(i64::from(*value)),
        HkxValue::U32(value) => Some(i64::from(*value)),
        HkxValue::I64(value) => Some(*value),
        HkxValue::U64(value) => i64::try_from(*value).ok(),
        _ => None,
    }
}

fn bind_clip_to_skeleton(
    binding: AnimationBindingEvidence,
    skeleton: &crate::phase::skeleton::SourceRigSkeletonArtifactReceipt,
    skeleton_runtime_path: &str,
) -> Result<ClipBinding, String> {
    if binding.original_skeleton_name != skeleton.skeleton_name {
        return Err(format!(
            "clip originalSkeletonName {:?} differs from reconstructed skeleton {:?}",
            binding.original_skeleton_name, skeleton.skeleton_name
        ));
    }
    if binding
        .transform_indices
        .iter()
        .any(|index| *index >= skeleton.ordered_bone_names.len())
    {
        return Err("clip transform track maps outside the reconstructed skeleton".to_string());
    }
    if binding
        .float_indices
        .iter()
        .any(|index| *index >= skeleton.float_slot_names.len())
    {
        return Err("clip float track maps outside the reconstructed float slots".to_string());
    }
    Ok(ClipBinding {
        skeleton_path: skeleton_runtime_path.to_string(),
        original_skeleton_name: binding.original_skeleton_name,
        declared_transform_tracks: binding.transform_tracks,
        transform_track_to_bone_indices: binding.transform_indices,
        declared_float_tracks: binding.float_tracks,
        float_track_to_float_slot_indices: binding.float_indices,
    })
}

fn controller_decl_from_receipt(
    receipt: &SkyrimCreatureControllerReceipt,
) -> Result<CreatureControllerDecl, String> {
    const FO4_INVALID_RIGID_BODY_TYPE: i32 = u8::MAX as i32;

    let model = receipt
        .model
        .as_ref()
        .ok_or_else(|| "controller receipt has no model basis".to_string())?;
    let rigid_body_type = match receipt.rigid_body_type {
        Some(value) => value,
        None if matches!(
            receipt.layout,
            SkyrimControllerLayoutEvidence::LegacyCharacterControllerInfo { .. }
        ) =>
        {
            FO4_INVALID_RIGID_BODY_TYPE
        }
        None => return Err("controller receipt has no rigid body type".to_string()),
    };
    Ok(CreatureControllerDecl {
        collision_filter_info: receipt
            .collision_filter_info
            .ok_or_else(|| "controller receipt has no collision filter".to_string())?,
        rigid_body_type,
        model_up_ms: model.up_ms,
        model_forward_ms: model.forward_ms,
        model_right_ms: model.right_ms,
        model_scale: model.scale,
    })
}

fn clip_is_semantically_looping(
    graph: &crate::source_rig::CapabilityGraphManifest,
    clip_id: &str,
) -> bool {
    graph.roles.iter().any(|role| {
        role.generator
            .clip_names(&role.clip_name)
            .iter()
            .any(|name| *name == clip_id)
            && matches!(
                role.role,
                crate::source_rig::CreatureClipRole::Idle
                    | crate::source_rig::CreatureClipRole::GroundForward
                    | crate::source_rig::CreatureClipRole::SwimIdle
                    | crate::source_rig::CreatureClipRole::SwimForward
                    | crate::source_rig::CreatureClipRole::FlyIdle
                    | crate::source_rig::CreatureClipRole::FlyForward
                    | crate::source_rig::CreatureClipRole::StationaryIdle
                    | crate::source_rig::CreatureClipRole::ContinuousAttackLoop
            )
    })
}

#[allow(clippy::too_many_arguments)]
fn stage_skyrim_converted_artifact(
    converted_root: &Path,
    role: SourceRigArtifactRole,
    provenance: SourceRigArtifactProvenance,
    runtime_path: &str,
    source_evidence_blake3: String,
    bytes: &[u8],
    requests: &mut Vec<SourceRigConvertedArtifactRequestReceipt>,
    receipts: &mut Vec<SourceRigArtifactReceipt>,
    artifacts: &mut Vec<SkyrimCreatureConvertedArtifact>,
) -> Result<(), String> {
    if runtime_path
        .split(['\\', '/'])
        .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return Err(format!("invalid converted runtime path {runtime_path}"));
    }
    let path = runtime_path
        .split('\\')
        .fold(converted_root.to_path_buf(), |path, part| path.join(part));
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("create converted artifact directory: {error}"))?;
    }
    fs::write(&path, bytes)
        .map_err(|error| format!("write converted artifact {}: {error}", path.display()))?;
    let request_bytes = serde_json::to_vec(&(
        &role,
        runtime_path,
        "skyrimse",
        &source_evidence_blake3,
        "skyrim_source_owned_conversion_v1",
    ))
    .map_err(|error| format!("serialize converted artifact request: {error}"))?;
    requests.push(SourceRigConvertedArtifactRequestReceipt {
        role: role.clone(),
        runtime_path: runtime_path.to_string(),
        source_game: "skyrimse".to_string(),
        source_evidence_blake3,
        conversion_request_blake3: blake3::hash(&request_bytes).to_hex().to_string(),
    });
    receipts.push(SourceRigArtifactReceipt {
        role,
        provenance,
        runtime_path: runtime_path.to_string(),
        byte_len: bytes.len() as u64,
        blake3: blake3::hash(bytes).to_hex().to_string(),
    });
    artifacts.push(SkyrimCreatureConvertedArtifact {
        runtime_path: runtime_path.to_string(),
        source_path: path,
    });
    Ok(())
}

fn exact_asset_hash(
    family: &SkyrimCreatureFamilyJob,
    path: &str,
    role: SkyrimAssetRole,
) -> Result<String, String> {
    let claims = family
        .asset_claims
        .iter()
        .filter(|claim| claim.role == role && same_runtime_path(&claim.path, path))
        .collect::<Vec<_>>();
    match claims.as_slice() {
        [claim] => match &claim.disposition {
            SkyrimAssetClaimDisposition::Present { blake3 } if blake3.len() == 64 => {
                Ok(blake3.clone())
            }
            disposition => Err(format!("asset {path} is not hash-bound: {disposition:?}")),
        },
        _ => Err(format!(
            "expected one exact {role:?} asset claim for {path}, got {}",
            claims.len()
        )),
    }
}

#[derive(Clone, Copy)]
struct SkyrimNpcRuntimeStats {
    level: u16,
    health: u16,
    action_points: u16,
}

fn skyrim_npc_runtime_stats(
    record: &Record,
    interner: &StringInterner,
) -> Result<SkyrimNpcRuntimeStats, String> {
    let acbs = record
        .fields
        .iter()
        .find(|field| field.sig.as_str() == "ACBS")
        .ok_or_else(|| "NPC_ has no ACBS".to_string())?;
    let (stamina_offset, level, health_offset, template_flags) = match &acbs.value {
        crate::record::FieldValue::Bytes(bytes) if bytes.len() == 24 => (
            i16::from_le_bytes(bytes[6..8].try_into().unwrap()),
            u16::from_le_bytes(bytes[8..10].try_into().unwrap()),
            i16::from_le_bytes(bytes[20..22].try_into().unwrap()),
            u16::from_le_bytes(bytes[18..20].try_into().unwrap()),
        ),
        crate::record::FieldValue::Struct(fields) => (
            struct_i64(fields, "stamina_offset", interner)
                .and_then(|value| i16::try_from(value).ok())
                .ok_or_else(|| "NPC_.ACBS has no stamina_offset".to_string())?,
            struct_i64(fields, "level", interner)
                .and_then(|value| u16::try_from(value).ok())
                .ok_or_else(|| "NPC_.ACBS has no level".to_string())?,
            struct_i64(fields, "health_offset", interner)
                .and_then(|value| i16::try_from(value).ok())
                .ok_or_else(|| "NPC_.ACBS has no health_offset".to_string())?,
            struct_i64(fields, "template_flags", interner)
                .and_then(|value| u16::try_from(value).ok())
                .ok_or_else(|| "NPC_.ACBS has no template_flags".to_string())?,
        ),
        _ => return Err("NPC_.ACBS is not the exact 24-byte Skyrim layout".to_string()),
    };
    if template_flags & 0x0002 != 0 {
        return Err(format!(
            "NPC_ uses template flags {template_flags:04X}; exact inherited stat flattening is required"
        ));
    }
    let derive = |offset: i16| (50_i32 + i32::from(offset)).clamp(1, i32::from(u16::MAX)) as u16;
    Ok(SkyrimNpcRuntimeStats {
        level: level.max(1),
        health: derive(health_offset),
        action_points: derive(stamina_offset),
    })
}

fn effective_npc_template_record<'a>(
    record: &'a Record,
    inherited_flag: u16,
    records: &HashMap<FormKey, &'a Record>,
    interner: &StringInterner,
) -> Result<&'a Record, String> {
    let mut current = record;
    let mut visited = HashSet::new();
    loop {
        if !visited.insert(current.form_key) {
            return Err("NPC_ template chain contains a cycle".to_string());
        }
        let flags = skyrim_npc_template_flags(current, interner)?;
        if flags & inherited_flag == 0 {
            return Ok(current);
        }
        let target = current
            .fields
            .iter()
            .find(|field| field.sig.as_str() == "TPLT")
            .and_then(|field| first_form_key(&field.value))
            .ok_or_else(|| {
                format!(
                    "NPC_ inherits template category {inherited_flag:04X} without a TPLT target"
                )
            })?;
        current = resolve_npc_template_target(target, records, interner)?;
    }
}

fn resolve_npc_template_target<'a>(
    target: FormKey,
    records: &HashMap<FormKey, &'a Record>,
    interner: &StringInterner,
) -> Result<&'a Record, String> {
    let record = records.get(&target).copied().ok_or_else(|| {
        format!(
            "NPC_ template target {} is missing",
            target.format(interner)
        )
    })?;
    match record.sig.as_str() {
        "NPC_" => Ok(record),
        "LVLN" => {
            let mut targets = Vec::new();
            collect_lvln_npc_targets(record, records, &mut HashSet::new(), &mut targets);
            targets.sort_by_key(|target| form_key_sort_key(*target, interner));
            targets.dedup();
            let target = targets.first().copied().ok_or_else(|| {
                format!(
                    "LVLN template target {} has no decoded NPC_ entries",
                    target.format(interner)
                )
            })?;
            records
                .get(&target)
                .copied()
                .ok_or_else(|| "selected LVLN NPC_ template is missing".to_string())
        }
        signature => Err(format!(
            "NPC_ template target {} has unsupported signature {signature}",
            target.format(interner)
        )),
    }
}

fn collect_lvln_npc_targets(
    leveled_list: &Record,
    records: &HashMap<FormKey, &Record>,
    visited: &mut HashSet<FormKey>,
    output: &mut Vec<FormKey>,
) {
    if !visited.insert(leveled_list.form_key) {
        return;
    }
    for field in leveled_list
        .fields
        .iter()
        .filter(|field| field.sig.as_str() == "LVLO")
    {
        let mut targets = collect_form_keys(&field.value);
        if targets.is_empty() {
            targets.extend(
                embedded_form_ids(&field.value, 4, 12)
                    .into_iter()
                    .filter_map(|raw| {
                        resolve_embedded_form_id(leveled_list.form_key, raw, records)
                    }),
            );
        }
        for target in targets {
            match records.get(&target).copied() {
                Some(record) if record.sig.as_str() == "NPC_" => output.push(target),
                Some(record) if record.sig.as_str() == "LVLN" => {
                    collect_lvln_npc_targets(record, records, visited, output);
                }
                _ => {}
            }
        }
    }
}

fn skyrim_npc_template_flags(record: &Record, interner: &StringInterner) -> Result<u16, String> {
    let acbs = record
        .fields
        .iter()
        .find(|field| field.sig.as_str() == "ACBS")
        .ok_or_else(|| "NPC_ has no ACBS".to_string())?;
    match &acbs.value {
        crate::record::FieldValue::Bytes(bytes) if bytes.len() == 24 => {
            Ok(u16::from_le_bytes(bytes[18..20].try_into().unwrap()))
        }
        crate::record::FieldValue::Struct(fields) => struct_i64(fields, "template_flags", interner)
            .and_then(|value| u16::try_from(value).ok())
            .ok_or_else(|| "NPC_.ACBS has no template_flags".to_string()),
        _ => Err("NPC_.ACBS is not the exact 24-byte Skyrim layout".to_string()),
    }
}

fn first_form_key(value: &crate::record::FieldValue) -> Option<FormKey> {
    match value {
        crate::record::FieldValue::FormKey(value) if value.local != 0 => Some(*value),
        crate::record::FieldValue::List(values) => values.iter().find_map(first_form_key),
        crate::record::FieldValue::Struct(fields) => {
            fields.iter().find_map(|(_, value)| first_form_key(value))
        }
        _ => None,
    }
}

fn collect_form_keys(value: &crate::record::FieldValue) -> Vec<FormKey> {
    let mut output = Vec::new();
    match value {
        crate::record::FieldValue::FormKey(value) if value.local != 0 => output.push(*value),
        crate::record::FieldValue::List(values) => {
            for value in values {
                output.extend(collect_form_keys(value));
            }
        }
        crate::record::FieldValue::Struct(fields) => {
            for (_, value) in fields {
                output.extend(collect_form_keys(value));
            }
        }
        _ => {}
    }
    output
}

fn struct_i64(
    fields: &[(crate::sym::Sym, crate::record::FieldValue)],
    name: &str,
    interner: &StringInterner,
) -> Option<i64> {
    fields
        .iter()
        .find(|(field, _)| interner.resolve(*field) == Some(name))
        .and_then(|(_, value)| field_i64(value))
}

fn field_i64(value: &crate::record::FieldValue) -> Option<i64> {
    match value {
        crate::record::FieldValue::Int(value) => Some(*value),
        crate::record::FieldValue::Uint(value) => i64::try_from(*value).ok(),
        _ => None,
    }
}

fn melee_record_key_plan(
    attacks: &[CreatureAttackRecordVariant],
) -> Vec<CreatureMeleeRecordKeyPlan> {
    attacks
        .iter()
        .filter_map(|attack| match &attack.projection {
            CreatureAttackRecordProjection::MeleeUnarmed {
                weapon_form_key, ..
            } => Some(CreatureMeleeRecordKeyPlan {
                attack_id: attack.id.clone(),
                form_key: weapon_form_key.clone(),
                primary: attack.primary,
            }),
            _ => None,
        })
        .collect()
}

fn base_unarmed_weapon_identity(
    attacks: &[CreatureAttackRecordVariant],
    target_plugin: &str,
    placeholder_editor_id: &str,
) -> (TargetFormKey, String) {
    attacks
        .iter()
        .find_map(|attack| match &attack.projection {
            CreatureAttackRecordProjection::MeleeUnarmed {
                weapon_form_key,
                weapon_editor_id,
                ..
            } => Some((weapon_form_key.clone(), weapon_editor_id.clone())),
            _ => None,
        })
        .unwrap_or_else(|| {
            (
                TargetFormKey::new(0, target_plugin),
                placeholder_editor_id.to_string(),
            )
        })
}

fn live_attack_projections(
    candidate: &super::creature_recipe::SkyrimCreatureCandidateRecipe,
    family: &SkyrimCreatureFamilyJob,
    dependency_projections: &[SkyrimCreatureTargetDependencyProjection],
    reservations: &HashMap<FormKey, &SkyrimCreatureRecordReservation>,
    ancillary_reservations: &[SkyrimCreatureAncillaryReservationReceipt],
    fallback_spell: Option<&CreatureTargetRecordReference>,
    target_plugin: &str,
    editor_stem: &str,
    interner: &StringInterner,
) -> Result<Vec<CreatureAttackRecordVariant>, String> {
    if family.graph.template == CreatureGraphTemplate::PassiveGround {
        if candidate.attack_events.is_empty()
            && candidate.attack_contract.is_empty()
            && candidate.attack_spells.is_empty()
            && !family.graph.roles.iter().any(|role| {
                matches!(
                    role.role,
                    crate::source_rig::CreatureClipRole::MeleeAttack
                        | crate::source_rig::CreatureClipRole::ProjectileAttack
                        | crate::source_rig::CreatureClipRole::ContinuousAttackStart
                        | crate::source_rig::CreatureClipRole::ContinuousAttackLoop
                        | crate::source_rig::CreatureClipRole::ContinuousAttackStop
                )
            })
        {
            return Ok(Vec::new());
        }
        return Err(
            "PassiveGround candidate has contradictory source attack semantics".to_string(),
        );
    }
    if candidate.attack_data.is_empty() {
        let source_race = parse_form_key(&candidate.source_race, interner)
            .ok_or_else(|| format!("invalid candidate RACE locator {}", candidate.source_race))?;
        let source_plugin = interner
            .resolve(source_race.plugin)
            .ok_or_else(|| "candidate RACE plugin is unavailable".to_string())?;
        let melee_event = synthetic_melee_attack_event(
            source_plugin,
            source_race.local,
            &candidate.attack_events,
            &candidate.attack_contract,
            &family.graph.roles,
            &family.graph.explicit_events,
            &family.graph.candidate_attack_bindings,
        );
        if melee_event.is_none()
            && let Some(event) = synthetic_projectile_attack_event(
                &candidate.attack_events,
                &candidate.attack_contract,
                &family.graph.roles,
                &family.graph.explicit_events,
            )
        {
            let projection = fallback_spell
                .cloned()
                .map(|spell| CreatureAttackRecordProjection::SpellAbility { spell })
                .unwrap_or_else(fo4_creature_ranged_fallback_projection);
            return Ok(vec![CreatureAttackRecordVariant {
                id: "attack_00".to_string(),
                event,
                primary: true,
                projection,
                damage_multiplier: 1.0,
                chance: 1.0,
                strike_angle: 0.0,
                action_point_cost: 0.0,
                target_data: CreatureAttackTargetData::default(),
            }]);
        }
        let event = melee_event.ok_or_else(|| {
            format!(
                "active candidate has no source-backed melee event or mapped primary NPC spell; template={:?} roles={:?}",
                family.graph.template,
                family
                    .graph
                    .roles
                    .iter()
                    .map(|role| (&role.role, &role.trigger_event, &role.trigger_aliases))
                    .collect::<Vec<_>>()
            )
        })?;
        let weapon_form_key = ancillary_target(
            ancillary_reservations,
            source_race,
            &event,
            0,
            SkyrimCreatureAncillaryRecordKind::Weapon,
            target_plugin,
            interner,
        )?;
        return Ok(vec![CreatureAttackRecordVariant {
            id: "attack_00".to_string(),
            event,
            primary: true,
            projection: CreatureAttackRecordProjection::MeleeUnarmed {
                weapon_form_key,
                weapon_editor_id: format!("{editor_stem}Attack_00"),
                damage: 10,
                reach: 1.0,
                attack_seconds: 0.5,
            },
            damage_multiplier: 1.0,
            chance: 1.0,
            strike_angle: 0.0,
            action_point_cost: 0.0,
            target_data: CreatureAttackTargetData {
                recovery_time: 0.5,
                ..CreatureAttackTargetData::default()
            },
        }]);
    }
    candidate
        .attack_data
        .iter()
        .enumerate()
        .map(|(index, attack)| {
            validate_attack_target_values(attack)?;
            let (projection, spell_policy, preserve_source_receipt) = match &attack.attack_spell {
                Some(spell) if spell.signature == "SPEL" => {
                    let source = parse_form_key(&spell.source, interner).ok_or_else(|| {
                        format!(
                            "ATKD[{}] has invalid SPEL locator {}",
                            attack.ordinal, spell.source
                        )
                    })?;
                    if dependency_projections
                        .iter()
                        .any(|projection| projection.source.form_key == source)
                    {
                        let target = target_reference_for_source(
                            source,
                            "SPEL",
                            dependency_projections,
                            reservations,
                            target_plugin,
                        )?;
                        (
                            CreatureAttackRecordProjection::SpellAbility { spell: target },
                            CreatureAttackSpellPolicy::DirectSpell,
                            true,
                        )
                    } else {
                        (
                            graph_compatible_attack_fallback_projection(
                                candidate,
                                family,
                                attack,
                                ancillary_reservations,
                                target_plugin,
                                editor_stem,
                                interner,
                            )?,
                            CreatureAttackSpellPolicy::NoSourceSpell,
                            false,
                        )
                    }
                }
                Some(spell) if spell.signature == "SHOU" => (
                    graph_compatible_attack_fallback_projection(
                        candidate,
                        family,
                        attack,
                        ancillary_reservations,
                        target_plugin,
                        editor_stem,
                        interner,
                    )?,
                    CreatureAttackSpellPolicy::NoSourceSpell,
                    false,
                ),
                Some(spell) => {
                    return Err(format!(
                        "ATKD[{}] event {:?} references unsupported {} {}",
                        attack.ordinal, attack.event, spell.signature, spell.source
                    ));
                }
                None => (
                    generated_unarmed_attack_projection(
                        candidate,
                        attack,
                        ancillary_reservations,
                        target_plugin,
                        editor_stem,
                        interner,
                    )?,
                    CreatureAttackSpellPolicy::NoSourceSpell,
                    true,
                ),
            };
            let source_atkd = preserve_source_receipt
                .then(|| attack_source_data_receipt(attack, spell_policy, interner))
                .transpose()?;
            Ok(CreatureAttackRecordVariant {
                id: format!("attack_{index:02}"),
                event: attack.event.clone(),
                primary: index == 0,
                projection,
                damage_multiplier: f32::from_bits(attack.damage_multiplier_bits),
                chance: f32::from_bits(attack.attack_chance_bits),
                strike_angle: f32::from_bits(attack.strike_angle_bits),
                action_point_cost: 0.0,
                target_data: CreatureAttackTargetData {
                    attack_flags: attack.attack_flags,
                    attack_angle: f32::from_bits(attack.attack_angle_bits),
                    stagger: f32::from_bits(attack.stagger_bits),
                    knockdown: f32::from_bits(attack.knockdown_bits),
                    recovery_time: f32::from_bits(attack.recovery_time_bits),
                    action_points_multiplier: f32::from_bits(attack.stamina_multiplier_bits),
                    stagger_offset: 0,
                    source_atkd,
                },
            })
        })
        .collect()
}

fn fo4_creature_ranged_fallback_projection() -> CreatureAttackRecordProjection {
    CreatureAttackRecordProjection::RangedEquipment {
        equipment: vec![CreatureTargetRecordReference::new(
            "WEAP",
            TargetFormKey::new(FO4_CREATURE_RANGED_FALLBACK_WEAPON, "Fallout4.esm"),
        )],
        projectile: None,
        ammunition: None,
    }
}

fn graph_compatible_attack_fallback_projection(
    candidate: &super::creature_recipe::SkyrimCreatureCandidateRecipe,
    family: &SkyrimCreatureFamilyJob,
    attack: &SkyrimRaceAttackDataReceipt,
    ancillary_reservations: &[SkyrimCreatureAncillaryReservationReceipt],
    target_plugin: &str,
    editor_stem: &str,
    interner: &StringInterner,
) -> Result<CreatureAttackRecordProjection, String> {
    let role =
        graph_attack_role_for_event(&family.graph.roles, &attack.event).ok_or_else(|| {
            format!(
                "ATKD[{}] event {:?} has no executable graph role",
                attack.ordinal, attack.event
            )
        })?;
    match role {
        crate::source_rig::CreatureClipRole::ProjectileAttack => {
            Ok(fo4_creature_ranged_fallback_projection())
        }
        crate::source_rig::CreatureClipRole::MeleeAttack => generated_unarmed_attack_projection(
            candidate,
            attack,
            ancillary_reservations,
            target_plugin,
            editor_stem,
            interner,
        ),
        _ => Err(format!(
            "ATKD[{}] event {:?} resolves to unsupported attack role {role:?}",
            attack.ordinal, attack.event
        )),
    }
}

fn graph_attack_role_for_event(
    roles: &[crate::source_rig::CapabilityClipRole],
    event: &str,
) -> Option<crate::source_rig::CreatureClipRole> {
    roles
        .iter()
        .find(|role| {
            role.trigger_event
                .iter()
                .chain(&role.trigger_aliases)
                .any(|candidate| candidate.eq_ignore_ascii_case(event))
        })
        .map(|role| role.role)
}

fn generated_unarmed_attack_projection(
    candidate: &super::creature_recipe::SkyrimCreatureCandidateRecipe,
    attack: &SkyrimRaceAttackDataReceipt,
    ancillary_reservations: &[SkyrimCreatureAncillaryReservationReceipt],
    target_plugin: &str,
    editor_stem: &str,
    interner: &StringInterner,
) -> Result<CreatureAttackRecordProjection, String> {
    let source_race = parse_form_key(&candidate.source_race, interner)
        .ok_or_else(|| format!("invalid candidate RACE locator {}", candidate.source_race))?;
    let weapon_form_key = ancillary_target(
        ancillary_reservations,
        source_race,
        &attack.event,
        attack.ordinal,
        SkyrimCreatureAncillaryRecordKind::Weapon,
        target_plugin,
        interner,
    )?;
    let damage_multiplier = f32::from_bits(attack.damage_multiplier_bits).abs();
    let damage = (damage_multiplier * 10.0)
        .round()
        .clamp(1.0, u16::MAX as f32) as u16;
    let recovery_time = f32::from_bits(attack.recovery_time_bits);
    let attack_seconds = if recovery_time > 0.0 {
        recovery_time.max(0.1)
    } else {
        0.5
    };
    Ok(CreatureAttackRecordProjection::MeleeUnarmed {
        weapon_form_key,
        weapon_editor_id: format!("{editor_stem}Attack_{:02}", attack.ordinal),
        damage,
        reach: 1.0,
        attack_seconds,
    })
}

fn validate_attack_target_values(attack: &SkyrimRaceAttackDataReceipt) -> Result<(), String> {
    for (field, value) in [
        ("damage_multiplier", attack.damage_multiplier_bits),
        ("attack_chance", attack.attack_chance_bits),
        ("attack_angle", attack.attack_angle_bits),
        ("strike_angle", attack.strike_angle_bits),
        ("stagger", attack.stagger_bits),
        ("knockdown", attack.knockdown_bits),
        ("recovery_time", attack.recovery_time_bits),
        ("stamina_multiplier", attack.stamina_multiplier_bits),
    ] {
        if !f32::from_bits(value).is_finite() {
            return Err(format!(
                "ATKD[{}] event {:?} has non-finite {field}",
                attack.ordinal, attack.event
            ));
        }
    }
    Ok(())
}

fn attack_source_data_receipt(
    attack: &SkyrimRaceAttackDataReceipt,
    spell_policy: CreatureAttackSpellPolicy,
    interner: &StringInterner,
) -> Result<CreatureAttackSourceDataReceipt, String> {
    let source_reference = |
        reference: &super::creature_recipe::SkyrimRaceAttackFormReceipt,
    |
     -> Result<CreatureSourceRecordReference, String> {
        Ok(CreatureSourceRecordReference {
            source_identity: source_identity(
                parse_form_key(&reference.source, interner).ok_or_else(|| {
                    format!(
                        "ATKD[{}] event {:?} has invalid {} locator {}",
                        attack.ordinal, attack.event, reference.signature, reference.source
                    )
                })?,
                interner,
            )?,
            signature: reference.signature.clone(),
        })
    };
    let attack_type = attack
        .attack_type
        .as_ref()
        .map(source_reference)
        .transpose()?;
    let attack_spell = attack
        .attack_spell
        .as_ref()
        .map(source_reference)
        .transpose()?;
    let proof = |schema: &str,
                 field: &str,
                 policy: &str,
                 evidence: String,
                 extra: Option<(&str, serde_json::Value)>| {
        let mut value = serde_json::Map::new();
        value.insert(
            "evidence_blake3".to_string(),
            serde_json::Value::String(evidence),
        );
        value.insert(
            "field".to_string(),
            serde_json::Value::String(field.to_string()),
        );
        value.insert(
            "policy".to_string(),
            serde_json::Value::String(policy.to_string()),
        );
        if let Some((key, extra_value)) = extra {
            value.insert(key.to_string(), extra_value);
        }
        let canonical_json = serde_json::to_string(&serde_json::Value::Object(value))
            .expect("semantic proof JSON is serializable");
        CreatureSemanticProofReceipt {
            schema: schema.to_string(),
            canonical_json_blake3: blake3::hash(canonical_json.as_bytes()).to_hex().to_string(),
            canonical_json,
        }
    };
    let evidence = |field: &str| {
        blake3::hash(
            format!(
                "skyrimse-creature-atkd-v1:{}:{}:{}:{field}",
                attack.ordinal, attack.event, attack.damage_multiplier_bits
            )
            .as_bytes(),
        )
        .to_hex()
        .to_string()
    };
    Ok(CreatureAttackSourceDataReceipt {
        schema_id: "skyrimse-race-atkd-v1".to_string(),
        damage_multiplier_bits: attack.damage_multiplier_bits,
        chance_bits: attack.attack_chance_bits,
        attack_spell,
        attack_flags: attack.attack_flags,
        attack_angle_bits: attack.attack_angle_bits,
        strike_angle_bits: attack.strike_angle_bits,
        stagger_bits: attack.stagger_bits,
        attack_type: attack_type.clone(),
        knockdown_bits: attack.knockdown_bits,
        recovery_time_bits: attack.recovery_time_bits,
        stamina_multiplier_bits: attack.stamina_multiplier_bits,
        event: attack.event.clone(),
        ordinal: u32::try_from(attack.ordinal)
            .map_err(|_| format!("ATKD ordinal {} exceeds u32", attack.ordinal))?,
        attack_type_policy: match attack_type {
            Some(_) => CreatureAttackTypePolicy::RuntimeInertKeyword {
                proof: proof(
                    ATTACK_TYPE_RUNTIME_INERT_PROOF_SCHEMA,
                    "attack_type",
                    "runtime_inert_keyword",
                    evidence("attack_type"),
                    Some((
                        "source_signature",
                        serde_json::Value::String("KYWD".to_string()),
                    )),
                ),
            },
            None => CreatureAttackTypePolicy::NullOmitted,
        },
        attack_spell_policy: spell_policy,
        stamina_multiplier_policy: CreatureAttackStaminaPolicy::PreserveAsActionPointsMultiplier {
            proof: proof(
                ATTACK_STAMINA_TO_ACTION_POINTS_PROOF_SCHEMA,
                "stamina_multiplier",
                "preserve_as_action_points_multiplier",
                evidence("stamina_multiplier"),
                Some((
                    "relation",
                    serde_json::Value::String("bit_identical".to_string()),
                )),
            ),
        },
        stagger_offset_policy: CreatureAttackStaggerOffsetPolicy::ExplicitZero {
            proof: proof(
                ATTACK_STAGGER_OFFSET_ZERO_PROOF_SCHEMA,
                "stagger_offset",
                "explicit_zero",
                evidence("stagger_offset"),
                Some(("target_value", serde_json::Value::from(0))),
            ),
        },
    })
}

fn target_dependency_projections(
    candidate: &super::creature_mvp_adapter::SkyrimCreatureDependencyCandidate,
    dependency: &SkyrimCreatureDependencyLedger,
    reservations: &HashMap<FormKey, &SkyrimCreatureRecordReservation>,
    mapper_state: &MapperState,
    target_plugin: &str,
    interner: &StringInterner,
) -> Result<Vec<SkyrimCreatureTargetDependencyProjection>, String> {
    let records = dependency
        .records
        .iter()
        .map(|record| (record.source.form_key, record))
        .collect::<HashMap<_, _>>();
    let mut projections = Vec::new();
    for source in &candidate.closure {
        let record = records
            .get(source)
            .copied()
            .ok_or_else(|| format!("dependency closure has no receipt for {source:?}"))?;
        let kind = match record.disposition {
            SkyrimRecordLoweringDisposition::Ready { kind } => kind,
            SkyrimRecordLoweringDisposition::Blocked { .. } => {
                return Err(format!("dependency {} is blocked", record.source.signature));
            }
        };
        if kind == SkyrimRecordLoweringKind::SourceRigProjection
            && reservations.contains_key(source)
        {
            continue;
        }
        let target = mapper_state
            .source_to_target
            .get(source)
            .copied()
            .ok_or_else(|| {
                format!(
                    "translated dependency {} has no target FormKey mapping",
                    record.source.signature
                )
            })?;
        let plugin = if target.plugin == source.plugin {
            if target_plugin.trim().is_empty() {
                return Err("target output plugin is unavailable".to_string());
            }
            target_plugin.to_string()
        } else {
            interner
                .resolve(target.plugin)
                .filter(|plugin| !plugin.trim().is_empty())
                .ok_or_else(|| {
                    format!(
                        "dependency {} target plugin is unavailable",
                        record.source.signature
                    )
                })?
                .to_string()
        };
        if target.local == 0 || target.local > 0x00ff_ffff {
            return Err(format!(
                "dependency {} has invalid target local {:06X}",
                record.source.signature, target.local
            ));
        }
        projections.push(SkyrimCreatureTargetDependencyProjection {
            source: record.source.clone(),
            target: CreatureTargetRecordReference::new(
                skyrim_creature_target_signature(&record.source.signature),
                TargetFormKey::new(target.local, plugin),
            ),
        });
    }
    Ok(projections)
}

fn source_projection_field_receipts(
    candidate: &super::creature_mvp_adapter::SkyrimCreatureDependencyCandidate,
    dependency: &SkyrimCreatureDependencyLedger,
) -> Vec<SkyrimCreatureSourceFieldReceipt> {
    let records = dependency
        .records
        .iter()
        .map(|record| (record.source.form_key, record))
        .collect::<HashMap<_, _>>();
    candidate
        .closure
        .iter()
        .filter_map(|source| records.get(source).copied())
        .filter(|record| {
            matches!(
                record.disposition,
                SkyrimRecordLoweringDisposition::Ready {
                    kind: SkyrimRecordLoweringKind::SourceRigProjection
                }
            )
        })
        .flat_map(|record| {
            record
                .fields
                .iter()
                .cloned()
                .map(|source| SkyrimCreatureSourceFieldReceipt {
                    decision: SkyrimCreatureSourceFieldDecision::Derived {
                        target_signature: record.source.signature.clone(),
                        target_field: source.field_path.clone(),
                        policy_id: "skyrim_live_creature_projection_v1".to_string(),
                    },
                    source,
                })
        })
        .collect()
}

fn source_record_target_references(
    owner: FormKey,
    source_field: &str,
    target_signature: &str,
    dependency: &SkyrimCreatureDependencyLedger,
    projections: &[SkyrimCreatureTargetDependencyProjection],
) -> Result<Vec<CreatureTargetRecordReference>, String> {
    let record = dependency
        .records
        .iter()
        .find(|record| record.source.form_key == owner)
        .ok_or_else(|| format!("dependency ledger omitted source NPC_ {owner:?}"))?;
    let targets = projections
        .iter()
        .map(|projection| (projection.source.form_key, &projection.target))
        .collect::<HashMap<_, _>>();
    let mut references = record
        .references
        .iter()
        .filter_map(|reference| {
            let order = source_field_index(&reference.field_path, source_field)?;
            (reference.target_signature.as_deref() == Some(target_signature))
                .then_some((order, reference.target))
        })
        .collect::<Vec<_>>();
    references.sort_by_key(|(order, _)| *order);
    let dependency_records = dependency
        .records
        .iter()
        .map(|record| (record.source.form_key, record))
        .collect::<HashMap<_, _>>();
    references
        .into_iter()
        .filter_map(|(_, source)| match targets.get(&source).cloned().cloned() {
            Some(target) => Some(Ok(target)),
            None if !dependency_records.contains_key(&source) => None,
            None => Some(Err(format!(
                "{source_field} {target_signature} dependency {source:?} has no target projection"
            ))),
        })
        .collect()
}

fn source_npc_inventory(
    record: &Record,
    dependency: &SkyrimCreatureDependencyLedger,
    projections: &[SkyrimCreatureTargetDependencyProjection],
    records: &HashMap<FormKey, &Record>,
    reservations: &HashMap<FormKey, &SkyrimCreatureRecordReservation>,
    target_plugin: &str,
    interner: &StringInterner,
) -> Result<Vec<CreatureNpcInventoryEntry>, String> {
    let dependency_record = dependency
        .records
        .iter()
        .find(|candidate| candidate.source.form_key == record.form_key)
        .ok_or_else(|| {
            format!(
                "dependency ledger omitted source NPC_ {:?}",
                record.form_key
            )
        })?;
    let targets = projections
        .iter()
        .map(|projection| (projection.source.form_key, &projection.target))
        .collect::<HashMap<_, _>>();
    record
        .fields
        .iter()
        .enumerate()
        .filter(|(_, field)| field.sig.as_str() == "CNTO")
        .map(|(field_index, field)| {
            let prefix = format!("CNTO[{field_index}]");
            let references = dependency_record
                .references
                .iter()
                .filter(|reference| reference.field_path.starts_with(&prefix))
                .collect::<Vec<_>>();
            let (source, source_signature) = match references.as_slice() {
                [reference] => (
                    reference.target,
                    reference.target_signature.clone().ok_or_else(|| {
                        format!("CNTO[{field_index}] target signature is unknown")
                    })?,
                ),
                [] => {
                    let typed = embedded_form_keys(&field.value);
                    let source = match typed.as_slice() {
                        [source] => *source,
                        [] => {
                            let embedded = embedded_form_ids(&field.value, 0, 0);
                            let [raw] = embedded.as_slice() else {
                                return Err(format!(
                                    "source NPC_ CNTO[{field_index}] resolves 0 record references and {} embedded FormIDs",
                                    embedded.len()
                                ));
                            };
                            let owner_local = FormKey {
                                local: raw & 0x00ff_ffff,
                                plugin: record.form_key.plugin,
                            };
                            resolve_embedded_form_id(record.form_key, *raw, records)
                                .or_else(|| {
                                    projections
                                        .iter()
                                        .any(|projection| {
                                            projection.source.form_key == owner_local
                                        })
                                        .then_some(owner_local)
                                })
                                .ok_or_else(|| {
                                    format!(
                                        "source NPC_ CNTO[{field_index}] embedded FormID {raw:08X} is unresolved"
                                    )
                                })?
                        }
                        _ => {
                            return Err(format!(
                                "source NPC_ CNTO[{field_index}] contains {} typed FormKeys",
                                typed.len()
                            ));
                        }
                    };
                    let signature = projections
                        .iter()
                        .find(|projection| projection.source.form_key == source)
                        .map(|projection| projection.source.signature.clone())
                        .or_else(|| {
                            records
                                .get(&source)
                                .map(|record| record.sig.as_str().to_string())
                        })
                        .ok_or_else(|| {
                            format!("source NPC_ CNTO[{field_index}] target record is unavailable")
                        })?;
                    (source, signature)
                }
                _ => {
                    return Err(format!(
                        "source NPC_ CNTO[{field_index}] resolves {} record references",
                        references.len()
                    ));
                }
            };
            let expected_signature = skyrim_creature_target_signature(&source_signature);
            let target_record = targets
                .get(&source)
                .cloned()
                .cloned()
                .or_else(|| {
                    reserved_dependency_target(
                        source,
                        expected_signature,
                        reservations,
                        target_plugin,
                    )
                })
                .ok_or_else(|| {
                    format!("CNTO dependency {source:?} has no target projection")
                })?;
            if target_record.signature != expected_signature {
                return Err(format!(
                    "CNTO[{field_index}] target signature {} != {}",
                    target_record.signature, expected_signature
                ));
            }
            let count = cnto_count(&field.value, interner).ok_or_else(|| {
                format!("source NPC_ CNTO[{field_index}] has no exact signed count")
            })?;
            if count == 0 {
                return Err(format!("source NPC_ CNTO[{field_index}] has zero count"));
            }
            Ok(CreatureNpcInventoryEntry {
                target_record,
                count,
                ownership: match record.fields.get(field_index + 1) {
                    Some(field) if field.sig.as_str() == "COED" => {
                        Some(source_npc_inventory_ownership(
                            field_index + 1,
                            &field.value,
                            dependency_record,
                            projections,
                            reservations,
                            target_plugin,
                            interner,
                        )?)
                    }
                    _ => None,
                },
            })
        })
        .collect()
}

fn embedded_form_keys(value: &crate::record::FieldValue) -> Vec<FormKey> {
    match value {
        crate::record::FieldValue::FormKey(form_key) => (form_key.local != 0)
            .then_some(*form_key)
            .into_iter()
            .collect(),
        crate::record::FieldValue::List(values) => {
            values.iter().flat_map(embedded_form_keys).collect()
        }
        crate::record::FieldValue::Struct(fields) => fields
            .iter()
            .flat_map(|(_, value)| embedded_form_keys(value))
            .collect(),
        _ => Vec::new(),
    }
}

fn source_npc_inventory_ownership(
    field_index: usize,
    value: &crate::record::FieldValue,
    dependency_record: &super::creature_mvp_adapter::SkyrimCreatureDependencyRecord,
    projections: &[SkyrimCreatureTargetDependencyProjection],
    reservations: &HashMap<FormKey, &SkyrimCreatureRecordReservation>,
    target_plugin: &str,
    interner: &StringInterner,
) -> Result<CreatureNpcInventoryOwnership, String> {
    let (global_or_rank, condition) = coed_values(value, interner).ok_or_else(|| {
        format!("source NPC_ COED[{field_index}] is not the exact 12-byte layout")
    })?;
    if !condition.is_finite() || condition < 0.0 {
        return Err(format!(
            "source NPC_ COED[{field_index}] has invalid item condition {condition}"
        ));
    }
    let prefix = format!("COED[{field_index}]");
    let owners = dependency_record
        .references
        .iter()
        .filter(|reference| {
            reference.field_path.starts_with(&prefix)
                && matches!(
                    reference.target_signature.as_deref(),
                    Some("NPC_") | Some("FACT")
                )
        })
        .collect::<Vec<_>>();
    let owner = match owners.as_slice() {
        [] => None,
        [owner] => Some(owner),
        _ => {
            return Err(format!(
                "source NPC_ COED[{field_index}] resolves {} owners",
                owners.len()
            ));
        }
    };
    if owner.is_none() && coed_owner_is_nonzero(value, interner) {
        return Err(format!(
            "source NPC_ COED[{field_index}] has an unresolved owner FormID"
        ));
    }
    if let Some(owner) = owner
        && owner.target_signature.as_deref() == Some("FACT")
    {
        return Ok(CreatureNpcInventoryOwnership::FactionRank {
            faction: target_reference_for_source(
                owner.target,
                "FACT",
                projections,
                reservations,
                target_plugin,
            )?,
            required_rank: global_or_rank as i32,
            condition,
        });
    }
    if global_or_rank != 0 {
        return Err(format!(
            "source NPC_ COED[{field_index}] has a nonzero GLOB FormID whose Skyrim schema field is not FormKey-resolved"
        ));
    }
    let owner = owner
        .map(|owner| {
            target_reference_for_source(
                owner.target,
                "NPC_",
                projections,
                reservations,
                target_plugin,
            )
        })
        .transpose()?;
    Ok(CreatureNpcInventoryOwnership::OwnerGlobal {
        owner,
        global: None,
        condition,
    })
}

fn target_reference_for_source(
    source: FormKey,
    signature: &str,
    projections: &[SkyrimCreatureTargetDependencyProjection],
    reservations: &HashMap<FormKey, &SkyrimCreatureRecordReservation>,
    target_plugin: &str,
) -> Result<CreatureTargetRecordReference, String> {
    if let Some(projection) = projections
        .iter()
        .find(|projection| projection.source.form_key == source)
    {
        if projection.target.signature != signature {
            return Err(format!(
                "dependency {:?} signature {} != {signature}",
                source, projection.target.signature
            ));
        }
        return Ok(projection.target.clone());
    }
    let reservation = reservations
        .get(&source)
        .copied()
        .filter(|reservation| {
            matches!(
                (signature, reservation.kind),
                ("NPC_", SkyrimCreatureReservedRecordKind::Npc)
            )
        })
        .ok_or_else(|| format!("{signature} dependency {source:?} has no target projection"))?;
    Ok(CreatureTargetRecordReference::new(
        signature,
        TargetFormKey::new(reservation.target.local, target_plugin),
    ))
}

fn coed_values(value: &crate::record::FieldValue, interner: &StringInterner) -> Option<(u32, f32)> {
    match value {
        crate::record::FieldValue::Struct(fields) => {
            let global_or_rank = fields
                .iter()
                .find(|(name, _)| interner.resolve(*name) == Some("global_variable_required_rank"))
                .and_then(|(_, value)| field_i64(value))
                .and_then(|value| u32::try_from(value).ok())?;
            let condition = fields
                .iter()
                .find(|(name, _)| interner.resolve(*name) == Some("item_condition"))
                .and_then(|(_, value)| match value {
                    crate::record::FieldValue::Float(value) => Some(*value),
                    _ => None,
                })?;
            Some((global_or_rank, condition))
        }
        crate::record::FieldValue::Bytes(bytes) if bytes.len() == 12 => Some((
            u32::from_le_bytes(bytes[4..8].try_into().ok()?),
            f32::from_le_bytes(bytes[8..12].try_into().ok()?),
        )),
        _ => None,
    }
}

fn coed_owner_is_nonzero(value: &crate::record::FieldValue, interner: &StringInterner) -> bool {
    match value {
        crate::record::FieldValue::Struct(fields) => fields
            .iter()
            .find(|(name, _)| interner.resolve(*name) == Some("owner"))
            .is_some_and(|(_, value)| match value {
                crate::record::FieldValue::FormKey(value) => value.local != 0,
                crate::record::FieldValue::Uint(value) => *value != 0,
                crate::record::FieldValue::Int(value) => *value != 0,
                _ => false,
            }),
        crate::record::FieldValue::Bytes(bytes) if bytes.len() == 12 => {
            u32::from_le_bytes(bytes[0..4].try_into().unwrap()) != 0
        }
        _ => false,
    }
}

fn cnto_count(value: &crate::record::FieldValue, interner: &StringInterner) -> Option<i32> {
    match value {
        crate::record::FieldValue::Struct(fields) => fields
            .iter()
            .find(|(name, _)| {
                interner
                    .resolve(*name)
                    .is_some_and(|name| name.eq_ignore_ascii_case("count"))
            })
            .and_then(|(_, value)| field_i64(value))
            .and_then(|value| i32::try_from(value).ok()),
        crate::record::FieldValue::List(values) => {
            values.iter().find_map(|value| cnto_count(value, interner))
        }
        crate::record::FieldValue::Bytes(bytes) if bytes.len() >= 8 => {
            Some(i32::from_le_bytes(bytes[4..8].try_into().ok()?))
        }
        _ => None,
    }
}

fn source_field_index(field_path: &str, source_field: &str) -> Option<usize> {
    let suffix = field_path.strip_prefix(source_field)?.strip_prefix('[')?;
    suffix.split_once(']')?.0.parse().ok()
}

fn reserved_target(
    source: FormKey,
    expected_kind: SkyrimCreatureReservedRecordKind,
    reservations: &HashMap<FormKey, &SkyrimCreatureRecordReservation>,
    target_plugin: &str,
) -> Result<TargetFormKey, String> {
    let reservation = reservations
        .get(&source)
        .copied()
        .ok_or_else(|| format!("missing immutable {expected_kind:?} reservation"))?;
    if reservation.kind != expected_kind
        || reservation.target.local == 0
        || reservation.target.local > 0x00ff_ffff
    {
        return Err(format!("invalid immutable {expected_kind:?} reservation"));
    }
    Ok(TargetFormKey::new(reservation.target.local, target_plugin))
}

fn reserved_dependency_target(
    source: FormKey,
    expected_signature: &str,
    reservations: &HashMap<FormKey, &SkyrimCreatureRecordReservation>,
    target_plugin: &str,
) -> Option<CreatureTargetRecordReference> {
    let reservation = reservations.get(&source)?;
    let signature = match reservation.kind {
        SkyrimCreatureReservedRecordKind::Race => "RACE",
        SkyrimCreatureReservedRecordKind::Npc => "NPC_",
        SkyrimCreatureReservedRecordKind::Armor => "ARMO",
        SkyrimCreatureReservedRecordKind::ArmorAddon => "ARMA",
        SkyrimCreatureReservedRecordKind::BodyPartData => "BPTD",
    };
    (signature == expected_signature
        && reservation.target.local != 0
        && reservation.target.local <= 0x00ff_ffff)
        .then(|| {
            CreatureTargetRecordReference::new(
                signature,
                TargetFormKey::new(reservation.target.local, target_plugin),
            )
        })
}

fn source_identity(
    source: FormKey,
    interner: &StringInterner,
) -> Result<SourceCreatureIdentity, String> {
    let plugin = interner
        .resolve(source.plugin)
        .filter(|plugin| !plugin.trim().is_empty())
        .ok_or_else(|| "source FormKey plugin is unavailable".to_string())?;
    Ok(SourceCreatureIdentity {
        namespace: "skyrimse".to_string(),
        plugin: plugin.to_string(),
        local_form_id: source.local,
    })
}

fn parse_form_key(value: &str, interner: &StringInterner) -> Option<FormKey> {
    let (local, plugin) = value.split_once('@')?;
    let local = u32::from_str_radix(local, 16).ok()?;
    (!plugin.trim().is_empty() && local > 0 && local <= 0x00ff_ffff).then(|| FormKey {
        local,
        plugin: interner.intern(plugin),
    })
}

fn source_key(source: FormKey, interner: &StringInterner) -> String {
    SourceCreatureIdentity {
        namespace: "skyrimse".to_string(),
        plugin: interner
            .resolve(source.plugin)
            .unwrap_or_default()
            .to_string(),
        local_form_id: source.local,
    }
    .stable_key()
}

fn projected_editor_id(prefix: &str, source: &str, local: u32) -> String {
    let stem = source
        .chars()
        .filter(|character| character.is_ascii_alphanumeric() || *character == '_')
        .collect::<String>();
    format!("{prefix}{}_{local:06X}", stem.trim_start_matches('_'))
}

fn record_editor_id(record: &Record, interner: &StringInterner) -> Option<String> {
    record
        .fields
        .iter()
        .find(|field| field.sig.as_str() == "EDID")
        .and_then(|field| field_string(&field.value, interner))
}

fn record_display_name(record: &Record, interner: &StringInterner) -> Option<String> {
    record
        .fields
        .iter()
        .find(|field| field.sig.as_str() == "FULL")
        .and_then(|field| field_string(&field.value, interner))
}

fn field_string(value: &crate::record::FieldValue, interner: &StringInterner) -> Option<String> {
    match value {
        crate::record::FieldValue::String(value) => interner.resolve(*value).map(str::to_string),
        crate::record::FieldValue::Bytes(value) => std::str::from_utf8(value)
            .ok()
            .map(|value| value.trim_end_matches('\0').to_string()),
        _ => None,
    }
    .filter(|value| !value.trim().is_empty())
}

fn form_key_sort_key(source: FormKey, interner: &StringInterner) -> (String, u32) {
    (
        interner
            .resolve(source.plugin)
            .unwrap_or_default()
            .to_ascii_lowercase(),
        source.local,
    )
}

fn same_runtime_path(left: &str, right: &str) -> bool {
    path_key(left) == path_key(right)
}

fn data_relative_mesh_path(path: &str) -> String {
    let canonical = path_key(path);
    if canonical.starts_with("meshes/") {
        canonical
    } else {
        format!("meshes/{canonical}")
    }
}

fn runtime_nif_path(data_relative_path: &str) -> String {
    let path = data_relative_path.replace('/', "\\");
    path.strip_prefix("meshes\\")
        .or_else(|| path.strip_prefix("Meshes\\"))
        .unwrap_or(&path)
        .to_string()
}

fn live_blocker(
    code: impl Into<String>,
    detail: impl Into<String>,
) -> SkyrimCreatureLivePreparationBlocker {
    SkyrimCreatureLivePreparationBlocker {
        code: code.into(),
        detail: detail.into(),
    }
}

fn canonical_live_blockers(blockers: &mut Vec<SkyrimCreatureLivePreparationBlocker>) {
    blockers.sort();
    blockers.dedup();
}

fn canonical_dependency_projections(
    mut projections: Vec<SkyrimCreatureTargetDependencyProjection>,
    interner: &StringInterner,
) -> Vec<SkyrimCreatureTargetDependencyProjection> {
    projections.sort_by_key(|projection| {
        (
            form_key_sort_key(projection.source.form_key, interner),
            projection.target.signature.clone(),
            projection.target.form_key.plugin.to_ascii_lowercase(),
            projection.target.form_key.local,
        )
    });
    projections.dedup_by(|left, right| left == right);
    projections
}

fn canonical_source_field_receipts(
    mut receipts: Vec<SkyrimCreatureSourceFieldReceipt>,
    interner: &StringInterner,
) -> Vec<SkyrimCreatureSourceFieldReceipt> {
    receipts.sort_by_key(|receipt| {
        (
            form_key_sort_key(receipt.source.owner.form_key, interner),
            receipt.source.field_path.clone(),
        )
    });
    receipts.dedup_by(|left, right| left == right);
    receipts
}

fn build_live_family_assets(
    catalog: &CreatureCorpusPlan,
    motion: &SkyrimCreatureMotionCatalog,
    dependency: &SkyrimCreatureDependencyLedger,
    source_data_root: &Path,
    interner: &StringInterner,
) -> Vec<SkyrimCreatureFamilyAssetEvidence> {
    let dependency_records = dependency
        .records
        .iter()
        .map(|record| (record.source.form_key, record))
        .collect::<HashMap<_, _>>();
    let dependency_candidates = dependency
        .candidates
        .iter()
        .map(|candidate| (candidate.source_race, candidate))
        .collect::<HashMap<_, _>>();
    let mut output = Vec::with_capacity(motion.families.len());
    for family in &motion.families {
        let inventory = motion.inventory(&family.family_id);
        let races = races_for_project(catalog, &family.project_path)
            .into_iter()
            .filter(|race| {
                dependency_candidates
                    .get(&race.source_race)
                    .is_some_and(ready_candidate)
            })
            .collect::<Vec<_>>();
        let mut claims = BTreeMap::<(String, SkyrimAssetRole), LiveClaim>::new();
        if let Some(inventory) = inventory {
            add_inventory_claims(inventory, family, motion, source_data_root, &mut claims);
        }
        for race in &races {
            for body in &race.body_models {
                add_claim(
                    &mut claims,
                    source_data_root,
                    body,
                    SkyrimAssetRole::BodyNif,
                );
            }
            for skeleton in &race.skeleton_paths {
                add_claim(
                    &mut claims,
                    source_data_root,
                    skeleton,
                    SkyrimAssetRole::VisualSkeletonNif,
                );
            }
        }
        let graph_variants = inventory
            .map(|inventory| graph_variants(&races, inventory, source_data_root))
            .unwrap_or_default();
        for variant in &graph_variants {
            add_claim(
                &mut claims,
                source_data_root,
                &variant.skeleton_path,
                SkyrimAssetRole::VisualSkeletonNif,
            );
        }
        expand_nif_dependencies(source_data_root, &mut claims);

        let controllers = inventory.map(controller_evidence).unwrap_or_default();
        let body_variants = races
            .iter()
            .flat_map(|race| {
                race.body_model_parts
                    .iter()
                    .map(|part| SkyrimCreatureBodyVariantEvidence {
                        source_race: race.source_race.format(interner),
                        sex: SkyrimCreatureSex::Ungendered,
                        armor_addon: part.armor_addon.format(interner),
                        body_nif: canonical_runtime_path(&part.body_nif),
                    })
            })
            .collect::<Vec<_>>();
        let ragdoll = inventory
            .map(|inventory| ragdoll_evidence(inventory, &claims))
            .unwrap_or_else(|| SkyrimRagdollCapabilityEvidence::Missing {
                expected_paths: Vec::new(),
            });
        let weapon_bearing = weapon_bearing_evidence(
            &races,
            &dependency_candidates,
            &dependency_records,
            interner,
        );
        output.push(SkyrimCreatureFamilyAssetEvidence {
            family_id: family.family_id.clone(),
            graph_variants,
            controllers,
            ragdoll,
            weapon_bearing,
            body_variants,
            asset_claims: claims.into_values().map(LiveClaim::finish).collect(),
        });
    }
    output.sort_by(|left, right| left.family_id.cmp(&right.family_id));
    output
}

fn races_for_project<'a>(
    catalog: &'a CreatureCorpusPlan,
    project_path: &str,
) -> Vec<&'a CreatureRacePlan> {
    let key = project_key(project_path);
    catalog
        .races
        .iter()
        .filter(|race| {
            primary_slot(&race.project_paths).is_some_and(|path| project_key(path) == key)
        })
        .collect()
}

// Creature races are ungendered, but Skyrim still stores male and female slots and
// every DLC race leaves the base creature it was copied from sitting in the female
// one — DLC2LurkerRace ships Giant, DLC2AshHopperRace ships Skeever. Matching those
// vestigial slots drags foreign races into a family and makes its skeletons
// disagree, so only the male slot speaks for the race.
fn primary_slot(paths: &[String]) -> Option<&String> {
    paths.first()
}

fn add_inventory_claims(
    inventory: &SkyrimCreatureFamilyInventory,
    family: &super::creature_motion::SkyrimFamilyMotionSet,
    motion: &SkyrimCreatureMotionCatalog,
    source_data_root: &Path,
    claims: &mut BTreeMap<(String, SkyrimAssetRole), LiveClaim>,
) {
    add_claim(
        claims,
        source_data_root,
        &inventory.project_path,
        SkyrimAssetRole::Project,
    );
    for (paths, role) in [
        (&inventory.character_paths, SkyrimAssetRole::Character),
        (
            &inventory.animation_skeleton_paths,
            SkyrimAssetRole::AnimationSkeleton,
        ),
        (&inventory.ragdoll_paths, SkyrimAssetRole::Ragdoll),
        (&inventory.behavior_paths, SkyrimAssetRole::Behavior),
    ] {
        for path in paths {
            add_claim(claims, source_data_root, path, role);
        }
    }
    for clip_id in &family.clip_ids {
        if let Some(clip) = motion.clip(clip_id) {
            add_claim(
                claims,
                source_data_root,
                &clip.clip_path,
                SkyrimAssetRole::AnimationClip,
            );
        }
    }
}

fn graph_variants(
    races: &[&CreatureRacePlan],
    inventory: &SkyrimCreatureFamilyInventory,
    source_data_root: &Path,
) -> Vec<SkyrimCreatureGraphVariantEvidence> {
    let characters = inventory.character_paths.iter().collect::<BTreeSet<_>>();
    let animation_skeletons = inventory
        .animation_skeleton_paths
        .iter()
        .collect::<BTreeSet<_>>();
    let characters = characters.into_iter().collect::<Vec<_>>();
    let animation_skeletons = animation_skeletons.into_iter().collect::<Vec<_>>();
    let primary_skeletons = races
        .iter()
        .filter_map(|race| primary_slot(&race.skeleton_paths))
        .collect::<Vec<_>>();
    let Some(visual_skeleton) = single_distinct_path(primary_skeletons.iter().copied())
        .or_else(|| equivalent_visual_skeleton(&primary_skeletons, source_data_root))
    else {
        return Vec::new();
    };
    let ([character], [animation_skeleton]) =
        (characters.as_slice(), animation_skeletons.as_slice())
    else {
        return Vec::new();
    };
    let source = asset_path(source_data_root, animation_skeleton);
    let receipt = match crate::phase::skeleton::convert_skyrim_source_owned_skeleton_artifact(
        &source,
        "Actors\\B21_SkyrimEvidence\\CharacterAssets\\Skeleton.hkx",
    ) {
        Ok(receipt) => receipt,
        Err(error) => {
            if std::env::var_os("SKYRIMSE_CREATURE_DIAGNOSTIC_VERBOSE").is_some() {
                eprintln!(
                    "Skyrim graph variant rejected animation skeleton {}: {}",
                    animation_skeleton, error
                );
            }
            return Vec::new();
        }
    };
    let roots = receipt
        .ordered_bone_names
        .iter()
        .zip(&receipt.parent_indices)
        .filter_map(|(name, parent)| (*parent < 0).then_some(name.clone()))
        .collect::<Vec<_>>();
    let Some(actual_root_bone) = roots
        .iter()
        .find(|name| name.as_str() == receipt.skeleton_name)
    else {
        return Vec::new();
    };
    vec![SkyrimCreatureGraphVariantEvidence {
        sex: SkyrimCreatureSex::Ungendered,
        project_path: canonical_runtime_path(&inventory.project_path),
        character_path: canonical_runtime_path(character),
        skeleton_path: canonical_runtime_path(visual_skeleton),
        actual_root_bone: (*actual_root_bone).clone(),
        behavior_paths: inventory
            .behavior_paths
            .iter()
            .map(|path| canonical_runtime_path(path))
            .collect(),
    }]
}

// Candidate family ids come from the catalog partition
// (`skyrim-creature-family-NNNN`), which groups races differently than the motion
// catalog's project slugs; the two id spaces never intersect. `races_for_project`
// already scoped this race to the motion family by havok project path, so
// readiness is all that is left to decide here.
fn ready_candidate(
    candidate: &&super::creature_mvp_adapter::SkyrimCreatureDependencyCandidate,
) -> bool {
    matches!(
        candidate.disposition,
        SkyrimCreatureDependencyCandidateDisposition::Ready
    )
}

// Skyrim sometimes ships one rig under two filenames: the BoarRiekling folder holds
// `skeleton.nif` and `skeletonboar.nif`, both carrying the same 92 bones, and its
// races are split across them. Differing paths therefore do not prove differing
// rigs, so fall back to comparing bone sets and let the most-used file win.
// Genuinely different bone sets still return None, which is a real ambiguity.
fn equivalent_visual_skeleton<'a>(
    paths: &[&'a String],
    source_data_root: &Path,
) -> Option<&'a String> {
    let mut uses = BTreeMap::<String, (usize, &'a String)>::new();
    for path in paths {
        uses.entry(path_key(path)).or_insert((0, path)).0 += 1;
    }
    let mut bone_sets = uses
        .values()
        .map(|(_, path)| skeleton_bone_names(source_data_root, path));
    let first = bone_sets.next()??;
    if !bone_sets.all(|bones| bones.is_some_and(|bones| bones == first)) {
        return None;
    }
    uses.values()
        .max_by_key(|(count, path)| (*count, std::cmp::Reverse(path.to_ascii_lowercase())))
        .map(|(_, path)| *path)
}

fn skeleton_bone_names(source_data_root: &Path, path: &str) -> Option<BTreeSet<String>> {
    let nif = NifFile::load(asset_path(source_data_root, path)).ok()?;
    // The scene root is named after the file, so it would report two copies of one
    // rig as different bone sets purely on filename.
    let children = nif
        .get_hierarchy()
        .into_values()
        .flatten()
        .collect::<BTreeSet<_>>();
    Some(
        nif.find_blocks("NiNode")
            .into_iter()
            .filter(|block_id| children.contains(&(*block_id as i32)))
            .filter_map(
                |block_id| match nif.get_block(block_id)?.get_field("Name") {
                    Some(nif_core_native::model::NifValue::String(name)) => Some(name.clone()),
                    _ => None,
                },
            )
            .collect(),
    )
}

fn single_distinct_path<'a>(paths: impl Iterator<Item = &'a String>) -> Option<&'a String> {
    let mut paths = paths.collect::<BTreeSet<_>>().into_iter();
    let path = paths.next()?;
    paths.next().is_none().then_some(path)
}

fn controller_evidence(
    inventory: &SkyrimCreatureFamilyInventory,
) -> Vec<SkyrimCreatureControllerRecipeEvidence> {
    inventory
        .controllers
        .iter()
        .filter_map(|controller| {
            let legacy = controller.legacy_controller_recipe_evidence()?;
            let up = dominant_axis(legacy.model.up_ms)?;
            let forward = dominant_axis(legacy.model.forward_ms)?;
            if up == forward {
                return None;
            }
            let architecture = match controller.architecture.as_ref() {
                Some(SkyrimControllerArchitectureEvidence::CharacterProxyCinfo) => {
                    ControllerArchitecture::Proxy
                }
                Some(SkyrimControllerArchitectureEvidence::FixedCinfo) => {
                    ControllerArchitecture::Fixed
                }
                Some(SkyrimControllerArchitectureEvidence::InlineRigidBodySetup)
                | Some(SkyrimControllerArchitectureEvidence::CharacterRigidBodyCinfo) => {
                    ControllerArchitecture::Standard
                }
                Some(SkyrimControllerArchitectureEvidence::CustomCinfo { .. }) => return None,
                None => ControllerArchitecture::Standard,
            };
            Some(SkyrimCreatureControllerRecipeEvidence {
                character_path: canonical_runtime_path(&controller.character_path),
                layout: controller_layout(&legacy.layout),
                axes: Some(SkyrimControllerAxesEvidence { up, forward }),
                capsule: Some(SkyrimControllerCapsuleReceipt {
                    total_height: legacy.capsule.total_height,
                    radius: legacy.capsule.radius,
                    architecture,
                }),
            })
        })
        .collect()
}

fn controller_layout(value: &MotionControllerLayout) -> SkyrimControllerLayoutEvidence {
    match value {
        MotionControllerLayout::LegacyCharacterControllerInfo {
            contents_version,
            character_data_signature,
            controller_signature,
        } => SkyrimControllerLayoutEvidence::LegacyCharacterControllerInfo {
            contents_version: contents_version.clone(),
            character_data_signature: *character_data_signature,
            controller_signature: *controller_signature,
        },
        MotionControllerLayout::NestedCharacterControllerSetup {
            contents_version,
            character_data_signature,
            controller_signature,
        } => SkyrimControllerLayoutEvidence::NestedCharacterControllerSetup {
            contents_version: contents_version.clone(),
            character_data_signature: *character_data_signature,
            controller_signature: *controller_signature,
        },
    }
}

fn dominant_axis(vector: [f32; 4]) -> Option<MeasurementAxis> {
    let components = [vector[0].abs(), vector[1].abs(), vector[2].abs()];
    if components.iter().any(|value| !value.is_finite()) {
        return None;
    }
    let maximum = components.iter().copied().reduce(f32::max)?;
    let matches = components
        .iter()
        .enumerate()
        .filter(|(_, value)| **value == maximum)
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    match matches.as_slice() {
        [0] if maximum > 0.0 => Some(MeasurementAxis::X),
        [1] if maximum > 0.0 => Some(MeasurementAxis::Y),
        [2] if maximum > 0.0 => Some(MeasurementAxis::Z),
        _ => None,
    }
}

fn ragdoll_evidence(
    inventory: &SkyrimCreatureFamilyInventory,
    claims: &BTreeMap<(String, SkyrimAssetRole), LiveClaim>,
) -> SkyrimRagdollCapabilityEvidence {
    if inventory.ragdoll_paths.is_empty() {
        return SkyrimRagdollCapabilityEvidence::NotApplicable {
            reason: "decoded hkbCharacterStringData has no ragdollName".to_string(),
        };
    }
    let paths = inventory
        .ragdoll_paths
        .iter()
        .map(|path| canonical_runtime_path(path))
        .collect::<Vec<_>>();
    if paths.iter().all(|path| {
        claims
            .get(&(path_key(path), SkyrimAssetRole::Ragdoll))
            .is_some_and(|claim| {
                matches!(
                    claim.disposition,
                    SkyrimAssetClaimDisposition::Present { .. }
                )
            })
    }) {
        SkyrimRagdollCapabilityEvidence::Present { paths }
    } else {
        SkyrimRagdollCapabilityEvidence::Missing {
            expected_paths: paths,
        }
    }
}

fn weapon_bearing_evidence(
    races: &[&CreatureRacePlan],
    candidates: &HashMap<
        crate::ids::FormKey,
        &super::creature_mvp_adapter::SkyrimCreatureDependencyCandidate,
    >,
    records: &HashMap<
        crate::ids::FormKey,
        &super::creature_mvp_adapter::SkyrimCreatureDependencyRecord,
    >,
    interner: &StringInterner,
) -> SkyrimExplicitCapabilityEvidence {
    let mut weapons = races
        .iter()
        .filter_map(|race| candidates.get(&race.source_race))
        .flat_map(|candidate| candidate.closure.iter())
        .filter_map(|form_key| records.get(form_key))
        .filter(|record| {
            record.source.signature == "WEAP"
                && matches!(
                    record.disposition,
                    SkyrimRecordLoweringDisposition::Ready { .. }
                )
        })
        .map(|record| record.source.form_key.format(interner))
        .collect::<Vec<_>>();
    weapons.sort_by_key(|value| value.to_ascii_lowercase());
    weapons.dedup_by(|left, right| left.eq_ignore_ascii_case(right));
    if weapons.is_empty() {
        SkyrimExplicitCapabilityEvidence::NotApplicable {
            evidence: "recursive source record closure contains no WEAP".to_string(),
        }
    } else {
        SkyrimExplicitCapabilityEvidence::Supported {
            evidence: format!("recursive source WEAP closure: {}", weapons.join(",")),
        }
    }
}

#[derive(Clone)]
struct LiveClaim {
    path: String,
    role: SkyrimAssetRole,
    dependencies: BTreeSet<String>,
    disposition: SkyrimAssetClaimDisposition,
}

impl LiveClaim {
    fn finish(self) -> SkyrimRecursiveAssetClaim {
        SkyrimRecursiveAssetClaim {
            path: self.path,
            role: self.role,
            dependencies: self.dependencies.into_iter().collect(),
            disposition: self.disposition,
        }
    }
}

fn add_claim(
    claims: &mut BTreeMap<(String, SkyrimAssetRole), LiveClaim>,
    source_data_root: &Path,
    path: &str,
    role: SkyrimAssetRole,
) {
    let path = canonical_runtime_path(path);
    let key = path_key(&path);
    let disposition = std::fs::read(asset_path(source_data_root, &path))
        .ok()
        .filter(|bytes| !bytes.is_empty())
        .map(|bytes| SkyrimAssetClaimDisposition::Present {
            blake3: blake3::hash(&bytes).to_hex().to_string(),
        })
        .unwrap_or(SkyrimAssetClaimDisposition::Missing);
    claims.entry((key, role)).or_insert(LiveClaim {
        path,
        role,
        dependencies: BTreeSet::new(),
        disposition,
    });
}

fn expand_nif_dependencies(
    source_data_root: &Path,
    claims: &mut BTreeMap<(String, SkyrimAssetRole), LiveClaim>,
) {
    let nifs = claims
        .values()
        .filter(|claim| {
            matches!(
                claim.role,
                SkyrimAssetRole::BodyNif | SkyrimAssetRole::VisualSkeletonNif
            )
        })
        .map(|claim| (claim.path.clone(), claim.role))
        .collect::<Vec<_>>();
    for (nif_path, nif_role) in nifs {
        let Ok(nif) = NifFile::load(asset_path(source_data_root, &nif_path)) else {
            continue;
        };
        let referenced = nif.referenced_asset_paths();
        let materials = referenced
            .materials
            .into_iter()
            .filter(|path| valid_recursive_asset_reference(path))
            .collect::<Vec<_>>();
        let textures = referenced
            .textures
            .into_iter()
            .filter(|path| valid_recursive_asset_reference(path))
            .collect::<Vec<_>>();
        let dependencies = materials
            .iter()
            .chain(&textures)
            .map(|path| canonical_runtime_path(path))
            .collect::<Vec<_>>();
        if let Some(claim) = claims.get_mut(&(path_key(&nif_path), nif_role)) {
            claim.dependencies.extend(dependencies.iter().cloned());
        }
        for material in materials {
            add_claim(
                claims,
                source_data_root,
                &material,
                SkyrimAssetRole::Material,
            );
        }
        for texture in textures {
            add_claim(claims, source_data_root, &texture, SkyrimAssetRole::Texture);
        }
    }
}

fn valid_recursive_asset_reference(path: &str) -> bool {
    !path.is_empty()
        && !path.chars().any(char::is_control)
        && !path
            .replace('\\', "/")
            .split('/')
            .any(|component| component == "..")
}

fn asset_path(source_data_root: &Path, path: &str) -> PathBuf {
    let normalized = path_key(path);
    let is_data_root_path = normalized.starts_with("textures/")
        || normalized.starts_with("materials/")
        || normalized.starts_with("meshes/");
    let root_is_meshes = source_data_root
        .file_name()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case("meshes"));
    let base = if root_is_meshes && is_data_root_path {
        source_data_root
            .parent()
            .unwrap_or(source_data_root)
            .to_path_buf()
    } else if !root_is_meshes && !is_data_root_path {
        let meshes = source_data_root.join("Meshes");
        if meshes.is_dir() {
            meshes
        } else {
            source_data_root.to_path_buf()
        }
    } else {
        source_data_root.to_path_buf()
    };
    normalized
        .split('/')
        .filter(|component| !component.is_empty())
        .fold(base, |current, component| current.join(component))
}

fn canonical_runtime_path(path: &str) -> String {
    let normalized = path.trim().replace('/', "\\");
    let normalized = normalized.trim_start_matches(".\\");
    if normalized
        .get(.."meshes\\".len())
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("meshes\\"))
    {
        normalized["meshes\\".len()..].to_string()
    } else {
        normalized.to_string()
    }
}

fn path_key(path: &str) -> String {
    canonical_runtime_path(path)
        .replace('\\', "/")
        .to_ascii_lowercase()
}

fn project_key(path: &str) -> String {
    let key = path_key(path);
    key.strip_prefix("actors/").unwrap_or(&key).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::{SigCode, SubrecordSig};
    use crate::record::{FieldEntry, FieldValue};
    use crate::skyrimse_fo4_runtime::creature_recipe::{
        SkyrimCreatureClipRoleReceipt, SkyrimCreatureMotionRoleReceipt,
    };
    use smallvec::SmallVec;

    #[test]
    fn optional_installed_atronach_turn_preserves_bound_root_motion() {
        let Some(data_root) = std::env::var_os("SKYRIMSE_CREATURE_DATA_DIR").map(PathBuf::from)
        else {
            return;
        };
        let source_path = data_root.join("Meshes/Actors/atronachflame/animations/turnloopingl.hkx");
        let source = fs::read(&source_path)
            .unwrap_or_else(|error| panic!("read {}: {error}", source_path.display()));
        let converted = havok_native::api::havok_reemit_skyrim_2010_animation_asset_to_fo4(&source)
            .expect("re-emit installed Flame Atronach turn clip");
        let motion_path = data_root.join("Meshes/animationdatasinglefile.txt");
        let motion =
            load_skyrim_bound_motion_samples(&motion_path, "atronachflame/atronachflame.hkx", 31)
                .expect("load installed Flame Atronach BoundAnims samples");
        let expected_sample_count = motion.translations.len().max(motion.rotations.len());
        let translation_delta = motion
            .translations
            .first()
            .zip(motion.translations.last())
            .map(|(first, last)| {
                std::array::from_fn(|component| last.value[component] - first.value[component])
            })
            .unwrap_or([0.0; 3]);
        let clip = SkyrimCreatureClipReceipt {
            clip_id: "clip_actors_atronachflame_animations_turnloopingl_hkx".to_string(),
            clip_path: "Actors\\atronachflame\\animations\\turnloopingl.hkx".to_string(),
            family_ids: vec!["atronach_flame".to_string()],
            role: SkyrimCreatureClipRoleReceipt::Role(SkyrimCreatureMotionRoleReceipt::TurnLeft),
            trigger_event: Some("turnLeft".to_string()),
            trigger_aliases: Vec::new(),
            role_evidence: Vec::new(),
            root_motion: SkyrimRootMotionReceipt::Sampled {
                source: SkyrimRootMotionSourceReceipt::BoundAnims,
                sample_count: expected_sample_count,
                translation_delta,
                rotation_start: motion.rotations.first().map(|sample| sample.value),
                rotation_end: motion.rotations.last().map(|sample| sample.value),
            },
            root_motion_locators: vec![SkyrimRootMotionLocatorReceipt::BoundAnims {
                source_path: motion_path.display().to_string(),
                project_stem: "atronachflame".to_string(),
                animation_index: 31,
            }],
            events: Vec::new(),
            annotations: Vec::new(),
            original_skeleton_name: Some("NPC Root [Root]".to_string()),
        };
        let output = preserve_bound_root_motion(&converted, &clip)
            .expect("embed installed Flame Atronach BoundAnims samples");
        let hkx = HkxFile::read(&output).expect("decode converted clip with root motion");
        let frames = hkx
            .objects()
            .iter()
            .enumerate()
            .filter(|(_, object)| object.class_name == "hkaDefaultAnimatedReferenceFrame")
            .collect::<Vec<_>>();
        assert_eq!(frames.len(), 1);
        let frame_samples = frames[0]
            .1
            .members
            .iter()
            .find(|member| member.name == "referenceFrameSamples")
            .expect("referenceFrameSamples");
        assert!(matches!(
            &frame_samples.value,
            HkxValue::Array(values) if values.len() == expected_sample_count
        ));
        assert!(hkx.objects().iter().any(|object| {
            object.members.iter().any(|member| {
                member.name == "extractedMotion"
                    && member.value == HkxValue::Pointer(Some(frames[0].0))
            })
        }));
    }

    #[test]
    fn live_member_keys_match_recipe_identity_keys() {
        let interner = StringInterner::new();
        let source = FormKey {
            local: 0x0131f5,
            plugin: interner.intern("Skyrim.esm"),
        };

        assert_eq!(
            source_key(source, &interner),
            source_identity(source, &interner)
                .expect("source identity")
                .stable_key()
        );
    }

    #[test]
    fn family_assets_accept_ready_candidates_carrying_catalog_family_ids() {
        let interner = StringInterner::new();
        let candidate = SkyrimCreatureDependencyCandidate {
            source_race: FormKey {
                local: 0x0131f9,
                plugin: interner.intern("Skyrim.esm"),
            },
            family_id: Some("skyrim-creature-family-0042".to_string()),
            npc_sources: Vec::new(),
            template_sources: Vec::new(),
            seeds: Vec::new(),
            closure: Vec::new(),
            disposition: SkyrimCreatureDependencyCandidateDisposition::Ready,
        };

        assert!(ready_candidate(&&candidate));

        let mut blocked = candidate;
        blocked.disposition = SkyrimCreatureDependencyCandidateDisposition::Blocked {
            blockers: Vec::new(),
        };
        assert!(!ready_candidate(&&blocked));

        let excluded = SkyrimCreatureDependencyCandidate {
            family_id: None,
            disposition: SkyrimCreatureDependencyCandidateDisposition::CuratedExclusion,
            ..blocked
        };
        assert!(!ready_candidate(&&excluded));
    }

    #[test]
    fn graph_variant_requires_one_distinct_visual_skeleton() {
        let giant = "Actors\\Giant\\Character Assets\\Skeleton.nif".to_string();
        let benthic = "Actors\\DLC02\\BenthicLurker\\Character Assets\\Skeleton.nif".to_string();

        assert_eq!(
            single_distinct_path([&giant, &giant].into_iter()),
            Some(&giant)
        );
        assert_eq!(single_distinct_path([&giant, &benthic].into_iter()), None);
    }

    #[test]
    fn optional_boar_skeleton_filenames_describe_one_shared_rig() {
        let Some(data_root) = std::env::var_os("SKYRIMSE_CREATURE_DATA_DIR") else {
            return;
        };
        let data_root = PathBuf::from(data_root);
        let rider = "Actors\\DLC02\\BoarRiekling\\Character Assets\\skeleton.nif".to_string();
        let boar = "Actors\\DLC02\\BoarRiekling\\Character Assets\\SkeletonBoar.nif".to_string();
        assert_eq!(
            skeleton_bone_names(&data_root, &rider),
            skeleton_bone_names(&data_root, &boar),
            "boar and rider skeleton files should carry the same bones"
        );
        // Two races on SkeletonBoar, one on skeleton.nif — the majority file wins.
        assert_eq!(
            equivalent_visual_skeleton(&[&boar, &rider, &boar], &data_root),
            Some(&boar)
        );
    }

    #[test]
    fn vestigial_female_slots_do_not_join_a_race_to_a_foreign_family() {
        let interner = StringInterner::new();
        let plugin = interner.intern("Skyrim.esm");
        let race_plan = |local, projects: &[&str], skeletons: &[&str]| CreatureRacePlan {
            source_race: FormKey { local, plugin },
            source_plugin: "Skyrim.esm".to_string(),
            editor_id: None,
            skin: None,
            armor_addons: Vec::new(),
            body_models: Vec::new(),
            body_model_parts: Vec::new(),
            body_part_data: None,
            project_paths: projects.iter().map(|path| (*path).to_string()).collect(),
            skeleton_paths: skeletons.iter().map(|path| (*path).to_string()).collect(),
            attack_events: Vec::new(),
            attack_contract: Vec::new(),
            attack_data: Vec::new(),
            attack_spells: Vec::new(),
            issues: Vec::new(),
        };
        let giant = race_plan(
            0x0131f4,
            &["Actors\\Giant\\GiantProject.hkx"],
            &["Actors\\Giant\\Character Assets\\Skeleton.nif"],
        );
        // DLC2LurkerRace keeps Giant in both of its female slots.
        let lurker = race_plan(
            0x014495,
            &[
                "Actors\\DLC02\\BenthicLurker\\BenthicLurkerProject.hkx",
                "Actors\\Giant\\GiantProject.hkx",
            ],
            &[
                "Actors\\DLC02\\BenthicLurker\\Character Assets\\Skeleton.nif",
                "Actors\\Giant\\Character Assets\\Skeleton.nif",
            ],
        );
        let catalog = CreatureCorpusPlan {
            races: vec![giant.clone(), lurker.clone()],
            ..Default::default()
        };

        let matched = races_for_project(&catalog, "giant/giantproject.hkx");
        assert_eq!(
            matched
                .iter()
                .map(|race| race.source_race)
                .collect::<Vec<_>>(),
            vec![giant.source_race]
        );
        assert_eq!(
            single_distinct_path(
                matched
                    .iter()
                    .filter_map(|race| primary_slot(&race.skeleton_paths))
            ),
            Some(&giant.skeleton_paths[0])
        );

        let matched = races_for_project(&catalog, "dlc02/benthiclurker/benthiclurkerproject.hkx");
        assert_eq!(
            matched
                .iter()
                .map(|race| race.source_race)
                .collect::<Vec<_>>(),
            vec![lurker.source_race]
        );
        assert_eq!(
            single_distinct_path(
                matched
                    .iter()
                    .filter_map(|race| primary_slot(&race.skeleton_paths))
            ),
            Some(&lurker.skeleton_paths[0])
        );
    }

    #[test]
    fn projected_npc_sources_exclude_transitive_npc_dependencies() {
        let interner = StringInterner::new();
        let plugin = interner.intern("Skyrim.esm");
        let source_race = FormKey {
            local: 0x0131f5,
            plugin,
        };
        let creature_npc = FormKey {
            local: 0x0204c0,
            plugin,
        };
        let transitive_npc = FormKey {
            local: 0x01750c,
            plugin,
        };
        let candidate = SkyrimCreatureDependencyCandidate {
            source_race,
            family_id: Some("atronach_flame".to_string()),
            npc_sources: vec![creature_npc, creature_npc],
            template_sources: Vec::new(),
            seeds: vec![source_race, creature_npc],
            closure: vec![source_race, creature_npc, transitive_npc],
            disposition: SkyrimCreatureDependencyCandidateDisposition::Ready,
        };

        assert_eq!(
            projected_creature_npc_sources(&candidate, &interner),
            vec![creature_npc]
        );
    }

    #[test]
    fn shared_support_records_reserve_per_race_variants() {
        let interner = StringInterner::new();
        let plugin = interner.intern("Skyrim.esm");
        let race_a = FormKey {
            local: 0x0131e7,
            plugin,
        };
        let race_b = FormKey {
            local: 0x0131e8,
            plugin,
        };
        let skin = FormKey {
            local: 0x000980,
            plugin,
        };
        let armor_addon = FormKey {
            local: 0x000e87,
            plugin,
        };
        let body_part_data = FormKey {
            local: 0x000e88,
            plugin,
        };
        let race_plan = |source_race| CreatureRacePlan {
            source_race,
            source_plugin: "Skyrim.esm".to_string(),
            editor_id: None,
            skin: Some(skin),
            armor_addons: vec![armor_addon],
            body_models: Vec::new(),
            body_model_parts: Vec::new(),
            body_part_data: Some(body_part_data),
            project_paths: Vec::new(),
            skeleton_paths: Vec::new(),
            attack_events: Vec::new(),
            attack_contract: Vec::new(),
            attack_data: Vec::new(),
            attack_spells: Vec::new(),
            issues: Vec::new(),
        };
        let catalog = CreatureCorpusPlan {
            races: vec![race_plan(race_a), race_plan(race_b)],
            families: vec![super::super::creature_catalog::CreatureFamilyPlan {
                family_id: "bear".to_string(),
                key: super::super::creature_catalog::CreatureFamilyKey {
                    project_paths: Vec::new(),
                    skeleton_paths: Vec::new(),
                    body_models: Vec::new(),
                    attack_contract: Vec::new(),
                },
                races: vec![race_a, race_b],
                issues: Vec::new(),
            }],
            ..Default::default()
        };
        let motion = SkyrimCreatureMotionCatalog {
            families: Vec::new(),
            inventories: Vec::new(),
            clips: Vec::new(),
            accounting: Default::default(),
        };

        let requests =
            enumerate_skyrim_creature_ancillary_reservation_requests(&catalog, &motion, &interner)
                .expect("shared support reservations");
        let variants = requests
            .iter()
            .filter(|request| {
                matches!(
                    request.kind,
                    SkyrimCreatureAncillaryRecordKind::ArmorVariant
                        | SkyrimCreatureAncillaryRecordKind::ArmorAddonVariant
                        | SkyrimCreatureAncillaryRecordKind::BodyPartDataVariant
                )
            })
            .map(|request| (request.source_race, request.ordinal, request.kind))
            .collect::<Vec<_>>();

        assert_eq!(
            variants,
            vec![
                (
                    race_b,
                    skin.local,
                    SkyrimCreatureAncillaryRecordKind::ArmorVariant,
                ),
                (
                    race_b,
                    armor_addon.local,
                    SkyrimCreatureAncillaryRecordKind::ArmorAddonVariant,
                ),
                (
                    race_b,
                    body_part_data.local,
                    SkyrimCreatureAncillaryRecordKind::BodyPartDataVariant,
                ),
            ]
        );
    }

    #[test]
    fn actor_action_reservations_use_the_emitted_family_behavior_path() {
        let runtime_root = skyrim_family_runtime_root("wolf");
        assert!(runtime_root.starts_with("Actors\\B21_SkyrimCreature_"));
        assert_eq!(
            skyrim_family_root_behavior_path("wolf"),
            format!("{runtime_root}\\Behaviors\\SkyrimRootBehavior.hkx")
        );
    }

    #[test]
    fn unsupported_skyrim_ranged_attack_uses_verified_fo4_creature_weapon() {
        let CreatureAttackRecordProjection::RangedEquipment {
            equipment,
            projectile,
            ammunition,
        } = fo4_creature_ranged_fallback_projection()
        else {
            panic!("fallback must remain a ranged equipment projection");
        };
        assert_eq!(equipment.len(), 1);
        assert_eq!(equipment[0].signature, "WEAP");
        assert_eq!(
            equipment[0].form_key,
            TargetFormKey::new(FO4_CREATURE_RANGED_FALLBACK_WEAPON, "Fallout4.esm")
        );
        assert!(projectile.is_none());
        assert!(ammunition.is_none());
    }

    #[test]
    fn secondary_melee_attack_supplies_the_base_unarmed_identity() {
        let weapon = TargetFormKey::new(0x812, "Output.esm");
        let attacks = vec![
            CreatureAttackRecordVariant {
                id: "attack_00".to_string(),
                event: "attackSpell".to_string(),
                primary: true,
                projection: fo4_creature_ranged_fallback_projection(),
                damage_multiplier: 1.0,
                chance: 1.0,
                strike_angle: 0.0,
                action_point_cost: 0.0,
                target_data: CreatureAttackTargetData::default(),
            },
            CreatureAttackRecordVariant {
                id: "attack_01".to_string(),
                event: "attackBite".to_string(),
                primary: false,
                projection: CreatureAttackRecordProjection::MeleeUnarmed {
                    weapon_form_key: weapon.clone(),
                    weapon_editor_id: "B21_AttackBite".to_string(),
                    damage: 10,
                    reach: 1.0,
                    attack_seconds: 0.5,
                },
                damage_multiplier: 1.0,
                chance: 1.0,
                strike_angle: 0.0,
                action_point_cost: 0.0,
                target_data: CreatureAttackTargetData::default(),
            },
        ];

        assert_eq!(
            base_unarmed_weapon_identity(&attacks, "Output.esm", "B21_Unarmed"),
            (weapon, "B21_AttackBite".to_string())
        );
    }

    #[test]
    fn fallback_role_comes_from_the_graph_event_that_will_execute_it() {
        let roles = vec![
            crate::source_rig::CapabilityClipRole {
                role: crate::source_rig::CreatureClipRole::MeleeAttack,
                state_name: "Melee".to_string(),
                clip_name: "MeleeClip".to_string(),
                generator: Default::default(),
                trigger_event: Some("attackBite".to_string()),
                trigger_aliases: vec!["attackPower".to_string()],
                motion: Default::default(),
            },
            crate::source_rig::CapabilityClipRole {
                role: crate::source_rig::CreatureClipRole::ProjectileAttack,
                state_name: "Projectile".to_string(),
                clip_name: "ProjectileClip".to_string(),
                generator: Default::default(),
                trigger_event: Some("attackSpell".to_string()),
                trigger_aliases: Vec::new(),
                motion: Default::default(),
            },
        ];

        assert_eq!(
            graph_attack_role_for_event(&roles, "ATTACKPOWER"),
            Some(crate::source_rig::CreatureClipRole::MeleeAttack)
        );
        assert_eq!(
            graph_attack_role_for_event(&roles, "attackSpell"),
            Some(crate::source_rig::CreatureClipRole::ProjectileAttack)
        );
    }

    #[test]
    fn optional_live_vampire_and_werewolf_animation_skeletons_reemit() {
        let Some(data_root) = std::env::var_os("SKYRIMSE_CREATURE_DATA_DIR") else {
            return;
        };
        let data_root = PathBuf::from(data_root);
        for relative in [
            "Actors\\vampirelord\\character assets\\skeleton.hkx",
            "Actors\\werewolfbeast\\character assets\\skeleton.hkx",
        ] {
            let receipt = crate::phase::skeleton::convert_skyrim_source_owned_skeleton_artifact(
                &asset_path(&data_root, relative),
                "Actors\\B21_SkyrimEvidence\\CharacterAssets\\Skeleton.hkx",
            )
            .unwrap_or_else(|error| panic!("{relative}: {error}"));
            let roots = receipt
                .ordered_bone_names
                .iter()
                .zip(&receipt.parent_indices)
                .filter_map(|(name, parent)| (*parent < 0).then_some(name))
                .collect::<Vec<_>>();
            assert!(
                roots
                    .iter()
                    .any(|name| name.as_str() == receipt.skeleton_name),
                "{relative}: skeleton_name={:?} roots={roots:?}",
                receipt.skeleton_name
            );
        }
    }

    #[test]
    fn recursive_asset_references_reject_control_bytes_and_parent_traversal() {
        assert!(valid_recursive_asset_reference(
            "textures/effects/witchlight_n.dds"
        ));
        assert!(!valid_recursive_asset_reference("textures/\u{8}nor"));
        assert!(!valid_recursive_asset_reference("textures/../outside.dds"));
    }

    fn attack(
        ordinal: usize,
        event: &str,
        attack_spell: Option<super::super::creature_recipe::SkyrimRaceAttackFormReceipt>,
    ) -> SkyrimRaceAttackDataReceipt {
        SkyrimRaceAttackDataReceipt {
            ordinal,
            event: event.to_string(),
            damage_multiplier_bits: 1.5_f32.to_bits(),
            attack_chance_bits: 0.75_f32.to_bits(),
            attack_spell,
            attack_flags: 0x12,
            attack_angle_bits: 45.0_f32.to_bits(),
            strike_angle_bits: 30.0_f32.to_bits(),
            stagger_bits: 0.25_f32.to_bits(),
            attack_type: None,
            knockdown_bits: 0.0_f32.to_bits(),
            recovery_time_bits: 0.6_f32.to_bits(),
            stamina_multiplier_bits: 1.25_f32.to_bits(),
        }
    }

    fn npc_with_acbs(interner: &StringInterner, bytes: Vec<u8>) -> Record {
        let mut record = Record::new(
            SigCode::from_str("NPC_").unwrap(),
            FormKey {
                local: 0x123,
                plugin: interner.intern("Skyrim.esm"),
            },
        );
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("ACBS").unwrap(),
            value: FieldValue::Bytes(SmallVec::from_vec(bytes)),
        });
        record
    }

    #[test]
    fn skyrim_acbs_stats_use_exact_offsets_and_only_reject_inherited_stats() {
        let interner = StringInterner::new();
        let mut bytes = vec![0_u8; 24];
        bytes[6..8].copy_from_slice(&25_i16.to_le_bytes());
        bytes[8..10].copy_from_slice(&12_u16.to_le_bytes());
        bytes[20..22].copy_from_slice(&75_i16.to_le_bytes());
        let record = npc_with_acbs(&interner, bytes.clone());
        let stats = skyrim_npc_runtime_stats(&record, &interner).unwrap();
        assert_eq!(stats.level, 12);
        assert_eq!(stats.health, 125);
        assert_eq!(stats.action_points, 75);

        bytes[18..20].copy_from_slice(&0x0040_u16.to_le_bytes());
        assert!(
            skyrim_npc_runtime_stats(&npc_with_acbs(&interner, bytes.clone()), &interner).is_ok()
        );

        bytes[18..20].copy_from_slice(&0x0002_u16.to_le_bytes());
        let error = skyrim_npc_runtime_stats(&npc_with_acbs(&interner, bytes), &interner)
            .err()
            .unwrap();
        assert!(error.contains("exact inherited stat flattening"));
    }

    #[test]
    fn raw_lvln_template_entry_resolves_to_npc() {
        let interner = StringInterner::new();
        let npc = npc_with_acbs(&interner, vec![0_u8; 24]);
        let mut leveled_list = Record::new(
            SigCode::from_str("LVLN").unwrap(),
            FormKey {
                local: 0x456,
                plugin: interner.intern("Skyrim.esm"),
            },
        );
        let mut lvlo = vec![0_u8; 12];
        lvlo[4..8].copy_from_slice(&npc.form_key.local.to_le_bytes());
        leveled_list.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("LVLO").unwrap(),
            value: FieldValue::Bytes(SmallVec::from_vec(lvlo)),
        });
        let records = [&npc, &leveled_list]
            .into_iter()
            .map(|record| (record.form_key, record))
            .collect::<HashMap<_, _>>();

        let resolved =
            resolve_npc_template_target(leveled_list.form_key, &records, &interner).unwrap();

        assert_eq!(resolved.form_key, npc.form_key);
    }

    #[test]
    fn cnto_count_preserves_signed_struct_and_binary_counts() {
        let interner = StringInterner::new();
        let count = interner.intern("count");
        assert_eq!(
            cnto_count(
                &FieldValue::Struct(vec![(count, FieldValue::Int(-3))]),
                &interner,
            ),
            Some(-3)
        );
        let mut bytes = vec![0_u8; 8];
        bytes[4..8].copy_from_slice(&17_i32.to_le_bytes());
        assert_eq!(
            cnto_count(&FieldValue::Bytes(SmallVec::from_vec(bytes)), &interner),
            Some(17)
        );
        assert_eq!(cnto_count(&FieldValue::List(Vec::new()), &interner), None);
    }

    #[test]
    fn coed_values_preserve_union_bits_and_item_condition() {
        let interner = StringInterner::new();
        let global_or_rank = interner.intern("global_variable_required_rank");
        let item_condition = interner.intern("item_condition");
        assert_eq!(
            coed_values(
                &FieldValue::Struct(vec![
                    (global_or_rank, FieldValue::Uint(u64::from(u32::MAX))),
                    (item_condition, FieldValue::Float(0.75)),
                ]),
                &interner,
            ),
            Some((u32::MAX, 0.75))
        );
        let mut bytes = vec![0_u8; 12];
        bytes[4..8].copy_from_slice(&12_u32.to_le_bytes());
        bytes[8..12].copy_from_slice(&0.5_f32.to_le_bytes());
        assert_eq!(
            coed_values(&FieldValue::Bytes(SmallVec::from_vec(bytes)), &interner),
            Some((12, 0.5))
        );
    }

    #[test]
    fn source_field_index_reads_global_subrecord_order() {
        assert_eq!(source_field_index("CNTO[12].item", "CNTO"), Some(12));
        assert_eq!(source_field_index("SPLO[7]", "SPLO"), Some(7));
        assert_eq!(source_field_index("CNTO.item", "CNTO"), None);
    }

    #[test]
    fn legacy_controller_without_rigid_body_type_uses_fo4_invalid_enum_byte() {
        let receipt = SkyrimCreatureControllerReceipt {
            character_path: "Actors/Canine/WolfCharacter.hkx".to_string(),
            character_data_class: "hkbCharacterData".to_string(),
            controller_class: Some("hkbCharacterDataCharacterControllerInfo".to_string()),
            controller_cinfo_class: None,
            architecture: None,
            layout: SkyrimControllerLayoutEvidence::LegacyCharacterControllerInfo {
                contents_version: "hk_2010.2.0-r1".to_string(),
                character_data_signature: 0,
                controller_signature: 0,
            },
            collision_filter_info: Some(1),
            rigid_body_type: None,
            shape_type: None,
            capsule: None,
            axes: None,
            model: Some(
                super::super::creature_recipe::SkyrimControllerModelReceipt {
                    up_ms: [0.0, 0.0, 1.0, 0.0],
                    forward_ms: [0.0, 1.0, 0.0, 0.0],
                    right_ms: [1.0, 0.0, 0.0, 0.0],
                    scale: 1.0,
                },
            ),
            disposition:
                super::super::creature_recipe::SkyrimControllerDispositionReceipt::Complete,
        };

        let controller = controller_decl_from_receipt(&receipt).unwrap();

        assert_eq!(controller.rigid_body_type, 255);
    }

    #[test]
    fn source_free_and_shout_attacks_reserve_distinct_unarmed_weapons() {
        let interner = StringInterner::new();
        let race = FormKey {
            local: 0x123,
            plugin: interner.intern("Skyrim.esm"),
        };
        let shout = FormKey {
            local: 0x456,
            plugin: interner.intern("Skyrim.esm"),
        };
        let spell = FormKey {
            local: 0x789,
            plugin: interner.intern("Skyrim.esm"),
        };
        let catalog = CreatureCorpusPlan {
            races: vec![CreatureRacePlan {
                source_race: race,
                source_plugin: "Skyrim.esm".to_string(),
                editor_id: Some("FixtureRace".to_string()),
                skin: None,
                armor_addons: Vec::new(),
                body_models: Vec::new(),
                body_model_parts: Vec::new(),
                body_part_data: None,
                project_paths: Vec::new(),
                skeleton_paths: Vec::new(),
                attack_events: Vec::new(),
                attack_contract: Vec::new(),
                attack_data: vec![
                    super::super::creature_catalog::CreatureRaceAttackDataPlan {
                        ordinal: 0,
                        event: "attackBite".to_string(),
                        damage_multiplier_bits: 1.0_f32.to_bits(),
                        attack_chance_bits: 1.0_f32.to_bits(),
                        attack_spell: None,
                        attack_spell_signature: None,
                        attack_flags: 0,
                        attack_angle_bits: 0.0_f32.to_bits(),
                        strike_angle_bits: 0.0_f32.to_bits(),
                        stagger_bits: 0.0_f32.to_bits(),
                        attack_type: None,
                        attack_type_signature: None,
                        knockdown_bits: 0.0_f32.to_bits(),
                        recovery_time_bits: 0.5_f32.to_bits(),
                        stamina_multiplier_bits: 1.0_f32.to_bits(),
                    },
                    super::super::creature_catalog::CreatureRaceAttackDataPlan {
                        ordinal: 1,
                        event: "attackShout".to_string(),
                        damage_multiplier_bits: 1.0_f32.to_bits(),
                        attack_chance_bits: 1.0_f32.to_bits(),
                        attack_spell: Some(shout),
                        attack_spell_signature: Some("SHOU".to_string()),
                        attack_flags: 0,
                        attack_angle_bits: 0.0_f32.to_bits(),
                        strike_angle_bits: 0.0_f32.to_bits(),
                        stagger_bits: 0.0_f32.to_bits(),
                        attack_type: None,
                        attack_type_signature: None,
                        knockdown_bits: 0.0_f32.to_bits(),
                        recovery_time_bits: 0.5_f32.to_bits(),
                        stamina_multiplier_bits: 1.0_f32.to_bits(),
                    },
                    super::super::creature_catalog::CreatureRaceAttackDataPlan {
                        ordinal: 2,
                        event: "attackSpell".to_string(),
                        damage_multiplier_bits: 1.0_f32.to_bits(),
                        attack_chance_bits: 1.0_f32.to_bits(),
                        attack_spell: Some(spell),
                        attack_spell_signature: Some("SPEL".to_string()),
                        attack_flags: 0,
                        attack_angle_bits: 0.0_f32.to_bits(),
                        strike_angle_bits: 0.0_f32.to_bits(),
                        stagger_bits: 0.0_f32.to_bits(),
                        attack_type: None,
                        attack_type_signature: None,
                        knockdown_bits: 0.0_f32.to_bits(),
                        recovery_time_bits: 0.5_f32.to_bits(),
                        stamina_multiplier_bits: 1.0_f32.to_bits(),
                    },
                ],
                attack_spells: Vec::new(),
                issues: Vec::new(),
            }],
            ..Default::default()
        };
        let motion = SkyrimCreatureMotionCatalog {
            families: Vec::new(),
            inventories: Vec::new(),
            clips: Vec::new(),
            accounting: Default::default(),
        };

        let requests =
            enumerate_skyrim_creature_ancillary_reservation_requests(&catalog, &motion, &interner)
                .unwrap();

        assert_eq!(requests.len(), 4);
        assert_eq!(requests[0].event, "synthetic_npc");
        assert_eq!(
            requests[0].kind,
            SkyrimCreatureAncillaryRecordKind::SyntheticNpc
        );
        assert_eq!(requests[1].event, "attackBite");
        assert_eq!(requests[1].ordinal, 0);
        assert_eq!(requests[2].event, "attackShout");
        assert_eq!(requests[2].ordinal, 1);
        assert_eq!(requests[3].event, "attackSpell");
        assert_eq!(requests[3].ordinal, 2);
        assert!(
            requests[1..]
                .iter()
                .all(|request| request.kind == SkyrimCreatureAncillaryRecordKind::Weapon)
        );
    }

    #[test]
    fn attack_receipt_preserves_representable_atkd_and_keyword_policy() {
        let interner = StringInterner::new();
        let plugin = interner.intern("Skyrim.esm");
        let spell = FormKey {
            local: 0x456,
            plugin,
        };
        let keyword = FormKey {
            local: 0x789,
            plugin,
        };
        let mut attack = attack(
            4,
            "attackBite",
            Some(super::super::creature_recipe::SkyrimRaceAttackFormReceipt {
                source: spell.format(&interner),
                signature: "SPEL".to_string(),
            }),
        );
        attack.attack_type = Some(super::super::creature_recipe::SkyrimRaceAttackFormReceipt {
            source: keyword.format(&interner),
            signature: "KYWD".to_string(),
        });

        let receipt =
            attack_source_data_receipt(&attack, CreatureAttackSpellPolicy::DirectSpell, &interner)
                .unwrap();

        assert_eq!(
            receipt.damage_multiplier_bits,
            attack.damage_multiplier_bits
        );
        assert_eq!(
            receipt.stamina_multiplier_bits,
            attack.stamina_multiplier_bits
        );
        assert_eq!(receipt.attack_spell.unwrap().signature, "SPEL");
        assert!(matches!(
            receipt.attack_type_policy,
            CreatureAttackTypePolicy::RuntimeInertKeyword { .. }
        ));
        assert!(matches!(
            receipt.stamina_multiplier_policy,
            CreatureAttackStaminaPolicy::PreserveAsActionPointsMultiplier { .. }
        ));
    }

    #[test]
    fn multiple_unarmed_attacks_all_enter_the_record_key_plan() {
        let attacks = [
            CreatureAttackRecordVariant {
                id: "attack_00".to_string(),
                event: "attackBite".to_string(),
                primary: true,
                projection: CreatureAttackRecordProjection::MeleeUnarmed {
                    weapon_form_key: TargetFormKey::new(0x800, "Converted.esm"),
                    weapon_editor_id: "B21_AttackBite".to_string(),
                    damage: 10,
                    reach: 1.0,
                    attack_seconds: 0.5,
                },
                damage_multiplier: 1.0,
                chance: 1.0,
                strike_angle: 0.0,
                action_point_cost: 0.0,
                target_data: CreatureAttackTargetData::default(),
            },
            CreatureAttackRecordVariant {
                id: "attack_01".to_string(),
                event: "attackClaw".to_string(),
                primary: false,
                projection: CreatureAttackRecordProjection::MeleeUnarmed {
                    weapon_form_key: TargetFormKey::new(0x801, "Converted.esm"),
                    weapon_editor_id: "B21_AttackClaw".to_string(),
                    damage: 12,
                    reach: 1.0,
                    attack_seconds: 0.5,
                },
                damage_multiplier: 1.0,
                chance: 1.0,
                strike_angle: 0.0,
                action_point_cost: 0.0,
                target_data: CreatureAttackTargetData::default(),
            },
        ];

        let key_plan = melee_record_key_plan(&attacks);

        assert_eq!(key_plan.len(), 2);
        assert_eq!(key_plan[0].attack_id, "attack_00");
        assert!(key_plan[0].primary);
        assert_eq!(key_plan[1].attack_id, "attack_01");
        assert!(!key_plan[1].primary);
    }

    #[test]
    fn non_finite_atkd_values_are_terminal() {
        let mut attack = attack(0, "attackBite", None);
        attack.recovery_time_bits = f32::NAN.to_bits();
        assert!(
            validate_attack_target_values(&attack)
                .unwrap_err()
                .contains("non-finite recovery_time")
        );
    }

    #[test]
    fn actor_action_receipts_form_a_single_family_record_batch() {
        use crate::source_rig::{ActorActionKind, ActorActionRequirement};

        let interner = StringInterner::new();
        let requirement = ActorActionRequirement {
            kind: ActorActionKind::Melee,
            parent_editor_id: "ActionMelee".to_string(),
            parent_form_id: 0x0130_0B,
            behavior_path: "Actors\\B21_SkyrimCreatures\\fixture.hkx".to_string(),
            animation_event: "attackBite".to_string(),
        };
        let request = CreatureActorActionReservationRequest {
            family_id: "fixture_family".to_string(),
            requirement: requirement.clone(),
            editor_id: "B21_SkyrimActorAction_fixture".to_string(),
        };
        let receipt = CreatureActorActionReservationReceipt {
            family_id: request.family_id.clone(),
            requirement,
            editor_id: request.editor_id.clone(),
            target: FormKey {
                local: 0x800,
                plugin: interner.intern("Converted.esm"),
            },
        };

        let plans = actor_action_record_plans(&[request], &[receipt], &interner).unwrap();
        let family = plans.get("fixture_family").unwrap();
        assert_eq!(family.len(), 1);
        assert_eq!(family[0].editor_id, "B21_SkyrimActorAction_fixture");
        assert_eq!(family[0].form_key.local, 0x800);
    }
}
