use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use nif_core_native::model::NifFile;
use rayon::prelude::*;
use serde_json::Value as JsonValue;

use crate::phase::progress::ProgressReporter;
use crate::phase::{LogLevel, Phase, PhaseCtx, PhaseError, PhaseEvent, PhaseReport};

const ERROR_EXAMPLE_LIMIT: usize = 25;

struct TextureEntry {
    source_path: String,
    resolved_path: String,
}

struct NifEntry {
    source_path: String,
    resolved_path: String,
}

#[derive(Default)]
struct NifDiscovery {
    textures: Vec<TextureEntry>,
    failures: u32,
    warning_messages: Vec<String>,
}

impl NifDiscovery {
    fn fail(&mut self, message: String) {
        self.failures += 1;
        if self.warning_messages.len() < ERROR_EXAMPLE_LIMIT {
            self.warning_messages.push(message);
        }
    }
}

struct PreparedTextureEntry {
    source_path: String,
    resolved_path: String,
    output: Result<PathBuf, String>,
}

#[derive(Default)]
struct TextureAgg {
    copied: u32,
    failed: u32,
    sink_failures: u32,
    error_messages: Vec<String>,
    cancelled: bool,
}

impl TextureAgg {
    fn fail(&mut self, message: String) {
        self.failed += 1;
        if self.error_messages.len() < ERROR_EXAMPLE_LIMIT {
            self.error_messages.push(message);
        }
    }

    fn merge(mut self, other: Self) -> Self {
        self.copied += other.copied;
        self.failed += other.failed;
        self.sink_failures += other.sink_failures;
        for message in other.error_messages {
            if self.error_messages.len() < ERROR_EXAMPLE_LIMIT {
                self.error_messages.push(message);
            }
        }
        self.cancelled |= other.cancelled;
        self
    }
}

pub struct CopyTexturesPhase;

impl Phase for CopyTexturesPhase {
    fn name(&self) -> &'static str {
        "copy_textures"
    }

    fn run(&self, ctx: &mut PhaseCtx<'_>) -> Result<PhaseReport, PhaseError> {
        let mut entries = parse_texture_entries(ctx.params)?;
        let NifDiscovery {
            textures,
            failures: discovery_failures,
            warning_messages: discovery_warnings,
        } = discover_nif_textures(parse_nif_entries(ctx.params)?);
        entries.extend(textures);
        let (entries, collision_messages) = prepare_texture_entries(entries, ctx.mod_path);
        emit_collision_warnings(ctx, &collision_messages);
        emit_discovery_warnings(ctx, discovery_failures, &discovery_warnings);
        let total = entries.len() as u32;
        let mod_path = ctx.mod_path;
        let data_root = mod_path.join("data");
        let cancel = ctx.cancel;
        let sink = ctx.run.output_sink.clone();
        let reporter = Arc::new(ProgressReporter::new(
            "copy_textures",
            total,
            ctx.run.event_tx.clone(),
        ));

        let agg = entries
            .into_par_iter()
            .fold(TextureAgg::default, |mut agg, entry| {
                if cancel.load(std::sync::atomic::Ordering::Relaxed) {
                    agg.cancelled = true;
                    return agg;
                }
                reporter.set_item(entry.source_path.clone());
                let source = Path::new(&entry.resolved_path);
                match entry.output {
                    Err(error) => agg.fail(error),
                    Ok(output) if !source.is_file() => {
                        let _ = std::fs::remove_file(&output);
                        agg.fail(format!("Texture not found: {}", entry.source_path));
                    }
                    Ok(output) => match copy_texture(source, &output) {
                        Ok(copied) => {
                            agg.copied += u32::from(copied);
                            if !register_with_sink(sink.as_deref(), &data_root, &output) {
                                let _ = std::fs::remove_file(&output);
                                agg.sink_failures += 1;
                            }
                        }
                        Err(error) => {
                            let _ = std::fs::remove_file(&output);
                            agg.fail(format!("Texture failed: {}: {error}", entry.source_path))
                        }
                    },
                }
                reporter.inc(1);
                agg
            })
            .reduce(TextureAgg::default, TextureAgg::merge);
        reporter.finish();

        for message in &agg.error_messages {
            let _ = ctx.run.event_tx.try_send(PhaseEvent::Log {
                phase: "copy_textures",
                level: LogLevel::Error,
                message: message.clone(),
            });
        }
        let omitted = agg.failed as usize - agg.error_messages.len();
        if omitted > 0 {
            let _ = ctx.run.event_tx.try_send(PhaseEvent::Log {
                phase: "copy_textures",
                level: LogLevel::Error,
                message: format!("{omitted} additional texture files failed"),
            });
        }
        if agg.cancelled {
            let _ = ctx.run.event_tx.try_send(PhaseEvent::Log {
                phase: "copy_textures",
                level: LogLevel::Warn,
                message: "copy_textures: cancelled".to_string(),
            });
        }
        let _ = ctx.run.event_tx.try_send(PhaseEvent::Log {
            phase: "copy_textures",
            level: LogLevel::Info,
            message: format!(
                "copy_textures: copied={}, failed={}",
                agg.copied,
                agg.failed + discovery_failures
            ),
        });

        Ok(PhaseReport {
            assets_written: agg.copied,
            warnings: agg.failed + discovery_failures,
            items_failed: agg.failed + agg.sink_failures + discovery_failures,
            ..Default::default()
        })
    }
}

