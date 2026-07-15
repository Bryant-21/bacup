use std::collections::HashSet;
use std::path::Path;

use nif_core_native::model::{NifFile, ReferencedAssetPaths};

/// Normalize any asset path to a lowercase, forward-slash, data-relative key.
pub fn normalize_rel(path: &str) -> String {
    let p = path.trim().trim_matches('\0').replace('\\', "/");
    let p = p.trim_start_matches('/').to_ascii_lowercase();
    if let Some((_, rest)) = p.split_once("/data/") {
        rest.to_string()
    } else {
        p.strip_prefix("data/").map(str::to_string).unwrap_or(p)
    }
}

fn normalize_texture_member(path: &str) -> String {
    let n = normalize_rel(path);
    if n.is_empty() || n.starts_with("textures/") {
        n
    } else {
        format!("textures/{n}")
    }
}

fn normalize_material_member(path: &str) -> String {
    let n = normalize_rel(path);
    if n.is_empty() || n.starts_with("materials/") {
        n
    } else {
        format!("materials/{n}")
    }
}

/// Walk each configured mesh root under `fo76_dir`; return normalized rel-paths
/// whose identical rel-path also exists under `fo4_dir`. Case-insensitive.
pub fn collect_colliding_meshes(
    mesh_roots: &[String],
    fo76_dir: &Path,
    fo4_dir: &Path,
) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for root in mesh_roots {
        let root_norm = normalize_rel(root);
        let fo76_root = fo76_dir.join(root_norm.replace('/', std::path::MAIN_SEPARATOR_STR));
        if !fo76_root.is_dir() {
            continue;
        }
        for entry in walkdir(&fo76_root) {
            if !entry.is_file() {
                continue;
            }
            let Ok(rel) = entry.strip_prefix(fo76_dir) else {
                continue;
            };
            let key = normalize_rel(&rel.to_string_lossy());
            if !key.ends_with(".nif") || seen.contains(&key) {
                continue;
            }
            // FO4 twin at the same rel-path (case-insensitive via lowercased key).
            let fo4_twin = fo4_dir.join(key.replace('/', std::path::MAIN_SEPARATOR_STR));
            if fo4_twin.is_file() {
                seen.insert(key.clone());
                out.push(key);
            }
        }
    }
    out
}

fn collect_source_meshes(mesh_roots: &[String], fo76_dir: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for root in mesh_roots {
        let root_norm = normalize_rel(root);
        let fo76_root = fo76_dir.join(root_norm.replace('/', std::path::MAIN_SEPARATOR_STR));
        if !fo76_root.is_dir() {
            continue;
        }
        for entry in walkdir(&fo76_root) {
            if !entry.is_file() {
                continue;
            }
            let Ok(rel) = entry.strip_prefix(fo76_dir) else {
                continue;
            };
            let key = normalize_rel(&rel.to_string_lossy());
            if key.ends_with(".nif") && seen.insert(key.clone()) {
                out.push(key);
            }
        }
    }
    out
}

/// Minimal recursive file walk (avoids adding a walkdir dependency if absent;
/// if the crate already depends on `walkdir`, replace with `walkdir::WalkDir`).
fn walkdir(root: &Path) -> Vec<std::path::PathBuf> {
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
            } else {
                out.push(p);
            }
        }
    }
    out
}

/// Read a BGSM/BGEM file; return its non-empty texture rel-paths (normalized).
/// Unparseable/missing files yield an empty vec (logged by the caller).
pub fn read_material_texture_paths(material_abs: &Path) -> Vec<String> {
    let Ok(bytes) = std::fs::read(material_abs) else {
        return Vec::new();
    };
    let ext = material_abs
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    // Collect raw (owned) texture strings, then normalize + dedup.
    let mut raw: Vec<String> = Vec::new();
    if ext == "bgem" {
        if let Ok(d) = materials_native::bgem::parse(&bytes) {
            raw.push(d.BaseTexture);
            raw.push(d.GrayscaleTexture);
            raw.push(d.EnvmapTexture);
            raw.push(d.NormalTexture);
            raw.push(d.EnvmapMaskTexture);
            raw.extend(
                [d.SpecularTexture, d.LightingTexture, d.GlowTexture]
                    .into_iter()
                    .flatten(),
            );
        }
    } else if let Ok(d) = materials_native::bgsm::parse(&bytes) {
        raw.push(d.DiffuseTexture);
        raw.push(d.NormalTexture);
        raw.push(d.SmoothSpecTexture);
        raw.push(d.GreyscaleTexture);
        raw.extend(
            [
                d.EnvmapTexture,
                d.GlowTexture,
                d.SpecularTexture,
                d.LightingTexture,
            ]
            .into_iter()
            .flatten(),
        );
    }
    let mut out: Vec<String> = Vec::new();
    for s in raw {
        let n = normalize_texture_member(&s);
        if !n.is_empty() && !out.contains(&n) {
            out.push(n);
        }
    }
    out
}

