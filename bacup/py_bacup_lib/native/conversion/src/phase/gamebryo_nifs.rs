use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use materials_native::bgsm;
use nif_core_native::convert_file::{ConvertFileOptions, ConvertFileReport, convert_nif_file};
use nif_core_native::model::{NifFile, NifValue};
use rayon::prelude::*;
use serde_json::Value as JsonValue;

use crate::phase::progress::ProgressReporter;
use crate::phase::{LogLevel, Phase, PhaseCtx, PhaseError, PhaseEvent, PhaseReport};

const ERROR_EXAMPLE_LIMIT: usize = 25;

struct GamebryoNifEntry {
    source_path: String,
    resolved_path: String,
}

#[derive(Default)]
struct GamebryoNifAgg {
    assets_written: u32,
    items_failed: u32,
    report_warnings: u32,
    error_messages: Vec<String>,
    warning_messages: Vec<String>,
    cancelled: bool,
}

impl GamebryoNifAgg {
    fn add(&mut self, source_path: &str, result: Result<ConvertFileReport, String>) {
        match result {
            Ok(report) => {
                self.assets_written += 1;
                self.report_warnings += report.warnings.len() as u32;
                self.warning_messages.extend(
                    report
                        .warnings
                        .into_iter()
                        .map(|warning| format!("Gamebryo NIF warning {source_path}: {warning}")),
                );
            }
            Err(error) => {
                self.items_failed += 1;
                self.error_messages
                    .push(format!("Gamebryo NIF failed {source_path}: {error}"));
            }
        }
    }

    fn merge(mut self, other: Self) -> Self {
        self.assets_written += other.assets_written;
        self.items_failed += other.items_failed;
        self.report_warnings += other.report_warnings;
        self.error_messages.extend(other.error_messages);
        self.warning_messages.extend(other.warning_messages);
        self.cancelled |= other.cancelled;
        self
    }
}

pub struct ConvertGamebryoNifsPhase;

impl Phase for ConvertGamebryoNifsPhase {
    fn name(&self) -> &'static str {
        "convert_gamebryo_nifs"
    }

    fn run(&self, ctx: &mut PhaseCtx<'_>) -> Result<PhaseReport, PhaseError> {
        let entries = parse_nif_entries(ctx.params)?;
        let material_root = normalize_material_root(
            ctx.params
                .get("material_out_rel")
                .and_then(JsonValue::as_str)
                .ok_or_else(|| PhaseError::BadParams("missing material_out_rel".into()))?,
        )?;
        let total = entries.len() as u32;
        let mod_path = ctx.mod_path;
        let source_game = ctx.run.source.as_str();
        let target_game = ctx.run.target.as_str();
        let cancel = ctx.cancel;
        let sink = ctx.run.output_sink.clone();
        let data_root = mod_path.join("data");
        let written_materials = Arc::new(Mutex::new(HashSet::new()));
        let reporter = Arc::new(ProgressReporter::new(
            "convert_gamebryo_nifs",
            total,
            ctx.run.event_tx.clone(),
        ));

        let agg = entries
            .into_par_iter()
            .fold(GamebryoNifAgg::default, |mut agg, entry| {
                if cancel.load(std::sync::atomic::Ordering::Relaxed) {
                    agg.cancelled = true;
                    return agg;
                }
                reporter.set_item(entry.source_path.clone());
                let result = convert_entry(
                    &entry,
                    mod_path,
                    source_game,
                    target_game,
                    &material_root,
                    &written_materials,
                    sink.as_deref(),
                    &data_root,
                );
                reporter.inc(1);
                agg.add(&entry.source_path, result);
                agg
            })
            .reduce(GamebryoNifAgg::default, GamebryoNifAgg::merge);
        reporter.finish();

        if agg.cancelled {
            return Err(PhaseError::Cancelled);
        }
        emit_messages(
            ctx,
            LogLevel::Warn,
            &agg.warning_messages,
            "additional Gamebryo NIF warnings",
        );
        emit_messages(
            ctx,
            LogLevel::Error,
            &agg.error_messages,
            "additional Gamebryo NIF failures",
        );
        let _ = ctx.run.event_tx.try_send(PhaseEvent::Log {
            phase: "convert_gamebryo_nifs",
            level: LogLevel::Info,
            message: format!(
                "convert_gamebryo_nifs: converted={}, failed={}",
                agg.assets_written, agg.items_failed
            ),
        });

        Ok(PhaseReport {
            assets_written: agg.assets_written,
            warnings: agg.report_warnings + agg.items_failed,
            items_failed: agg.items_failed,
            ..Default::default()
        })
    }
}

fn parse_nif_entries(params: &JsonValue) -> Result<Vec<GamebryoNifEntry>, PhaseError> {
    let entries = params
        .get("nif_paths")
        .and_then(JsonValue::as_array)
        .ok_or_else(|| PhaseError::BadParams("missing nif_paths".into()))?;
    entries
        .iter()
        .enumerate()
        .map(|(index, entry)| {
            let source_path = entry
                .get("source_path")
                .and_then(JsonValue::as_str)
                .ok_or_else(|| {
                    PhaseError::BadParams(format!(
                        "nif_paths[{index}].source_path missing or not a string"
                    ))
                })?
                .to_string();
            let resolved_path = entry
                .get("resolved_path")
                .and_then(JsonValue::as_str)
                .unwrap_or("")
                .to_string();
            Ok(GamebryoNifEntry {
                source_path,
                resolved_path,
            })
        })
        .collect()
}

fn convert_entry(
    entry: &GamebryoNifEntry,
    mod_path: &Path,
    source_game: &str,
    target_game: &str,
    material_root: &str,
    written_materials: &Mutex<HashSet<PathBuf>>,
    sink: Option<&crate::sinks::SinkSet>,
    data_root: &Path,
) -> Result<ConvertFileReport, String> {
    let source = Path::new(&entry.resolved_path);
    if !source.is_file() {
        return Err(format!("NIF not found: {}", entry.resolved_path));
    }
    let output = nif_output_path(mod_path, &entry.source_path)?;
    let result = convert_entry_staged(
        source,
        &output,
        mod_path,
        source_game,
        target_game,
        material_root,
        written_materials,
        sink,
        data_root,
    );
    if result.is_err() {
        let _ = std::fs::remove_file(&output);
    }
    result
}