/// Texture files a source NIF names but no record points at.
///
/// Gamebryo source trees lean on this heavily: FNV/FO3 and Skyrim NIFs carry
/// texture sets the record graph never mentions. `convert_textures_v2` needs
/// the same discovery this phase does, so it shares the implementation.
pub(crate) struct NifTextureDiscovery {
    /// `(game-relative source path, absolute resolved path)`.
    pub textures: Vec<(String, String)>,
    pub failures: u32,
    pub warnings: Vec<String>,
}

pub(crate) fn discover_nif_texture_dependencies_with_progress(
    params: &JsonValue,
    reporter: &ProgressReporter,
) -> Result<NifTextureDiscovery, PhaseError> {
    let NifDiscovery {
        textures,
        failures,
        warning_messages,
    } = discover_nif_textures_with_progress(parse_nif_entries(params)?, Some(reporter));
    Ok(NifTextureDiscovery {
        textures: textures
            .into_iter()
            .map(|entry| (entry.source_path, entry.resolved_path))
            .collect(),
        failures,
        warnings: warning_messages,
    })
}

fn parse_texture_entries(params: &JsonValue) -> Result<Vec<TextureEntry>, PhaseError> {
    let entries = params
        .get("texture_paths")
        .and_then(JsonValue::as_array)
        .ok_or_else(|| PhaseError::BadParams("missing texture_paths".into()))?;
    entries
        .iter()
        .enumerate()
        .map(|(index, entry)| {
            let source_path = entry
                .get("source_path")
                .and_then(JsonValue::as_str)
                .ok_or_else(|| {
                    PhaseError::BadParams(format!(
                        "texture_paths[{index}].source_path missing or not a string"
                    ))
                })?
                .to_string();
            let resolved_path = entry
                .get("resolved_path")
                .and_then(JsonValue::as_str)
                .unwrap_or("")
                .to_string();
            Ok(TextureEntry {
                source_path,
                resolved_path,
            })
        })
        .collect()
}

fn parse_nif_entries(params: &JsonValue) -> Result<Vec<NifEntry>, PhaseError> {
    let Some(entries) = params.get("nif_paths") else {
        return Ok(Vec::new());
    };
    let entries = entries
        .as_array()
        .ok_or_else(|| PhaseError::BadParams("nif_paths must be an array".into()))?;
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
            Ok(NifEntry {
                source_path,
                resolved_path,
            })
        })
        .collect()
}

