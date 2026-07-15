// Phase: copy_sounds
//
// Params shape (JSON):
// { "sound_paths": [ { "source_path": "...", "resolved_path": "/abs/..." } ] }
//
// Parity contract = creation_lib/conversion/pipeline/sounds.py::copy_sounds:
//   - dedup against ctx.target_extracted_dir using the UN-stripped
//     "sound/"-rooted subpath  -> PhaseReport.records_dropped
//   - missing/unresolved source -> PhaseReport.warnings + rate-limited
//     "Audio not found: <source_path>" errors (limit 25 + "<n> additional ...")
//   - existing output -> silent skip (no counter)
//   - directory source -> recursive copy
//   - copied -> PhaseReport.assets_written
//   - final INFO line "copy_sounds: copied=<n>, base_game_skipped=<n>, failed=<n>"

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use rayon::prelude::*;
use serde_json::Value as JsonValue;

use crate::phase::progress::ProgressReporter;
use crate::phase::{LogLevel, Phase, PhaseCtx, PhaseError, PhaseEvent, PhaseReport};

const MISSING_EXAMPLE_LIMIT: usize = 25;

/// Game-profile ids that count as asset-prefix path components (mirrors
/// `bacup_lib.paths._KNOWN_ASSET_PREFIXES`).
const KNOWN_ASSET_PREFIXES: [&str; 7] = [
    "fnv",
    "fo3",
    "fo4",
    "fo76",
    "oblivion",
    "skyrimse",
    "starfield",
];

const KNOWN_ROOTS: [&str; 4] = ["Meshes", "Textures", "Materials", "Sound"];

struct SoundEntry {
    source_path: String,
    resolved_path: String,
}

#[derive(Default)]
struct SoundAgg {
    copied: u32,
    skipped: u32,
    failed: u32,
    missing_examples: Vec<String>,
    missing_total: u32,
    error_messages: Vec<String>,
    cancelled: bool,
    /// Successful copies whose BA2 sink registration failed.
    sink_failures: u32,
}

impl SoundAgg {
    fn note_missing(&mut self, source_path: &str) {
        self.failed += 1;
        self.missing_total += 1;
        if self.missing_examples.len() < MISSING_EXAMPLE_LIMIT {
            self.missing_examples
                .push(format!("Audio not found: {source_path}"));
        }
    }

    fn merge(mut self, other: SoundAgg) -> SoundAgg {
        self.copied += other.copied;
        self.skipped += other.skipped;
        self.failed += other.failed;
        self.missing_total += other.missing_total;
        for msg in other.missing_examples {
            if self.missing_examples.len() < MISSING_EXAMPLE_LIMIT {
                self.missing_examples.push(msg);
            }
        }
        self.error_messages.extend(other.error_messages);
        self.cancelled |= other.cancelled;
        self.sink_failures += other.sink_failures;
        self
    }
}

pub struct CopySoundsPhase;

