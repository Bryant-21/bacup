//! Pure FNV/FO3 recipe-to-source-rig adapter.
//!
//! This module validates already converted artifacts and constructs an
//! executable precommit recipe. It performs no conversion or publication.

use std::collections::BTreeSet;
use std::sync::Arc;

use nif_core_native::creature_closure::{CreatureClosureReceipt, CreatureNifRole};
use serde::Serialize;
use thiserror::Error;

use crate::source_rig::{
    CapabilityGraphManifest, CreatureActorActionRecordPlan, CreatureAttackRecordProjection,
    CreatureBodyPartProjection, CreatureClipRole, CreatureGraphTemplate, CreatureManifest,
    CreatureRecordKeyPlan, CreatureRecordProjectionManifest, NoRagdollReason, RaceDataMapping,
    RagdollDisposition, SourceRigArtifactReceipt, SourceRigArtifactRole, SourceRigBridgeError,
    SourceRigConvertedArtifactRequestReceipt, SourceRigExecutableRecipe, SourceRigFieldDecision,
    SourceRigFieldReceipt, SourceRigPairLedgerEvidence, SourceRigRecipeBridgeInput,
    SourceRigRecipeError, SourceRigRecordBatchIntent, SourceRigRuntimeModelClosureReceipt,
    SourceRigSelectedFamilyEvidence, build_source_rig_executable_recipe,
};
#[cfg(test)]
use crate::source_rig::{CapabilityRoleGenerator, SourceCreatureIdentity};

use super::creature_catalog::{LegacyCreatureGame, RigFamilyKey};
use super::creature_motion::{
    BindingCompatibility, MotionCandidate, MotionRole, RootMotionEvidence,
};
use super::creature_recipe::{
    CanonicalAssetClaim, CreatureFamilyRecipeJob, CreatureFamilyRecipeLedger,
    CreatureGraphCapability, CreatureRecipeAccounting, CreatureRecordRecipeDisposition,
    CreatureRecordRecipeEntry, FamilyRecipeDisposition, IndexedAssetKind,
    RagdollCapabilityEvidence, RagdollMode,
};

pub const FNV_FO3_SOURCE_RIG_BRIDGE_SCHEMA: &str = "fnv_fo3_creature_recipe_v4";

#[derive(Serialize)]
struct SelectedPairLedgerEvidence<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    profile_race_data: Option<&'a super::creature_profile_race_data::CreatureProfileRaceData>,
    #[serde(skip_serializing_if = "Option::is_none")]
    template_branch: Option<&'a super::creature_templates::LegacyTemplateBranchReceipt>,
    schema_version: u32,
    ledger_blake3: &'a str,
    accounting: &'a CreatureRecipeAccounting,
    record: &'a CreatureRecordRecipeEntry,
    family: &'a CreatureFamilyRecipeJob,
}

#[derive(Clone, Debug)]
pub struct FnvFo3SourceRigAdapterInput {
    pub profile_race_data: Option<super::creature_profile_race_data::CreatureProfileRaceData>,
    pub template_branch: Option<super::creature_templates::LegacyTemplateBranchReceipt>,
    pub ledger: Arc<CreatureFamilyRecipeLedger>,
    pub family: RigFamilyKey,
    pub creature_closure: CreatureClosureReceipt,
    pub creature_closure_request_blake3: String,
    pub artifact_requests: Vec<SourceRigConvertedArtifactRequestReceipt>,
    pub artifact_receipts: Vec<SourceRigArtifactReceipt>,
    pub runtime_model_closure: Option<SourceRigRuntimeModelClosureReceipt>,
    pub rig: CreatureManifest,
    pub graph: CapabilityGraphManifest,
    pub projection: CreatureRecordProjectionManifest,
    pub key_plan: CreatureRecordKeyPlan,
    pub actor_action_records: Vec<CreatureActorActionRecordPlan>,
    pub batch_intent: SourceRigRecordBatchIntent,
    pub field_receipts: Vec<SourceRigFieldReceipt>,
}