fn discover_nif_textures(nif_entries: Vec<NifEntry>) -> NifDiscovery {
    discover_nif_textures_with_progress(nif_entries, None)
}

fn discover_nif_textures_with_progress(
    nif_entries: Vec<NifEntry>,
    reporter: Option<&ProgressReporter>,
) -> NifDiscovery {
    let mut discovery = NifDiscovery::default();
    for entry in nif_entries {
        if let Some(reporter) = reporter {
            reporter.set_item(format!("Scanning NIF textures: {}", entry.source_path));
        }
        let nif_path = Path::new(&entry.resolved_path);
        match NifFile::load(nif_path.to_path_buf()) {
            Ok(nif) => match extracted_root_from_nif(nif_path) {
                Ok(source_root) => {
                    for texture_ref in nif.referenced_asset_paths().textures {
                        match texture_entry_from_ref(&source_root, &texture_ref) {
                            Ok(texture) => discovery.textures.push(texture),
                            Err(error) => discovery.fail(format!(
                                "copy_textures: NIF texture ref rejected {} ({texture_ref}): {error}",
                                entry.source_path
                            )),
                        }
                    }
                }
                Err(error) => discovery.fail(format!(
                    "copy_textures: NIF dependency root failed {}: {error}",
                    entry.source_path
                )),
            },
            Err(error) => {
                discovery.fail(format!(
                    "copy_textures: NIF dependency scan failed {}: {error}",
                    entry.source_path
                ));
            }
        }
        if let Some(reporter) = reporter {
            reporter.inc(1);
        }
    }
    discovery
}

fn extracted_root_from_nif(nif_path: &Path) -> Result<PathBuf, String> {
    let mut ancestor = nif_path.parent();
    while let Some(directory) = ancestor {
        if directory
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.eq_ignore_ascii_case("Meshes"))
        {
            return directory
                .parent()
                .map(Path::to_path_buf)
                .ok_or_else(|| format!("Meshes directory has no parent: {}", directory.display()));
        }
        ancestor = directory.parent();
    }
    Err(format!(
        "resolved NIF path has no Meshes ancestor: {}",
        nif_path.display()
    ))
}

fn texture_entry_from_ref(source_root: &Path, texture_ref: &str) -> Result<TextureEntry, String> {
    let parts = texture_relative_parts(texture_ref)?;
    let source_path = format!("Textures/{}", parts.join("/"));
    let resolved_path = parts
        .iter()
        .fold(source_root.join("Textures"), |path, part| path.join(part));
    Ok(TextureEntry {
        source_path,
        resolved_path: resolved_path.to_string_lossy().to_string(),
    })
}

fn prepare_texture_entries(
    entries: Vec<TextureEntry>,
    mod_path: &Path,
) -> (Vec<PreparedTextureEntry>, Vec<String>) {
    let mut prepared = Vec::with_capacity(entries.len());
    let mut first_by_output = HashMap::<String, (String, String)>::new();
    let mut collisions = Vec::new();
    for entry in entries {
        let output = texture_output_path(mod_path, &entry.source_path);
        if let Ok(path) = &output {
            let key = path
                .to_string_lossy()
                .replace('\\', "/")
                .to_ascii_lowercase();
            if let Some((kept_source, kept_output)) = first_by_output.get(&key) {
                collisions.push(format!(
                    "copy_textures: duplicate output path {kept_output}; keeping {kept_source}, skipping {}",
                    entry.source_path
                ));
                continue;
            }
            first_by_output.insert(
                key,
                (
                    entry.source_path.clone(),
                    path.to_string_lossy().to_string(),
                ),
            );
        }
        prepared.push(PreparedTextureEntry {
            source_path: entry.source_path,
            resolved_path: entry.resolved_path,
            output,
        });
    }
    (prepared, collisions)
}

