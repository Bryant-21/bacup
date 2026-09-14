use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use havok_native::hkx::HkxFile;

use super::emit::{
    SourceRigScaffold, emit_capability_scaffold, emit_idle_scaffold, emit_mvp_scaffold,
    emit_mvp_scaffold_with_motion,
};
use super::manifest::{
    CapabilityGraphManifest, CreatureManifest, MvpGraphManifest, MvpMotionManifest,
    ValidationErrors,
};
use super::xml::validate_fo4_havok_xml_signatures;

const FO4_CLASS_VERSION: u32 = 11;
const FO4_CLASS_VERSION_XML: &str = "11";
const FO4_CONTENTS_VERSION: &str = "hk_2014.1.0-r1";
const AMD64_POINTER_SIZE: u8 = 8;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceRigRuntimeManifest {
    pub project: String,
    pub character: String,
    pub root_behavior: String,
    pub core_behavior: String,
    pub animation_skeleton: String,
    pub ragdoll: Option<String>,
    pub animation_clips: Vec<String>,
}

impl SourceRigRuntimeManifest {
    pub fn cross_file_paths(&self) -> Vec<&str> {
        let mut paths = vec![
            self.project.as_str(),
            self.character.as_str(),
            self.root_behavior.as_str(),
            self.core_behavior.as_str(),
            self.animation_skeleton.as_str(),
        ];
        if let Some(ragdoll) = &self.ragdoll {
            paths.push(ragdoll);
        }
        paths.extend(self.animation_clips.iter().map(String::as_str));
        paths
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PackedScaffoldArtifact {
    pub runtime_path: String,
    pub source_xml_path: String,
    pub xml_path: PathBuf,
    pub hkx_path: PathBuf,
    pub byte_len: usize,
    pub class_version: u32,
    pub contents_version: String,
    pub pointer_size: u8,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceRigPackReport {
    pub output_root: PathBuf,
    pub race_visual_skeleton_nif: String,
    pub runtime_manifest: SourceRigRuntimeManifest,
    pub artifacts: Vec<PackedScaffoldArtifact>,
}

impl SourceRigPackReport {
    pub fn artifact(&self, runtime_path: &str) -> Option<&PackedScaffoldArtifact> {
        self.artifacts
            .iter()
            .find(|artifact| artifact.runtime_path.eq_ignore_ascii_case(runtime_path))
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SourceRigPackError {
    #[error(transparent)]
    Validation(#[from] ValidationErrors),
    #[error("failed to {operation} {path}: {source}")]
    Io {
        operation: &'static str,
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("failed to pack {runtime_path}: {source}")]
    Pack {
        runtime_path: String,
        #[source]
        source: havok_native::error::HavokError,
    },
    #[error("failed to read packed {runtime_path}: {source}")]
    ReadPacked {
        runtime_path: String,
        #[source]
        source: havok_native::error::HavokError,
    },
    #[error("packed {runtime_path} has an invalid FO4 header: {message}")]
    InvalidHeader {
        runtime_path: String,
        message: String,
    },
    #[error("failed to unpack packed {runtime_path} for validation: {source}")]
    UnpackPacked {
        runtime_path: String,
        #[source]
        source: havok_native::error::HavokError,
    },
    #[error("packed {runtime_path} produced invalid round-trip XML: {message}")]
    InvalidRoundtripXml {
        runtime_path: String,
        message: String,
    },
}

pub fn pack_idle_scaffold(
    manifest: &CreatureManifest,
    output_root: impl AsRef<Path>,
) -> Result<SourceRigPackReport, SourceRigPackError> {
    let scaffold = emit_idle_scaffold(manifest)?;
    pack_scaffold(manifest, scaffold, output_root.as_ref())
}

pub fn pack_mvp_scaffold(
    manifest: &CreatureManifest,
    graph: &MvpGraphManifest,
    output_root: impl AsRef<Path>,
) -> Result<SourceRigPackReport, SourceRigPackError> {
    let scaffold = emit_mvp_scaffold(manifest, graph)?;
    pack_scaffold(manifest, scaffold, output_root.as_ref())
}

pub fn pack_mvp_scaffold_with_motion(
    manifest: &CreatureManifest,
    graph: &MvpGraphManifest,
    motion: &MvpMotionManifest,
    output_root: impl AsRef<Path>,
) -> Result<SourceRigPackReport, SourceRigPackError> {
    let scaffold = emit_mvp_scaffold_with_motion(manifest, graph, motion)?;
    pack_scaffold(manifest, scaffold, output_root.as_ref())
}

pub fn pack_capability_scaffold(
    manifest: &CreatureManifest,
    graph: &CapabilityGraphManifest,
    output_root: impl AsRef<Path>,
) -> Result<SourceRigPackReport, SourceRigPackError> {
    let scaffold = emit_capability_scaffold(manifest, graph)?;
    pack_scaffold(manifest, scaffold, output_root.as_ref())
}

fn pack_scaffold(
    manifest: &CreatureManifest,
    scaffold: SourceRigScaffold,
    output_root: &Path,
) -> Result<SourceRigPackReport, SourceRigPackError> {
    let output_root = output_root.to_path_buf();
    create_dir_all(&output_root)?;

    let mut artifacts = Vec::with_capacity(scaffold.artifacts.len());
    for artifact in &scaffold.artifacts {
        let xml_path = join_runtime_path(&output_root, &artifact.source_xml_path);
        let hkx_path = join_runtime_path(&output_root, &artifact.runtime_path);
        if let Some(parent) = xml_path.parent() {
            create_dir_all(parent)?;
        }
        if let Some(parent) = hkx_path.parent() {
            create_dir_all(parent)?;
        }

        write(&xml_path, artifact.xml.as_bytes())?;
        let packed = havok_native::api::havok_xml_to_hkx(&artifact.xml).map_err(|source| {
            SourceRigPackError::Pack {
                runtime_path: artifact.runtime_path.clone(),
                source,
            }
        })?;
        write(&hkx_path, &packed)?;

        let reread_bytes = read(&hkx_path)?;
        let reread =
            HkxFile::read(&reread_bytes).map_err(|source| SourceRigPackError::ReadPacked {
                runtime_path: artifact.runtime_path.clone(),
                source,
            })?;
        validate_packed_header(&artifact.runtime_path, &reread)?;

        let roundtrip_xml =
            havok_native::api::havok_hkx_to_xml(&reread_bytes).map_err(|source| {
                SourceRigPackError::UnpackPacked {
                    runtime_path: artifact.runtime_path.clone(),
                    source,
                }
            })?;
        validate_roundtrip_xml_header(&artifact.runtime_path, &roundtrip_xml)?;
        validate_fo4_havok_xml_signatures(&roundtrip_xml).map_err(|source| {
            SourceRigPackError::InvalidRoundtripXml {
                runtime_path: artifact.runtime_path.clone(),
                message: source.to_string(),
            }
        })?;

        artifacts.push(PackedScaffoldArtifact {
            runtime_path: artifact.runtime_path.clone(),
            source_xml_path: artifact.source_xml_path.clone(),
            xml_path,
            hkx_path,
            byte_len: reread_bytes.len(),
            class_version: reread.class_version(),
            contents_version: reread.contents_version().to_string(),
            pointer_size: reread.packfile().header.pointer_size,
        });
    }

    Ok(SourceRigPackReport {
        output_root,
        race_visual_skeleton_nif: scaffold.race_visual_skeleton_nif.clone(),
        runtime_manifest: runtime_manifest(manifest, &scaffold),
        artifacts,
    })
}

fn runtime_manifest(
    manifest: &CreatureManifest,
    scaffold: &SourceRigScaffold,
) -> SourceRigRuntimeManifest {
    SourceRigRuntimeManifest {
        project: manifest.paths.project.clone(),
        character: manifest.paths.character.clone(),
        root_behavior: manifest.paths.root_behavior.clone(),
        core_behavior: manifest.paths.core_behavior.clone(),
        animation_skeleton: manifest.animation_skeleton.path.clone(),
        ragdoll: manifest.ragdoll.runtime_path().map(str::to_string),
        animation_clips: scaffold.animation_clips.clone(),
    }
}

fn validate_packed_header(runtime_path: &str, packed: &HkxFile) -> Result<(), SourceRigPackError> {
    let pointer_size = packed.packfile().header.pointer_size;
    if packed.class_version() != FO4_CLASS_VERSION
        || packed.contents_version() != FO4_CONTENTS_VERSION
        || pointer_size != AMD64_POINTER_SIZE
    {
        return Err(SourceRigPackError::InvalidHeader {
            runtime_path: runtime_path.to_string(),
            message: format!(
                "expected classversion {FO4_CLASS_VERSION}, contents {FO4_CONTENTS_VERSION}, pointer size {AMD64_POINTER_SIZE}; got classversion {}, contents {}, pointer size {pointer_size}",
                packed.class_version(),
                packed.contents_version()
            ),
        });
    }
    Ok(())
}

fn validate_roundtrip_xml_header(runtime_path: &str, xml: &str) -> Result<(), SourceRigPackError> {
    let document = roxmltree::Document::parse(xml).map_err(|source| {
        SourceRigPackError::InvalidRoundtripXml {
            runtime_path: runtime_path.to_string(),
            message: source.to_string(),
        }
    })?;
    let root = document.root_element();
    let class_version = root.attribute("classversion").unwrap_or_default();
    let contents_version = root.attribute("contentsversion").unwrap_or_default();
    if root.tag_name().name() != "hkpackfile"
        || class_version != FO4_CLASS_VERSION_XML
        || contents_version != FO4_CONTENTS_VERSION
    {
        return Err(SourceRigPackError::InvalidRoundtripXml {
            runtime_path: runtime_path.to_string(),
            message: format!(
                "expected hkpackfile classversion {FO4_CLASS_VERSION} contents {FO4_CONTENTS_VERSION}; got {} classversion {class_version:?} contents {contents_version:?}",
                root.tag_name().name()
            ),
        });
    }
    Ok(())
}

fn join_runtime_path(root: &Path, runtime_path: &str) -> PathBuf {
    runtime_path
        .split('\\')
        .fold(root.to_path_buf(), |path, component| path.join(component))
}

fn create_dir_all(path: &Path) -> Result<(), SourceRigPackError> {
    fs::create_dir_all(path).map_err(|source| SourceRigPackError::Io {
        operation: "create directory",
        path: path.to_path_buf(),
        source,
    })
}

fn write(path: &Path, contents: &[u8]) -> Result<(), SourceRigPackError> {
    fs::write(path, contents).map_err(|source| SourceRigPackError::Io {
        operation: "write",
        path: path.to_path_buf(),
        source,
    })
}

fn read(path: &Path) -> Result<Vec<u8>, SourceRigPackError> {
    fs::read(path).map_err(|source| SourceRigPackError::Io {
        operation: "read",
        path: path.to_path_buf(),
        source,
    })
}
