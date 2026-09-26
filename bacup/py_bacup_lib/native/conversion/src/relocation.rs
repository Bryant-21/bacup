use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock, RwLock};
use std::time::SystemTime;

use nif_core_native::model::{NifFile, ReferencedAssetPaths};

#[derive(Clone, Debug)]
struct NifDependencyEntry {
    len: u64,
    modified: Option<SystemTime>,
    refs: Arc<OnceLock<Option<ReferencedAssetPaths>>>,
}

#[derive(Debug, Default)]
pub struct NifDependencyCache {
    entries: RwLock<HashMap<PathBuf, NifDependencyEntry>>,
    requests: AtomicU64,
    loads: AtomicU64,
}

impl NifDependencyCache {
    fn fork(&self) -> Self {
        let entries = self
            .entries
            .read()
            .map(|entries| entries.clone())
            .unwrap_or_default();
        Self {
            entries: RwLock::new(entries),
            requests: AtomicU64::new(0),
            loads: AtomicU64::new(0),
        }
    }

    pub fn get(&self, path: &Path) -> Option<ReferencedAssetPaths> {
        let metadata = std::fs::metadata(path).ok()?;
        let modified = metadata.modified().ok();
        self.requests.fetch_add(1, Ordering::Relaxed);
        let key = if cfg!(windows) {
            PathBuf::from(path.to_string_lossy().replace('\\', "/").to_lowercase())
        } else {
            path.to_path_buf()
        };
        let matches =
            |entry: &&NifDependencyEntry| entry.len == metadata.len() && entry.modified == modified;
        let cached = self
            .entries
            .read()
            .unwrap()
            .get(&key)
            .filter(matches)
            .map(|entry| Arc::clone(&entry.refs));
        let refs = cached.unwrap_or_else(|| {
            let mut entries = self.entries.write().unwrap();
            if let Some(entry) = entries.get(&key).filter(matches) {
                return Arc::clone(&entry.refs);
            }
            let refs = Arc::new(OnceLock::new());
            entries.insert(
                key,
                NifDependencyEntry {
                    len: metadata.len(),
                    modified,
                    refs: Arc::clone(&refs),
                },
            );
            refs
        });
        refs.get_or_init(|| {
            self.loads.fetch_add(1, Ordering::Relaxed);
            NifFile::load_referenced_asset_paths(path).ok()
        })
        .clone()
    }

    pub fn counts(&self) -> (u64, u64) {
        (
            self.requests.load(Ordering::Relaxed),
            self.loads.load(Ordering::Relaxed),
        )
    }
}

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
    /// Subset of `members` relocated for geometry + collision only; their
    /// material/texture slots must stay on the shared (un-namespaced) paths.
    pub mesh_only_members: HashSet<String>,
    pub warnings: Vec<String>,
    pub nif_dependencies: Arc<NifDependencyCache>,
    pub(crate) preparation: Option<Arc<RelocationPreparation>>,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub(crate) struct RelocationPreparationKey {
    source_dir: PathBuf,
    mesh_roots: Vec<String>,
}

impl RelocationPreparationKey {
    fn new(source_dir: &Path, mesh_roots: &[String]) -> Self {
        Self {
            source_dir: source_dir.to_path_buf(),
            mesh_roots: mesh_roots.to_vec(),
        }
    }
}

#[derive(Debug)]
pub(crate) struct RelocationPreparation {
    members: HashSet<String>,
    mesh_only_members: HashSet<String>,
    warnings: Vec<String>,
    nif_dependencies: Arc<NifDependencyCache>,
}

impl RelocationPreparation {
    fn from_run_result(result: RelocationBuildResult) -> Self {
        Self {
            members: result.members,
            mesh_only_members: result.mesh_only_members,
            warnings: result.warnings,
            nif_dependencies: result.nif_dependencies,
        }
    }

    fn into_run_result(preparation: Arc<Self>) -> RelocationBuildResult {
        RelocationBuildResult {
            members: preparation.members.clone(),
            mesh_only_members: preparation.mesh_only_members.clone(),
            warnings: preparation.warnings.clone(),
            nif_dependencies: Arc::new(preparation.nif_dependencies.fork()),
            preparation: Some(preparation),
        }
    }
}