fn emit_collision_warnings(ctx: &PhaseCtx<'_>, messages: &[String]) {
    for message in messages.iter().take(ERROR_EXAMPLE_LIMIT) {
        let _ = ctx.run.event_tx.try_send(PhaseEvent::Log {
            phase: "copy_textures",
            level: LogLevel::Warn,
            message: message.clone(),
        });
    }
    if messages.len() > ERROR_EXAMPLE_LIMIT {
        let _ = ctx.run.event_tx.try_send(PhaseEvent::Log {
            phase: "copy_textures",
            level: LogLevel::Warn,
            message: format!(
                "copy_textures: {} additional duplicate output paths",
                messages.len() - ERROR_EXAMPLE_LIMIT
            ),
        });
    }
}

fn emit_discovery_warnings(ctx: &PhaseCtx<'_>, failures: u32, messages: &[String]) {
    for message in messages {
        let _ = ctx.run.event_tx.try_send(PhaseEvent::Log {
            phase: "copy_textures",
            level: LogLevel::Warn,
            message: message.clone(),
        });
    }
    let omitted = failures as usize - messages.len();
    if omitted > 0 {
        let _ = ctx.run.event_tx.try_send(PhaseEvent::Log {
            phase: "copy_textures",
            level: LogLevel::Warn,
            message: format!("copy_textures: {omitted} additional NIF dependency failures"),
        });
    }
}

fn texture_output_path(mod_path: &Path, source_path: &str) -> Result<PathBuf, String> {
    let parts = texture_relative_parts(source_path)?;
    Ok(parts
        .into_iter()
        .fold(mod_path.join("data").join("Textures"), |path, part| {
            path.join(part)
        }))
}

fn texture_relative_parts(source_path: &str) -> Result<Vec<String>, String> {
    let normalized = source_path.replace('\\', "/");
    let mut parts = normalized
        .split('/')
        .map(str::trim)
        .filter(|part| !part.is_empty() && *part != ".")
        .map(str::to_string)
        .collect::<Vec<_>>();
    if parts.iter().any(|part| part == ".." || part.contains(':')) {
        return Err(format!("unsafe texture path: {source_path}"));
    }
    for root in ["data", "textures"] {
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
        return Err(format!(
            "texture path has no relative filename: {source_path}"
        ));
    }
    Ok(parts)
}

fn is_game_prefix(value: &str) -> bool {
    matches!(
        value.to_ascii_lowercase().as_str(),
        "fnv" | "fo3" | "fo4" | "fo76" | "oblivion" | "skyrim" | "skyrimse" | "starfield"
    )
}

fn copy_texture(source: &Path, output: &Path) -> std::io::Result<bool> {
    if output.is_file() && files_equal(source, output)? {
        return Ok(false);
    }
    let parent = output.parent().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("texture output has no parent: {}", output.display()),
        )
    })?;
    std::fs::create_dir_all(parent)?;
    let mut staged = tempfile::NamedTempFile::new_in(parent)?;
    let mut source_file = std::fs::File::open(source)?;
    std::io::copy(&mut source_file, &mut staged)?;
    std::io::Write::flush(staged.as_file_mut())?;
    staged.as_file().sync_all()?;
    if !files_equal(source, staged.path())? {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("staged texture verification failed: {}", source.display()),
        ));
    }
    staged
        .persist(output)
        .map_err(|error| error.error)
        .map(|_| true)
}

fn files_equal(left: &Path, right: &Path) -> std::io::Result<bool> {
    if std::fs::metadata(left)?.len() != std::fs::metadata(right)?.len() {
        return Ok(false);
    }
    let mut left = std::io::BufReader::new(std::fs::File::open(left)?);
    let mut right = std::io::BufReader::new(std::fs::File::open(right)?);
    let mut left_buffer = [0u8; 64 * 1024];
    let mut right_buffer = [0u8; 64 * 1024];
    loop {
        let left_len = std::io::Read::read(&mut left, &mut left_buffer)?;
        let right_len = std::io::Read::read(&mut right, &mut right_buffer)?;
        if left_len != right_len || left_buffer[..left_len] != right_buffer[..right_len] {
            return Ok(false);
        }
        if left_len == 0 {
            return Ok(true);
        }
    }
}

