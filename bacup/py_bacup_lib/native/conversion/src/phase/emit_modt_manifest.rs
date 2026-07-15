//! Phase: `emit_modt_manifest` — the MODT compute-manifest PRODUCER (Plan B).
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
//! ## v1 scope
//! - Textures come from the **resolved material slots** (RULE 2), by slot ROLE
//!   (RULE 4 fixes sRGB). Meshes with **no external material** (inline shaders)
//!   are SKIPPED — their slot roles are unrecoverable, so `regenerate_modt` drops
//!   MODT for them (slower load, never broken).
//! - `addon_nodes = []` (static-mesh targets only).
//! - A mesh whose NIF/material fails to load or parse, or that references a
//!   material file missing on disk, is skipped whole — never a partial entry.
//!
//! Phase-contract: NO Python / GIL. Pure file walk + parse + JSON write.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use rayon::prelude::*;

use materials_native::bgem::BgemData;
use materials_native::bgsm::BgsmData;
use nif_core_native::model::NifFile;

use crate::fixups::harvest_modt::normalize_model_path;
use crate::modt_manifest::{ManifestTexture, MeshModtEntry, MeshModtManifest};
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

/// Build one mesh's manifest entry from its resolved material rel-paths and each
/// material's loose file bytes. `None` (skip the mesh) when:
/// - there are no external materials (inline-shader mesh — roles unrecoverable),
/// - a referenced material file was missing on disk (`loaded` under-covers), or
/// - any material fails to parse — never emit a partial (wrong) entry.
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
fn build_mesh_entry(
    data_dir: &Path,
    meshes_root: &Path,
    nif_path: &Path,
) -> Option<(String, MeshModtEntry)> {
    let rel = nif_path.strip_prefix(meshes_root).ok()?;
    let key = normalize_model_path(&rel.to_string_lossy());
    if key.is_empty() {
        return None;
    }
    let nif = NifFile::load(nif_path).ok()?;
    let material_rel_paths = nif.referenced_asset_paths().materials;
    if material_rel_paths.is_empty() {
        return None;
    }
    let mut loaded = Vec::with_capacity(material_rel_paths.len());
    for rel in &material_rel_paths {
        if let Ok(bytes) = std::fs::read(data_dir.join(rel)) {
            loaded.push((rel.clone(), bytes));
        }
    }
    let entry = build_entry(&material_rel_paths, &loaded)?;
    Some((key, entry))
}

fn emit_manifest(
    mod_path: &Path,
    manifest_path: &Path,
    cancel: &AtomicBool,
) -> Result<u32, PhaseError> {
    let data_dir = mod_path.join("data");
    let meshes_root = find_child_ci(&data_dir, "meshes");
    let nif_paths = meshes_root
        .as_ref()
        .map(|d| collect_nifs(d))
        .unwrap_or_default();

    if cancel.load(Ordering::Relaxed) {
        return Err(PhaseError::Cancelled);
    }

    let meshes: BTreeMap<String, MeshModtEntry> = match meshes_root {
        Some(ref root) => nif_paths
            .par_iter()
            .filter_map(|nif_path| build_mesh_entry(&data_dir, root, nif_path))
            .collect(),
        None => BTreeMap::new(),
    };

    let count = meshes.len() as u32;
    let manifest = MeshModtManifest { meshes };

    if let Some(parent) = manifest_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| PhaseError::Internal(format!("mkdir {}: {e}", parent.display())))?;
    }
    let json = serde_json::to_string_pretty(&manifest)
        .map_err(|e| PhaseError::Internal(format!("serialize manifest: {e}")))?;
    std::fs::write(manifest_path, json)
        .map_err(|e| PhaseError::Internal(format!("write {}: {e}", manifest_path.display())))?;

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

        let count = emit_manifest(ctx.mod_path, &manifest_path, ctx.cancel)?;
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
    fn emit_manifest_writes_empty_object_when_no_meshes() {
        let tmp = tempfile::tempdir().unwrap();
        let manifest_path = tmp.path().join("debug/modt/mesh_manifest.json");
        let cancel = AtomicBool::new(false);

        let count = emit_manifest(tmp.path(), &manifest_path, &cancel).unwrap();
        assert_eq!(count, 0);

        let text = std::fs::read_to_string(&manifest_path).unwrap();
        assert_eq!(text, "{}");
        // And it round-trips as a manifest.
        let back: MeshModtManifest = serde_json::from_str(&text).unwrap();
        assert!(back.is_empty());
    }
}