fn convert_entry_staged(
    source: &Path,
    output: &Path,
    mod_path: &Path,
    source_game: &str,
    target_game: &str,
    material_root: &str,
    written_materials: &Mutex<HashSet<PathBuf>>,
    sink: Option<&crate::sinks::SinkSet>,
    data_root: &Path,
) -> Result<ConvertFileReport, String> {
    let parent = output
        .parent()
        .ok_or_else(|| format!("NIF output has no parent: {}", output.display()))?;
    std::fs::create_dir_all(parent)
        .map_err(|error| format!("create {}: {error}", parent.display()))?;
    let prefix = format!(
        ".{}.",
        output
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("gamebryo_nif")
    );
    let staged = tempfile::Builder::new()
        .prefix(&prefix)
        .suffix(".tmp")
        .tempfile_in(parent)
        .map_err(|error| format!("stage {}: {error}", output.display()))?
        .into_temp_path();
    let report = convert_nif_file(
        source,
        staged.as_ref(),
        source_game,
        target_game,
        None,
        &ConvertFileOptions::default(),
    )
    .map_err(|error| error.to_string())?;
    if !report.supported || !report.errors.is_empty() {
        return Err(if report.errors.is_empty() {
            format!("unsupported {source_game} -> {target_game} conversion")
        } else {
            report.errors.join("; ")
        });
    }
    let source_root = source_data_root(source);
    synthesize_materials(
        staged.as_ref(),
        mod_path,
        material_root,
        written_materials,
        sink,
        data_root,
        source_root.as_deref(),
    )?;
    staged
        .persist(output)
        .map_err(|error| format!("publish staged NIF {}: {}", output.display(), error.error))?;
    if let Err(error) = register_output(sink, data_root, output) {
        let _ = std::fs::remove_file(output);
        return Err(error);
    }
    Ok(report)
}

// Shader flag bits, mirroring nif_core's Skyrim/FO4 BSLightingShaderProperty.
pub(crate) const SLSF1_SPECULAR: u64 = 1 << 0;
pub(crate) const SLSF1_ENVIRONMENT_MAPPING: u64 = 1 << 7;
pub(crate) const SLSF1_OWN_EMIT: u64 = 1 << 22;
pub(crate) const SLSF2_DOUBLE_SIDED: u64 = 1 << 4;
pub(crate) const SLSF2_GLOW_MAP: u64 = 1 << 6;
pub(crate) const SLSF2_TREE_ANIM: u64 = 1 << 29;

/// Everything about one BSLightingShaderProperty that affects the FO4 material.
/// Two shapes sharing a texture set but differing in flags or scalars are
/// distinct materials and must hash differently.
#[derive(Debug, Clone, Default)]
pub(crate) struct GamebryoMaterialSpec {
    pub(crate) textures: Vec<String>,
    pub(crate) flags_1: u64,
    pub(crate) flags_2: u64,
    pub(crate) specular_strength: f32,
    pub(crate) smoothness: f32,
    pub(crate) environment_map_scale: f32,
    pub(crate) texture_clamp_mode: u64,
    pub(crate) alpha_test_ref: u8,
    pub(crate) alpha_test: bool,
    pub(crate) two_sided: bool,
    pub(crate) tree: bool,
}