#[derive(Debug, Error)]
pub enum FnvFo3SourceRigBridgeError {
    #[error("invalid FNV/FO3 source-rig adapter evidence ({code}): {message}")]
    Invalid { code: &'static str, message: String },
    #[error("FNV/FO3 creature ledger is invalid: {0}")]
    Ledger(String),
    #[error(transparent)]
    SourceRig(#[from] SourceRigBridgeError),
    #[error(transparent)]
    Recipe(#[from] SourceRigRecipeError),
}

#[cfg(test)]
pub fn build_fnv_fo3_source_rig_bridge_input(
    input: FnvFo3SourceRigAdapterInput,
) -> Result<SourceRigRecipeBridgeInput, FnvFo3SourceRigBridgeError> {
    build_fnv_fo3_source_rig_bridge_input_impl(input, None)
}

pub(crate) fn build_fnv_fo3_source_rig_bridge_input_prevalidated(
    input: FnvFo3SourceRigAdapterInput,
    expected_ledger_blake3: &str,
) -> Result<SourceRigRecipeBridgeInput, FnvFo3SourceRigBridgeError> {
    build_fnv_fo3_source_rig_bridge_input_impl(input, Some(expected_ledger_blake3))
}

fn build_fnv_fo3_source_rig_bridge_input_impl(
    input: FnvFo3SourceRigAdapterInput,
    expected_ledger_blake3: Option<&str>,
) -> Result<SourceRigRecipeBridgeInput, FnvFo3SourceRigBridgeError> {
    let ledger_blake3 = if let Some(expected) = expected_ledger_blake3 {
        if input.ledger.content_hash_blake3 != expected {
            return Err(FnvFo3SourceRigBridgeError::Ledger(
                "prevalidated recipe ledger hash differs from the adapter ledger".to_string(),
            ));
        }
        expected.to_string()
    } else {
        input
            .ledger
            .canonical_hash_blake3()
            .map_err(|error| FnvFo3SourceRigBridgeError::Ledger(error.to_string()))?
    };
    let family = input
        .ledger
        .families
        .iter()
        .find(|job| job.rig == input.family)
        .ok_or_else(|| {
            invalid(
                "family_missing",
                "selected family is absent from the ledger",
            )
        })?;
    if !matches!(family.disposition, FamilyRecipeDisposition::Ready) {
        return Err(invalid(
            "family_not_ready",
            "only a terminal Ready family can produce a source-rig recipe",
        ));
    }

    let source_game = game_name(family.rig.game);
    validate_source_identity(&input, family, source_game)?;
    validate_family_contract(&input, family, source_game)?;
    validate_nif_closure(&input, family, source_game)?;
    validate_converted_evidence(&input, family, source_game)?;
    validate_field_decisions(&input.field_receipts, source_game)?;

    let source_identity = input.projection.source_primary_identity.clone();
    let source_root_identity = input.template_branch.as_ref().map(|branch| {
        let root = &branch.profile().expect("validated template branch").root;
        crate::source_rig::SourceCreatureIdentity { namespace: source_game.into(), plugin: root.plugin.clone(), local_form_id: root.local }
    });
    let record_identity = source_root_identity.as_ref().unwrap_or(&source_identity);
    let record = input
        .ledger
        .records
        .iter()
        .find(|record| {
            record.source.local == record_identity.local_form_id
                && record
                    .source
                    .plugin
                    .eq_ignore_ascii_case(&record_identity.plugin)
        })
        .ok_or_else(|| {
            invalid(
                "source_member_missing",
                "primary source identity is absent from the selected ledger evidence",
            )
        })?;
    let canonical_json =
        selected_pair_ledger_canonical_json(&input.ledger, record, family, &ledger_blake3, input.template_branch.as_ref(), input.profile_race_data.as_ref())?;
    let canonical_json_blake3 = blake3::hash(canonical_json.as_bytes()).to_hex().to_string();
    let family_id = canonical_fnv_fo3_family_id(&family.rig);
    if input.batch_intent.family_id != family_id {
        return Err(invalid(
            "family_id_mismatch",
            format!(
                "batch family {:?} differs from evidence family {:?}",
                input.batch_intent.family_id, family_id
            ),
        ));
    }

    let actual_root_bone = family
        .actual_root_node
        .clone()
        .ok_or_else(|| invalid("actual_root_missing", "Ready family has no actual root"))?;
    let race_data = input.projection.race_data.clone();
    let selected_family = SourceRigSelectedFamilyEvidence {
        source_games: vec![source_game.to_string()],
        actual_root_bone,
        visual_skeleton_nif: input.rig.visual_skeleton_nif.clone(),
        visual_creature_closure_target_path: visual_closure_target(&input)?,
        animation_skeleton: input.rig.animation_skeleton.clone(),
        controller: input.rig.controller,
        graph: input.graph.clone(),
        clips: input.rig.clips.clone(),
        ragdoll: input.rig.ragdoll.clone(),
        race_data,
    };

    Ok(SourceRigRecipeBridgeInput {
        pair_ledger: SourceRigPairLedgerEvidence {
            source_root_identity,
            schema: FNV_FO3_SOURCE_RIG_BRIDGE_SCHEMA.to_string(),
            canonical_json,
            ledger_blake3,
            canonical_json_blake3,
            source_identity,
            family_id,
        },
        selected_family,
        creature_closure: input.creature_closure,
        creature_closure_request_blake3: input.creature_closure_request_blake3,
        artifact_requests: input.artifact_requests,
        artifact_receipts: input.artifact_receipts,
        rig: input.rig,
        graph: input.graph,
        projection: input.projection,
        key_plan: input.key_plan,
        actor_action_records: input.actor_action_records,
        batch_intent: input.batch_intent,
        field_receipts: input.field_receipts,
    })
}

fn selected_pair_ledger_canonical_json(
    ledger: &CreatureFamilyRecipeLedger,
    record: &CreatureRecordRecipeEntry,
    family: &CreatureFamilyRecipeJob,
    ledger_blake3: &str,
    template_branch: Option<&super::creature_templates::LegacyTemplateBranchReceipt>,
    profile_race_data: Option<&super::creature_profile_race_data::CreatureProfileRaceData>,
) -> Result<String, FnvFo3SourceRigBridgeError> {
    serde_json::to_string(&SelectedPairLedgerEvidence {
        profile_race_data,
        template_branch,
        schema_version: ledger.schema_version,
        ledger_blake3,
        accounting: &ledger.accounting,
        record,
        family,
    })
    .map_err(|error| FnvFo3SourceRigBridgeError::Ledger(error.to_string()))
}

#[cfg(test)]
pub fn build_fnv_fo3_source_rig_executable_recipe(
    input: FnvFo3SourceRigAdapterInput,
) -> Result<SourceRigExecutableRecipe, FnvFo3SourceRigBridgeError> {
    let runtime_model_closure = input.runtime_model_closure.clone();
    let mut recipe =
        build_source_rig_executable_recipe(build_fnv_fo3_source_rig_bridge_input(input)?)?;
    if let Some(receipt) = runtime_model_closure {
        recipe = recipe.with_runtime_model_closure(receipt)?;
    }
    Ok(recipe)
}

pub(crate) fn build_fnv_fo3_source_rig_executable_recipe_prevalidated(
    input: FnvFo3SourceRigAdapterInput,
    expected_ledger_blake3: &str,
) -> Result<SourceRigExecutableRecipe, FnvFo3SourceRigBridgeError> {
    let runtime_model_closure = input.runtime_model_closure.clone();
    let mut recipe = build_source_rig_executable_recipe(
        build_fnv_fo3_source_rig_bridge_input_prevalidated(input, expected_ledger_blake3)?,
    )?;
    if let Some(receipt) = runtime_model_closure {
        recipe = recipe.with_runtime_model_closure(receipt)?;
    }
    Ok(recipe)
}

fn validate_source_identity(
    input: &FnvFo3SourceRigAdapterInput,
    family: &CreatureFamilyRecipeJob,
    source_game: &str,
) -> Result<(), FnvFo3SourceRigBridgeError> {
    let identity = &input.projection.source_primary_identity;
    if let Some(branch) = &input.template_branch {
        return validate_template_branch_identity(input, family, branch);
    }
    if !identity.namespace.eq_ignore_ascii_case(source_game) {
        return Err(invalid(
            "source_game_mismatch",
            "source identity namespace does not preserve the family game",
        ));
    }
    let member = input.ledger.records.iter().find(|record| {
        record.source.local == identity.local_form_id
            && record.source.plugin.eq_ignore_ascii_case(&identity.plugin)
    });
    let Some(member) = member else {
        return Err(invalid(
            "source_member_missing",
            "primary source identity is not a family ledger member",
        ));
    };
    let family_member = match &member.disposition {
        CreatureRecordRecipeDisposition::VisualOwner { rig, .. } => {
            rig == &family.rig && family.members.contains(&member.source)
        }
        CreatureRecordRecipeDisposition::Proxy {
            rig,
            terminal_visual_owners,
        } => {
            rig == &family.rig
                && terminal_visual_owners
                    .iter()
                    .any(|owner| family.members.contains(owner))
        }
        CreatureRecordRecipeDisposition::RejectedProxy { .. }
        | CreatureRecordRecipeDisposition::Special { .. } => false,
    };
    if member.provenance.game != family.rig.game || !family_member {
        return Err(invalid(
            "source_member_not_visual_owner",
            "source identity is not a visual owner or proven proxy of the selected family",
        ));
    }
    if !member
        .provenance
        .source_plugin
        .eq_ignore_ascii_case(&identity.plugin)
    {
        return Err(invalid(
            "source_plugin_mismatch",
            "primary source plugin differs from record provenance",
        ));
    }
    Ok(())
}

fn validate_template_branch_identity(
    input: &FnvFo3SourceRigAdapterInput,
    family: &CreatureFamilyRecipeJob,
    branch: &super::creature_templates::LegacyTemplateBranchReceipt,
) -> Result<(), FnvFo3SourceRigBridgeError> {
    branch.validate(family.rig.game).map_err(|error| invalid("template_branch_identity", error))?;
    let profile = branch.profile().map_err(|error| invalid("template_branch_path", error))?;
    let lookup = |key: &super::creature_catalog::StableFormKey| input.ledger.records.iter()
        .find(|record| record.source.local == key.local && record.source.plugin.eq_ignore_ascii_case(&key.plugin));
    let root = lookup(&profile.root).ok_or_else(|| invalid("template_root_missing", "template root is absent from source ledger"))?;
    let model_owner = &profile.owners[6];
    let owner = lookup(model_owner).ok_or_else(|| invalid("template_model_owner_missing", "template model owner is absent from source ledger"))?;
    if root.provenance.game != family.rig.game || owner.provenance.game != family.rig.game
        || !matches!(&owner.disposition, CreatureRecordRecipeDisposition::VisualOwner { rig, .. } if rig == &family.rig)
        || !family.members.contains(&owner.source) {
        return Err(invalid("template_model_owner_family", "template branch does not use its proven source visual family"));
    }
    let allowed = match &root.disposition {
        CreatureRecordRecipeDisposition::VisualOwner { .. } => root.source == owner.source,
        CreatureRecordRecipeDisposition::Proxy { terminal_visual_owners, .. } => terminal_visual_owners.contains(&owner.source),
        CreatureRecordRecipeDisposition::RejectedProxy { blockers } => !blockers.is_empty() && blockers.iter().all(|blocker| match blocker {
            super::creature_recipe::RecordRecipeBlocker::RequiresLeveledTemplateProjection(owners) => owners.contains(&owner.source),
            super::creature_recipe::RecordRecipeBlocker::AmbiguousFamilies(families) => families.contains(&family.rig),
            _ => false,
        }),
        CreatureRecordRecipeDisposition::Special { .. } => false,
    };
    if !allowed || profile.owners.iter().any(|key| lookup(key).is_none_or(|record| record.provenance.game != root.provenance.game)) {
        return Err(invalid("template_source_boundary", "template branch crosses a source boundary or bypasses an unrelated catalog blocker"));
    }
    let expected = if branch.wrapper {
        crate::source_rig::SourceCreatureIdentity { namespace: game_name(family.rig.game).into(), plugin: profile.root.plugin.clone(), local_form_id: profile.root.local }
    } else { branch.generated_identity.clone() };
    if input.projection.source_primary_identity != expected
        || branch.wrapper != input.projection.templates.is_some()
        || !input.projection.variants.iter().any(|variant| variant.source_identity == branch.generated_identity)
        || (branch.wrapper && branch.plan.branch_paths().first() != Some(&branch.entry_indices)) {
        return Err(invalid("template_projection_identity", "template primary, generated branch NPC or canonical wrapper ownership differs from source proof"));
    }
    Ok(())
}

fn validate_family_contract(
    input: &FnvFo3SourceRigAdapterInput,
    family: &CreatureFamilyRecipeJob,
    source_game: &str,
) -> Result<(), FnvFo3SourceRigBridgeError> {
    let contract = family.graph_contract.as_ref().ok_or_else(|| {
        invalid(
            "graph_contract_missing",
            "Ready family has no graph contract",
        )
    })?;
    let expected_templates = match contract.capability {
        CreatureGraphCapability::PassiveGround => vec![CreatureGraphTemplate::PassiveGround],
        CreatureGraphCapability::GroundMelee => vec![CreatureGraphTemplate::GroundMelee],
        CreatureGraphCapability::GroundRangedProjectile => {
            vec![CreatureGraphTemplate::GroundRangedProjectile]
        }
        CreatureGraphCapability::GroundMeleeRanged => {
            vec![CreatureGraphTemplate::GroundMeleeRanged]
        }
        CreatureGraphCapability::GroundSwim => vec![CreatureGraphTemplate::GroundSwim],
        CreatureGraphCapability::GroundFly => vec![CreatureGraphTemplate::GroundFly],
        CreatureGraphCapability::Swim => vec![CreatureGraphTemplate::Swim],
        CreatureGraphCapability::Fly => vec![CreatureGraphTemplate::Fly],
        CreatureGraphCapability::StationaryTurret => {
            vec![CreatureGraphTemplate::StationaryTurret]
        }
        CreatureGraphCapability::RobotContinuousAttack => {
            vec![CreatureGraphTemplate::RobotContinuousAttack]
        }
        CreatureGraphCapability::HumanoidWeaponOverlay => {
            let has = |role| contract.required_roles.contains(&role);
            if has(MotionRole::MeleeAttack)
                && (has(MotionRole::RangedAttack) || has(MotionRole::Projectile))
            {
                vec![CreatureGraphTemplate::GroundMeleeRanged]
            } else if has(MotionRole::RangedAttack) || has(MotionRole::Projectile) {
                vec![CreatureGraphTemplate::GroundRangedProjectile]
            } else if has(MotionRole::MeleeAttack) {
                vec![CreatureGraphTemplate::GroundMelee]
            } else {
                vec![CreatureGraphTemplate::PassiveGround]
            }
        }
    };
    let overlays_match = match contract.capability {
        CreatureGraphCapability::HumanoidWeaponOverlay => !input.graph.overlays.is_empty(),
        _ => input.graph.overlays.is_empty(),
    };
    if !expected_templates.contains(&input.graph.template) || !overlays_match {
        return Err(invalid(
            "graph_template_mismatch",
            "target graph template or overlay channels differ from pair evidence",
        ));
    }
    let actual_root = family
        .actual_root_node
        .as_deref()
        .ok_or_else(|| invalid("actual_root_missing", "Ready family has no actual root"))?;
    let roots = input
        .rig
        .animation_skeleton
        .bones
        .iter()
        .filter(|bone| bone.parent_index.is_none())
        .collect::<Vec<_>>();
    if roots.len() != 1 || roots[0].name != actual_root {
        return Err(invalid(
            "actual_root_mismatch",
            "converted skeleton root differs from measured NIF root",
        ));
    }
    match &input.projection.body_parts {
        CreatureBodyPartProjection::SourceParts { root_node, .. } if root_node == actual_root => {}
        CreatureBodyPartProjection::RootOnly32 {
            node, vats_target, ..
        } if node == actual_root && vats_target == actual_root => {}
        _ => {
            return Err(invalid(
                "body_root_mismatch",
                "BPTD root projection differs from measured root",
            ));
        }
    }
    let target = if let Some(profile) = &input.profile_race_data {
        super::creature_profile_race_data::validate(profile, family)
            .map_err(|error| invalid("profile_race_data_invalid", error))?;
        if let Some(branch) = &input.template_branch {
            let expected_owner = &branch.profile().map_err(|error| invalid("profile_race_data_invalid", error))?.owners[1];
            if profile.stats_owner != *expected_owner {
                return Err(invalid("profile_race_data_owner_mismatch", "RACE Stats owner differs from the selected template branch"));
            }
        }
        match &profile.derivation.mapping {
            RaceDataMapping::Mapped { target } => target,
            _ => return Err(invalid("profile_race_data_invalid", "profile RACE is unmapped")),
        }
    } else {
        mapped_race_data(family)?
    };
    if &input.projection.race_data != target {
        return Err(invalid(
            "race_data_mismatch",
            "record projection RACE.DATA differs from measured derivation",
        ));
    }
    // FO4 scales this shared model-space shape through ScaleCustomCharacterController.
    if !input.projection.race_keywords.contains(&super::creature_profile_race_data::controller_scale_keyword()) {
        return Err(invalid("controller_scale_keyword_missing", "source controller capsules require the target ScaleCustomCharacterController keyword"));
    }
    let capsule = family
        .race_data
        .as_ref()
        .and_then(|race| race.measured_evidence.capsule.as_ref())
        .ok_or_else(|| {
            invalid(
                "capsule_missing",
                "Ready family lacks a source controller capsule",
            )
        })?;
    if input.rig.capsule.height.to_bits() != capsule.total_height.to_bits()
        || input.rig.capsule.radius.to_bits() != capsule.radius.to_bits()
    {
        return Err(invalid(
            "capsule_mismatch",
            "runtime capsule differs from the shared family artifact evidence",
        ));
    }
    let controller_values = input
        .rig
        .controller
        .model_up_ms
        .into_iter()
        .chain(input.rig.controller.model_forward_ms)
        .chain(input.rig.controller.model_right_ms)
        .chain([input.rig.controller.model_scale]);
    if controller_values
        .into_iter()
        .any(|value| !value.is_finite())
        || input.rig.controller.model_scale <= 0.0
    {
        return Err(invalid(
            "controller_invalid",
            "controller basis and scale must be explicit finite evidence",
        ));
    }
    if input
        .projection
        .source_primary_identity
        .namespace
        .to_ascii_lowercase()
        != source_game
    {
        return Err(invalid(
            "controller_game_mismatch",
            "controller source game differs",
        ));
    }

    match contract.capability {
        CreatureGraphCapability::GroundRangedProjectile
        | CreatureGraphCapability::StationaryTurret => {
            if !input.key_plan.melee_attacks.is_empty()
                || input.projection.attacks.iter().any(|attack| {
                    matches!(
                        attack.projection,
                        CreatureAttackRecordProjection::MeleeUnarmed { .. }
                    )
                })
            {
                return Err(invalid(
                    "phantom_melee_weapon",
                    "nonmelee family declares a melee WEAP projection",
                ));
            }
        }
        _ => {}
    }
    Ok(())
}

fn validate_nif_closure(
    input: &FnvFo3SourceRigAdapterInput,
    family: &CreatureFamilyRecipeJob,
    source_game: &str,
) -> Result<(), FnvFo3SourceRigBridgeError> {
    input
        .creature_closure
        .validate()
        .map_err(|error| invalid("nif_closure_invalid", error.to_string()))?;
    if !input
        .creature_closure
        .source_game
        .eq_ignore_ascii_case(source_game)
        || !input
            .creature_closure
            .target_game
            .eq_ignore_ascii_case("fo4")
        || !is_lower_blake3(&input.creature_closure_request_blake3)
        || input.creature_closure_request_blake3 != input.creature_closure.request_blake3
    {
        return Err(invalid(
            "nif_closure_game",
            "NIF closure game or request hash differs from selected family",
        ));
    }
    let expected_skeleton = canonical_mesh_path(&family.rig.skeleton_path);
    let selected_body_paths = family
        .body_variants
        .iter()
        .flat_map(|variant| &variant.body_paths)
        .map(|path| canonical_mesh_path(path))
        .collect::<BTreeSet<_>>();
    let expected_bodies = family
        .assets
        .iter()
        .filter(|asset| {
            asset.kind == IndexedAssetKind::Body
                && selected_body_paths.contains(&canonical_mesh_path(&asset.path))
        })
        .map(|asset| canonical_mesh_path(&asset.path))
        .collect::<BTreeSet<_>>();
    let actual_skeletons = input
        .creature_closure
        .inputs
        .iter()
        .filter(|entry| entry.role == CreatureNifRole::Skeleton)
        .map(|entry| canonical_mesh_path(&entry.source_data_relative_path))
        .collect::<BTreeSet<_>>();
    let actual_bodies = input
        .creature_closure
        .inputs
        .iter()
        .filter(|entry| entry.role == CreatureNifRole::Body)
        .map(|entry| canonical_mesh_path(&entry.source_data_relative_path))
        .collect::<BTreeSet<_>>();
    if actual_skeletons != BTreeSet::from([expected_skeleton]) || actual_bodies != expected_bodies {
        return Err(invalid(
            "nif_closure_sources",
            "NIF closure does not exactly cover the family skeleton and body variants",
        ));
    }
    for disposition in &input.creature_closure.inputs {
        let kind = match disposition.role {
            CreatureNifRole::Skeleton => IndexedAssetKind::Skeleton,
            CreatureNifRole::Body => IndexedAssetKind::Body,
            CreatureNifRole::BodyPartReplacement | CreatureNifRole::DependencyModel => {
                if disposition.source_byte_len == 0 || !is_lower_blake3(&disposition.source_blake3) || !input.creature_closure.artifacts.iter().any(|artifact| artifact.target_data_relative_path == disposition.target_data_relative_path && artifact.byte_len > 0 && is_lower_blake3(&artifact.fingerprint)) {
                    return Err(invalid("nif_closure_source_hash", "replacement NIF lacks source and output byte evidence"));
                }
                continue;
            }
        };
        let claim = asset(family, &disposition.source_data_relative_path, kind)?;
        if disposition.source_byte_len == 0
            || disposition.source_blake3 != claim.content_hash_blake3
        {
            return Err(invalid(
                "nif_closure_source_hash",
                format!(
                    "closure source bytes for {:?} differ from strict ledger evidence",
                    disposition.source_data_relative_path
                ),
            ));
        }
    }
    let body_runtime = canonical_mesh_path(&input.projection.base.body_nif);
    if !input.creature_closure.inputs.iter().any(|entry| {
        entry.role == CreatureNifRole::Body
            && canonical_mesh_path(&entry.target_data_relative_path) == body_runtime
    }) {
        return Err(invalid(
            "body_projection_mismatch",
            "record body NIF is absent from the validated converted closure",
        ));
    }
    if let CreatureBodyPartProjection::SourceParts { parts, .. } = &input.projection.body_parts {
        for part in parts.iter().filter(|part| !part.limb_replacement.is_empty()) {
            if !input.creature_closure.inputs.iter().any(|entry| entry.role == CreatureNifRole::BodyPartReplacement && canonical_mesh_path(&entry.target_data_relative_path) == canonical_mesh_path(&part.limb_replacement)) {
                return Err(invalid("replacement_projection_mismatch", "BPTD replacement NIF is absent from converted closure"));
            }
        }
    }
    Ok(())
}

fn validate_converted_evidence(
    input: &FnvFo3SourceRigAdapterInput,
    family: &CreatureFamilyRecipeJob,
    source_game: &str,
) -> Result<(), FnvFo3SourceRigBridgeError> {
    for request in &input.artifact_requests {
        if !request.source_game.eq_ignore_ascii_case(source_game)
            || !is_lower_blake3(&request.source_evidence_blake3)
            || !is_lower_blake3(&request.conversion_request_blake3)
        {
            return Err(invalid(
                "converted_request_identity",
                "converted artifact request has a missing hash or mismatched source game",
            ));
        }
    }
    for receipt in &input.artifact_receipts {
        if receipt.byte_len == 0 || !is_lower_blake3(&receipt.blake3) {
            return Err(invalid(
                "converted_receipt_hash",
                "converted artifact receipt lacks canonical output evidence",
            ));
        }
    }
    let skeleton = asset(
        family,
        &family.rig.skeleton_path,
        IndexedAssetKind::Skeleton,
    )?;
    let skeleton_runtime_name = skeleton.skeleton_runtime_name.as_deref().ok_or_else(|| {
        invalid(
            "skeleton_identity_missing",
            "ledger lacks skeleton runtime name",
        )
    })?;
    if input.rig.animation_skeleton.runtime_name != skeleton_runtime_name
        || (!input.rig.animation_skeleton.float_slots.is_empty()
            && input.rig.animation_skeleton.float_slots != skeleton.skeleton_float_slots)
    {
        return Err(invalid(
            "skeleton_identity_mismatch",
            "runtime skeleton name or ordered float slots differ from strict ledger evidence",
        ));
    }
    let skeleton_request = request(
        input,
        &SourceRigArtifactRole::AnimationSkeleton,
        &input.rig.animation_skeleton.path,
    )?;
    if skeleton_request.source_evidence_blake3 != skeleton.content_hash_blake3 {
        return Err(invalid(
            "skeleton_hash_mismatch",
            "converted skeleton request differs from indexed source skeleton",
        ));
    }
    for role in &input.graph.roles {
        for clip_name in role.generator.clip_names(&role.clip_name) {
        let clip = input
            .rig
            .clips
            .iter()
            .find(|clip| clip.name == clip_name)
            .ok_or_else(|| invalid("clip_missing", format!("missing clip {clip_name:?}")))?;
        let artifact_role = SourceRigArtifactRole::AnimationClip {
            clip_name: clip.name.clone(),
        };
        let artifact_request = request(input, &artifact_role, &clip.path)?;
        let motion_roles = pair_motion_roles(input.graph.template, role.role);
        let aim_candidates = if let crate::source_rig::CapabilityRoleGenerator::SourceAim { bank_index, .. } = &role.generator {
            super::creature_live_preparations::source_aim_candidates(family, *bank_index)
        } else { Vec::new() };
        let candidate = family
            .required_motion
            .iter()
            .filter(|required| motion_roles.contains(&required.role))
            .flat_map(|required| &required.candidates)
            .chain(aim_candidates.iter())
            .find(|candidate| candidate_hash(candidate) == artifact_request.source_evidence_blake3)
            .ok_or_else(|| {
                invalid(
                    "clip_evidence_mismatch",
                    format!(
                        "clip {:?} is not bound to its selected {:?} KF sequence",
                        clip.name, motion_roles
                    ),
                )
            })?;
        validate_candidate(input, family, role, clip, candidate)?;
        }
    }
    for overlay in &input.graph.overlays {
        let clip = input
            .rig
            .clips
            .iter()
            .find(|clip| clip.name == overlay.clip_name)
            .ok_or_else(|| {
                invalid(
                    "clip_missing",
                    format!("missing overlay clip {:?}", overlay.clip_name),
                )
            })?;
        let artifact_role = SourceRigArtifactRole::AnimationClip {
            clip_name: clip.name.clone(),
        };
        let artifact_request = request(input, &artifact_role, &clip.path)?;
        let candidates = family
            .required_motion
            .iter()
            .find(|required| required.role == MotionRole::Overlay)
            .ok_or_else(|| invalid("motion_role_missing", "missing role Overlay"))?;
        let candidate = candidates
            .candidates
            .iter()
            .find(|candidate| candidate_hash(candidate) == artifact_request.source_evidence_blake3)
            .ok_or_else(|| {
                invalid(
                    "clip_evidence_mismatch",
                    format!(
                        "overlay clip {:?} is not bound to its selected KF sequence",
                        clip.name
                    ),
                )
            })?;
        validate_candidate_binding(input, family, clip, candidate, overlay.motion, false)?;
        for event in [&overlay.start_event, &overlay.stop_event] {
            if !candidate
                .events
                .iter()
                .any(|source| source.raw_text == *event)
            {
                return Err(invalid(
                    "clip_event_mismatch",
                    format!("overlay clip {:?} lacks exact event {event:?}", clip.name),
                ));
            }
        }
    }

    match (&family.ragdoll, &input.rig.ragdoll) {
        (
            Some(RagdollCapabilityEvidence::Supported { owner_asset, .. }),
            RagdollDisposition::SourceOwned {
                runtime_path,
                receipt,
            },
        ) => {
            let kind = match family.ragdoll.as_ref() {
                Some(RagdollCapabilityEvidence::Supported {
                    mode: RagdollMode::EmbeddedSkeleton,
                    ..
                }) => IndexedAssetKind::Skeleton,
                _ => IndexedAssetKind::Ragdoll,
            };
            let claim = asset(family, owner_asset, kind)?;
            let request = request(input, &SourceRigArtifactRole::Ragdoll, runtime_path)?;
            if request.source_evidence_blake3 != claim.content_hash_blake3
                || receipt.byte_len == 0
                || !is_lower_blake3(&receipt.blake3)
            {
                return Err(invalid(
                    "ragdoll_evidence_mismatch",
                    "source-owned ragdoll request/receipt differs from pair evidence",
                ));
            }
        }
        (
            Some(RagdollCapabilityEvidence::ExplicitlyUnsupported { .. }),
            RagdollDisposition::NoRagdoll {
                reason: NoRagdollReason::UnsupportedSourceRagdoll,
            },
        ) => {}
        (
            _,
            RagdollDisposition::NoRagdoll {
                reason: NoRagdollReason::Deferred,
            },
        ) => {
            return Err(invalid(
                "ragdoll_deferred",
                "deferred ragdoll is not an executable disposition",
            ));
        }
        _ => {
            return Err(invalid(
                "ragdoll_evidence_mismatch",
                "runtime ragdoll disposition differs from pair evidence",
            ));
        }
    }
    Ok(())
}

fn validate_candidate(
    input: &FnvFo3SourceRigAdapterInput,
    family: &CreatureFamilyRecipeJob,
    role: &crate::source_rig::CapabilityClipRole,
    clip: &crate::source_rig::ClipDecl,
    candidate: &MotionCandidate,
) -> Result<(), FnvFo3SourceRigBridgeError> {
    let projects_stationary_turret_cycle = input.graph.template
        == CreatureGraphTemplate::StationaryTurret
        && role.role == CreatureClipRole::ProjectileAttack
        && candidate.looping
        && !clip.looping;
    let projects_automatic_phase = super::creature_live_preparations::validate_automatic_phase_projection(
        family, candidate, role, clip,
    ).map_err(|detail| invalid("automatic_attack_phase_mismatch", detail))?;
    validate_candidate_binding(
        input,
        family,
        clip,
        candidate,
        role.motion,
        projects_stationary_turret_cycle || projects_automatic_phase,
    )?;
    if role.role == CreatureClipRole::MeleeAttack
        && let Some(event) = &role.trigger_event
    {
        if !candidate
            .events
            .iter()
            .any(|source| source.raw_text == *event)
            && !is_generated_melee_actor_action_event(event)
        {
            return Err(invalid(
                "clip_event_mismatch",
                format!("clip {:?} lacks exact trigger event {event:?}", clip.name),
            ));
        }
    }
    Ok(())
}

fn is_generated_melee_actor_action_event(event: &str) -> bool {
    let Some(ordinal) = event.strip_prefix("melee_fnvfo3_") else {
        return false;
    };
    ordinal.len() == 2 && ordinal.bytes().all(|byte| byte.is_ascii_digit()) && ordinal != "00"
}

fn validate_candidate_binding(
    input: &FnvFo3SourceRigAdapterInput,
    family: &CreatureFamilyRecipeJob,
    clip: &crate::source_rig::ClipDecl,
    candidate: &MotionCandidate,
    motion: crate::source_rig::ClipMotionPolicy,
    allow_looping_source_projection: bool,
) -> Result<(), FnvFo3SourceRigBridgeError> {
    let accumulator_added = matches!(&candidate.root_motion,
        RootMotionEvidence::Stationary { accum_root: Some(root) }
        | RootMotionEvidence::Planar { accum_root: root, .. }
        if exact_accumulator_track_augmentation(
            &candidate.binding.ordered_transform_targets,
            candidate.binding.transform_track_count,
            root,
            &input.rig.animation_skeleton.bones,
            &clip.binding.transform_track_to_bone_indices,
        )
    );
    let expected_track_count = candidate.binding.transform_track_count + usize::from(accumulator_added);
    let property_sequences = input.rig.property_sequences.iter().filter(|sequence| sequence.clip_name == clip.name).collect::<Vec<_>>();
    let migrated_property_tracks = !property_sequences.is_empty()
        && input.rig.animation_skeleton.float_slots.is_empty()
        && property_sequences.iter().all(|sequence| sequence.source_float_slots == candidate.binding.required_float_slots)
        && candidate.binding.required_float_slots.len() == candidate.binding.float_track_count;
    if !property_sequences.is_empty() && !migrated_property_tracks {
        return Err(invalid("clip_property_binding_mismatch", format!("clip {:?} property sequences differ from exact source controller evidence", clip.name)));
    }
    let expected_float_count = if migrated_property_tracks { 0 } else { candidate.binding.float_track_count };
    if candidate.source_game != family.rig.game
        || candidate.binding.compatibility != BindingCompatibility::Verified
        || canonical_mesh_path(&candidate.binding.source_skeleton_path)
            != canonical_mesh_path(&family.rig.skeleton_path)
        || expected_track_count != clip.binding.declared_transform_tracks
        || expected_track_count != clip.binding.transform_track_to_bone_indices.len()
        || clip.binding.skeleton_path != input.rig.animation_skeleton.path
        || clip.binding.original_skeleton_name != input.rig.animation_skeleton.runtime_name
        || clip.binding.declared_float_tracks != expected_float_count
        || clip.binding.float_track_to_float_slot_indices.len()
            != expected_float_count
        || (candidate.looping != clip.looping && !allow_looping_source_projection)
    {
        return Err(invalid(
            "clip_binding_mismatch",
            format!("clip {:?} differs from verified KF binding", clip.name),
        ));
    }
    let mapped_float_slots = clip
        .binding
        .float_track_to_float_slot_indices
        .iter()
        .map(|&index| input.rig.animation_skeleton.float_slots.get(index))
        .collect::<Option<Vec<_>>>()
        .ok_or_else(|| {
            invalid(
                "clip_float_binding_mismatch",
                format!(
                    "clip {:?} references an absent skeleton float slot",
                    clip.name
                ),
            )
        })?;
    if !migrated_property_tracks && mapped_float_slots
        != candidate
            .binding
            .required_float_slots
            .iter()
            .collect::<Vec<_>>()
    {
        return Err(invalid(
            "clip_float_binding_mismatch",
            format!(
                "clip {:?} float-track mapping differs from exact KF evidence",
                clip.name
            ),
        ));
    }
    asset(family, &candidate.source_kf, IndexedAssetKind::Kf)?;
    let planar = matches!(candidate.root_motion, RootMotionEvidence::Planar { .. });
    if motion.animation_driven != planar
        || (planar && motion.extracted_planar_reference_frames == 0)
        || (!planar && motion.extracted_planar_reference_frames != 0)
        || matches!(
            candidate.root_motion,
            RootMotionEvidence::Unknown | RootMotionEvidence::Unsupported { .. }
        )
    {
        return Err(invalid(
            "root_motion_mismatch",
            format!(
                "clip {:?} root motion policy differs from KF evidence",
                clip.name
            ),
        ));
    }
    Ok(())
}

fn exact_accumulator_track_augmentation(
    source_targets: &[String],
    source_count: usize,
    accumulator: &str,
    bones: &[crate::source_rig::BoneDecl],
    mapping: &[usize],
) -> bool {
    let normalize = |name: &str| name.split_whitespace().collect::<Vec<_>>().join(" ").to_ascii_lowercase();
    let accumulator = normalize(accumulator);
    if source_targets.len() != source_count || mapping.len() != source_count + 1
        || source_targets.iter().any(|name| normalize(name) == accumulator)
    {
        return false;
    }
    source_targets.iter().zip(mapping).all(|(source, &index)| {
        bones.get(index).is_some_and(|bone| normalize(&bone.name) == normalize(source))
    }) && mapping.last().and_then(|index| bones.get(*index))
        .is_some_and(|bone| normalize(&bone.name) == accumulator)
}

fn validate_field_decisions(
    receipts: &[SourceRigFieldReceipt],
    source_game: &str,
) -> Result<(), FnvFo3SourceRigBridgeError> {
    if receipts.is_empty() {
        return Err(invalid(
            "field_decisions_missing",
            "explicit per-field decisions are required",
        ));
    }
    for receipt in receipts {
        if receipt.field.trim().is_empty() || !is_lower_blake3(&receipt.value_blake3) {
            return Err(invalid(
                "field_decision_invalid",
                "field decision has an empty field or noncanonical hash",
            ));
        }
        let games_match = match &receipt.decision {
            SourceRigFieldDecision::Source {
                source_game: game,
                source_field,
            } => game.eq_ignore_ascii_case(source_game) && !source_field.trim().is_empty(),
            SourceRigFieldDecision::Derived {
                source_games,
                source_fields,
                policy_id,
            } => {
                !source_games.is_empty()
                    && source_games
                        .iter()
                        .all(|game| game.eq_ignore_ascii_case(source_game))
                    && !source_fields.is_empty()
                    && source_fields.iter().all(|field| !field.trim().is_empty())
                    && !policy_id.trim().is_empty()
            }
            SourceRigFieldDecision::Policy { policy_id } => !policy_id.trim().is_empty(),
        };
        if !games_match {
            return Err(invalid(
                "field_decision_provenance",
                "field decision loses or changes FNV/FO3 provenance",
            ));
        }
    }
    Ok(())
}

fn pair_motion_roles(
    template: CreatureGraphTemplate,
    role: CreatureClipRole,
) -> &'static [MotionRole] {
    match role {
        CreatureClipRole::WeaponAim => &[],
        CreatureClipRole::Idle => &[MotionRole::Idle],
        CreatureClipRole::SwimIdle if template == CreatureGraphTemplate::GroundSwim => {
            &[MotionRole::Idle, MotionRole::Swim]
        }
        CreatureClipRole::FlyIdle if template == CreatureGraphTemplate::GroundFly => {
            &[MotionRole::Idle, MotionRole::Fly]
        }
        CreatureClipRole::SwimIdle | CreatureClipRole::FlyIdle => &[MotionRole::Idle],
        CreatureClipRole::StationaryIdle => &[MotionRole::StationaryOrTurret, MotionRole::Idle],
        CreatureClipRole::GroundForward => &[MotionRole::GroundLocomotion],
        CreatureClipRole::TurnLeft90 | CreatureClipRole::TurnRight90 | CreatureClipRole::TurnLeft | CreatureClipRole::TurnRight => &[MotionRole::Turn],
        CreatureClipRole::Hurt => &[MotionRole::Hurt],
        CreatureClipRole::Death => &[MotionRole::Death],
        CreatureClipRole::GetUp => &[MotionRole::ScriptedIdle],
        CreatureClipRole::ScriptedAction | CreatureClipRole::ScriptedLoop => &[MotionRole::ScriptedIdle],
        CreatureClipRole::MeleeAttack => &[MotionRole::MeleeAttack],
        CreatureClipRole::ProjectileAttack => match template {
            CreatureGraphTemplate::StationaryTurret => &[
                MotionRole::Fire,
                MotionRole::Projectile,
                MotionRole::MeleeAttack,
            ],
            _ => &[MotionRole::RangedAttack, MotionRole::Projectile],
        },
        CreatureClipRole::SwimForward => &[MotionRole::Swim],
        CreatureClipRole::FlyForward => &[MotionRole::Fly],
        CreatureClipRole::ContinuousAttackStart
        | CreatureClipRole::ContinuousAttackLoop
        | CreatureClipRole::ContinuousAttackStop => &[MotionRole::ContinuousOrRobot],
    }
}

fn mapped_race_data(
    family: &CreatureFamilyRecipeJob,
) -> Result<&crate::source_rig::Fo4RaceDataTarget, FnvFo3SourceRigBridgeError> {
    let race = family
        .race_data
        .as_ref()
        .ok_or_else(|| invalid("race_data_missing", "Ready family has no RACE.DATA recipe"))?;
    match &race.derivation.mapping {
        RaceDataMapping::Mapped { target } => Ok(target),
        RaceDataMapping::Missing { .. } => Err(invalid(
            "race_data_missing",
            "Ready family has an incomplete RACE.DATA mapping",
        )),
    }
}

fn asset<'a>(
    family: &'a CreatureFamilyRecipeJob,
    path: &str,
    kind: IndexedAssetKind,
) -> Result<&'a CanonicalAssetClaim, FnvFo3SourceRigBridgeError> {
    let matches = family
        .assets
        .iter()
        .filter(|claim| {
            claim.game == family.rig.game
                && canonical_mesh_path(&claim.path) == canonical_mesh_path(path)
                && claim.kind == kind
        })
        .collect::<Vec<_>>();
    if matches.len() != 1 || !is_lower_blake3(&matches[0].content_hash_blake3) {
        return Err(invalid(
            "asset_evidence_mismatch",
            format!("asset {path:?} has missing, ambiguous, or invalid {kind:?} evidence"),
        ));
    }
    Ok(matches[0])
}

