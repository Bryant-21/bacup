use std::collections::{BTreeMap, BTreeSet};

use nif_core_native::creature_closure::{
    CreatureClosureReceipt, CreatureInputDisposition, CreatureNifRole,
};
use thiserror::Error;

use crate::source_rig::{
    CapabilityGraphManifest, ClipMotionPolicy, CreatureClipRole, CreatureGraphTemplate,
    CreatureManifest, CreatureRecordKeyPlan, CreatureRecordProjectionManifest, NoRagdollReason,
    RagdollDisposition, SourceCreatureIdentity, SourceRigArtifactReceipt, SourceRigArtifactRole,
    SourceRigBridgeError, SourceRigConvertedArtifactRequestReceipt, SourceRigExecutableRecipe,
    SourceRigFieldDecision, SourceRigFieldReceipt, SourceRigPairLedgerEvidence,
    SourceRigRecipeBridgeInput, SourceRigRecordBatchIntent, SourceRigSelectedFamilyEvidence,
    build_source_rig_executable_recipe,
};

use super::creature_recipe::{
    SKYRIM_CREATURE_RECIPE_VERSION, SKYRIM_LEGACY_CHARACTER_DATA_SIGNATURE,
    SKYRIM_LEGACY_CONTROLLER_SIGNATURE, SkyrimAssetClaimDisposition, SkyrimAssetRole,
    SkyrimControllerDispositionReceipt, SkyrimControllerLayoutEvidence,
    SkyrimCreatureCandidateDisposition, SkyrimCreatureClipReceipt, SkyrimCreatureClipRoleReceipt,
    SkyrimCreatureFamilyDisposition, SkyrimCreatureFamilyJob, SkyrimCreatureGraphVariantEvidence,
    SkyrimCreatureMotionRoleReceipt, SkyrimCreatureRecipeLedger, SkyrimCreatureSex,
    SkyrimRagdollCapabilityEvidence, SkyrimRootMotionReceipt,
};