pub(crate) fn material_spec_hash(spec: &GamebryoMaterialSpec) -> u64 {
    let mut hash = texture_set_hash(&spec.textures);
    for value in [
        spec.flags_1,
        spec.flags_2,
        u64::from(spec.specular_strength.to_bits()),
        u64::from(spec.smoothness.to_bits()),
        u64::from(spec.environment_map_scale.to_bits()),
        spec.texture_clamp_mode,
        u64::from(spec.alpha_test_ref),
        spec.alpha_test as u64,
        spec.two_sided as u64,
        spec.tree as u64,
    ] {
        for byte in value.to_le_bytes() {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
    }
    hash
}

fn slot(spec: &GamebryoMaterialSpec, index: usize) -> String {
    spec.textures
        .get(index)
        .map(|path| normalize_texture_slot(path))
        .unwrap_or_default()
}

fn nonempty(value: String) -> Option<String> {
    (!value.is_empty()).then_some(value)
}

/// Derive the FO4 `_s` path from the normal's path.
///
/// The texture engine keys the `_s` output off the `_n` in the normal's stem
/// (`path_with_role_suffix`), so a normal without `_n` yields no `_s` and the
/// slot must stay empty rather than naming a file nothing writes.
fn smooth_spec_path(normal: &str) -> String {
    if normal.is_empty() {
        return String::new();
    }
    let Some(stem_end) = normal.rfind('.') else {
        return String::new();
    };
    let (stem, ext) = normal.split_at(stem_end);
    let Some(index) = stem.to_ascii_lowercase().rfind("_n") else {
        return String::new();
    };
    format!("{}_s{ext}", &stem[..index])
}

/// Replace a non-cube environment map with a vanilla FO4 cubemap.
///
/// FNV ships 29 flat 2D images in the env-map slot and Skyrim 4; FO4 binds that
/// slot as a cube, so the source cannot be used. Returns true when a
/// substitution was made.
pub(crate) fn substitute_non_cube_envmap(
    spec: &mut GamebryoMaterialSpec,
    source_root: Option<&Path>,
    material_source_path: &str,
) -> bool {
    if has_flag_information(spec) && spec.flags_1 & SLSF1_ENVIRONMENT_MAPPING == 0 {
        return false;
    }
    let slot_four = slot(spec, 4);
    if slot_four.is_empty() {
        return false;
    }
    // Without a resolvable source root we cannot tell a cube from a 2D image;
    // leave the authored slot alone rather than substituting blind.
    let Some(source_root) = source_root else {
        return false;
    };

    let on_disk = source_root.join("textures").join(&slot_four);
    let is_cube = directxtex_native::read_dds_probe(&on_disk)
        .map(|probe| probe.is_cubemap)
        .unwrap_or(false);
    if is_cube {
        return false;
    }

    while spec.textures.len() < 5 {
        spec.textures.push(String::new());
    }
    match materials_native::convert::select_fo4_cubemap(material_source_path) {
        Some((cubemap, scale)) => {
            spec.textures[4] = cubemap.to_string();
            spec.environment_map_scale = scale;
        }
        None => {
            spec.flags_1 &= !SLSF1_ENVIRONMENT_MAPPING;
            spec.textures[4] = String::new();
        }
    }
    true
}

/// FNV/FO3 `BSShaderPPLightingProperty` carries no FO4-style SLSF1 bits, so the
/// converted block arrives with `Shader Flags 1 == 0`. That means "no flag
/// information", not "every feature off" — deriving booleans straight from the
/// bits would disable specular and env-mapping on every FNV material. When the
/// word is empty we fall back to evidence from the texture slots instead.
fn has_flag_information(spec: &GamebryoMaterialSpec) -> bool {
    spec.flags_1 != 0
}

pub(crate) fn fo4_bgsm_from_spec(spec: &GamebryoMaterialSpec) -> bgsm::BgsmData {
    let diffuse = slot(spec, 0);
    let normal = slot(spec, 1);
    let flags_known = has_flag_information(spec);
    let mut material = fo4_bgsm_v2(diffuse, normal.clone());

    material.header.tile_u = spec.texture_clamp_mode & 0x02 != 0;
    material.header.tile_v = spec.texture_clamp_mode & 0x01 != 0;
    material.header.alpha_test = spec.alpha_test;
    if spec.alpha_test {
        material.header.alpha_test_ref = spec.alpha_test_ref;
    }
    material.header.two_sided = spec.two_sided;
    material.SmoothSpecTexture = smooth_spec_path(&normal);
    material.SpecularEnabled = if flags_known {
        spec.flags_1 & SLSF1_SPECULAR != 0
    } else {
        // Matches the pre-existing unconditional default for Gamebryo sources;
        // per-texel strength now comes from the synthesized `_s` map.
        true
    };
    material.SpecularMult = spec.specular_strength;
    material.Smoothness = spec.smoothness;
    material.Glowmap = spec.flags_2 & SLSF2_GLOW_MAP != 0;
    material.EmitEnabled =
        spec.flags_2 & SLSF2_GLOW_MAP != 0 || (!spec.tree && spec.flags_1 & SLSF1_OWN_EMIT != 0);
    material.Tree = spec.tree;
    if material.Glowmap || material.EmitEnabled {
        material.GlowTexture = nonempty(slot(spec, 2));
    }

    let env_mapped = if flags_known {
        spec.flags_1 & SLSF1_ENVIRONMENT_MAPPING != 0
    } else {
        // No flag word: an authored slot-4 env map is the only evidence.
        !slot(spec, 4).is_empty()
    };
    if env_mapped {
        material.header.env_mapping = Some(true);
        material.header.env_mapping_mask_scale = Some(spec.environment_map_scale);
        material.EnvmapTexture = nonempty(slot(spec, 4));
    } else {
        material.header.env_mapping = Some(false);
        // Vanilla FO4 writes 1.0 here even when env mapping is off; 0.0 means
        // "fully masked out" and is wrong shader behaviour.
        material.header.env_mapping_mask_scale = Some(1.0);
    }

    // Slot 5 is the environment mask. It has no FO4 BGSM slot — the texture
    // engine already folded it into `_s.R`.
    material
}

/// Derive the extracted-data root from a source NIF path by walking up past the
/// `meshes/` component, so env-map slots can be probed on disk.
pub(crate) fn source_data_root(nif_source: &Path) -> Option<PathBuf> {
    let mut current = nif_source.parent()?;
    loop {
        if current
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.eq_ignore_ascii_case("meshes"))
        {
            return current.parent().map(Path::to_path_buf);
        }
        current = current.parent()?;
    }
}

/// Read a shader-flag word. `convert_file` writes both `Shader Flags 1` and the
/// `Shader Flags 1:FO4` alias; the alias wins when present, mirroring the
/// precedence in `convert_file::flag_names_to_bits`.
fn shader_u64(block: &nif_core_native::model::NifBlock, field: &str) -> u64 {
    let alias = format!("{field}:FO4");
    block
        .fields
        .get(alias.as_str())
        .or_else(|| block.get_field(field))
        .map(NifValue::as_i64)
        .filter(|value| *value >= 0)
        .map(|value| value as u64)
        .unwrap_or(0)
}

fn shader_f32(block: &nif_core_native::model::NifBlock, field: &str, default: f32) -> f32 {
    match block.get_field(field) {
        Some(NifValue::Float(value)) => *value as f32,
        _ => default,
    }
}

fn shader_clamp_mode(block: &nif_core_native::model::NifBlock) -> u64 {
    match block.get_field("Texture Clamp Mode") {
        Some(NifValue::UInt(value)) => *value,
        Some(NifValue::Int(value)) if *value >= 0 => *value as u64,
        Some(NifValue::String(value)) => match value.as_str() {
            "CLAMP_S_WRAP_T" => 1,
            "WRAP_S_CLAMP_T" => 2,
            "WRAP_S_WRAP_T" => 3,
            _ => 0,
        },
        _ => 0,
    }
}

fn alpha_settings_by_shader(nif: &NifFile) -> HashMap<usize, (bool, u8)> {
    let mut settings = HashMap::new();
    for shape in nif.blocks.iter().filter(|block| {
        matches!(
            block.type_name.as_str(),
            "BSTriShape" | "BSSubIndexTriShape"
        )
    }) {
        let Some(shader_id) = shape
            .get_field("Shader Property")
            .map(NifValue::as_i64)
            .filter(|id| *id >= 0)
            .map(|id| id as usize)
        else {
            continue;
        };
        let Some(alpha) = shape
            .get_field("Alpha Property")
            .map(NifValue::as_i64)
            .filter(|id| *id >= 0)
            .and_then(|id| nif.get_block(id as usize))
            .filter(|block| block.type_name == "NiAlphaProperty")
        else {
            continue;
        };
        let alpha_test = alpha
            .get_field("Flags")
            .map(NifValue::as_i64)
            .is_some_and(|flags| flags >= 0 && flags as u64 & (1 << 9) != 0);
        let threshold = alpha
            .get_field("Threshold")
            .map(NifValue::as_i64)
            .filter(|value| *value >= 0)
            .map(|value| value.min(u8::MAX as i64) as u8)
            .unwrap_or(128);
        if alpha_test || !settings.contains_key(&shader_id) {
            settings.insert(shader_id, (alpha_test, threshold));
        }
    }
    settings
}