impl Phase for CopySoundsPhase {
    fn name(&self) -> &'static str {
        "copy_sounds"
    }

    fn run(&self, ctx: &mut PhaseCtx<'_>) -> Result<PhaseReport, PhaseError> {
        let entries = parse_sound_entries(ctx.params)?;
        let total = entries.len() as u32;
        let mod_path = ctx.mod_path;
        let target_extracted_dir = ctx.target_extracted_dir;
        let target_assets = ctx.run.target_assets.clone();
        let cancel = ctx.cancel;
        emit_sound_output_collision_warnings(ctx, &entries);
        // Sink registration of copied outputs (see phase/nifs.rs).
        // Directory sources register every file under the copied tree.
        let sink = ctx.run.output_sink.clone();
        let data_root = mod_path.join("data");
        let register_with_sink = |output: &Path| -> bool {
            let Some(s) = &sink else { return true };
            let mut ok = true;
            let mut stack = vec![output.to_path_buf()];
            while let Some(p) = stack.pop() {
                if p.is_dir() {
                    if let Ok(rd) = std::fs::read_dir(&p) {
                        stack.extend(rd.flatten().map(|e| e.path()));
                    }
                } else if p.is_file() {
                    if let Ok(rel) = p.strip_prefix(&data_root) {
                        let rel_str = rel.to_string_lossy().replace('\\', "/");
                        ok &= s.add_existing_file(&rel_str, &p).is_ok();
                    }
                }
            }
            ok
        };

        let reporter = Arc::new(ProgressReporter::new(
            "copy_sounds",
            total,
            ctx.run.event_tx.clone(),
        ));

        let agg: SoundAgg = entries
            .into_par_iter()
            .fold(SoundAgg::default, |mut agg, entry| {
                if cancel.load(std::sync::atomic::Ordering::Relaxed) {
                    agg.cancelled = true;
                    return agg;
                }
                let dedup = sound_dedup_subpath(&entry.source_path);
                let target_has = target_assets
                    .as_ref()
                    .is_some_and(|store| store.has_asset(&dedup))
                    || target_extracted_dir
                        .map(|root| root.join(Path::new(&dedup)).exists())
                        .unwrap_or(false);
                if target_has {
                    agg.skipped += 1;
                } else if entry.resolved_path.is_empty() {
                    agg.note_missing(&entry.source_path);
                } else {
                    let source = Path::new(&entry.resolved_path);
                    let output =
                        sound_output_path(mod_path, &entry.source_path, &entry.resolved_path);
                    if !source.exists() {
                        agg.note_missing(&entry.source_path);
                    } else if output.exists() {
                        // silent skip — neither copied nor failed; the
                        // existing output still streams (reuse path).
                        if !register_with_sink(&output) {
                            agg.sink_failures += 1;
                        }
                    } else {
                        match copy_sound_asset(source, &output) {
                            Ok(()) => {
                                agg.copied += 1;
                                if !register_with_sink(&output) {
                                    agg.sink_failures += 1;
                                }
                            }
                            Err(error) => {
                                agg.failed += 1;
                                agg.error_messages
                                    .push(format!("Audio failed: {}: {error}", entry.source_path));
                            }
                        }
                    }
                }
                reporter.inc(1);
                agg
            })
            .reduce(SoundAgg::default, SoundAgg::merge);
        reporter.finish();

        for message in &agg.missing_examples {
            let _ = ctx.run.event_tx.try_send(PhaseEvent::Log {
                phase: "copy_sounds",
                level: LogLevel::Error,
                message: message.clone(),
            });
        }
        let omitted = agg.missing_total as usize - agg.missing_examples.len();
        if omitted > 0 {
            let _ = ctx.run.event_tx.try_send(PhaseEvent::Log {
                phase: "copy_sounds",
                level: LogLevel::Error,
                message: format!("{omitted} additional audio files were not found"),
            });
        }
        for message in agg.error_messages {
            let _ = ctx.run.event_tx.try_send(PhaseEvent::Log {
                phase: "copy_sounds",
                level: LogLevel::Error,
                message,
            });
        }
        if agg.cancelled {
            let _ = ctx.run.event_tx.try_send(PhaseEvent::Log {
                phase: "copy_sounds",
                level: LogLevel::Warn,
                message: "copy_sounds: cancelled".to_string(),
            });
        }
        let _ = ctx.run.event_tx.try_send(PhaseEvent::Log {
            phase: "copy_sounds",
            level: LogLevel::Info,
            message: format!(
                "copy_sounds: copied={}, base_game_skipped={}, failed={}",
                agg.copied, agg.skipped, agg.failed
            ),
        });

        let _ = ctx.run.event_tx.try_send(PhaseEvent::Progress {
            phase: "copy_sounds",
            current: total,
            total,
            item: None,
        });

        Ok(PhaseReport {
            assets_written: agg.copied,
            records_dropped: agg.skipped,
            warnings: agg.failed,
            items_failed: agg.failed + agg.sink_failures,
            ..Default::default()
        })
    }
}