pub const SKYRIM_SOURCE_RIG_BRIDGE_SCHEMA: &str = "skyrimse_creature_recipe_v3";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SkyrimSourceRigFamilySelection {
    pub sex: SkyrimCreatureSex,
    pub project_path: String,
    pub character_path: String,
    pub skeleton_path: String,
    pub body_nif_path: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SkyrimSourceRigFieldDecision {
    pub field: String,
    pub decision: SourceRigFieldDecision,
}

#[derive(Clone, Debug)]
pub struct SkyrimSourceRigBridgeInput {
    pub ledger: SkyrimCreatureRecipeLedger,
    pub family_id: String,
    pub selection: SkyrimSourceRigFamilySelection,
    pub creature_closure: CreatureClosureReceipt,
    pub creature_closure_request_blake3: String,
    pub artifact_requests: Vec<SourceRigConvertedArtifactRequestReceipt>,
    pub artifact_receipts: Vec<SourceRigArtifactReceipt>,
    pub rig: CreatureManifest,
    pub projection: CreatureRecordProjectionManifest,
    pub key_plan: CreatureRecordKeyPlan,
    pub batch_intent: SourceRigRecordBatchIntent,
    pub field_decisions: Vec<SkyrimSourceRigFieldDecision>,
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum SkyrimSourceRigBridgeError {
    #[error("invalid Skyrim source-rig bridge evidence ({code}): {message}")]
    Invalid { code: &'static str, message: String },
    #[error("invalid Skyrim creature recipe ledger: {0}")]
    Ledger(String),
    #[error("source-rig bridge rejected Skyrim evidence: {0}")]
    Bridge(String),
}

pub fn build_skyrim_source_rig_bridge_input(
    input: SkyrimSourceRigBridgeInput,
) -> Result<SourceRigRecipeBridgeInput, SkyrimSourceRigBridgeError> {
    let (mut bridge, decisions) = assemble_skyrim_source_rig_bridge_input(input)?;
    bridge.field_receipts = bind_field_decisions(&bridge, decisions)?;
    Ok(bridge)
}

pub fn required_skyrim_source_rig_field_names(
    mut input: SkyrimSourceRigBridgeInput,
) -> Result<Vec<String>, SkyrimSourceRigBridgeError> {
    input.field_decisions.clear();
    let (bridge, _) = assemble_skyrim_source_rig_bridge_input(input)?;
    Ok(bridge
        .required_field_receipts_from_policy("skyrim_bridge_field_inventory")
        .into_iter()
        .map(|receipt| receipt.field)
        .collect())
}

fn assemble_skyrim_source_rig_bridge_input(
    input: SkyrimSourceRigBridgeInput,
) -> Result<
    (
        SourceRigRecipeBridgeInput,
        Vec<SkyrimSourceRigFieldDecision>,
    ),
    SkyrimSourceRigBridgeError,
> {
    input
        .ledger
        .validate()
        .map_err(|error| SkyrimSourceRigBridgeError::Ledger(error.to_string()))?;
    if input.ledger.version != SKYRIM_CREATURE_RECIPE_VERSION
        || SKYRIM_SOURCE_RIG_BRIDGE_SCHEMA
            != format!("skyrimse_creature_recipe_v{}", input.ledger.version)
    {
        return Err(invalid(
            "ledger_schema",
            "Skyrim recipe schema version is not bridge-compatible",
        ));
    }
    validate_source_identity(&input.projection.source_primary_identity)?;
    let family = input
        .ledger
        .family_jobs
        .iter()
        .find(|family| family.family_id == input.family_id)
        .ok_or_else(|| invalid("family", "selected family is absent from the Skyrim ledger"))?;
    if !matches!(
        family.disposition,
        SkyrimCreatureFamilyDisposition::Ready { .. }
    ) {
        return Err(invalid(
            "family_disposition",
            "only a Ready Skyrim family can enter source-rig preparation",
        ));
    }
    if !family.graph.ready || !family.graph.blockers.is_empty() {
        return Err(invalid(
            "family_graph",
            "Ready family has contradictory graph readiness evidence",
        ));
    }
    let source_race = source_race_key(&input.projection.source_primary_identity);
    validate_source_race(family, &input.ledger, &source_race)?;
    let variant = selected_graph_variant(family, &input.selection)?;
    validate_body_variant(family, &input.selection, &source_race)?;
    let controller = family
        .controllers
        .iter()
        .filter(|controller| same_path(&controller.character_path, &variant.character_path))
        .collect::<Vec<_>>();
    let [controller] = controller.as_slice() else {
        return Err(invalid(
            "controller_selection",
            "selected character must have exactly one decoded controller receipt",
        ));
    };
    validate_controller(controller, &input.rig)?;
    validate_root(variant, &input.rig, &input.projection)?;
    let graph = graph_manifest(family)?;
    validate_graph_and_clips(family, &graph, &input.rig)?;
    validate_ragdoll(family, &input.rig)?;
    validate_race_data(family, &input.ledger, &source_race, &input.projection)?;
    validate_attack_template(graph.template, &input.projection)?;
    input
        .creature_closure
        .validate()
        .map_err(|error| invalid("nif_closure", error.to_string()))?;
    validate_nif_closure(
        &input.creature_closure,
        family,
        variant,
        &input.selection,
        &input.rig,
        &input.projection,
    )?;
    if input.creature_closure_request_blake3 != input.creature_closure.request_blake3 {
        return Err(invalid(
            "nif_closure_request",
            "NIF closure request commitment differs from its exact source identities and hashes",
        ));
    }
    validate_artifact_bindings(
        family,
        &input.artifact_requests,
        &input.artifact_receipts,
        &input.rig,
    )?;

    let canonical_json = input
        .ledger
        .canonical_json()
        .map_err(|error| SkyrimSourceRigBridgeError::Ledger(error.to_string()))?;
    let pair_ledger = SourceRigPairLedgerEvidence {
        schema: SKYRIM_SOURCE_RIG_BRIDGE_SCHEMA.to_string(),
        canonical_json: canonical_json.clone(),
        ledger_blake3: input.ledger.content_blake3.clone(),
        canonical_json_blake3: blake3::hash(canonical_json.as_bytes()).to_hex().to_string(),
        source_identity: input.projection.source_primary_identity.clone(),
        family_id: family.family_id.clone(),
    };
    let selected_family = SourceRigSelectedFamilyEvidence {
        source_games: vec!["skyrimse".to_string()],
        actual_root_bone: variant.actual_root_bone.clone(),
        visual_skeleton_nif: input.rig.visual_skeleton_nif.clone(),
        visual_creature_closure_target_path: closure_target_path(
            &input.creature_closure,
            CreatureNifRole::Skeleton,
            &variant.skeleton_path,
            &input.rig.visual_skeleton_nif,
        )?,
        animation_skeleton: input.rig.animation_skeleton.clone(),
        controller: input.rig.controller,
        graph: graph.clone(),
        clips: input.rig.clips.clone(),
        ragdoll: input.rig.ragdoll.clone(),
        race_data: input.projection.race_data.clone(),
    };
    let bridge = SourceRigRecipeBridgeInput {
        pair_ledger,
        selected_family,
        creature_closure: input.creature_closure,
        creature_closure_request_blake3: input.creature_closure_request_blake3,
        artifact_requests: input.artifact_requests,
        artifact_receipts: input.artifact_receipts,
        rig: input.rig,
        graph,
        projection: input.projection,
        key_plan: input.key_plan,
        batch_intent: input.batch_intent,
        field_receipts: Vec::new(),
        actor_action_records: Vec::new(),
    };
    Ok((bridge, input.field_decisions))
}

pub fn build_skyrim_source_rig_executable_recipe(
    input: SkyrimSourceRigBridgeInput,
) -> Result<SourceRigExecutableRecipe, SkyrimSourceRigBridgeError> {
    let bridge = build_skyrim_source_rig_bridge_input(input)?;
    build_source_rig_executable_recipe(bridge)
        .map_err(|error| SkyrimSourceRigBridgeError::Bridge(error.to_string()))
}

fn validate_source_identity(
    source: &SourceCreatureIdentity,
) -> Result<(), SkyrimSourceRigBridgeError> {
    if !source.namespace.eq_ignore_ascii_case("skyrimse")
        || source.plugin.trim().is_empty()
        || source.local_form_id == 0
        || source.local_form_id > 0x00ff_ffff
    {
        return Err(invalid(
            "source_identity",
            "source identity must be an explicit SkyrimSE plugin FormKey",
        ));
    }
    Ok(())
}

fn validate_source_race(
    family: &SkyrimCreatureFamilyJob,
    ledger: &SkyrimCreatureRecipeLedger,
    source_race: &str,
) -> Result<(), SkyrimSourceRigBridgeError> {
    if !family
        .source_races
        .iter()
        .any(|race| race.eq_ignore_ascii_case(source_race))
    {
        return Err(invalid(
            "source_race",
            "projection source race is not bound to the selected family",
        ));
    }
    let candidates = ledger
        .candidates
        .iter()
        .filter(|candidate| candidate.source_race.eq_ignore_ascii_case(source_race))
        .collect::<Vec<_>>();
    let [candidate] = candidates.as_slice() else {
        return Err(invalid(
            "source_race",
            "projection source race must resolve to exactly one candidate",
        ));
    };
    let SkyrimCreatureCandidateDisposition::Ready { family_job_ids } = &candidate.disposition
    else {
        return Err(invalid(
            "candidate_disposition",
            "projection source candidate is not Ready",
        ));
    };
    if !family_job_ids.iter().any(|id| id == &family.family_id) {
        return Err(invalid(
            "candidate_family",
            "Ready candidate does not select the requested family",
        ));
    }
    Ok(())
}

fn selected_graph_variant<'a>(
    family: &'a SkyrimCreatureFamilyJob,
    selection: &SkyrimSourceRigFamilySelection,
) -> Result<&'a SkyrimCreatureGraphVariantEvidence, SkyrimSourceRigBridgeError> {
    let variants = family
        .graph_variants
        .iter()
        .filter(|variant| {
            variant.sex == selection.sex
                && same_path(&variant.project_path, &selection.project_path)
                && same_path(&variant.character_path, &selection.character_path)
                && same_path(&variant.skeleton_path, &selection.skeleton_path)
        })
        .collect::<Vec<_>>();
    let [variant] = variants.as_slice() else {
        return Err(invalid(
            "graph_variant",
            "sex/project/character/skeleton selection must match exactly one graph variant",
        ));
    };
    Ok(variant)
}

fn validate_body_variant(
    family: &SkyrimCreatureFamilyJob,
    selection: &SkyrimSourceRigFamilySelection,
    source_race: &str,
) -> Result<(), SkyrimSourceRigBridgeError> {
    let matches = family
        .body_variants
        .iter()
        .filter(|variant| {
            variant.source_race.eq_ignore_ascii_case(source_race)
                && same_path(&variant.body_nif, &selection.body_nif_path)
                && (variant.sex == selection.sex || variant.sex == SkyrimCreatureSex::Ungendered)
        })
        .count();
    if matches == 0 {
        return Err(invalid(
            "body_variant",
            "source race, sex, and body NIF must match a conditioned body variant",
        ));
    }
    Ok(())
}

fn validate_controller(
    controller: &super::creature_recipe::SkyrimCreatureControllerReceipt,
    rig: &CreatureManifest,
) -> Result<(), SkyrimSourceRigBridgeError> {
    if controller.disposition != SkyrimControllerDispositionReceipt::Complete {
        return Err(invalid("controller", "decoded controller is not Complete"));
    }
    match &controller.layout {
        SkyrimControllerLayoutEvidence::LegacyCharacterControllerInfo {
            contents_version,
            character_data_signature,
            controller_signature,
        } if contents_version == "hk_2010.2.0-r1"
            && *character_data_signature == SKYRIM_LEGACY_CHARACTER_DATA_SIGNATURE
            && *controller_signature == SKYRIM_LEGACY_CONTROLLER_SIGNATURE => {}
        _ => {
            return Err(invalid(
                "controller_layout",
                "controller is not the exact decoded Skyrim legacy layout",
            ));
        }
    }
    let (Some(filter), Some(capsule), Some(_axes), Some(model)) = (
        controller.collision_filter_info,
        controller.capsule.as_ref(),
        controller.axes,
        controller.model.as_ref(),
    ) else {
        return Err(invalid(
            "controller_evidence",
            "controller filter, capsule, axes, and model basis must be explicit",
        ));
    };
    let rigid_body_type = controller.rigid_body_type.unwrap_or(u8::MAX as i32);
    if filter != rig.controller.collision_filter_info
        || rigid_body_type != rig.controller.rigid_body_type
        || !same_f32(model.scale, rig.controller.model_scale)
        || !same_vec4(model.up_ms, rig.controller.model_up_ms)
        || !same_vec4(model.forward_ms, rig.controller.model_forward_ms)
        || !same_vec4(model.right_ms, rig.controller.model_right_ms)
        || !same_f32(capsule.total_height, rig.capsule.height)
        || !same_f32(capsule.radius, rig.capsule.radius)
    {
        return Err(invalid(
            "controller_projection",
            "runtime controller basis, filter, type, or capsule differs from decoded evidence",
        ));
    }
    Ok(())
}

fn validate_root(
    variant: &SkyrimCreatureGraphVariantEvidence,
    rig: &CreatureManifest,
    projection: &CreatureRecordProjectionManifest,
) -> Result<(), SkyrimSourceRigBridgeError> {
    if variant.actual_root_bone.trim().is_empty()
        || variant.actual_root_bone.contains('/')
        || variant.actual_root_bone.contains('\\')
        || variant
            .actual_root_bone
            .to_ascii_lowercase()
            .ends_with(".nif")
        || variant
            .actual_root_bone
            .to_ascii_lowercase()
            .ends_with(".hkx")
    {
        return Err(invalid(
            "actual_root",
            "decoded root must be a bone name, never a path",
        ));
    }
    let roots = rig
        .animation_skeleton
        .bones
        .iter()
        .filter(|bone| bone.parent_index.is_none())
        .collect::<Vec<_>>();
    if !roots
        .iter()
        .any(|root| root.name == variant.actual_root_bone)
    {
        return Err(invalid(
            "actual_root",
            "decoded root is not a converted skeleton root",
        ));
    }
    let crate::source_rig::CreatureBodyPartProjection::RootOnly32 {
        node, vats_target, ..
    } = &projection.body_parts;
    if node != &variant.actual_root_bone || vats_target != &variant.actual_root_bone {
        return Err(invalid(
            "actual_root",
            "record projection root and VATS target differ from decoded root",
        ));
    }
    Ok(())
}

pub(crate) fn graph_manifest(
    family: &SkyrimCreatureFamilyJob,
) -> Result<CapabilityGraphManifest, SkyrimSourceRigBridgeError> {
    let idle_event = family
        .graph
        .idle_event
        .clone()
        .ok_or_else(|| invalid("graph_idle_event", "Ready graph has no explicit idle event"))?;
    Ok(CapabilityGraphManifest {
        template: family.graph.template,
        roles: family.graph.roles.clone(),
        candidate_attack_bindings: family.graph.candidate_attack_bindings.clone(),
        idle_event,
        explicit_events: family.graph.explicit_events.clone(),
        overlays: family.graph.overlays.clone(),
    })
}

fn validate_graph_and_clips(
    family: &SkyrimCreatureFamilyJob,
    graph: &CapabilityGraphManifest,
    rig: &CreatureManifest,
) -> Result<(), SkyrimSourceRigBridgeError> {
    let declared = rig
        .clips
        .iter()
        .map(|clip| clip.name.as_str())
        .collect::<BTreeSet<_>>();
    let referenced = graph
        .roles
        .iter()
        .flat_map(|role| role.generator.clip_names(&role.clip_name))
        .chain(graph.overlays.iter().map(|role| role.clip_name.as_str()))
        .collect::<BTreeSet<_>>();
    if declared != referenced || declared.len() != rig.clips.len() {
        return Err(invalid(
            "clip_binding",
            "converted clips must exactly equal graph-referenced Skyrim clip IDs",
        ));
    }
    for clip in &rig.clips {
        let receipt = clip_receipt(family, &clip.name)?;
        let original_skeleton_name =
            receipt.original_skeleton_name.as_deref().ok_or_else(|| {
                invalid(
                    "original_skeleton_name",
                    format!(
                        "clip {:?} lacks exact decoded original-skeleton evidence",
                        clip.name
                    ),
                )
            })?;
        if clip.binding.skeleton_path != rig.animation_skeleton.path
            || clip.binding.original_skeleton_name != original_skeleton_name
            || rig.animation_skeleton.runtime_name != original_skeleton_name
            || clip.binding.declared_transform_tracks == 0
            || clip.binding.declared_transform_tracks
                != clip.binding.transform_track_to_bone_indices.len()
            || clip
                .binding
                .transform_track_to_bone_indices
                .iter()
                .any(|index| *index >= rig.animation_skeleton.bones.len())
            || clip.binding.declared_float_tracks
                != clip.binding.float_track_to_float_slot_indices.len()
            || clip
                .binding
                .float_track_to_float_slot_indices
                .iter()
                .any(|index| *index >= rig.animation_skeleton.float_slots.len())
        {
            return Err(invalid(
                "clip_binding",
                format!(
                    "clip {:?} lacks an exact converted skeleton binding",
                    clip.name
                ),
            ));
        }
    }
    for role in &graph.roles {
        let mut receipt_events = BTreeSet::new();
        for clip_name in role.generator.clip_names(&role.clip_name) {
            let receipt = clip_receipt(family, clip_name)?;
            if !role_matches(role.role, &receipt.role)
                || role.motion != motion_policy(&receipt.root_motion)?
            {
                return Err(invalid(
                    "graph_role",
                    format!(
                        "role or root motion differs for generator clip {:?}",
                        clip_name
                    ),
                ));
            }
            receipt_events.extend(
                receipt
                    .trigger_event
                    .iter()
                    .chain(&receipt.trigger_aliases)
                    .map(|event| event.to_ascii_lowercase()),
            );
        }
        let graph_events = role
            .trigger_event
            .iter()
            .chain(&role.trigger_aliases)
            .map(|event| event.to_ascii_lowercase())
            .collect::<BTreeSet<_>>();
        if graph_events != receipt_events {
            return Err(invalid(
                "graph_role",
                format!(
                    "trigger event set differs for generator role {:?}",
                    role.clip_name
                ),
            ));
        }
    }
    for overlay in &graph.overlays {
        let receipt = clip_receipt(family, &overlay.clip_name)?;
        if !matches!(receipt.role, SkyrimCreatureClipRoleReceipt::SharedOverlay)
            || overlay.motion != motion_policy(&receipt.root_motion)?
        {
            return Err(invalid(
                "graph_overlay",
                format!("overlay evidence differs for {:?}", overlay.clip_name),
            ));
        }
    }
    Ok(())
}

fn clip_receipt<'a>(
    family: &'a SkyrimCreatureFamilyJob,
    clip_name: &str,
) -> Result<&'a SkyrimCreatureClipReceipt, SkyrimSourceRigBridgeError> {
    let clips = family
        .clips
        .iter()
        .filter(|clip| clip.clip_id == clip_name)
        .collect::<Vec<_>>();
    let [clip] = clips.as_slice() else {
        return Err(invalid(
            "clip_evidence",
            format!("graph clip {clip_name:?} does not have one exact ledger receipt"),
        ));
    };
    Ok(clip)
}

