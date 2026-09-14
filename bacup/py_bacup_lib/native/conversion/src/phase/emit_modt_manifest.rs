//! Phase: `emit_modt_manifest`, the producer of the MODT compute manifest.
//!
//! Walks the mod's output meshes (`data/Meshes/**/*.nif`), resolves each mesh's
//! external materials (`.bgsm`/`.bgem`), and emits a [`MeshModtManifest`] mapping
//! `normalize_model_path(<mesh>)` -> the mesh's resolved texture/material graph.
//! The `regenerate_modt` phase consumes this file to compute a byte-exact FO4
//! `MODT` for novel converted meshes (see `src/test_fixtures/modt/README.md`).
//!
//! ## Params (JSON)
//! ```text
//! { "manifest_path": "<abs path>" }   // default: <mod_path>/debug/modt/mesh_manifest.json
//! ```
//!
//! ## Scope
//! - Textures come from the resolved material slots (README RULE 2), by slot
//!   role (RULE 4 fixes sRGB). Meshes with no external material (inline
//!   shaders) are skipped: their slot roles are unrecoverable, so
//!   `regenerate_modt` drops MODT for them (slower load, never broken).
//! - `addon_nodes = []` (static-mesh targets only).
//! - A mesh whose NIF/material fails to load or parse, or that references a
//!   material file missing on disk, is skipped whole — never a partial entry.
//!
//! Phase-contract: NO Python / GIL. Pure file walk + parse + JSON write.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;

use rayon::prelude::*;

use materials_native::bgem::BgemData;
use materials_native::bgsm::BgsmData;
use nif_core_native::model::NifFile;

use crate::fixups::harvest_modt::normalize_model_path;
use crate::modt_manifest::{
    ManifestTexture, MeshModtEntry, MeshModtManifest, OutputNifDependencies,
};
use crate::phase::{Phase, PhaseCtx, PhaseError, PhaseReport};

// ---------------------------------------------------------------------------
// Slot -> role mapping (must agree with `modt_manifest::role_is_srgb`)
// ---------------------------------------------------------------------------

/// A slot string is "populated" iff it is non-empty after trimming the BGSM/BGEM
/// empty-slot sentinel (`\0`) and whitespace.
fn nonempty(slot: &str) -> Option<String> {
    let t = slot.trim_matches('\0').trim();
    if t.is_empty() {
        None
    } else {
        Some(t.to_string())
    }
}

fn os(slot: &Option<String>) -> &str {
    slot.as_deref().unwrap_or("")
}

fn push_slot(out: &mut Vec<ManifestTexture>, slot: &str, role: &str) {
    if let Some(path) = nonempty(slot) {
        out.push(ManifestTexture {
            path,
            role: role.to_string(),
        });
    }
}

/// Named texture slots of a BGSM, in file order, with their MODT slot roles.
/// sRGB roles (per RULE 4): `diffuse`, `greyscale`, `envmap`, `glow`.
fn slot_textures_bgsm(m: &BgsmData) -> Vec<ManifestTexture> {
    let mut out = Vec::new();
    push_slot(&mut out, &m.DiffuseTexture, "diffuse");
    push_slot(&mut out, &m.NormalTexture, "normal");
    push_slot(&mut out, &m.SmoothSpecTexture, "smoothspec");
    push_slot(&mut out, &m.GreyscaleTexture, "greyscale");
    push_slot(&mut out, os(&m.EnvmapTexture), "envmap");
    push_slot(&mut out, os(&m.GlowTexture), "glow");
    push_slot(&mut out, os(&m.InnerLayerTexture), "inner");
    push_slot(&mut out, os(&m.WrinklesTexture), "wrinkle");
    push_slot(&mut out, os(&m.DisplacementTexture), "displacement");
    push_slot(&mut out, os(&m.SpecularTexture), "specular");
    push_slot(&mut out, os(&m.LightingTexture), "lighting");
    push_slot(&mut out, os(&m.FlowTexture), "flow");
    push_slot(
        &mut out,
        os(&m.DistanceFieldAlphaTexture),
        "distancefieldalpha",
    );
    out
}