fn request<'a>(
    input: &'a FnvFo3SourceRigAdapterInput,
    role: &SourceRigArtifactRole,
    path: &str,
) -> Result<&'a SourceRigConvertedArtifactRequestReceipt, FnvFo3SourceRigBridgeError> {
    let matches = input
        .artifact_requests
        .iter()
        .filter(|request| &request.role == role && request.runtime_path == path)
        .collect::<Vec<_>>();
    if matches.len() != 1 {
        return Err(invalid(
            "converted_request_missing",
            format!("expected exactly one converted request for {path:?}"),
        ));
    }
    Ok(matches[0])
}

fn visual_closure_target(
    input: &FnvFo3SourceRigAdapterInput,
) -> Result<String, FnvFo3SourceRigBridgeError> {
    let runtime_path = canonical_mesh_path(&input.rig.visual_skeleton_nif);
    let targets = input
        .creature_closure
        .inputs
        .iter()
        .filter(|entry| {
            entry.role == CreatureNifRole::Skeleton
                && canonical_mesh_path(&entry.target_data_relative_path) == runtime_path
        })
        .map(|entry| entry.target_data_relative_path.clone())
        .collect::<Vec<_>>();
    if targets.len() != 1 {
        return Err(invalid(
            "visual_closure_target_missing",
            "visual skeleton path is not uniquely owned by the NIF closure",
        ));
    }
    Ok(targets[0].clone())
}