fn motion_policy(
    root_motion: &SkyrimRootMotionReceipt,
) -> Result<ClipMotionPolicy, SkyrimSourceRigBridgeError> {
    match root_motion {
        SkyrimRootMotionReceipt::Stationary { .. } => Ok(ClipMotionPolicy::default()),
        SkyrimRootMotionReceipt::Sampled { sample_count, .. } => Ok(ClipMotionPolicy {
            animation_driven: true,
            extracted_planar_reference_frames: *sample_count,
        }),
        SkyrimRootMotionReceipt::Unknown | SkyrimRootMotionReceipt::Unsupported { .. } => {
            Err(invalid(
                "root_motion",
                "Ready graph references unresolved root motion",
            ))
        }
    }
}

fn role_matches(role: CreatureClipRole, receipt: &SkyrimCreatureClipRoleReceipt) -> bool {
    let SkyrimCreatureClipRoleReceipt::Role(receipt) = receipt else {
        return false;
    };
    matches!(
        (role, receipt),
        (
            CreatureClipRole::Idle,
            SkyrimCreatureMotionRoleReceipt::Idle
        ) | (
            CreatureClipRole::GroundForward,
            SkyrimCreatureMotionRoleReceipt::GroundLocomotion
        ) | (
            CreatureClipRole::TurnLeft90,
            SkyrimCreatureMotionRoleReceipt::TurnLeft
        ) | (
            CreatureClipRole::TurnRight90,
            SkyrimCreatureMotionRoleReceipt::TurnRight
        ) | (
            CreatureClipRole::MeleeAttack,
            SkyrimCreatureMotionRoleReceipt::MeleeAttack
        ) | (
            CreatureClipRole::ProjectileAttack,
            SkyrimCreatureMotionRoleReceipt::RangedAttack
                | SkyrimCreatureMotionRoleReceipt::ProjectileAttack
                | SkyrimCreatureMotionRoleReceipt::SpellAttack
        ) | (
            CreatureClipRole::SwimIdle,
            SkyrimCreatureMotionRoleReceipt::SwimIdle
        ) | (
            CreatureClipRole::SwimForward,
            SkyrimCreatureMotionRoleReceipt::SwimLocomotion
        ) | (
            CreatureClipRole::FlyIdle,
            SkyrimCreatureMotionRoleReceipt::FlyIdle
        ) | (
            CreatureClipRole::FlyForward,
            SkyrimCreatureMotionRoleReceipt::FlyLocomotion
        ) | (
            CreatureClipRole::StationaryIdle,
            SkyrimCreatureMotionRoleReceipt::StationaryIdle
        ) | (
            CreatureClipRole::ContinuousAttackStart,
            SkyrimCreatureMotionRoleReceipt::MechanicalStart
        ) | (
            CreatureClipRole::ContinuousAttackLoop,
            SkyrimCreatureMotionRoleReceipt::MechanicalLoop
        ) | (
            CreatureClipRole::ContinuousAttackStop,
            SkyrimCreatureMotionRoleReceipt::MechanicalStop
        )
    )
}

fn validate_ragdoll(
    family: &SkyrimCreatureFamilyJob,
    rig: &CreatureManifest,
) -> Result<(), SkyrimSourceRigBridgeError> {
    if matches!(
        rig.ragdoll,
        RagdollDisposition::NoRagdoll {
            reason: NoRagdollReason::Deferred
        }
    ) {
        return Err(invalid(
            "ragdoll",
            "Skyrim execution preparation never accepts deferred ragdoll evidence",
        ));
    }
    match (&family.ragdoll, &rig.ragdoll) {
        (
            SkyrimRagdollCapabilityEvidence::Present { paths },
            RagdollDisposition::SourceOwned {
                runtime_path,
                receipt,
            },
        ) if !paths.is_empty()
            && !runtime_path.trim().is_empty()
            && receipt.byte_len > 0
            && is_blake3(&receipt.blake3) =>
        {
            Ok(())
        }
        (
            SkyrimRagdollCapabilityEvidence::NotApplicable { .. },
            RagdollDisposition::NoRagdoll {
                reason:
                    NoRagdollReason::NotApplicable
                    | NoRagdollReason::SourceHasNoRagdoll
                    | NoRagdollReason::UnsupportedSourceRagdoll,
            },
        ) => Ok(()),
        _ => Err(invalid(
            "ragdoll",
            "runtime ragdoll disposition differs from the Ready Skyrim evidence",
        )),
    }
}

fn validate_race_data(
    family: &SkyrimCreatureFamilyJob,
    ledger: &SkyrimCreatureRecipeLedger,
    source_race: &str,
    projection: &CreatureRecordProjectionManifest,
) -> Result<(), SkyrimSourceRigBridgeError> {
    let candidate = ledger
        .candidates
        .iter()
        .find(|candidate| candidate.source_race.eq_ignore_ascii_case(source_race))
        .expect("source race uniqueness validated above");
    if candidate.race_data.field_receipts != ledger.race_data_policy.fields {
        return Err(invalid(
            "race_data_receipts",
            "candidate RACE.DATA receipts differ from the ledger policy receipt",
        ));
    }
    let Some(derivation) = &candidate.race_data.derivation else {
        return Err(invalid(
            "race_data",
            "Ready candidate lacks an explicit RACE.DATA derivation",
        ));
    };
    let crate::source_rig::RaceDataMapping::Mapped { target } = &derivation.mapping else {
        return Err(invalid(
            "race_data",
            "Ready candidate RACE.DATA is not fully mapped",
        ));
    };
    if target != &projection.race_data
        || !family.source_races.iter().any(|race| race == source_race)
    {
        return Err(invalid(
            "race_data",
            "projected RACE.DATA or race ownership differs from Skyrim evidence",
        ));
    }
    Ok(())
}