/// Named texture slots of a BGEM, in file order, with their MODT slot roles.
/// sRGB roles (per RULE 4): `base`, `greyscale`, `envmap`, `glow`.
fn slot_textures_bgem(m: &BgemData) -> Vec<ManifestTexture> {
    let mut out = Vec::new();
    push_slot(&mut out, &m.BaseTexture, "base");
    push_slot(&mut out, &m.GrayscaleTexture, "greyscale");
    push_slot(&mut out, &m.EnvmapTexture, "envmap");
    push_slot(&mut out, &m.NormalTexture, "normal");
    push_slot(&mut out, &m.EnvmapMaskTexture, "envmask");
    push_slot(&mut out, os(&m.SpecularTexture), "specular");
    push_slot(&mut out, os(&m.LightingTexture), "lighting");
    push_slot(&mut out, os(&m.GlowTexture), "glow");
    out
}

/// Parse one material's loose bytes (dispatched by rel-path extension) into its
/// named texture slots. `None` for an unknown extension or a parse failure.
#[cfg(test)]
fn material_textures(rel_path: &str, bytes: &[u8]) -> Option<Vec<ManifestTexture>> {
    let lower = rel_path.to_ascii_lowercase();
    if lower.ends_with(".bgsm") {
        Some(slot_textures_bgsm(
            &materials_native::bgsm::parse(bytes).ok()?,
        ))
    } else if lower.ends_with(".bgem") {
        Some(slot_textures_bgem(
            &materials_native::bgem::parse(bytes).ok()?,
        ))
    } else {
        None
    }
}

pub(crate) struct ParsedMaterialTextures {
    pub textures: Vec<ManifestTexture>,
    pub root: Option<String>,
}

fn parse_material_textures(rel_path: &str, bytes: &[u8]) -> Option<ParsedMaterialTextures> {
    let lower = rel_path.to_ascii_lowercase();
    if lower.ends_with(".bgsm") {
        let material = materials_native::bgsm::parse(bytes).ok()?;
        Some(ParsedMaterialTextures {
            textures: slot_textures_bgsm(&material),
            root: nonempty(&material.RootMaterialPath),
        })
    } else if lower.ends_with(".bgem") {
        let material = materials_native::bgem::parse(bytes).ok()?;
        Some(ParsedMaterialTextures {
            textures: slot_textures_bgem(&material),
            root: None,
        })
    } else {
        None
    }
}

/// Build one mesh's manifest entry from its resolved material rel-paths and each
/// material's loose file bytes. `None` (skip the mesh) when:
/// - there are no external materials (inline-shader mesh — roles unrecoverable),
/// - a referenced material file was missing on disk (`loaded` under-covers), or
/// - any material fails to parse — never emit a partial (wrong) entry.
#[cfg(test)]
fn build_entry(
    material_rel_paths: &[String],
    loaded: &[(String, Vec<u8>)],
) -> Option<MeshModtEntry> {
    if material_rel_paths.is_empty() || loaded.len() != material_rel_paths.len() {
        return None;
    }
    let mut textures = Vec::new();
    for (rel, bytes) in loaded {
        textures.extend(material_textures(rel, bytes)?);
    }
    Some(MeshModtEntry {
        materials: material_rel_paths.to_vec(),
        textures,
        addon_nodes: Vec::new(),
    })
}

type CachedMaterialTextures = Arc<OnceLock<Option<ParsedMaterialTextures>>>;

#[derive(Default)]
pub(crate) struct MaterialTextureCache {
    entries: Mutex<HashMap<PathBuf, CachedMaterialTextures>>,
}

impl MaterialTextureCache {
    pub(crate) fn get(&self, path: &Path, relative_path: &str) -> CachedMaterialTextures {
        let entry = self
            .entries
            .lock()
            .unwrap()
            .entry(path.to_path_buf())
            .or_default()
            .clone();
        entry.get_or_init(|| {
            let bytes = std::fs::read(path).ok()?;
            parse_material_textures(relative_path, &bytes)
        });
        entry
    }
}

// ---------------------------------------------------------------------------
// File walk + orchestration
// ---------------------------------------------------------------------------

/// Case-insensitive single-level child lookup (handles `Meshes` vs `meshes`).
fn find_child_ci(parent: &Path, name: &str) -> Option<PathBuf> {
    let target = name.to_ascii_lowercase();
    std::fs::read_dir(parent)
        .ok()?
        .flatten()
        .find(|e| e.file_name().to_string_lossy().to_ascii_lowercase() == target)
        .map(|e| e.path())
}