/// Public entry: collision compare + cascade closure over real files.
pub fn build_relocation_member_set(
    mesh_roots: &[String],
    fo76_dir: &Path,
    fo4_dir: &Path,
) -> RelocationBuildResult {
    let mut warnings = Vec::new();
    let nif_dependencies = Arc::new(NifDependencyCache::default());
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
    let source_refs = Arc::clone(&nif_dependencies);
    let load_source_nif = move |rel: &str| -> Option<ReferencedAssetPaths> {
        let abs = source.join(rel.replace('/', std::path::MAIN_SEPARATOR_STR));
        source_refs.get(&abs)
    };
    let target = fo4_dir.to_path_buf();
    let target_refs = Arc::clone(&nif_dependencies);
    let load_target_nif = move |rel: &str| -> Option<ReferencedAssetPaths> {
        let abs = target.join(rel.replace('/', std::path::MAIN_SEPARATOR_STR));
        target_refs.get(&abs)
    };
    let source2 = fo76_dir.to_path_buf();
    let load_mat = move |rel: &str| -> Vec<String> {
        let abs = source2.join(rel.replace('/', std::path::MAIN_SEPARATOR_STR));
        read_material_texture_paths(&abs)
    };
    let nif_loaders: [&dyn Fn(&str) -> Option<ReferencedAssetPaths>; 2] =
        [&load_source_nif, &load_target_nif];
    let related_meshes = collect_source_meshes(mesh_roots, fo76_dir);
    let mut members = build_relocation_member_set_from_loaders_with_related_nifs(
        &meshes,
        &related_meshes,
        &nif_loaders,
        &load_mat,
    );
    append_existing_forced_materials(
        &mut members,
        FO76_FO4_DEFAULT_RELOCATION_MATERIAL_PATHS,
        fo76_dir,
        &load_mat,
    );
    let mesh_only_members = add_mesh_only_members(
        &mut members,
        FO76_FO4_COLLISION_ONLY_RELOCATION_MESH_PATHS,
        fo76_dir,
    );
    RelocationBuildResult {
        members,
        mesh_only_members,
        warnings,
        nif_dependencies,
        preparation: None,
    }
}

pub fn build_relocation_member_set_with_target_store(
    mesh_roots: &[String],
    fo76_dir: &Path,
    target_store: &crate::target_assets::TargetAssetStore,
) -> RelocationBuildResult {
    let key = RelocationPreparationKey::new(fo76_dir, mesh_roots);
    let preparation = target_store.relocation_preparation(key, || {
        RelocationPreparation::from_run_result(
            build_relocation_member_set_with_target_store_uncached(
                mesh_roots,
                fo76_dir,
                target_store,
            ),
        )
    });
    RelocationPreparation::into_run_result(preparation)
}

