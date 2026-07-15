//! Materials fold-in: drives the existing materials_native converter with
//! (a) the relocation union + forced Materials/FO76/... subpaths and (b) the
//! bCastShadows sanitizer folded in as a bgsm_default_override (so the former
//! post-hoc rglob pass in regen_fo76.py becomes a no-op for engine outputs).
//! BGSM/BGEM byte invariants ("\0" empty strings, v22→v2 scale defaults) come
//! from the shared writers — reused, never reimplemented.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use materials_native::convert::{
    ConvertMaterialsReport, ConvertMaterialsRequest, Game, MaterialEntry, downgrade_bgem,
    downgrade_bgsm, repair_missing_fo76_smoothspec_from_specular, run_convert_materials,
};

pub struct MaterialsEngineParams {
    pub mod_path: PathBuf,
    pub source_extracted: PathBuf,
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
}

pub fn run_materials_engine(params: MaterialsEngineParams) -> ConvertMaterialsReport {
    let mut request = ConvertMaterialsRequest {
        materials: params.materials,
        source_game: Some(params.source_game),
        target_game: Some(params.target_game),
        asset_prefix: "fo76".to_string(), // accepted-for-compat; output unprefixed
        source_materialsdb: params.source_materialsdb,
        overwrite_existing: params.overwrite_existing,
        // The bCastShadows fold-in: applied to every written BGSM by
        // write_bgsm -> apply_bgsm_overrides.
        bgsm_default_overrides: vec![("bCastShadows".to_string(), serde_json::json!(true))],
        convert_all: params.convert_all,
        pbr_carry: params.pbr_carry,
        source_path_overrides: crate::material_source_overrides::material_source_overrides()
            .clone(),
        target_asset_paths: params.target_asset_paths,
    };

    apply_relocation_to_material_request(
        &mut request,
        &params.source_extracted,
        &params.relocation_members,
        &params.namespace,
    );

    // Source-path overrides are applied inside run_convert_materials so the
    // convert_all enumeration honors them too (request.source_path_overrides).

    run_convert_materials(
        &params.mod_path,
        &request,
        params.source_game,
        params.target_game,
        &params.source_extracted,
        params.target_extracted.as_deref(),
        params.target_data_dir.as_deref(),
    )
}