/// Recursively collect every `*.nif` (case-insensitive) under `root`.
fn collect_nifs(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in rd.flatten() {
            let p = entry.path();
            if p.is_dir() {
                stack.push(p);
            } else if p
                .extension()
                .map(|e| e.eq_ignore_ascii_case("nif"))
                .unwrap_or(false)
            {
                out.push(p);
            }
        }
    }
    out
}

/// Resolve one output NIF to its `(key, entry)`; `None` skips the mesh.
#[derive(Default)]
struct MeshBuildProfile {
    nifs: u64,
    bytes_read: u64,
    dependency_candidates: u64,
    captured: u64,
    fallback: u64,
    entries: u64,
    read_ns: u128,
    lookup_ns: u128,
    hash_ns: u128,
    parse_ns: u128,
    release_ns: u128,
    material_ns: u128,
    capture_ns: u128,
}

impl MeshBuildProfile {
    fn merge(mut self, other: Self) -> Self {
        self.nifs += other.nifs;
        self.bytes_read += other.bytes_read;
        self.dependency_candidates += other.dependency_candidates;
        self.captured += other.captured;
        self.fallback += other.fallback;
        self.entries += other.entries;
        self.read_ns += other.read_ns;
        self.lookup_ns += other.lookup_ns;
        self.hash_ns += other.hash_ns;
        self.parse_ns += other.parse_ns;
        self.release_ns += other.release_ns;
        self.material_ns += other.material_ns;
        self.capture_ns += other.capture_ns;
        self
    }
}

fn build_mesh_entry_profiled(
    data_dir: &Path,
    meshes_root: &Path,
    nif_path: &Path,
    materials: &MaterialTextureCache,
    dependencies: &OutputNifDependencies,
    capture_fallbacks: Option<&OutputNifDependencies>,
) -> (Option<(String, MeshModtEntry)>, MeshBuildProfile) {
    let mut profile = MeshBuildProfile {
        nifs: 1,
        ..Default::default()
    };
    let Some(rel) = nif_path.strip_prefix(meshes_root).ok() else {
        return (None, profile);
    };
    let key = normalize_model_path(&rel.to_string_lossy());
    if key.is_empty() {
        return (None, profile);
    }
    let started = Instant::now();
    let Some(bytes) = std::fs::read(nif_path).ok() else {
        profile.read_ns += started.elapsed().as_nanos();
        return (None, profile);
    };
    profile.read_ns += started.elapsed().as_nanos();
    profile.bytes_read += bytes.len() as u64;

    let started = Instant::now();
    let captured = dependencies.lookup(nif_path);
    profile.lookup_ns += started.elapsed().as_nanos();
    profile.dependency_candidates += captured.is_some() as u64;
    let matched = if let Some(captured) = captured {
        let started = Instant::now();
        let matched = captured.digest == *blake3::hash(&bytes).as_bytes();
        profile.hash_ns += started.elapsed().as_nanos();
        matched.then_some(captured)
    } else {
        None
    };
    dependencies.record_match(matched.is_some());
    let material_rel_paths = if let Some(captured) = matched {
        profile.captured += 1;
        captured.materials.clone()
    } else {
        profile.fallback += 1;
        let started = Instant::now();
        let Some(nif) = NifFile::from_bytes(&bytes, Some(nif_path.to_path_buf())).ok() else {
            profile.parse_ns += started.elapsed().as_nanos();
            return (None, profile);
        };
        let materials = nif.referenced_asset_paths().materials;
        drop(nif);
        profile.parse_ns += started.elapsed().as_nanos();
        if let Some(captures) = capture_fallbacks {
            let started = Instant::now();
            captures.insert(
                nif_path,
                nif_core_native::convert_file::FinalNifDependencies {
                    digest: *blake3::hash(&bytes).as_bytes(),
                    materials: materials.clone(),
                },
            );
            profile.capture_ns += started.elapsed().as_nanos();
        }
        materials
    };
    let started = Instant::now();
    drop(bytes);
    profile.release_ns += started.elapsed().as_nanos();
    if material_rel_paths.is_empty() {
        return (None, profile);
    }
    let started = Instant::now();
    let mut textures = Vec::new();
    for rel in &material_rel_paths {
        let slots = materials.get(&data_dir.join(rel), rel);
        let Some(slots) = slots.get().and_then(|slots| slots.as_ref()) else {
            profile.material_ns += started.elapsed().as_nanos();
            return (None, profile);
        };
        textures.extend(slots.textures.iter().cloned());
    }
    profile.material_ns += started.elapsed().as_nanos();
    let entry = MeshModtEntry {
        materials: material_rel_paths,
        textures,
        addon_nodes: Vec::new(),
    };
    profile.entries += 1;
    (Some((key, entry)), profile)
}