fn candidate_hash(candidate: &MotionCandidate) -> String {
    let bytes = serde_json::to_vec(candidate).expect("MotionCandidate serialization is infallible");
    blake3::hash(&bytes).to_hex().to_string()
}

pub(crate) fn canonical_fnv_fo3_family_id(rig: &RigFamilyKey) -> String {
    format!(
        "{}|{}",
        game_name(rig.game),
        canonical_mesh_path(&rig.skeleton_path)
    )
}

fn game_name(game: LegacyCreatureGame) -> &'static str {
    match game {
        LegacyCreatureGame::Fnv => "fnv",
        LegacyCreatureGame::Fo3 => "fo3",
    }
}

fn canonical_mesh_path(path: &str) -> String {
    let path = path.trim().replace('\\', "/").to_ascii_lowercase();
    path.strip_prefix("meshes/").unwrap_or(&path).to_string()
}

fn is_lower_blake3(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn invalid(code: &'static str, message: impl Into<String>) -> FnvFo3SourceRigBridgeError {
    FnvFo3SourceRigBridgeError::Invalid {
        code,
        message: message.into(),
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use nif_core_native::creature_closure::{
        CreatureArtifactKind, CreatureClosureRequest, CreatureNifInput, stage_creature_nif_closure,
    };
    use nif_core_native::model::NifFile;
    use tempfile::TempDir;

    use crate::source_rig::{
        BoneDecl, CapabilityClipRole, Capsule, ClipBinding, ClipDecl, ClipMotionPolicy,
        CreatureAttackRecordVariant, CreatureControllerDecl, CreatureMeleeRecordKeyPlan,
        CreatureNpcRecordKeyPlan, CreatureNpcRecordVariant, CreaturePrimaryRecordMapping,
        CreatureRecordEditorIds, CreatureRecordFormKeys, CreatureRecordManifest, EventDecl,
        EventUsage, GraphDeclarations, ScaffoldPaths, SourceRigArtifactProvenance,
        SourceRigRecipeBridgeReceipt, TargetFormKey, VariableDecl, VariableType, VariableValue,
    };

    use super::super::creature_catalog::{CreatureProvenance, LegacyRecordSource};
    use super::super::creature_dependencies::{
        CreatureDependencyOptions, build_creature_dependency_ledger,
    };
    use super::super::creature_mvp_adapter::{
        FnvFo3MvpCandidateDestination, FnvFo3MvpCandidateDisposition,
        FnvFo3MvpCandidatePreparationInput, FnvFo3MvpConvertedArtifact,
        build_fnv_fo3_mvp_candidate_preparations,
    };
    use super::super::creature_recipe::tests::{
        source_rig_bridge_bind_nif_hashes, source_rig_bridge_ranged_test_ledger,
        source_rig_bridge_test_ledger,
    };
    use super::*;
    use crate::ids::{FormKey, SigCode};
    use crate::record::Record;
    use crate::sym::StringInterner;

    include!("creature_scale_tests.rs");

    #[test]
    fn accumulator_track_augmentation_requires_exact_original_tracks_and_missing_root() {
        let bones = vec![
            BoneDecl { name: "Bip01".into(), parent_index: None },
            BoneDecl { name: "Bip01 NonAccum".into(), parent_index: Some(0) },
            BoneDecl { name: "Bip01 Head".into(), parent_index: Some(1) },
        ];
        let source = vec!["Bip01 NonAccum".into(), "Bip01 Head".into()];
        assert!(exact_accumulator_track_augmentation(&source, 2, "Bip01", &bones, &[1, 2, 0]));
        assert!(!exact_accumulator_track_augmentation(&source, 2, "Bip01", &bones, &[2, 1, 0]));
        assert!(!exact_accumulator_track_augmentation(&source, 2, "Bip01", &bones, &[1, 2, 2]));
        assert!(!exact_accumulator_track_augmentation(&source, 2, "Bip01", &bones, &[1, 2, 0, 0]));
        assert!(!exact_accumulator_track_augmentation(&["Bip01".into()], 1, "Bip01", &bones, &[0, 0]));
    }

    fn adapter_fixture(game: LegacyCreatureGame, ranged: bool) -> FnvFo3SourceRigAdapterInput {
        let interner = StringInterner::new();
        let mut ledger = if ranged {
            source_rig_bridge_ranged_test_ledger(&interner)
        } else {
            source_rig_bridge_test_ledger(&interner, game)
        };
        let source_game = game_name(ledger.families[0].rig.game);
        let temp = TempDir::new().unwrap();
        let source_root = temp.path().join("source");
        for path in [
            "meshes/creatures/gecko/skeleton.nif",
            "meshes/creatures/gecko/gecko.nif",
        ] {
            let output = path
                .split('/')
                .fold(source_root.clone(), |root, part| root.join(part));
            fs::create_dir_all(output.parent().unwrap()).unwrap();
            let mut nif = NifFile::new(source_game);
            nif.rebuild_header();
            nif.save(Some(output)).unwrap();
        }
        let closure = stage_creature_nif_closure(&CreatureClosureRequest {
            source_game: source_game.to_string(),
            source_data_root: source_root,
            private_staging_root: temp.path().join("stage"),
            target_namespace: "actors".to_string(),
            texture_fallbacks: std::collections::BTreeMap::new(),
            inputs: vec![
                CreatureNifInput {
                    role: CreatureNifRole::Skeleton,
                    source_data_relative_path: "meshes/creatures/gecko/skeleton.nif".to_string(),
                    source_owner: "recipe-ledger".to_string(),
                    body_variant: None,
                },
                CreatureNifInput {
                    role: CreatureNifRole::Body,
                    source_data_relative_path: "meshes/creatures/gecko/gecko.nif".to_string(),
                    source_owner: "recipe-ledger".to_string(),
                    body_variant: Some("default".to_string()),
                },
            ],
        })
        .unwrap();
        let skeleton_source_blake3 = closure
            .inputs
            .iter()
            .find(|entry| entry.role == CreatureNifRole::Skeleton)
            .unwrap()
            .source_blake3
            .clone();
        let body_source_blake3 = closure
            .inputs
            .iter()
            .find(|entry| entry.role == CreatureNifRole::Body)
            .unwrap()
            .source_blake3
            .clone();
        source_rig_bridge_bind_nif_hashes(
            &mut ledger,
            &[
                ("creatures/gecko/skeleton.nif", &skeleton_source_blake3),
                ("creatures/gecko/gecko.nif", &body_source_blake3),
            ],
        );
        let family = ledger.families[0].clone();
        let skeleton_evidence = asset(
            &family,
            &family.rig.skeleton_path,
            IndexedAssetKind::Skeleton,
        )
        .unwrap()
        .clone();
        let closure_target = |suffix: &str| {
            closure
                .artifacts
                .iter()
                .find(|artifact| {
                    artifact.kind == CreatureArtifactKind::Nif
                        && artifact.target_data_relative_path.ends_with(suffix)
                })
                .unwrap()
                .target_data_relative_path
                .replace('/', "\\")
                .strip_prefix("meshes\\")
                .unwrap()
                .to_string()
        };
        let visual_nif = closure_target("/skeleton.nif");
        let body_nif = closure_target("/gecko.nif");
        let skeleton_path = "Actors\\creatures\\CharacterAssets\\Skeleton.hkx".to_string();
        let selected_roles = if ranged {
            [
                MotionRole::Idle,
                MotionRole::GroundLocomotion,
                MotionRole::RangedAttack,
            ]
        } else {
            [
                MotionRole::Idle,
                MotionRole::GroundLocomotion,
                MotionRole::MeleeAttack,
            ]
        };
        let clip_specs = selected_roles
            .into_iter()
            .map(|motion_role| {
                let candidate = &family
                    .required_motion
                    .iter()
                    .find(|required| required.role == motion_role)
                    .unwrap()
                    .candidates[0];
                let (name, role, trigger) = match motion_role {
                    MotionRole::Idle => ("idle", CreatureClipRole::Idle, None),
                    MotionRole::GroundLocomotion => {
                        ("walk", CreatureClipRole::GroundForward, Some("startWalk"))
                    }
                    MotionRole::MeleeAttack => (
                        "attack",
                        CreatureClipRole::MeleeAttack,
                        Some("meleeGeckoBite"),
                    ),
                    MotionRole::RangedAttack => (
                        "attack",
                        CreatureClipRole::ProjectileAttack,
                        Some("meleeGeckoBite"),
                    ),
                    _ => unreachable!(),
                };
                let path = format!("Actors\\creatures\\Animations\\{name}.hkx");
                (candidate, name, role, trigger, path)
            })
            .collect::<Vec<_>>();
        let clips = clip_specs
            .iter()
            .map(|(candidate, name, _, _, path)| ClipDecl {
                name: (*name).to_string(),
                path: path.clone(),
                binding: ClipBinding {
                    skeleton_path: skeleton_path.clone(),
                    original_skeleton_name: skeleton_evidence
                        .skeleton_runtime_name
                        .clone()
                        .unwrap(),
                    declared_transform_tracks: candidate.binding.transform_track_count,
                    transform_track_to_bone_indices: (0..candidate.binding.transform_track_count)
                        .collect(),
                    declared_float_tracks: candidate.binding.float_track_count,
                    float_track_to_float_slot_indices: candidate
                        .binding
                        .required_float_slots
                        .iter()
                        .map(|slot| {
                            skeleton_evidence
                                .skeleton_float_slots
                                .iter()
                                .position(|candidate| candidate == slot)
                                .unwrap()
                        })
                        .collect(),
                },
                looping: candidate.looping,
            })
            .collect::<Vec<_>>();
        let events = vec![
            EventDecl {
                name: "Idle".to_string(),
                usage: EventUsage::Generic,
                flags: 0,
            },
            EventDecl {
                name: "startWalk".to_string(),
                usage: EventUsage::Generic,
                flags: 0,
            },
            EventDecl {
                name: "meleeGeckoBite".to_string(),
                usage: if ranged {
                    EventUsage::Generic
                } else {
                    EventUsage::MeleeAttack
                },
                flags: 0,
            },
        ];
        let declarations = GraphDeclarations {
            events: events.clone(),
            variables: vec![VariableDecl {
                name: "bGraphDriven".to_string(),
                variable_type: VariableType::Bool,
                initial_value: VariableValue::Bool(true),
            }],
            character_properties: Vec::new(),
        };
        let capsule = family
            .race_data
            .as_ref()
            .unwrap()
            .measured_evidence
            .capsule
            .as_ref()
            .unwrap();
        let rig = CreatureManifest {
            property_sequences: Vec::new(),
            creature_name: "creatures".to_string(),
            visual_skeleton_nif: visual_nif,
            animation_skeleton: crate::source_rig::SkeletonDecl {
                path: skeleton_path,
                runtime_name: skeleton_evidence.skeleton_runtime_name.clone().unwrap(),
                bones: (0_usize..4)
                    .map(|index| BoneDecl {
                        name: if index == 0 {
                            "Bip01".to_string()
                        } else {
                            format!("Bone{index}")
                        },
                        parent_index: index.checked_sub(1),
                    })
                    .collect(),
                float_slots: skeleton_evidence.skeleton_float_slots.clone(),
            },
            controller: CreatureControllerDecl {
                collision_filter_info: 1,
                rigid_body_type: 255,
                model_up_ms: [0.0, 0.0, 1.0, 0.0],
                model_forward_ms: [1.0, 0.0, 0.0, 0.0],
                model_right_ms: [0.0, -1.0, 0.0, 0.0],
                model_scale: 1.0,
            },
            ragdoll: RagdollDisposition::NoRagdoll {
                reason: NoRagdollReason::UnsupportedSourceRagdoll,
            },
            clips,
            idle_clip: "idle".to_string(),
            capsule: Capsule {
                height: capsule.total_height,
                radius: capsule.radius,
            },
            paths: ScaffoldPaths {
                project: "Actors\\creatures\\GeckoProject.hkx".to_string(),
                character: "Actors\\creatures\\Characters\\GeckoCharacter.hkx".to_string(),
                root_behavior: "Actors\\creatures\\Behaviors\\GeckoRootBehavior.hkx".to_string(),
                core_behavior: "Actors\\creatures\\Behaviors\\GeckoCoreBehavior.hkx".to_string(),
            },
            root: declarations.clone(),
            core: declarations,
        };
        let graph = CapabilityGraphManifest {
            source_channels: Vec::new(),
            template: if ranged {
                CreatureGraphTemplate::GroundRangedProjectile
            } else {
                CreatureGraphTemplate::GroundMelee
            },
            roles: clip_specs
                .iter()
                .map(|(_, name, role, trigger, _)| CapabilityClipRole {
                    equipment_index: None,
                    role: *role,
                    state_name: format!("State_{name}"),
                    clip_name: (*name).to_string(),
                    generator: CapabilityRoleGenerator::Single,
                    trigger_event: trigger.map(str::to_string),
                    trigger_aliases: Vec::new(),
                    motion: ClipMotionPolicy::default(),
                })
                .collect(),
            idle_event: "Idle".to_string(),
            explicit_events: events,
            candidate_attack_bindings: Vec::new(),
            source_inert_attack_receipts: Vec::new(),
            overlays: Vec::new(),
        };
        let source_record = ledger
            .records
            .iter()
            .find(|record| {
                matches!(
                    record.disposition,
                    CreatureRecordRecipeDisposition::VisualOwner { .. }
                )
            })
            .unwrap();
        let source = SourceCreatureIdentity {
            namespace: source_game.to_string(),
            plugin: source_record.source.plugin.clone(),
            local_form_id: source_record.source.local,
        };
        let key = |offset: u32| TargetFormKey::new(0xD00_u32 + offset, "B21_CreatureRecipe.esp");
        let base = CreatureRecordManifest {
            target_plugin: "B21_CreatureRecipe.esp".to_string(),
            editor_id_prefix: "B21_".to_string(),
            form_keys: CreatureRecordFormKeys {
                race: key(0),
                npc: key(1),
                skin: key(2),
                armor_addon: key(3),
                body_part_data: key(4),
                unarmed_weapon: key(5),
            },
            editor_ids: CreatureRecordEditorIds {
                race: "B21_GeckoRace".to_string(),
                npc: "B21_GeckoNPC".to_string(),
                skin: "B21_GeckoSkin".to_string(),
                armor_addon: "B21_GeckoAA".to_string(),
                body_part_data: "B21_GeckoBPTD".to_string(),
                unarmed_weapon: "B21_GeckoUnarmed".to_string(),
            },
            display_name: "Gecko".to_string(),
            body_nif,
        };
        let ranged_spell = crate::source_rig::CreatureTargetRecordReference::new(
            "SPEL",
            TargetFormKey::new(0x12_3456, "Fallout4.esm"),
        );
        let attacks = if ranged {
            vec![CreatureAttackRecordVariant {
                id: "spit".to_string(),
                event: "meleeGeckoBite".to_string(),
                primary: true,
                projection: CreatureAttackRecordProjection::SpellAbility {
                    spell: ranged_spell.clone(),
                },
                damage_multiplier: 1.0,
                chance: 1.0,
                strike_angle: 35.0,
                action_point_cost: 20.0,
                target_data: crate::source_rig::CreatureAttackTargetData::default(),
            }]
        } else {
            vec![CreatureAttackRecordVariant {
                id: "bite".to_string(),
                event: "meleeGeckoBite".to_string(),
                primary: true,
                projection: CreatureAttackRecordProjection::MeleeUnarmed {
                    weapon_form_key: base.form_keys.unarmed_weapon.clone(),
                    weapon_editor_id: base.editor_ids.unarmed_weapon.clone(),
                    damage: 10,
                    reach: 0.7,
                    attack_seconds: 0.8,
                },
                damage_multiplier: 1.0,
                chance: 1.0,
                strike_angle: 35.0,
                action_point_cost: 20.0,
                target_data: crate::source_rig::CreatureAttackTargetData::default(),
            }]
        };
        let projection = CreatureRecordProjectionManifest { templates: None, equipment_slots: Vec::new(), base: base.clone(),
        race_keywords: vec![super::super::creature_profile_race_data::controller_scale_keyword()],
        source_primary_identity: source.clone(),
        race_data: mapped_race_data(&family).unwrap().clone(),
        body_parts: CreatureBodyPartProjection::root_only_32("Bip01"),
        body_nif_parts: Vec::new(),
        npc_inventory: Vec::new(),
        npc_spells: Vec::new(),
        npc_death_item: None,
        npc_equipment: Vec::new(),
        variants: vec![CreatureNpcRecordVariant {
            actor_data: None,
            source_identity: source.clone(),
            form_key: base.form_keys.npc.clone(),
            editor_id: base.editor_ids.npc.clone(),
            display_name: "Gecko".to_string(),
            primary: true,
            level: 8,
            health: 100,
            action_points: 50,
            npc_inventory: None,
            npc_equipment: None,
            npc_spells: None,
            npc_perks: Vec::new(),
            npc_death_item: None,
        }],
        attacks, };
        let key_plan = CreatureRecordKeyPlan {
            planned_source_identity: source.clone(),
            base: base.form_keys.clone(),
            armor_addons: vec![base.form_keys.armor_addon.clone()],
            npc_variants: vec![CreatureNpcRecordKeyPlan {
                source_identity: source.clone(),
                form_key: base.form_keys.npc.clone(),
                primary: true,
            }],
            melee_attacks: if ranged {
                Vec::new()
            } else {
                vec![CreatureMeleeRecordKeyPlan {
                    attack_id: "bite".to_string(),
                    form_key: base.form_keys.unarmed_weapon.clone(),
                    primary: true,
                }]
            },
        };
        let batch_intent = SourceRigRecordBatchIntent {
            family_id: canonical_fnv_fo3_family_id(&family.rig),
            primary_mapping: CreaturePrimaryRecordMapping {
                source: source.clone(),
                target: base.form_keys.npc.clone(),
            },
            required_target_records: if ranged {
                vec![super::super::creature_profile_race_data::controller_scale_keyword(), ranged_spell]
            } else {
                vec![super::super::creature_profile_race_data::controller_scale_keyword()]
            },
        };
        let skeleton_claim = asset(
            &family,
            &family.rig.skeleton_path,
            IndexedAssetKind::Skeleton,
        )
        .unwrap();
        let mut artifact_requests = vec![SourceRigConvertedArtifactRequestReceipt {
            role: SourceRigArtifactRole::AnimationSkeleton,
            runtime_path: rig.animation_skeleton.path.clone(),
            source_game: source_game.to_string(),
            source_evidence_blake3: skeleton_claim.content_hash_blake3.clone(),
            conversion_request_blake3: blake3::hash(b"skeleton request").to_hex().to_string(),
        }];
        artifact_requests.extend(clip_specs.iter().map(|(candidate, name, _, _, path)| {
            SourceRigConvertedArtifactRequestReceipt {
                role: SourceRigArtifactRole::AnimationClip {
                    clip_name: (*name).to_string(),
                },
                runtime_path: path.clone(),
                source_game: source_game.to_string(),
                source_evidence_blake3: candidate_hash(candidate),
                conversion_request_blake3: blake3::hash(format!("request {name}").as_bytes())
                    .to_hex()
                    .to_string(),
            }
        }));
        let artifact_receipts = artifact_requests
            .iter()
            .map(|request| SourceRigArtifactReceipt {
                role: request.role.clone(),
                provenance: match request.role {
                    SourceRigArtifactRole::AnimationSkeleton => {
                        SourceRigArtifactProvenance::ReconstructedSourceSkeleton
                    }
                    SourceRigArtifactRole::AnimationClip { .. } => {
                        SourceRigArtifactProvenance::ConvertedSourceClip
                    }
                    SourceRigArtifactRole::Ragdoll => {
                        SourceRigArtifactProvenance::ReconstructedSourceRagdoll
                    }
                },
                runtime_path: request.runtime_path.clone(),
                byte_len: 32,
                blake3: blake3::hash(request.runtime_path.as_bytes())
                    .to_hex()
                    .to_string(),
            })
            .collect::<Vec<_>>();
        let closure_request_hash = closure.request_blake3.clone();
        let ledger_blake3 = ledger.canonical_hash_blake3().unwrap();
        let record = ledger
            .records
            .iter()
            .find(|record| {
                record.source.local == source.local_form_id
                    && record.source.plugin.eq_ignore_ascii_case(&source.plugin)
            })
            .unwrap();
        let canonical_json =
            selected_pair_ledger_canonical_json(&ledger, record, &family, &ledger_blake3, None, None).unwrap();
        let pair_ledger = SourceRigPairLedgerEvidence {
            source_root_identity: None,
            schema: FNV_FO3_SOURCE_RIG_BRIDGE_SCHEMA.to_string(),
            canonical_json: canonical_json.clone(),
            ledger_blake3,
            canonical_json_blake3: blake3::hash(canonical_json.as_bytes()).to_hex().to_string(),
            source_identity: source,
            family_id: batch_intent.family_id.clone(),
        };
        let selected_family = SourceRigSelectedFamilyEvidence {
            source_games: vec![source_game.to_string()],
            actual_root_bone: "Bip01".to_string(),
            visual_skeleton_nif: rig.visual_skeleton_nif.clone(),
            visual_creature_closure_target_path: closure
                .inputs
                .iter()
                .find(|entry| entry.role == CreatureNifRole::Skeleton)
                .unwrap()
                .target_data_relative_path
                .clone(),
            animation_skeleton: rig.animation_skeleton.clone(),
            controller: rig.controller,
            graph: graph.clone(),
            clips: rig.clips.clone(),
            ragdoll: rig.ragdoll.clone(),
            race_data: projection.race_data.clone(),
        };
        let bridge = SourceRigRecipeBridgeInput {
            pair_ledger,
            selected_family,
            creature_closure: closure.clone(),
            creature_closure_request_blake3: closure_request_hash.clone(),
            artifact_requests: artifact_requests.clone(),
            artifact_receipts: artifact_receipts.clone(),
            rig: rig.clone(),
            graph: graph.clone(),
            projection: projection.clone(),
            key_plan: key_plan.clone(),
            actor_action_records: Vec::new(),
            batch_intent: batch_intent.clone(),
            field_receipts: Vec::new(),
        };
        let receipt: SourceRigRecipeBridgeReceipt = bridge.bridge_receipt();
        let field_receipts =
            SourceRigExecutableRecipe::required_field_receipts_from_source_for_intent(
                &rig,
                &graph,
                &projection,
                &key_plan,
                &batch_intent,
                Some(&receipt),
                source_game,
            );
        FnvFo3SourceRigAdapterInput {
            profile_race_data: None,
            template_branch: None,
            ledger: Arc::new(ledger),
            family: family.rig.clone(),
            creature_closure: closure,
            creature_closure_request_blake3: closure_request_hash,
            artifact_requests,
            artifact_receipts,
            runtime_model_closure: None,
            rig,
            graph,
            projection,
            key_plan,
            actor_action_records: Vec::new(),
            batch_intent,
            field_receipts,
        }
    }

    #[test]
    fn template_branch_identity_requires_exact_path_and_source_game() {
        use super::super::creature_templates::{LegacyTemplateBranchReceipt, LegacyTemplatePlan, LegacyTemplateProfile};
        let mut input = adapter_fixture(LegacyCreatureGame::Fnv, false);
        let family = input.ledger.families[0].clone();
        let root = input.ledger.records.iter().find(|record| record.source.local == input.projection.source_primary_identity.local_form_id).unwrap().source.clone();
        let profile = LegacyTemplateProfile { root: root.clone(), owners: std::array::from_fn(|_| root.clone()), actor_flags: 0 };
        let plan = LegacyTemplatePlan::Leveled {
            source: super::super::creature_catalog::StableFormKey { local: 0xA00, plugin: root.plugin.clone() },
            entries: vec![LegacyTemplatePlan::Creature(profile.clone()), LegacyTemplatePlan::Creature(profile)],
        };
        let branch = LegacyTemplateBranchReceipt::new(&plan, vec![1], LegacyCreatureGame::Fnv, false).unwrap();
        input.projection.source_primary_identity = branch.generated_identity.clone();
        input.projection.variants[0].source_identity = branch.generated_identity.clone();
        validate_template_branch_identity(&input, &family, &branch).unwrap();
        let mut changed_path = branch.clone();
        changed_path.entry_indices = vec![0];
        assert!(validate_template_branch_identity(&input, &family, &changed_path).is_err());
        let foreign = LegacyTemplateBranchReceipt::new(&plan, vec![1], LegacyCreatureGame::Fo3, false).unwrap();
        input.projection.source_primary_identity = foreign.generated_identity.clone();
        input.projection.variants[0].source_identity = foreign.generated_identity.clone();
        assert!(validate_template_branch_identity(&input, &family, &foreign).is_err());
        input.projection.source_primary_identity = branch.generated_identity.clone();
        input.projection.variants[0].source_identity = branch.generated_identity.clone();
        let mut forbidden = input.clone();
        Arc::make_mut(&mut forbidden.ledger).records.iter_mut().find(|record| record.source == root).unwrap().provenance.game = LegacyCreatureGame::Fo3;
        assert!(validate_template_branch_identity(&forbidden, &family, &branch).is_err());
    }

    #[test]
    fn profile_race_stats_change_records_without_mutating_shared_capsule_artifacts() {
        use crate::ids::SubrecordSig;
        use crate::record::{FieldEntry, FieldValue};
        let interner = StringInterner::new();
        let mut input = adapter_fixture(LegacyCreatureGame::Fnv, false);
        let family = input.ledger.families[0].clone();
        let owner = family.race_data.as_ref().unwrap().selection.source_creature.clone();
        let mut source = Record::new(SigCode(*b"CREA"),FormKey { local:owner.local,plugin:interner.intern(&owner.plugin) });
        let mut acbs = [0_u8;24];
        acbs[8..10].copy_from_slice(&12_u16.to_le_bytes());
        acbs[14..16].copy_from_slice(&100_u16.to_le_bytes());
        source.fields = vec![
            FieldEntry {sig:SubrecordSig(*b"ACBS"),value:FieldValue::Bytes(acbs.to_vec().into())},
            FieldEntry {sig:SubrecordSig(*b"BNAM"),value:FieldValue::Float(1.0)},
            FieldEntry {sig:SubrecordSig(*b"TNAM"),value:FieldValue::Float(90.0)},
        ].into();
        let first = super::super::creature_profile_race_data::prepare(&family,&owner,&source,&interner).unwrap();
        source.fields[1].value = FieldValue::Float(1.5);
        acbs[14..16].copy_from_slice(&150_u16.to_le_bytes());
        source.fields[0].value = FieldValue::Bytes(acbs.to_vec().into());
        let second = super::super::creature_profile_race_data::prepare(&family,&owner,&source,&interner).unwrap();
        let RaceDataMapping::Mapped {target:first_target} = &first.derivation.mapping else {panic!("mapped first profile")};
        let RaceDataMapping::Mapped {target:second_target} = &second.derivation.mapping else {panic!("mapped second profile")};
        assert_eq!(second_target.male_height,first_target.male_height*1.5);
        assert_eq!(second_target.acceleration_rate,first_target.acceleration_rate*1.5);
        let capsule = input.rig.capsule.clone();
        input.projection.race_data = second_target.clone();
        input.profile_race_data = Some(second.clone());
        validate_family_contract(&input,&family,"fnv").unwrap();
        assert_eq!(input.rig.capsule,capsule);
        let json = serde_json::to_string(&second).unwrap();
        let restored = serde_json::from_str(&json).unwrap();
        super::super::creature_profile_race_data::validate(&restored,&family).unwrap();
        let mut forged = second;
        forged.speed_multiplier = 200;
        assert!(super::super::creature_profile_race_data::validate(&forged,&family).is_err());
    }

    #[test]
    fn ready_recipes_are_deterministic_roundtrip_and_keep_source_identity() {
        let input = adapter_fixture(LegacyCreatureGame::Fnv, false);
        let recipe = build_fnv_fo3_source_rig_executable_recipe(input).unwrap();
        let json = recipe.canonical_json().unwrap();
        assert_eq!(SourceRigExecutableRecipe::from_json(&json).unwrap(), recipe);
        assert_eq!(recipe.stable_hash_blake3().unwrap().len(), 64);
        let batch = recipe
            .rebuild_record_family_batch(&StringInterner::new())
            .unwrap();
        assert_eq!(
            batch
                .closures
                .iter()
                .map(|closure| closure.closure.records.len())
                .sum::<usize>(),
            6
        );

        let recipe = build_fnv_fo3_source_rig_executable_recipe(adapter_fixture(
            LegacyCreatureGame::Fo3,
            false,
        ))
        .unwrap();
        let receipt = recipe.bridge_receipt.unwrap();
        assert_eq!(receipt.source_identity.namespace, "fo3");
        assert!(receipt.family_id.starts_with("fo3|"));

        let recipe = build_fnv_fo3_source_rig_executable_recipe(adapter_fixture(
            LegacyCreatureGame::Fnv,
            true,
        ))
        .unwrap();
        let batch = recipe
            .rebuild_record_family_batch(&StringInterner::new())
            .unwrap();
        assert!(batch.closures.iter().all(|closure| {
            closure
                .closure
                .records
                .iter()
                .all(|record| record.sig.as_str() != "WEAP")
        }));
    }

    #[test]
    fn candidate_clones_share_the_full_ledger_and_emit_only_selected_pair_evidence() {
        let input = adapter_fixture(LegacyCreatureGame::Fnv, false);
        let clone = input.clone();
        let full_ledger_bytes = input.ledger.canonical_json().unwrap().len();

        assert!(Arc::ptr_eq(&input.ledger, &clone.ledger));
        let bridge = build_fnv_fo3_source_rig_bridge_input(input).unwrap();
        assert!(bridge.pair_ledger.canonical_json.len() < full_ledger_bytes);
    }

    #[test]
    fn absent_optional_body_variant_is_not_required_in_the_staged_nif_closure() {
        let mut input = adapter_fixture(LegacyCreatureGame::Fnv, false);
        Arc::make_mut(&mut input.ledger).families[0].body_variants[0]
            .body_paths
            .push("creatures/gecko/optional_missing_body.nif".to_string());
        source_rig_bridge_bind_nif_hashes(Arc::make_mut(&mut input.ledger), &[]);
        let ledger_blake3 = input.ledger.canonical_hash_blake3().unwrap();
        let family = &input.ledger.families[0];
        let record = input
            .ledger
            .records
            .iter()
            .find(|record| {
                record.source.local == input.projection.source_primary_identity.local_form_id
                    && record
                        .source
                        .plugin
                        .eq_ignore_ascii_case(&input.projection.source_primary_identity.plugin)
            })
            .unwrap();
        let canonical_json =
            selected_pair_ledger_canonical_json(&input.ledger, record, family, &ledger_blake3, None, None)
                .unwrap();
        let receipt = SourceRigRecipeBridgeReceipt {
            source_root_identity: None,
            version: crate::source_rig::SOURCE_RIG_BRIDGE_VERSION,
            ledger_schema: FNV_FO3_SOURCE_RIG_BRIDGE_SCHEMA.to_string(),
            ledger_blake3,
            ledger_canonical_json_blake3: blake3::hash(canonical_json.as_bytes())
                .to_hex()
                .to_string(),
            source_identity: input.projection.source_primary_identity.clone(),
            family_id: input.batch_intent.family_id.clone(),
            creature_closure_request_blake3: input.creature_closure_request_blake3.clone(),
            creature_closure_receipt_blake3: input.creature_closure.receipt_hash.clone(),
            artifact_requests: input.artifact_requests.clone(),
            artifact_receipts: input.artifact_receipts.clone(),
        };
        input.field_receipts =
            SourceRigExecutableRecipe::required_field_receipts_from_source_for_intent(
                &input.rig,
                &input.graph,
                &input.projection,
                &input.key_plan,
                &input.batch_intent,
                Some(&receipt),
                "fnv",
            );

        build_fnv_fo3_source_rig_executable_recipe(input).unwrap();
    }

    #[test]
    fn generated_locomotion_and_melee_triggers_need_not_be_kf_annotations() {
        let input = adapter_fixture(LegacyCreatureGame::Fnv, false);
        let family = input
            .ledger
            .families
            .iter()
            .find(|family| family.rig == input.family)
            .unwrap();
        let mut role = input
            .graph
            .roles
            .iter()
            .find(|role| role.role == CreatureClipRole::GroundForward)
            .unwrap()
            .clone();
        role.trigger_event = Some("moveStart".to_string());
        let clip = input
            .rig
            .clips
            .iter()
            .find(|clip| clip.name == role.clip_name)
            .unwrap();
        let candidate = family
            .required_motion
            .iter()
            .find(|motion| motion.role == MotionRole::GroundLocomotion)
            .unwrap()
            .candidates
            .first()
            .unwrap();

        validate_candidate(&input, family, &role, clip, candidate).unwrap();

        let input = adapter_fixture(LegacyCreatureGame::Fnv, false);
        let family = input
            .ledger
            .families
            .iter()
            .find(|family| family.rig == input.family)
            .unwrap();
        let mut role = input
            .graph
            .roles
            .iter()
            .find(|role| role.role == CreatureClipRole::MeleeAttack)
            .unwrap()
            .clone();
        role.trigger_event = Some("melee_fnvfo3_01".to_string());
        let clip = input
            .rig
            .clips
            .iter()
            .find(|clip| clip.name == role.clip_name)
            .unwrap();
        let candidate = family
            .required_motion
            .iter()
            .find(|motion| motion.role == MotionRole::MeleeAttack)
            .unwrap()
            .candidates
            .first()
            .unwrap();

        validate_candidate(&input, family, &role, clip, candidate).unwrap();
    }

    #[test]
    fn mvp_adapter_returns_one_hash_bound_ready_candidate() {
        let source_rig = adapter_fixture(LegacyCreatureGame::Fnv, false);
        let recipe_ledger = source_rig.ledger.clone();
        let source = &recipe_ledger
            .records
            .iter()
            .find(|record| {
                matches!(
                    record.disposition,
                    CreatureRecordRecipeDisposition::VisualOwner { .. }
                )
            })
            .unwrap()
            .source;
        let interner = StringInterner::new();
        let record = Record::new(
            SigCode(*b"CREA"),
            FormKey {
                local: source.local,
                plugin: interner.intern(&source.plugin),
            },
        );
        let provenance = CreatureProvenance {
            game: LegacyCreatureGame::Fnv,
            source_plugin: source.plugin.clone(),
            precedence: 0,
        };
        let dependency_ledger = build_creature_dependency_ledger(
            &[LegacyRecordSource {
                record: &record,
                provenance,
            }],
            CreatureDependencyOptions {
                expected_creature_winners: Some(1),
            },
            &interner,
        )
        .unwrap();
        let converted_artifacts = source_rig
            .artifact_receipts
            .iter()
            .map(|receipt| FnvFo3MvpConvertedArtifact {
                runtime_path: receipt.runtime_path.clone(),
                source_path: std::path::PathBuf::from(format!(
                    "converted/{}",
                    receipt.runtime_path.replace('\\', "/")
                )),
            })
            .collect();
        let family = source_rig.family.clone();
        let ledger = build_fnv_fo3_mvp_candidate_preparations(
            &dependency_ledger,
            &recipe_ledger,
            vec![FnvFo3MvpCandidateDestination {
                source: source.clone(),
                output_slug: "gecko-000100".to_string(),
                publish_root: std::path::PathBuf::from("Meshes/Actors/Gecko"),
            }],
            vec![FnvFo3MvpCandidatePreparationInput {
                source: source.clone(),
                family,
                source_rig,
                closure_staged_data_root: std::path::PathBuf::from("stage/actors"),
                converted_artifacts,
            }],
        )
        .unwrap();
        assert_eq!(ledger.candidate_count, 1);
        assert_eq!(ledger.ready_count, 1);
        assert_eq!(ledger.blocked_count, 0);
        assert!(matches!(
            ledger.candidates[0].disposition,
            FnvFo3MvpCandidateDisposition::Ready { .. }
        ));
        assert_eq!(ledger.content_hash_blake3.len(), 64);
    }

    #[test]
    fn property_sequence_migration_requires_exact_source_controller_evidence() {
        let mut input = adapter_fixture(LegacyCreatureGame::Fnv, false);
        let family = input.ledger.families[0].clone();
        let mut candidate = family.required_motion.iter().find(|motion| motion.role == MotionRole::Idle).unwrap().candidates[0].clone();
        candidate.binding.float_track_count = 2;
        candidate.binding.required_float_slots = vec!["screen-u".into(), "screen-v".into()];
        let clip = input.rig.clips[0].clone();
        let motion = input.graph.roles[0].motion;
        assert!(validate_candidate_binding(&input, &family, &clip, &candidate, motion, false).is_err());
        input.rig.property_sequences.push(crate::source_rig::CreaturePropertySequence {
            clip_name: clip.name.clone(), crop_start_bits: 0, crop_end_bits: 0,
            looping: clip.looping, sequence_names: vec!["BodyScreenIdle".into()],
            source_float_slots: candidate.binding.required_float_slots.clone(),
        });
        validate_candidate_binding(&input, &family, &clip, &candidate, motion, false).unwrap();
        input.rig.property_sequences[0].source_float_slots.swap(0, 1);
        assert!(matches!(validate_candidate_binding(&input, &family, &clip, &candidate, motion, false), Err(FnvFo3SourceRigBridgeError::Invalid { code: "clip_property_binding_mismatch", .. })));
    }

    #[test]
    fn tampered_or_mismatched_inputs_produce_no_recipe() {
        let mut root = adapter_fixture(LegacyCreatureGame::Fnv, false);
        root.rig.animation_skeleton.bones[0].name = "WrongRoot".to_string();
        assert!(build_fnv_fo3_source_rig_executable_recipe(root).is_err());
        let mut capsule = adapter_fixture(LegacyCreatureGame::Fnv, false);
        capsule.rig.capsule.radius += 0.25;
        assert!(build_fnv_fo3_source_rig_executable_recipe(capsule).is_err());
        let mut skeleton_name = adapter_fixture(LegacyCreatureGame::Fnv, false);
        skeleton_name.rig.animation_skeleton.runtime_name = "PathDerivedGecko".to_string();
        assert!(matches!(
            build_fnv_fo3_source_rig_executable_recipe(skeleton_name),
            Err(FnvFo3SourceRigBridgeError::Invalid {
                code: "skeleton_identity_mismatch",
                ..
            })
        ));
        let mut clip_name = adapter_fixture(LegacyCreatureGame::Fnv, false);
        clip_name.rig.clips[0].binding.original_skeleton_name = "PathDerivedGecko".to_string();
        assert!(matches!(
            build_fnv_fo3_source_rig_executable_recipe(clip_name),
            Err(FnvFo3SourceRigBridgeError::Invalid {
                code: "clip_binding_mismatch",
                ..
            })
        ));
        let mut float_map = adapter_fixture(LegacyCreatureGame::Fnv, false);
        float_map.rig.clips[0].binding.declared_float_tracks = 1;
        float_map.rig.clips[0]
            .binding
            .float_track_to_float_slot_indices = vec![0];
        assert!(matches!(
            build_fnv_fo3_source_rig_executable_recipe(float_map),
            Err(FnvFo3SourceRigBridgeError::Invalid {
                code: "clip_binding_mismatch",
                ..
            })
        ));

        let mut input = adapter_fixture(LegacyCreatureGame::Fnv, false);
        Arc::make_mut(&mut input.ledger).records[0].editor_id = Some("tampered".to_string());
        assert!(matches!(
            build_fnv_fo3_source_rig_executable_recipe(input),
            Err(FnvFo3SourceRigBridgeError::Ledger(_))
        ));

        let mut input = adapter_fixture(LegacyCreatureGame::Fnv, false);
        input.projection.source_primary_identity.local_form_id += 1;
        assert!(build_fnv_fo3_source_rig_executable_recipe(input).is_err());

        let mut input = adapter_fixture(LegacyCreatureGame::Fnv, false);
        let changed_hash = blake3::hash(b"same path but changed source bytes")
            .to_hex()
            .to_string();
        source_rig_bridge_bind_nif_hashes(
            Arc::make_mut(&mut input.ledger),
            &[("creatures/gecko/gecko.nif", &changed_hash)],
        );
        assert!(matches!(
            build_fnv_fo3_source_rig_executable_recipe(input),
            Err(FnvFo3SourceRigBridgeError::Invalid {
                code: "nif_closure_source_hash",
                ..
            })
        ));

        let mut hash = adapter_fixture(LegacyCreatureGame::Fnv, false);
        hash.artifact_requests[0].source_evidence_blake3.clear();
        assert!(build_fnv_fo3_source_rig_executable_recipe(hash).is_err());
        let mut ragdoll = adapter_fixture(LegacyCreatureGame::Fnv, false);
        ragdoll.rig.ragdoll = RagdollDisposition::deferred();
        assert!(build_fnv_fo3_source_rig_executable_recipe(ragdoll).is_err());
    }
}