pub(crate) fn apply_relocation_to_material_request(
    request: &mut ConvertMaterialsRequest,
    source_dir: &Path,
    relocation_members: &HashSet<String>,
    namespace: &str,
) {
    if namespace.trim().is_empty() || relocation_members.is_empty() {
        return;
    }

    let overrides = crate::material_source_overrides::material_source_overrides();
    let mut existing: HashSet<String> = request
        .materials
        .iter()
        .filter_map(|m| material_member_key(&m.source_path, source_dir))
        .collect();

    // Material members themselves move into the namespace so they cannot clobber
    // FO4 base materials.
    for member in relocation_members.iter() {
        if (!member.ends_with(".bgsm") && !member.ends_with(".bgem")) || existing.contains(member) {
            continue;
        }
        let abs = source_dir.join(member.replace('/', std::path::MAIN_SEPARATOR_STR));
        if !abs.is_file() {
            continue;
        }
        request.materials.push(MaterialEntry {
            source_path: member.clone(),
            resolved_path: abs.to_string_lossy().to_string(),
            is_cdb_ref: false,
            output_subpath: Some(crate::relocation::insert_namespace_after_root(
                member, namespace,
            )),
            texture_namespace: Some(namespace.to_string()),
            texture_namespace_paths: HashSet::new(),
        });
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
            entry.texture_namespace = Some(namespace.to_string());
            continue;
        }
        let texture_namespace_paths =
            relocated_output_texture_paths(entry, source_dir, relocation_members, overrides);
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
    for mut entry in enumerate_source_materials_for_relocation(source_dir) {
        let Some(key) = material_member_key(&entry.source_path, source_dir) else {
            continue;
        };
        if existing.contains(&key) || relocation_members.contains(&key) {
            continue;
        }
        let texture_namespace_paths =
            relocated_output_texture_paths(&entry, source_dir, relocation_members, overrides);
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

fn relocated_output_texture_paths(
    entry: &MaterialEntry,
    source_dir: &Path,
    relocation_members: &HashSet<String>,
    overrides: &std::collections::HashMap<String, String>,
) -> HashSet<String> {
    let Some(scan_path) = material_texture_scan_path(entry, source_dir, overrides) else {
        return HashSet::new();
    };
    let Ok(bytes) = std::fs::read(&scan_path) else {
        return HashSet::new();
    };
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
    (key.ends_with(".bgsm") || key.ends_with(".bgem")).then_some(key)
}

fn enumerate_source_materials_for_relocation(source_dir: &Path) -> Vec<MaterialEntry> {
    let mut out = Vec::new();
    for root_name in ["Materials", "materials"] {
        let root = source_dir.join(root_name);
        if root.is_dir() {
            collect_source_materials_for_relocation(&root, source_dir, &mut out);
        }
    }
    out
}

fn collect_source_materials_for_relocation(
    dir: &Path,
    source_dir: &Path,
    out: &mut Vec<MaterialEntry>,
) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_source_materials_for_relocation(&path, source_dir, out);
            continue;
        }
        let is_material = path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| {
                let ext = ext.to_ascii_lowercase();
                ext == "bgsm" || ext == "bgem"
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
        }
    }

    #[test]
    fn fold_in_forces_cast_shadows_true_and_preserves_invariants() {
        let tmp = std::env::temp_dir().join("mat_engine_fold_in");
        let _ = std::fs::remove_dir_all(&tmp);
        let source = tmp.join("source");
        write_fo76_bgsm(
            &source.join("Materials/Test/rock.bgsm".replace('/', std::path::MAIN_SEPARATOR_STR)),
            false,
            true,
        );

        let report = run_materials_engine(engine_params(&source, &tmp.join("mod")));
        assert!(report.assets_written >= 1, "one material must convert");

        let out =
            find_bgsm_under(&tmp.join("mod").join("data").join("Materials")).expect("output bgsm");
        let parsed = bgsm::parse(&std::fs::read(&out).unwrap()).expect("output BGSM must parse");
        // (1) Sanitizer fold-in (the former regen_fo76.py rglob pass is a no-op here):
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

    #[test]
    fn engine_equals_legacy_converter_with_manual_override() {
        // The engine is the legacy converter + the override — prove byte equality.
        let tmp = std::env::temp_dir().join("mat_engine_equiv");
        let _ = std::fs::remove_dir_all(&tmp);
        let source = tmp.join("source");
        write_fo76_bgsm(
            &source.join("Materials/Test/rock.bgsm".replace('/', std::path::MAIN_SEPARATOR_STR)),
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

    #[test]
    fn relocation_member_absent_from_list_converts_into_namespace() {
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
        assert!(
            find_bgsm_under(&fo76_dir).is_some(),
            "expected a relocated material under {}",
            fo76_dir.display()
        );
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn relocation_member_convert_all_suppresses_default_duplicate() {
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

    #[test]
    fn convert_all_namespaces_root_material_when_texture_member_is_relocated() {
        let tmp = std::env::temp_dir().join("mat_engine_texture_member_namespaces_root_material");
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

    #[test]
    fn convert_all_namespaces_only_relocated_slots_for_mixed_root_material() {
        let tmp =
            std::env::temp_dir().join("mat_engine_texture_member_namespaces_mixed_root_material");
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

    #[test]
    fn convert_all_namespaces_root_effect_material_when_texture_member_is_relocated() {
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

    #[test]
    fn relocation_member_applies_material_source_override() {
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

        run_materials_engine(params);

        let out = tmp
            .join("mod")
            .join("data")
            .join("materials")
            .join("FO76")
            .join("landscape")
            .join("ground")
            .join("temp_groundtexture01.bgsm");
        let parsed = bgsm::parse(&std::fs::read(&out).unwrap()).expect("output BGSM must parse");
        let diffuse = parsed
            .DiffuseTexture
            .trim_end_matches('\0')
            .replace('\\', "/");
        assert_eq!(diffuse, "FO76/Landscape/Ground/ForestRocks01_d.dds");
        assert!(!diffuse.to_ascii_lowercase().contains("temp_groundtexture"));
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