fn parse_sound_entries(p: &JsonValue) -> Result<Vec<SoundEntry>, PhaseError> {
    let list = p
        .get("sound_paths")
        .and_then(|v| v.as_array())
        .ok_or_else(|| PhaseError::BadParams("missing sound_paths".into()))?;
    let mut out = Vec::with_capacity(list.len());
    for item in list {
        let source_path = item
            .get("source_path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| PhaseError::BadParams("sound entry missing source_path".into()))?
            .to_string();
        let resolved_path = item
            .get("resolved_path")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        out.push(SoundEntry {
            source_path,
            resolved_path,
        });
    }
    Ok(out)
}

fn emit_sound_output_collision_warnings(ctx: &mut PhaseCtx<'_>, entries: &[SoundEntry]) {
    let mut by_output: HashMap<String, (String, Vec<String>)> = HashMap::new();
    for entry in entries {
        let output = sound_output_subpath(&entry.source_path);
        let key = output.to_ascii_lowercase();
        let (_, sources) = by_output.entry(key).or_insert_with(|| (output, Vec::new()));
        if !sources
            .iter()
            .any(|source| source.eq_ignore_ascii_case(&entry.source_path))
        {
            sources.push(entry.source_path.clone());
        }
    }

    let mut collision_count = 0usize;
    for (_key, (output, sources)) in by_output {
        if sources.len() <= 1 {
            continue;
        }
        collision_count += 1;
        if collision_count <= 25 {
            let preview = sources
                .iter()
                .take(8)
                .cloned()
                .collect::<Vec<_>>()
                .join("; ");
            let suffix = if sources.len() > 8 { "; ..." } else { "" };
            let _ = ctx.run.event_tx.try_send(PhaseEvent::Log {
                phase: "copy_sounds",
                level: LogLevel::Warn,
                message: format!(
                    "copy_sounds: duplicate output path {output} from {} sources: {preview}{suffix}",
                    sources.len()
                ),
            });
        }
    }
    if collision_count > 25 {
        let _ = ctx.run.event_tx.try_send(PhaseEvent::Log {
            phase: "copy_sounds",
            level: LogLevel::Warn,
            message: format!(
                "copy_sounds: {} additional duplicate output paths",
                collision_count - 25
            ),
        });
    }
}

/// Dedup key = `creation_lib...pipeline.sounds._sound_data_subpath`:
/// normalize backslashes, preserve music under a "Music/" root, otherwise
/// ensure a "sound/" root (lowercase compare), do NOT strip any prefix.
fn sound_dedup_subpath(source_path: &str) -> String {
    let path = source_path
        .replace('\\', "/")
        .trim_start_matches('/')
        .to_string();
    let lower = path.to_ascii_lowercase();
    if lower.starts_with("data/music/") {
        return format!("Music/{}", &path[11..]);
    }
    if lower.starts_with("music/") {
        return format!("Music/{}", &path[6..]);
    }
    if path.to_ascii_lowercase().starts_with("sound/") {
        path
    } else {
        format!("sound/{path}")
    }
}

/// Output subpath = `apply_asset_prefix(_sound_data_subpath(source_path))`
/// (paths.py). The data-subpath preserves "Music/" or ensures a "sound/" root;
/// apply_asset_prefix then canonicalizes known root case and drops a known
/// game-prefix component.
fn sound_output_subpath(source_path: &str) -> String {
    apply_asset_prefix(&sound_dedup_subpath(source_path))
}

fn sound_output_path(mod_path: &Path, source_path: &str, resolved_path: &str) -> PathBuf {
    let subpath = apply_resolved_extension(sound_output_subpath(source_path), resolved_path);
    mod_path.join("data").join(Path::new(&subpath))
}

const AUDIO_EXTS: [&str; 3] = ["wav", "xwm", "fuz"];

fn is_audio_ext(ext: &str) -> bool {
    AUDIO_EXTS
        .iter()
        .any(|known| known.eq_ignore_ascii_case(ext))
}

/// Preserve the *real* source format. SNDR/sound records routinely name a
/// `.wav` that only exists on disk as `.xwm`/`.fuz`; the resolver falls back to
/// the real file but the record-derived output path keeps `.wav`, so a byte copy
/// would write XWM/FUZ bytes into a `.wav`. Re-extension the output after the
/// resolved file. Directory sources and non-audio resolves keep the record name.
fn apply_resolved_extension(subpath: String, resolved_path: &str) -> String {
    if resolved_path.is_empty() {
        return subpath;
    }
    let resolved = Path::new(resolved_path);
    if resolved.is_dir() {
        return subpath;
    }
    let Some(resolved_ext) = resolved.extension().and_then(|e| e.to_str()) else {
        return subpath;
    };
    if !is_audio_ext(resolved_ext) {
        return subpath;
    }
    let current = Path::new(&subpath);
    let cur_ext = current.extension().and_then(|e| e.to_str()).unwrap_or("");
    if !is_audio_ext(cur_ext) || cur_ext.eq_ignore_ascii_case(resolved_ext) {
        return subpath;
    }
    current
        .with_extension(resolved_ext)
        .to_string_lossy()
        .replace('\\', "/")
}

/// Port of `bacup_lib.paths.apply_asset_prefix` (sufficient for
/// the always-"sound/"-rooted inputs this phase produces; returns the input
/// unchanged when the leading component is not a known root, matching Python).
fn apply_asset_prefix(path: &str) -> String {
    let normalized = strip_data_prefix_before_known_root(&normalize_path(path));
    if normalized.is_empty() {
        return path.to_string();
    }
    let mut parts: Vec<String> = normalized.split('/').map(|s| s.to_string()).collect();
    if parts.is_empty() {
        return path.to_string();
    }
    let Some(root) = KNOWN_ROOTS
        .iter()
        .find(|known| known.eq_ignore_ascii_case(&parts[0]))
    else {
        return path.to_string();
    };
    parts[0] = (*root).to_string();
    if parts.len() > 1
        && KNOWN_ASSET_PREFIXES
            .iter()
            .any(|pfx| pfx.eq_ignore_ascii_case(&parts[1]))
    {
        let mut collapsed = Vec::with_capacity(parts.len() - 1);
        collapsed.push(parts[0].clone());
        collapsed.extend_from_slice(&parts[2..]);
        parts = collapsed;
    }
    parts.join("/")
}

fn normalize_path(path: &str) -> String {
    path.replace('\\', "/")
        .trim()
        .trim_start_matches('/')
        .to_string()
}

fn strip_data_prefix_before_known_root(path: &str) -> String {
    let parts: Vec<&str> = path.split('/').collect();
    if parts.len() > 2
        && parts[0].eq_ignore_ascii_case("data")
        && KNOWN_ROOTS.iter().any(|r| r.eq_ignore_ascii_case(parts[1]))
    {
        parts[1..].join("/")
    } else {
        path.to_string()
    }
}

fn copy_sound_asset(source: &Path, output: &Path) -> std::io::Result<()> {
    if source.is_dir() {
        copy_dir_recursive(source, output)
    } else {
        if let Some(parent) = output.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::copy(source, output).map(|_| ())
    }
}

fn copy_dir_recursive(source: &Path, dest: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dest)?;
    for entry in std::fs::read_dir(source)? {
        let entry = entry?;
        let from = entry.path();
        let to = dest.join(entry.file_name());
        if from.is_dir() {
            copy_dir_recursive(&from, &to)?;
        } else {
            std::fs::copy(&from, &to)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copy_sounds_subpath_rules() {
        assert_eq!(sound_output_subpath("fx/boom.wav"), "Sound/fx/boom.wav");
        assert_eq!(
            sound_output_subpath("sound/fx/boom.wav"),
            "Sound/fx/boom.wav"
        );
        assert_eq!(
            sound_output_subpath("Sound/fo76/fx/boom.wav"),
            "Sound/fx/boom.wav"
        );
        assert_eq!(
            sound_output_subpath("music/76/explore/MUS_76_Explore.wav"),
            "Music/76/explore/MUS_76_Explore.wav"
        );
        assert_eq!(
            sound_output_subpath("Data/music/76/explore/MUS_76_Explore.wav"),
            "Music/76/explore/MUS_76_Explore.wav"
        );
        // _sound_data_subpath runs BEFORE apply_asset_prefix: it prepends
        // "sound/" because the path does not start with "sound/", so the
        // leading "data/sound/" survives as a nested component (matches Python).
        assert_eq!(
            sound_output_subpath("data/sound/fx/boom.wav"),
            "Sound/data/sound/fx/boom.wav"
        );
        // dedup key keeps the prefix (Python _sound_data_subpath does NOT strip it)
        assert_eq!(
            sound_dedup_subpath("Sound/fo76/fx/boom.wav"),
            "Sound/fo76/fx/boom.wav"
        );
        assert_eq!(sound_dedup_subpath("fx/boom.wav"), "sound/fx/boom.wav");
    }

    #[test]
    fn output_preserves_resolved_audio_format() {
        // SNDR names a `.wav` that only exists on disk as `.xwm`: the output
        // must be named after the real file so XWM bytes never land in a `.wav`.
        // (is_dir() is false for these non-existent paths, so the helper runs.)
        assert_eq!(
            apply_resolved_extension("Sound/fx/radio.wav".into(), "/abs/radio.xwm"),
            "Sound/fx/radio.xwm",
        );
        assert_eq!(
            apply_resolved_extension("Sound/voice/line.wav".into(), "/abs/line.fuz"),
            "Sound/voice/line.fuz",
        );
        // Matching/identical extension is left untouched.
        assert_eq!(
            apply_resolved_extension("Sound/fx/radio.xwm".into(), "/abs/radio.xwm"),
            "Sound/fx/radio.xwm",
        );
        // Empty resolved, non-audio resolved, and non-audio source are no-ops.
        assert_eq!(
            apply_resolved_extension("Sound/fx/radio.wav".into(), ""),
            "Sound/fx/radio.wav",
        );
        assert_eq!(
            apply_resolved_extension("Sound/fx/radio.wav".into(), "/abs/radio.txt"),
            "Sound/fx/radio.wav",
        );
    }

    #[test]
    fn copy_sounds_matches_python_semantics() {
        use crate::phase::{Phase, PhaseCtx, PhaseReport};
        use crate::run::{RunConfig, RunError, RunParams, create_run, drop_run, with_run};
        use crate::translator::Game;
        use std::sync::atomic::AtomicBool;

        let temp = tempfile::tempdir().unwrap();
        let src_root = temp.path().join("src");
        let target_extracted = temp.path().join("target");
        let mod_dir = temp.path().join("mod");
        std::fs::create_dir_all(&src_root).unwrap();

        // (a) present in target-extracted -> records_dropped
        let a_src = src_root.join("present.wav");
        std::fs::write(&a_src, b"AAAA").unwrap();
        let a_dedup = target_extracted.join("sound/fx/present.wav");
        std::fs::create_dir_all(a_dedup.parent().unwrap()).unwrap();
        std::fs::write(&a_dedup, b"base").unwrap();

        // (c) normal file -> copied to mod/data/Sound/fx/boom.wav
        let c_src = src_root.join("boom.wav");
        std::fs::write(&c_src, b"BOOM").unwrap();

        // (c2) music file -> copied to mod/data/Music/... to match MUST paths.
        let music_src = src_root.join("music.wav");
        std::fs::write(&music_src, b"MUSIC").unwrap();

        // (d) prefixed path -> output strips fo76 component
        let d_src = src_root.join("explode.wav");
        std::fs::write(&d_src, b"DDDD").unwrap();

        // (e) output already exists -> silent skip
        let e_src = src_root.join("exists.wav");
        std::fs::write(&e_src, b"NEW").unwrap();
        let e_out = mod_dir.join("data/Sound/fx/exists.wav");
        std::fs::create_dir_all(e_out.parent().unwrap()).unwrap();
        std::fs::write(&e_out, b"OLD").unwrap();

        // (f) record names .wav but only .xwm exists on disk -> output keeps .xwm
        let f_src = src_root.join("radio.xwm");
        std::fs::write(&f_src, b"XWM").unwrap();

        let id = create_run(RunParams {
            source: Game::Fo76,
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

        let report = with_run(id, |run| -> Result<PhaseReport, RunError> {
            let cancel = std::sync::Arc::new(AtomicBool::new(false));
            let params = serde_json::json!({
                "sound_paths": [
                    { "source_path": "fx/present.wav", "resolved_path": a_src.to_string_lossy() },
                    { "source_path": "fx/missing.wav", "resolved_path": "" },
                    { "source_path": "fx/boom.wav", "resolved_path": c_src.to_string_lossy() },
                    { "source_path": "music/76/explore/music.wav", "resolved_path": music_src.to_string_lossy() },
                    { "source_path": "Sound/fo76/fx/explode.wav", "resolved_path": d_src.to_string_lossy() },
                    { "source_path": "fx/exists.wav", "resolved_path": e_src.to_string_lossy() },
                    { "source_path": "fx/radio.wav", "resolved_path": f_src.to_string_lossy() },
                ]
            });
            let mut ctx = PhaseCtx {
                run,
                mod_path: &mod_dir,
                source_extracted_dir: &src_root,
                target_extracted_dir: Some(&target_extracted),
                target_data_dir: None,
                params: &params,
                cancel: &cancel,
            };
            CopySoundsPhase
                .run(&mut ctx)
                .map_err(|e| RunError::InvalidConfig(e.to_string()))
        })
        .unwrap();

        assert_eq!(
            report.assets_written, 4,
            "boom + music + explode + radio copied"
        );
        assert_eq!(report.records_dropped, 1, "present skipped (base game)");
        assert_eq!(report.warnings, 1, "missing failed");

        // (c) copied to canonical Sound root
        let boom_out = mod_dir.join("data/Sound/fx/boom.wav");
        assert!(boom_out.exists());
        assert_eq!(std::fs::read(&boom_out).unwrap(), b"BOOM");
        let music_out = mod_dir.join("data/Music/76/explore/music.wav");
        assert!(music_out.exists());
        assert_eq!(std::fs::read(&music_out).unwrap(), b"MUSIC");
        // (d) fo76 prefix stripped
        let explode_out = mod_dir.join("data/Sound/fx/explode.wav");
        assert!(explode_out.exists());
        assert_eq!(std::fs::read(&explode_out).unwrap(), b"DDDD");
        // (e) untouched (silent skip)
        assert_eq!(std::fs::read(&e_out).unwrap(), b"OLD");
        // (f) format preserved: output is radio.xwm, no .wav left behind
        let radio_out = mod_dir.join("data/Sound/fx/radio.xwm");
        assert!(radio_out.exists());
        assert_eq!(std::fs::read(&radio_out).unwrap(), b"XWM");
        assert!(!mod_dir.join("data/Sound/fx/radio.wav").exists());

        drop_run(id).unwrap();
    }
}