fn validate_attack_template(
    template: CreatureGraphTemplate,
    projection: &CreatureRecordProjectionManifest,
) -> Result<(), SkyrimSourceRigBridgeError> {
    let has_melee = projection.attacks.iter().any(|attack| {
        matches!(
            attack.projection,
            crate::source_rig::CreatureAttackRecordProjection::MeleeUnarmed { .. }
        )
    });
    if !matches!(
        template,
        CreatureGraphTemplate::GroundMelee
            | CreatureGraphTemplate::GroundMeleeRanged
            | CreatureGraphTemplate::GroundSwim
            | CreatureGraphTemplate::GroundFly
            | CreatureGraphTemplate::Swim
            | CreatureGraphTemplate::Fly
    ) && has_melee
    {
        return Err(invalid(
            "attack_template",
            "non-melee graph template cannot project a synthetic unarmed WEAP",
        ));
    }
    Ok(())
}

fn validate_nif_closure(
    closure: &CreatureClosureReceipt,
    family: &SkyrimCreatureFamilyJob,
    variant: &SkyrimCreatureGraphVariantEvidence,
    selection: &SkyrimSourceRigFamilySelection,
    rig: &CreatureManifest,
    projection: &CreatureRecordProjectionManifest,
) -> Result<(), SkyrimSourceRigBridgeError> {
    if !closure.source_game.eq_ignore_ascii_case("skyrimse")
        || !closure.target_game.eq_ignore_ascii_case("fo4")
        || !is_blake3(&closure.receipt_hash)
    {
        return Err(invalid(
            "nif_closure",
            "NIF closure lacks exact SkyrimSE-to-FO4 identity or hash",
        ));
    }
    for input in &closure.inputs {
        let role = match input.role {
            CreatureNifRole::Skeleton => SkyrimAssetRole::VisualSkeletonNif,
            CreatureNifRole::Body => SkyrimAssetRole::BodyNif,
        };
        let expected = closure_claim_hash(family, &input.source_data_relative_path, role)?;
        if input.source_byte_len == 0 || !input.source_blake3.eq_ignore_ascii_case(&expected) {
            return Err(invalid(
                "nif_source_hash",
                format!(
                    "{role:?} source {:?} differs from the exact ledger asset bytes",
                    input.source_data_relative_path
                ),
            ));
        }
    }
    validate_nif_input(
        closure,
        CreatureNifRole::Skeleton,
        &variant.skeleton_path,
        &rig.visual_skeleton_nif,
    )?;
    validate_nif_input(
        closure,
        CreatureNifRole::Body,
        &selection.body_nif_path,
        &projection.base.body_nif,
    )?;
    Ok(())
}

fn closure_claim_hash(
    family: &SkyrimCreatureFamilyJob,
    path: &str,
    role: SkyrimAssetRole,
) -> Result<String, SkyrimSourceRigBridgeError> {
    let claims = family
        .asset_claims
        .iter()
        .filter(|claim| claim.role == role && same_data_path(&claim.path, path))
        .collect::<Vec<_>>();
    let [claim] = claims.as_slice() else {
        return Err(invalid(
            "nif_source_claim",
            format!("{role:?} source {path:?} lacks one exact ledger asset claim"),
        ));
    };
    let SkyrimAssetClaimDisposition::Present { blake3 } = &claim.disposition else {
        return Err(invalid(
            "nif_source_claim",
            format!("{role:?} source {path:?} is not present in the ledger"),
        ));
    };
    if !is_blake3(blake3) {
        return Err(invalid(
            "nif_source_hash",
            format!("{role:?} source {path:?} lacks a valid ledger BLAKE3"),
        ));
    }
    Ok(blake3.to_ascii_lowercase())
}

fn validate_nif_input(
    closure: &CreatureClosureReceipt,
    role: CreatureNifRole,
    source_path: &str,
    runtime_path: &str,
) -> Result<(), SkyrimSourceRigBridgeError> {
    let target = closure_target_path(closure, role, source_path, runtime_path)?;
    if !closure.artifacts.iter().any(|artifact| {
        artifact.target_data_relative_path == target
            && artifact.byte_len > 0
            && is_blake3(&artifact.fingerprint)
    }) {
        return Err(invalid(
            "nif_closure_artifact",
            format!("{role:?} converted NIF artifact is absent or unhashed"),
        ));
    }
    Ok(())
}

fn closure_target_path(
    closure: &CreatureClosureReceipt,
    role: CreatureNifRole,
    source_path: &str,
    runtime_path: &str,
) -> Result<String, SkyrimSourceRigBridgeError> {
    let inputs = closure
        .inputs
        .iter()
        .filter(|input| {
            input.role == role
                && same_data_path(&input.source_data_relative_path, source_path)
                && same_data_path(&input.target_data_relative_path, runtime_path)
        })
        .collect::<Vec<&CreatureInputDisposition>>();
    if inputs.len() != 1 {
        return Err(invalid(
            "nif_closure_path",
            format!("{role:?} source and target NIF paths lack one exact closure receipt"),
        ));
    }
    Ok(inputs[0].target_data_relative_path.clone())
}

fn validate_artifact_bindings(
    family: &SkyrimCreatureFamilyJob,
    requests: &[SourceRigConvertedArtifactRequestReceipt],
    receipts: &[SourceRigArtifactReceipt],
    rig: &CreatureManifest,
) -> Result<(), SkyrimSourceRigBridgeError> {
    if requests.iter().any(|request| {
        !request.source_game.eq_ignore_ascii_case("skyrimse")
            || !is_blake3(&request.source_evidence_blake3)
            || !is_blake3(&request.conversion_request_blake3)
    }) || receipts
        .iter()
        .any(|receipt| receipt.byte_len == 0 || !is_blake3(&receipt.blake3))
    {
        return Err(invalid(
            "artifact_hash",
            "converted artifact requests and receipts require exact non-empty BLAKE3 evidence",
        ));
    }
    let [animation_skeleton_path] = family.inventory.animation_skeleton_paths.as_slice() else {
        return Err(invalid(
            "artifact_source_evidence",
            "selected family must have one exact animation skeleton source",
        ));
    };
    let skeleton_hash = claim_hash(
        family,
        animation_skeleton_path,
        SkyrimAssetRole::AnimationSkeleton,
    )?;
    require_request_hash(
        requests,
        &SourceRigArtifactRole::AnimationSkeleton,
        &skeleton_hash,
    )?;
    for role in &family.graph.roles {
        let clip = clip_receipt(family, &role.clip_name)?;
        let hash = claim_hash(family, &clip.clip_path, SkyrimAssetRole::AnimationClip)?;
        require_request_hash(
            requests,
            &SourceRigArtifactRole::AnimationClip {
                clip_name: role.clip_name.clone(),
            },
            &hash,
        )?;
    }
    for overlay in &family.graph.overlays {
        let clip = clip_receipt(family, &overlay.clip_name)?;
        let hash = claim_hash(family, &clip.clip_path, SkyrimAssetRole::AnimationClip)?;
        require_request_hash(
            requests,
            &SourceRigArtifactRole::AnimationClip {
                clip_name: overlay.clip_name.clone(),
            },
            &hash,
        )?;
    }
    if let RagdollDisposition::SourceOwned { receipt, .. } = &rig.ragdoll {
        let SkyrimRagdollCapabilityEvidence::Present { paths } = &family.ragdoll else {
            return Err(invalid(
                "ragdoll",
                "source-owned ragdoll lacks ledger evidence",
            ));
        };
        let hashes = paths
            .iter()
            .map(|path| claim_hash(family, path, SkyrimAssetRole::Ragdoll))
            .collect::<Result<BTreeSet<_>, _>>()?;
        let request = unique_request(requests, &SourceRigArtifactRole::Ragdoll)?;
        if !hashes.contains(&request.source_evidence_blake3)
            || !receipts.iter().any(|artifact| {
                artifact.role == SourceRigArtifactRole::Ragdoll
                    && artifact.byte_len == receipt.byte_len
                    && artifact.blake3.eq_ignore_ascii_case(&receipt.blake3)
            })
        {
            return Err(invalid(
                "ragdoll_receipt",
                "converted ragdoll bytes are not bound to a declared Skyrim source ragdoll",
            ));
        }
    }
    Ok(())
}

fn claim_hash(
    family: &SkyrimCreatureFamilyJob,
    path: &str,
    role: SkyrimAssetRole,
) -> Result<String, SkyrimSourceRigBridgeError> {
    let claims = family
        .asset_claims
        .iter()
        .filter(|claim| claim.role == role && same_path(&claim.path, path))
        .collect::<Vec<_>>();
    let [claim] = claims.as_slice() else {
        return Err(invalid(
            "asset_claim",
            format!("{role:?} path {path:?} lacks one exact recursive asset claim"),
        ));
    };
    let SkyrimAssetClaimDisposition::Present { blake3 } = &claim.disposition else {
        return Err(invalid(
            "asset_claim",
            format!("{role:?} path {path:?} is not present"),
        ));
    };
    if !is_blake3(blake3) {
        return Err(invalid(
            "asset_hash",
            format!("{role:?} path {path:?} lacks a valid BLAKE3"),
        ));
    }
    Ok(blake3.to_ascii_lowercase())
}