fn synthesize_materials(
    nif_path: &Path,
    mod_path: &Path,
    material_root: &str,
    written_materials: &Mutex<HashSet<PathBuf>>,
    sink: Option<&crate::sinks::SinkSet>,
    data_root: &Path,
    source_root: Option<&Path>,
) -> Result<(), String> {
    let mut nif = NifFile::load(nif_path.to_path_buf()).map_err(|error| error.to_string())?;
    let texture_sets = nif
        .blocks
        .iter()
        .filter(|block| block.type_name == "BSShaderTextureSet")
        .filter_map(|block| {
            let Some(NifValue::Array(values)) = block.get_field("Textures") else {
                return None;
            };
            let textures = values
                .iter()
                .map(|value| match value {
                    NifValue::String(path) => path.clone(),
                    _ => String::new(),
                })
                .collect::<Vec<_>>();
            Some((block.block_id, textures))
        })
        .collect::<HashMap<_, _>>();
    let alpha_settings = alpha_settings_by_shader(&nif);

    let mut specs: HashMap<usize, GamebryoMaterialSpec> = HashMap::new();
    for block in nif.blocks.iter() {
        if block.type_name != "BSLightingShaderProperty" {
            continue;
        }
        let Some(texture_set_id) = block
            .get_field("Texture Set")
            .map(NifValue::as_i64)
            .filter(|id| *id >= 0)
            .map(|id| id as usize)
        else {
            continue;
        };
        let Some(textures) = texture_sets.get(&texture_set_id) else {
            continue;
        };
        let flags_2 = shader_u64(block, "Shader Flags 2");
        let (alpha_test, alpha_test_ref) = alpha_settings
            .get(&block.block_id)
            .copied()
            .unwrap_or((false, 128));
        let mut spec = GamebryoMaterialSpec {
            textures: textures.clone(),
            flags_1: shader_u64(block, "Shader Flags 1"),
            flags_2,
            specular_strength: shader_f32(block, "Specular Strength", 1.0),
            smoothness: shader_f32(block, "Smoothness", 0.5),
            environment_map_scale: shader_f32(block, "Environment Map Scale", 1.0),
            texture_clamp_mode: shader_clamp_mode(block),
            alpha_test_ref,
            alpha_test,
            two_sided: flags_2 & SLSF2_DOUBLE_SIDED != 0,
            tree: flags_2 & SLSF2_TREE_ANIM != 0,
        };
        // Must run before the hash so substituted materials do not collide with
        // unsubstituted ones sharing the same texture set.
        substitute_non_cube_envmap(&mut spec, source_root, material_root);
        specs.insert(texture_set_id, spec);
    }

    let mut material_by_texture_set = HashMap::new();
    for (texture_set_id, spec) in &specs {
        let filename = format!("gamebryo_{:016x}.bgsm", material_spec_hash(spec));
        let material_ref = format!("{}\\{filename}", material_root.replace('/', "\\"));
        let material_path = mod_path.join("data").join(material_root).join(&filename);
        write_material_once(&material_path, spec, written_materials, sink, data_root)?;
        material_by_texture_set.insert(*texture_set_id, material_ref);
    }

    for block in nif.blocks.iter_mut() {
        if block.type_name != "BSLightingShaderProperty" {
            continue;
        }
        let Some(texture_set_id) = block
            .get_field("Texture Set")
            .map(NifValue::as_i64)
            .filter(|id| *id >= 0)
            .map(|id| id as usize)
        else {
            continue;
        };
        if let Some(material_ref) = material_by_texture_set.get(&texture_set_id) {
            block.set_field("Name", NifValue::String(material_ref.clone()));
        }
    }
    nif.save(Some(nif_path.to_path_buf()))
        .map_err(|error| error.to_string())
}

fn write_material_once(
    path: &Path,
    spec: &GamebryoMaterialSpec,
    written_materials: &Mutex<HashSet<PathBuf>>,
    sink: Option<&crate::sinks::SinkSet>,
    data_root: &Path,
) -> Result<(), String> {
    let mut written = written_materials
        .lock()
        .map_err(|_| "material write lock poisoned".to_string())?;
    if written.contains(path) {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("create {}: {error}", parent.display()))?;
    }
    let material = fo4_bgsm_from_spec(spec);
    write_bytes_atomic(path, &bgsm::write(&material))
        .map_err(|error| format!("write {}: {error}", path.display()))?;
    if let Err(error) = register_output(sink, data_root, path) {
        let _ = std::fs::remove_file(path);
        return Err(error);
    }
    written.insert(path.to_path_buf());
    Ok(())
}

fn write_bytes_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let parent = path.parent().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("output has no parent: {}", path.display()),
        )
    })?;
    std::fs::create_dir_all(parent)?;
    let mut staged = tempfile::NamedTempFile::new_in(parent)?;
    std::io::Write::write_all(&mut staged, bytes)?;
    std::io::Write::flush(staged.as_file_mut())?;
    staged.as_file().sync_all()?;
    staged
        .persist(path)
        .map_err(|error| error.error)
        .map(|_| ())
}

