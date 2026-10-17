//! Materials conversion drives the existing materials_native converter with
//! the relocation union, forced Materials/FO76/... subpaths, and required
//! FO4 BGSM defaults.
//! BGSM/BGEM byte invariants ("\0" empty strings, v22→v2 scale defaults) come
//! from the shared writers — reused, never reimplemented.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use materials_native::convert::{
    ConvertMaterialsReport, ConvertMaterialsRequest, Game, MaterialEntry, RootMaterialRelocation,
    downgrade_bgem, downgrade_bgsm, repair_missing_fo76_smoothspec_from_specular,
    run_convert_materials_with_inventory,
};

pub struct MaterialsEngineParams {
    pub mod_path: PathBuf,
    pub source_extracted: PathBuf,
    pub source_inventory: Option<std::sync::Arc<crate::source_inventory::SourceAssetInventory>>,
    pub target_extracted: Option<PathBuf>,
    pub target_data_dir: Option<PathBuf>,
    pub source_game: Game,
    pub target_game: Game,
    pub materials: Vec<MaterialEntry>,
    pub convert_all: bool,
    pub pbr_carry: bool,
    pub relocation_members: HashSet<String>,
    pub namespace: String,
    pub source_materialsdb: Option<PathBuf>,
    pub overwrite_existing: bool,
    pub target_asset_paths: HashSet<String>,
    /// Data-relative prefixes whose materials deliberately overwrite the target
    /// game's at the same path instead of being skipped as base-owned.
    pub base_overwrite_prefixes: Vec<String>,
    /// Exact paths that overwrite the target game's (`ConversionRun::native_overwrite_paths`).
    pub base_overwrite_paths: HashSet<String>,
    /// Native assets mode: FO76 materials ship byte for byte (FO4CS reads them).
    pub copy_as_authored: bool,
}

pub fn run_materials_engine(params: MaterialsEngineParams) -> ConvertMaterialsReport {
    let inventory_materials = params
        .convert_all
        .then(|| {
            params.source_inventory.as_deref().and_then(|inventory| {
                enumerate_source_materials_from_inventory(
                    inventory,
                    &params.source_extracted,
                    &["Materials"],
                    params.source_game == Game::Starfield && params.target_game == Game::Fo4,
                )
            })
        })
        .flatten();
    let mut request = ConvertMaterialsRequest {
        materials: params.materials,
        source_game: Some(params.source_game),
        target_game: Some(params.target_game),
        asset_prefix: "fo76".to_string(), // accepted-for-compat; output unprefixed
        source_materialsdb: params.source_materialsdb,
        overwrite_existing: params.overwrite_existing,
        bgsm_default_overrides: vec![("bCastShadows".to_string(), serde_json::json!(true))],
        convert_all: params.convert_all,
        pbr_carry: params.pbr_carry,
        source_path_overrides: crate::material_source_overrides::material_source_overrides()
            .clone(),
        target_asset_paths: params.target_asset_paths,
        base_overwrite_prefixes: params.base_overwrite_prefixes,
        base_overwrite_paths: params.base_overwrite_paths,
        copy_as_authored: params.copy_as_authored,
        root_material_relocation: None,
    };

    apply_relocation_to_material_request(
        &mut request,
        &params.source_extracted,
        &params.relocation_members,
        &params.namespace,
        params.source_inventory.as_deref(),
    );
    relocate_authored_templates(&mut request, &params.source_extracted, &params.namespace);

    // Source-path overrides are applied inside run_convert_materials so the
    // convert_all enumeration honors them too (request.source_path_overrides).

    let enumerated_source_materials = inventory_materials.map(|mut materials| {
        materials.sort_by(|left, right| {
            left.source_path
                .to_lowercase()
                .cmp(&right.source_path.to_lowercase())
        });
        materials
    });
    run_convert_materials_with_inventory(
        &params.mod_path,
        &request,
        params.source_game,
        params.target_game,
        &params.source_extracted,
        params.target_extracted.as_deref(),
        params.target_data_dir.as_deref(),
        enumerated_source_materials,
    )
}

/// Materials copied as authored keep their RootMaterialPath, and a source template written at
/// its own path (`Materials/Template/...`) would replace the target game's same-named template
/// for every target material that roots on it. The source's templates move under the
/// namespace, and every copied material rooted on one is pointed at the moved copy.
fn relocate_authored_templates(
    request: &mut ConvertMaterialsRequest,
    source_dir: &Path,
    namespace: &str,
) {
    if !request.copy_as_authored || namespace.trim().is_empty() {
        return;
    }
    let mut templates = Vec::new();
    for root in ["Materials", "materials"] {
        let dir = source_dir.join(root).join("Template");
        if dir.is_dir() {
            collect_source_materials_for_relocation(&dir, source_dir, false, &mut templates);
        }
    }
    let mut relocated = HashSet::new();
    for template in templates {
        let Some(key) = material_member_key(&template.source_path, source_dir) else {
            continue;
        };
        if !key.ends_with(".bgsm") || !relocated.insert(key["materials/".len()..].to_owned()) {
            continue;
        }
        let output_subpath =
            crate::relocation::insert_namespace_after_root(&template.source_path, namespace);
        match request.materials.iter_mut().find(|entry| {
            material_member_key(&entry.source_path, source_dir).as_deref() == Some(key.as_str())
        }) {
            Some(entry) => entry.output_subpath = Some(output_subpath),
            None => request.materials.push(MaterialEntry {
                output_subpath: Some(output_subpath),
                ..template
            }),
        }
    }
    if !relocated.is_empty() {
        request.root_material_relocation = Some(RootMaterialRelocation {
            namespace: namespace.trim().to_owned(),
            relocated,
        });
    }
}

