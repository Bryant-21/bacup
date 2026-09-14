//! Output sinks: Ba2ShardWriter (spill writers, plan+finalize at join),
//! LooseSink (the loose tree — phases keep writing it), TerrainSidecarSink
//! (Terrain/*.btd4 — ALWAYS loose, never packed). Attached per-run via
//! ConversionRun.output_sink; None = legacy behavior everywhere.

pub mod ba2;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

pub use ba2::Ba2ShardWriter;
use ba2::PreparedBa2Batch;
use serde::Serialize;

/// Mirror of build/archive_plan.py::classify_archive_family. Keep in lockstep
/// — a Python cross-language test pins both over the same fixture list.
pub fn classify_family(relative_path: &str) -> &'static str {
    let normalized = relative_path.replace('\\', "/");
    let normalized = normalized.trim_start_matches('/');
    let mut lower = normalized.to_ascii_lowercase();
    {
        let parts: Vec<&str> = lower.split('/').collect();
        if parts.first() == Some(&"data") {
            lower = parts[1..].join("/");
        }
    }
    let parts: Vec<&str> = if lower.is_empty() {
        Vec::new()
    } else {
        lower.split('/').collect()
    };
    let suffix = path_suffix(&lower);
    let first = parts.first().copied().unwrap_or("");

    if first == "textures" {
        return "Textures";
    }
    if first == "interface" {
        return "Interface";
    }
    if first == "materials" || matches!(suffix, ".bgsm" | ".bgem") {
        return "Materials";
    }
    if first == "strings" || matches!(suffix, ".strings" | ".dlstrings" | ".ilstrings") {
        return "Strings";
    }
    if matches!(first, "sound" | "music") || matches!(suffix, ".xwm" | ".wav") {
        return "Sounds";
    }
    if suffix == ".hkx" || parts.contains(&"animations") {
        return "Animations";
    }
    if first == "scripts" || matches!(suffix, ".pex" | ".psc") {
        return "Scripts";
    }
    if matches!(suffix, ".bto" | ".btr") {
        return "LOD";
    }
    if first == "meshes" || suffix == ".nif" {
        return "Meshes";
    }
    "Main"
}

/// pathlib.Path(...).suffix semantics on the FINAL component: the last '.'
/// segment, empty for leading-dot-only or trailing-dot names.
fn path_suffix(lower: &str) -> &str {
    let name = lower.rsplit('/').next().unwrap_or("");
    match name.rfind('.') {
        Some(i) if i > 0 && i < name.len() - 1 => &name[i..],
        _ => "",
    }
}

/// Terrain sidecars (and anything outside data/ semantics) must never reach
/// the BA2 sink.
fn rejected_by_ba2(rel: &str) -> bool {
    let normalized = rel.replace('\\', "/");
    let lower = normalized.trim_start_matches('/').to_ascii_lowercase();
    let mut parts = lower.split('/');
    let first = parts.next().unwrap_or("");
    let first = if first == "data" {
        parts.next().unwrap_or("")
    } else {
        first
    };
    first == "terrain" || lower.ends_with(".btd4")
}

pub struct LooseSink {
    pub enabled: bool,
    /// `mods/<mod>` — loose files land under root/data/<rel> like today.
    pub mod_root: PathBuf,
}

#[derive(Default)]
pub struct TerrainSidecarSink {
    /// Mod-root-relative sidecar paths (Terrain/<EDID>.btd4) registered by
    /// the driver after the terrain phase; consumed by the manifest + deploy
    /// step. Never packed.
    pub sidecars: Mutex<Vec<String>>,
}

impl TerrainSidecarSink {
    pub fn register(&self, rel: &str) {
        let mut guard = self.sidecars.lock().expect("sidecars mutex poisoned");
        if !guard.iter().any(|s| s == rel) {
            guard.push(rel.to_string());
        }
    }

    pub fn list(&self) -> Vec<String> {
        self.sidecars
            .lock()
            .expect("sidecars mutex poisoned")
            .clone()
    }
}