fn fo4_bgsm_v2(diffuse: String, normal: String) -> bgsm::BgsmData {
    let mut material = bgsm::BgsmData::default();
    material.header.signature = bgsm::BGSM_SIGNATURE;
    material.header.version = 2;
    material.header.u_scale = 1.0;
    material.header.v_scale = 1.0;
    material.header.alpha = 1.0;
    material.header.alpha_blend_mode1 = 6;
    material.header.alpha_blend_mode2 = 7;
    material.header.alpha_test_ref = 128;
    material.header.zbuffer_write = true;
    material.header.zbuffer_test = true;
    material.header.env_mapping = Some(false);
    material.header.env_mapping_mask_scale = Some(1.0);
    material.DiffuseTexture = diffuse;
    material.NormalTexture = normal;
    material.SpecularEnabled = true;
    material.SpecularColor = [1.0, 1.0, 1.0];
    material.SpecularMult = 1.0;
    material.Smoothness = 0.5;
    material.FresnelPower = 5.0;
    material.WetnessControlSpecScale = -1.0;
    material.WetnessControlSpecPowerScale = -1.0;
    material.WetnessControlSpecMinvar = -1.0;
    material.WetnessControlEnvMapScale = Some(-1.0);
    material.WetnessControlFresnelPower = -1.0;
    material.WetnessControlMetalness = -1.0;
    material.EmittanceMult = 1.0;
    material.BackLighting = Some(false);
    material.ReceiveShadows = true;
    material.CastShadows = true;
    material.GrayscaleToPaletteScale = 1.0;
    material
}

fn register_output(
    sink: Option<&crate::sinks::SinkSet>,
    data_root: &Path,
    output: &Path,
) -> Result<(), String> {
    let Some(sink) = sink else { return Ok(()) };
    let relative = output
        .strip_prefix(data_root)
        .map_err(|_| format!("output is outside data root: {}", output.display()))?
        .to_string_lossy()
        .replace('\\', "/");
    sink.add_existing_file(&relative, output)
        .map(|_| ())
        .map_err(|error| format!("register {relative}: {error}"))
}

fn normalize_texture_slot(path: &str) -> String {
    let normalized = path
        .trim_end_matches('\0')
        .trim()
        .replace('\\', "/")
        .trim_start_matches('/')
        .to_string();
    if normalized
        .get(..9)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("textures/"))
    {
        normalized[9..].to_string()
    } else {
        normalized
    }
}