/// Insert `<namespace>/` after the top-level data-root segment, preserving the
/// original casing of the remainder.
pub fn insert_namespace_after_root(rel_path: &str, namespace: &str) -> String {
    let p = rel_path.replace('\\', "/");
    let p = p.trim_start_matches('/');
    match p.split_once('/') {
        Some((root, rest)) => format!("{root}/{namespace}/{rest}"),
        None => format!("{namespace}/{p}"),
    }
}

/// Resolve a phase entry's `source_path` (absolute under `source_dir`, or already
/// data-relative) to a normalized relocation-member key for set membership tests.
pub fn member_key_for_source_path(source_path: &str, source_dir: &Path) -> String {
    let rel = match Path::new(source_path).strip_prefix(source_dir) {
        Ok(r) => r.to_string_lossy().to_string(),
        Err(_) => source_path.to_string(),
    };
    normalize_rel(&rel)
}

/// Orchestration with injected loaders (testable without disk NIFs).
/// Returns the full member set: colliding meshes + every texture/material they
/// reach — the NIF's own texture slots and external materials, plus one hop into
/// each material to pick up the textures it references.
pub fn build_relocation_member_set_inner(
    colliding_meshes: &[String],
    load_nif_deps: &dyn Fn(&str) -> Option<ReferencedAssetPaths>,
    load_material_textures: &dyn Fn(&str) -> Vec<String>,
) -> HashSet<String> {
    build_relocation_member_set_from_loaders(
        colliding_meshes,
        &[load_nif_deps],
        load_material_textures,
    )
}

pub fn build_relocation_member_set_from_loaders(
    colliding_meshes: &[String],
    load_nif_deps: &[&dyn Fn(&str) -> Option<ReferencedAssetPaths>],
    load_material_textures: &dyn Fn(&str) -> Vec<String>,
) -> HashSet<String> {
    build_relocation_member_set_from_loaders_with_related_nifs(
        colliding_meshes,
        &[],
        load_nif_deps,
        load_material_textures,
    )
}

fn build_relocation_member_set_from_loaders_with_related_nifs(
    colliding_meshes: &[String],
    related_meshes: &[String],
    load_nif_deps: &[&dyn Fn(&str) -> Option<ReferencedAssetPaths>],
    load_material_textures: &dyn Fn(&str) -> Vec<String>,
) -> HashSet<String> {
    let mut members: HashSet<String> = HashSet::new();
    let mut material_queue: Vec<String> = Vec::new();
    let mut materials_seen: HashSet<String> = HashSet::new();

    for mesh in colliding_meshes {
        let key = normalize_rel(mesh);
        if !members.insert(key.clone()) {
            continue;
        }
        for loader in load_nif_deps {
            if let Some(refs) = loader(&key) {
                for t in refs.textures {
                    let tk = normalize_texture_member(&t);
                    if !tk.is_empty() {
                        members.insert(tk);
                    }
                }
                for m in refs.materials {
                    let mk = normalize_material_member(&m);
                    if mk.is_empty() {
                        continue;
                    }
                    members.insert(mk.clone());
                    if materials_seen.insert(mk.clone()) {
                        material_queue.push(mk);
                    }
                }
            }
        }
    }
    close_material_texture_members(&mut members, &mut material_queue, load_material_textures);
    expand_members_with_related_nifs(
        &mut members,
        related_meshes,
        load_nif_deps,
        load_material_textures,
    );
    members
}

fn close_material_texture_members(
    members: &mut HashSet<String>,
    material_queue: &mut Vec<String>,
    load_material_textures: &dyn Fn(&str) -> Vec<String>,
) {
    while let Some(mat) = material_queue.pop() {
        let texture_source_mat = material_texture_source_member(&mat);
        for t in load_material_textures(&texture_source_mat) {
            let tk = normalize_texture_member(&t);
            if !tk.is_empty() {
                members.insert(tk);
            }
        }
    }
}

