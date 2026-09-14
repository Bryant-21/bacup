//! Source-rig precommit preparation. This module stages but never publishes.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use havok_native::convert::creature_ragdoll::validate_fo4_creature_ragdoll_file;
use havok_native::hkx::HkxFile;
use havok_native::hkx::model::HkxObject;
use havok_native::hkx::types::HkxValue;
use nif_core_native::creature_closure::{
    CreatureArtifactKind, CreatureClosureReceipt, CreatureCollisionDisposition,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::{
    CreaturePrimaryRecordMapping, CreatureRecordFamilyBatch, CreatureTargetRecordReference,
    ProjectedRecordIdentity, RagdollDisposition, SourceRigExecutableRecipe, SourceRigPackReport,
    SourceRigRuntimeModelArtifactReceipt, pack_capability_scaffold,
};
use crate::sym::StringInterner;

pub const PREPARED_SOURCE_RIG_EXECUTION_VERSION: u32 = 2;

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(tag = "role", rename_all = "snake_case")]
pub enum SourceRigArtifactRole {
    AnimationSkeleton,
    AnimationClip { clip_name: String },
    Ragdoll,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceRigArtifactProvenance {
    ReconstructedSourceSkeleton,
    ConvertedSourceClip,
    ReconstructedSourceRagdoll,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct SourceRigArtifactReceipt {
    pub role: SourceRigArtifactRole,
    pub provenance: SourceRigArtifactProvenance,
    pub runtime_path: String,
    pub byte_len: u64,
    pub blake3: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceRigArtifactInput {
    pub path: PathBuf,
    pub receipt: SourceRigArtifactReceipt,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceRigRuntimeModelArtifactInput {
    pub path: PathBuf,
    pub receipt: SourceRigRuntimeModelArtifactReceipt,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceRigCreatureClosureInput {
    pub receipt_json: String,
    pub staged_data_root: PathBuf,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct PreparedScaffoldArtifactReceipt {
    pub runtime_path: String,
    pub byte_len: u64,
    pub blake3: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct PreparedRecordFamilyIntentReceipt {
    pub family_id: String,
    pub primary_mappings: Vec<CreaturePrimaryRecordMapping>,
    pub required_target_records: Vec<CreatureTargetRecordReference>,
    pub projected_records: Vec<ProjectedRecordIdentity>,
    pub record_count: usize,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct PreparedSourceRigExecutionReceipt {
    pub version: u32,
    pub recipe_blake3: String,
    pub creature_closure_blake3: String,
    pub source_artifacts: Vec<SourceRigArtifactReceipt>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_model_closure_blake3: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub runtime_model_artifacts: Vec<SourceRigRuntimeModelArtifactReceipt>,
    pub scaffold_artifacts: Vec<PreparedScaffoldArtifactReceipt>,
    pub record_family: PreparedRecordFamilyIntentReceipt,
}

impl PreparedSourceRigExecutionReceipt {
    pub fn canonical_json(&self) -> Result<String, SourceRigExecutionError> {
        let mut receipt = self.clone();
        receipt.normalize();
        serde_json::to_string(&receipt)
            .map_err(|error| SourceRigExecutionError::Serialization(error.to_string()))
    }

    pub fn stable_hash_blake3(&self) -> Result<String, SourceRigExecutionError> {
        Ok(blake3::hash(self.canonical_json()?.as_bytes())
            .to_hex()
            .to_string())
    }

    fn normalize(&mut self) {
        self.source_artifacts.sort_by_key(|artifact| {
            (
                artifact.runtime_path.to_ascii_lowercase(),
                artifact.role.clone(),
            )
        });
        self.runtime_model_artifacts
            .sort_by_key(|artifact| artifact.target_data_path.to_ascii_lowercase());
        self.scaffold_artifacts
            .sort_by_key(|artifact| artifact.runtime_path.to_ascii_lowercase());
        self.record_family.primary_mappings.sort_by_key(|mapping| {
            (
                mapping.source.stable_key(),
                mapping.target.plugin.to_ascii_lowercase(),
                mapping.target.local,
            )
        });
        self.record_family
            .required_target_records
            .sort_by_key(|record| {
                (
                    record.signature.clone(),
                    record.form_key.plugin.to_ascii_lowercase(),
                    record.form_key.local,
                )
            });
        self.record_family.projected_records.sort_by_key(|record| {
            (
                record.signature.clone(),
                record.target_form_key.plugin.to_ascii_lowercase(),
                record.target_form_key.local,
            )
        });
    }
}

#[derive(Clone, Debug)]
pub struct PreparedSourceRigExecution {
    pub staging_root: PathBuf,
    pub receipt: PreparedSourceRigExecutionReceipt,
    pub record_batch: CreatureRecordFamilyBatch,
}

#[derive(Debug, Error)]
pub enum SourceRigExecutionError {
    #[error("private source-rig staging root already exists: {0}")]
    StagingRootExists(PathBuf),
    #[error("failed to {operation} {path}: {source}")]
    Io {
        operation: &'static str,
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("source-rig recipe failed validation: {0}")]
    Recipe(String),
    #[error("source-rig execution requires an evidence bridge receipt")]
    MissingBridgeReceipt,
    #[error("unsupported source-rig execution contract ({feature}): {message}")]
    Unsupported {
        feature: &'static str,
        message: String,
    },
    #[error("source-rig artifact contract failed ({code}): {message}")]
    Artifact { code: &'static str, message: String },
    #[error("source-rig scaffold preparation failed: {0}")]
    Pack(String),
    #[error("prepared source-rig receipt serialization failed: {0}")]
    Serialization(String),
}

struct ValidatedArtifact {
    receipt: SourceRigArtifactReceipt,
    bytes: Vec<u8>,
}

struct ValidatedRuntimeModelArtifact {
    receipt: SourceRigRuntimeModelArtifactReceipt,
    bytes: Vec<u8>,
}

fn validate_creature_closure(
    recipe: &SourceRigExecutableRecipe,
    input: &SourceRigCreatureClosureInput,
) -> Result<String, SourceRigExecutionError> {
    let receipt =
        CreatureClosureReceipt::parse_and_validate(&input.receipt_json).map_err(|error| {
            artifact_error(
                "invalid_creature_closure_receipt",
                format!("creature NIF closure receipt failed validation: {error}"),
            )
        })?;
    let mut required_nifs = BTreeSet::from([recipe.rig.visual_skeleton_nif.to_ascii_lowercase()]);
    required_nifs.extend(
        recipe
            .projection
            .effective_body_nif_parts()
            .into_iter()
            .map(|part| part.body_nif.to_ascii_lowercase()),
    );
    let mut received_nifs = BTreeSet::new();
    for artifact in &receipt.artifacts {
        let relative = artifact.target_data_relative_path.replace('/', "\\");
        let staged = relative
            .split('\\')
            .fold(input.staged_data_root.clone(), |path, part| path.join(part));
        let bytes = read(&staged, "read creature NIF closure artifact")?;
        if artifact.byte_len != bytes.len() as u64
            || !artifact
                .fingerprint
                .eq_ignore_ascii_case(&blake3::hash(&bytes).to_hex().to_string())
        {
            return Err(artifact_error(
                "creature_closure_content_mismatch",
                format!(
                    "creature closure artifact changed after receipt: {:?}",
                    artifact.target_data_relative_path
                ),
            ));
        }
        if artifact.kind == CreatureArtifactKind::Nif {
            let runtime_path = relative
                .strip_prefix("meshes\\")
                .unwrap_or(&relative)
                .to_ascii_lowercase();
            received_nifs.insert(runtime_path);
        }
    }
    if !required_nifs.is_subset(&received_nifs) {
        return Err(artifact_error(
            "creature_closure_nif_mismatch",
            format!(
                "creature closure does not contain every recipe NIF; required {required_nifs:?}, got {received_nifs:?}"
            ),
        ));
    }
    let articulated = receipt.inputs.iter().any(|input| {
        matches!(
            input.collision,
            CreatureCollisionDisposition::ArticulatedDeferredForHkx { .. }
        )
    });
    if articulated && !recipe.rig.ragdoll.resolves_articulated_collision() {
        return Err(artifact_error(
            "articulated_collision_requires_ragdoll",
            "NIF closure deferred articulated collision but recipe has neither a source-owned nor explicitly unsupported source ragdoll disposition",
        ));
    }
    Ok(receipt.receipt_hash)
}

/// Reload, validate, and stage a complete source-rig family without publishing it.
pub fn prepare_source_rig_execution(
    recipe_path: impl AsRef<Path>,
    artifacts: &[SourceRigArtifactInput],
    creature_closure: &SourceRigCreatureClosureInput,
    private_staging_root: impl AsRef<Path>,
    interner: &StringInterner,
) -> Result<PreparedSourceRigExecution, SourceRigExecutionError> {
    prepare_source_rig_execution_with_runtime_models(
        recipe_path,
        artifacts,
        &[],
        creature_closure,
        private_staging_root,
        interner,
    )
}

pub fn prepare_source_rig_execution_with_runtime_models(
    recipe_path: impl AsRef<Path>,
    artifacts: &[SourceRigArtifactInput],
    runtime_model_artifacts: &[SourceRigRuntimeModelArtifactInput],
    creature_closure: &SourceRigCreatureClosureInput,
    private_staging_root: impl AsRef<Path>,
    interner: &StringInterner,
) -> Result<PreparedSourceRigExecution, SourceRigExecutionError> {
    let recipe_path = recipe_path.as_ref();
    let private_staging_root = private_staging_root.as_ref();
    if private_staging_root.exists() {
        return Err(SourceRigExecutionError::StagingRootExists(
            private_staging_root.to_path_buf(),
        ));
    }

    let recipe_json = read_to_string(recipe_path, "read recipe")?;
    let recipe = SourceRigExecutableRecipe::from_json(&recipe_json)
        .map_err(|error| SourceRigExecutionError::Recipe(error.to_string()))?;
    let bridge = recipe
        .bridge_receipt
        .as_ref()
        .ok_or(SourceRigExecutionError::MissingBridgeReceipt)?;
    let recipe_blake3 = recipe
        .stable_hash_blake3()
        .map_err(|error| SourceRigExecutionError::Recipe(error.to_string()))?;
    let creature_closure_blake3 = validate_creature_closure(&recipe, creature_closure)?;
    let validated = validate_artifacts(&recipe, artifacts)?;
    let validated_runtime_models =
        validate_runtime_model_artifacts(&recipe, runtime_model_artifacts)?;
    validate_runtime_model_path_collisions(&validated, &validated_runtime_models)?;
    if !bridge
        .creature_closure_receipt_blake3
        .eq_ignore_ascii_case(&creature_closure_blake3)
    {
        return Err(artifact_error(
            "bridge_creature_closure_mismatch",
            "prepared creature closure differs from the recipe bridge receipt",
        ));
    }
    let mut expected = bridge.artifact_receipts.clone();
    let mut actual = validated
        .iter()
        .map(|artifact| artifact.receipt.clone())
        .collect::<Vec<_>>();
    expected.sort_by_key(|artifact| artifact.runtime_path.to_ascii_lowercase());
    actual.sort_by_key(|artifact| artifact.runtime_path.to_ascii_lowercase());
    if expected != actual {
        return Err(artifact_error(
            "bridge_artifact_receipt_mismatch",
            "provided converted artifacts differ from the recipe bridge receipt",
        ));
    }
    let runtime_model_closure_blake3 = recipe
        .runtime_model_closure
        .as_ref()
        .map(|receipt| receipt.stable_hash_blake3())
        .transpose()
        .map_err(|error| SourceRigExecutionError::Recipe(error.to_string()))?;
    let record_batch = recipe
        .rebuild_record_family_batch(interner)
        .map_err(|error| SourceRigExecutionError::Recipe(error.to_string()))?;
    let record_family = prepared_record_family_receipt(&record_batch);

    let staged = (|| {
        let pack = pack_capability_scaffold(&recipe.rig, &recipe.graph, private_staging_root)
            .map_err(|error| SourceRigExecutionError::Pack(error.to_string()))?;
        stage_creature_closure_artifacts(creature_closure, private_staging_root)?;
        for artifact in &validated {
            let destination =
                join_runtime_path(private_staging_root, &artifact.receipt.runtime_path);
            if let Some(parent) = destination.parent() {
                create_dir_all(parent)?;
            }
            write(&destination, &artifact.bytes)?;
        }
        for artifact in &validated_runtime_models {
            let destination =
                join_data_path(private_staging_root, &artifact.receipt.target_data_path)?;
            if let Some(parent) = destination.parent() {
                create_dir_all(parent)?;
            }
            write(&destination, &artifact.bytes)?;
        }
        let mut receipt = PreparedSourceRigExecutionReceipt {
            version: PREPARED_SOURCE_RIG_EXECUTION_VERSION,
            recipe_blake3,
            creature_closure_blake3,
            source_artifacts: validated
                .iter()
                .map(|artifact| artifact.receipt.clone())
                .collect(),
            runtime_model_closure_blake3,
            runtime_model_artifacts: validated_runtime_models
                .iter()
                .map(|artifact| artifact.receipt.clone())
                .collect(),
            scaffold_artifacts: packed_scaffold_receipts(&pack)?,
            record_family,
        };
        receipt.normalize();
        receipt.canonical_json()?;
        Ok(PreparedSourceRigExecution {
            staging_root: private_staging_root.to_path_buf(),
            receipt,
            record_batch,
        })
    })();

    if staged.is_err() && private_staging_root.exists() {
        let _ = fs::remove_dir_all(private_staging_root);
    }
    staged
}

fn stage_creature_closure_artifacts(
    input: &SourceRigCreatureClosureInput,
    staging_root: &Path,
) -> Result<(), SourceRigExecutionError> {
    let receipt = CreatureClosureReceipt::parse_and_validate(&input.receipt_json)
        .map_err(|error| artifact_error("invalid_creature_closure_receipt", error.to_string()))?;
    for artifact in receipt.artifacts {
        let relative = artifact.target_data_relative_path.replace('/', "\\");
        let source = relative
            .split('\\')
            .fold(input.staged_data_root.clone(), |path, part| path.join(part));
        let runtime = relative.strip_prefix("meshes\\").unwrap_or(&relative);
        let destination = join_runtime_path(staging_root, runtime);
        if let Some(parent) = destination.parent() {
            create_dir_all(parent)?;
        }
        write(
            &destination,
            &read(&source, "read creature closure artifact")?,
        )?;
    }
    Ok(())
}

fn validate_artifacts(
    recipe: &SourceRigExecutableRecipe,
    artifacts: &[SourceRigArtifactInput],
) -> Result<Vec<ValidatedArtifact>, SourceRigExecutionError> {
    let expected = expected_artifacts(recipe)?;
    let mut provided = BTreeSet::new();
    let mut validated = Vec::with_capacity(artifacts.len());
    for artifact in artifacts {
        let key = artifact.receipt.runtime_path.to_ascii_lowercase();
        if !provided.insert(key.clone()) {
            return Err(artifact_error(
                "duplicate_artifact",
                format!(
                    "duplicate artifact receipt for {:?}",
                    artifact.receipt.runtime_path
                ),
            ));
        }
        let Some((runtime_path, role, provenance)) = expected.get(&key) else {
            return Err(artifact_error(
                "unexpected_artifact",
                format!(
                    "recipe does not require {:?}",
                    artifact.receipt.runtime_path
                ),
            ));
        };
        if artifact.receipt.runtime_path != *runtime_path
            || artifact.receipt.role != *role
            || artifact.receipt.provenance != *provenance
        {
            return Err(artifact_error(
                "artifact_receipt_mismatch",
                format!("receipt does not match required artifact {runtime_path:?}"),
            ));
        }
        if artifact.receipt.byte_len == 0 || !is_blake3(&artifact.receipt.blake3) {
            return Err(artifact_error(
                "invalid_artifact_receipt",
                format!("invalid size or BLAKE3 for {runtime_path:?}"),
            ));
        }
        let bytes = read(&artifact.path, "read converted artifact")?;
        let actual_hash = blake3::hash(&bytes).to_hex().to_string();
        if artifact.receipt.byte_len != bytes.len() as u64
            || !artifact.receipt.blake3.eq_ignore_ascii_case(&actual_hash)
        {
            return Err(artifact_error(
                "artifact_content_mismatch",
                format!("converted artifact changed after receipt: {runtime_path:?}"),
            ));
        }
        validate_artifact_payload(&artifact.receipt.role, &bytes, recipe)?;
        if matches!(artifact.receipt.role, SourceRigArtifactRole::Ragdoll) {
            let RagdollDisposition::SourceOwned { receipt, .. } = &recipe.rig.ragdoll else {
                return Err(artifact_error(
                    "unexpected_ragdoll",
                    "no-ragdoll recipe cannot accept a ragdoll artifact",
                ));
            };
            if receipt.byte_len != artifact.receipt.byte_len
                || !receipt
                    .blake3
                    .eq_ignore_ascii_case(&artifact.receipt.blake3)
            {
                return Err(artifact_error(
                    "ragdoll_receipt_mismatch",
                    "provided reconstructed ragdoll does not match the recipe receipt",
                ));
            }
        }
        validated.push(ValidatedArtifact {
            receipt: artifact.receipt.clone(),
            bytes,
        });
    }
    let missing = expected
        .keys()
        .filter(|path| !provided.contains(*path))
        .map(|path| expected[path].0.clone())
        .collect::<Vec<_>>();
    if !missing.is_empty() {
        return Err(artifact_error(
            "missing_artifact",
            format!(
                "missing required converted artifacts: {}",
                missing.join(", ")
            ),
        ));
    }
    validated.sort_by_key(|artifact| artifact.receipt.runtime_path.to_ascii_lowercase());
    Ok(validated)
}

fn validate_runtime_model_artifacts(
    recipe: &SourceRigExecutableRecipe,
    artifacts: &[SourceRigRuntimeModelArtifactInput],
) -> Result<Vec<ValidatedRuntimeModelArtifact>, SourceRigExecutionError> {
    let expected = recipe
        .runtime_model_closure
        .as_ref()
        .map(|receipt| receipt.canonical_target_artifacts())
        .transpose()
        .map_err(|error| SourceRigExecutionError::Recipe(error.to_string()))?
        .unwrap_or_default()
        .into_iter()
        .map(|artifact| (artifact.target_data_path.to_ascii_lowercase(), artifact))
        .collect::<BTreeMap<_, _>>();
    let mut provided = BTreeSet::new();
    let mut validated = Vec::with_capacity(artifacts.len());
    for artifact in artifacts {
        let key = artifact.receipt.target_data_path.to_ascii_lowercase();
        if !provided.insert(key.clone()) {
            return Err(artifact_error(
                "duplicate_runtime_model_artifact",
                format!(
                    "duplicate runtime-model artifact {:?}",
                    artifact.receipt.target_data_path
                ),
            ));
        }
        let Some(expected_receipt) = expected.get(&key) else {
            return Err(artifact_error(
                "unexpected_runtime_model_artifact",
                format!(
                    "recipe does not require runtime-model artifact {:?}",
                    artifact.receipt.target_data_path
                ),
            ));
        };
        if &artifact.receipt != expected_receipt {
            return Err(artifact_error(
                "runtime_model_artifact_receipt_mismatch",
                format!(
                    "runtime-model artifact receipt differs for {:?}",
                    artifact.receipt.target_data_path
                ),
            ));
        }
        let bytes = read(&artifact.path, "read runtime-model artifact")?;
        let actual_hash = blake3::hash(&bytes).to_hex().to_string();
        if artifact.receipt.byte_len != bytes.len() as u64
            || !artifact.receipt.blake3.eq_ignore_ascii_case(&actual_hash)
        {
            return Err(artifact_error(
                "runtime_model_artifact_content_mismatch",
                format!(
                    "runtime-model artifact changed after receipt: {:?}",
                    artifact.receipt.target_data_path
                ),
            ));
        }
        validated.push(ValidatedRuntimeModelArtifact {
            receipt: artifact.receipt.clone(),
            bytes,
        });
    }
    let missing = expected
        .keys()
        .filter(|path| !provided.contains(*path))
        .map(|path| expected[path].target_data_path.clone())
        .collect::<Vec<_>>();
    if !missing.is_empty() {
        return Err(artifact_error(
            "missing_runtime_model_artifact",
            format!(
                "missing required runtime-model artifacts: {}",
                missing.join(", ")
            ),
        ));
    }
    validated.sort_by_key(|artifact| artifact.receipt.target_data_path.to_ascii_lowercase());
    Ok(validated)
}

fn validate_runtime_model_path_collisions(
    source_rig: &[ValidatedArtifact],
    runtime_models: &[ValidatedRuntimeModelArtifact],
) -> Result<(), SourceRigExecutionError> {
    let source_paths = source_rig
        .iter()
        .map(|artifact| {
            (
                format!("meshes\\{}", artifact.receipt.runtime_path).to_ascii_lowercase(),
                (artifact.receipt.byte_len, artifact.receipt.blake3.as_str()),
            )
        })
        .collect::<BTreeMap<_, _>>();
    for artifact in runtime_models {
        let key = artifact.receipt.target_data_path.to_ascii_lowercase();
        if let Some((byte_len, hash)) = source_paths.get(&key)
            && (*byte_len != artifact.receipt.byte_len
                || !hash.eq_ignore_ascii_case(&artifact.receipt.blake3))
        {
            return Err(artifact_error(
                "runtime_model_path_collision",
                format!(
                    "runtime-model target {:?} conflicts with a source-rig artifact",
                    artifact.receipt.target_data_path
                ),
            ));
        }
    }
    Ok(())
}

type ExpectedArtifact = (String, SourceRigArtifactRole, SourceRigArtifactProvenance);

fn expected_artifacts(
    recipe: &SourceRigExecutableRecipe,
) -> Result<BTreeMap<String, ExpectedArtifact>, SourceRigExecutionError> {
    let mut expected = BTreeMap::new();
    insert_expected(
        &mut expected,
        recipe.rig.animation_skeleton.path.clone(),
        SourceRigArtifactRole::AnimationSkeleton,
        SourceRigArtifactProvenance::ReconstructedSourceSkeleton,
    )?;
    let clip_names = recipe
        .graph
        .roles
        .iter()
        .map(|role| role.clip_name.as_str())
        .chain(
            recipe
                .graph
                .overlays
                .iter()
                .map(|overlay| overlay.clip_name.as_str()),
        )
        .collect::<BTreeSet<_>>();
    for clip_name in clip_names {
        let clip = recipe
            .rig
            .clips
            .iter()
            .find(|clip| clip.name == clip_name)
            .expect("recipe graph validation guarantees clip declarations");
        insert_expected(
            &mut expected,
            clip.path.clone(),
            SourceRigArtifactRole::AnimationClip {
                clip_name: clip.name.clone(),
            },
            SourceRigArtifactProvenance::ConvertedSourceClip,
        )?;
    }
    if let RagdollDisposition::SourceOwned { runtime_path, .. } = &recipe.rig.ragdoll {
        insert_expected(
            &mut expected,
            runtime_path.clone(),
            SourceRigArtifactRole::Ragdoll,
            SourceRigArtifactProvenance::ReconstructedSourceRagdoll,
        )?;
    }
    Ok(expected)
}

fn insert_expected(
    expected: &mut BTreeMap<String, ExpectedArtifact>,
    runtime_path: String,
    role: SourceRigArtifactRole,
    provenance: SourceRigArtifactProvenance,
) -> Result<(), SourceRigExecutionError> {
    let key = runtime_path.to_ascii_lowercase();
    if expected
        .insert(key, (runtime_path.clone(), role, provenance))
        .is_some()
    {
        return Err(artifact_error(
            "duplicate_runtime_path",
            format!("recipe assigns multiple artifact roles to {runtime_path:?}"),
        ));
    }
    Ok(())
}

fn validate_artifact_payload(
    role: &SourceRigArtifactRole,
    bytes: &[u8],
    recipe: &SourceRigExecutableRecipe,
) -> Result<(), SourceRigExecutionError> {
    let hkx = HkxFile::read(bytes).map_err(|error| {
        artifact_error(
            "invalid_hkx",
            format!("{role:?} artifact is not readable HKX: {error}"),
        )
    })?;
    if hkx.class_version() != 11
        || hkx.contents_version() != "hk_2014.1.0-r1"
        || hkx.packfile().header.pointer_size != 8
    {
        return Err(artifact_error(
            "invalid_hkx_header",
            format!("{role:?} artifact is not an FO4 AMD64 HKX"),
        ));
    }
    match role {
        SourceRigArtifactRole::AnimationSkeleton => validate_animation_skeleton(&hkx, recipe)?,
        SourceRigArtifactRole::AnimationClip { clip_name } => {
            validate_animation_clip(&hkx, recipe, clip_name)?
        }
        SourceRigArtifactRole::Ragdoll => validate_ragdoll_closure(&hkx, recipe)?,
    }
    Ok(())
}

fn validate_animation_skeleton(
    hkx: &HkxFile,
    recipe: &SourceRigExecutableRecipe,
) -> Result<(), SourceRigExecutionError> {
    let (container_index, container) = unique_class(hkx, "hkaAnimationContainer", "skeleton")?;
    let (skeleton_index, skeleton) = unique_class(hkx, "hkaSkeleton", "skeleton")?;
    validate_exact_pointer_array(
        hkx,
        container,
        "skeletons",
        &[skeleton_index],
        "skeleton_semantics",
    )?;
    for member_name in ["animations", "bindings", "attachments", "skins"] {
        validate_empty_array(container, member_name, "skeleton_semantics")?;
    }
    let expected = &recipe.rig.animation_skeleton;
    if string_member(skeleton, "name", "skeleton_semantics")? != expected.runtime_name {
        return Err(artifact_error(
            "skeleton_semantics",
            "hkaSkeleton.name differs from the source-declared runtime name",
        ));
    }
    let expected_bones = expected
        .bones
        .iter()
        .map(|bone| {
            (
                bone.name.as_str(),
                bone.parent_index.map(|parent| parent as i32),
            )
        })
        .collect::<Vec<_>>();
    if skeleton_bones(skeleton).as_deref() != Some(expected_bones.as_slice()) {
        return Err(artifact_error(
            "skeleton_semantics",
            "hkaSkeleton bone names, order, count, or parent indices differ from the recipe",
        ));
    }
    if string_array_member(skeleton, "floatSlots", "skeleton_semantics")? != expected.float_slots {
        return Err(artifact_error(
            "skeleton_semantics",
            "hkaSkeleton.floatSlots differs from the ordered recipe declaration",
        ));
    }
    validate_qs_transform_array(
        skeleton,
        "referencePose",
        expected.bones.len(),
        "skeleton.reference_pose",
    )?;
    validate_float_array(
        skeleton,
        "referenceFloats",
        expected.float_slots.len(),
        "skeleton.reference_floats",
    )?;
    let _ = container_index;
    Ok(())
}

fn validate_animation_clip(
    hkx: &HkxFile,
    recipe: &SourceRigExecutableRecipe,
    clip_name: &str,
) -> Result<(), SourceRigExecutionError> {
    let (_, container) = unique_class(hkx, "hkaAnimationContainer", "clip")?;
    let (binding_index, binding) = unique_class(hkx, "hkaAnimationBinding", "clip")?;
    for member_name in ["skeletons", "attachments", "skins"] {
        validate_empty_array(container, member_name, "clip_semantics")?;
    }
    let animation_indices = pointer_array(container, "animations", "clip_semantics")?;
    let binding_indices = pointer_array(container, "bindings", "clip_semantics")?;
    if animation_indices.len() != 1 || binding_indices != [binding_index] {
        return Err(artifact_error(
            "clip_semantics",
            "animation container must reference exactly one animation and its only binding",
        ));
    }
    let animation_index = animation_indices[0];
    let animation = hkx.objects().get(animation_index).ok_or_else(|| {
        artifact_error(
            "clip_semantics",
            "animation container points outside the HKX object table",
        )
    })?;
    if !is_supported_animation_class(&animation.class_name) {
        return Err(unsupported(
            "clip.animation_class",
            format!(
                "cannot semantically validate animation class {:?}",
                animation.class_name
            ),
        ));
    }
    let animation_objects = hkx
        .objects()
        .iter()
        .enumerate()
        .filter(|(_, object)| is_animation_object_class(&object.class_name))
        .map(|(index, object)| (index, object))
        .collect::<Vec<_>>();
    if animation_objects
        .iter()
        .any(|(_, object)| !is_supported_animation_class(&object.class_name))
    {
        return Err(unsupported(
            "clip.animation_class",
            "clip contains an animation object whose semantic fields are not supported",
        ));
    }
    if animation_objects
        .iter()
        .map(|(index, _)| *index)
        .collect::<Vec<_>>()
        != [animation_index]
    {
        return Err(artifact_error(
            "clip_semantics",
            "clip contains extra or ambiguous animation objects",
        ));
    }
    if pointer_member(binding, "animation", "clip_semantics")? != Some(animation_index) {
        return Err(artifact_error(
            "clip_semantics",
            "hkaAnimationBinding.animation does not reference the sole container animation",
        ));
    }

    let clip = recipe
        .rig
        .clips
        .iter()
        .find(|clip| clip.name == clip_name)
        .expect("artifact closure is derived from validated recipe clips");
    if clip.binding.skeleton_path != recipe.rig.animation_skeleton.path {
        return Err(artifact_error(
            "clip_semantics",
            "clip target skeleton path differs from the recipe animation skeleton",
        ));
    }
    if string_member(binding, "originalSkeletonName", "clip_semantics")?
        != clip.binding.original_skeleton_name
    {
        return Err(artifact_error(
            "clip_semantics",
            "hkaAnimationBinding.originalSkeletonName differs from the recipe",
        ));
    }
    let actual_transform_tracks =
        usize_member(animation, "numberOfTransformTracks", "clip_semantics")?;
    let actual_bone_mapping = canonical_track_mapping(
        usize_array_member(binding, "transformTrackToBoneIndices", "clip_semantics")?,
        actual_transform_tracks,
    );
    if actual_transform_tracks != clip.binding.declared_transform_tracks
        || actual_bone_mapping != clip.binding.transform_track_to_bone_indices
    {
        let first_mapping_mismatch = actual_bone_mapping
            .iter()
            .zip(&clip.binding.transform_track_to_bone_indices)
            .position(|(actual, expected)| actual != expected)
            .or_else(|| {
                (actual_bone_mapping.len() != clip.binding.transform_track_to_bone_indices.len())
                    .then_some(
                        actual_bone_mapping
                            .len()
                            .min(clip.binding.transform_track_to_bone_indices.len()),
                    )
            });
        return Err(artifact_error(
            "clip_semantics",
            format!(
                "clip {clip_name:?} transform tracks actual={actual_transform_tracks} recipe={}; bone mapping lengths actual={} recipe={} first mismatch={first_mapping_mismatch:?}",
                clip.binding.declared_transform_tracks,
                actual_bone_mapping.len(),
                clip.binding.transform_track_to_bone_indices.len(),
            ),
        ));
    }
    let actual_float_tracks = usize_member(animation, "numberOfFloatTracks", "clip_semantics")?;
    let actual_float_mapping = canonical_track_mapping(
        usize_array_member(binding, "floatTrackToFloatSlotIndices", "clip_semantics")?,
        actual_float_tracks,
    );
    if actual_float_tracks != clip.binding.declared_float_tracks
        || actual_float_mapping != clip.binding.float_track_to_float_slot_indices
    {
        return Err(artifact_error(
            "clip_semantics",
            "animation float-track count or ordered float-slot mapping differs from the recipe",
        ));
    }

    let motion = recipe
        .graph
        .roles
        .iter()
        .filter(|role| role.clip_name == clip_name)
        .map(|role| role.motion)
        .chain(
            recipe
                .graph
                .overlays
                .iter()
                .filter(|overlay| overlay.clip_name == clip_name)
                .map(|overlay| overlay.motion),
        )
        .collect::<Vec<_>>();
    if motion.len() != 1 {
        return Err(artifact_error(
            "clip_semantics",
            "clip must have exactly one graph motion declaration",
        ));
    }
    validate_clip_root_motion(hkx, animation, motion[0])
}

fn canonical_track_mapping(mapping: Vec<usize>, track_count: usize) -> Vec<usize> {
    if mapping.is_empty() && track_count > 0 {
        (0..track_count).collect()
    } else {
        mapping
    }
}

fn validate_clip_root_motion(
    hkx: &HkxFile,
    animation: &HkxObject,
    motion: super::ClipMotionPolicy,
) -> Result<(), SourceRigExecutionError> {
    let frames = hkx
        .objects()
        .iter()
        .enumerate()
        .filter(|(_, object)| object.class_name == "hkaDefaultAnimatedReferenceFrame")
        .collect::<Vec<_>>();
    let extracted = pointer_member(animation, "extractedMotion", "clip.root_motion")?;
    if motion.extracted_planar_reference_frames == 0 {
        if extracted.is_some() || !frames.is_empty() {
            return Err(artifact_error(
                "clip_root_motion",
                "in-place clip contains an extracted-motion object",
            ));
        }
        return Ok(());
    }
    if frames.len() != 1 || extracted != Some(frames[0].0) {
        return Err(artifact_error(
            "clip_root_motion",
            format!(
                "animation-driven clip must reference exactly one planar reference-frame object: extracted={extracted:?} frame_indices={:?}",
                frames.iter().map(|(index, _)| *index).collect::<Vec<_>>()
            ),
        ));
    }
    let count = vector4_array_len(
        frames[0].1,
        "referenceFrameSamples",
        "clip.reference_frame_samples",
    )?;
    if count != motion.extracted_planar_reference_frames {
        return Err(artifact_error(
            "clip_root_motion",
            format!(
                "reference-frame sample count {count} differs from declared {}",
                motion.extracted_planar_reference_frames
            ),
        ));
    }
    Ok(())
}

fn unique_class<'a>(
    hkx: &'a HkxFile,
    class_name: &str,
    artifact_kind: &str,
) -> Result<(usize, &'a HkxObject), SourceRigExecutionError> {
    let matches = hkx
        .objects()
        .iter()
        .enumerate()
        .filter(|(_, object)| object.class_name == class_name)
        .collect::<Vec<_>>();
    if matches.len() != 1 {
        return Err(artifact_error(
            "artifact_semantics",
            format!(
                "{artifact_kind} artifact must contain exactly one {class_name}, found {}",
                matches.len()
            ),
        ));
    }
    Ok(matches[0])
}

fn is_supported_animation_class(class_name: &str) -> bool {
    matches!(
        class_name,
        "hkaInterleavedUncompressedAnimation"
            | "hkaSplineCompressedAnimation"
            | "hkaDeltaCompressedAnimation"
            | "hkaWaveletCompressedAnimation"
            | "hkaQuantizedAnimation"
    )
}

fn is_animation_object_class(class_name: &str) -> bool {
    class_name.starts_with("hka")
        && class_name.contains("Animation")
        && !matches!(class_name, "hkaAnimationContainer" | "hkaAnimationBinding")
}

fn validate_empty_array(
    object: &HkxObject,
    member_name: &str,
    feature: &'static str,
) -> Result<(), SourceRigExecutionError> {
    if semantic_array_member(object, member_name, feature)?.is_empty() {
        Ok(())
    } else {
        Err(artifact_error(
            feature,
            format!("{}.{} must be empty", object.class_name, member_name),
        ))
    }
}

fn validate_exact_pointer_array(
    hkx: &HkxFile,
    object: &HkxObject,
    member_name: &str,
    expected: &[usize],
    feature: &'static str,
) -> Result<(), SourceRigExecutionError> {
    let actual = pointer_array(object, member_name, feature)?;
    if actual != expected || actual.iter().any(|index| *index >= hkx.objects().len()) {
        return Err(artifact_error(
            feature,
            format!(
                "{}.{} differs from the exact expected object references",
                object.class_name, member_name
            ),
        ));
    }
    Ok(())
}

fn semantic_array_member<'a>(
    object: &'a HkxObject,
    member_name: &str,
    feature: &'static str,
) -> Result<&'a [HkxValue], SourceRigExecutionError> {
    match member(object, member_name) {
        Some(HkxValue::Array(values)) => Ok(values),
        Some(value) => Err(unsupported(
            feature,
            format!(
                "HKX reader exposed {}.{} as {}, not an array",
                object.class_name,
                member_name,
                value.variant_name()
            ),
        )),
        None => Err(unsupported(
            feature,
            format!(
                "HKX reader did not expose {}.{}",
                object.class_name, member_name
            ),
        )),
    }
}

fn pointer_array(
    object: &HkxObject,
    member_name: &str,
    feature: &'static str,
) -> Result<Vec<usize>, SourceRigExecutionError> {
    semantic_array_member(object, member_name, feature)?
        .iter()
        .map(|value| match value {
            HkxValue::Pointer(Some(index)) => Ok(*index),
            _ => Err(artifact_error(
                feature,
                format!(
                    "{}.{} contains a null or non-pointer entry",
                    object.class_name, member_name
                ),
            )),
        })
        .collect()
}

fn pointer_member(
    object: &HkxObject,
    member_name: &str,
    feature: &'static str,
) -> Result<Option<usize>, SourceRigExecutionError> {
    match member(object, member_name) {
        Some(HkxValue::Pointer(index)) => Ok(*index),
        Some(value) => Err(unsupported(
            feature,
            format!(
                "HKX reader exposed {}.{} as {}, not a pointer",
                object.class_name,
                member_name,
                value.variant_name()
            ),
        )),
        None => Err(unsupported(
            feature,
            format!(
                "HKX reader did not expose {}.{}",
                object.class_name, member_name
            ),
        )),
    }
}

fn string_member<'a>(
    object: &'a HkxObject,
    member_name: &str,
    feature: &'static str,
) -> Result<&'a str, SourceRigExecutionError> {
    match member(object, member_name) {
        Some(HkxValue::String {
            value,
            is_null: false,
        }) => Ok(value),
        Some(value) => Err(unsupported(
            feature,
            format!(
                "HKX reader exposed {}.{} as {}, not a non-null string",
                object.class_name,
                member_name,
                value.variant_name()
            ),
        )),
        None => Err(unsupported(
            feature,
            format!(
                "HKX reader did not expose {}.{}",
                object.class_name, member_name
            ),
        )),
    }
}

fn string_array_member(
    object: &HkxObject,
    member_name: &str,
    feature: &'static str,
) -> Result<Vec<String>, SourceRigExecutionError> {
    semantic_array_member(object, member_name, feature)?
        .iter()
        .map(|value| match value {
            HkxValue::String {
                value,
                is_null: false,
            } => Ok(value.clone()),
            _ => Err(unsupported(
                feature,
                format!(
                    "HKX reader did not expose every {}.{} entry as a string",
                    object.class_name, member_name
                ),
            )),
        })
        .collect()
}

fn usize_member(
    object: &HkxObject,
    member_name: &str,
    feature: &'static str,
) -> Result<usize, SourceRigExecutionError> {
    let value = member(object, member_name).ok_or_else(|| {
        unsupported(
            feature,
            format!(
                "HKX reader did not expose {}.{}",
                object.class_name, member_name
            ),
        )
    })?;
    integer_value(value)
        .and_then(|value| usize::try_from(value).ok())
        .ok_or_else(|| {
            unsupported(
                feature,
                format!(
                    "HKX reader did not expose {}.{} as a nonnegative integer",
                    object.class_name, member_name
                ),
            )
        })
}

fn usize_array_member(
    object: &HkxObject,
    member_name: &str,
    feature: &'static str,
) -> Result<Vec<usize>, SourceRigExecutionError> {
    semantic_array_member(object, member_name, feature)?
        .iter()
        .map(|value| {
            integer_value(value)
                .and_then(|value| usize::try_from(value).ok())
                .ok_or_else(|| {
                    unsupported(
                        feature,
                        format!(
                            "HKX reader did not expose every {}.{} entry as a nonnegative integer",
                            object.class_name, member_name
                        ),
                    )
                })
        })
        .collect()
}

fn validate_qs_transform_array(
    object: &HkxObject,
    member_name: &str,
    expected: usize,
    feature: &'static str,
) -> Result<(), SourceRigExecutionError> {
    let Some(value) = member(object, member_name) else {
        return Err(unsupported(
            feature,
            format!(
                "HKX reader did not expose {}.{}",
                object.class_name, member_name
            ),
        ));
    };
    let valid = match value {
        HkxValue::F32List(values) => {
            values.len() == expected * 12 && values.iter().all(|value| value.is_finite())
        }
        HkxValue::Array(values) => {
            values.len() == expected
                && values.iter().all(|value| {
                    matches!(value, HkxValue::F32List(parts) if parts.len() == 12 && parts.iter().all(|part| part.is_finite()))
                })
        }
        _ => {
            return Err(unsupported(
                feature,
                format!(
                    "HKX reader exposed {}.{} as an unsupported {} representation",
                    object.class_name,
                    member_name,
                    value.variant_name()
                ),
            ));
        }
    };
    if !valid {
        return Err(artifact_error(
            "skeleton_semantics",
            "hkaSkeleton.referencePose count or values are invalid",
        ));
    }
    Ok(())
}

fn validate_float_array(
    object: &HkxObject,
    member_name: &str,
    expected: usize,
    feature: &'static str,
) -> Result<(), SourceRigExecutionError> {
    let Some(value) = member(object, member_name) else {
        return Err(unsupported(
            feature,
            format!(
                "HKX reader did not expose {}.{}",
                object.class_name, member_name
            ),
        ));
    };
    let values = match value {
        HkxValue::F32List(values) => values.clone(),
        HkxValue::Array(values) => values
            .iter()
            .map(|value| match value {
                HkxValue::F32(value) => Ok(*value),
                HkxValue::Half(value) => Ok(*value),
                _ => Err(unsupported(
                    feature,
                    "HKX reader did not expose the float array as scalar floats",
                )),
            })
            .collect::<Result<Vec<_>, _>>()?,
        _ => {
            return Err(unsupported(
                feature,
                format!(
                    "HKX reader exposed {}.{} as an unsupported {} representation",
                    object.class_name,
                    member_name,
                    value.variant_name()
                ),
            ));
        }
    };
    if values.len() != expected || values.iter().any(|value| !value.is_finite()) {
        return Err(artifact_error(
            "skeleton_semantics",
            "hkaSkeleton.referenceFloats count or values are invalid",
        ));
    }
    Ok(())
}

fn vector4_array_len(
    object: &HkxObject,
    member_name: &str,
    feature: &'static str,
) -> Result<usize, SourceRigExecutionError> {
    let Some(value) = member(object, member_name) else {
        return Err(unsupported(
            feature,
            format!(
                "HKX reader did not expose {}.{}",
                object.class_name, member_name
            ),
        ));
    };
    match value {
        HkxValue::F32List(values)
            if values.len() % 4 == 0 && values.iter().all(|value| value.is_finite()) =>
        {
            Ok(values.len() / 4)
        }
        HkxValue::Array(values)
            if values.iter().all(|value| {
                matches!(value, HkxValue::F32List(parts) if parts.len() == 4 && parts.iter().all(|part| part.is_finite()))
            }) =>
        {
            Ok(values.len())
        }
        _ => Err(unsupported(
            feature,
            format!(
                "HKX reader exposed {}.{} in an unsupported vector4-array representation",
                object.class_name, member_name
            ),
        )),
    }
}

fn validate_ragdoll_closure(
    hkx: &HkxFile,
    recipe: &SourceRigExecutableRecipe,
) -> Result<(), SourceRigExecutionError> {
    let body_count = class_count(hkx, "hkpRigidBody");
    let constraint_instances = class_count(hkx, "hkpConstraintInstance");
    if body_count == 0 || constraint_instances % 2 != 0 {
        return Err(artifact_error(
            "ragdoll_closure",
            "ragdoll must contain rigid bodies and paired constraint instances",
        ));
    }
    validate_fo4_creature_ragdoll_file(hkx, body_count, constraint_instances / 2)
        .map_err(|error| artifact_error("ragdoll_closure", error.to_string()))?;

    let expected_bones = recipe
        .rig
        .animation_skeleton
        .bones
        .iter()
        .map(|bone| {
            (
                bone.name.as_str(),
                bone.parent_index.map(|parent| parent as i32),
            )
        })
        .collect::<Vec<_>>();
    let skeleton_matches = hkx
        .objects()
        .iter()
        .filter(|object| object.class_name == "hkaSkeleton")
        .any(|object| skeleton_bones(object).is_some_and(|bones| bones == expected_bones));
    if !skeleton_matches {
        return Err(artifact_error(
            "ragdoll_skeleton_mismatch",
            "ragdoll animation skeleton bone names, order, count, or parents differ from recipe",
        ));
    }

    let ragdoll = hkx
        .objects()
        .iter()
        .find(|object| object.class_name == "hkaRagdollInstance")
        .expect("FO4 ragdoll validation guarantees one instance");
    validate_pointer_array_classes(hkx, ragdoll, "rigidBodies", "hkpRigidBody", body_count)?;
    validate_pointer_array_classes(
        hkx,
        ragdoll,
        "constraints",
        "hkpConstraintInstance",
        constraint_instances / 2,
    )?;
    let map = array_member(ragdoll, "boneToRigidBodyMap")?;
    let mut mapped = BTreeSet::new();
    for value in map {
        let index = integer_value(value).ok_or_else(|| {
            artifact_error(
                "ragdoll_closure",
                "boneToRigidBodyMap contains a non-integer",
            )
        })?;
        if index >= 0 {
            let index = usize::try_from(index).expect("nonnegative i64 fits usize");
            if index >= body_count || !mapped.insert(index) {
                return Err(artifact_error(
                    "ragdoll_closure",
                    "boneToRigidBodyMap contains an out-of-range or duplicate body",
                ));
            }
        }
    }
    if mapped.len() != body_count {
        return Err(artifact_error(
            "ragdoll_closure",
            "boneToRigidBodyMap does not close over every rigid body",
        ));
    }
    Ok(())
}

fn class_count(hkx: &HkxFile, class_name: &str) -> usize {
    hkx.objects()
        .iter()
        .filter(|object| object.class_name == class_name)
        .count()
}

fn member<'a>(object: &'a HkxObject, name: &str) -> Option<&'a HkxValue> {
    object
        .members
        .iter()
        .find(|member| member.name == name)
        .map(|member| &member.value)
}

fn array_member<'a>(
    object: &'a HkxObject,
    name: &str,
) -> Result<&'a [HkxValue], SourceRigExecutionError> {
    match member(object, name) {
        Some(HkxValue::Array(values)) => Ok(values),
        _ => Err(artifact_error(
            "ragdoll_closure",
            format!("{}.{} is not a readable array", object.class_name, name),
        )),
    }
}