fn texture_set_hash(textures: &[String]) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    for texture in textures {
        for byte in normalize_texture_slot(texture).to_ascii_lowercase().bytes() {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
        hash ^= 0xff;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

fn normalize_material_root(path: &str) -> Result<String, PhaseError> {
    let mut parts = safe_relative_parts(path)?;
    if parts
        .first()
        .is_some_and(|part| part.eq_ignore_ascii_case("data"))
    {
        parts.remove(0);
    }
    if parts
        .first()
        .is_some_and(|part| part.eq_ignore_ascii_case("materials"))
    {
        parts[0] = "Materials".to_string();
    } else {
        parts.insert(0, "Materials".to_string());
    }
    if parts.len() == 1 {
        return Err(PhaseError::BadParams("material_out_rel is empty".into()));
    }
    Ok(parts.join("/"))
}

fn nif_output_path(mod_path: &Path, source_path: &str) -> Result<PathBuf, String> {
    let mut parts = safe_relative_parts(source_path).map_err(|error| error.to_string())?;
    for root in ["data", "meshes"] {
        if parts
            .first()
            .is_some_and(|part| part.eq_ignore_ascii_case(root))
        {
            parts.remove(0);
        }
    }
    if parts.first().is_some_and(|part| is_game_prefix(part)) {
        parts.remove(0);
    }
    if parts.is_empty() {
        return Err("source_path does not contain a mesh-relative path".to_string());
    }
    Ok(parts
        .into_iter()
        .fold(mod_path.join("data").join("Meshes"), |path, part| {
            path.join(part)
        }))
}

fn safe_relative_parts(path: &str) -> Result<Vec<String>, PhaseError> {
    let mut parts = Vec::new();
    for part in path.replace('\\', "/").split('/') {
        let part = part.trim();
        if part.is_empty() || part == "." {
            continue;
        }
        if part == ".." || part.contains(':') {
            return Err(PhaseError::BadParams(format!(
                "unsafe relative path: {path}"
            )));
        }
        parts.push(part.to_string());
    }
    Ok(parts)
}

fn is_game_prefix(value: &str) -> bool {
    matches!(
        value.to_ascii_lowercase().as_str(),
        "fnv" | "fo3" | "fo4" | "fo76" | "oblivion" | "skyrim" | "skyrimse" | "starfield"
    )
}

fn emit_messages(ctx: &PhaseCtx<'_>, level: LogLevel, messages: &[String], overflow_label: &str) {
    for message in messages.iter().take(ERROR_EXAMPLE_LIMIT) {
        let _ = ctx.run.event_tx.try_send(PhaseEvent::Log {
            phase: "convert_gamebryo_nifs",
            level,
            message: message.clone(),
        });
    }
    if messages.len() > ERROR_EXAMPLE_LIMIT {
        let _ = ctx.run.event_tx.try_send(PhaseEvent::Log {
            phase: "convert_gamebryo_nifs",
            level,
            message: format!("{} {overflow_label}", messages.len() - ERROR_EXAMPLE_LIMIT),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::run::{RunConfig, RunError, RunParams, create_run, drop_run, with_run};
    use crate::sinks::{Ba2ShardWriter, LooseSink, SinkSet, TerrainSidecarSink};
    use crate::translator::Game;
    use materials_native::bgsm;
    use nif_core_native::model::{NifFile, NifValue};
    use std::sync::atomic::AtomicBool;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn converts_real_fnv_static_and_synthesizes_bgsm() {
        let fixture = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("src/test_fixtures/gamebryo_nifs/cratelarge01.nif");
        let temp = std::env::temp_dir().join(format!(
            "conversion_gamebryo_nifs_{}_{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mod_path = temp.join("mod");
        let run_id = create_run(RunParams {
            source: Game::Fnv,
            target: Game::Fo4,
            source_handle_id: 9999,
            target_handle_id: 9998,
            master_handle_ids: vec![],
            config: RunConfig {
                output_plugin_name: "Output.esp".into(),
                ..Default::default()
            },
        })
        .unwrap();
        let sink = Arc::new(SinkSet {
            ba2: Some(Ba2ShardWriter::new(temp.join("spill")).unwrap()),
            loose: LooseSink {
                enabled: false,
                mod_root: mod_path.clone(),
            },
            terrain: TerrainSidecarSink::default(),
        });
        with_run(run_id, |run| -> Result<(), RunError> {
            run.output_sink = Some(Arc::clone(&sink));
            Ok(())
        })
        .unwrap();

        let report = with_run(run_id, |run| -> Result<PhaseReport, RunError> {
            let cancel = AtomicBool::new(false);
            let params = serde_json::json!({
                "nif_paths": [{
                    "source_path": "Meshes/fnv/clutter/crate/cratelarge01.nif",
                    "resolved_path": fixture.to_string_lossy(),
                }],
                "material_out_rel": "materials/Test/gamebryo",
            });
            let mut ctx = PhaseCtx {
                run,
                mod_path: &mod_path,
                source_extracted_dir: std::path::Path::new(""),
                target_extracted_dir: None,
                target_data_dir: None,
                params: &params,
                cancel: &cancel,
            };
            ConvertGamebryoNifsPhase
                .run(&mut ctx)
                .map_err(|error| RunError::InvalidConfig(error.to_string()))
        })
        .unwrap();

        assert_eq!(report.assets_written, 1);
        assert_eq!(report.items_failed, 0);
        let output_nif = mod_path.join("data/Meshes/clutter/crate/cratelarge01.nif");
        let converted = NifFile::load(output_nif).unwrap();
        assert!(converted.find_blocks("BSTriShape").len() >= 1);
        assert!(converted.find_blocks("NiTriStrips").is_empty());

        let material_dir = mod_path.join("data/Materials/Test/gamebryo");
        let materials = std::fs::read_dir(&material_dir)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect::<Vec<_>>();
        assert_eq!(materials.len(), 1);
        let material = bgsm::parse(&std::fs::read(&materials[0]).unwrap()).unwrap();
        assert_eq!(material.DiffuseTexture, "clutter/crate/CrateLarge01.dds\0");
        assert_eq!(material.NormalTexture, "clutter/crate/CrateLarge01_n.dds\0");
        assert_eq!(material.header.u_scale, 1.0);
        assert_eq!(material.header.v_scale, 1.0);
        assert_eq!(material.header.alpha, 1.0);
        assert!(material.header.zbuffer_write);
        assert!(material.header.zbuffer_test);
        assert_eq!(material.header.alpha_test_ref, 128);
        assert!(material.SpecularEnabled);
        assert_eq!(material.SpecularColor, [1.0, 1.0, 1.0]);
        assert_eq!(material.SpecularMult, 1.0);
        assert_eq!(material.Smoothness, 0.5);
        assert_eq!(material.FresnelPower, 5.0);
        assert!(material.ReceiveShadows);
        assert!(material.CastShadows);

        let material_names = converted
            .blocks
            .iter()
            .filter(|block| block.type_name == "BSLightingShaderProperty")
            .filter_map(|block| match block.get_field("Name") {
                Some(NifValue::String(name)) => Some(name.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(material_names.len(), 1);
        assert!(material_names[0].starts_with("Materials\\Test\\gamebryo\\"));
        assert!(material_names[0].ends_with(".bgsm"));
        let streamed = sink.ba2.as_ref().unwrap().streamed_rel_paths();
        assert_eq!(streamed.len(), 2);
        assert!(streamed.contains(&"meshes/clutter/crate/cratelarge01.nif".to_string()));
        assert!(
            streamed
                .iter()
                .any(|path| path.starts_with("materials/test/gamebryo/gamebryo_"))
        );

        drop_run(run_id).unwrap();
        let _ = std::fs::remove_dir_all(temp);
    }

    #[test]
    fn failed_material_write_does_not_poison_dedup_state() {
        let temp = std::env::temp_dir().join(format!(
            "conversion_gamebryo_material_retry_{}_{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let data_root = temp.join("data");
        let blocked_parent = data_root.join("Materials/Test");
        std::fs::create_dir_all(blocked_parent.parent().unwrap()).unwrap();
        std::fs::write(&blocked_parent, b"not a directory").unwrap();
        let material_path = blocked_parent.join("retry.bgsm");
        let spec = GamebryoMaterialSpec {
            textures: vec!["Textures/Test/d.dds".to_string()],
            ..GamebryoMaterialSpec::default()
        };
        let written = Mutex::new(HashSet::new());

        assert!(write_material_once(&material_path, &spec, &written, None, &data_root).is_err());
        std::fs::remove_file(&blocked_parent).unwrap();
        write_material_once(&material_path, &spec, &written, None, &data_root).unwrap();
        assert!(material_path.is_file());
        assert_eq!(written.lock().unwrap().len(), 1);

        let _ = std::fs::remove_dir_all(temp);
    }

    fn write_flat_2d_dds(path: &Path) {
        let mut image = directxtex_native::ScratchImage::default();
        image
            .initialize_2d(
                directxtex_native::DXGI_FORMAT_R8G8B8A8_UNORM,
                8,
                8,
                1,
                1,
                directxtex_native::CP_FLAGS_NONE,
            )
            .unwrap();
        let bytes = image
            .save_dds(directxtex_native::DDS_FLAGS_NONE)
            .unwrap()
            .buffer()
            .to_vec();
        std::fs::write(path, bytes).unwrap();
    }

    #[test]
    fn material_spec_carries_slots_flags_and_scalars() {
        let spec = GamebryoMaterialSpec {
            textures: vec![
                "Textures/Clutter/Crate/CrateLarge01.dds".to_string(),
                "Textures/Clutter/Crate/CrateLarge01_n.dds".to_string(),
                String::new(),
                String::new(),
                "Textures/Cubemaps/MetalChrome01Cube_e.dds".to_string(),
                "Textures/Clutter/Crate/CrateLarge01_m.dds".to_string(),
            ],
            flags_1: SLSF1_SPECULAR | SLSF1_ENVIRONMENT_MAPPING,
            flags_2: 0,
            specular_strength: 0.75,
            smoothness: 0.42,
            environment_map_scale: 0.30,
            ..GamebryoMaterialSpec::default()
        };

        let material = fo4_bgsm_from_spec(&spec);

        assert_eq!(material.DiffuseTexture, "Clutter/Crate/CrateLarge01.dds");
        assert_eq!(material.NormalTexture, "Clutter/Crate/CrateLarge01_n.dds");
        assert_eq!(
            material.SmoothSpecTexture, "Clutter/Crate/CrateLarge01_s.dds",
            "the _s map synthesized by the texture engine"
        );
        assert_eq!(
            material.EnvmapTexture.as_deref(),
            Some("Cubemaps/MetalChrome01Cube_e.dds")
        );
        assert!(material.SpecularEnabled);
        assert!((material.SpecularMult - 0.75).abs() < 1e-6);
        assert!((material.Smoothness - 0.42).abs() < 1e-6);
        assert_eq!(material.header.env_mapping, Some(true));
        assert!((material.header.env_mapping_mask_scale.unwrap() - 0.30).abs() < 1e-6);
        assert!(
            material.GlowTexture.is_none(),
            "slot 5 is the env mask, not glow"
        );
    }

    #[test]
    fn material_spec_without_env_mapping_flag_leaves_cubemap_empty() {
        let spec = GamebryoMaterialSpec {
            textures: vec![
                "Textures/Landscape/Rocks/Rock01.dds".to_string(),
                "Textures/Landscape/Rocks/Rock01_n.dds".to_string(),
            ],
            flags_1: SLSF1_SPECULAR,
            flags_2: 0,
            specular_strength: 1.0,
            smoothness: 0.1,
            environment_map_scale: 1.0,
            ..GamebryoMaterialSpec::default()
        };

        let material = fo4_bgsm_from_spec(&spec);

        assert_eq!(material.header.env_mapping, Some(false));
        assert!(material.EnvmapTexture.is_none());
        assert_eq!(
            material.header.env_mapping_mask_scale,
            Some(1.0),
            "vanilla FO4 writes 1.0 even when env mapping is off"
        );
    }

    #[test]
    fn material_spec_maps_slot_two_to_glow_when_glow_mapped() {
        let spec = GamebryoMaterialSpec {
            textures: vec![
                "Textures/Signs/NeonSign.dds".to_string(),
                "Textures/Signs/NeonSign_n.dds".to_string(),
                "Textures/Signs/NeonSign_g.dds".to_string(),
            ],
            flags_1: 0,
            flags_2: SLSF2_GLOW_MAP,
            specular_strength: 1.0,
            smoothness: 0.1,
            environment_map_scale: 1.0,
            ..GamebryoMaterialSpec::default()
        };

        let material = fo4_bgsm_from_spec(&spec);

        assert_eq!(
            material.GlowTexture.as_deref(),
            Some("Signs/NeonSign_g.dds")
        );
        assert!(material.Glowmap);
    }

    #[test]
    fn tall_grass_material_preserves_fo4_render_state() {
        let spec = GamebryoMaterialSpec {
            textures: vec!["Textures/Landscape/Grass/GrassWastelandComp01.dds".to_string()],
            flags_1: SLSF1_OWN_EMIT,
            flags_2: SLSF2_DOUBLE_SIDED | SLSF2_TREE_ANIM,
            smoothness: 0.282,
            texture_clamp_mode: 3,
            alpha_test_ref: 100,
            alpha_test: true,
            two_sided: true,
            tree: true,
            ..GamebryoMaterialSpec::default()
        };

        let material = fo4_bgsm_from_spec(&spec);

        assert!(material.header.tile_u);
        assert!(material.header.tile_v);
        assert!(material.header.alpha_test);
        assert_eq!(material.header.alpha_test_ref, 100);
        assert!(material.header.two_sided);
        assert!(material.Tree);
        assert!(!material.EmitEnabled);
        assert!((material.Smoothness - 0.282).abs() < 1e-6);
    }

    #[test]
    fn alpha_property_drives_synthesized_material_threshold() {
        let mut nif = NifFile::new("fo4");
        let shader_id = nif.add_block("BSLightingShaderProperty", None);
        let alpha_id = nif.add_block(
            "NiAlphaProperty",
            Some(indexmap::IndexMap::from([
                ("Flags".to_string(), NifValue::UInt(4844)),
                ("Threshold".to_string(), NifValue::UInt(100)),
            ])),
        );
        nif.add_block(
            "BSTriShape",
            Some(indexmap::IndexMap::from([
                (
                    "Shader Property".to_string(),
                    NifValue::Ref(shader_id as i32),
                ),
                ("Alpha Property".to_string(), NifValue::Ref(alpha_id as i32)),
            ])),
        );

        assert_eq!(
            alpha_settings_by_shader(&nif).get(&shader_id),
            Some(&(true, 100))
        );
    }

    #[test]
    fn material_hash_separates_identical_textures_with_different_flags() {
        let base = GamebryoMaterialSpec {
            textures: vec!["Textures/A.dds".to_string(), "Textures/A_n.dds".to_string()],
            flags_1: 0,
            flags_2: 0,
            specular_strength: 1.0,
            smoothness: 0.5,
            environment_map_scale: 1.0,
            ..GamebryoMaterialSpec::default()
        };
        let mut env_mapped = base.clone();
        env_mapped.flags_1 = SLSF1_ENVIRONMENT_MAPPING;

        assert_ne!(
            material_spec_hash(&base),
            material_spec_hash(&env_mapped),
            "two materials sharing a texture set but differing in flags must not collide"
        );
    }

    #[test]
    fn non_cube_envmap_slot_is_substituted_with_a_vanilla_cubemap() {
        let tmp = tempfile::tempdir().unwrap();
        let source_root = tmp.path();
        let envmap_dir = source_root.join("textures/architecture/novac");
        std::fs::create_dir_all(&envmap_dir).unwrap();
        write_flat_2d_dds(&envmap_dir.join("motel_window_e.dds"));

        let mut spec = GamebryoMaterialSpec {
            textures: vec![
                "Textures/Architecture/Novac/motel_window.dds".to_string(),
                "Textures/Architecture/Novac/motel_window_n.dds".to_string(),
                String::new(),
                String::new(),
                "Textures/Architecture/Novac/motel_window_e.dds".to_string(),
            ],
            flags_1: SLSF1_ENVIRONMENT_MAPPING,
            flags_2: 0,
            specular_strength: 1.0,
            smoothness: 0.1,
            environment_map_scale: 1.0,
            ..GamebryoMaterialSpec::default()
        };

        let substituted = substitute_non_cube_envmap(
            &mut spec,
            Some(source_root),
            "Materials/Weapons/Novac/motel.bgsm",
        );

        assert!(substituted, "a 2D source must be reported as substituted");
        assert!(
            spec.textures[4].starts_with("Shared/Cubemaps/"),
            "slot 4 must now name a vanilla FO4 cubemap, got {:?}",
            spec.textures[4]
        );
    }

    #[test]
    fn unresolvable_source_root_leaves_the_authored_envmap_alone() {
        let mut spec = GamebryoMaterialSpec {
            textures: vec![
                "Textures/Gone/thing.dds".to_string(),
                "Textures/Gone/thing_n.dds".to_string(),
                String::new(),
                String::new(),
                "Textures/Gone/thing_e.dds".to_string(),
            ],
            flags_1: SLSF1_ENVIRONMENT_MAPPING,
            flags_2: 0,
            specular_strength: 1.0,
            smoothness: 0.1,
            environment_map_scale: 1.0,
            ..GamebryoMaterialSpec::default()
        };

        assert!(!substitute_non_cube_envmap(
            &mut spec,
            None,
            "Materials/Gone/thing.bgsm"
        ));
        assert_eq!(spec.textures[4], "Textures/Gone/thing_e.dds");
    }

    #[test]
    fn empty_flag_word_means_unknown_not_everything_off() {
        // Real FNV statics convert with Shader Flags 1 == 0 (verified against
        // the CrateLarge01 fixture). Deriving booleans straight from the bits
        // would disable specular on every FNV material.
        let spec = GamebryoMaterialSpec {
            textures: vec![
                "Textures/Clutter/Crate/CrateLarge01.dds".to_string(),
                "Textures/Clutter/Crate/CrateLarge01_n.dds".to_string(),
            ],
            flags_1: 0,
            flags_2: 0,
            specular_strength: 1.0,
            smoothness: 0.5,
            environment_map_scale: 1.0,
            ..GamebryoMaterialSpec::default()
        };

        let material = fo4_bgsm_from_spec(&spec);

        assert!(
            material.SpecularEnabled,
            "specular must not silently vanish"
        );
        assert_eq!(
            material.SmoothSpecTexture,
            "Clutter/Crate/CrateLarge01_s.dds"
        );
        assert_eq!(
            material.header.env_mapping,
            Some(false),
            "no slot-4 evidence means no env mapping"
        );
    }

    #[test]
    fn empty_flag_word_still_honours_an_authored_env_map_slot() {
        let spec = GamebryoMaterialSpec {
            textures: vec![
                "Textures/Strip/Metal01.dds".to_string(),
                "Textures/Strip/Metal01_n.dds".to_string(),
                String::new(),
                String::new(),
                "Shared/Cubemaps/MetalChrome01Cube_e.dds".to_string(),
            ],
            flags_1: 0,
            flags_2: 0,
            specular_strength: 1.0,
            smoothness: 0.5,
            environment_map_scale: 0.5,
            ..GamebryoMaterialSpec::default()
        };

        let material = fo4_bgsm_from_spec(&spec);

        assert_eq!(material.header.env_mapping, Some(true));
        assert_eq!(
            material.EnvmapTexture.as_deref(),
            Some("Shared/Cubemaps/MetalChrome01Cube_e.dds")
        );
    }

    #[test]
    fn source_data_root_walks_up_past_meshes() {
        let root = source_data_root(Path::new("X:/extracted/fnv/meshes/clutter/crate.nif"));
        assert_eq!(root, Some(PathBuf::from("X:/extracted/fnv")));
        assert_eq!(source_data_root(Path::new("crate.nif")), None);
    }

    #[test]
    fn failed_material_synthesis_leaves_no_nif_output_or_sink_entry() {
        let fixture = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("src/test_fixtures/gamebryo_nifs/cratelarge01.nif");
        let temp = std::env::temp_dir().join(format!(
            "conversion_gamebryo_nif_failure_cleanup_{}_{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mod_path = temp.join("mod");
        let data_root = mod_path.join("data");
        let blocked_material_root = data_root.join("Materials/Test");
        std::fs::create_dir_all(blocked_material_root.parent().unwrap()).unwrap();
        std::fs::write(&blocked_material_root, b"not a directory").unwrap();
        let sink = SinkSet {
            ba2: Some(Ba2ShardWriter::new(temp.join("spill")).unwrap()),
            loose: LooseSink {
                enabled: false,
                mod_root: mod_path.clone(),
            },
            terrain: TerrainSidecarSink::default(),
        };
        let entry = GamebryoNifEntry {
            source_path: "Meshes/fnv/clutter/crate/cratelarge01.nif".to_string(),
            resolved_path: fixture.to_string_lossy().to_string(),
        };

        let result = convert_entry(
            &entry,
            &mod_path,
            "fnv",
            "fo4",
            "Materials/Test/gamebryo",
            &Mutex::new(HashSet::new()),
            Some(&sink),
            &data_root,
        );

        assert!(result.is_err());
        let output_dir = data_root.join("Meshes/clutter/crate");
        assert!(!output_dir.join("cratelarge01.nif").exists());
        if output_dir.is_dir() {
            assert_eq!(std::fs::read_dir(&output_dir).unwrap().count(), 0);
        }
        assert!(
            sink.ba2
                .as_ref()
                .unwrap()
                .streamed_rel_paths()
                .iter()
                .all(|path| !path.ends_with(".nif"))
        );

        let _ = std::fs::remove_dir_all(temp);
    }
}