pub(crate) fn apply_relocation_to_material_request(
    request: &mut ConvertMaterialsRequest,
    source_dir: &Path,
    relocation_members: &HashSet<String>,
    namespace: &str,
    source_inventory: Option<&crate::source_inventory::SourceAssetInventory>,
) {
    if namespace.trim().is_empty() || relocation_members.is_empty() {
        return;
    }

    let overrides = crate::material_source_overrides::material_source_overrides();
    let copy_as_authored = request.copy_as_authored;
    let starfield_to_fo4 =
        request.source_game == Some(Game::Starfield) && request.target_game == Some(Game::Fo4);
    let source_materialsdb = starfield_to_fo4.then(|| {
        request
            .source_materialsdb
            .clone()
            .unwrap_or_else(|| source_dir.join("Materials/materialsbeta.cdb"))
    });
    let mut existing: HashSet<String> = request
        .materials
        .iter()
        .filter_map(|m| material_member_key(&m.source_path, source_dir))
        .collect();

    // Material members themselves move into the namespace so they cannot clobber
    // FO4 base materials.
    for member in relocation_members.iter() {
        if (!member.ends_with(".bgsm") && !member.ends_with(".bgem") && !member.ends_with(".mat"))
            || existing.contains(member)
        {
            continue;
        }
        let abs = source_dir.join(member.replace('/', std::path::MAIN_SEPARATOR_STR));
        if !abs.is_file() {
            continue;
        }
        let mut entry = MaterialEntry {
            source_path: member.clone(),
            resolved_path: abs.to_string_lossy().to_string(),
            is_cdb_ref: false,
            output_subpath: Some(crate::relocation::insert_namespace_after_root(
                member, namespace,
            )),
            texture_namespace: None,
            texture_namespace_paths: HashSet::new(),
        };
        entry.texture_namespace_paths = relocated_output_texture_paths(
            &entry,
            source_dir,
            relocation_members,
            overrides,
            copy_as_authored,
            source_materialsdb.as_deref(),
        );
        if !entry.texture_namespace_paths.is_empty() {
            entry.texture_namespace = Some(namespace.to_string());
        }
        request.materials.push(entry);
        existing.insert(member.clone());
    }

    for entry in request.materials.iter_mut() {
        let Some(key) = material_member_key(&entry.source_path, source_dir) else {
            continue;
        };
        if relocation_members.contains(&key) {
            entry.output_subpath = Some(crate::relocation::insert_namespace_after_root(
                &key, namespace,
            ));
            entry.texture_namespace_paths = relocated_output_texture_paths(
                entry,
                source_dir,
                relocation_members,
                overrides,
                copy_as_authored,
                source_materialsdb.as_deref(),
            );
            entry.texture_namespace =
                (!entry.texture_namespace_paths.is_empty()).then(|| namespace.to_string());
            continue;
        }
        let texture_namespace_paths = relocated_output_texture_paths(
            entry,
            source_dir,
            relocation_members,
            overrides,
            copy_as_authored,
            source_materialsdb.as_deref(),
        );
        if !texture_namespace_paths.is_empty() {
            entry
                .texture_namespace
                .get_or_insert_with(|| namespace.to_string());
            entry.texture_namespace_paths = texture_namespace_paths;
        }
    }

    if !request.convert_all {
        return;
    }

    // convert_all enumerates materials inside materials_native. Add explicit
    // entries for non-relocated materials whose texture slots must be namespaced,
    // and give them an explicit original output_subpath so the automatic entry is
    // suppressed instead of racing a duplicate write to the same path.
    let inventory_entries = source_inventory.and_then(|inventory| {
        enumerate_source_materials_from_inventory(
            inventory,
            source_dir,
            &["Materials", "materials"],
            request.source_game == Some(Game::Starfield) && request.target_game == Some(Game::Fo4),
        )
    });
    for mut entry in inventory_entries
        .unwrap_or_else(|| enumerate_source_materials_for_relocation(source_dir, starfield_to_fo4))
    {
        let Some(key) = material_member_key(&entry.source_path, source_dir) else {
            continue;
        };
        if existing.contains(&key) || relocation_members.contains(&key) {
            continue;
        }
        let texture_namespace_paths = relocated_output_texture_paths(
            &entry,
            source_dir,
            relocation_members,
            overrides,
            copy_as_authored,
            source_materialsdb.as_deref(),
        );
        if texture_namespace_paths.is_empty() {
            continue;
        }
        entry.output_subpath = Some(key.clone());
        entry.texture_namespace = Some(namespace.to_string());
        entry.texture_namespace_paths = texture_namespace_paths;
        request.materials.push(entry);
        existing.insert(key);
    }
}

fn enumerate_source_materials_from_inventory(
    inventory: &crate::source_inventory::SourceAssetInventory,
    source_dir: &Path,
    root_names: &[&str],
    include_mat: bool,
) -> Option<Vec<MaterialEntry>> {
    let mut out = Vec::new();
    for root_name in root_names {
        let root = source_dir.join(root_name);
        let extensions: &[&str] = if include_mat {
            &[".bgsm", ".bgem", ".mat"]
        } else {
            &[".bgsm", ".bgem"]
        };
        let files = inventory.files_if_compatible(&root, extensions)?;
        for path in files {
            let Ok(rel) = path.strip_prefix(source_dir) else {
                continue;
            };
            out.push(MaterialEntry {
                source_path: rel.to_string_lossy().replace('\\', "/"),
                resolved_path: path.to_string_lossy().to_string(),
                is_cdb_ref: false,
                output_subpath: None,
                texture_namespace: None,
                texture_namespace_paths: HashSet::new(),
            });
        }
    }
    Some(out)
}

fn relocated_output_texture_paths(
    entry: &MaterialEntry,
    source_dir: &Path,
    relocation_members: &HashSet<String>,
    overrides: &std::collections::HashMap<String, String>,
    copy_as_authored: bool,
    source_materialsdb: Option<&Path>,
) -> HashSet<String> {
    if entry.source_path.to_ascii_lowercase().ends_with(".mat") {
        let Some(cdb) = source_materialsdb else {
            return HashSet::new();
        };
        let Ok(material) = materials_native::cdb_to_bgsm::project_material(cdb, &entry.source_path)
        else {
            return HashSet::new();
        };
        let mut paths = HashSet::new();
        collect_ce2_texture_members(&material, relocation_members, &mut paths);
        return paths;
    }
    let Some(scan_path) = material_texture_scan_path(entry, source_dir, overrides) else {
        return HashSet::new();
    };
    let Ok(bytes) = std::fs::read(&scan_path) else {
        return HashSet::new();
    };
    if copy_as_authored {
        // Authored materials keep FO76's texture names, so the keys are the source paths.
        return materials_native::convert::authored_material_texture_paths(&bytes)
            .iter()
            .filter_map(|path| texture_member_key(path))
            .filter(|key| relocation_members.contains(key))
            .collect();
    }
    let ext = scan_path
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if ext == "bgem" {
        return relocated_output_bgem_texture_paths(&bytes, &entry.source_path, relocation_members);
    }
    relocated_output_bgsm_texture_paths(&bytes, &entry.source_path, &scan_path, relocation_members)
}

