//! Ba2ShardWriter — append-as-complete BA2 spill streaming.
//!
//! Backed by TWO `bsarchive_native::incremental::IncrementalFo4Writer`s:
//! one DX10 spill for the Textures family ("fo4dds" archives) and one GNRL
//! spill shared by every other family ("fo4" archives). Family
//! classification routes adds and rejects Terrain sidecars; the per-family
//! 16 GiB shard SPLITS come from the Python planner at join
//! (archive_plan.plan_archive_outputs) — one spill can feed multiple
//! planned shards, including shards whose membership folds several families
//! together (Strings→Main, Scripts-absorbs-Main), which is why the spills
//! are format-keyed rather than family-keyed (as-built deviation, declared).

use std::fs;
use std::path::{Path, PathBuf};

use bsarchive_native::incremental::{CompressionSettings, Fo4WriterKind, IncrementalFo4Writer};

use super::{classify_family, rejected_by_ba2};

pub struct Ba2ShardWriter {
    spill_dir: PathBuf,
    gnrl: IncrementalFo4Writer,
    dx10: IncrementalFo4Writer,
}

impl Ba2ShardWriter {
    pub fn new(spill_dir: PathBuf) -> Result<Self, String> {
        fs::create_dir_all(&spill_dir).map_err(|e| format!("spill dir: {e}"))?;
        // Per-format levels from bsarchive's single source of truth (DX10=4, GNRL=6).
        let gnrl = IncrementalFo4Writer::new(
            spill_dir.join("GNRL.spill"),
            Fo4WriterKind::Gnrl,
            CompressionSettings::for_writer_kind(Fo4WriterKind::Gnrl),
        )?;
        let dx10 = IncrementalFo4Writer::new(
            spill_dir.join("DX10.spill"),
            Fo4WriterKind::Dx10,
            CompressionSettings::for_writer_kind(Fo4WriterKind::Dx10),
        )?;
        Ok(Self {
            spill_dir,
            gnrl,
            dx10,
        })
    }

    fn writer_for(&self, rel: &str) -> Result<&IncrementalFo4Writer, String> {
        if rejected_by_ba2(rel) {
            return Err(format!(
                "terrain sidecars are never packed (BA2 sink rejects {rel})"
            ));
        }
        Ok(if classify_family(rel) == "Textures" {
            &self.dx10
        } else {
            &self.gnrl
        })
    }

    /// First-wins; Ok(false) = already streamed.
    pub fn add_bytes(&self, rel: &str, bytes: &[u8]) -> Result<bool, String> {
        self.writer_for(rel)?.add_bytes(rel, bytes)
    }

    /// First-wins; Ok(false) = already streamed.
    pub fn add_file(&self, rel: &str, path: &Path) -> Result<bool, String> {
        self.writer_for(rel)?.add_file(rel, path)
    }

    pub fn contains(&self, rel: &str) -> bool {
        if rejected_by_ba2(rel) {
            return false;
        }
        if classify_family(rel) == "Textures" {
            self.dx10.contains(rel)
        } else {
            self.gnrl.contains(rel)
        }
    }

    /// Normalized (lowercase, forward-slash) rel paths streamed so far.
    pub fn streamed_rel_paths(&self) -> Vec<String> {
        let mut out = self.gnrl.rel_paths();
        out.extend(self.dx10.rel_paths());
        out.sort_unstable();
        out
    }

    pub fn entry_count(&self) -> usize {
        self.gnrl.entry_count() + self.dx10.entry_count()
    }

    /// Write one PLANNED archive (texture_archive comes from the planner's
    /// PlannedArchive flag) containing exactly `ordered_rels`.
    pub fn finalize_archive(
        &self,
        output: &Path,
        texture_archive: bool,
        ordered_rels: &[&str],
    ) -> Result<(), String> {
        let writer = if texture_archive {
            &self.dx10
        } else {
            &self.gnrl
        };
        writer.finalize(output, ordered_rels)
    }

    /// Delete the spill files (abort path; the Python side deletes partial
    /// *.ba2 outputs).
    pub fn abort(&self) {
        let _ = fs::remove_file(self.gnrl.spill_path());
        let _ = fs::remove_file(self.dx10.spill_path());
    }

    /// Delete the spill directory after a successful join.
    pub fn cleanup(&self) {
        let _ = fs::remove_file(self.gnrl.spill_path());
        let _ = fs::remove_file(self.dx10.spill_path());
        let _ = fs::remove_dir(&self.spill_dir);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_dir(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "ba2-shard-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    #[test]
    fn routes_textures_to_dx10_and_rest_to_gnrl() {
        let tmp = tmp_dir("route");
        let w = Ba2ShardWriter::new(tmp.join("spill")).unwrap();
        // Non-dds under textures/ is a Textures-family member: the DX10
        // writer must reject it exactly like the one-shot "fo4dds" pack.
        let err = w.add_bytes("Textures/readme.txt", b"x").unwrap_err();
        assert!(err.contains("DDS"), "got: {err}");
        // GNRL adds for every other family.
        assert!(w.add_bytes("Meshes/a.nif", b"m").unwrap());
        assert!(w.add_bytes("Sound/fx/s.xwm", b"s").unwrap());
        assert!(w.add_bytes("misc/readme.txt", b"r").unwrap());
        assert_eq!(w.entry_count(), 3);
        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn finalize_pulls_across_family_folds() {
        // Strings fold into Main-family planned archives: one GNRL spill
        // must serve a planned archive mixing both families.
        let tmp = tmp_dir("fold");
        let w = Ba2ShardWriter::new(tmp.join("spill")).unwrap();
        w.add_bytes("Strings/X_en.STRINGS", b"strings bytes")
            .unwrap();
        w.add_bytes("misc/readme.txt", b"main bytes").unwrap();
        let out = tmp.join("X - Main.ba2");
        w.finalize_archive(&out, false, &["Strings/X_en.STRINGS", "misc/readme.txt"])
            .unwrap();
        assert!(out.is_file());
        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn abort_removes_spills() {
        let tmp = tmp_dir("abort");
        let w = Ba2ShardWriter::new(tmp.join("spill")).unwrap();
        w.add_bytes("Meshes/a.nif", b"m").unwrap();
        assert!(tmp.join("spill").join("GNRL.spill").is_file());
        w.abort();
        assert!(!tmp.join("spill").join("GNRL.spill").exists());
        assert!(!tmp.join("spill").join("DX10.spill").exists());
        let _ = fs::remove_dir_all(&tmp);
    }
}
