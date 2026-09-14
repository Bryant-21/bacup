//! Ba2ShardWriter: append-as-complete BA2 spill streaming.
//!
//! Two `bsarchive_native::incremental::IncrementalFo4Writer` spills: DX10 for
//! the Textures family ("fo4dds" archives) and GNRL for every other family
//! ("fo4"). Adds are routed by family; Terrain sidecars are rejected. The
//! Python planner (`archive_plan.plan_archive_outputs`) splits spills into
//! 16 GiB shards at join. One spill can feed several shards and one shard can
//! fold several families together (Strings→Main, Scripts absorbs Main), so
//! spills are keyed by format rather than family.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use bsarchive_native::incremental::{
    CompressionSettings, Fo4WriterKind, IncrementalFo4Writer, PreparedIncrementalFo4Batch,
};

use super::{classify_family, rejected_by_ba2};

pub struct Ba2ShardWriter {
    spill_dir: PathBuf,
    gnrl: IncrementalFo4Writer,
    dx10: IncrementalFo4Writer,
    transaction: Mutex<()>,
}

pub struct PreparedBa2Batch {
    gnrl: PreparedIncrementalFo4Batch,
    dx10: PreparedIncrementalFo4Batch,
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
            transaction: Mutex::new(()),
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
        let _transaction = self.transaction.lock().expect("BA2 transaction poisoned");
        self.writer_for(rel)?.add_bytes(rel, bytes)
    }

    /// First-wins; Ok(false) = already streamed.
    pub fn add_file(&self, rel: &str, path: &Path) -> Result<bool, String> {
        let _transaction = self.transaction.lock().expect("BA2 transaction poisoned");
        self.writer_for(rel)?.add_file(rel, path)
    }

    pub fn prepare_bytes_batch(
        &self,
        entries: &[(&str, &[u8])],
    ) -> Result<PreparedBa2Batch, String> {
        let _transaction = self.transaction.lock().expect("BA2 transaction poisoned");
        let mut gnrl = Vec::new();
        let mut dx10 = Vec::new();
        for &(rel, bytes) in entries {
            self.writer_for(rel)?;
            let entry = (rel, bytes);
            if classify_family(rel) == "Textures" {
                dx10.push(entry);
            } else {
                gnrl.push(entry);
            }
        }
        Ok(PreparedBa2Batch {
            gnrl: self.gnrl.prepare_bytes_batch(&gnrl)?,
            dx10: self.dx10.prepare_bytes_batch(&dx10)?,
        })
    }

    pub fn commit_prepared_batch(&self, batch: PreparedBa2Batch) -> Result<(), String> {
        self.commit_prepared_batch_inner(batch, false)
    }

    fn commit_prepared_batch_inner(
        &self,
        batch: PreparedBa2Batch,
        fail_after_gnrl: bool,
    ) -> Result<(), String> {
        let _transaction = self.transaction.lock().expect("BA2 transaction poisoned");
        let gnrl = self.gnrl.commit_prepared_batch(batch.gnrl)?;
        if fail_after_gnrl {
            self.gnrl.rollback_committed_batch(gnrl)?;
            return Err("injected BA2 batch failure after GNRL commit".to_string());
        }
        if let Err(error) = self.dx10.commit_prepared_batch(batch.dx10) {
            return match self.gnrl.rollback_committed_batch(gnrl) {
                Ok(()) => Err(error),
                Err(rollback) => Err(format!("{error}; GNRL batch rollback failed: {rollback}")),
            };
        }
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn commit_prepared_batch_with_failure(
        &self,
        batch: PreparedBa2Batch,
    ) -> Result<(), String> {
        self.commit_prepared_batch_inner(batch, true)
    }

    pub fn contains(&self, rel: &str) -> bool {
        let _transaction = self.transaction.lock().expect("BA2 transaction poisoned");
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
        let _transaction = self.transaction.lock().expect("BA2 transaction poisoned");
        let mut out = self.gnrl.rel_paths();
        out.extend(self.dx10.rel_paths());
        out.sort_unstable();
        out
    }

    pub fn entry_count(&self) -> usize {
        let _transaction = self.transaction.lock().expect("BA2 transaction poisoned");
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
        let _transaction = self.transaction.lock().expect("BA2 transaction poisoned");
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
        let _transaction = self.transaction.lock().expect("BA2 transaction poisoned");
        let _ = fs::remove_file(self.gnrl.spill_path());
        let _ = fs::remove_file(self.dx10.spill_path());
    }

    /// Delete the spill directory after a successful join.
    pub fn cleanup(&self) {
        let _transaction = self.transaction.lock().expect("BA2 transaction poisoned");
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