fn require_request_hash(
    requests: &[SourceRigConvertedArtifactRequestReceipt],
    role: &SourceRigArtifactRole,
    expected: &str,
) -> Result<(), SkyrimSourceRigBridgeError> {
    let request = unique_request(requests, role)?;
    if !request
        .source_evidence_blake3
        .eq_ignore_ascii_case(expected)
    {
        return Err(invalid(
            "artifact_source_evidence",
            format!("converted request for {role:?} differs from the ledger asset hash"),
        ));
    }
    Ok(())
}

fn unique_request<'a>(
    requests: &'a [SourceRigConvertedArtifactRequestReceipt],
    role: &SourceRigArtifactRole,
) -> Result<&'a SourceRigConvertedArtifactRequestReceipt, SkyrimSourceRigBridgeError> {
    let matches = requests
        .iter()
        .filter(|request| &request.role == role)
        .collect::<Vec<_>>();
    let [request] = matches.as_slice() else {
        return Err(invalid(
            "artifact_request",
            format!("{role:?} must have exactly one converted request"),
        ));
    };
    Ok(request)
}

fn bind_field_decisions(
    bridge: &SourceRigRecipeBridgeInput,
    decisions: Vec<SkyrimSourceRigFieldDecision>,
) -> Result<Vec<SourceRigFieldReceipt>, SkyrimSourceRigBridgeError> {
    let mut by_field = BTreeMap::new();
    for decision in decisions {
        if decision.field.trim().is_empty()
            || by_field
                .insert(decision.field.clone(), decision.decision)
                .is_some()
        {
            return Err(invalid(
                "field_decision",
                "field decisions must have unique explicit field names",
            ));
        }
    }
    let mut receipts = bridge.required_field_receipts_from_policy("skyrim_bridge_hash_binding");
    for receipt in &mut receipts {
        receipt.decision = by_field.remove(&receipt.field).ok_or_else(|| {
            invalid(
                "field_decision",
                format!("missing explicit decision for {:?}", receipt.field),
            )
        })?;
    }
    if let Some(field) = by_field.keys().next() {
        return Err(invalid(
            "field_decision",
            format!("decision names non-persisted field {field:?}"),
        ));
    }
    Ok(receipts)
}

fn source_race_key(source: &SourceCreatureIdentity) -> String {
    format!("{:06X}@{}", source.local_form_id, source.plugin)
}

fn same_vec4(left: [f32; 4], right: [f32; 4]) -> bool {
    left.into_iter()
        .zip(right)
        .all(|(left, right)| same_f32(left, right))
}

fn same_f32(left: f32, right: f32) -> bool {
    left.to_bits() == right.to_bits()
}

fn same_path(left: &str, right: &str) -> bool {
    normalized_path(left) == normalized_path(right)
}

fn same_data_path(left: &str, right: &str) -> bool {
    normalized_path(left)
        .strip_prefix("meshes\\")
        .unwrap_or(&normalized_path(left))
        == normalized_path(right)
            .strip_prefix("meshes\\")
            .unwrap_or(&normalized_path(right))
}

fn normalized_path(value: &str) -> String {
    value.trim().replace('/', "\\").to_ascii_lowercase()
}

