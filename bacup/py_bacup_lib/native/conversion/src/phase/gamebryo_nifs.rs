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
    synthesize_materials(
        staged.as_ref(),
        mod_path,
        material_root,
        written_materials,
        sink,
        data_root,
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

fn synthesize_materials(
    nif_path: &Path,
    mod_path: &Path,
    material_root: &str,
    written_materials: &Mutex<HashSet<PathBuf>>,
    sink: Option<&crate::sinks::SinkSet>,
    data_root: &Path,
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
        .collect::<Vec<_>>();

    let mut material_by_texture_set = HashMap::new();
    for (texture_set_id, textures) in texture_sets {
        let filename = format!("gamebryo_{:016x}.bgsm", texture_set_hash(&textures));
        let material_ref = format!("{}\\{filename}", material_root.replace('/', "\\"));
        let material_path = mod_path.join("data").join(material_root).join(&filename);
        write_material_once(
            &material_path,
            &textures,
            written_materials,
            sink,
            data_root,
        )?;
        material_by_texture_set.insert(texture_set_id, material_ref);
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
    textures: &[String],
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
    let diffuse = textures
        .first()
        .map_or_else(String::new, |path| normalize_texture_slot(path));
    let normal = textures
        .get(1)
        .map_or_else(String::new, |path| normalize_texture_slot(path));
    let material = fo4_bgsm_v2(diffuse, normal);
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
        let textures = vec!["Textures/Test/d.dds".to_string()];
        let written = Mutex::new(HashSet::new());

        assert!(
            write_material_once(&material_path, &textures, &written, None, &data_root).is_err()
        );
        std::fs::remove_file(&blocked_parent).unwrap();
        write_material_once(&material_path, &textures, &written, None, &data_root).unwrap();
        assert!(material_path.is_file());
        assert_eq!(written.lock().unwrap().len(), 1);

        let _ = std::fs::remove_dir_all(temp);
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