fn expand_members_with_related_nifs(
    members: &mut HashSet<String>,
    related_meshes: &[String],
    load_nif_deps: &[&dyn Fn(&str) -> Option<ReferencedAssetPaths>],
    load_material_textures: &dyn Fn(&str) -> Vec<String>,
) {
    let mut cached_refs: Vec<(Vec<String>, Vec<String>)> = Vec::new();
    for mesh in related_meshes {
        let mut textures = Vec::new();
        let mut materials = Vec::new();
        for loader in load_nif_deps {
            if let Some(refs) = loader(mesh) {
                textures.extend(
                    refs.textures
                        .into_iter()
                        .map(|t| normalize_texture_member(&t)),
                );
                materials.extend(
                    refs.materials
                        .into_iter()
                        .map(|m| normalize_material_member(&m)),
                );
            }
        }
        if !textures.is_empty() || !materials.is_empty() {
            cached_refs.push((textures, materials));
        }
    }

    loop {
        let mut changed = false;
        let mut material_queue = Vec::new();
        for (textures, materials) in &cached_refs {
            let touches_relocated_member = textures.iter().any(|t| members.contains(t))
                || materials.iter().any(|m| members.contains(m));
            if !touches_relocated_member {
                continue;
            }
            for texture in textures {
                if !texture.is_empty() && members.insert(texture.clone()) {
                    changed = true;
                }
            }
            for material in materials {
                if material.is_empty() {
                    continue;
                }
                if members.insert(material.clone()) {
                    changed = true;
                    material_queue.push(material.clone());
                }
            }
        }
        close_material_texture_members(members, &mut material_queue, load_material_textures);
        if !changed {
            break;
        }
    }
}

fn material_texture_source_member(material_member: &str) -> String {
    crate::material_source_overrides::material_source_overrides()
        .get(material_member)
        .cloned()
        .unwrap_or_else(|| material_member.to_owned())
}

#[derive(Debug, Default)]
pub struct RelocationBuildResult {
    pub members: HashSet<String>,
    pub warnings: Vec<String>,
}

/// Public entry: collision compare + cascade closure over real files.
pub fn build_relocation_member_set(
    mesh_roots: &[String],
    fo76_dir: &Path,
    fo4_dir: &Path,
) -> RelocationBuildResult {
    let mut warnings = Vec::new();
    let any_root_present = mesh_roots.iter().any(|r| {
        fo4_dir
            .join(normalize_rel(r).replace('/', std::path::MAIN_SEPARATOR_STR))
            .is_dir()
    });
    if !any_root_present {
        warnings.push(format!(
            "relocation: no configured mesh root present under FO4 extracted dir {} — \
             collision detection disabled; FO76 landscape may clobber FO4 base. \
             Extract FO4 meshes or set FO4_EXTRACTED_DIR.",
            fo4_dir.display()
        ));
    }
    let mut meshes = collect_colliding_meshes(mesh_roots, fo76_dir, fo4_dir);
    append_existing_forced_meshes(
        &mut meshes,
        FO76_FO4_DEFAULT_RELOCATION_MESH_PATHS,
        fo76_dir,
    );
    let source = fo76_dir.to_path_buf();
    let load_source_nif = move |rel: &str| -> Option<ReferencedAssetPaths> {
        let abs = source.join(rel.replace('/', std::path::MAIN_SEPARATOR_STR));
        NifFile::load(abs).ok().map(|n| n.referenced_asset_paths())
    };
    let target = fo4_dir.to_path_buf();
    let load_target_nif = move |rel: &str| -> Option<ReferencedAssetPaths> {
        let abs = target.join(rel.replace('/', std::path::MAIN_SEPARATOR_STR));
        NifFile::load(abs).ok().map(|n| n.referenced_asset_paths())
    };
    let source2 = fo76_dir.to_path_buf();
    let load_mat = move |rel: &str| -> Vec<String> {
        let abs = source2.join(rel.replace('/', std::path::MAIN_SEPARATOR_STR));
        read_material_texture_paths(&abs)
    };
    let nif_loaders: [&dyn Fn(&str) -> Option<ReferencedAssetPaths>; 2] =
        [&load_source_nif, &load_target_nif];
    let related_meshes = collect_source_meshes(mesh_roots, fo76_dir);
    let members = build_relocation_member_set_from_loaders_with_related_nifs(
        &meshes,
        &related_meshes,
        &nif_loaders,
        &load_mat,
    );
    RelocationBuildResult { members, warnings }
}