pub struct SinkSet {
    pub ba2: Option<Ba2ShardWriter>,
    pub loose: LooseSink,
    pub terrain: TerrainSidecarSink,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct SinkBatchFileReceipt {
    pub relative_path: String,
    pub byte_len: usize,
    pub blake3: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct SinkBatchReceipt {
    pub file_count: usize,
    pub loose_file_count: usize,
    pub ba2_entry_count: usize,
    pub files: Vec<SinkBatchFileReceipt>,
}

impl SinkBatchReceipt {
    pub fn canonical_json(&self) -> Result<String, String> {
        serde_json::to_string(self)
            .map_err(|error| format!("failed to serialize sink batch receipt: {error}"))
    }
}

struct FrozenSinkFile {
    relative_path: String,
    bytes: Vec<u8>,
}

pub struct PreparedSinkBatch<'sink> {
    sink: &'sink SinkSet,
    ba2: Option<PreparedBa2Batch>,
    receipt: SinkBatchReceipt,
}

impl PreparedSinkBatch<'_> {
    pub fn receipt(&self) -> &SinkBatchReceipt {
        &self.receipt
    }

    pub fn commit(self) -> Result<SinkBatchReceipt, String> {
        self.commit_inner(false)
    }

    fn commit_inner(self, fail_after_gnrl: bool) -> Result<SinkBatchReceipt, String> {
        if self.sink.loose.enabled {
            for file in &self.receipt.files {
                let published = self
                    .sink
                    .loose
                    .mod_root
                    .join("data")
                    .join(&file.relative_path);
                let bytes = std::fs::read(&published).map_err(|error| {
                    format!(
                        "prepared sink batch loose file is not published at {}: {error}",
                        published.display()
                    )
                })?;
                let actual_blake3 = blake3::hash(&bytes).to_hex();
                if bytes.len() != file.byte_len || actual_blake3.as_str() != file.blake3.as_str() {
                    return Err(format!(
                        "prepared sink batch loose file changed before commit: {}",
                        published.display()
                    ));
                }
            }
        }
        if let Some(batch) = self.ba2 {
            #[cfg(test)]
            if fail_after_gnrl {
                self.sink
                    .ba2
                    .as_ref()
                    .expect("prepared BA2 sink")
                    .commit_prepared_batch_with_failure(batch)?;
                unreachable!("injected BA2 failure must return an error");
            }
            #[cfg(not(test))]
            let _ = fail_after_gnrl;
            self.sink
                .ba2
                .as_ref()
                .expect("prepared BA2 sink")
                .commit_prepared_batch(batch)?;
        }
        Ok(self.receipt)
    }

    #[cfg(test)]
    fn commit_with_ba2_failure(self) -> Result<SinkBatchReceipt, String> {
        self.commit_inner(true)
    }
}

impl SinkSet {
    /// The phase-side write choke point. `rel` is data-relative
    /// ("Meshes/a.nif"). Writes loose (when enabled) and appends to the BA2
    /// spill. Either half failing is an item failure for the caller to count.
    pub fn write_asset(&self, rel: &str, bytes: &[u8]) -> Result<(), String> {
        if self.loose.enabled {
            let dst = self.loose.mod_root.join("data").join(rel);
            if let Some(parent) = dst.parent() {
                std::fs::create_dir_all(parent).map_err(|e| format!("loose mkdir: {e}"))?;
            }
            std::fs::write(&dst, bytes).map_err(|e| format!("loose write: {e}"))?;
        }
        if let Some(ba2) = &self.ba2 {
            ba2.add_bytes(rel, bytes)?;
        }
        Ok(())
    }

    /// Register an already-on-disk loose output with the BA2 spill (the
    /// skip-existing / loose-artifact route — NIFs, BTOs, sounds).
    pub fn add_existing_file(&self, rel: &str, abs: &Path) -> Result<bool, String> {
        match &self.ba2 {
            Some(ba2) => ba2.add_file(rel, abs),
            None => Ok(false),
        }
    }

    /// Freeze and pre-encode an already-produced file set without changing
    /// the loose tree or live BA2 spill/index. The caller may promote its
    /// batch roots and validate prepared records before committing.
    pub fn prepare_existing_files_batch(
        &self,
        registrations: &[(&str, &Path)],
    ) -> Result<PreparedSinkBatch<'_>, String> {
        let mut frozen = Vec::with_capacity(registrations.len());
        let mut seen = std::collections::BTreeSet::new();
        for (relative_path, absolute_path) in registrations {
            let relative_path = normalize_sink_relative_path(relative_path)?;
            let key = relative_path.to_ascii_lowercase();
            if !seen.insert(key) {
                return Err(format!(
                    "duplicate relative path in prepared sink batch: {relative_path}"
                ));
            }
            let bytes = std::fs::read(absolute_path).map_err(|error| {
                format!(
                    "read prepared sink batch file {}: {error}",
                    absolute_path.display()
                )
            })?;
            frozen.push(FrozenSinkFile {
                relative_path,
                bytes,
            });
        }
        frozen.sort_by(|left, right| {
            left.relative_path
                .to_ascii_lowercase()
                .cmp(&right.relative_path.to_ascii_lowercase())
        });