fn collect_ce2_texture_members(
    material: &materials_native::Ce2MaterialPayload,
    relocation_members: &HashSet<String>,
    paths: &mut HashSet<String>,
) {
    for layer in &material.layers {
        for path in [
            &layer.texture_set.diffuse,
            &layer.texture_set.normal,
            &layer.texture_set.rough,
            &layer.texture_set.metal,
            &layer.texture_set.ao,
            &layer.texture_set.opacity,
            &layer.texture_set.emissive,
        ] {
            if let Some(key) =
                texture_member_key(path).filter(|key| relocation_members.contains(key))
            {
                paths.insert(key);
            }
        }
    }
    for blender in &material.blenders {
        if let Some(key) =
            texture_member_key(&blender.mask_texture).filter(|key| relocation_members.contains(key))
        {
            paths.insert(key);
        }
    }
    for lod in &material.lod_materials {
        collect_ce2_texture_members(lod, relocation_members, paths);
    }
}

fn relocated_output_bgsm_texture_paths(
    bytes: &[u8],
    source_path: &str,
    resolved_path: &Path,
    relocation_members: &HashSet<String>,
) -> HashSet<String> {
    let Ok(original) = materials_native::bgsm::parse(bytes) else {
        return HashSet::new();
    };
    let original_smoothspec_empty = original
        .SmoothSpecTexture
        .replace('\0', "")
        .trim()
        .is_empty();
    let original_specular = original.SpecularTexture.clone();
    let mut source = original.clone();
    repair_missing_fo76_smoothspec_from_specular(&mut source, resolved_path, Game::Fo76, Game::Fo4);
    let output = downgrade_bgsm(source.clone(), source_path, Game::Fo76, Game::Fo4);
    let mut out = HashSet::new();
    add_if_source_relocated(
        &mut out,
        &source.DiffuseTexture,
        &output.DiffuseTexture,
        relocation_members,
    );
    add_if_source_relocated(
        &mut out,
        &source.NormalTexture,
        &output.NormalTexture,
        relocation_members,
    );
    if original_smoothspec_empty {
        if let Some(specular) = original_specular.as_deref() {
            add_if_source_relocated(
                &mut out,
                specular,
                &output.SmoothSpecTexture,
                relocation_members,
            );
        }
    } else {
        add_if_source_relocated(
            &mut out,
            &source.SmoothSpecTexture,
            &output.SmoothSpecTexture,
            relocation_members,
        );
    }
    add_if_source_relocated(
        &mut out,
        &source.GreyscaleTexture,
        &output.GreyscaleTexture,
        relocation_members,
    );
    if let Some(source_glow) = source.GlowTexture.as_deref() {
        if let Some(output_glow) = output.GlowTexture.as_deref() {
            add_if_source_relocated(&mut out, source_glow, output_glow, relocation_members);
        }
    }
    if let Some(source_lighting) = source.LightingTexture.as_deref() {
        if let Some(output_glow) = output.GlowTexture.as_deref() {
            add_if_source_relocated(&mut out, source_lighting, output_glow, relocation_members);
        }
    }
    if let Some(source_envmap) = source.EnvmapTexture.as_deref() {
        if let Some(output_envmap) = output.EnvmapTexture.as_deref() {
            add_if_source_relocated(&mut out, source_envmap, output_envmap, relocation_members);
        }
    }
    if let Some(source_wrinkles) = source.WrinklesTexture.as_deref() {
        if let Some(output_wrinkles) = output.WrinklesTexture.as_deref() {
            add_if_source_relocated(
                &mut out,
                source_wrinkles,
                output_wrinkles,
                relocation_members,
            );
        }
    }
    out
}

fn relocated_output_bgem_texture_paths(
    bytes: &[u8],
    source_path: &str,
    relocation_members: &HashSet<String>,
) -> HashSet<String> {
    let Ok(source) = materials_native::bgem::parse(bytes) else {
        return HashSet::new();
    };
    let output = downgrade_bgem(source.clone(), source_path, Game::Fo76, Game::Fo4);
    let mut out = HashSet::new();
    add_if_source_relocated(
        &mut out,
        &source.BaseTexture,
        &output.BaseTexture,
        relocation_members,
    );
    add_if_source_relocated(
        &mut out,
        &source.GrayscaleTexture,
        &output.GrayscaleTexture,
        relocation_members,
    );
    add_if_source_relocated(
        &mut out,
        &source.EnvmapTexture,
        &output.EnvmapTexture,
        relocation_members,
    );
    add_if_source_relocated(
        &mut out,
        &source.NormalTexture,
        &output.NormalTexture,
        relocation_members,
    );
    add_if_source_relocated(
        &mut out,
        &source.EnvmapMaskTexture,
        &output.EnvmapMaskTexture,
        relocation_members,
    );
    if let Some(source_specular) = source.SpecularTexture.as_deref() {
        if let Some(output_specular) = output.SpecularTexture.as_deref() {
            add_if_source_relocated(
                &mut out,
                source_specular,
                output_specular,
                relocation_members,
            );
        }
    }
    if let Some(source_lighting) = source.LightingTexture.as_deref() {
        if let Some(output_lighting) = output.LightingTexture.as_deref() {
            add_if_source_relocated(
                &mut out,
                source_lighting,
                output_lighting,
                relocation_members,
            );
        }
    }
    if let Some(source_glow) = source.GlowTexture.as_deref() {
        if let Some(output_glow) = output.GlowTexture.as_deref() {
            add_if_source_relocated(&mut out, source_glow, output_glow, relocation_members);
        }
    }
    out
}

fn add_if_source_relocated(
    out: &mut HashSet<String>,
    source_texture: &str,
    output_texture: &str,
    relocation_members: &HashSet<String>,
) {
    let Some(source_key) = texture_member_key(source_texture) else {
        return;
    };
    if !relocation_members.contains(&source_key) {
        return;
    }
    if let Some(output_key) = texture_member_key(output_texture) {
        out.insert(output_key);
    }
}