pub(crate) fn build_relocation_member_set_with_target_store_uncached(
    mesh_roots: &[String],
    fo76_dir: &Path,
    target_store: &crate::target_assets::TargetAssetStore,
) -> RelocationBuildResult {
    let nif_dependencies = Arc::new(NifDependencyCache::default());
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
    let source_refs = Arc::clone(&nif_dependencies);
    let load_source_nif = move |rel: &str| -> Option<ReferencedAssetPaths> {
        let abs = source.join(rel.replace('/', std::path::MAIN_SEPARATOR_STR));
        source_refs.get(&abs)
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
    let mut members = build_relocation_member_set_from_loaders_with_related_nifs(
        &meshes,
        &source_meshes,
        &nif_loaders,
        &load_material,
    );
    append_existing_forced_materials(
        &mut members,
        FO76_FO4_DEFAULT_RELOCATION_MATERIAL_PATHS,
        fo76_dir,
        &load_material,
    );
    let mesh_only_members = add_mesh_only_members(
        &mut members,
        FO76_FO4_COLLISION_ONLY_RELOCATION_MESH_PATHS,
        fo76_dir,
    );
    RelocationBuildResult {
        members,
        mesh_only_members,
        warnings: target_store.warnings().to_vec(),
        nif_dependencies,
        preparation: None,
    }
}

pub fn extend_with_changed_decal_assets(
    members: &mut HashSet<String>,
    decal_materials: &[String],
    source_dir: &Path,
    target_dir: &Path,
) -> Vec<String> {
    extend_with_changed_decal_assets_inner(members, decal_materials, source_dir, |member| {
        let path = target_dir.join(member.replace('/', std::path::MAIN_SEPARATOR_STR));
        Ok(path.is_file().then_some(path))
    })
}

pub fn extend_with_changed_decal_assets_from_target_store(
    members: &mut HashSet<String>,
    decal_materials: &[String],
    source_dir: &Path,
    target_store: &crate::target_assets::TargetAssetStore,
) -> Vec<String> {
    extend_with_changed_decal_assets_inner(members, decal_materials, source_dir, |member| {
        target_store.materialize(member)
    })
}

fn extend_with_changed_decal_assets_inner(
    members: &mut HashSet<String>,
    decal_materials: &[String],
    source_dir: &Path,
    mut target_path: impl FnMut(&str) -> Result<Option<std::path::PathBuf>, String>,
) -> Vec<String> {
    let mut warnings = Vec::new();
    for material in decal_materials {
        let material = normalize_material_member(material);
        if material.is_empty() {
            continue;
        }
        let source_material = source_dir.join(material.replace('/', std::path::MAIN_SEPARATOR_STR));
        if !source_material.is_file() {
            continue;
        }

        let target_material = match target_path(&material) {
            Ok(path) => path,
            Err(error) => {
                warnings.push(format!("relocation: compare {material}: {error}"));
                continue;
            }
        };
        let material_differs = target_material
            .as_deref()
            .is_some_and(|target| !files_are_identical(&source_material, target));

        let mut changed_textures = Vec::new();
        for texture in read_material_texture_paths(&source_material) {
            let source_texture =
                source_dir.join(texture.replace('/', std::path::MAIN_SEPARATOR_STR));
            if !source_texture.is_file() {
                continue;
            }
            match target_path(&texture) {
                Ok(Some(target_texture))
                    if !files_are_identical(&source_texture, &target_texture) =>
                {
                    changed_textures.push(texture);
                }
                Ok(_) => {}
                Err(error) => {
                    warnings.push(format!("relocation: compare {texture}: {error}"));
                }
            }
        }

        if target_material.is_some() && (material_differs || !changed_textures.is_empty()) {
            members.insert(material);
        }
        members.extend(changed_textures);
    }
    warnings
}

fn files_are_identical(left: &Path, right: &Path) -> bool {
    let Ok(left_metadata) = std::fs::metadata(left) else {
        return false;
    };
    let Ok(right_metadata) = std::fs::metadata(right) else {
        return false;
    };
    if left_metadata.len() != right_metadata.len() {
        return false;
    }
    match (std::fs::read(left), std::fs::read(right)) {
        (Ok(left), Ok(right)) => left == right,
        _ => false,
    }
}

/// Default landscape collision roots when pair config leaves them empty.
pub const FO76_FO4_DEFAULT_RELOCATION_MESH_ROOTS: &[&str] = &["meshes/landscape"];
pub const SKYRIMSE_FO4_DEFAULT_RELOCATION_MESH_ROOTS: &[&str] = &["meshes/landscape"];

/// Specific FO76→FO4 same-path assets that must relocate even outside the broad
/// landscape collision root.
pub const FO76_FO4_DEFAULT_RELOCATION_MESH_PATHS: &[&str] = &[
    "meshes/architecture/buildings/hightech/lobby/hitextintwalltoptrimblong01.nif",
    "meshes/dlc06/setdressing/vaultworkshop/dlc06vaultsafetyposter07.nif",
    "meshes/dlc06/setdressing/vaultworkshop/dlc06vaultsafetyposter11.nif",
    "meshes/effects/fxbitsleavesfromtreetops.nif",
    "meshes/effects/fxbitsleaveswind01.nif",
    "meshes/interiors/redrocket/redr_smbldg01_pumpmeterfree01.nif",
    "meshes/interiors/redrocket/redr_smbldg01_pumpmeterfree02.nif",
    // FO76's Newspaper01 is the Charleston Herald at FO4's Boston Bugle path.
    "meshes/props/newspaper02.nif",
    "meshes/setdressing/acducts/acductmed2way02.nif",
    // FO76 and FO4 share this mesh/material path but use different poster atlases.
    "meshes/setdressing/signage/advertsposter03.nif",
    // Shares `materials/setdressing/minutemen/flagminutemen01.bgsm` with
    // flagwallminutemen01 below. Relocating only the wall namespaced the shared
    // material (and every converted MSWP keyed on it) while this mesh kept
    // colliding with FO4 base, so the base flagpole loaded and its swap could
    // never match — see `flagpole_shares_relocated_material_with_flagwall`.
    "meshes/setdressing/minutemen/flagpoleminutemen02.nif",
    "meshes/setdressing/minutemen/flagwallminutemen01.nif",
    "meshes/setdressing/metalbarrel/metalbarrel01staticfiregrating.nif",
    "meshes/vehicles/automotive/busschool01empty.nif",
];

/// FO76→FO4 same-path meshes whose FO4 copy has no collision object while
/// FO76's does, so falling through to the FO4 mesh leaves them walk-through.
/// They relocate for geometry + collision only: their material/texture paths
/// stay shared, so the look is unchanged. Kept in sync with the extracted data
/// by `collision_only_relocation_list_matches_real_data`.
pub const FO76_FO4_COLLISION_ONLY_RELOCATION_MESH_PATHS: &[&str] = &[
    "meshes/animobjects/holotape01.nif",
    "meshes/architecture/buildings/decokit/decomaina1x1windroofend01.nif",
    "meshes/architecture/buildings/hightech/extensions/hitextextacorneb01.nif",
    "meshes/architecture/buildings/hightech/extensions/hitextextacornebshort01.nif",
    "meshes/architecture/buildings/hightech/extensions/hitextextacornebwindowa01.nif",
    "meshes/architecture/buildings/hightech/lobby/hitextintlobbyatop02.nif",
    "meshes/architecture/buildings/hightech/lobby/hitextintlobbyatopcornera01.nif",
    "meshes/architecture/buildings/hightech/lobby/hitextintlobbyatopcornerb01.nif",
    "meshes/architecture/buildings/hightech/lobby/hitextintlobbyatoplong02.nif",
    "meshes/architecture/buildings/hightech/lobby/hitextlobbyalongbottomplate01.nif",
    "meshes/architecture/buildings/hightech/lobby/hitextlobbyrailingundecornerin02.nif",
    "meshes/architecture/buildings/hightech/lobby/hitextlobbyrailingunderlong01.nif",
    "meshes/architecture/buildings/hightech/skin/hitextacaplongtop01.nif",
    "meshes/architecture/buildings/hightech/skin/hitextacornercwindowe01.nif",
    "meshes/architecture/buildings/hightech/skin/hitextawallcornerd01.nif",
    "meshes/architecture/buildings/hightech/skin/hitextawallinshort01.nif",
    "meshes/architecture/buildings/hightech/skin/hitextawallinshortcorner01.nif",
    "meshes/architecture/buildings/hightech/skin/hitextawallinshorthalf01.nif",
    "meshes/architecture/buildings/hightech/skin/hitextawallinshortlong01.nif",
    "meshes/architecture/buildings/hightech/skin/hitextawalltrimend01.nif",
    "meshes/architecture/buildings/hightech/skin/hitextawindowa01intwin.nif",
    "meshes/architecture/buildings/hightech/skin/hitextawindowatalllong01.nif",
    "meshes/architecture/buildings/hightech/skin/hitextawindowbtalllong02.nif",
    "meshes/architecture/buildings/hightech/skin/hitextawindowc01intwin.nif",
    "meshes/architecture/buildings/hightech/skin/hitextbcornerdtall01.nif",
    "meshes/architecture/buildings/hightech/skin/hitextcwindowa01.nif",
    "meshes/architecture/buildings/residential/res01moderndormer01.nif",
    "meshes/architecture/buildings/residential/res01moderndormer01rr.nif",
    "meshes/architecture/ecdowns/ecdstandsunder01.nif",
    "meshes/architecture/parkinggarage/pgarageouttrim1x1cor01.nif",
    "meshes/architecture/parkinggarage/pgarageouttrim1x1str01.nif",
    "meshes/architecture/parkinggarage/pgarageouttrim1x2str01.nif",
    "meshes/architecture/parkinggarage/pgarageouttrim2x2cor01.nif",
    "meshes/interiors/building/brick/big_wallkit/bldbrickbgwallscktimid01.nif",
    "meshes/interiors/building/brick/med_wallkit/bldbrickmdwallbothole09.nif",
    "meshes/interiors/building/concrete/big_wallkit/bldconcbgwallscktimid01.nif",
    "meshes/interiors/building/concrete/med_wallkit/bldconcmdwallbothole09.nif",
    "meshes/interiors/building/concrete/small_floorceilkit/bldconcsmflrplatholepipe01.nif",
    "meshes/interiors/building/deco/big_wallkit/blddecobgwallscktimid01.nif",
    "meshes/interiors/building/deco/med_wallkit/blddecomdwallbothole09.nif",
    "meshes/interiors/building/wlp/med_wallkit/bldwlpmdwallcorner04.nif",
    "meshes/interiors/building/woodb/big_wallkit/bldwoodbbgwallscktimid01.nif",
    "meshes/interiors/building/woodp/med_wallkit/bldwoodpmdwallmusmuralpillar02.nif",
    "meshes/interiors/concrete/brick/stadium/concbrckbgwall01.nif",
    "meshes/interiors/hightech/lgrooms/hitlgrmbotwallcurve01.nif",
    "meshes/interiors/hightech/lgrooms/hitlgrmmid02corout02.nif",
    "meshes/interiors/hightech/lgrooms/hitlgrmmid02corout03.nif",
    "meshes/interiors/hightech/lgrooms/hitlgrmmid04corout04.nif",
    "meshes/interiors/hightech/lgrooms/hitlgrmtopcorout02.nif",
    "meshes/interiors/hightech/lgrooms/hitlgrmtopcorout03.nif",
    "meshes/interiors/hightech/lgrooms/hitlgrmtopcorout04.nif",
    "meshes/interiors/industrial/bldshellout/indbldshelloutcutbase01.nif",
    "meshes/interiors/industrial/catwalks/indcatcap01.nif",
    "meshes/interiors/industrial/framecat/indframecat256x256emptyshapethick02.nif",
    "meshes/interiors/redrocket/redrextrooftrim2way02.nif",
    "meshes/interiors/vault/catwalk/vltcatwalkrailsidehalfendr01.nif",
    "meshes/interiors/vault/ceilings/vltceilingarchendmid01.nif",
    "meshes/interiors/vault/ceilings/vltceilingarchhalfr01.nif",
    "meshes/interiors/vault/ceilings/vltceilingtrim01.nif",
    "meshes/interiors/vault/freewall/vltcorinfree01.nif",
    "meshes/interiors/vault/hallres/vlthallresdoortalltop01.nif",
    "meshes/interiors/vault/setdressing/vltac01.nif",
    "meshes/interiors/vault/setdressing/vltcoolantpipes01.nif",
    "meshes/loadscreenart/creatureradstag.nif",
    "meshes/props/hightech/hightechlight01.nif",
    "meshes/props/hightech/hightechlight01broken.nif",
    "meshes/props/hightech/hightechlight02.nif",
    "meshes/props/hightech/hightechlight02broken.nif",
    "meshes/props/hightech/hightechpendum01.nif",
    "meshes/props/hightech/hightechpendum01broken.nif",
    "meshes/props/hightech/hightechpendumrod01.nif",
    "meshes/props/hightech/hightechpendumrodcap01.nif",
    "meshes/props/lab/animmicroscope.nif",
    "meshes/setdressing/acducts/acductlgsplit01.nif",
    "meshes/setdressing/acducts/acductmedend03.nif",
    "meshes/setdressing/corpses/corpsemolerat01.nif",
    "meshes/setdressing/corpses/corpseyaoguai01.nif",
    "meshes/setdressing/flag/flagpolenoflag.nif",
    "meshes/setdressing/greebs/diamondroofgreebpower01.nif",
    "meshes/setdressing/greebs/diamondroofgreebpower02.nif",
    "meshes/setdressing/greebs/diamondroofgreebpower03.nif",
    "meshes/setdressing/greebs/diamondroofgreebpower04.nif",
    "meshes/setdressing/greebs/diamondroofgreebpower05.nif",
    "meshes/setdressing/lightfixtures/flourescentlightbulboff.nif",
    "meshes/setdressing/lightfixtures/lightfixture02on.nif",
    "meshes/setdressing/loadscreens/radiotower01ls.nif",
    "meshes/setdressing/metalfurniture/metalshelf01_broken04.nif",
    "meshes/setdressing/playerhouse_ruin/playerhouse_ruin_curtainrod01.nif",
    "meshes/setdressing/playerhouse_ruin/playerhouse_ruin_curtainrod02.nif",
    "meshes/setdressing/playerhouse_ruin/playerhouse_ruin_curtainrod03.nif",
    "meshes/setdressing/playerhouse_ruin/playerhouse_ruin_curtainrod04.nif",
    "meshes/setdressing/rubble/extrubble_bricks_clump01.nif",
    "meshes/setdressing/streetlamps/coloniallampwall01.nif",
    "meshes/setdressing/utilitypoles/utilitywallmount01.nif",
    "meshes/setdressing/utilitypoles/utilitywallmount02.nif",
];

/// Data-relative subtrees where FO76's remastered assets deliberately overwrite
/// FO4's at the shared path, rather than being skipped as base-owned. Unlike
/// relocation (which namespaces under `FO76\` and only affects converted FO76
/// records), an entry here changes how the kit looks everywhere in the game —
/// so only list kits FO76 genuinely re-authored and that map cleanly onto the
/// FO4 paths.
///
/// `weapons/handmade` is FO4's pipe-weapon kit (Pipe Pistol/Rifle/Bolt-Action/
/// Revolver plus the Handmade Rifle). All 29 FO76 materials there collide 1:1
/// with FO4 paths, and every texture they reference either ships already as an
/// FO76-unique path or collides inside this same subtree.
pub const FO76_FO4_DEFAULT_BASE_OVERWRITE_PREFIXES: &[&str] =
    &["materials/weapons/handmade/", "textures/weapons/handmade/"];

pub const FO76_FO4_DEFAULT_RELOCATION_MATERIAL_PATHS: &[&str] = &[
    "materials/interiors/vault/vltconduitsvents01.bgsm",
    "materials/interiors/vault/vltutilpipes01.bgsm",
    "materials/landscape/roads/roadrailings01.bgsm",
];

/// Returns the meshes newly added; a mesh the closure already relocated in full
/// stays a full member.
fn add_mesh_only_members(
    members: &mut HashSet<String>,
    mesh_only_meshes: &[&str],
    fo76_dir: &Path,
) -> HashSet<String> {
    let mut added = HashSet::new();
    for mesh in mesh_only_meshes {
        let key = normalize_rel(mesh);
        let source = fo76_dir.join(key.replace('/', std::path::MAIN_SEPARATOR_STR));
        if source.is_file() && members.insert(key.clone()) {
            added.insert(key);
        }
    }
    added
}

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

fn append_existing_forced_materials(
    members: &mut HashSet<String>,
    forced_materials: &[&str],
    fo76_dir: &Path,
    load_material_textures: &dyn Fn(&str) -> Vec<String>,
) {
    let mut material_queue = Vec::new();
    for material in forced_materials {
        let key = normalize_material_member(material);
        if key.is_empty() || !members.insert(key.clone()) {
            continue;
        }
        let source = fo76_dir.join(key.replace('/', std::path::MAIN_SEPARATOR_STR));
        if source.is_file() {
            material_queue.push(key);
        } else {
            members.remove(&key);
        }
    }
    close_material_texture_members(members, &mut material_queue, load_material_textures);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dependency_metadata_is_shared_and_refreshes_after_source_overwrite() {
        use indexmap::IndexMap;
        use nif_core_native::model::NifValue;
        use rayon::prelude::*;
        let source = tempfile::tempdir().unwrap();
        let target = tempfile::tempdir().unwrap();
        let source_path = source.path().join("mesh.nif");
        let target_path = target.path().join("mesh.nif");
        let write = |path: &Path, material: &str| {
            let mut nif = NifFile::new("fo76");
            let mut fields = IndexMap::new();
            fields.insert("Name".into(), NifValue::String(material.into()));
            nif.add_block("BSLightingShaderProperty", Some(fields));
            nif.save(Some(path.to_path_buf())).unwrap();
        };
        write(&source_path, "Materials/Source.bgsm");
        write(&target_path, "Materials/Target.bgsm");
        let cache = NifDependencyCache::default();
        (0..32).into_par_iter().for_each(|_| {
            assert_eq!(
                cache.get(&source_path).unwrap().materials,
                ["materials/source.bgsm"]
            );
        });
        assert_eq!(cache.counts(), (32, 1));
        assert_eq!(
            cache.get(&target_path).unwrap().materials,
            ["materials/target.bgsm"]
        );
        assert_eq!(cache.counts(), (33, 2));
        write(&source_path, "Materials/UpdatedSourceLongerName.bgsm");
        assert_eq!(
            cache.get(&source_path).unwrap().materials,
            ["materials/updatedsourcelongername.bgsm"]
        );
        assert_eq!(cache.counts(), (34, 3));
    }

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
    fn forced_default_meshes_and_materials_are_members_only_when_present_in_source() {
        use materials_native::bgsm;

        let tmp = std::env::temp_dir().join("reloc_forced_defaults");
        let _ = std::fs::remove_dir_all(&tmp);
        let fo76 = tmp.join("fo76");
        let fo4 = tmp.join("fo4");
        std::fs::create_dir_all(&fo76).unwrap();
        std::fs::create_dir_all(&fo4).unwrap();
        let forced_meshes = [
            "meshes/architecture/buildings/hightech/lobby/hitextintwalltoptrimblong01.nif",
            "meshes/dlc06/setdressing/vaultworkshop/dlc06vaultsafetyposter07.nif",
            "meshes/dlc06/setdressing/vaultworkshop/dlc06vaultsafetyposter11.nif",
            "meshes/effects/fxbitsleavesfromtreetops.nif",
            "meshes/effects/fxbitsleaveswind01.nif",
            "meshes/interiors/redrocket/redr_smbldg01_pumpmeterfree01.nif",
            "meshes/interiors/redrocket/redr_smbldg01_pumpmeterfree02.nif",
            "meshes/props/newspaper02.nif",
            "meshes/setdressing/acducts/acductmed2way02.nif",
            "meshes/setdressing/signage/advertsposter03.nif",
            "meshes/setdressing/minutemen/flagpoleminutemen02.nif",
            "meshes/setdressing/minutemen/flagwallminutemen01.nif",
            "meshes/setdressing/metalbarrel/metalbarrel01staticfiregrating.nif",
            "meshes/vehicles/automotive/busschool01empty.nif",
        ];

        let empty = build_relocation_member_set(&["meshes/landscape".to_string()], &fo76, &fo4);
        for mesh in forced_meshes {
            assert!(!empty.members.contains(mesh), "absent source: {mesh}");
        }

        for mesh in forced_meshes {
            touch(&fo76.join(mesh));
        }
        for material in FO76_FO4_DEFAULT_RELOCATION_MATERIAL_PATHS {
            let mut data = bgsm::BgsmData::default();
            data.header.signature = bgsm::BGSM_SIGNATURE;
            data.header.version = 22;
            let stem = Path::new(material)
                .file_stem()
                .and_then(|value| value.to_str())
                .unwrap();
            data.DiffuseTexture = format!("Textures\\Forced\\{stem}_d.dds");
            data.NormalTexture = format!("Textures\\Forced\\{stem}_n.dds");
            data.SpecularTexture = Some(format!("Textures\\Forced\\{stem}_r.dds"));
            data.LightingTexture = Some(format!("Textures\\Forced\\{stem}_l.dds"));
            let path = fo76.join(material.replace('/', std::path::MAIN_SEPARATOR_STR));
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, bgsm::write(&data)).unwrap();
        }

        let result = build_relocation_member_set(&["meshes/landscape".to_string()], &fo76, &fo4);

        for mesh in forced_meshes {
            assert!(result.members.contains(mesh), "{mesh}");
        }
        for material in FO76_FO4_DEFAULT_RELOCATION_MATERIAL_PATHS {
            assert!(result.members.contains(*material));
            let stem = Path::new(material)
                .file_stem()
                .and_then(|value| value.to_str())
                .unwrap()
                .to_ascii_lowercase();
            for suffix in ["d", "n", "r", "l"] {
                assert!(
                    result
                        .members
                        .contains(&format!("textures/forced/{stem}_{suffix}.dds")),
                    "missing source dependency for {material}: {suffix}"
                );
            }
        }
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
    fn decal_assets_relocate_only_when_bytes_changed() {
        use materials_native::bgsm;

        let tmp = std::env::temp_dir().join("reloc_decal_assets");
        let _ = std::fs::remove_dir_all(&tmp);
        let source = tmp.join("source");
        let target = tmp.join("target");
        let write = |root: &Path, rel: &str, bytes: &[u8]| {
            let path = root.join(rel.replace('/', std::path::MAIN_SEPARATOR_STR));
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, bytes).unwrap();
        };
        let material_bytes = |diffuse: &str, normal: &str| {
            let mut data = bgsm::BgsmData::default();
            data.header.signature = bgsm::BGSM_SIGNATURE;
            data.header.version = 20;
            data.DiffuseTexture = diffuse.to_string();
            data.NormalTexture = normal.to_string();
            bgsm::write(&data)
        };

        let changed = "materials/dlc04/decals/parking.bgsm";
        let changed_diffuse = "textures/dlc04/decals/parking_d.dds";
        let changed_normal = "textures/dlc04/decals/parking_n.dds";
        let exact = "materials/decals/exact.bgsm";
        let exact_diffuse = "textures/decals/exact_d.dds";
        for root in [&source, &target] {
            write(
                root,
                changed,
                &material_bytes(changed_diffuse, changed_normal),
            );
            write(root, changed_normal, b"x");
            write(root, exact, &material_bytes(exact_diffuse, ""));
            write(root, exact_diffuse, b"same");
        }
        write(&source, changed_diffuse, b"fo76");
        write(&target, changed_diffuse, b"fo4");

        let mut members = HashSet::new();
        let warnings = extend_with_changed_decal_assets(
            &mut members,
            &[changed.to_string(), exact.to_string()],
            &source,
            &target,
        );

        assert!(warnings.is_empty());
        assert_eq!(
            members,
            HashSet::from([changed.to_string(), changed_diffuse.to_string()])
        );
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn relocation_paths_normalize_data_prefixes_and_insert_namespace_after_root() {
        assert_eq!(
            insert_namespace_after_root("textures/landscape/rock01_d.dds", "FO76"),
            "textures/FO76/landscape/rock01_d.dds"
        );
        assert_eq!(
            insert_namespace_after_root("meshes/landscape/rock01.nif", "FO76"),
            "meshes/FO76/landscape/rock01.nif"
        );
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
        let meshes = vec![
            "meshes/landscape/rock01.nif".to_string(),
            "meshes/interiors/redrocket/redr_smbldg01_pumpmeterfree01.nif".to_string(),
        ];

        let load_nif = |rel: &str| -> Option<ReferencedAssetPaths> {
            match rel {
                "meshes/landscape/rock01.nif" => Some(ReferencedAssetPaths {
                    textures: vec!["textures/landscape/rock01_d.dds".to_string()],
                    materials: vec!["materials/landscape/rock01.bgsm".to_string()],
                }),
                "meshes/interiors/redrocket/redr_smbldg01_pumpmeterfree01.nif" => {
                    Some(ReferencedAssetPaths {
                        textures: Vec::new(),
                        materials: vec![
                            "Materials\\Interiors\\RedRocket\\RedRPumps01.BGSM".to_string(),
                        ],
                    })
                }
                other => panic!("unexpected NIF lookup: {other}"),
            }
        };
        let load_mat = |rel: &str| -> Vec<String> {
            match rel {
                "materials/landscape/rock01.bgsm" => {
                    vec!["textures/landscape/rock01_n.dds".to_string()]
                }
                "materials/interiors/redrocket/redrpumps01.bgsm" => vec![
                    "Interiors/RedRocket/RedR_09_Pumps_01_d.dds".to_string(),
                    "Interiors/RedRocket/RedR_09_Pumps_01_n.dds".to_string(),
                    "Interiors/RedRocket/RedR_09_Pumps_01_r.dds".to_string(),
                    "Interiors/RedRocket/RedR_09_Pumps_01_l.dds".to_string(),
                ],
                other => panic!("unexpected material lookup: {other}"),
            }
        };

        let members = build_relocation_member_set_inner(&meshes, &load_nif, &load_mat);

        let mut got: Vec<_> = members.into_iter().collect();
        got.sort();
        assert_eq!(
            got,
            vec![
                "materials/interiors/redrocket/redrpumps01.bgsm".to_string(),
                "materials/landscape/rock01.bgsm".to_string(),
                "meshes/interiors/redrocket/redr_smbldg01_pumpmeterfree01.nif".to_string(),
                "meshes/landscape/rock01.nif".to_string(),
                "textures/interiors/redrocket/redr_09_pumps_01_d.dds".to_string(),
                "textures/interiors/redrocket/redr_09_pumps_01_l.dds".to_string(),
                "textures/interiors/redrocket/redr_09_pumps_01_n.dds".to_string(),
                "textures/interiors/redrocket/redr_09_pumps_01_r.dds".to_string(),
                "textures/landscape/rock01_d.dds".to_string(),
                "textures/landscape/rock01_n.dds".to_string(),
            ]
        );
    }

    #[test]
    fn build_member_set_merges_source_and_target_nif_deps_from_files() {
        use indexmap::IndexMap;
        use materials_native::bgsm;
        use nif_core_native::model::{NifValue, ReferencedAssetPaths};

        let tmp = tempfile::tempdir().unwrap();
        let fo76 = tmp.path().join("fo76");
        let fo4 = tmp.path().join("fo4");
        let mesh = "meshes/landscape/dirtcliffs/terrainshelfrocks01.nif";
        let native =
            |root: &Path, rel: &str| root.join(rel.replace('/', std::path::MAIN_SEPARATOR_STR));
        for (root, material) in [
            (&fo76, "Materials/Landscape/Ground/ForestRocks01Decal.bgsm"),
            (&fo4, "Materials/Landscape/Ground/RootsEroded01Decal.bgsm"),
        ] {
            let path = native(root, mesh);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            let mut nif = NifFile::new("fo76");
            let mut fields = IndexMap::new();
            fields.insert("Name".into(), NifValue::String(material.into()));
            nif.add_block("BSLightingShaderProperty", Some(fields));
            nif.save(Some(path)).unwrap();
        }
        for (material, stem) in [
            (
                "materials/landscape/ground/forestrocks01decal.bgsm",
                "ForestRocks01",
            ),
            (
                "materials/landscape/ground/rootseroded01decal.bgsm",
                "RootsEroded01",
            ),
        ] {
            let mut data = bgsm::BgsmData::default();
            data.header.signature = bgsm::BGSM_SIGNATURE;
            data.header.version = 20;
            data.DiffuseTexture = format!("Landscape\\Ground\\{stem}_d.dds");
            data.NormalTexture = format!("Landscape\\Ground\\{stem}_n.dds");
            let path = native(&fo76, material);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, bgsm::write(&data)).unwrap();
        }

        let load_source_nif = |rel: &str| -> Option<ReferencedAssetPaths> {
            NifFile::load(native(&fo76, rel))
                .ok()
                .map(|nif| nif.referenced_asset_paths())
        };
        let load_target_nif = |rel: &str| -> Option<ReferencedAssetPaths> {
            NifFile::load(native(&fo4, rel))
                .ok()
                .map(|nif| nif.referenced_asset_paths())
        };
        let load_mat = |rel: &str| read_material_texture_paths(&native(&fo76, rel));
        let nif_loaders: [&dyn Fn(&str) -> Option<ReferencedAssetPaths>; 2] =
            [&load_source_nif, &load_target_nif];

        let members =
            build_relocation_member_set_from_loaders(&[mesh.to_string()], &nif_loaders, &load_mat);

        for member in [
            mesh,
            "materials/landscape/ground/forestrocks01decal.bgsm",
            "materials/landscape/ground/rootseroded01decal.bgsm",
            "textures/landscape/ground/forestrocks01_d.dds",
            "textures/landscape/ground/forestrocks01_n.dds",
            "textures/landscape/ground/rootseroded01_d.dds",
            "textures/landscape/ground/rootseroded01_n.dds",
        ] {
            assert!(members.contains(member), "{member}: {members:?}");
        }
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
    fn collision_only_mesh_relocates_without_pulling_its_materials() {
        use indexmap::IndexMap;
        use nif_core_native::model::NifValue;
        let tmp = tempfile::tempdir().unwrap();
        let fo76 = tmp.path().join("fo76");
        let fo4 = tmp.path().join("fo4");
        std::fs::create_dir_all(&fo4).unwrap();
        let mesh = "meshes/interiors/hightech/lgrooms/hitlgrmbotwallcurve01.nif";
        let mesh_path = fo76.join(mesh);
        std::fs::create_dir_all(mesh_path.parent().unwrap()).unwrap();
        let mut nif = NifFile::new("fo76");
        let mut fields = IndexMap::new();
        fields.insert(
            "Name".into(),
            NifValue::String("Materials/Interiors/HighTech/FlatMetalSupports01a.bgsm".into()),
        );
        nif.add_block("BSLightingShaderProperty", Some(fields));
        nif.save(Some(mesh_path)).unwrap();

        let result = build_relocation_member_set(&["meshes/landscape".to_string()], &fo76, &fo4);

        assert!(result.members.contains(mesh));
        assert!(result.mesh_only_members.contains(mesh));
        assert!(
            !result
                .members
                .contains("materials/interiors/hightech/flatmetalsupports01a.bgsm"),
            "a collision-only mesh must not drag its material into the FO76 namespace"
        );
    }
}