#[cfg(test)]
fn build_mesh_entry(
    data_dir: &Path,
    meshes_root: &Path,
    nif_path: &Path,
    materials: &MaterialTextureCache,
    dependencies: &OutputNifDependencies,
) -> Option<(String, MeshModtEntry)> {
    build_mesh_entry_profiled(
        data_dir,
        meshes_root,
        nif_path,
        materials,
        dependencies,
        None,
    )
    .0
}

fn emit_manifest(
    mod_path: &Path,
    manifest_path: &Path,
    cancel: &AtomicBool,
    dependencies: &OutputNifDependencies,
) -> Result<u32, PhaseError> {
    let dependency_counts = dependencies.counts();
    let total_started = Instant::now();
    let data_dir = mod_path.join("data");
    let meshes_root = find_child_ci(&data_dir, "meshes");
    let collect_started = Instant::now();
    let nif_paths = meshes_root
        .as_ref()
        .map(|d| collect_nifs(d))
        .unwrap_or_default();
    let collect_ns = collect_started.elapsed().as_nanos();

    if cancel.load(Ordering::Relaxed) {
        return Err(PhaseError::Cancelled);
    }

    let materials = MaterialTextureCache::default();
    let build_started = Instant::now();
    let (mesh_rows, profile) = match meshes_root {
        Some(ref root) => nif_paths
            .par_iter()
            .map(|nif_path| {
                build_mesh_entry_profiled(&data_dir, root, nif_path, &materials, dependencies, None)
            })
            .fold(
                || (Vec::new(), MeshBuildProfile::default()),
                |(mut rows, profile), (entry, item_profile)| {
                    if let Some(entry) = entry {
                        rows.push(entry);
                    }
                    (rows, profile.merge(item_profile))
                },
            )
            .reduce(
                || (Vec::new(), MeshBuildProfile::default()),
                |(mut left_rows, left_profile), (mut right_rows, right_profile)| {
                    left_rows.append(&mut right_rows);
                    (left_rows, left_profile.merge(right_profile))
                },
            ),
        None => (Vec::new(), MeshBuildProfile::default()),
    };
    let build_ns = build_started.elapsed().as_nanos();
    let merge_started = Instant::now();
    let meshes: BTreeMap<String, MeshModtEntry> = mesh_rows.into_iter().collect();
    let merge_ns = merge_started.elapsed().as_nanos();

    let count = meshes.len() as u32;
    let manifest = MeshModtManifest { meshes };

    if let Some(parent) = manifest_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| PhaseError::Internal(format!("mkdir {}: {e}", parent.display())))?;
    }
    let serialize_started = Instant::now();
    let json = serde_json::to_string_pretty(&manifest)
        .map_err(|e| PhaseError::Internal(format!("serialize manifest: {e}")))?;
    let serialize_ns = serialize_started.elapsed().as_nanos();
    let manifest_bytes = json.len();
    let write_started = Instant::now();
    std::fs::write(manifest_path, json)
        .map_err(|e| PhaseError::Internal(format!("write {}: {e}", manifest_path.display())))?;
    let write_ns = write_started.elapsed().as_nanos();

    let counts = dependencies.counts();
    eprintln!(
        "[modt_manifest] nifs={} captured={} fallback={} entries={count}",
        nif_paths.len(),
        counts.0 - dependency_counts.0,
        counts.1 - dependency_counts.1
    );
    eprintln!(
        "[modt_manifest_profile] wall_ms={} collect_ms={} build_ms={} merge_ms={} serialize_ms={} write_ms={} read_worker_ms={} lookup_worker_ms={} hash_worker_ms={} parse_worker_ms={} release_worker_ms={} material_worker_ms={} nifs={} nif_bytes={} manifest_bytes={} dependency_candidates={} captured={} fallback={} entries={}",
        total_started.elapsed().as_millis(),
        collect_ns / 1_000_000,
        build_ns / 1_000_000,
        merge_ns / 1_000_000,
        serialize_ns / 1_000_000,
        write_ns / 1_000_000,
        profile.read_ns / 1_000_000,
        profile.lookup_ns / 1_000_000,
        profile.hash_ns / 1_000_000,
        profile.parse_ns / 1_000_000,
        profile.release_ns / 1_000_000,
        profile.material_ns / 1_000_000,
        profile.nifs,
        profile.bytes_read,
        manifest_bytes,
        profile.dependency_candidates,
        profile.captured,
        profile.fallback,
        profile.entries,
    );

    Ok(count)
}