fn texture_member_key(path: &str) -> Option<String> {
    let key = crate::relocation::normalize_rel(path);
    if key.is_empty() {
        return None;
    }
    if key.starts_with("textures/") {
        Some(key)
    } else {
        Some(format!("textures/{key}"))
    }
}

fn material_texture_scan_path(
    entry: &MaterialEntry,
    source_dir: &Path,
    overrides: &std::collections::HashMap<String, String>,
) -> Option<PathBuf> {
    let key = material_member_key(&entry.source_path, source_dir)?;
    if let Some(replacement) = overrides.get(&key) {
        let path = source_dir.join(replacement.replace('/', std::path::MAIN_SEPARATOR_STR));
        if path.is_file() {
            return Some(path);
        }
    }
    let resolved = Path::new(&entry.resolved_path);
    if resolved.is_file() {
        Some(resolved.to_path_buf())
    } else {
        let path = source_dir.join(key.replace('/', std::path::MAIN_SEPARATOR_STR));
        path.is_file().then_some(path)
    }
}

fn material_member_key(source_path: &str, source_dir: &Path) -> Option<String> {
    let mut key = crate::relocation::member_key_for_source_path(source_path, source_dir);
    if key.is_empty() {
        return None;
    }
    if !key.starts_with("materials/") {
        key = format!("materials/{key}");
    }
    (key.ends_with(".bgsm") || key.ends_with(".bgem") || key.ends_with(".mat")).then_some(key)
}

fn enumerate_source_materials_for_relocation(
    source_dir: &Path,
    include_mat: bool,
) -> Vec<MaterialEntry> {
    let mut out = Vec::new();
    for root_name in ["Materials", "materials"] {
        let root = source_dir.join(root_name);
        if root.is_dir() {
            collect_source_materials_for_relocation(&root, source_dir, include_mat, &mut out);
        }
    }
    out
}