fn skeleton_bones(object: &HkxObject) -> Option<Vec<(&str, Option<i32>)>> {
    let bones = match member(object, "bones")? {
        HkxValue::Array(values) => values,
        _ => return None,
    };
    let parents = match member(object, "parentIndices")? {
        HkxValue::Array(values) => values,
        _ => return None,
    };
    if bones.len() != parents.len() {
        return None;
    }
    bones
        .iter()
        .zip(parents)
        .map(|(bone, parent)| {
            let members = bone.as_object_members()?;
            let name = members.iter().find(|member| member.name == "name")?;
            let HkxValue::String {
                value,
                is_null: false,
            } = &name.value
            else {
                return None;
            };
            let parent = integer_value(parent)?;
            Some((value.as_str(), (parent >= 0).then_some(parent as i32)))
        })
        .collect()
}

fn validate_pointer_array_classes(
    hkx: &HkxFile,
    owner: &HkxObject,
    member_name: &str,
    class_name: &str,
    expected: usize,
) -> Result<(), SourceRigExecutionError> {
    let values = array_member(owner, member_name)?;
    if values.len() != expected {
        return Err(artifact_error(
            "ragdoll_closure",
            format!("{member_name} count differs from reconstructed closure"),
        ));
    }
    for value in values {
        let HkxValue::Pointer(Some(index)) = value else {
            return Err(artifact_error(
                "ragdoll_closure",
                format!("{member_name} contains a null or non-pointer entry"),
            ));
        };
        if hkx
            .objects()
            .get(*index)
            .map(|object| object.class_name.as_str())
            != Some(class_name)
        {
            return Err(artifact_error(
                "ragdoll_closure",
                format!("{member_name} does not point exclusively to {class_name}"),
            ));
        }
    }
    Ok(())
}