fn register_with_sink(
    sink: Option<&crate::sinks::SinkSet>,
    data_root: &Path,
    output: &Path,
) -> bool {
    let Some(sink) = sink else { return true };
    let Ok(relative) = output.strip_prefix(data_root) else {
        return true;
    };
    sink.add_existing_file(&relative.to_string_lossy().replace('\\', "/"), output)
        .is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::phase::{PhaseCtx, registry};
    use crate::run::{RunConfig, RunError, RunParams, create_run, drop_run, with_run};
    use crate::translator::Game;
    use std::sync::atomic::AtomicBool;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "copy_textures_{label}_{}_{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    fn crate_fixture_tree(temp: &Path) -> (PathBuf, Vec<u8>, Vec<u8>) {
        let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("src/test_fixtures/gamebryo_nifs/cratelarge01.nif");
        let root = temp.join("extracted/fnv");
        let nif = root.join("Meshes/clutter/crate/cratelarge01.nif");
        let diffuse = root.join("Textures/clutter/crate/cratelarge01.dds");
        let normal = root.join("Textures/clutter/crate/cratelarge01_n.dds");
        std::fs::create_dir_all(nif.parent().unwrap()).unwrap();
        std::fs::create_dir_all(diffuse.parent().unwrap()).unwrap();
        std::fs::copy(fixture, &nif).unwrap();
        let diffuse_bytes = b"DDS fixture diffuse\0\x01".to_vec();
        let normal_bytes = b"DDS fixture normal\0\x02".to_vec();
        std::fs::write(&diffuse, &diffuse_bytes).unwrap();
        std::fs::write(&normal, &normal_bytes).unwrap();
        (nif, diffuse_bytes, normal_bytes)
    }

    fn run_copy(
        mod_path: &Path,
        params: JsonValue,
        cancelled: bool,
    ) -> (PhaseReport, Vec<PhaseEvent>) {
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
        let report = with_run(run_id, |run| -> Result<PhaseReport, RunError> {
            let cancel = AtomicBool::new(cancelled);
            let mut ctx = PhaseCtx {
                run,
                mod_path,
                source_extracted_dir: Path::new(""),
                target_extracted_dir: None,
                target_data_dir: None,
                params: &params,
                cancel: &cancel,
            };
            CopyTexturesPhase
                .run(&mut ctx)
                .map_err(|error| RunError::InvalidConfig(error.to_string()))
        })
        .unwrap();
        let events = with_run(run_id, |run| -> Result<Vec<PhaseEvent>, RunError> {
            Ok(run.event_rx.try_iter().collect())
        })
        .unwrap();
        drop_run(run_id).unwrap();
        (report, events)
    }

    #[test]
    fn texture_output_strips_data_root_and_game_prefix() {
        let mod_path = Path::new("C:/mod");
        assert_eq!(
            texture_output_path(mod_path, "Data\\Textures\\fnv\\clutter\\crate\\crate_d.dds")
                .unwrap(),
            mod_path.join("data/Textures/clutter/crate/crate_d.dds")
        );
        assert_eq!(
            texture_output_path(mod_path, "textures/clutter/crate/crate_d.dds").unwrap(),
            mod_path.join("data/Textures/clutter/crate/crate_d.dds")
        );
        assert!(
            texture_entry_from_ref(Path::new("C:/extracted/fnv"), "textures/../escape.dds")
                .is_err()
        );
    }

    #[test]
    fn nif_dependency_scan_reports_native_progress() {
        let temp = temp_dir("nif_progress");
        let params = serde_json::json!({
            "nif_paths": [
                {
                    "source_path": "Meshes/fnv/missing_a.nif",
                    "resolved_path": temp.join("extracted/fnv/Meshes/missing_a.nif").to_string_lossy(),
                },
                {
                    "source_path": "Meshes/fnv/missing_b.nif",
                    "resolved_path": temp.join("extracted/fnv/Meshes/missing_b.nif").to_string_lossy(),
                }
            ]
        });
        let (tx, rx) = crossbeam_channel::unbounded();
        let reporter = ProgressReporter::new("convert_textures_v2", 2, tx);

        let discovery =
            discover_nif_texture_dependencies_with_progress(&params, &reporter).unwrap();
        reporter.finish();
        let events: Vec<_> = rx.try_iter().collect();

        assert_eq!(discovery.failures, 2);
        assert!(events.iter().any(|event| matches!(
            event,
            PhaseEvent::Progress {
                current: 1,
                total: 2,
                item: Some(item),
                ..
            } if item.ends_with("Meshes/fnv/missing_a.nif")
        )));
        assert!(events.iter().any(|event| matches!(
            event,
            PhaseEvent::Progress {
                current: 2,
                total: 2,
                item: Some(item),
                ..
            } if item.ends_with("Meshes/fnv/missing_b.nif")
        )));
    }

    #[test]
    fn copies_texture_bytes_unchanged() {
        let temp = temp_dir("bytes");
        let source = temp.join("source.dds");
        std::fs::create_dir_all(&temp).unwrap();
        let bytes = b"DDS binary payload\0\x7f";
        std::fs::write(&source, bytes).unwrap();
        let (report, _) = run_copy(
            &temp.join("mod"),
            serde_json::json!({
                "texture_paths": [{
                    "source_path": "Textures/fnv/clutter/source.dds",
                    "resolved_path": source.to_string_lossy(),
                }]
            }),
            false,
        );
        assert_eq!(report.assets_written, 1);
        assert_eq!(report.items_failed, 0);
        assert_eq!(
            std::fs::read(temp.join("mod/data/Textures/clutter/source.dds")).unwrap(),
            bytes
        );
        let _ = std::fs::remove_dir_all(temp);
    }

    #[test]
    fn stale_existing_output_is_replaced() {
        let temp = temp_dir("existing");
        let source = temp.join("source.dds");
        let output = temp.join("mod/data/Textures/clutter/source.dds");
        std::fs::create_dir_all(output.parent().unwrap()).unwrap();
        std::fs::write(&source, b"source").unwrap();
        std::fs::write(&output, b"target").unwrap();
        let (report, _) = run_copy(
            &temp.join("mod"),
            serde_json::json!({
                "texture_paths": [{
                    "source_path": "Textures/fnv/clutter/source.dds",
                    "resolved_path": source.to_string_lossy(),
                }]
            }),
            false,
        );
        assert_eq!(report.assets_written, 1);
        assert_eq!(std::fs::read(&output).unwrap(), b"source");
        let _ = std::fs::remove_dir_all(temp);
    }

    #[test]
    fn content_equal_existing_output_is_reused() {
        let temp = temp_dir("equal_existing");
        let source = temp.join("source.dds");
        let output = temp.join("mod/data/Textures/clutter/source.dds");
        std::fs::create_dir_all(output.parent().unwrap()).unwrap();
        std::fs::write(&source, b"same bytes").unwrap();
        std::fs::write(&output, b"same bytes").unwrap();
        let (report, _) = run_copy(
            &temp.join("mod"),
            serde_json::json!({
                "texture_paths": [{
                    "source_path": "Textures/fnv/clutter/source.dds",
                    "resolved_path": source.to_string_lossy(),
                }]
            }),
            false,
        );

        assert_eq!(report.assets_written, 0);
        assert_eq!(report.items_failed, 0);
        assert_eq!(std::fs::read(&output).unwrap(), b"same bytes");
        let _ = std::fs::remove_dir_all(temp);
    }

    #[test]
    fn missing_source_removes_stale_output() {
        let temp = temp_dir("missing_source_stale_output");
        let output = temp.join("mod/data/Textures/clutter/source.dds");
        std::fs::create_dir_all(output.parent().unwrap()).unwrap();
        std::fs::write(&output, b"stale").unwrap();
        let (report, _) = run_copy(
            &temp.join("mod"),
            serde_json::json!({
                "texture_paths": [{
                    "source_path": "Textures/fnv/clutter/source.dds",
                    "resolved_path": temp.join("missing.dds").to_string_lossy(),
                }]
            }),
            false,
        );

        assert_eq!(report.items_failed, 1);
        assert!(!output.exists());
        let _ = std::fs::remove_dir_all(temp);
    }

    #[test]
    fn failed_atomic_publish_cleans_staged_texture() {
        let temp = temp_dir("failed_publish_cleanup");
        let source = temp.join("source.dds");
        let occupied_output = temp.join("occupied.dds");
        std::fs::create_dir_all(&occupied_output).unwrap();
        std::fs::write(&source, b"source").unwrap();

        assert!(copy_texture(&source, &occupied_output).is_err());
        assert_eq!(
            std::fs::read_dir(&temp)
                .unwrap()
                .map(|entry| entry.unwrap().file_name())
                .collect::<std::collections::HashSet<_>>(),
            [
                source.file_name().unwrap(),
                occupied_output.file_name().unwrap()
            ]
            .into_iter()
            .map(std::ffi::OsStr::to_os_string)
            .collect()
        );

        let _ = std::fs::remove_dir_all(temp);
    }

    #[test]
    fn cancellation_stops_copying() {
        let temp = temp_dir("cancelled");
        let source = temp.join("source.dds");
        std::fs::create_dir_all(&temp).unwrap();
        std::fs::write(&source, b"source").unwrap();
        let (report, _) = run_copy(
            &temp.join("mod"),
            serde_json::json!({
                "texture_paths": [{
                    "source_path": "Textures/fnv/clutter/source.dds",
                    "resolved_path": source.to_string_lossy(),
                }]
            }),
            true,
        );
        assert_eq!(report.assets_written, 0);
        assert!(!temp.join("mod/data/Textures/clutter/source.dds").exists());
        let _ = std::fs::remove_dir_all(temp);
    }

    #[test]
    fn collapsed_output_collision_keeps_first_source_and_warns() {
        let temp = temp_dir("collision");
        let first = temp.join("first.dds");
        let second = temp.join("second.dds");
        std::fs::create_dir_all(&temp).unwrap();
        std::fs::write(&first, b"first").unwrap();
        std::fs::write(&second, b"second").unwrap();
        let (report, events) = run_copy(
            &temp.join("mod"),
            serde_json::json!({
                "texture_paths": [
                    {
                        "source_path": "Textures/fnv/clutter/shared.dds",
                        "resolved_path": first.to_string_lossy(),
                    },
                    {
                        "source_path": "Textures/clutter/shared.dds",
                        "resolved_path": second.to_string_lossy(),
                    }
                ]
            }),
            false,
        );
        assert_eq!(report.assets_written, 1);
        assert_eq!(report.items_failed, 0);
        assert_eq!(
            std::fs::read(temp.join("mod/data/Textures/clutter/shared.dds")).unwrap(),
            b"first"
        );
        assert!(events.iter().any(|event| matches!(
            event,
            PhaseEvent::Log { level: LogLevel::Warn, message, .. }
                if message.contains("duplicate output path")
                    && message.contains("Textures/fnv/clutter/shared.dds")
                    && message.contains("Textures/clutter/shared.dds")
        )));
        let _ = std::fs::remove_dir_all(temp);
    }

    #[test]
    fn discovers_and_copies_textures_from_real_nif_fixture() {
        let temp = temp_dir("nif_discovery");
        let (nif, diffuse, normal) = crate_fixture_tree(&temp);
        let (report, _) = run_copy(
            &temp.join("mod"),
            serde_json::json!({
                "texture_paths": [],
                "nif_paths": [{
                    "source_path": "Meshes/fnv/clutter/crate/cratelarge01.nif",
                    "resolved_path": nif.to_string_lossy(),
                }]
            }),
            false,
        );

        assert_eq!(report.assets_written, 2);
        assert_eq!(report.items_failed, 0);
        assert_eq!(
            std::fs::read(temp.join("mod/data/Textures/clutter/crate/cratelarge01.dds")).unwrap(),
            diffuse
        );
        assert_eq!(
            std::fs::read(temp.join("mod/data/Textures/clutter/crate/cratelarge01_n.dds")).unwrap(),
            normal
        );
        let _ = std::fs::remove_dir_all(temp);
    }

    #[test]
    fn nif_discovery_is_order_independent_and_dedups_explicit_texture() {
        for nif_first in [false, true] {
            let temp = temp_dir(if nif_first {
                "nif_first"
            } else {
                "textures_first"
            });
            let (nif, _, normal) = crate_fixture_tree(&temp);
            let explicit = temp.join("explicit.dds");
            let explicit_bytes = b"explicit diffuse wins";
            std::fs::write(&explicit, explicit_bytes).unwrap();
            let nif_paths = serde_json::json!([{
                "source_path": "Meshes/fnv/clutter/crate/cratelarge01.nif",
                "resolved_path": nif.to_string_lossy(),
            }]);
            let texture_paths = serde_json::json!([{
                "source_path": "Textures/fnv/clutter/crate/cratelarge01.dds",
                "resolved_path": explicit.to_string_lossy(),
            }]);
            let params = if nif_first {
                serde_json::json!({
                    "nif_paths": nif_paths,
                    "texture_paths": texture_paths,
                })
            } else {
                serde_json::json!({
                    "texture_paths": texture_paths,
                    "nif_paths": nif_paths,
                })
            };
            let (report, events) = run_copy(&temp.join("mod"), params, false);

            assert_eq!(report.assets_written, 2);
            assert_eq!(report.items_failed, 0);
            assert_eq!(
                std::fs::read(temp.join("mod/data/Textures/clutter/crate/cratelarge01.dds"))
                    .unwrap(),
                explicit_bytes
            );
            assert_eq!(
                std::fs::read(temp.join("mod/data/Textures/clutter/crate/cratelarge01_n.dds"))
                    .unwrap(),
                normal
            );
            assert!(events.iter().any(|event| matches!(
                event,
                PhaseEvent::Log { level: LogLevel::Warn, message, .. }
                    if message.contains("duplicate output path")
            )));
            let _ = std::fs::remove_dir_all(temp);
        }
    }

    #[test]
    fn nif_discovery_failure_does_not_block_explicit_texture() {
        let temp = temp_dir("nif_failure");
        let explicit = temp.join("explicit.dds");
        std::fs::create_dir_all(&temp).unwrap();
        std::fs::write(&explicit, b"explicit").unwrap();
        let (report, events) = run_copy(
            &temp.join("mod"),
            serde_json::json!({
                "texture_paths": [{
                    "source_path": "Textures/fnv/explicit.dds",
                    "resolved_path": explicit.to_string_lossy(),
                }],
                "nif_paths": [{
                    "source_path": "Meshes/fnv/missing.nif",
                    "resolved_path": temp.join("extracted/fnv/Meshes/missing.nif").to_string_lossy(),
                }]
            }),
            false,
        );

        assert_eq!(report.assets_written, 1);
        assert_eq!(report.warnings, 1);
        assert_eq!(report.items_failed, 1);
        assert_eq!(
            std::fs::read(temp.join("mod/data/Textures/explicit.dds")).unwrap(),
            b"explicit"
        );
        assert!(events.iter().any(|event| matches!(
            event,
            PhaseEvent::Log { level: LogLevel::Warn, message, .. }
                if message.contains("NIF dependency scan failed")
        )));
        let _ = std::fs::remove_dir_all(temp);
    }

    #[test]
    fn phase_is_registered() {
        assert!(registry().names().contains(&"copy_textures"));
    }
}