        let ba2_entries = frozen
            .iter()
            .map(|file| (file.relative_path.as_str(), file.bytes.as_slice()))
            .collect::<Vec<_>>();
        let ba2 = self
            .ba2
            .as_ref()
            .map(|ba2| ba2.prepare_bytes_batch(&ba2_entries))
            .transpose()?;
        let receipt_files = frozen
            .iter()
            .map(|file| SinkBatchFileReceipt {
                relative_path: file.relative_path.clone(),
                byte_len: file.bytes.len(),
                blake3: blake3::hash(&file.bytes).to_hex().to_string(),
            })
            .collect::<Vec<_>>();
        let file_count = receipt_files.len();
        Ok(PreparedSinkBatch {
            sink: self,
            ba2,
            receipt: SinkBatchReceipt {
                file_count,
                loose_file_count: if self.loose.enabled { file_count } else { 0 },
                ba2_entry_count: if self.ba2.is_some() { file_count } else { 0 },
                files: receipt_files,
            },
        })
    }
}

fn normalize_sink_relative_path(path: &str) -> Result<String, String> {
    let trimmed = path.trim();
    let rooted = trimmed.starts_with('/') || trimmed.starts_with('\\');
    let normalized = trimmed
        .replace('\\', "/")
        .trim_start_matches("./")
        .to_string();
    if rooted
        || normalized.is_empty()
        || normalized.contains(':')
        || normalized
            .split('/')
            .any(|part| part.is_empty() || matches!(part, "." | ".."))
    {
        return Err(format!("unsafe sink relative path: {path}"));
    }
    Ok(normalized)
}

/// Helper for phase write sites: None sink = byte-for-byte the legacy
/// create_dir_all + fs::write; Some = the SinkSet choke point.
pub fn sink_write_or_fs(
    sink: Option<&Arc<SinkSet>>,
    abs_dst: &Path,
    data_rel: &str,
    bytes: &[u8],
) -> Result<(), String> {
    match sink {
        None => {
            if let Some(parent) = abs_dst.parent() {
                std::fs::create_dir_all(parent).map_err(|e| format!("mkdir: {e}"))?;
            }
            std::fs::write(abs_dst, bytes).map_err(|e| format!("write: {e}"))
        }
        Some(s) => s.write_asset(data_rel, bytes),
    }
}

static NEXT_SINK_ID: AtomicU64 = AtomicU64::new(1);

pub fn sink_registry() -> &'static Mutex<HashMap<u64, Arc<SinkSet>>> {
    static SINKS: OnceLock<Mutex<HashMap<u64, Arc<SinkSet>>>> = OnceLock::new();
    SINKS.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn register_sink(sink: SinkSet) -> u64 {
    let id = NEXT_SINK_ID.fetch_add(1, Ordering::Relaxed);
    sink_registry()
        .lock()
        .expect("sink registry mutex poisoned")
        .insert(id, Arc::new(sink));
    id
}

pub fn sink_handle(id: u64) -> Result<Arc<SinkSet>, String> {
    sink_registry()
        .lock()
        .expect("sink registry mutex poisoned")
        .get(&id)
        .cloned()
        .ok_or_else(|| format!("unknown sink id: {id}"))
}