pub struct EmitModtManifestPhase;

impl Phase for EmitModtManifestPhase {
    fn name(&self) -> &'static str {
        "emit_modt_manifest"
    }

    fn run(&self, ctx: &mut PhaseCtx<'_>) -> Result<PhaseReport, PhaseError> {
        let manifest_path = ctx
            .params
            .get("manifest_path")
            .and_then(|v| v.as_str())
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                ctx.mod_path
                    .join("debug")
                    .join("modt")
                    .join("mesh_manifest.json")
            });

        let count = emit_manifest(
            ctx.mod_path,
            &manifest_path,
            ctx.cancel,
            &ctx.run.output_nif_dependencies,
        )?;
        Ok(PhaseReport {
            records_changed: count,
            ..Default::default()
        })
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use materials_native::bgem::{self, BgemData};
    use materials_native::bgsm::{self, BgsmData};

    #[test]
    fn captured_dependencies_fall_back_after_same_size_timestamp_overwrite() {
        use nif_core_native::convert_file::FinalNifDependencies;
        use nif_core_native::model::NifValue;
        let temp = tempfile::tempdir().unwrap();
        let data = temp.path().join("data");
        let meshes = data.join("meshes");
        std::fs::create_dir_all(&meshes).unwrap();
        std::fs::create_dir(data.join("materials")).unwrap();
        for name in ["a", "b"] {
            let mut material = BgsmData::default();
            material.header.signature = bgsm::BGSM_SIGNATURE;
            material.header.version = 2;
            material.DiffuseTexture = format!("textures/{name}.dds");
            std::fs::write(
                data.join(format!("materials/{name}.bgsm")),
                bgsm::write(&material),
            )
            .unwrap();
        }
        let mut nif = NifFile::new("fo4");
        let shader = nif.add_block("BSLightingShaderProperty", None);
        nif.blocks[shader].set_field("Name", NifValue::String("materials/a.bgsm".into()));
        let bytes = nif.to_bytes().unwrap();
        let path = meshes.join("test.nif");
        std::fs::write(&path, &bytes).unwrap();
        let timestamp = path.metadata().unwrap().modified().unwrap();
        let captures = OutputNifDependencies::default();
        captures.insert(&path, FinalNifDependencies::capture(&nif, &bytes));
        let uncaptured = OutputNifDependencies::default();
        let materials = MaterialTextureCache::default();
        let expected = build_mesh_entry(&data, &meshes, &path, &materials, &uncaptured).unwrap();
        let actual = build_mesh_entry(&data, &meshes, &path, &materials, &captures).unwrap();
        assert_eq!(
            serde_json::to_value(&actual).unwrap(),
            serde_json::to_value(&expected).unwrap()
        );
        assert!(captures.matching(&path, &bytes).is_some());
        nif.blocks[shader].set_field("Name", NifValue::String("materials/b.bgsm".into()));
        let changed = nif.to_bytes().unwrap();
        assert_eq!(changed.len(), bytes.len());
        std::fs::write(&path, &changed).unwrap();
        std::fs::File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_times(std::fs::FileTimes::new().set_modified(timestamp))
            .unwrap();
        assert_eq!(path.metadata().unwrap().modified().unwrap(), timestamp);
        assert!(captures.matching(&path, &changed).is_none());
        let expected = build_mesh_entry(&data, &meshes, &path, &materials, &uncaptured).unwrap();
        let actual = build_mesh_entry(&data, &meshes, &path, &materials, &captures).unwrap();
        assert_eq!(
            serde_json::to_value(&actual).unwrap(),
            serde_json::to_value(&expected).unwrap()
        );
        assert_eq!(actual.1.materials, vec!["materials/b.bgsm"]);
        std::fs::remove_file(&path).unwrap();
        assert!(build_mesh_entry(&data, &meshes, &path, &materials, &captures).is_none());
        std::fs::write(&path, b"invalid NIF").unwrap();
        assert!(build_mesh_entry(&data, &meshes, &path, &materials, &captures).is_none());
    }

    fn roles(texs: &[ManifestTexture]) -> Vec<&str> {
        texs.iter().map(|t| t.role.as_str()).collect()
    }

    fn srgb_count(texs: &[ManifestTexture]) -> usize {
        texs.iter().filter(|t| t.is_srgb()).count()
    }

    #[test]
    fn bgsm_slot_roles_and_srgb_count() {
        let mut m = BgsmData::default();
        m.DiffuseTexture = "textures\\a_d.dds".into();
        m.NormalTexture = "textures\\a_n.dds".into();
        m.SmoothSpecTexture = "textures\\a_s.dds".into();
        m.GreyscaleTexture = "textures\\a_g.dds".into();
        m.EnvmapTexture = Some("textures\\cube_e.dds".into());
        // Empty / sentinel slots must be skipped, not emitted.
        m.GlowTexture = Some("\0".into());
        m.SpecularTexture = Some(String::new());

        let texs = slot_textures_bgsm(&m);
        assert_eq!(
            roles(&texs),
            vec!["diffuse", "normal", "smoothspec", "greyscale", "envmap"]
        );
        // diffuse, greyscale, envmap are sRGB; normal, smoothspec are linear.
        assert_eq!(srgb_count(&texs), 3);
    }

    #[test]
    fn bgem_slot_roles_and_srgb_count() {
        let mut m = BgemData::default();
        m.BaseTexture = "textures\\b_d.dds".into();
        m.GrayscaleTexture = "textures\\b_g.dds".into();
        m.EnvmapTexture = "textures\\b_e.dds".into();
        m.NormalTexture = "textures\\b_n.dds".into();
        m.EnvmapMaskTexture = "textures\\b_m.dds".into();
        m.GlowTexture = Some("textures\\b_glow.dds".into());
        m.SpecularTexture = Some(String::new()); // skipped

        let texs = slot_textures_bgem(&m);
        assert_eq!(
            roles(&texs),
            vec!["base", "greyscale", "envmap", "normal", "envmask", "glow"]
        );
        // base, greyscale, envmap, glow are sRGB; normal, envmask are linear.
        assert_eq!(srgb_count(&texs), 4);
    }

    /// The per-mesh builder over in-memory material bytes (round-tripped through
    /// `bgsm::write`) — exercises extension dispatch + parse + role extraction.
    #[test]
    fn build_entry_from_in_memory_bgsm_bytes() {
        let mut m = BgsmData::default();
        m.header.signature = bgsm::BGSM_SIGNATURE;
        m.header.version = 2;
        m.DiffuseTexture = "textures\\x_d.dds".into();
        m.NormalTexture = "textures\\x_n.dds".into();
        let bytes = bgsm::write(&m);

        let rel = "materials/x.bgsm".to_string();
        let loaded = vec![(rel.clone(), bytes)];
        let entry = build_entry(&[rel.clone()], &loaded).expect("entry built");

        assert_eq!(entry.materials, vec![rel]);
        assert_eq!(roles(&entry.textures), vec!["diffuse", "normal"]);
        assert_eq!(srgb_count(&entry.textures), 1);
        assert!(entry.addon_nodes.is_empty());
    }

    #[test]
    fn build_entry_from_in_memory_bgem_bytes() {
        let mut m = BgemData::default();
        m.header.signature = bgem::BGEM_SIGNATURE;
        m.header.version = 20;
        m.BaseTexture = "textures\\y_d.dds".into();
        m.EnvmapTexture = "textures\\y_e.dds".into();
        m.NormalTexture = "textures\\y_n.dds".into();
        let bytes = bgem::write(&m);

        let rel = "materials/y.bgem".to_string();
        let entry = build_entry(&[rel.clone()], &[(rel.clone(), bytes)]).expect("entry built");

        assert_eq!(roles(&entry.textures), vec!["base", "envmap", "normal"]);
        assert_eq!(srgb_count(&entry.textures), 2); // base + envmap
    }

    #[test]
    fn build_entry_skips_when_no_materials() {
        assert!(build_entry(&[], &[]).is_none());
    }

    #[test]
    fn build_entry_skips_on_missing_material_file() {
        // A material is referenced but none loaded (file missing) → skip whole mesh.
        let refs = vec!["materials/x.bgsm".to_string()];
        assert!(build_entry(&refs, &[]).is_none());
    }

    #[test]
    fn build_entry_skips_on_parse_failure() {
        let rel = "materials/bad.bgsm".to_string();
        let loaded = vec![(rel.clone(), vec![0u8, 1, 2, 3])];
        assert!(build_entry(&[rel], &loaded).is_none());
    }

    #[test]
    fn shared_material_slots_match_uncached_parsing() {
        let tmp = tempfile::tempdir().unwrap();
        let mut material = BgsmData::default();
        material.header.signature = bgsm::BGSM_SIGNATURE;
        material.header.version = 2;
        material.DiffuseTexture = "textures/shared_d.dds".into();
        material.NormalTexture = "textures/shared_n.dds".into();
        let bytes = bgsm::write(&material);
        let path = tmp.path().join("shared.bgsm");
        std::fs::write(&path, &bytes).unwrap();
        let cache = MaterialTextureCache::default();
        let entries: Vec<_> = (0..128)
            .into_par_iter()
            .map(|_| cache.get(&path, "shared.bgsm"))
            .collect();
        let expected = serde_json::to_value(material_textures("shared.bgsm", &bytes)).unwrap();
        for entry in &entries {
            assert!(Arc::ptr_eq(&entries[0], entry));
            assert_eq!(
                serde_json::to_value(
                    entry
                        .get()
                        .unwrap()
                        .as_ref()
                        .map(|material| &material.textures)
                )
                .unwrap(),
                expected
            );
        }
    }

    #[test]
    fn material_snapshot_expires_between_manifest_runs() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("shared.bgsm");
        let first = MaterialTextureCache::default();
        assert!(first.get(&path, "shared.bgsm").get().unwrap().is_none());
        let mut material = BgsmData::default();
        material.header.signature = bgsm::BGSM_SIGNATURE;
        material.header.version = 2;
        material.DiffuseTexture = "textures/overwritten_d.dds".into();
        std::fs::write(&path, bgsm::write(&material)).unwrap();
        let second = MaterialTextureCache::default();
        let slots = second.get(&path, "shared.bgsm");
        assert_eq!(
            slots.get().unwrap().as_ref().unwrap().textures[0].path,
            "textures/overwritten_d.dds"
        );
        std::fs::write(&path, b"invalid material").unwrap();
        let third = MaterialTextureCache::default();
        assert!(third.get(&path, "shared.bgsm").get().unwrap().is_none());
    }

    #[test]
    #[ignore = "requires MODT_CORPUS_ROOT and MODT_CORPUS_REPORT"]
    fn material_cache_matches_uncached_output_corpus() {
        let mod_root = PathBuf::from(std::env::var("MODT_CORPUS_ROOT").unwrap());
        let report_path = PathBuf::from(std::env::var("MODT_CORPUS_REPORT").unwrap());
        let data_dir = mod_root.join("data");
        let root = find_child_ci(&data_dir, "meshes").unwrap();
        let paths = collect_nifs(&root);
        assert!(!paths.is_empty());
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(20)
            .build()
            .unwrap();
        let started = std::time::Instant::now();
        let dependencies = OutputNifDependencies::default();
        let uncaptured = OutputNifDependencies::default();
        let baseline_materials = MaterialTextureCache::default();
        let (baseline_rows, baseline_profile) = pool.install(|| {
            paths
                .par_iter()
                .map(|path| {
                    build_mesh_entry_profiled(
                        &data_dir,
                        &root,
                        path,
                        &baseline_materials,
                        &uncaptured,
                        Some(&dependencies),
                    )
                })
                .fold(
                    || (Vec::new(), MeshBuildProfile::default()),
                    |(mut rows, profile), (entry, item_profile)| {
                        if let Some(entry) = entry {
                            rows.push(entry);
                        }
                        (rows, profile.merge(item_profile))
                    },
                )
                .reduce(
                    || (Vec::new(), MeshBuildProfile::default()),
                    |(mut left_rows, left_profile), (mut right_rows, right_profile)| {
                        left_rows.append(&mut right_rows);
                        (left_rows, left_profile.merge(right_profile))
                    },
                )
        });
        let baseline: BTreeMap<_, _> = baseline_rows.into_iter().collect();
        let baseline_seconds = started.elapsed().as_secs_f64();
        let materials = MaterialTextureCache::default();
        let started = std::time::Instant::now();
        let (actual_rows, cached_profile) = pool.install(|| {
            paths
                .par_iter()
                .map(|path| {
                    build_mesh_entry_profiled(
                        &data_dir,
                        &root,
                        path,
                        &materials,
                        &dependencies,
                        None,
                    )
                })
                .fold(
                    || (Vec::new(), MeshBuildProfile::default()),
                    |(mut rows, profile), (entry, item_profile)| {
                        if let Some(entry) = entry {
                            rows.push(entry);
                        }
                        (rows, profile.merge(item_profile))
                    },
                )
                .reduce(
                    || (Vec::new(), MeshBuildProfile::default()),
                    |(mut left_rows, left_profile), (mut right_rows, right_profile)| {
                        left_rows.append(&mut right_rows);
                        (left_rows, left_profile.merge(right_profile))
                    },
                )
        });
        let actual: BTreeMap<_, _> = actual_rows.into_iter().collect();
        let cached_seconds = started.elapsed().as_secs_f64();
        assert_eq!(
            serde_json::to_value(&actual).unwrap(),
            serde_json::to_value(&baseline).unwrap()
        );
        let report = serde_json::json!({
            "source": mod_root,
            "nifs_checked": paths.len(),
            "manifest_entries": actual.len(),
            "material_paths_loaded": materials.entries.lock().unwrap().len(),
            "baseline_seconds": baseline_seconds,
            "cached_seconds": cached_seconds,
            "manifests_identical": true,
            "captured_hits": dependencies.counts().0,
            "fallback_parses": dependencies.counts().1,
            "baseline_profile": {
                "bytes_read": baseline_profile.bytes_read,
                "read_worker_ms": baseline_profile.read_ns / 1_000_000,
                "lookup_worker_ms": baseline_profile.lookup_ns / 1_000_000,
                "hash_worker_ms": baseline_profile.hash_ns / 1_000_000,
                "parse_worker_ms": baseline_profile.parse_ns / 1_000_000,
                "release_worker_ms": baseline_profile.release_ns / 1_000_000,
                "material_worker_ms": baseline_profile.material_ns / 1_000_000,
                "capture_worker_ms": baseline_profile.capture_ns / 1_000_000,
            },
            "cached_profile": {
                "bytes_read": cached_profile.bytes_read,
                "read_worker_ms": cached_profile.read_ns / 1_000_000,
                "lookup_worker_ms": cached_profile.lookup_ns / 1_000_000,
                "hash_worker_ms": cached_profile.hash_ns / 1_000_000,
                "parse_worker_ms": cached_profile.parse_ns / 1_000_000,
                "release_worker_ms": cached_profile.release_ns / 1_000_000,
                "material_worker_ms": cached_profile.material_ns / 1_000_000,
            },
        });
        if let Some(path) = std::env::var_os("MODT_CORPUS_MANIFEST") {
            std::fs::write(
                path,
                serde_json::to_vec(&MeshModtManifest { meshes: actual }).unwrap(),
            )
            .unwrap();
        }
        std::fs::write(report_path, serde_json::to_string_pretty(&report).unwrap()).unwrap();
        eprintln!("{report}");
    }

    #[test]
    fn emit_manifest_writes_empty_object_when_no_meshes() {
        let tmp = tempfile::tempdir().unwrap();
        let manifest_path = tmp.path().join("debug/modt/mesh_manifest.json");
        let cancel = AtomicBool::new(false);

        let count = emit_manifest(
            tmp.path(),
            &manifest_path,
            &cancel,
            &OutputNifDependencies::default(),
        )
        .unwrap();
        assert_eq!(count, 0);

        let text = std::fs::read_to_string(&manifest_path).unwrap();
        assert_eq!(text, "{}");
        // And it round-trips as a manifest.
        let back: MeshModtManifest = serde_json::from_str(&text).unwrap();
        assert!(back.is_empty());
    }
}
