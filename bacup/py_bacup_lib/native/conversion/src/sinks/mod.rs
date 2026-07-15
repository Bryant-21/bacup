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
}