pub fn build_relocation_member_set_with_target_store(
    mesh_roots: &[String],
    fo76_dir: &Path,
    target_store: &crate::target_assets::TargetAssetStore,
) -> RelocationBuildResult {
    let source_meshes = collect_source_meshes(mesh_roots, fo76_dir);
    let mut meshes: Vec<String> = source_meshes
        .iter()
        .filter(|mesh| target_store.has_asset(mesh))
        .cloned()
        .collect();
    append_existing_forced_meshes(
        &mut meshes,
        FO76_FO4_DEFAULT_RELOCATION_MESH_PATHS,
        fo76_dir,
    );

    let source = fo76_dir.to_path_buf();
    let load_source_nif = move |rel: &str| -> Option<ReferencedAssetPaths> {
        let abs = source.join(rel.replace('/', std::path::MAIN_SEPARATOR_STR));
        NifFile::load(abs)
            .ok()
            .map(|nif| nif.referenced_asset_paths())
    };
    let load_target_nif = |rel: &str| -> Option<ReferencedAssetPaths> {
        if !target_store.has_asset(rel) {
            return None;
        }
        let mut refs = ReferencedAssetPaths::default();
        for dependency in target_store.dependencies_for(rel) {
            if dependency.starts_with("materials/") {
                refs.materials.push(dependency.clone());
                refs.textures.extend(
                    target_store
                        .dependencies_for(dependency)
                        .iter()
                        .filter(|path| path.starts_with("textures/"))
                        .cloned(),
                );
            } else if dependency.starts_with("textures/") {
                refs.textures.push(dependency.clone());
            }
        }
        Some(refs)
    };
    let source_materials = fo76_dir.to_path_buf();
    let load_material = move |rel: &str| -> Vec<String> {
        let abs = source_materials.join(rel.replace('/', std::path::MAIN_SEPARATOR_STR));
        read_material_texture_paths(&abs)
    };
    let nif_loaders: [&dyn Fn(&str) -> Option<ReferencedAssetPaths>; 2] =
        [&load_source_nif, &load_target_nif];
    let members = build_relocation_member_set_from_loaders_with_related_nifs(
        &meshes,
        &source_meshes,
        &nif_loaders,
        &load_material,
    );
    RelocationBuildResult {
        members,
        warnings: target_store.warnings().to_vec(),
    }
}

/// Default mesh roots for FO76→FO4 when config leaves them empty.
pub const FO76_FO4_DEFAULT_RELOCATION_MESH_ROOTS: &[&str] = &["meshes/landscape"];

/// Specific FO76→FO4 same-path assets that must relocate even outside the broad
/// landscape collision root.
pub const FO76_FO4_DEFAULT_RELOCATION_MESH_PATHS: &[&str] = &[
    "meshes/architecture/buildings/hightech/lobby/hitextintwalltoptrimblong01.nif",
    "meshes/setdressing/minutemen/flagwallminutemen01.nif",
    "meshes/setdressing/metalbarrel/metalbarrel01staticfiregrating.nif",
];