fn is_blake3(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn invalid(code: &'static str, message: impl Into<String>) -> SkyrimSourceRigBridgeError {
    SkyrimSourceRigBridgeError::Invalid {
        code,
        message: message.into(),
    }
}

impl From<SourceRigBridgeError> for SkyrimSourceRigBridgeError {
    fn from(error: SourceRigBridgeError) -> Self {
        Self::Bridge(error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nif_core_native::creature_closure::{
        CreatureClosureRequest, CreatureNifInput, stage_creature_nif_closure,
    };
    use nif_core_native::model::NifFile;

    use crate::skyrimse_fo4_runtime::creature_race_data::{
        CreatureRaceDataDisposition, CreatureRaceDataInputField,
    };
    use crate::source_rig::race_data::{
        CapsuleEvidence, ControllerArchitecture, Fo4RaceDataDerivation, Fo4RaceDataScalePolicy,
        MeasurementAxis,
    };
    use crate::source_rig::{
        BoneDecl, Capsule, ClipBinding, ClipDecl, CreatureAttackRecordProjection,
        CreatureAttackRecordVariant, CreatureAttackTargetData, CreatureBodyPartProjection,
        CreatureControllerDecl, CreatureMeleeRecordKeyPlan, CreatureNpcRecordKeyPlan,
        CreatureNpcRecordVariant, CreaturePrimaryRecordMapping, CreatureRecordEditorIds,
        CreatureRecordFormKeys, CreatureRecordManifest, Fo4RaceDataTarget, Fo4RaceFlag,
        Fo4RaceFlag2, Fo4RaceSize, GraphDeclarations, MvpGraphManifest, MvpMotionManifest,
        PropertyDecl, RaceDataMapping, ScaffoldPaths, SourceRigArtifactProvenance, TargetFormKey,
        VariableDecl,
    };

    use super::super::creature_recipe::{
        SkyrimCreatureCapability, SkyrimCreatureControllerReceipt, SkyrimCreatureGraphEvidence,
        SkyrimCreatureInventoryReceipt, SkyrimCreatureRaceDataReceipt,
        SkyrimCreatureRecipeAccounting, SkyrimExplicitCapabilityEvidence,
        SkyrimRaceDataFieldProvenance, SkyrimRaceDataFieldReceipt, SkyrimRaceDataPolicyReceipt,
        SkyrimRecursiveAssetClaim,
    };

    fn target(local: u32) -> TargetFormKey {
        TargetFormKey::new(local, "B21_Test.esp")
    }

    fn race_data() -> Fo4RaceDataTarget {
        Fo4RaceDataTarget {
            male_height: 1.0,
            female_height: 1.0,
            male_default_weight: [0.0; 3],
            female_default_weight: [0.0; 3],
            flags: vec![Fo4RaceFlag::Walks],
            acceleration_rate: 1.0,
            deceleration_rate: 1.0,
            size: Fo4RaceSize::Medium,
            injured_health_percent: 0.2,
            body_biped_object: 3,
            aim_angle_tolerance: 30.0,
            flight_radius: 0.0,
            angular_acceleration_rate: 1.0,
            angular_tolerance: 30.0,
            flags_2: vec![Fo4RaceFlag2::UseQuadrupedController],
            xp_value: 0,
            orientation_limit_pitch: 0.0,
            orientation_limit_roll: 0.0,
        }
    }

    fn record_manifest(body_nif: String) -> CreatureRecordManifest {
        CreatureRecordManifest {
            target_plugin: "B21_Test.esp".to_string(),
            editor_id_prefix: "B21_".to_string(),
            form_keys: CreatureRecordFormKeys {
                race: target(0x800),
                npc: target(0x801),
                skin: target(0x802),
                armor_addon: target(0x803),
                body_part_data: target(0x804),
                unarmed_weapon: target(0x805),
            },
            editor_ids: CreatureRecordEditorIds {
                race: "B21_WolfRace".to_string(),
                npc: "B21_WolfNPC".to_string(),
                skin: "B21_WolfSkin".to_string(),
                armor_addon: "B21_WolfBodyAA".to_string(),
                body_part_data: "B21_WolfBodyPartData".to_string(),
                unarmed_weapon: "B21_WolfUnarmed".to_string(),
            },
            display_name: "Wolf".to_string(),
            body_nif,
        }
    }

    fn declarations(graph: &CapabilityGraphManifest) -> GraphDeclarations {
        GraphDeclarations {
            events: graph.explicit_events.clone(),
            variables: Vec::<VariableDecl>::new(),
            character_properties: Vec::<PropertyDecl>::new(),
        }
    }

    fn hash(value: &str) -> String {
        blake3::hash(value.as_bytes()).to_hex().to_string()
    }

    fn ledger_hash(ledger: &mut SkyrimCreatureRecipeLedger) {
        ledger.content_blake3.clear();
        ledger.content_blake3 = blake3::hash(&serde_json::to_vec(ledger).unwrap())
            .to_hex()
            .to_string();
        ledger.validate().unwrap();
    }

    fn runtime_path(target_path: &str) -> String {
        target_path
            .replace('/', "\\")
            .strip_prefix("meshes\\")
            .unwrap()
            .to_string()
    }

    fn refresh_field_decisions(input: &mut SkyrimSourceRigBridgeInput) {
        input.field_decisions = required_skyrim_source_rig_field_names(input.clone())
            .unwrap()
            .into_iter()
            .map(|field| SkyrimSourceRigFieldDecision {
                field,
                decision: SourceRigFieldDecision::Source {
                    source_game: "skyrimse".to_string(),
                    source_field: "skyrim_recipe_evidence".to_string(),
                },
            })
            .collect();
    }

    fn fixture() -> SkyrimSourceRigBridgeInput {
        let temp = tempfile::tempdir().unwrap().keep();
        let source_root = temp.join("source");
        let skeleton_source = source_root.join("meshes/BridgeWolf/skeleton.nif");
        let body_source = source_root.join("meshes/BridgeWolf/body.nif");
        std::fs::create_dir_all(skeleton_source.parent().unwrap()).unwrap();
        let mut skeleton_nif = NifFile::new("skyrimse");
        skeleton_nif.rebuild_header();
        skeleton_nif.save(Some(skeleton_source)).unwrap();
        let mut body_nif = NifFile::new("skyrimse");
        body_nif.rebuild_header();
        body_nif.save(Some(body_source)).unwrap();
        let closure = stage_creature_nif_closure(&CreatureClosureRequest {
            source_game: "skyrimse".to_string(),
            source_data_root: source_root,
            private_staging_root: temp.join("stage"),
            target_namespace: "Actors".to_string(),
            texture_fallbacks: std::collections::BTreeMap::new(),
            inputs: vec![
                CreatureNifInput {
                    role: CreatureNifRole::Skeleton,
                    source_data_relative_path: "meshes/BridgeWolf/skeleton.nif".to_string(),
                    source_owner: "wolf-ledger".to_string(),
                    body_variant: None,
                },
                CreatureNifInput {
                    role: CreatureNifRole::Body,
                    source_data_relative_path: "meshes/BridgeWolf/body.nif".to_string(),
                    source_owner: "wolf-ledger".to_string(),
                    body_variant: Some("male".to_string()),
                },
            ],
        })
        .unwrap();
        let skeleton_source_hash = closure
            .inputs
            .iter()
            .find(|input| input.role == CreatureNifRole::Skeleton)
            .unwrap()
            .source_blake3
            .clone();
        let body_source_hash = closure
            .inputs
            .iter()
            .find(|input| input.role == CreatureNifRole::Body)
            .unwrap()
            .source_blake3
            .clone();
        let closure_request_blake3 = closure.request_blake3.clone();
        let skeleton_target = closure
            .inputs
            .iter()
            .find(|input| input.role == CreatureNifRole::Skeleton)
            .unwrap()
            .target_data_relative_path
            .clone();
        let body_target = closure
            .inputs
            .iter()
            .find(|input| input.role == CreatureNifRole::Body)
            .unwrap()
            .target_data_relative_path
            .clone();

        let clip_specs = [
            ("idle", "idle.hkx", true),
            ("walk", "walk.hkx", true),
            ("turn_left", "turn_left.hkx", false),
            ("turn_right", "turn_right.hkx", false),
            ("attack", "attack.hkx", false),
        ];
        let animation_skeleton_path = "Actors\\BridgeWolf\\Skeleton.hkx".to_string();
        let clips = clip_specs
            .iter()
            .map(|(name, file, looping)| ClipDecl {
                name: (*name).to_string(),
                path: format!("Actors\\BridgeWolf\\Animations\\{file}"),
                binding: ClipBinding {
                    skeleton_path: animation_skeleton_path.clone(),
                    original_skeleton_name: "BridgeWolfSkeleton".to_string(),
                    declared_transform_tracks: 2,
                    transform_track_to_bone_indices: vec![0, 1],
                    declared_float_tracks: 0,
                    float_track_to_float_slot_indices: Vec::new(),
                },
                looping: *looping,
            })
            .collect::<Vec<_>>();
        let mvp = MvpGraphManifest {
            idle_clip: "idle".to_string(),
            walk_forward_clip: "walk".to_string(),
            turn_left_90_clip: "turn_left".to_string(),
            turn_right_90_clip: "turn_right".to_string(),
            attack_1_clip: "attack".to_string(),
            melee_event: "meleeWolfBite".to_string(),
        };
        let graph = CapabilityGraphManifest::from_mvp(&mvp, &MvpMotionManifest::default());
        let controller_decl = CreatureControllerDecl {
            collision_filter_info: 7,
            rigid_body_type: 8,
            model_up_ms: [0.0, 0.0, 1.0, 0.0],
            model_forward_ms: [0.0, 1.0, 0.0, 0.0],
            model_right_ms: [-1.0, 0.0, 0.0, 0.0],
            model_scale: 1.0,
        };
        let rig = CreatureManifest {
            creature_name: "BridgeWolf".to_string(),
            visual_skeleton_nif: runtime_path(&skeleton_target),
            animation_skeleton: crate::source_rig::SkeletonDecl {
                path: animation_skeleton_path.clone(),
                runtime_name: "BridgeWolfSkeleton".to_string(),
                bones: vec![
                    BoneDecl {
                        name: "NPC Root [Root]".to_string(),
                        parent_index: None,
                    },
                    BoneDecl {
                        name: "NPC COM [COM ]".to_string(),
                        parent_index: Some(0),
                    },
                ],
                float_slots: Vec::new(),
            },
            controller: controller_decl,
            ragdoll: RagdollDisposition::NoRagdoll {
                reason: NoRagdollReason::NotApplicable,
            },
            clips: clips.clone(),
            idle_clip: "idle".to_string(),
            capsule: Capsule {
                height: 80.0,
                radius: 12.0,
            },
            paths: ScaffoldPaths {
                project: "Actors\\BridgeWolf\\Project.hkx".to_string(),
                character: "Actors\\BridgeWolf\\Character.hkx".to_string(),
                root_behavior: "Actors\\BridgeWolf\\RootBehavior.hkx".to_string(),
                core_behavior: "Actors\\BridgeWolf\\CoreBehavior.hkx".to_string(),
            },
            root: declarations(&graph),
            core: declarations(&graph),
        };
        let source = SourceCreatureIdentity {
            namespace: "skyrimse".to_string(),
            plugin: "Skyrim.esm".to_string(),
            local_form_id: 0x12_3456,
        };
        let base = record_manifest(runtime_path(&body_target));
        let projection = CreatureRecordProjectionManifest {
            base: base.clone(),
            source_primary_identity: source.clone(),
            race_data: race_data(),
            body_parts: CreatureBodyPartProjection::root_only_32("NPC Root [Root]"),
            body_nif_parts: Vec::new(),
            variants: vec![CreatureNpcRecordVariant {
                source_identity: source.clone(),
                form_key: base.form_keys.npc.clone(),
                editor_id: base.editor_ids.npc.clone(),
                display_name: "Wolf".to_string(),
                primary: true,
                level: 5,
                health: 100,
                action_points: 70,
                npc_inventory: None,
                npc_equipment: None,
                npc_spells: None,
                npc_death_item: None,
            }],
            npc_inventory: Vec::new(),
            npc_equipment: Vec::new(),
            npc_spells: Vec::new(),
            npc_death_item: None,
            attacks: vec![CreatureAttackRecordVariant {
                id: "bite".to_string(),
                event: "meleeWolfBite".to_string(),
                primary: true,
                projection: CreatureAttackRecordProjection::MeleeUnarmed {
                    weapon_form_key: base.form_keys.unarmed_weapon.clone(),
                    weapon_editor_id: base.editor_ids.unarmed_weapon.clone(),
                    damage: 12,
                    reach: 0.7,
                    attack_seconds: 0.8,
                },
                damage_multiplier: 1.0,
                chance: 1.0,
                strike_angle: 30.0,
                action_point_cost: 20.0,
                target_data: CreatureAttackTargetData::default(),
            }],
        };
        let key_plan = CreatureRecordKeyPlan {
            planned_source_identity: source.clone(),
            base: base.form_keys.clone(),
            armor_addons: vec![base.form_keys.armor_addon.clone()],
            npc_variants: vec![CreatureNpcRecordKeyPlan {
                source_identity: source.clone(),
                form_key: base.form_keys.npc.clone(),
                primary: true,
            }],
            melee_attacks: vec![CreatureMeleeRecordKeyPlan {
                attack_id: "bite".to_string(),
                form_key: base.form_keys.unarmed_weapon.clone(),
                primary: true,
            }],
        };
        let batch_intent = SourceRigRecordBatchIntent {
            family_id: "wolf".to_string(),
            primary_mapping: CreaturePrimaryRecordMapping {
                source,
                target: base.form_keys.npc.clone(),
            },
            required_target_records: Vec::new(),
        };

        let policy_field = SkyrimRaceDataFieldReceipt {
            field: CreatureRaceDataInputField::RaceData,
            provenance: SkyrimRaceDataFieldProvenance::SourceRaceData,
            policy_id: None,
            reason: "winning Skyrim RACE.DATA".to_string(),
        };
        let clip_roles = [
            SkyrimCreatureMotionRoleReceipt::Idle,
            SkyrimCreatureMotionRoleReceipt::GroundLocomotion,
            SkyrimCreatureMotionRoleReceipt::TurnLeft,
            SkyrimCreatureMotionRoleReceipt::TurnRight,
            SkyrimCreatureMotionRoleReceipt::MeleeAttack,
        ];
        let clip_receipts = clip_specs
            .iter()
            .zip(clip_roles)
            .map(|((id, file, _), role)| SkyrimCreatureClipReceipt {
                clip_id: (*id).to_string(),
                clip_path: format!("actors\\wolf\\{file}"),
                family_ids: vec!["wolf".to_string()],
                role: SkyrimCreatureClipRoleReceipt::Role(role),
                trigger_event: graph
                    .roles
                    .iter()
                    .find(|entry| entry.clip_name == *id)
                    .unwrap()
                    .trigger_event
                    .clone(),
                trigger_aliases: graph
                    .roles
                    .iter()
                    .find(|entry| entry.clip_name == *id)
                    .unwrap()
                    .trigger_aliases
                    .clone(),
                role_evidence: Vec::new(),
                root_motion: SkyrimRootMotionReceipt::Stationary {
                    source: super::super::creature_recipe::SkyrimRootMotionSourceReceipt::HavokReferenceFrame,
                    sample_count: 1,
                },
                root_motion_locators: Vec::new(),
                events: Vec::new(),
                annotations: Vec::new(),
                original_skeleton_name: Some("BridgeWolfSkeleton".to_string()),
            })
            .collect::<Vec<_>>();
        let mut asset_claims = vec![
            SkyrimRecursiveAssetClaim {
                path: "BridgeWolf\\skeleton.nif".to_string(),
                role: SkyrimAssetRole::AnimationSkeleton,
                dependencies: Vec::new(),
                disposition: SkyrimAssetClaimDisposition::Present {
                    blake3: skeleton_source_hash.clone(),
                },
            },
            SkyrimRecursiveAssetClaim {
                path: "BridgeWolf\\skeleton.nif".to_string(),
                role: SkyrimAssetRole::VisualSkeletonNif,
                dependencies: Vec::new(),
                disposition: SkyrimAssetClaimDisposition::Present {
                    blake3: skeleton_source_hash.clone(),
                },
            },
            SkyrimRecursiveAssetClaim {
                path: "BridgeWolf\\body.nif".to_string(),
                role: SkyrimAssetRole::BodyNif,
                dependencies: Vec::new(),
                disposition: SkyrimAssetClaimDisposition::Present {
                    blake3: body_source_hash,
                },
            },
        ];
        asset_claims.extend(
            clip_specs
                .iter()
                .map(|(_, file, _)| SkyrimRecursiveAssetClaim {
                    path: format!("actors\\wolf\\{file}"),
                    role: SkyrimAssetRole::AnimationClip,
                    dependencies: Vec::new(),
                    disposition: SkyrimAssetClaimDisposition::Present {
                        blake3: hash(&format!("clip-source-{file}")),
                    },
                }),
        );
        asset_claims.sort_by_key(|claim| (claim.path.clone(), claim.role));
        let family = SkyrimCreatureFamilyJob {
            family_id: "wolf".to_string(),
            project_path: "actors\\wolf\\project.hkx".to_string(),
            source_races: vec!["123456@Skyrim.esm".to_string()],
            inventory: SkyrimCreatureInventoryReceipt {
                project_path: "actors\\wolf\\project.hkx".to_string(),
                character_paths: vec!["actors\\wolf\\character.hkx".to_string()],
                animation_skeleton_paths: vec!["BridgeWolf\\skeleton.nif".to_string()],
                ragdoll_paths: Vec::new(),
                behavior_paths: vec!["actors\\wolf\\behavior.hkx".to_string()],
            },
            graph_variants: vec![SkyrimCreatureGraphVariantEvidence {
                sex: SkyrimCreatureSex::Male,
                project_path: "actors\\wolf\\project.hkx".to_string(),
                character_path: "actors\\wolf\\character.hkx".to_string(),
                skeleton_path: "BridgeWolf\\skeleton.nif".to_string(),
                actual_root_bone: "NPC Root [Root]".to_string(),
                behavior_paths: vec!["actors\\wolf\\behavior.hkx".to_string()],
            }],
            graph: SkyrimCreatureGraphEvidence {
                template: CreatureGraphTemplate::GroundMelee,
                ready: true,
                roles: graph.roles.clone(),
                candidate_attack_bindings: graph.candidate_attack_bindings.clone(),
                idle_event: Some(graph.idle_event.clone()),
                explicit_events: graph.explicit_events.clone(),
                variables: Vec::new(),
                overlays: Vec::new(),
                required_rigs: Vec::new(),
                rigs: Vec::new(),
                blockers: Vec::new(),
            },
            clips: clip_receipts,
            controllers: vec![SkyrimCreatureControllerReceipt {
                character_path: "actors\\wolf\\character.hkx".to_string(),
                character_data_class: "hkbCharacterData".to_string(),
                controller_class: None,
                controller_cinfo_class: None,
                architecture: None,
                layout: SkyrimControllerLayoutEvidence::LegacyCharacterControllerInfo {
                    contents_version: "hk_2010.2.0-r1".to_string(),
                    character_data_signature: SKYRIM_LEGACY_CHARACTER_DATA_SIGNATURE,
                    controller_signature: SKYRIM_LEGACY_CONTROLLER_SIGNATURE,
                },
                collision_filter_info: Some(7),
                rigid_body_type: Some(8),
                shape_type: Some(9),
                capsule: Some(CapsuleEvidence {
                    radius: 12.0,
                    total_height: 80.0,
                    architecture: ControllerArchitecture::Quadruped,
                }),
                axes: Some(
                    super::super::creature_recipe::SkyrimControllerAxesEvidence {
                        up: MeasurementAxis::Z,
                        forward: MeasurementAxis::Y,
                    },
                ),
                model: Some(
                    super::super::creature_recipe::SkyrimControllerModelReceipt {
                        up_ms: controller_decl.model_up_ms,
                        forward_ms: controller_decl.model_forward_ms,
                        right_ms: controller_decl.model_right_ms,
                        scale: controller_decl.model_scale,
                    },
                ),
                disposition: SkyrimControllerDispositionReceipt::Complete,
            }],
            ragdoll: SkyrimRagdollCapabilityEvidence::NotApplicable {
                reason: "source has no ragdoll".to_string(),
            },
            weapon_bearing: SkyrimExplicitCapabilityEvidence::NotApplicable {
                evidence: "unarmed".to_string(),
            },
            body_variants: vec![
                super::super::creature_recipe::SkyrimCreatureBodyVariantEvidence {
                    source_race: "123456@Skyrim.esm".to_string(),
                    sex: SkyrimCreatureSex::Male,
                    armor_addon: "123457@Skyrim.esm".to_string(),
                    body_nif: "BridgeWolf\\body.nif".to_string(),
                },
            ],
            asset_claims,
            disposition: SkyrimCreatureFamilyDisposition::Ready {
                capabilities: vec![
                    SkyrimCreatureCapability::GroundLocomotion,
                    SkyrimCreatureCapability::MeleeAttack,
                ],
            },
        };
        let candidate = super::super::creature_recipe::SkyrimCreatureCandidateRecipe {
            source_race: "123456@Skyrim.esm".to_string(),
            editor_id: Some("WolfRace".to_string()),
            source_skin: None,
            source_body_part_data: None,
            source_armor_addons: Vec::new(),
            source_body_models: vec!["BridgeWolf\\body.nif".to_string()],
            project_paths: vec!["actors\\wolf\\project.hkx".to_string()],
            skeleton_paths: vec!["BridgeWolf\\skeleton.nif".to_string()],
            attack_events: vec!["meleeWolfBite".to_string()],
            attack_contract: Vec::new(),
            attack_data: Vec::new(),
            attack_spells: Vec::new(),
            npc_templates: Vec::new(),
            race_data: SkyrimCreatureRaceDataReceipt {
                disposition: CreatureRaceDataDisposition::Complete,
                evidence: None,
                derivation: Some(Fo4RaceDataDerivation {
                    mapping: RaceDataMapping::Mapped {
                        target: projection.race_data.clone(),
                    },
                    missing_evidence: Vec::new(),
                    scale_audit: None,
                }),
                unarmed_data: Some(
                    super::super::creature_race_data::CreatureRaceUnarmedDataEvidence {
                        damage_bits: 10.0_f32.to_bits(),
                        reach_bits: 0.68_f32.to_bits(),
                    },
                ),
                missing_fields: Vec::new(),
                invalid_fields: Vec::new(),
                field_receipts: vec![policy_field.clone()],
            },
            disposition: SkyrimCreatureCandidateDisposition::Ready {
                family_job_ids: vec!["wolf".to_string()],
            },
        };
        let mut ledger = SkyrimCreatureRecipeLedger {
            version: SKYRIM_CREATURE_RECIPE_VERSION,
            source_game: "skyrimse".to_string(),
            target_game: "fo4".to_string(),
            race_data_policy: SkyrimRaceDataPolicyReceipt {
                policy_id: "skyrim_race_data_v1".to_string(),
                schema_id: "skyrim_to_fo4_race_data_v1".to_string(),
                source_data_schema: "skyrimse_race_data".to_string(),
                target_data_schema: "fo4_race_data".to_string(),
                scale_policy: Fo4RaceDataScalePolicy {
                    small_max_stature: 1.0,
                    medium_max_stature: 2.0,
                    large_max_stature: 3.0,
                },
                fields: vec![policy_field],
            },
            candidates: vec![candidate],
            family_jobs: vec![family],
            accounting: SkyrimCreatureRecipeAccounting {
                candidates: 1,
                ready_candidates: 1,
                family_jobs: 1,
                ready_family_jobs: 1,
                ..SkyrimCreatureRecipeAccounting::default()
            },
            content_blake3: String::new(),
        };
        ledger_hash(&mut ledger);

        let mut artifact_requests = vec![SourceRigConvertedArtifactRequestReceipt {
            role: SourceRigArtifactRole::AnimationSkeleton,
            runtime_path: animation_skeleton_path.clone(),
            source_game: "skyrimse".to_string(),
            source_evidence_blake3: skeleton_source_hash,
            conversion_request_blake3: hash("skeleton-request"),
        }];
        let mut artifact_receipts = vec![SourceRigArtifactReceipt {
            role: SourceRigArtifactRole::AnimationSkeleton,
            provenance: SourceRigArtifactProvenance::ReconstructedSourceSkeleton,
            runtime_path: animation_skeleton_path,
            byte_len: 10,
            blake3: hash("skeleton-output"),
        }];
        for clip in &clips {
            let file = clip.path.rsplit('\\').next().unwrap();
            artifact_requests.push(SourceRigConvertedArtifactRequestReceipt {
                role: SourceRigArtifactRole::AnimationClip {
                    clip_name: clip.name.clone(),
                },
                runtime_path: clip.path.clone(),
                source_game: "skyrimse".to_string(),
                source_evidence_blake3: hash(&format!("clip-source-{file}")),
                conversion_request_blake3: hash(&format!("clip-request-{file}")),
            });
            artifact_receipts.push(SourceRigArtifactReceipt {
                role: SourceRigArtifactRole::AnimationClip {
                    clip_name: clip.name.clone(),
                },
                provenance: SourceRigArtifactProvenance::ConvertedSourceClip,
                runtime_path: clip.path.clone(),
                byte_len: 10,
                blake3: hash(&format!("clip-output-{file}")),
            });
        }
        let mut input = SkyrimSourceRigBridgeInput {
            ledger,
            family_id: "wolf".to_string(),
            selection: SkyrimSourceRigFamilySelection {
                sex: SkyrimCreatureSex::Male,
                project_path: "actors\\wolf\\project.hkx".to_string(),
                character_path: "actors\\wolf\\character.hkx".to_string(),
                skeleton_path: "BridgeWolf\\skeleton.nif".to_string(),
                body_nif_path: "BridgeWolf\\body.nif".to_string(),
            },
            creature_closure: closure,
            creature_closure_request_blake3: closure_request_blake3,
            artifact_requests,
            artifact_receipts,
            rig,
            projection,
            key_plan,
            batch_intent,
            field_decisions: Vec::new(),
        };
        refresh_field_decisions(&mut input);
        input
    }

    #[test]
    fn ready_wolf_bridge_is_deterministic_roundtrips_and_prepares() {
        let input = fixture();
        let mut shuffled = input.clone();
        shuffled.artifact_requests.reverse();
        shuffled.artifact_receipts.reverse();
        shuffled.field_decisions.reverse();
        let first = build_skyrim_source_rig_executable_recipe(input).unwrap();
        let second = build_skyrim_source_rig_executable_recipe(shuffled).unwrap();
        assert_eq!(
            first.canonical_json().unwrap(),
            second.canonical_json().unwrap()
        );
        let loaded =
            SourceRigExecutableRecipe::from_json(&first.canonical_json().unwrap()).unwrap();
        assert_eq!(loaded, first);
        let interner = crate::sym::StringInterner::new();
        let batch = first.rebuild_record_family_batch(&interner).unwrap();
        assert_eq!(batch.family_id, "wolf");
        assert_eq!(batch.closures.len(), 1);
    }

    #[test]
    fn legacy_controller_without_source_enums_matches_normalized_fo4_controller() {
        let mut input = fixture();
        input.ledger.family_jobs[0].controllers[0].rigid_body_type = None;
        input.ledger.family_jobs[0].controllers[0].shape_type = None;
        input.rig.controller.rigid_body_type = u8::MAX as i32;
        ledger_hash(&mut input.ledger);

        build_skyrim_source_rig_executable_recipe(input).unwrap();
    }

    #[test]
    fn decoded_float_tracks_bind_to_converted_skeleton_slots() {
        let mut input = fixture();
        input.rig.animation_skeleton.float_slots = vec!["Speed".to_string()];
        for clip in &mut input.rig.clips {
            clip.binding.declared_float_tracks = 1;
            clip.binding.float_track_to_float_slot_indices = vec![0];
        }
        refresh_field_decisions(&mut input);

        build_skyrim_source_rig_executable_recipe(input).unwrap();
    }

    #[test]
    fn absent_source_ragdoll_uses_explicit_source_reason() {
        let mut input = fixture();
        input.rig.ragdoll = RagdollDisposition::NoRagdoll {
            reason: NoRagdollReason::SourceHasNoRagdoll,
        };

        build_skyrim_source_rig_executable_recipe(input).unwrap();
    }

    #[test]
    fn unsupported_source_ragdoll_is_terminal_for_not_applicable_evidence() {
        let mut input = fixture();
        input.rig.ragdoll = RagdollDisposition::NoRagdoll {
            reason: NoRagdollReason::UnsupportedSourceRagdoll,
        };

        build_skyrim_source_rig_executable_recipe(input).unwrap();
    }

    #[test]
    fn repeated_body_path_across_armor_addons_is_not_ambiguous() {
        let mut input = fixture();
        let mut duplicate = input.ledger.family_jobs[0].body_variants[0].clone();
        duplicate.armor_addon = "123458@Skyrim.esm".to_string();
        input.ledger.family_jobs[0].body_variants.push(duplicate);
        ledger_hash(&mut input.ledger);

        build_skyrim_source_rig_executable_recipe(input).unwrap();
    }

    #[test]
    fn blocked_family_and_mismatched_receipt_fail_before_recipe_output() {
        let mut blocked = fixture();
        blocked.ledger.family_jobs[0].disposition =
            SkyrimCreatureFamilyDisposition::Blocked { issues: Vec::new() };
        blocked.ledger.accounting.ready_family_jobs = 0;
        blocked.ledger.accounting.blocked_family_jobs = 1;
        ledger_hash(&mut blocked.ledger);
        assert!(matches!(
            build_skyrim_source_rig_executable_recipe(blocked),
            Err(SkyrimSourceRigBridgeError::Invalid {
                code: "family_disposition",
                ..
            })
        ));

        let mut mismatch = fixture();
        mismatch.artifact_requests[0].source_evidence_blake3 = hash("wrong-source");
        assert!(matches!(
            build_skyrim_source_rig_executable_recipe(mismatch),
            Err(SkyrimSourceRigBridgeError::Invalid {
                code: "artifact_source_evidence",
                ..
            })
        ));

        let mut request_mismatch = fixture();
        request_mismatch.creature_closure_request_blake3 = hash("different-closure-request");
        assert!(matches!(
            build_skyrim_source_rig_executable_recipe(request_mismatch),
            Err(SkyrimSourceRigBridgeError::Invalid {
                code: "nif_closure_request",
                ..
            })
        ));
    }

    #[test]
    fn same_nif_path_with_changed_source_hash_fails_before_recipe_output() {
        let mut input = fixture();
        let skeleton_claim = input.ledger.family_jobs[0]
            .asset_claims
            .iter_mut()
            .find(|claim| claim.role == SkyrimAssetRole::VisualSkeletonNif)
            .unwrap();
        skeleton_claim.disposition = SkyrimAssetClaimDisposition::Present {
            blake3: hash("different-source-bytes-at-the-same-path"),
        };
        ledger_hash(&mut input.ledger);
        assert!(matches!(
            build_skyrim_source_rig_executable_recipe(input),
            Err(SkyrimSourceRigBridgeError::Invalid {
                code: "nif_source_hash",
                ..
            })
        ));
    }

    #[test]
    fn nonmelee_template_rejects_phantom_weapon_projection() {
        let mut input = fixture();
        input.ledger.family_jobs[0].graph.template = CreatureGraphTemplate::StationaryTurret;
        input.ledger.family_jobs[0].disposition = SkyrimCreatureFamilyDisposition::Ready {
            capabilities: vec![SkyrimCreatureCapability::Flying],
        };
        ledger_hash(&mut input.ledger);
        assert!(matches!(
            build_skyrim_source_rig_executable_recipe(input),
            Err(SkyrimSourceRigBridgeError::Invalid {
                code: "attack_template",
                ..
            })
        ));
    }
}
