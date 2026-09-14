use std::collections::BTreeSet;
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkyrimPscKind {
    QuestFragment,
    InfoFragment,
    Helper,
    AliasScript,
    RecordScript,
}

impl SkyrimPscKind {
    pub fn as_manifest_kind(self) -> &'static str {
        match self {
            Self::QuestFragment => "quest-fragment",
            Self::InfoFragment => "info-fragment",
            Self::Helper => "helper",
            Self::AliasScript => "alias-script",
            Self::RecordScript => "record-script",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkyrimPscSourceArtifact {
    pub class_name: String,
    pub source: String,
    pub kind: SkyrimPscKind,
    pub required: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeneratedPscManifestRow {
    pub manifest_id: String,
    pub class_name: String,
    pub relative_source_path: String,
    pub source_blake3: String,
    pub kind: String,
    pub required: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkyrimPscCompilerEvidence {
    pub manifest_id: String,
    pub class_name: String,
    pub relative_source_path: String,
    pub source_blake3: String,
    pub relative_pex_path: String,
    pub pex_artifact_path: PathBuf,
    pub pex_blake3: String,
    pub target_game: String,
    pub compile_run_id: String,
    pub compiled_success: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedSkyrimPscCompilerEvidence {
    pub class_name: String,
    pub relative_pex_path: String,
    pub pex_blake3: String,
    pub compile_run_id: String,
    pub entrypoints: BTreeSet<String>,
}

pub fn psc_manifest_row(
    artifact: &SkyrimPscSourceArtifact,
) -> Result<GeneratedPscManifestRow, String> {
    let mut seen = std::collections::BTreeSet::new();
    validate_artifact(artifact, &mut seen)?;
    let relative_source_path = format!("Scripts/Source/User/{}.psc", artifact.class_name);
    let source_blake3 = blake3::hash(artifact.source.as_bytes())
        .to_hex()
        .to_string();
    let manifest_id = blake3::hash(
        format!(
            "{}\0{}\0{}",
            artifact.class_name.to_ascii_lowercase(),
            normalize_relative_path(&relative_source_path),
            source_blake3
        )
        .as_bytes(),
    )
    .to_hex()
    .to_string();
    Ok(GeneratedPscManifestRow {
        manifest_id,
        class_name: artifact.class_name.clone(),
        relative_source_path,
        source_blake3,
        kind: artifact.kind.as_manifest_kind().to_string(),
        required: artifact.required,
    })
}

pub fn validate_compiler_evidence(
    manifest: &GeneratedPscManifestRow,
    evidence: Option<&SkyrimPscCompilerEvidence>,
) -> Result<ValidatedSkyrimPscCompilerEvidence, String> {
    let evidence = evidence.ok_or_else(|| {
        format!(
            "required Skyrim PSC {} has no compiler evidence",
            manifest.class_name
        )
    })?;
    if !evidence.compiled_success {
        return Err(format!(
            "Skyrim PSC {} compiler evidence is not successful",
            manifest.class_name
        ));
    }
    if evidence.manifest_id != manifest.manifest_id {
        return Err(format!(
            "Skyrim PSC {} compiler evidence identifies another manifest",
            manifest.class_name
        ));
    }
    if !evidence.target_game.eq_ignore_ascii_case("fo4") {
        return Err(format!(
            "Skyrim PSC {} was not compiled for Fallout 4",
            manifest.class_name
        ));
    }
    if !evidence
        .class_name
        .eq_ignore_ascii_case(&manifest.class_name)
        || normalize_relative_path(&evidence.relative_source_path)
            != normalize_relative_path(&manifest.relative_source_path)
    {
        return Err(format!(
            "Skyrim PSC {} compiler evidence identifies another source",
            manifest.class_name
        ));
    }
    if !evidence
        .source_blake3
        .eq_ignore_ascii_case(&manifest.source_blake3)
    {
        return Err(format!(
            "Skyrim PSC {} compiler evidence is stale",
            manifest.class_name
        ));
    }
    let expected_pex = format!("data/Scripts/{}.pex", manifest.class_name);
    if normalize_relative_path(&evidence.relative_pex_path)
        != normalize_relative_path(&expected_pex)
    {
        return Err(format!(
            "Skyrim PSC {} compiler evidence has an unexpected PEX path",
            manifest.class_name
        ));
    }
    if evidence.compile_run_id.trim().is_empty() || !is_blake3(&evidence.pex_blake3) {
        return Err(format!(
            "Skyrim PSC {} compiler evidence is incomplete",
            manifest.class_name
        ));
    }
    if !evidence.pex_artifact_path.is_absolute()
        || !path_has_relative_suffix(&evidence.pex_artifact_path, &expected_pex)
    {
        return Err(format!(
            "Skyrim PSC {} compiler evidence does not identify the expected PEX artifact",
            manifest.class_name
        ));
    }
    let pex_bytes = std::fs::read(&evidence.pex_artifact_path).map_err(|error| {
        format!(
            "read compiled Skyrim PEX {}: {error}",
            evidence.pex_artifact_path.display()
        )
    })?;
    if pex_bytes.is_empty() {
        return Err(format!(
            "compiled Skyrim PEX {} is empty",
            evidence.pex_artifact_path.display()
        ));
    }
    let actual_pex_blake3 = blake3::hash(&pex_bytes).to_hex().to_string();
    if !actual_pex_blake3.eq_ignore_ascii_case(&evidence.pex_blake3) {
        return Err(format!(
            "compiled Skyrim PEX {} does not match compiler evidence",
            evidence.pex_artifact_path.display()
        ));
    }
    let pex = papyrus_core::pex::parse_pex_bytes(&pex_bytes).map_err(|error| {
        format!(
            "parse compiled Skyrim PEX {}: {error}",
            evidence.pex_artifact_path.display()
        )
    })?;
    if pex.game_id != 2 {
        return Err(format!(
            "compiled Skyrim PEX {} is not a Fallout 4 artifact",
            evidence.pex_artifact_path.display()
        ));
    }
    let matching_objects = pex
        .objects
        .iter()
        .filter(|object| object.name.eq_ignore_ascii_case(&manifest.class_name))
        .collect::<Vec<_>>();
    if matching_objects.len() != 1 || !matching_objects[0].parent.eq_ignore_ascii_case("Quest") {
        return Err(format!(
            "compiled Skyrim PEX {} does not define quest class {}",
            evidence.pex_artifact_path.display(),
            manifest.class_name
        ));
    }
    let entrypoints = matching_objects[0]
        .states
        .iter()
        .flat_map(|state| state.functions.iter())
        .map(|function| function.name.to_ascii_lowercase())
        .collect();
    Ok(ValidatedSkyrimPscCompilerEvidence {
        class_name: manifest.class_name.clone(),
        relative_pex_path: expected_pex,
        pex_blake3: actual_pex_blake3,
        compile_run_id: evidence.compile_run_id.clone(),
        entrypoints,
    })
}

pub fn emit_script_sources(
    mod_path: &Path,
    artifacts: &[SkyrimPscSourceArtifact],
) -> Result<Vec<GeneratedPscManifestRow>, String> {
    if mod_path.as_os_str().is_empty() {
        return Err("Skyrim PSC emission requires a non-empty mod path".to_string());
    }
    let output = mod_path.join("Scripts").join("Source").join("User");
    std::fs::create_dir_all(&output)
        .map_err(|error| format!("create Skyrim PSC output directory: {error}"))?;
    let mut seen = std::collections::BTreeSet::new();
    let mut manifest = Vec::with_capacity(artifacts.len());
    for artifact in artifacts {
        validate_artifact(artifact, &mut seen)?;
        let row = psc_manifest_row(artifact)?;
        let relative_source_path = row.relative_source_path.clone();
        std::fs::write(
            output.join(format!("{}.psc", artifact.class_name)),
            artifact.source.as_bytes(),
        )
        .map_err(|error| format!("write {relative_source_path}: {error}"))?;
        manifest.push(row);
    }
    Ok(manifest)
}

fn normalize_relative_path(path: &str) -> String {
    path.trim().replace('\\', "/").to_ascii_lowercase()
}

fn path_has_relative_suffix(actual: &Path, relative: &str) -> bool {
    let actual = normalized_components(actual);
    let relative = normalized_components(Path::new(relative));
    actual.ends_with(&relative)
}

fn normalized_components(path: &Path) -> Vec<String> {
    path.components()
        .filter_map(|component| match component {
            Component::Normal(value) => Some(value.to_string_lossy().to_ascii_lowercase()),
            _ => None,
        })
        .collect()
}

fn is_blake3(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn validate_artifact(
    artifact: &SkyrimPscSourceArtifact,
    seen: &mut std::collections::BTreeSet<String>,
) -> Result<(), String> {
    if artifact.class_name.is_empty()
        || artifact.class_name.len() > 38
        || !artifact
            .class_name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        || !seen.insert(artifact.class_name.to_ascii_lowercase())
    {
        return Err(format!(
            "invalid or duplicate Skyrim Papyrus class {:?}",
            artifact.class_name
        ));
    }
    let declared = artifact
        .source
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.starts_with(';'))
        .and_then(|line| line.strip_prefix("ScriptName "))
        .and_then(|tail| tail.split_ascii_whitespace().next());
    if declared != Some(artifact.class_name.as_str()) {
        return Err(format!(
            "Skyrim PSC {} declares ScriptName {declared:?}",
            artifact.class_name
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn compile_evidence(
        root: &Path,
        artifact: &SkyrimPscSourceArtifact,
        manifest: &GeneratedPscManifestRow,
    ) -> (SkyrimPscCompilerEvidence, Vec<u8>) {
        let compiled = papyrus_core::compiler::compile_source(
            &artifact.source,
            &[],
            papyrus_core::profile::Game::Fo4,
            None,
        );
        assert!(
            compiled.ok,
            "compiler diagnostics: {:?}",
            compiled.diagnostics
        );
        let pex_bytes = compiled.pex_bytes.unwrap();
        assert!(!pex_bytes.is_empty());
        let pex_artifact_path = root
            .join("data")
            .join("Scripts")
            .join(format!("{}.pex", artifact.class_name));
        std::fs::create_dir_all(pex_artifact_path.parent().unwrap()).unwrap();
        std::fs::write(&pex_artifact_path, &pex_bytes).unwrap();
        (
            SkyrimPscCompilerEvidence {
                manifest_id: manifest.manifest_id.clone(),
                class_name: artifact.class_name.clone(),
                relative_source_path: manifest.relative_source_path.clone(),
                source_blake3: manifest.source_blake3.clone(),
                relative_pex_path: format!("data/Scripts/{}.pex", artifact.class_name),
                pex_artifact_path,
                pex_blake3: blake3::hash(&pex_bytes).to_hex().to_string(),
                target_game: "fo4".to_string(),
                compile_run_id: "compile-1".to_string(),
                compiled_success: true,
            },
            pex_bytes,
        )
    }

    #[test]
    fn rejects_mismatched_script_name_before_writing() {
        let root = tempfile::tempdir().unwrap();
        let error = emit_script_sources(
            root.path(),
            &[SkyrimPscSourceArtifact {
                class_name: "Expected".to_string(),
                source: "ScriptName Different Extends Quest\n".to_string(),
                kind: SkyrimPscKind::Helper,
                required: true,
            }],
        )
        .unwrap_err();
        assert!(error.contains("declares ScriptName"));
    }

    #[test]
    fn compiler_evidence_is_bound_to_the_source_and_real_pex() {
        let root = tempfile::tempdir().unwrap();
        let artifact = SkyrimPscSourceArtifact {
            class_name: "B21_SkyQF_000123".to_string(),
            source: "ScriptName B21_SkyQF_000123 Extends Quest\n".to_string(),
            kind: SkyrimPscKind::QuestFragment,
            required: true,
        };
        let manifest = psc_manifest_row(&artifact).unwrap();
        let (mut evidence, pex_bytes) = compile_evidence(root.path(), &artifact, &manifest);
        let validated = validate_compiler_evidence(&manifest, Some(&evidence)).unwrap();
        assert_eq!(
            validated.pex_blake3,
            blake3::hash(&pex_bytes).to_hex().to_string()
        );

        evidence.source_blake3 = blake3::hash(b"stale source").to_hex().to_string();
        let error = validate_compiler_evidence(&manifest, Some(&evidence)).unwrap_err();
        assert!(error.contains("stale"));
        assert!(
            validate_compiler_evidence(&manifest, None)
                .unwrap_err()
                .contains("no compiler evidence")
        );
    }

    #[test]
    fn compiler_evidence_rejects_missing_empty_and_hash_mismatched_pex() {
        let root = tempfile::tempdir().unwrap();
        let artifact = SkyrimPscSourceArtifact {
            class_name: "B21_SkyQF_000124".to_string(),
            source: "ScriptName B21_SkyQF_000124 Extends Quest\n".to_string(),
            kind: SkyrimPscKind::QuestFragment,
            required: true,
        };
        let manifest = psc_manifest_row(&artifact).unwrap();
        let (mut evidence, pex_bytes) = compile_evidence(root.path(), &artifact, &manifest);

        std::fs::remove_file(&evidence.pex_artifact_path).unwrap();
        assert!(
            validate_compiler_evidence(&manifest, Some(&evidence))
                .unwrap_err()
                .contains("read compiled Skyrim PEX")
        );

        std::fs::write(&evidence.pex_artifact_path, []).unwrap();
        evidence.pex_blake3 = blake3::hash(&[]).to_hex().to_string();
        assert!(
            validate_compiler_evidence(&manifest, Some(&evidence))
                .unwrap_err()
                .contains("is empty")
        );

        std::fs::write(&evidence.pex_artifact_path, &pex_bytes).unwrap();
        evidence.pex_blake3 = blake3::hash(b"another pex").to_hex().to_string();
        assert!(
            validate_compiler_evidence(&manifest, Some(&evidence))
                .unwrap_err()
                .contains("does not match compiler evidence")
        );
    }
}