fn collect_source_materials_for_relocation(
    dir: &Path,
    source_dir: &Path,
    include_mat: bool,
    out: &mut Vec<MaterialEntry>,
) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_source_materials_for_relocation(&path, source_dir, include_mat, out);
            continue;
        }
        let is_material = path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| {
                let ext = ext.to_ascii_lowercase();
                ext == "bgsm" || ext == "bgem" || (include_mat && ext == "mat")
            })
            .unwrap_or(false);
        if !is_material {
            continue;
        }
        let Ok(rel) = path.strip_prefix(source_dir) else {
            continue;
        };
        out.push(MaterialEntry {
            source_path: rel.to_string_lossy().replace('\\', "/"),
            resolved_path: path.to_string_lossy().to_string(),
            is_cdb_ref: false,
            output_subpath: None,
            texture_namespace: None,
            texture_namespace_paths: HashSet::new(),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use materials_native::{bgem, bgsm};
    use std::path::Path;

    #[test]
    #[ignore = "requires extracted Starfield materialsbeta.cdb"]
    fn starfield_cdb_relocation_follows_inherited_layers_and_blender_masks() {
        let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../../extracted/starfield");
        let cdb = source.join("Materials/materialsbeta.cdb");
        let material_path =
            "Materials/Clothes/Spacesuit_Constellation/Spacesuit_Constellation_UpperBody.mat";
        let material =
            materials_native::cdb_to_bgsm::project_material(&cdb, material_path).unwrap();
        assert_eq!(material.layers.len(), 4);
        assert_eq!(material.blenders.len(), 3);
        let members = [
            &material.layers[1].texture_set.diffuse,
            &material.layers[1].texture_set.normal,
            &material.layers[1].texture_set.rough,
            &material.blenders[0].mask_texture,
        ]
        .into_iter()
        .map(|path| texture_member_key(path).expect("source texture member"))
        .collect::<HashSet<_>>();
        assert_eq!(members.len(), 4);
        let entry = MaterialEntry {
            source_path: material_path.into(),
            resolved_path: material_path.into(),
            is_cdb_ref: true,
            output_subpath: None,
            texture_namespace: None,
            texture_namespace_paths: HashSet::new(),
        };
        let actual = relocated_output_texture_paths(
            &entry,
            &source,
            &members,
            &std::collections::HashMap::new(),
            false,
            Some(&cdb),
        );
        assert_eq!(actual, members);
    }

    fn write_fo76_bgsm(path: &Path, cast_shadows: bool, empty_spec_slot: bool) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let mut data = bgsm::BgsmData::default();
        data.header.signature = bgsm::BGSM_SIGNATURE;
        data.header.version = 20; // FO76-era header (>= 10): wetness/env fields absent
        data.DiffuseTexture = "Textures\\Landscape\\Rock01_d.dds".to_string();
        data.NormalTexture = "Textures\\Landscape\\Rock01_n.dds".to_string();
        if empty_spec_slot {
            data.SmoothSpecTexture = String::new(); // must serialize as "\0", never len-0
        }
        data.CastShadows = cast_shadows;
        std::fs::write(path, bgsm::write(&data)).unwrap();
    }

    fn write_fo76_bgsm_with_diffuse(path: &Path, diffuse: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let mut data = bgsm::BgsmData::default();
        data.header.signature = bgsm::BGSM_SIGNATURE;
        data.header.version = 20;
        data.DiffuseTexture = diffuse.to_string();
        data.NormalTexture = diffuse.replace("_d.dds", "_n.dds");
        std::fs::write(path, bgsm::write(&data)).unwrap();
    }

    fn write_fo76_bgsm_with_textures(path: &Path, diffuse: &str, normal: &str, specular: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let mut data = bgsm::BgsmData::default();
        data.header.signature = bgsm::BGSM_SIGNATURE;
        data.header.version = 20;
        data.DiffuseTexture = diffuse.to_string();
        data.NormalTexture = normal.to_string();
        data.SpecularTexture = Some(specular.to_string());
        std::fs::write(path, bgsm::write(&data)).unwrap();
    }

    fn write_fo76_bgem_with_textures(
        path: &Path,
        base: &str,
        envmap: &str,
        normal: &str,
        lighting: &str,
    ) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let mut data = bgem::BgemData::default();
        data.header.signature = bgem::BGEM_SIGNATURE;
        data.header.version = 22;
        data.BaseTexture = base.to_string();
        data.EnvmapTexture = envmap.to_string();
        data.NormalTexture = normal.to_string();
        data.LightingTexture = Some(lighting.to_string());
        std::fs::write(path, bgem::write(&data)).unwrap();
    }

    fn find_bgsm_under(dir: &Path) -> Option<std::path::PathBuf> {
        let rd = std::fs::read_dir(dir).ok()?;
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                if let Some(found) = find_bgsm_under(&p) {
                    return Some(found);
                }
            } else if p
                .extension()
                .and_then(|x| x.to_str())
                .map(|x| x.eq_ignore_ascii_case("bgsm"))
                .unwrap_or(false)
            {
                return Some(p);
            }
        }
        None
    }

    fn engine_params(source: &Path, mod_path: &Path) -> MaterialsEngineParams {
        MaterialsEngineParams {
            mod_path: mod_path.to_path_buf(),
            source_extracted: source.to_path_buf(),
            source_inventory: None,
            target_extracted: None,
            target_data_dir: None,
            source_game: materials_native::convert::Game::Fo76,
            target_game: materials_native::convert::Game::Fo4,
            materials: Vec::new(),
            convert_all: true,
            pbr_carry: false,
            relocation_members: std::collections::HashSet::new(),
            namespace: String::new(),
            source_materialsdb: None,
            overwrite_existing: true,
            target_asset_paths: HashSet::new(),
            base_overwrite_prefixes: Vec::new(),
            base_overwrite_paths: HashSet::new(),
            copy_as_authored: false,
        }
    }

    #[test]
    fn fold_in_and_manual_override_match_legacy_converter() {
        {
            let tmp = std::env::temp_dir().join("mat_engine_fold_in");
            let _ = std::fs::remove_dir_all(&tmp);
            let source = tmp.join("source");
            write_fo76_bgsm(
                &source
                    .join("Materials/Test/rock.bgsm".replace('/', std::path::MAIN_SEPARATOR_STR)),
                false,
                true,
            );

            let report = run_materials_engine(engine_params(&source, &tmp.join("mod")));
            assert!(report.assets_written >= 1, "one material must convert");

            let out = find_bgsm_under(&tmp.join("mod").join("data").join("Materials"))
                .expect("output bgsm");
            let parsed =
                bgsm::parse(&std::fs::read(&out).unwrap()).expect("output BGSM must parse");
            assert!(
                parsed.CastShadows,
                "bCastShadows must be forced true at conversion time"
            );
            // (2) v22→v2 production invariants:
            assert_eq!(parsed.WetnessControlEnvMapScale, Some(-1.0));
            assert_eq!(parsed.header.env_mapping_mask_scale, Some(1.0));
            // (3) Empty-slot "\0" invariant: a len-0 slot would misalign the stream
            //     and corrupt every later field — the fact that CastShadows (a late
            //     field) parsed correctly above IS the regression check for the
            //     empty SmoothSpecTexture slot written earlier in the stream.
            let _ = std::fs::remove_dir_all(&tmp);
        }
        {
            // The engine is the legacy converter + the override — prove byte equality.
            let tmp = std::env::temp_dir().join("mat_engine_equiv");
            let _ = std::fs::remove_dir_all(&tmp);
            let source = tmp.join("source");
            write_fo76_bgsm(
                &source
                    .join("Materials/Test/rock.bgsm".replace('/', std::path::MAIN_SEPARATOR_STR)),
                false,
                false,
            );

            run_materials_engine(engine_params(&source, &tmp.join("mod_engine")));

            let legacy_request = materials_native::convert::ConvertMaterialsRequest {
                materials: Vec::new(),
                source_game: Some(materials_native::convert::Game::Fo76),
                target_game: Some(materials_native::convert::Game::Fo4),
                asset_prefix: "fo76".to_string(),
                source_materialsdb: None,
                overwrite_existing: true,
                bgsm_default_overrides: vec![("bCastShadows".to_string(), serde_json::json!(true))],
                convert_all: true,
                pbr_carry: false,
                source_path_overrides: std::collections::HashMap::new(),
                target_asset_paths: HashSet::new(),
                base_overwrite_prefixes: Vec::new(),
                base_overwrite_paths: HashSet::new(),
                copy_as_authored: false,
                root_material_relocation: None,
            };
            materials_native::convert::run_convert_materials(
                &tmp.join("mod_legacy"),
                &legacy_request,
                materials_native::convert::Game::Fo76,
                materials_native::convert::Game::Fo4,
                &source,
                None,
                None,
            );

            let engine_out =
                find_bgsm_under(&tmp.join("mod_engine").join("data")).expect("engine bgsm");
            let legacy_out =
                find_bgsm_under(&tmp.join("mod_legacy").join("data")).expect("legacy bgsm");
            assert_eq!(
                std::fs::read(&engine_out).unwrap(),
                std::fs::read(&legacy_out).unwrap(),
                "engine wrapper must not change converter bytes"
            );
            let _ = std::fs::remove_dir_all(&tmp);
        }
    }

    #[test]
    fn relocation_members_convert_suppress_duplicates_and_apply_overrides() {
        {
            // Port of material_phase_converts_relocation_member_absent_from_params
            // at engine level.
            let tmp = std::env::temp_dir().join("mat_engine_relocation");
            let _ = std::fs::remove_dir_all(&tmp);
            let source = tmp.join("source");
            write_fo76_bgsm(
                &source.join(
                    "Materials/Landscape/rock01.bgsm".replace('/', std::path::MAIN_SEPARATOR_STR),
                ),
                true,
                false,
            );

            let mut params = engine_params(&source, &tmp.join("mod"));
            params.convert_all = false; // materials intentionally EMPTY — the member must still convert
            params.namespace = "FO76".to_string();
            params
                .relocation_members
                .insert("materials/landscape/rock01.bgsm".to_string());

            run_materials_engine(params);

            let fo76_dir = tmp.join("mod").join("data").join("Materials").join("FO76");
            let output = find_bgsm_under(&fo76_dir).unwrap_or_else(|| {
                panic!("expected a relocated material under {}", fo76_dir.display())
            });
            let parsed = bgsm::parse(&std::fs::read(output).unwrap()).unwrap();
            assert!(
                !parsed
                    .DiffuseTexture
                    .replace('\\', "/")
                    .to_ascii_lowercase()
                    .contains("fo76/"),
                "a relocated material must still reuse texture slots that were exactly deduplicated"
            );
            let _ = std::fs::remove_dir_all(&tmp);
        }
        {
            let tmp = std::env::temp_dir().join("mat_engine_relocation_convert_all");
            let _ = std::fs::remove_dir_all(&tmp);
            let source = tmp.join("source");
            write_fo76_bgsm(
                &source.join(
                    "Materials/Landscape/Grass/forest76waterhemlock01.bgsm"
                        .replace('/', std::path::MAIN_SEPARATOR_STR),
                ),
                true,
                false,
            );

            let mut params = engine_params(&source, &tmp.join("mod"));
            params.convert_all = true;
            params.namespace = "FO76".to_string();
            params
                .relocation_members
                .insert("materials/landscape/grass/forest76waterhemlock01.bgsm".to_string());

            run_materials_engine(params);

            let root_output = tmp
                .join("mod")
                .join("data")
                .join("Materials")
                .join("Landscape")
                .join("Grass")
                .join("forest76waterhemlock01.bgsm");
            let namespaced_output = tmp
                .join("mod")
                .join("data")
                .join("materials")
                .join("FO76")
                .join("landscape")
                .join("grass")
                .join("forest76waterhemlock01.bgsm");
            assert!(
                !root_output.exists(),
                "relocated material should not also be emitted at the default path"
            );
            assert!(
                namespaced_output.is_file(),
                "expected relocated material at {}",
                namespaced_output.display()
            );
            let _ = std::fs::remove_dir_all(&tmp);
        }
        {
            let tmp = std::env::temp_dir().join("mat_engine_source_override");
            let _ = std::fs::remove_dir_all(&tmp);
            let source = tmp.join("source");
            write_fo76_bgsm_with_diffuse(
                &source.join(
                    "materials/landscape/ground/temp_groundtexture01.bgsm"
                        .replace('/', std::path::MAIN_SEPARATOR_STR),
                ),
                "Textures\\Landscape\\Ground\\TEMP_GroundTexture01_d.dds",
            );
            write_fo76_bgsm_with_diffuse(
                &source.join(
                    "materials/landscape/ground/forestrocks01.bgsm"
                        .replace('/', std::path::MAIN_SEPARATOR_STR),
                ),
                "Textures\\Landscape\\Ground\\ForestRocks01_d.dds",
            );

            let mut params = engine_params(&source, &tmp.join("mod"));
            params.convert_all = false;
            params.namespace = "FO76".to_string();
            params
                .relocation_members
                .insert("materials/landscape/ground/temp_groundtexture01.bgsm".to_string());
            params
                .relocation_members
                .insert("textures/landscape/ground/forestrocks01_d.dds".to_string());

            run_materials_engine(params);

            let out = tmp
                .join("mod")
                .join("data")
                .join("materials")
                .join("FO76")
                .join("landscape")
                .join("ground")
                .join("temp_groundtexture01.bgsm");
            let parsed =
                bgsm::parse(&std::fs::read(&out).unwrap()).expect("output BGSM must parse");
            let diffuse = parsed
                .DiffuseTexture
                .trim_end_matches('\0')
                .replace('\\', "/");
            assert_eq!(diffuse, "FO76/Landscape/Ground/ForestRocks01_d.dds");
            assert!(!diffuse.to_ascii_lowercase().contains("temp_groundtexture"));
            let _ = std::fs::remove_dir_all(&tmp);
        }
    }

    #[test]
    fn relocated_material_namespaces_only_changed_texture_slots() {
        let tmp = std::env::temp_dir().join("mat_engine_relocated_decal_mixed_textures");
        let _ = std::fs::remove_dir_all(&tmp);
        let source = tmp.join("source");
        let material = source
            .join("Materials")
            .join("DLC04")
            .join("Decals")
            .join("Parking.bgsm");
        write_fo76_bgsm_with_textures(
            &material,
            "DLC04/Decals/Parking_d.dds",
            "DLC04/Decals/Parking_n.dds",
            "DLC04/Decals/Parking_r.dds",
        );

        let mut params = engine_params(&source, &tmp.join("mod"));
        params.namespace = "FO76".to_string();
        params
            .relocation_members
            .insert("materials/dlc04/decals/parking.bgsm".to_string());
        params
            .relocation_members
            .insert("textures/dlc04/decals/parking_d.dds".to_string());

        run_materials_engine(params);

        let output = tmp
            .join("mod")
            .join("data")
            .join("Materials")
            .join("FO76")
            .join("DLC04")
            .join("Decals")
            .join("Parking.bgsm");
        let parsed = bgsm::parse(&std::fs::read(output).unwrap()).unwrap();
        assert_eq!(
            parsed.DiffuseTexture.trim_end_matches('\0'),
            "FO76/DLC04/Decals/Parking_d.dds"
        );
        assert_eq!(
            parsed.NormalTexture.trim_end_matches('\0'),
            "DLC04/Decals/Parking_n.dds"
        );
        assert_eq!(
            parsed.SmoothSpecTexture.trim_end_matches('\0'),
            "DLC04/Decals/Parking_s.dds"
        );
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn convert_all_namespaces_root_materials_for_relocated_members() {
        {
            let tmp =
                std::env::temp_dir().join("mat_engine_texture_member_namespaces_root_material");
            let _ = std::fs::remove_dir_all(&tmp);
            let source = tmp.join("source");
            let material = source
                .join("Materials")
                .join("SetDressing")
                .join("acducts01.bgsm");
            write_fo76_bgsm_with_textures(
                &material,
                "SetDressing/ACducts01_d.dds",
                "SetDressing/ACducts01_n.dds",
                "SetDressing/ACducts01_r.dds",
            );

            let mut params = engine_params(&source, &tmp.join("mod"));
            params.namespace = "FO76".to_string();
            params
                .relocation_members
                .insert("textures/setdressing/acducts01_d.dds".to_string());
            params
                .relocation_members
                .insert("textures/setdressing/acducts01_n.dds".to_string());
            params
                .relocation_members
                .insert("textures/setdressing/acducts01_r.dds".to_string());

            run_materials_engine(params);

            let root_output = tmp
                .join("mod")
                .join("data")
                .join("Materials")
                .join("SetDressing")
                .join("acducts01.bgsm");
            let namespaced_output = tmp
                .join("mod")
                .join("data")
                .join("Materials")
                .join("FO76")
                .join("SetDressing")
                .join("acducts01.bgsm");
            assert!(root_output.is_file(), "expected original material path");
            assert!(
                !namespaced_output.exists(),
                "texture-only relocation should not move the material itself"
            );
            let parsed = bgsm::parse(&std::fs::read(&root_output).unwrap()).unwrap();
            assert_eq!(
                parsed.DiffuseTexture.trim_end_matches('\0'),
                "FO76/SetDressing/ACducts01_d.dds"
            );
            assert_eq!(
                parsed.NormalTexture.trim_end_matches('\0'),
                "FO76/SetDressing/ACducts01_n.dds"
            );
            assert_eq!(
                parsed.SmoothSpecTexture.trim_end_matches('\0'),
                "FO76/SetDressing/ACducts01_s.dds"
            );
            let _ = std::fs::remove_dir_all(&tmp);
        }
        {
            let tmp = std::env::temp_dir()
                .join("mat_engine_texture_member_namespaces_mixed_root_material");
            let _ = std::fs::remove_dir_all(&tmp);
            let source = tmp.join("source");
            let material = source
                .join("Materials")
                .join("SetDressing")
                .join("WhiteSpring")
                .join("WhiteSpring_Lamp01.bgsm");
            write_fo76_bgsm_with_textures(
                &material,
                "SetDressing/WhiteSpring/WhiteSpring_Fancy_Furniture_Sets_15_d.dds",
                "SetDressing/WhiteSpring/WhiteSpring_Fancy_Furniture_Sets_15_n.dds",
                "Shared/Default_r.dds",
            );

            let mut params = engine_params(&source, &tmp.join("mod"));
            params.namespace = "FO76".to_string();
            params
                .relocation_members
                .insert("textures/shared/default_r.dds".to_string());

            run_materials_engine(params);

            let output = tmp
                .join("mod")
                .join("data")
                .join("Materials")
                .join("SetDressing")
                .join("WhiteSpring")
                .join("WhiteSpring_Lamp01.bgsm");
            let parsed = bgsm::parse(&std::fs::read(&output).unwrap()).unwrap();
            assert_eq!(
                parsed.DiffuseTexture.trim_end_matches('\0'),
                "SetDressing/WhiteSpring/WhiteSpring_Fancy_Furniture_Sets_15_d.dds"
            );
            assert_eq!(
                parsed.NormalTexture.trim_end_matches('\0'),
                "SetDressing/WhiteSpring/WhiteSpring_Fancy_Furniture_Sets_15_n.dds"
            );
            assert_eq!(
                parsed.SmoothSpecTexture.trim_end_matches('\0'),
                "FO76/Shared/Default_s.dds"
            );
            let _ = std::fs::remove_dir_all(&tmp);
        }
        {
            let tmp = std::env::temp_dir().join("mat_engine_texture_member_namespaces_root_bgem");
            let _ = std::fs::remove_dir_all(&tmp);
            let source = tmp.join("source");
            let material = source
                .join("Materials")
                .join("SetDressing")
                .join("ScorchedSpecimenJar")
                .join("SpecimenjarAcid01.bgem");
            write_fo76_bgem_with_textures(
                &material,
                "Shared/Default_d.dds",
                "Shared/Cubemaps/mipblur_DefaultOutside1_Copper.dds",
                "Shared/Default_n.dds",
                "Shared/Default_Noise_20_l.dds",
            );

            let mut params = engine_params(&source, &tmp.join("mod"));
            params.namespace = "FO76".to_string();
            params
                .relocation_members
                .insert("textures/shared/default_d.dds".to_string());
            params
                .relocation_members
                .insert("textures/shared/default_n.dds".to_string());

            run_materials_engine(params);

            let output = tmp
                .join("mod")
                .join("data")
                .join("Materials")
                .join("SetDressing")
                .join("ScorchedSpecimenJar")
                .join("SpecimenjarAcid01.bgem");
            let parsed = bgem::parse(&std::fs::read(&output).unwrap()).unwrap();
            assert_eq!(
                parsed.BaseTexture.trim_end_matches('\0'),
                "FO76/Shared/Default_d.dds"
            );
            assert_eq!(
                parsed.NormalTexture.trim_end_matches('\0'),
                "FO76/Shared/Default_n.dds"
            );
            assert_eq!(
                parsed.EnvmapTexture.trim_end_matches('\0'),
                "Shared/Cubemaps/mipblur_DefaultOutside1_Copper.dds",
                "shared cubemaps remain un-namespaced"
            );
            let _ = std::fs::remove_dir_all(&tmp);
        }
    }

    #[test]
    fn source_inventory_material_enumeration_matches_direct_walkers() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source");
        for relative in [
            "Materials/Actors/Nested/body.BGSM",
            "Materials/Effects/glow.bgem",
            "Materials/ignore.txt",
        ] {
            let path = source.join(relative);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, b"fixture").unwrap();
        }
        let inventory = crate::source_inventory::SourceAssetInventory::default();
        let direct = enumerate_source_materials_for_relocation(&source, false);
        let from_inventory = enumerate_source_materials_from_inventory(
            &inventory,
            &source,
            &["Materials", "materials"],
            false,
        )
        .unwrap();
        let project = |entries: Vec<MaterialEntry>| {
            entries
                .into_iter()
                .map(|entry| (entry.source_path, entry.resolved_path))
                .collect::<Vec<_>>()
        };
        assert_eq!(project(from_inventory), project(direct));
    }

    fn write_fo76_v22_bgsm(path: &Path, diffuse: &str, normal: &str, specular: &str) -> Vec<u8> {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let mut data = bgsm::BgsmData::default();
        data.header.signature = bgsm::BGSM_SIGNATURE;
        data.header.version = 22;
        data.DiffuseTexture = diffuse.to_string();
        data.NormalTexture = normal.to_string();
        data.SpecularTexture = Some(specular.to_string());
        data.RootMaterialPath = "Materials\\Template\\Base.bgsm\0".to_string();
        data.CastShadows = false;
        let bytes = bgsm::write(&data);
        std::fs::write(path, &bytes).unwrap();
        bytes
    }

    fn assert_only_textures_changed(
        source: &[u8],
        output: &[u8],
        diffuse: &str,
        normal: &str,
        specular: &str,
    ) {
        let source = bgsm::parse(source).unwrap();
        let mut output = bgsm::parse(output).unwrap();
        assert_eq!(output.DiffuseTexture, diffuse);
        assert_eq!(output.NormalTexture, normal);
        assert_eq!(output.SpecularTexture.as_deref(), Some(specular));
        output.DiffuseTexture = source.DiffuseTexture.clone();
        output.NormalTexture = source.NormalTexture.clone();
        output.SpecularTexture = source.SpecularTexture.clone();
        assert_eq!(
            serde_json::to_value(&output).unwrap(),
            serde_json::to_value(&source).unwrap()
        );
    }

    #[test]
    fn native_copy_namespaces_relocated_texture_paths_in_authored_materials() {
        let tmp = std::env::temp_dir().join("mat_engine_native_copy_relocated_textures");
        let _ = std::fs::remove_dir_all(&tmp);
        let source = tmp.join("source");
        let shared = write_fo76_v22_bgsm(
            &source
                .join("Materials")
                .join("SetDressing")
                .join("acducts01.bgsm"),
            "SetDressing\\ACducts01_d.dds\0",
            "SetDressing\\ACducts01_n.dds\0",
            "SetDressing\\ACducts01_r.dds\0",
        );
        let member = write_fo76_v22_bgsm(
            &source.join("Materials").join("Vehicles").join("bus01.bgsm"),
            "Vehicles\\Bus01_d.dds\0",
            "Vehicles\\Bus01_n.dds\0",
            "Vehicles\\Bus01_r.dds\0",
        );

        let mut params = engine_params(&source, &tmp.join("mod"));
        params.copy_as_authored = true;
        params.namespace = "FO76".to_string();
        for member in [
            "textures/setdressing/acducts01_d.dds",
            "textures/setdressing/acducts01_r.dds",
            "materials/vehicles/bus01.bgsm",
            "textures/vehicles/bus01_d.dds",
            "textures/vehicles/bus01_n.dds",
            "textures/vehicles/bus01_r.dds",
        ] {
            params.relocation_members.insert(member.to_string());
        }
        run_materials_engine(params);

        let data = tmp.join("mod").join("data").join("Materials");
        assert_only_textures_changed(
            &shared,
            &std::fs::read(data.join("SetDressing").join("acducts01.bgsm")).unwrap(),
            "FO76\\SetDressing\\ACducts01_d.dds\0",
            "SetDressing\\ACducts01_n.dds\0",
            "FO76\\SetDressing\\ACducts01_r.dds\0",
        );
        assert_only_textures_changed(
            &member,
            &std::fs::read(data.join("FO76").join("Vehicles").join("bus01.bgsm")).unwrap(),
            "FO76\\Vehicles\\Bus01_d.dds\0",
            "FO76\\Vehicles\\Bus01_n.dds\0",
            "FO76\\Vehicles\\Bus01_r.dds\0",
        );
        let _ = std::fs::remove_dir_all(&tmp);
    }

    fn write_fo76_v22_bgsm_rooted_on(path: &Path, root: &str) -> Vec<u8> {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let mut data = bgsm::BgsmData::default();
        data.header.signature = bgsm::BGSM_SIGNATURE;
        data.header.version = 22;
        data.RootMaterialPath = root.to_string();
        let bytes = bgsm::write(&data);
        std::fs::write(path, &bytes).unwrap();
        bytes
    }

    fn root_of(path: &Path) -> String {
        bgsm::parse(&std::fs::read(path).unwrap())
            .unwrap()
            .RootMaterialPath
    }

    #[test]
    fn native_copy_moves_templates_under_the_namespace_and_repoints_their_roots() {
        let tmp = std::env::temp_dir().join("mat_engine_native_copy_relocated_templates");
        let _ = std::fs::remove_dir_all(&tmp);
        let source = tmp.join("source");
        let templates = source.join("Materials").join("Template");
        write_fo76_v22_bgsm_rooted_on(&templates.join("BasicRough.bgsm"), "\0");
        write_fo76_v22_bgsm_rooted_on(&templates.join("WoodTemplate_Wet.bgsm"), "\0");
        write_fo76_v22_bgsm_rooted_on(
            &templates.join("WoodTemplate.bgsm"),
            "Template/WoodTemplate_Wet.bgsm\0",
        );
        write_fo76_v22_bgsm_rooted_on(
            &source
                .join("Materials")
                .join("Architecture")
                .join("awning01.bgsm"),
            "Template/basicrough.bgsm\0",
        );
        write_fo76_v22_bgsm_rooted_on(
            &source
                .join("Materials")
                .join("Architecture")
                .join("legacy01.bgsm"),
            "template/defaultTemplate_wet.bgsm\0",
        );

        let mut params = engine_params(&source, &tmp.join("mod"));
        params.copy_as_authored = true;
        params.namespace = "FO76".to_string();
        run_materials_engine(params);

        let data = tmp.join("mod").join("data").join("Materials");
        let moved = data.join("FO76").join("Template");
        assert!(
            !data.join("Template").exists(),
            "a template left at its own path replaces the target game's"
        );
        assert_eq!(root_of(&moved.join("BasicRough.bgsm")), "\0");
        assert_eq!(
            root_of(&moved.join("WoodTemplate.bgsm")),
            "FO76/Template/WoodTemplate_Wet.bgsm\0"
        );
        assert!(moved.join("WoodTemplate_Wet.bgsm").is_file());
        let architecture = data.join("Architecture");
        assert_eq!(
            root_of(&architecture.join("awning01.bgsm")),
            "FO76/Template/basicrough.bgsm\0"
        );
        assert_eq!(
            root_of(&architecture.join("legacy01.bgsm")),
            "template/defaultTemplate_wet.bgsm\0",
            "a root the source does not ship stays on the target game's template"
        );
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn converted_materials_keep_templates_at_their_own_path() {
        let tmp = std::env::temp_dir().join("mat_engine_converted_templates_unmoved");
        let _ = std::fs::remove_dir_all(&tmp);
        let source = tmp.join("source");
        write_fo76_v22_bgsm_rooted_on(
            &source
                .join("Materials")
                .join("Template")
                .join("BasicRough.bgsm"),
            "\0",
        );

        let mut params = engine_params(&source, &tmp.join("mod"));
        params.namespace = "FO76".to_string();
        run_materials_engine(params);

        let data = tmp.join("mod").join("data").join("Materials");
        assert!(!data.join("FO76").join("Template").exists());
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