fn append_existing_forced_meshes(
    meshes: &mut Vec<String>,
    forced_meshes: &[&str],
    fo76_dir: &Path,
) {
    let mut seen: HashSet<String> = meshes.iter().cloned().collect();
    for mesh in forced_meshes {
        let key = normalize_rel(mesh);
        if key.is_empty() || !key.ends_with(".nif") || !seen.insert(key.clone()) {
            continue;
        }
        let source = fo76_dir.join(key.replace('/', std::path::MAIN_SEPARATOR_STR));
        if source.is_file() {
            meshes.push(key);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn touch(path: &Path) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, b"x").unwrap();
    }

    #[test]
    fn colliding_mesh_is_member_unique_mesh_is_not() {
        let tmp = std::env::temp_dir().join("reloc_collision_compare");
        let _ = std::fs::remove_dir_all(&tmp);
        let fo76 = tmp.join("fo76");
        let fo4 = tmp.join("fo4");
        // collides
        touch(&fo76.join("meshes/landscape/rocks/Rock01.nif"));
        touch(&fo4.join("Meshes/Landscape/Rocks/rock01.nif"));
        // fo76-unique (no fo4 twin)
        touch(&fo76.join("meshes/landscape/rocks/UniqueRock.nif"));
        // outside the configured root -> ignored even though it collides
        touch(&fo76.join("meshes/clutter/can01.nif"));
        touch(&fo4.join("meshes/clutter/can01.nif"));

        let members = collect_colliding_meshes(&["meshes/landscape".to_string()], &fo76, &fo4);

        let set: HashSet<String> = members.into_iter().collect();
        assert!(set.contains("meshes/landscape/rocks/rock01.nif"));
        assert!(!set.contains("meshes/landscape/rocks/uniquerock.nif"));
        assert!(!set.contains("meshes/clutter/can01.nif"));
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn forced_default_mesh_is_member_outside_configured_roots() {
        let tmp = std::env::temp_dir().join("reloc_forced_mesh");
        let _ = std::fs::remove_dir_all(&tmp);
        let fo76 = tmp.join("fo76");
        let fo4 = tmp.join("fo4");
        std::fs::create_dir_all(&fo4).unwrap();
        touch(
            &fo76.join(
                "meshes/architecture/buildings/hightech/lobby/hitextintwalltoptrimblong01.nif",
            ),
        );
        touch(&fo76.join("meshes/setdressing/minutemen/flagwallminutemen01.nif"));
        touch(&fo76.join("meshes/setdressing/metalbarrel/metalbarrel01staticfiregrating.nif"));

        let result = build_relocation_member_set(&["meshes/landscape".to_string()], &fo76, &fo4);

        assert!(result.members.contains(
            "meshes/architecture/buildings/hightech/lobby/hitextintwalltoptrimblong01.nif"
        ));
        assert!(
            result
                .members
                .contains("meshes/setdressing/minutemen/flagwallminutemen01.nif")
        );
        assert!(
            result
                .members
                .contains("meshes/setdressing/metalbarrel/metalbarrel01staticfiregrating.nif")
        );
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn forced_default_mesh_missing_from_source_is_not_member() {
        let tmp = std::env::temp_dir().join("reloc_forced_mesh_missing");
        let _ = std::fs::remove_dir_all(&tmp);
        let fo76 = tmp.join("fo76");
        let fo4 = tmp.join("fo4");
        std::fs::create_dir_all(&fo76).unwrap();
        std::fs::create_dir_all(&fo4).unwrap();

        let result = build_relocation_member_set(&["meshes/landscape".to_string()], &fo76, &fo4);

        assert!(
            !result
                .members
                .contains("meshes/setdressing/minutemen/flagwallminutemen01.nif")
        );
        assert!(
            !result
                .members
                .contains("meshes/setdressing/metalbarrel/metalbarrel01staticfiregrating.nif")
        );
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn read_material_texture_paths_reads_bgsm_diffuse_and_normal() {
        use materials_native::bgsm;
        let mut data = bgsm::BgsmData::default();
        data.header.signature = bgsm::BGSM_SIGNATURE;
        data.header.version = 20;
        data.DiffuseTexture =
            "C:\\Projects\\76\\Build\\PC\\Data\\Textures\\Landscape\\Rock01_d.dds".to_string();
        data.NormalTexture = "Textures\\Landscape\\Rock01_n.dds".to_string();
        let bytes = bgsm::write(&data);

        let tmp = std::env::temp_dir().join("reloc_mat_read");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        let path = tmp.join("rock01.bgsm");
        std::fs::write(&path, &bytes).unwrap();

        let mut got = read_material_texture_paths(&path);
        got.sort();
        assert_eq!(
            got,
            vec![
                "textures/landscape/rock01_d.dds".to_string(),
                "textures/landscape/rock01_n.dds".to_string(),
            ]
        );
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn insert_namespace_after_root_inserts_after_top_segment() {
        assert_eq!(
            insert_namespace_after_root("textures/landscape/rock01_d.dds", "FO76"),
            "textures/FO76/landscape/rock01_d.dds"
        );
        assert_eq!(
            insert_namespace_after_root("meshes/landscape/rock01.nif", "FO76"),
            "meshes/FO76/landscape/rock01.nif"
        );
    }

    #[test]
    fn normalize_rel_strips_absolute_data_prefix() {
        assert_eq!(
            normalize_rel("C:\\Projects\\76\\Build\\PC\\Data\\Materials\\Landscape\\Rock01.bgsm"),
            "materials/landscape/rock01.bgsm"
        );
        assert_eq!(
            normalize_rel("Data\\Textures\\Landscape\\Rock01_d.dds"),
            "textures/landscape/rock01_d.dds"
        );
    }

    #[test]
    fn build_member_set_inner_closes_over_nif_and_material_deps() {
        use nif_core_native::model::ReferencedAssetPaths;
        let meshes = vec!["meshes/landscape/rock01.nif".to_string()];

        let load_nif = |rel: &str| -> Option<ReferencedAssetPaths> {
            assert_eq!(rel, "meshes/landscape/rock01.nif");
            Some(ReferencedAssetPaths {
                textures: vec!["textures/landscape/rock01_d.dds".to_string()],
                materials: vec!["materials/landscape/rock01.bgsm".to_string()],
            })
        };
        let load_mat = |rel: &str| -> Vec<String> {
            assert_eq!(rel, "materials/landscape/rock01.bgsm");
            vec!["textures/landscape/rock01_n.dds".to_string()]
        };

        let members = build_relocation_member_set_inner(&meshes, &load_nif, &load_mat);

        let mut got: Vec<_> = members.into_iter().collect();
        got.sort();
        assert_eq!(
            got,
            vec![
                "materials/landscape/rock01.bgsm".to_string(),
                "meshes/landscape/rock01.nif".to_string(),
                "textures/landscape/rock01_d.dds".to_string(),
                "textures/landscape/rock01_n.dds".to_string(),
            ]
        );
    }

    #[test]
    fn build_member_set_merges_source_and_target_nif_deps() {
        use nif_core_native::model::ReferencedAssetPaths;
        let meshes = vec!["meshes/landscape/dirtcliffs/terrainshelfrocks01.nif".to_string()];

        let load_source_nif = |rel: &str| -> Option<ReferencedAssetPaths> {
            assert_eq!(rel, "meshes/landscape/dirtcliffs/terrainshelfrocks01.nif");
            Some(ReferencedAssetPaths {
                textures: vec!["textures/landscape/ground/temp_groundtexture01_d.dds".to_string()],
                materials: vec!["materials/landscape/ground/forestrocks01decal.bgsm".to_string()],
            })
        };
        let load_target_nif = |rel: &str| -> Option<ReferencedAssetPaths> {
            assert_eq!(rel, "meshes/landscape/dirtcliffs/terrainshelfrocks01.nif");
            Some(ReferencedAssetPaths {
                textures: Vec::new(),
                materials: vec!["materials/landscape/ground/rootseroded01decal.bgsm".to_string()],
            })
        };
        let load_mat = |rel: &str| -> Vec<String> {
            match rel {
                "materials/landscape/ground/rootseroded01decal.bgsm" => {
                    vec!["Landscape\\Ground\\RootsEroded01_d.dds".to_string()]
                }
                _ => Vec::new(),
            }
        };
        let nif_loaders: [&dyn Fn(&str) -> Option<ReferencedAssetPaths>; 2] =
            [&load_source_nif, &load_target_nif];

        let members = build_relocation_member_set_from_loaders(&meshes, &nif_loaders, &load_mat);

        assert!(members.contains("materials/landscape/ground/forestrocks01decal.bgsm"));
        assert!(members.contains("materials/landscape/ground/rootseroded01decal.bgsm"));
        assert!(members.contains("textures/landscape/ground/temp_groundtexture01_d.dds"));
        assert!(members.contains("textures/landscape/ground/rootseroded01_d.dds"));
    }

    #[test]
    fn related_unique_nif_expands_sibling_material_family() {
        use nif_core_native::model::ReferencedAssetPaths;
        let meshes = vec!["meshes/landscape/trees/stump01.nif".to_string()];
        let related = vec!["meshes/landscape/trees/treeforest03.nif".to_string()];

        let load_nif = |rel: &str| -> Option<ReferencedAssetPaths> {
            match rel {
                "meshes/landscape/trees/stump01.nif" => Some(ReferencedAssetPaths {
                    textures: Vec::new(),
                    materials: vec!["materials/landscape/trees/treeforestbare.bgsm".to_string()],
                }),
                "meshes/landscape/trees/treeforest03.nif" => Some(ReferencedAssetPaths {
                    textures: Vec::new(),
                    materials: vec![
                        "Materials\\Landscape\\Trees\\TreeForestBare.BGSM".to_string(),
                        "Materials\\Landscape\\Trees\\TreeForestBark.BGSM".to_string(),
                        "Materials\\Landscape\\Trees\\TreeForestLimbs.BGSM".to_string(),
                    ],
                }),
                other => panic!("unexpected NIF lookup: {other}"),
            }
        };
        let load_mat = |rel: &str| -> Vec<String> {
            match rel {
                "materials/landscape/trees/treeforestbare.bgsm" => {
                    vec!["Landscape/Trees/TreeForestBare_d.dds".to_string()]
                }
                "materials/landscape/trees/treeforestbark.bgsm" => {
                    vec!["Landscape/Trees/TreeForestBark_d.dds".to_string()]
                }
                "materials/landscape/trees/treeforestlimbs.bgsm" => {
                    vec!["Landscape/Trees/TreeForestLimbs_d.dds".to_string()]
                }
                other => panic!("unexpected material lookup: {other}"),
            }
        };
        let nif_loaders: [&dyn Fn(&str) -> Option<ReferencedAssetPaths>; 1] = [&load_nif];

        let members = build_relocation_member_set_from_loaders_with_related_nifs(
            &meshes,
            &related,
            &nif_loaders,
            &load_mat,
        );

        assert!(members.contains("materials/landscape/trees/treeforestbare.bgsm"));
        assert!(members.contains("materials/landscape/trees/treeforestbark.bgsm"));
        assert!(members.contains("materials/landscape/trees/treeforestlimbs.bgsm"));
        assert!(members.contains("textures/landscape/trees/treeforestbare_d.dds"));
        assert!(members.contains("textures/landscape/trees/treeforestbark_d.dds"));
        assert!(members.contains("textures/landscape/trees/treeforestlimbs_d.dds"));
        assert!(!members.contains("meshes/landscape/trees/treeforest03.nif"));
    }

    #[test]
    fn material_source_override_closes_over_replacement_textures() {
        use nif_core_native::model::ReferencedAssetPaths;
        let meshes = vec!["meshes/landscape/dirtcliffs/sinkholeclifflg01.nif".to_string()];

        let load_nif = |rel: &str| -> Option<ReferencedAssetPaths> {
            assert_eq!(rel, "meshes/landscape/dirtcliffs/sinkholeclifflg01.nif");
            Some(ReferencedAssetPaths {
                textures: Vec::new(),
                materials: vec![
                    "C:\\Projects\\76\\Build\\PC\\Data\\Materials\\Landscape\\Ground\\TEMP_GroundTexture01.bgsm"
                        .to_string(),
                ],
            })
        };
        let load_mat = |rel: &str| -> Vec<String> {
            match rel {
                "materials/landscape/ground/forestrocks01.bgsm" => vec![
                    "Landscape/Ground/ForestRocks01_d.dds".to_string(),
                    "Landscape/Ground/ForestRocks01_n.dds".to_string(),
                    "Landscape/Ground/ForestRocks01_r.dds".to_string(),
                    "Landscape/Ground/ForestRocks01_l.dds".to_string(),
                ],
                "materials/landscape/ground/temp_groundtexture01.bgsm" => {
                    panic!("TEMP material should use its source override for texture deps")
                }
                other => panic!("unexpected material dependency lookup: {other}"),
            }
        };

        let members = build_relocation_member_set_inner(&meshes, &load_nif, &load_mat);

        assert!(members.contains("materials/landscape/ground/temp_groundtexture01.bgsm"));
        assert!(members.contains("textures/landscape/ground/forestrocks01_d.dds"));
        assert!(members.contains("textures/landscape/ground/forestrocks01_n.dds"));
        assert!(members.contains("textures/landscape/ground/forestrocks01_r.dds"));
        assert!(members.contains("textures/landscape/ground/forestrocks01_l.dds"));
        assert!(!members.contains("textures/landscape/ground/temp_groundtexture01_d.dds"));
    }

    #[test]
    fn terrain_shelf_rocks_real_data_closes_over_sister_bgsm_textures() {
        let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(4)
            .expect("conversion crate is repo/bacup/py_bacup_lib/native/conversion")
            .to_path_buf();
        let fo76 = repo_root.join("extracted").join("fo76");
        let fo4 = repo_root.join("extracted").join("fo4");
        let mesh = "meshes/landscape/dirtcliffs/terrainshelfrocks01.nif";
        let sister_mat = "materials/landscape/ground/rootseroded01decal.bgsm";
        if !fo76
            .join(mesh.replace('/', std::path::MAIN_SEPARATOR_STR))
            .is_file()
            || !fo4
                .join(mesh.replace('/', std::path::MAIN_SEPARATOR_STR))
                .is_file()
            || !fo76
                .join(sister_mat.replace('/', std::path::MAIN_SEPARATOR_STR))
                .is_file()
        {
            eprintln!("skip: TerrainShelfRocks01 real-data probes absent");
            return;
        }

        let load_source_nif = |rel: &str| -> Option<ReferencedAssetPaths> {
            let abs = fo76.join(rel.replace('/', std::path::MAIN_SEPARATOR_STR));
            NifFile::load(abs).ok().map(|n| n.referenced_asset_paths())
        };
        let load_target_nif = |rel: &str| -> Option<ReferencedAssetPaths> {
            let abs = fo4.join(rel.replace('/', std::path::MAIN_SEPARATOR_STR));
            NifFile::load(abs).ok().map(|n| n.referenced_asset_paths())
        };
        let load_mat = |rel: &str| -> Vec<String> {
            let abs = fo76.join(rel.replace('/', std::path::MAIN_SEPARATOR_STR));
            read_material_texture_paths(&abs)
        };
        let nif_loaders: [&dyn Fn(&str) -> Option<ReferencedAssetPaths>; 2] =
            [&load_source_nif, &load_target_nif];

        let members =
            build_relocation_member_set_from_loaders(&[mesh.to_string()], &nif_loaders, &load_mat);

        assert!(members.contains(sister_mat));
        assert!(members.contains("textures/landscape/ground/rootseroded01_d.dds"));
        assert!(members.contains("textures/landscape/ground/rootseroded01_n.dds"));
        assert!(members.contains("textures/landscape/ground/rootseroded01_r.dds"));
    }

    /// Real-data validation: over the repo's extracted FO76 + FO4 dirs, a
    /// landscape mesh that also exists in FO4 is a relocation member, while a
    /// FO76-unique landscape mesh is NOT. Skips (returns) when the extracted
    /// dirs are absent so CI without game data stays green.
    #[test]
    fn collision_compare_relocates_only_colliding_landscape_on_real_data() {
        let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(4)
            .expect("conversion crate is repo/bacup/py_bacup_lib/native/conversion")
            .to_path_buf();
        let fo76 = repo_root.join("extracted").join("fo76");
        let fo4 = repo_root.join("extracted").join("fo4");
        if !fo76.join("meshes").join("landscape").is_dir() || !fo4.join("meshes").is_dir() {
            eprintln!(
                "skip: extracted FO76/FO4 dirs absent ({})",
                repo_root.display()
            );
            return;
        }

        let members = collect_colliding_meshes(&["meshes/landscape".to_string()], &fo76, &fo4);
        let set: HashSet<String> = members.into_iter().collect();

        // A landscape NIF present in BOTH games -> relocates.
        let colliding = "meshes/landscape/caveentrance/caveentr02.nif";
        // A FO76-only (Atlantic City) landscape NIF -> stays put.
        let fo76_unique = "meshes/landscape/ac_beach/beachfloor1024mid02.nif";

        // Only assert on probes that actually exist on this machine's extract.
        if fo76
            .join("meshes/landscape/caveentrance/caveentr02.nif")
            .is_file()
            && fo4
                .join("meshes/landscape/caveentrance/caveentr02.nif")
                .is_file()
        {
            assert!(
                set.contains(colliding),
                "expected colliding landscape NIF to be a relocation member"
            );
        }
        if fo76
            .join("meshes/landscape/ac_beach/beachfloor1024mid02.nif")
            .is_file()
        {
            assert!(
                !set.contains(fo76_unique),
                "FO76-unique landscape NIF must NOT be a relocation member"
            );
        }

        // Sanity: the set is a strict subset of FO76 landscape meshes, never
        // the whole tree.
        assert!(
            set.iter().all(|m| m.starts_with("meshes/landscape/")),
            "collision compare must only yield configured-root meshes"
        );
    }
}