pub fn drop_sink(id: u64) -> Result<(), String> {
    sink_registry()
        .lock()
        .expect("sink registry mutex poisoned")
        .remove(&id)
        .map(|_| ())
        .ok_or_else(|| format!("unknown sink id: {id}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// SHARED FIXTURE TABLE — duplicated in
    /// bacup/py_bacup_lib/python/bacup_lib/tests/test_unified_sinks.py
    /// (cross-language classifier pin). Keep both lists in lockstep.
    const CLASSIFY_TABLE: &[(&str, &str)] = &[
        ("data/Textures/x.dds", "Textures"),
        ("Textures/x.dds", "Textures"),
        ("textures/sub/dir/y.DDS", "Textures"),
        ("Interface/i.swf", "Interface"),
        ("materials/a.bgsm", "Materials"),
        ("Misc/stray.bgem", "Materials"),
        ("Strings/SeventySix_en.STRINGS", "Strings"),
        ("Strings/SeventySix_en.dlstrings", "Strings"),
        ("Strings/SeventySix_en.ilstrings", "Strings"),
        ("sound/fx/a.xwm", "Sounds"),
        ("music/m.wav", "Sounds"),
        ("meshes/animations/a.hkx", "Animations"),
        ("misc/animations/note.txt", "Animations"),
        ("scripts/a.pex", "Scripts"),
        ("Scripts/Source/a.psc", "Scripts"),
        ("terrain/x.bto", "LOD"),
        ("meshes/terrain/world/x.btr", "LOD"),
        ("lod x.btr", "LOD"),
        ("meshes/a.nif", "Meshes"),
        ("stray.nif", "Meshes"),
        ("misc/readme.txt", "Main"),
        ("data/misc/readme.txt", "Main"),
        ("Vis/uvd/file.uvd", "Main"),
        ("noextension", "Main"),
    ];

    #[test]
    fn classify_family_matches_python_table() {
        for (rel, expected) in CLASSIFY_TABLE {
            assert_eq!(classify_family(rel), *expected, "classify_family({rel:?})");
        }
    }

    #[test]
    fn sink_set_write_dedup_and_terrain_rejection() {
        let tmp = std::env::temp_dir().join(format!(
            "sinks-mod-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mod_root = tmp.join("mod");
        let spill_dir = tmp.join("spill");
        let sink = SinkSet {
            ba2: Some(Ba2ShardWriter::new(spill_dir.clone()).unwrap()),
            loose: LooseSink {
                enabled: true,
                mod_root: mod_root.clone(),
            },
            terrain: TerrainSidecarSink::default(),
        };

        sink.write_asset("Meshes/a.nif", b"nif bytes").unwrap();
        let loose = mod_root.join("data").join("Meshes").join("a.nif");
        assert!(loose.is_file(), "loose file missing");
        assert_eq!(std::fs::read(&loose).unwrap(), b"nif bytes");

        let ba2 = sink.ba2.as_ref().unwrap();
        // Second add of the same rel is first-wins.
        assert!(!ba2.add_bytes("Meshes/a.nif", b"other").unwrap());
        let streamed = ba2.streamed_rel_paths();
        assert_eq!(streamed, vec!["meshes/a.nif".to_string()]);

        // Terrain sidecars never reach the BA2 sink.
        let err = ba2.add_bytes("Terrain/APPALACHIA.btd4", b"x").unwrap_err();
        assert!(err.contains("never packed"), "got: {err}");
        let err2 = sink.write_asset("Terrain/APPALACHIA.btd4", b"x");
        assert!(err2.is_err());

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn loose_disabled_still_streams_to_ba2() {
        let tmp = std::env::temp_dir().join(format!(
            "sinks-noloose-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let sink = SinkSet {
            ba2: Some(Ba2ShardWriter::new(tmp.join("spill")).unwrap()),
            loose: LooseSink {
                enabled: false,
                mod_root: tmp.join("mod"),
            },
            terrain: TerrainSidecarSink::default(),
        };
        sink.write_asset("Meshes/b.nif", b"bytes").unwrap();
        assert!(
            !tmp.join("mod")
                .join("data")
                .join("Meshes")
                .join("b.nif")
                .exists()
        );
        assert_eq!(
            sink.ba2.as_ref().unwrap().streamed_rel_paths(),
            vec!["meshes/b.nif".to_string()]
        );
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn terrain_sidecar_registry_dedups() {
        let t = TerrainSidecarSink::default();
        t.register("Terrain/APPALACHIA.btd4");
        t.register("Terrain/APPALACHIA.btd4");
        assert_eq!(t.list(), vec!["Terrain/APPALACHIA.btd4".to_string()]);
    }

    #[test]
    fn prepared_existing_batch_is_invisible_until_loose_and_ba2_commit() {
        let temp = tempfile::tempdir().unwrap();
        let mod_root = temp.path().join("mod");
        let staged = temp.path().join("staged");
        std::fs::create_dir_all(&staged).unwrap();
        let body = staged.join("body.nif");
        let skeleton = staged.join("skeleton.hkx");
        std::fs::write(&body, b"body").unwrap();
        std::fs::write(&skeleton, b"skeleton").unwrap();
        let sink = SinkSet {
            ba2: Some(Ba2ShardWriter::new(temp.path().join("spill")).unwrap()),
            loose: LooseSink {
                enabled: true,
                mod_root: mod_root.clone(),
            },
            terrain: TerrainSidecarSink::default(),
        };
        let registrations = [
            ("Meshes/Actors/Wolf/body.nif", body.as_path()),
            ("Meshes/Actors/Wolf/skeleton.hkx", skeleton.as_path()),
        ];

        let prepared = sink.prepare_existing_files_batch(&registrations).unwrap();
        assert_eq!(prepared.receipt().file_count, 2);
        assert_eq!(prepared.receipt().loose_file_count, 2);
        assert_eq!(prepared.receipt().ba2_entry_count, 2);
        assert_eq!(sink.ba2.as_ref().unwrap().entry_count(), 0);
        assert!(!mod_root.join("data/Meshes/Actors/Wolf/body.nif").exists());

        for (relative, source) in registrations {
            let destination = mod_root.join("data").join(relative);
            std::fs::create_dir_all(destination.parent().unwrap()).unwrap();
            std::fs::copy(source, destination).unwrap();
        }
        let receipt = prepared.commit().unwrap();
        assert_eq!(receipt.file_count, 2);
        assert_eq!(
            sink.ba2.as_ref().unwrap().streamed_rel_paths(),
            vec![
                "meshes/actors/wolf/body.nif".to_string(),
                "meshes/actors/wolf/skeleton.hkx".to_string(),
            ]
        );
        assert!(receipt.canonical_json().unwrap().contains("blake3"));
    }

    #[test]
    fn changed_loose_file_aborts_prepared_batch_without_ba2_entries() {
        let temp = tempfile::tempdir().unwrap();
        let mod_root = temp.path().join("mod");
        let staged = temp.path().join("body.nif");
        std::fs::write(&staged, b"staged").unwrap();
        let sink = SinkSet {
            ba2: Some(Ba2ShardWriter::new(temp.path().join("spill")).unwrap()),
            loose: LooseSink {
                enabled: true,
                mod_root: mod_root.clone(),
            },
            terrain: TerrainSidecarSink::default(),
        };
        let prepared = sink
            .prepare_existing_files_batch(&[("Meshes/Actors/Wolf/body.nif", staged.as_path())])
            .unwrap();
        let published = mod_root.join("data/Meshes/Actors/Wolf/body.nif");
        std::fs::create_dir_all(published.parent().unwrap()).unwrap();
        std::fs::write(&published, b"changed").unwrap();

        let error = prepared.commit().unwrap_err();
        assert!(error.contains("changed before commit"), "got: {error}");
        assert_eq!(sink.ba2.as_ref().unwrap().entry_count(), 0);
    }

    #[test]
    fn injected_ba2_batch_failure_restores_spill_and_index() {
        let temp = tempfile::tempdir().unwrap();
        let staged = temp.path().join("body.nif");
        std::fs::write(&staged, b"staged").unwrap();
        let sink = SinkSet {
            ba2: Some(Ba2ShardWriter::new(temp.path().join("spill")).unwrap()),
            loose: LooseSink {
                enabled: false,
                mod_root: temp.path().join("mod"),
            },
            terrain: TerrainSidecarSink::default(),
        };
        let prepared = sink
            .prepare_existing_files_batch(&[("Meshes/Actors/Wolf/body.nif", staged.as_path())])
            .unwrap();

        let error = prepared.commit_with_ba2_failure().unwrap_err();
        assert!(error.contains("injected BA2 batch failure"), "got: {error}");
        let ba2 = sink.ba2.as_ref().unwrap();
        assert_eq!(ba2.entry_count(), 0);
        assert!(ba2.streamed_rel_paths().is_empty());
        assert_eq!(
            std::fs::metadata(temp.path().join("spill/GNRL.spill"))
                .unwrap()
                .len(),
            0
        );
    }
}
