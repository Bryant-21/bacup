//! Hash-bound staged runtime-model closure receipts for creature-owned records.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use nif_core_native::model::NifFile;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::SourceCreatureIdentity;

pub const SOURCE_RIG_RUNTIME_MODEL_CLOSURE_VERSION: u32 = 1;

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct SourceRigRuntimeModelRowKey {
    pub source_game: String,
    pub source_record: SourceCreatureIdentity,
    pub source_signature: String,
    pub row_index: u32,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct SourceRigRuntimeModelRowExpectation {
    pub key: SourceRigRuntimeModelRowKey,
    pub percentage: u8,
    pub has_collision: bool,
    pub source_model_filename: String,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceRigRuntimeModelArtifactKind {
    Nif,
    Bgsm,
    Bgem,
    Dds,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct SourceRigRuntimeModelArtifactReceipt {
    pub kind: SourceRigRuntimeModelArtifactKind,
    pub source_paths: Vec<String>,
    pub target_data_path: String,
    pub byte_len: u64,
    pub blake3: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct SourceRigRuntimeModelRowReceipt {
    pub key: SourceRigRuntimeModelRowKey,
    pub percentage: u8,
    pub has_collision: bool,
    pub source_model_filename: String,
    pub source_data_path: String,
    pub source_byte_len: u64,
    pub source_blake3: String,
    pub target_model_filename: String,
    pub target_data_path: String,
    pub target_byte_len: u64,
    pub target_blake3: String,
    pub artifacts: Vec<SourceRigRuntimeModelArtifactReceipt>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct SourceRigRuntimeModelClosureReceipt {
    pub version: u32,
    pub rows: Vec<SourceRigRuntimeModelRowReceipt>,
}

impl SourceRigRuntimeModelClosureReceipt {
    pub fn validate_structure(&self) -> Result<(), SourceRigRuntimeModelClosureError> {
        validate_receipt_structure(self)
    }

    pub fn canonical_json(&self) -> Result<String, SourceRigRuntimeModelClosureError> {
        self.validate_structure()?;
        serde_json::to_string(self)
            .map_err(|error| SourceRigRuntimeModelClosureError::Serialization(error.to_string()))
    }

    pub fn stable_hash_blake3(&self) -> Result<String, SourceRigRuntimeModelClosureError> {
        Ok(blake3::hash(self.canonical_json()?.as_bytes())
            .to_hex()
            .to_string())
    }

    pub fn canonical_target_artifacts(
        &self,
    ) -> Result<Vec<SourceRigRuntimeModelArtifactReceipt>, SourceRigRuntimeModelClosureError> {
        self.validate_structure()?;
        let mut artifacts = BTreeMap::<String, SourceRigRuntimeModelArtifactReceipt>::new();
        for artifact in self.rows.iter().flat_map(|row| &row.artifacts) {
            let key = runtime_key(&artifact.target_data_path);
            match artifacts.get_mut(&key) {
                Some(existing)
                    if existing.kind != artifact.kind
                        || existing.byte_len != artifact.byte_len
                        || !existing.blake3.eq_ignore_ascii_case(&artifact.blake3) =>
                {
                    return Err(invalid(format!(
                        "target runtime path {:?} has conflicting artifact receipts",
                        artifact.target_data_path
                    )));
                }
                Some(existing) => {
                    existing.source_paths.extend(artifact.source_paths.clone());
                    existing.source_paths.sort_by_key(|path| runtime_key(path));
                    existing
                        .source_paths
                        .dedup_by(|left, right| runtime_key(left) == runtime_key(right));
                }
                None => {
                    artifacts.insert(key, artifact.clone());
                }
            }
        }
        Ok(artifacts.into_values().collect())
    }
}

#[derive(Debug, Error)]
pub enum SourceRigRuntimeModelClosureError {
    #[error("invalid runtime-model closure: {0}")]
    Invalid(String),
    #[error("runtime-model closure I/O failed for {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("runtime-model closure serialization failed: {0}")]
    Serialization(String),
}

pub fn validate_source_rig_runtime_model_closure(
    expected_rows: &[SourceRigRuntimeModelRowExpectation],
    receipt: &SourceRigRuntimeModelClosureReceipt,
    source_data_roots: &BTreeMap<String, PathBuf>,
    staged_data_root: &Path,
) -> Result<(), SourceRigRuntimeModelClosureError> {
    receipt.validate_structure()?;
    if expected_rows.is_empty() || receipt.rows.is_empty() {
        return Err(invalid("runtime-model closure cannot be empty"));
    }

    let mut expected = BTreeMap::new();
    for row in expected_rows {
        validate_key(&row.key)?;
        validate_model_filename(&row.source_model_filename, "source model filename")?;
        if expected.insert(row.key.clone(), row).is_some() {
            return Err(invalid(format!("duplicate expected row {:?}", row.key)));
        }
    }
    let mut target_paths = BTreeMap::<String, (u64, String)>::new();
    for row in &receipt.rows {
        let expectation = expected
            .remove(&row.key)
            .ok_or_else(|| invalid(format!("receipt contains unexpected row {:?}", row.key)))?;
        validate_row(
            expectation,
            row,
            source_data_roots,
            staged_data_root,
            &mut target_paths,
        )?;
    }
    if !expected.is_empty() {
        return Err(invalid(format!(
            "receipt is missing {} expected row(s)",
            expected.len()
        )));
    }
    Ok(())
}

fn validate_receipt_structure(
    receipt: &SourceRigRuntimeModelClosureReceipt,
) -> Result<(), SourceRigRuntimeModelClosureError> {
    if receipt.version != SOURCE_RIG_RUNTIME_MODEL_CLOSURE_VERSION {
        return Err(invalid(format!(
            "unsupported receipt version {}",
            receipt.version
        )));
    }
    if receipt.rows.is_empty() {
        return Err(invalid("runtime-model closure cannot be empty"));
    }
    validate_receipt_order(receipt)?;
    let mut global_targets = BTreeMap::new();
    for row in &receipt.rows {
        validate_key(&row.key)?;
        validate_model_filename(&row.source_model_filename, "source model filename")?;
        validate_model_filename(&row.target_model_filename, "target model filename")?;
        validate_runtime_path(&row.source_data_path, Some("meshes"))?;
        validate_runtime_path(&row.target_data_path, Some("meshes"))?;
        if !row
            .source_data_path
            .eq_ignore_ascii_case(&format!("meshes\\{}", row.source_model_filename))
            || !row
                .target_data_path
                .eq_ignore_ascii_case(&format!("meshes\\{}", row.target_model_filename))
        {
            return Err(invalid(format!(
                "runtime-model row {:?} filename/path binding disagrees",
                row.key
            )));
        }
        validate_hash(&row.source_blake3, "source model")?;
        validate_hash(&row.target_blake3, "target model")?;
        register_target(
            &mut global_targets,
            &runtime_key(&row.target_data_path),
            row.target_byte_len,
            &row.target_blake3,
        )?;
        let mut target_paths = BTreeSet::new();
        for artifact in &row.artifacts {
            if artifact.source_paths.is_empty() {
                return Err(invalid(format!(
                    "runtime-model artifact {:?} has no source provenance",
                    artifact.target_data_path
                )));
            }
            for source in &artifact.source_paths {
                validate_runtime_path(source, None)?;
            }
            validate_runtime_path(&artifact.target_data_path, None)?;
            validate_kind_matches_path(artifact)?;
            validate_hash(&artifact.blake3, "target artifact")?;
            register_target(
                &mut global_targets,
                &runtime_key(&artifact.target_data_path),
                artifact.byte_len,
                &artifact.blake3,
            )?;
            if !target_paths.insert(runtime_key(&artifact.target_data_path)) {
                return Err(invalid(format!(
                    "runtime-model row {:?} repeats a target artifact",
                    row.key
                )));
            }
        }
        let primary_key = runtime_key(&row.target_data_path);
        let primary = row
            .artifacts
            .iter()
            .find(|artifact| runtime_key(&artifact.target_data_path) == primary_key)
            .ok_or_else(|| {
                invalid(format!(
                    "runtime-model row {:?} lacks its primary NIF artifact",
                    row.key
                ))
            })?;
        if primary.kind != SourceRigRuntimeModelArtifactKind::Nif
            || primary.byte_len != row.target_byte_len
            || !primary.blake3.eq_ignore_ascii_case(&row.target_blake3)
            || !primary
                .source_paths
                .iter()
                .any(|path| runtime_key(path) == runtime_key(&row.source_data_path))
        {
            return Err(invalid(format!(
                "runtime-model row {:?} primary NIF artifact disagrees with the row receipt",
                row.key
            )));
        }
    }
    Ok(())
}

fn validate_receipt_order(
    receipt: &SourceRigRuntimeModelClosureReceipt,
) -> Result<(), SourceRigRuntimeModelClosureError> {
    let keys = receipt
        .rows
        .iter()
        .map(|row| row.key.clone())
        .collect::<Vec<_>>();
    let mut ordered = keys.clone();
    ordered.sort();
    if keys != ordered || ordered.windows(2).any(|pair| pair[0] == pair[1]) {
        return Err(invalid(
            "runtime-model rows must be sorted and unique by source row key",
        ));
    }
    for row in &receipt.rows {
        let artifact_keys = row
            .artifacts
            .iter()
            .map(|artifact| {
                (
                    runtime_key(&artifact.target_data_path),
                    artifact.kind,
                    artifact.source_paths.clone(),
                )
            })
            .collect::<Vec<_>>();
        let mut ordered_artifacts = artifact_keys.clone();
        ordered_artifacts.sort();
        if artifact_keys != ordered_artifacts {
            return Err(invalid(format!(
                "runtime-model row {:?} artifacts are not canonically sorted",
                row.key
            )));
        }
        for artifact in &row.artifacts {
            let mut ordered_sources = artifact.source_paths.clone();
            ordered_sources.sort_by_key(|path| runtime_key(path));
            ordered_sources.dedup_by(|left, right| runtime_key(left) == runtime_key(right));
            if artifact.source_paths != ordered_sources {
                return Err(invalid(format!(
                    "runtime-model artifact {:?} source paths are not sorted and unique",
                    artifact.target_data_path
                )));
            }
        }
    }
    Ok(())
}

fn validate_row(
    expectation: &SourceRigRuntimeModelRowExpectation,
    row: &SourceRigRuntimeModelRowReceipt,
    source_data_roots: &BTreeMap<String, PathBuf>,
    staged_data_root: &Path,
    target_paths: &mut BTreeMap<String, (u64, String)>,
) -> Result<(), SourceRigRuntimeModelClosureError> {
    validate_key(&row.key)?;
    if row.percentage != expectation.percentage
        || row.has_collision != expectation.has_collision
        || row.source_model_filename != expectation.source_model_filename
    {
        return Err(invalid(format!(
            "runtime-model row {:?} does not match source DATA semantics",
            row.key
        )));
    }
    validate_model_filename(&row.source_model_filename, "source model filename")?;
    validate_model_filename(&row.target_model_filename, "target model filename")?;
    let expected_source_path = format!("meshes\\{}", row.source_model_filename);
    let expected_target_path = format!("meshes\\{}", row.target_model_filename);
    if !row
        .source_data_path
        .eq_ignore_ascii_case(&expected_source_path)
        || !row
            .target_data_path
            .eq_ignore_ascii_case(&expected_target_path)
    {
        return Err(invalid(format!(
            "runtime-model row {:?} filename/path binding disagrees",
            row.key
        )));
    }
    validate_runtime_path(&row.source_data_path, Some("meshes"))?;
    validate_runtime_path(&row.target_data_path, Some("meshes"))?;
    validate_hash(&row.source_blake3, "source model")?;
    validate_hash(&row.target_blake3, "target model")?;

    let source_root = source_data_roots
        .iter()
        .find_map(|(game, root)| {
            game.eq_ignore_ascii_case(&row.key.source_game)
                .then_some(root)
        })
        .ok_or_else(|| {
            invalid(format!(
                "missing source data root for {:?}",
                row.key.source_game
            ))
        })?;
    let source_path = source_root.join(path_from_runtime(&row.source_data_path));
    validate_file_receipt(
        &source_path,
        row.source_byte_len,
        &row.source_blake3,
        "source model",
    )?;
    let target_path = staged_data_root.join(path_from_runtime(&row.target_data_path));
    let target_bytes = validate_file_receipt(
        &target_path,
        row.target_byte_len,
        &row.target_blake3,
        "target model",
    )?;
    let target_nif = NifFile::from_bytes(&target_bytes, Some(target_path.clone()))
        .map_err(|error| invalid(format!("target NIF does not parse: {error}")))?;
    if target_nif.header.version != (20, 2, 0, 7)
        || target_nif.header.user_version != 12
        || target_nif.header.bs_version != 130
    {
        return Err(invalid(format!(
            "target NIF {:?} is not Fallout 4",
            row.target_data_path
        )));
    }
    if row.has_collision
        && !target_nif.blocks.iter().any(|block| {
            block
                .type_name
                .to_ascii_lowercase()
                .contains("collisionobject")
        })
    {
        return Err(invalid(format!(
            "collision-bearing row {:?} lost target NIF collision",
            row.key
        )));
    }

    let target_key = runtime_key(&row.target_data_path);
    register_target(
        target_paths,
        &target_key,
        row.target_byte_len,
        &row.target_blake3,
    )?;
    let mut artifacts = BTreeMap::new();
    for artifact in &row.artifacts {
        validate_artifact(artifact, staged_data_root, target_paths)?;
        let key = runtime_key(&artifact.target_data_path);
        if artifacts.insert(key.clone(), artifact).is_some() {
            return Err(invalid(format!(
                "runtime-model row {:?} repeats target artifact {key:?}",
                row.key
            )));
        }
    }
    let primary = artifacts.get(&target_key).ok_or_else(|| {
        invalid(format!(
            "runtime-model row {:?} lacks its primary NIF artifact",
            row.key
        ))
    })?;
    if primary.kind != SourceRigRuntimeModelArtifactKind::Nif
        || primary.byte_len != row.target_byte_len
        || !primary.blake3.eq_ignore_ascii_case(&row.target_blake3)
        || !primary
            .source_paths
            .iter()
            .any(|path| runtime_key(path) == runtime_key(&row.source_data_path))
    {
        return Err(invalid(format!(
            "runtime-model row {:?} primary NIF artifact disagrees with the row receipt",
            row.key
        )));
    }

    let mut required = BTreeSet::from([target_key]);
    let references = target_nif.referenced_asset_paths();
    for referenced in references.materials.iter().chain(&references.textures) {
        required.insert(runtime_key(referenced));
    }
    for artifact in artifacts.values().filter(|artifact| {
        matches!(
            artifact.kind,
            SourceRigRuntimeModelArtifactKind::Bgsm | SourceRigRuntimeModelArtifactKind::Bgem
        )
    }) {
        let path = staged_data_root.join(path_from_runtime(&artifact.target_data_path));
        validate_material_kind(&path, artifact.kind)?;
        for referenced in crate::relocation::read_material_texture_paths(&path) {
            required.insert(runtime_key(&referenced));
        }
    }
    let actual = artifacts.keys().cloned().collect::<BTreeSet<_>>();
    if actual != required {
        return Err(invalid(format!(
            "runtime-model row {:?} artifact closure differs: required={required:?} actual={actual:?}",
            row.key
        )));
    }
    for required_path in required {
        let artifact = artifacts.get(&required_path).expect("sets are equal");
        validate_kind_matches_path(artifact)?;
    }
    Ok(())
}

fn validate_artifact(
    artifact: &SourceRigRuntimeModelArtifactReceipt,
    staged_data_root: &Path,
    target_paths: &mut BTreeMap<String, (u64, String)>,
) -> Result<(), SourceRigRuntimeModelClosureError> {
    if artifact.source_paths.is_empty() {
        return Err(invalid(format!(
            "runtime-model artifact {:?} has no source provenance",
            artifact.target_data_path
        )));
    }
    for source in &artifact.source_paths {
        validate_runtime_path(source, None)?;
    }
    validate_runtime_path(&artifact.target_data_path, None)?;
    validate_kind_matches_path(artifact)?;
    validate_hash(&artifact.blake3, "target artifact")?;
    let path = staged_data_root.join(path_from_runtime(&artifact.target_data_path));
    validate_file_receipt(
        &path,
        artifact.byte_len,
        &artifact.blake3,
        "target artifact",
    )?;
    register_target(
        target_paths,
        &runtime_key(&artifact.target_data_path),
        artifact.byte_len,
        &artifact.blake3,
    )
}

fn register_target(
    target_paths: &mut BTreeMap<String, (u64, String)>,
    path: &str,
    byte_len: u64,
    hash: &str,
) -> Result<(), SourceRigRuntimeModelClosureError> {
    match target_paths.get(path) {
        Some((existing_len, existing_hash))
            if *existing_len != byte_len || !existing_hash.eq_ignore_ascii_case(hash) =>
        {
            Err(invalid(format!(
                "target runtime path {path:?} has conflicting byte commitments"
            )))
        }
        Some(_) => Ok(()),
        None => {
            target_paths.insert(path.to_string(), (byte_len, hash.to_string()));
            Ok(())
        }
    }
}

fn validate_key(
    key: &SourceRigRuntimeModelRowKey,
) -> Result<(), SourceRigRuntimeModelClosureError> {
    if key.source_game.trim().is_empty()
        || !key.source_record.is_valid()
        || key.source_signature.len() != 4
        || !key
            .source_signature
            .bytes()
            .all(|byte| byte.is_ascii_uppercase() || byte == b'_')
    {
        return Err(invalid(format!("invalid runtime-model row key {key:?}")));
    }
    Ok(())
}

fn validate_model_filename(
    filename: &str,
    label: &str,
) -> Result<(), SourceRigRuntimeModelClosureError> {
    if filename.trim().is_empty()
        || filename.contains('/')
        || filename.starts_with('\\')
        || filename.to_ascii_lowercase().starts_with("meshes\\")
        || !filename.to_ascii_lowercase().ends_with(".nif")
        || filename
            .split('\\')
            .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return Err(invalid(format!("invalid {label} {filename:?}")));
    }
    Ok(())
}

fn validate_runtime_path(
    path: &str,
    expected_root: Option<&str>,
) -> Result<(), SourceRigRuntimeModelClosureError> {
    if path.trim().is_empty()
        || path.contains('/')
        || path.starts_with('\\')
        || path
            .split('\\')
            .any(|part| part.is_empty() || part == "." || part == "..")
        || expected_root.is_some_and(|root| {
            !path
                .split('\\')
                .next()
                .is_some_and(|part| part.eq_ignore_ascii_case(root))
        })
    {
        return Err(invalid(format!("invalid runtime data path {path:?}")));
    }
    Ok(())
}

fn validate_kind_matches_path(
    artifact: &SourceRigRuntimeModelArtifactReceipt,
) -> Result<(), SourceRigRuntimeModelClosureError> {
    let expected = match artifact.kind {
        SourceRigRuntimeModelArtifactKind::Nif => ".nif",
        SourceRigRuntimeModelArtifactKind::Bgsm => ".bgsm",
        SourceRigRuntimeModelArtifactKind::Bgem => ".bgem",
        SourceRigRuntimeModelArtifactKind::Dds => ".dds",
    };
    if !artifact
        .target_data_path
        .to_ascii_lowercase()
        .ends_with(expected)
    {
        return Err(invalid(format!(
            "runtime-model artifact {:?} kind {:?} disagrees with its extension",
            artifact.target_data_path, artifact.kind
        )));
    }
    Ok(())
}

fn validate_material_kind(
    path: &Path,
    kind: SourceRigRuntimeModelArtifactKind,
) -> Result<(), SourceRigRuntimeModelClosureError> {
    let bytes = fs::read(path).map_err(|source| SourceRigRuntimeModelClosureError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let parsed = match kind {
        SourceRigRuntimeModelArtifactKind::Bgsm => {
            materials_native::bgsm::parse(&bytes).map(|_| ())
        }
        SourceRigRuntimeModelArtifactKind::Bgem => {
            materials_native::bgem::parse(&bytes).map(|_| ())
        }
        _ => return Ok(()),
    };
    parsed.map_err(|error| {
        invalid(format!(
            "target material {} does not parse: {error}",
            path.display()
        ))
    })
}

fn validate_file_receipt(
    path: &Path,
    expected_len: u64,
    expected_hash: &str,
    label: &str,
) -> Result<Vec<u8>, SourceRigRuntimeModelClosureError> {
    let bytes = fs::read(path).map_err(|source| SourceRigRuntimeModelClosureError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let actual_hash = blake3::hash(&bytes).to_hex().to_string();
    if bytes.len() as u64 != expected_len || !actual_hash.eq_ignore_ascii_case(expected_hash) {
        return Err(invalid(format!(
            "{label} {} byte commitment does not match",
            path.display()
        )));
    }
    Ok(bytes)
}

fn validate_hash(hash: &str, label: &str) -> Result<(), SourceRigRuntimeModelClosureError> {
    if hash.len() != 64 || !hash.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(invalid(format!("{label} has an invalid BLAKE3 hash")));
    }
    Ok(())
}

fn runtime_key(path: &str) -> String {
    path.replace('\\', "/").to_ascii_lowercase()
}

fn path_from_runtime(path: &str) -> PathBuf {
    path.split('\\').collect()
}

fn invalid(message: impl Into<String>) -> SourceRigRuntimeModelClosureError {
    SourceRigRuntimeModelClosureError::Invalid(message.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixture {
        expected: Vec<SourceRigRuntimeModelRowExpectation>,
        receipt: SourceRigRuntimeModelClosureReceipt,
        source_roots: BTreeMap<String, PathBuf>,
        staged_root: PathBuf,
        _temp: tempfile::TempDir,
    }

    #[test]
    fn runtime_model_closure_validates_exact_row_and_staged_nif() {
        let fixture = runtime_model_fixture(&["fnv"]);

        validate_source_rig_runtime_model_closure(
            &fixture.expected,
            &fixture.receipt,
            &fixture.source_roots,
            &fixture.staged_root,
        )
        .expect("runtime-model closure");
        assert_eq!(
            fixture.receipt.stable_hash_blake3().unwrap(),
            fixture.receipt.stable_hash_blake3().unwrap()
        );
    }

    #[test]
    fn runtime_model_closure_keeps_same_runtime_path_provenance_distinct() {
        let fixture = runtime_model_fixture(&["fnv", "fo3"]);

        validate_source_rig_runtime_model_closure(
            &fixture.expected,
            &fixture.receipt,
            &fixture.source_roots,
            &fixture.staged_root,
        )
        .expect("byte-identical target dedup");
        assert_eq!(fixture.receipt.rows.len(), 2);
        assert_ne!(
            fixture.receipt.rows[0].key.source_record,
            fixture.receipt.rows[1].key.source_record
        );

        let mut conflicting = fixture.receipt.clone();
        let conflicting_hash = blake3::hash(b"conflicting target").to_hex().to_string();
        conflicting.rows[1].target_blake3 = conflicting_hash.clone();
        conflicting.rows[1].artifacts[0].blake3 = conflicting_hash;
        assert!(
            conflicting
                .validate_structure()
                .unwrap_err()
                .to_string()
                .contains("conflicting byte commitments")
        );
    }

    #[test]
    fn runtime_model_closure_rejects_missing_row_and_forged_bytes() {
        let mut fixture = runtime_model_fixture(&["fnv", "fo3"]);
        fixture.receipt.rows.pop();
        assert!(
            validate_source_rig_runtime_model_closure(
                &fixture.expected,
                &fixture.receipt,
                &fixture.source_roots,
                &fixture.staged_root,
            )
            .unwrap_err()
            .to_string()
            .contains("missing 1 expected row")
        );

        let fixture = runtime_model_fixture(&["fnv"]);
        fs::write(
            fixture
                .staged_root
                .join(path_from_runtime(&fixture.receipt.rows[0].target_data_path)),
            b"forged",
        )
        .unwrap();
        assert!(
            validate_source_rig_runtime_model_closure(
                &fixture.expected,
                &fixture.receipt,
                &fixture.source_roots,
                &fixture.staged_root,
            )
            .unwrap_err()
            .to_string()
            .contains("target model")
        );
    }

    #[test]
    fn runtime_model_closure_rejects_lost_collision_and_extra_artifact() {
        let mut fixture = runtime_model_fixture(&["fnv"]);
        fixture.expected[0].has_collision = true;
        fixture.receipt.rows[0].has_collision = true;
        assert!(
            validate_source_rig_runtime_model_closure(
                &fixture.expected,
                &fixture.receipt,
                &fixture.source_roots,
                &fixture.staged_root,
            )
            .unwrap_err()
            .to_string()
            .contains("lost target NIF collision")
        );

        let mut fixture = runtime_model_fixture(&["fnv"]);
        let extra_path = fixture
            .staged_root
            .join("textures")
            .join("unreferenced.dds");
        fs::create_dir_all(extra_path.parent().unwrap()).unwrap();
        fs::write(&extra_path, b"dds").unwrap();
        fixture.receipt.rows[0]
            .artifacts
            .push(SourceRigRuntimeModelArtifactReceipt {
                kind: SourceRigRuntimeModelArtifactKind::Dds,
                source_paths: vec!["textures\\unreferenced.dds".to_string()],
                target_data_path: "textures\\unreferenced.dds".to_string(),
                byte_len: 3,
                blake3: blake3::hash(b"dds").to_hex().to_string(),
            });
        fixture.receipt.rows[0].artifacts.sort_by_key(|artifact| {
            (
                runtime_key(&artifact.target_data_path),
                artifact.kind,
                artifact.source_paths.clone(),
            )
        });
        assert!(
            validate_source_rig_runtime_model_closure(
                &fixture.expected,
                &fixture.receipt,
                &fixture.source_roots,
                &fixture.staged_root,
            )
            .unwrap_err()
            .to_string()
            .contains("artifact closure differs")
        );
    }

    fn runtime_model_fixture(games: &[&str]) -> Fixture {
        let temp = tempfile::tempdir().unwrap();
        let staged_root = temp.path().join("staged");
        let target_runtime_path = "meshes\\effects\\debris_fo4.nif";
        let target_path = staged_root.join(path_from_runtime(target_runtime_path));
        fs::create_dir_all(target_path.parent().unwrap()).unwrap();
        let target_bytes = NifFile::new("fo4").to_bytes().unwrap();
        fs::write(&target_path, &target_bytes).unwrap();
        let target_hash = blake3::hash(&target_bytes).to_hex().to_string();

        let mut expected = Vec::new();
        let mut rows = Vec::new();
        let mut source_roots = BTreeMap::new();
        for (index, game) in games.iter().enumerate() {
            let source_root = temp.path().join(format!("source-{game}"));
            let source_runtime_path = "meshes\\effects\\debris.nif";
            let source_path = source_root.join(path_from_runtime(source_runtime_path));
            fs::create_dir_all(source_path.parent().unwrap()).unwrap();
            let source_bytes = format!("{game}-source-nif").into_bytes();
            fs::write(&source_path, &source_bytes).unwrap();
            source_roots.insert((*game).to_string(), source_root);
            let key = SourceRigRuntimeModelRowKey {
                source_game: (*game).to_string(),
                source_record: SourceCreatureIdentity {
                    namespace: (*game).to_string(),
                    plugin: format!("{game}.esm"),
                    local_form_id: 0x1000 + index as u32,
                },
                source_signature: "DEBR".to_string(),
                row_index: 0,
            };
            expected.push(SourceRigRuntimeModelRowExpectation {
                key: key.clone(),
                percentage: 100,
                has_collision: false,
                source_model_filename: "effects\\debris.nif".to_string(),
            });
            rows.push(SourceRigRuntimeModelRowReceipt {
                key,
                percentage: 100,
                has_collision: false,
                source_model_filename: "effects\\debris.nif".to_string(),
                source_data_path: source_runtime_path.to_string(),
                source_byte_len: source_bytes.len() as u64,
                source_blake3: blake3::hash(&source_bytes).to_hex().to_string(),
                target_model_filename: "effects\\debris_fo4.nif".to_string(),
                target_data_path: target_runtime_path.to_string(),
                target_byte_len: target_bytes.len() as u64,
                target_blake3: target_hash.clone(),
                artifacts: vec![SourceRigRuntimeModelArtifactReceipt {
                    kind: SourceRigRuntimeModelArtifactKind::Nif,
                    source_paths: vec![source_runtime_path.to_string()],
                    target_data_path: target_runtime_path.to_string(),
                    byte_len: target_bytes.len() as u64,
                    blake3: target_hash.clone(),
                }],
            });
        }
        expected.sort_by(|left, right| left.key.cmp(&right.key));
        rows.sort_by(|left, right| left.key.cmp(&right.key));
        Fixture {
            expected,
            receipt: SourceRigRuntimeModelClosureReceipt {
                version: SOURCE_RIG_RUNTIME_MODEL_CLOSURE_VERSION,
                rows,
            },
            source_roots,
            staged_root,
            _temp: temp,
        }
    }
}