fn integer_value(value: &HkxValue) -> Option<i64> {
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

fn prepared_record_family_receipt(
    batch: &CreatureRecordFamilyBatch,
) -> PreparedRecordFamilyIntentReceipt {
    let mut required_target_records = batch
        .closures
        .iter()
        .flat_map(|closure| closure.required_target_records.iter().cloned())
        .collect::<Vec<_>>();
    required_target_records.sort_by_key(|record| {
        (
            record.signature.clone(),
            record.form_key.plugin.to_ascii_lowercase(),
            record.form_key.local,
        )
    });
    required_target_records.dedup();
    let mut projected_records = batch
        .closures
        .iter()
        .flat_map(|closure| closure.projected_identities.iter().cloned())
        .collect::<Vec<_>>();
    projected_records.sort_by_key(|record| {
        (
            record.signature.clone(),
            record.target_form_key.plugin.to_ascii_lowercase(),
            record.target_form_key.local,
        )
    });
    PreparedRecordFamilyIntentReceipt {
        family_id: batch.family_id.clone(),
        primary_mappings: batch.primary_mappings.clone(),
        required_target_records,
        record_count: batch
            .closures
            .iter()
            .map(|closure| closure.closure.records.len())
            .sum(),
        projected_records,
    }
}

fn packed_scaffold_receipts(
    report: &SourceRigPackReport,
) -> Result<Vec<PreparedScaffoldArtifactReceipt>, SourceRigExecutionError> {
    if report.artifacts.len() != 4 {
        return Err(SourceRigExecutionError::Pack(format!(
            "expected four capability scaffold HKX artifacts, got {}",
            report.artifacts.len()
        )));
    }
    let mut receipts = Vec::with_capacity(4);
    for artifact in &report.artifacts {
        let bytes = read(&artifact.hkx_path, "read packed scaffold")?;
        receipts.push(PreparedScaffoldArtifactReceipt {
            runtime_path: artifact.runtime_path.clone(),
            byte_len: bytes.len() as u64,
            blake3: blake3::hash(&bytes).to_hex().to_string(),
        });
    }
    receipts.sort_by_key(|receipt| receipt.runtime_path.to_ascii_lowercase());
    Ok(receipts)
}

fn join_runtime_path(root: &Path, runtime_path: &str) -> PathBuf {
    runtime_path
        .split('\\')
        .fold(root.to_path_buf(), |path, component| path.join(component))
}

fn join_data_path(root: &Path, data_path: &str) -> Result<PathBuf, SourceRigExecutionError> {
    if data_path.trim().is_empty()
        || data_path.contains('/')
        || data_path.starts_with('\\')
        || data_path
            .split('\\')
            .any(|component| component.is_empty() || component == "." || component == "..")
    {
        return Err(artifact_error(
            "runtime_model_data_path",
            format!("invalid runtime-model data path {data_path:?}"),
        ));
    }
    Ok(join_runtime_path(root, data_path))
}

fn read_to_string(path: &Path, operation: &'static str) -> Result<String, SourceRigExecutionError> {
    fs::read_to_string(path).map_err(|source| SourceRigExecutionError::Io {
        operation,
        path: path.to_path_buf(),
        source,
    })
}

fn read(path: &Path, operation: &'static str) -> Result<Vec<u8>, SourceRigExecutionError> {
    fs::read(path).map_err(|source| SourceRigExecutionError::Io {
        operation,
        path: path.to_path_buf(),
        source,
    })
}

fn create_dir_all(path: &Path) -> Result<(), SourceRigExecutionError> {
    fs::create_dir_all(path).map_err(|source| SourceRigExecutionError::Io {
        operation: "create private staging directory",
        path: path.to_path_buf(),
        source,
    })
}

fn write(path: &Path, bytes: &[u8]) -> Result<(), SourceRigExecutionError> {
    fs::write(path, bytes).map_err(|source| SourceRigExecutionError::Io {
        operation: "write private staged artifact",
        path: path.to_path_buf(),
        source,
    })
}

fn is_blake3(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn artifact_error(code: &'static str, message: impl Into<String>) -> SourceRigExecutionError {
    SourceRigExecutionError::Artifact {
        code,
        message: message.into(),
    }
}

fn unsupported(feature: &'static str, message: impl Into<String>) -> SourceRigExecutionError {
    SourceRigExecutionError::Unsupported {
        feature,
        message: message.into(),
    }
}
