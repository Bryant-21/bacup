//! Evidence-bound construction of executable source-rig recipes.

use std::collections::{BTreeMap, BTreeSet};

use nif_core_native::creature_closure::{
    CreatureArtifactKind, CreatureClosureReceipt, CreatureCollisionDisposition, CreatureNifRole,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::{
    CapabilityGraphManifest, ClipDecl, CreatureActorActionRecordPlan, CreatureControllerDecl,
    CreatureManifest, CreatureRecordKeyPlan, CreatureRecordProjectionManifest, Fo4RaceDataTarget,
    RagdollDisposition, SkeletonDecl, SourceCreatureIdentity, SourceRigArtifactProvenance,
    SourceRigArtifactReceipt, SourceRigArtifactRole, SourceRigExecutableRecipe,
    SourceRigFieldReceipt, SourceRigRecipeError, SourceRigRecordBatchIntent,
};

pub const SOURCE_RIG_BRIDGE_VERSION: u32 = 1;

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct SourceRigPairLedgerEvidence {
    pub schema: String,
    pub canonical_json: String,
    pub ledger_blake3: String,
    pub canonical_json_blake3: String,
    pub source_identity: SourceCreatureIdentity,
    pub family_id: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct SourceRigSelectedFamilyEvidence {
    pub source_games: Vec<String>,
    pub actual_root_bone: String,
    pub visual_skeleton_nif: String,
    pub visual_creature_closure_target_path: String,
    pub animation_skeleton: SkeletonDecl,
    pub controller: CreatureControllerDecl,
    pub graph: CapabilityGraphManifest,
    pub clips: Vec<ClipDecl>,
    pub ragdoll: RagdollDisposition,
    pub race_data: Fo4RaceDataTarget,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct SourceRigConvertedArtifactRequestReceipt {
    pub role: SourceRigArtifactRole,
    pub runtime_path: String,
    pub source_game: String,
    pub source_evidence_blake3: String,
    pub conversion_request_blake3: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct SourceRigRecipeBridgeReceipt {
    pub version: u32,
    pub ledger_schema: String,
    pub ledger_blake3: String,
    pub ledger_canonical_json_blake3: String,
    pub source_identity: SourceCreatureIdentity,
    pub family_id: String,
    pub creature_closure_request_blake3: String,
    pub creature_closure_receipt_blake3: String,
    pub artifact_requests: Vec<SourceRigConvertedArtifactRequestReceipt>,
    pub artifact_receipts: Vec<SourceRigArtifactReceipt>,
}

#[derive(Clone, Debug)]
pub struct SourceRigRecipeBridgeInput {
    pub pair_ledger: SourceRigPairLedgerEvidence,
    pub selected_family: SourceRigSelectedFamilyEvidence,
    pub creature_closure: CreatureClosureReceipt,
    pub creature_closure_request_blake3: String,
    pub artifact_requests: Vec<SourceRigConvertedArtifactRequestReceipt>,
    pub artifact_receipts: Vec<SourceRigArtifactReceipt>,
    pub rig: CreatureManifest,
    pub graph: CapabilityGraphManifest,
    pub projection: CreatureRecordProjectionManifest,
    pub key_plan: CreatureRecordKeyPlan,
    pub actor_action_records: Vec<CreatureActorActionRecordPlan>,
    pub batch_intent: SourceRigRecordBatchIntent,
    pub field_receipts: Vec<SourceRigFieldReceipt>,
}

impl SourceRigRecipeBridgeInput {
    pub fn bridge_receipt(&self) -> SourceRigRecipeBridgeReceipt {
        let mut receipt = SourceRigRecipeBridgeReceipt {
            version: SOURCE_RIG_BRIDGE_VERSION,
            ledger_schema: self.pair_ledger.schema.clone(),
            ledger_blake3: self.pair_ledger.ledger_blake3.clone(),
            ledger_canonical_json_blake3: self.pair_ledger.canonical_json_blake3.clone(),
            source_identity: self.pair_ledger.source_identity.clone(),
            family_id: self.pair_ledger.family_id.clone(),
            creature_closure_request_blake3: self.creature_closure_request_blake3.clone(),
            creature_closure_receipt_blake3: self.creature_closure.receipt_hash.clone(),
            artifact_requests: self.artifact_requests.clone(),
            artifact_receipts: self.artifact_receipts.clone(),
        };
        normalize_bridge_receipt(&mut receipt);
        receipt
    }

    pub fn required_field_receipts_from_policy(
        &self,
        policy_id: impl Into<String>,
    ) -> Vec<SourceRigFieldReceipt> {
        SourceRigExecutableRecipe::required_field_receipts_from_policy_for_intent_with_actor_actions(
            &self.rig,
            &self.graph,
            &self.projection,
            &self.key_plan,
            &self.actor_action_records,
            &self.batch_intent,
            Some(&self.bridge_receipt()),
            policy_id,
        )
    }
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum SourceRigBridgeError {
    #[error("source-rig bridge contract failed ({code}): {message}")]
    Invalid { code: &'static str, message: String },
    #[error("source-rig bridge rejected the pair ledger: {0}")]
    Ledger(String),
    #[error("source-rig bridge rejected the creature closure: {0}")]
    CreatureClosure(String),
    #[error("source-rig executable recipe failed validation: {0}")]
    Recipe(String),
}

pub fn build_source_rig_executable_recipe(
    mut input: SourceRigRecipeBridgeInput,
) -> Result<SourceRigExecutableRecipe, SourceRigBridgeError> {
    validate_pair_ledger(&input)?;
    normalize_family_evidence(&mut input.selected_family);
    validate_selected_family(&input)?;
    input
        .creature_closure
        .validate()
        .map_err(|error| SourceRigBridgeError::CreatureClosure(error.to_string()))?;
    let mut bridge_receipt = input.bridge_receipt();
    normalize_bridge_receipt(&mut bridge_receipt);
    validate_converted_artifacts(&input, &bridge_receipt)?;
    validate_articulated_collision(&input)?;

    SourceRigExecutableRecipe::new_bridged_with_actor_actions(
        input.rig,
        input.graph,
        input.projection,
        input.key_plan,
        input.actor_action_records,
        input.batch_intent,
        bridge_receipt,
        input.field_receipts,
    )
    .map_err(|error| SourceRigBridgeError::Recipe(error.to_string()))
}

fn validate_pair_ledger(input: &SourceRigRecipeBridgeInput) -> Result<(), SourceRigBridgeError> {
    let ledger = &input.pair_ledger;
    if ledger.schema.trim().is_empty()
        || ledger.family_id.trim().is_empty()
        || ledger.canonical_json.trim().is_empty()
    {
        return Err(invalid(
            "pair_ledger_identity",
            "ledger schema, family, and JSON are required",
        ));
    }
    serde_json::from_str::<serde_json::Value>(&ledger.canonical_json)
        .map_err(|error| SourceRigBridgeError::Ledger(error.to_string()))?;
    let actual = blake3::hash(ledger.canonical_json.as_bytes())
        .to_hex()
        .to_string();
    if !is_blake3(&ledger.ledger_blake3)
        || !ledger.canonical_json_blake3.eq_ignore_ascii_case(&actual)
    {
        return Err(invalid(
            "pair_ledger_hash",
            "pair-ledger canonical JSON does not match its bound BLAKE3",
        ));
    }
    if ledger.source_identity != input.projection.source_primary_identity
        || ledger.source_identity != input.key_plan.planned_source_identity
        || ledger.source_identity != input.batch_intent.primary_mapping.source
    {
        return Err(invalid(
            "pair_ledger_source_identity",
            "pair-ledger source identity differs from projection, key plan, or mapping",
        ));
    }
    if ledger.family_id != input.batch_intent.family_id {
        return Err(invalid(
            "pair_ledger_family",
            "selected pair-ledger family differs from batch intent",
        ));
    }
    Ok(())
}

fn normalize_family_evidence(evidence: &mut SourceRigSelectedFamilyEvidence) {
    evidence
        .source_games
        .sort_by_key(|game| game.to_ascii_lowercase());
    evidence
        .source_games
        .dedup_by(|left, right| left.eq_ignore_ascii_case(right));
    evidence.clips.sort_by_key(|clip| {
        (
            clip.name.to_ascii_lowercase(),
            clip.path.to_ascii_lowercase(),
        )
    });
    normalize_graph(&mut evidence.graph);
}

fn normalize_graph(graph: &mut CapabilityGraphManifest) {
    graph.roles.sort_by_key(|role| {
        (
            role.role,
            role.state_name.to_ascii_lowercase(),
            role.clip_name.to_ascii_lowercase(),
        )
    });
    graph
        .explicit_events
        .sort_by_key(|event| event.name.to_ascii_lowercase());
    graph
        .overlays
        .sort_by_key(|overlay| overlay.name.to_ascii_lowercase());
}

fn validate_selected_family(
    input: &SourceRigRecipeBridgeInput,
) -> Result<(), SourceRigBridgeError> {
    let evidence = &input.selected_family;
    if evidence.source_games.is_empty()
        || evidence
            .source_games
            .iter()
            .any(|game| game.trim().is_empty())
    {
        return Err(invalid(
            "family_source_games",
            "selected family must retain at least one explicit source game",
        ));
    }
    let roots = input
        .rig
        .animation_skeleton
        .bones
        .iter()
        .filter(|bone| bone.parent_index.is_none())
        .collect::<Vec<_>>();
    if !roots
        .iter()
        .any(|root| root.name == evidence.actual_root_bone)
    {
        return Err(invalid(
            "family_root",
            "selected actual root is not a manifest skeleton root",
        ));
    }
    if input.rig.visual_skeleton_nif != evidence.visual_skeleton_nif
        || input.rig.animation_skeleton != evidence.animation_skeleton
        || input.rig.controller != evidence.controller
        || input.rig.ragdoll != evidence.ragdoll
        || input.projection.race_data != evidence.race_data
    {
        return Err(invalid(
            "family_projection",
            "rig, controller, ragdoll, or RACE.DATA differs from selected family evidence",
        ));
    }
    let mut graph = input.graph.clone();
    normalize_graph(&mut graph);
    if graph != evidence.graph {
        return Err(invalid(
            "family_graph",
            "graph template, roles, events, or overlays differ from selected family evidence",
        ));
    }
    let mut clips = input.rig.clips.clone();
    clips.sort_by_key(|clip| {
        (
            clip.name.to_ascii_lowercase(),
            clip.path.to_ascii_lowercase(),
        )
    });
    if clips != evidence.clips {
        return Err(invalid(
            "family_clips",
            "clip declarations differ from selected family evidence",
        ));
    }
    let referenced = graph
        .roles
        .iter()
        .map(|role| role.clip_name.as_str())
        .chain(
            graph
                .overlays
                .iter()
                .map(|overlay| overlay.clip_name.as_str()),
        )
        .collect::<BTreeSet<_>>();
    let declared = clips
        .iter()
        .map(|clip| clip.name.as_str())
        .collect::<BTreeSet<_>>();
    if referenced != declared || referenced.len() != clips.len() {
        return Err(invalid(
            "family_clips",
            "every selected clip must be declared exactly once and graph-referenced",
        ));
    }
    Ok(())
}

fn normalize_bridge_receipt(receipt: &mut SourceRigRecipeBridgeReceipt) {
    receipt.artifact_requests.sort_by_key(|request| {
        (
            request.runtime_path.to_ascii_lowercase(),
            request.role.clone(),
        )
    });
    receipt.artifact_receipts.sort_by_key(|artifact| {
        (
            artifact.runtime_path.to_ascii_lowercase(),
            artifact.role.clone(),
        )
    });
}

fn validate_converted_artifacts(
    input: &SourceRigRecipeBridgeInput,
    bridge: &SourceRigRecipeBridgeReceipt,
) -> Result<(), SourceRigBridgeError> {
    let expected = expected_artifacts(&input.rig, &input.graph)?;
    if bridge.artifact_requests.len() != expected.len()
        || bridge.artifact_receipts.len() != expected.len()
    {
        return Err(invalid(
            "converted_artifact_closure",
            "converted artifact requests and receipts must exactly cover the executable closure",
        ));
    }
    let source_games = input
        .selected_family
        .source_games
        .iter()
        .map(|game| game.to_ascii_lowercase())
        .collect::<BTreeSet<_>>();
    let requests = bridge
        .artifact_requests
        .iter()
        .map(|request| (artifact_key(&request.role, &request.runtime_path), request))
        .collect::<BTreeMap<_, _>>();
    let receipts = bridge
        .artifact_receipts
        .iter()
        .map(|receipt| (artifact_key(&receipt.role, &receipt.runtime_path), receipt))
        .collect::<BTreeMap<_, _>>();
    if requests.len() != expected.len() || receipts.len() != expected.len() {
        return Err(invalid(
            "converted_artifact_closure",
            "duplicate converted artifact role/path identity",
        ));
    }
    for (key, provenance) in &expected {
        let request = requests.get(key).ok_or_else(|| {
            invalid(
                "missing_converted_request",
                format!("missing converted artifact request for {}", key.1),
            )
        })?;
        let receipt = receipts.get(key).ok_or_else(|| {
            invalid(
                "missing_converted_receipt",
                format!("missing converted artifact receipt for {}", key.1),
            )
        })?;
        if !source_games.contains(&request.source_game.to_ascii_lowercase())
            || !is_blake3(&request.source_evidence_blake3)
            || !is_blake3(&request.conversion_request_blake3)
            || receipt.provenance != *provenance
            || receipt.byte_len == 0
            || !is_blake3(&receipt.blake3)
        {
            return Err(invalid(
                "converted_artifact_receipt",
                format!("invalid converted request/output evidence for {}", key.1),
            ));
        }
    }

    input
        .creature_closure
        .validate()
        .map_err(|error| SourceRigBridgeError::CreatureClosure(error.to_string()))?;
    if !input
        .creature_closure
        .target_game
        .eq_ignore_ascii_case("fo4")
        || !source_games.contains(&input.creature_closure.source_game.to_ascii_lowercase())
        || !is_blake3(&input.creature_closure_request_blake3)
        || bridge.creature_closure_request_blake3 != input.creature_closure_request_blake3
        || input.creature_closure_request_blake3 != input.creature_closure.request_blake3
        || !is_blake3(&input.creature_closure.receipt_hash)
        || bridge.creature_closure_receipt_blake3 != input.creature_closure.receipt_hash
    {
        return Err(invalid(
            "creature_closure_identity",
            "creature NIF closure game or BLAKE3 identity differs",
        ));
    }
    let closure_target_path = &input.selected_family.visual_creature_closure_target_path;
    let closure_runtime_path = closure_target_path.replace('/', "\\");
    let closure_runtime_path = closure_runtime_path
        .strip_prefix("meshes\\")
        .unwrap_or(&closure_runtime_path);
    if closure_runtime_path != input.rig.visual_skeleton_nif {
        return Err(invalid(
            "creature_closure_path",
            "explicit creature-closure target path differs from visual runtime path",
        ));
    }
    let closure_visual = input
        .creature_closure
        .artifacts
        .iter()
        .find(|artifact| {
            artifact.kind == CreatureArtifactKind::Nif
                && artifact.target_data_relative_path == *closure_target_path
        })
        .ok_or_else(|| {
            invalid(
                "creature_closure_path",
                "validated NIF closure does not contain the exact visual runtime path",
            )
        })?;
    let mut visual_dispositions = input.creature_closure.inputs.iter().filter(|disposition| {
        disposition.role == CreatureNifRole::Skeleton
            && disposition.target_data_relative_path == *closure_target_path
    });
    let visual_disposition = visual_dispositions.next().ok_or_else(|| {
        invalid(
            "creature_closure_source",
            "selected visual target has no source skeleton disposition",
        )
    })?;
    if visual_dispositions.next().is_some()
        || visual_disposition.source_byte_len == 0
        || !is_blake3(&visual_disposition.source_blake3)
        || visual_disposition.output_fingerprint != closure_visual.fingerprint
        || !closure_visual
            .source_inputs
            .contains(&visual_disposition.input_key)
        || closure_visual.byte_len == 0
        || !is_blake3(&closure_visual.fingerprint)
    {
        return Err(invalid(
            "creature_closure_source",
            "selected visual target is not bound to one source-hashed skeleton disposition",
        ));
    }
    Ok(())
}

fn expected_artifacts(
    rig: &CreatureManifest,
    graph: &CapabilityGraphManifest,
) -> Result<
    BTreeMap<(SourceRigArtifactRole, String), SourceRigArtifactProvenance>,
    SourceRigBridgeError,
> {
    let mut expected = BTreeMap::new();
    insert_expected(
        &mut expected,
        SourceRigArtifactRole::AnimationSkeleton,
        &rig.animation_skeleton.path,
        SourceRigArtifactProvenance::ReconstructedSourceSkeleton,
    )?;
    let clip_names = graph
        .roles
        .iter()
        .map(|role| role.clip_name.as_str())
        .chain(
            graph
                .overlays
                .iter()
                .map(|overlay| overlay.clip_name.as_str()),
        )
        .collect::<BTreeSet<_>>();
    for clip_name in clip_names {
        let clip = rig
            .clips
            .iter()
            .find(|clip| clip.name == clip_name)
            .ok_or_else(|| invalid("family_clips", format!("missing clip {clip_name:?}")))?;
        insert_expected(
            &mut expected,
            SourceRigArtifactRole::AnimationClip {
                clip_name: clip.name.clone(),
            },
            &clip.path,
            SourceRigArtifactProvenance::ConvertedSourceClip,
        )?;
    }
    if let RagdollDisposition::SourceOwned { runtime_path, .. } = &rig.ragdoll {
        insert_expected(
            &mut expected,
            SourceRigArtifactRole::Ragdoll,
            runtime_path,
            SourceRigArtifactProvenance::ReconstructedSourceRagdoll,
        )?;
    }
    Ok(expected)
}

fn insert_expected(
    expected: &mut BTreeMap<(SourceRigArtifactRole, String), SourceRigArtifactProvenance>,
    role: SourceRigArtifactRole,
    path: &str,
    provenance: SourceRigArtifactProvenance,
) -> Result<(), SourceRigBridgeError> {
    if path.trim().is_empty() {
        return Err(invalid(
            "converted_artifact_path",
            "empty target runtime path",
        ));
    }
    let key = artifact_key(&role, path);
    if expected.insert(key, provenance).is_some() {
        return Err(invalid(
            "converted_artifact_path",
            format!("duplicate target runtime role/path {path:?}"),
        ));
    }
    Ok(())
}

fn artifact_key(role: &SourceRigArtifactRole, path: &str) -> (SourceRigArtifactRole, String) {
    (role.clone(), path.to_string())
}

fn validate_articulated_collision(
    input: &SourceRigRecipeBridgeInput,
) -> Result<(), SourceRigBridgeError> {
    let deferred = input.creature_closure.inputs.iter().any(|entry| {
        matches!(
            entry.collision,
            CreatureCollisionDisposition::ArticulatedDeferredForHkx { .. }
        )
    });
    if deferred && !input.rig.ragdoll.resolves_articulated_collision() {
        return Err(invalid(
            "articulated_collision_ragdoll",
            "articulated collision defer requires a source-owned or explicitly unsupported source ragdoll disposition",
        ));
    }
    Ok(())
}

fn is_blake3(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn invalid(code: &'static str, message: impl Into<String>) -> SourceRigBridgeError {
    SourceRigBridgeError::Invalid {
        code,
        message: message.into(),
    }
}

impl From<SourceRigRecipeError> for SourceRigBridgeError {
    fn from(error: SourceRigRecipeError) -> Self {
        Self::Recipe(error.to_string())
    }
}
