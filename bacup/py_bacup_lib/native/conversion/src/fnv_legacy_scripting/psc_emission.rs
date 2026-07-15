//! `.psc` file emission for translated FNV scripts.
//!
//! # What this does
//! After the FNV legacy scripting phase translates SCPT/QUST/INFO/SCEN
//! records into Papyrus source text on `TranslatedScript`/`TranslatedQuest`/
//! `TranslatedInfo`/`TranslatedScene`, this module flushes each non-empty
//! `psc_text` to a file under `mod_path/Source/User/<class_name>.psc`.
//!
//! The translation step is intentionally I/O-free so the
//! synthesizers can be unit-tested with handcrafted records. This module is
//! the seam where file I/O happens — gated on a non-empty `mod_path`. When
//! `mod_path` is empty (test mode / handle-only runs) emission is a no-op.
//!
//! # Output directory
//! `<mod_path>/Source/User/` is created on demand. Files use UTF-8 encoding
//! with the file name `<class_name>.psc`. Existing files are overwritten —
//! the synthesizer's class name is derived deterministically from the
//! source record, so two runs of the same conversion produce identical
//! file content.

use std::path::{Path, PathBuf};

use super::dialogue::TranslatedInfo;
use super::quest::TranslatedQuest;
use super::scene::TranslatedScene;
use super::script_synthesizer::TranslatedScript;

// ---------------------------------------------------------------------------
// EmitReport
// ---------------------------------------------------------------------------

/// Summary of `.psc` emission. Returned to the caller for logging /
/// inclusion in `FnvLegacyScriptingResult`.
#[derive(Debug, Default, Clone)]
pub struct PscEmitReport {
    /// Number of .psc files successfully written.
    pub files_written: u32,
    /// Number of records skipped because `psc_text` was empty or `mod_path`
    /// was unset.
    pub files_skipped: u32,
    /// Errors encountered (one string per failed write).
    pub errors: Vec<String>,
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Write every translated script's `.psc` text to `mod_path/Source/User/`.
///
/// Returns a `PscEmitReport` summarising the result. Never panics on I/O
/// errors — individual failures are accumulated in `errors`. When `mod_path`
/// is empty, the entire emission is skipped and every record is counted in
/// `files_skipped`.
pub fn emit_psc_files(
    mod_path: &Path,
    scripts: &[TranslatedScript],
    quests: &[TranslatedQuest],
    infos: &[TranslatedInfo],
    scenes: &[TranslatedScene],
) -> PscEmitReport {
    let mut report = PscEmitReport::default();

    // mod_path empty → no-op (test / handle-only runs).
    if mod_path.as_os_str().is_empty() {
        let total = scripts.len() + quests.len() + infos.len() + scenes.len();
        report.files_skipped = total as u32;
        return report;
    }

    let out_dir = mod_path.join("Source").join("User");

    for s in scripts {
        write_one(&out_dir, &s.script_class_name, &s.psc_text, &mut report);
    }
    for q in quests {
        write_one(
            &out_dir,
            &q.fragment_class_name,
            &q.fragment_psc_text,
            &mut report,
        );
    }
    for i in infos {
        match (&i.fragment_class_name, &i.fragment_psc_text) {
            (Some(class_name), Some(text)) => {
                write_one(&out_dir, class_name, text, &mut report);
            }
            _ => report.files_skipped += 1,
        }
    }
    for s in scenes {
        write_one(
            &out_dir,
            &s.fragment_class_name,
            &s.fragment_psc_text,
            &mut report,
        );
    }

    report
}

/// Internal: write one `.psc` file, accumulating into `report`.
/// Skips empty `text`; tries to create `out_dir` lazily.
fn write_one(out_dir: &Path, class_name: &str, text: &str, report: &mut PscEmitReport) {
    if text.is_empty() {
        report.files_skipped += 1;
        return;
    }
    if let Err(e) = ensure_dir(out_dir) {
        report.errors.push(format!(
            "psc_emission: create {} failed: {e}",
            out_dir.display()
        ));
        return;
    }
    let psc_path: PathBuf = out_dir.join(format!("{class_name}.psc"));
    match std::fs::write(&psc_path, text) {
        Ok(()) => report.files_written += 1,
        Err(e) => {
            report.errors.push(format!(
                "psc_emission: write {} failed: {e}",
                psc_path.display()
            ));
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Create `path` (and any missing parents) if it doesn't exist. Idempotent.
fn ensure_dir(path: &Path) -> std::io::Result<()> {
    if path.is_dir() {
        Ok(())
    } else {
        std::fs::create_dir_all(path)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::super::script_synthesizer::{PapyrusType, TranslatedScript};
    use super::*;
    use std::path::PathBuf;

    fn make_scpt(class_name: &str, psc_text: &str) -> TranslatedScript {
        TranslatedScript {
            source_editor_id: "TestScript".into(),
            source_form_key: "001234:FNV.esm".into(),
            script_class_name: class_name.into(),
            papyrus_type: PapyrusType::ObjectReference,
            psc_text: psc_text.into(),
        }
    }

    #[test]
    fn emits_scpt_files_under_source_user() {
        let dir = tempfile::tempdir().unwrap();
        let scripts = vec![
            make_scpt(
                "B21_nv_Foo",
                "ScriptName B21_nv_Foo extends ObjectReference\n",
            ),
            make_scpt(
                "B21_nv_Bar",
                "ScriptName B21_nv_Bar extends ObjectReference\n",
            ),
        ];
        let report = emit_psc_files(dir.path(), &scripts, &[], &[], &[]);
        assert_eq!(report.files_written, 2);
        assert_eq!(report.files_skipped, 0);
        assert!(report.errors.is_empty());

        let out_dir = dir.path().join("Source").join("User");
        assert!(out_dir.join("B21_nv_Foo.psc").is_file());
        assert!(out_dir.join("B21_nv_Bar.psc").is_file());

        let content = std::fs::read_to_string(out_dir.join("B21_nv_Foo.psc")).unwrap();
        assert!(content.contains("ScriptName B21_nv_Foo"));
    }

    #[test]
    fn empty_mod_path_is_noop_but_counts_skipped() {
        let scripts = vec![make_scpt("B21_nv_Foo", "body")];
        let empty: PathBuf = PathBuf::new();
        let report = emit_psc_files(&empty, &scripts, &[], &[], &[]);
        assert_eq!(report.files_written, 0);
        assert_eq!(report.files_skipped, 1);
        assert!(report.errors.is_empty());
    }

    #[test]
    fn empty_psc_text_is_skipped() {
        let dir = tempfile::tempdir().unwrap();
        let scripts = vec![make_scpt("B21_nv_Empty", "")];
        let report = emit_psc_files(dir.path(), &scripts, &[], &[], &[]);
        assert_eq!(report.files_written, 0);
        assert_eq!(report.files_skipped, 1);
        // Out dir not necessarily created when no real writes happen.
        assert!(
            !dir.path()
                .join("Source")
                .join("User")
                .join("B21_nv_Empty.psc")
                .exists()
        );
    }

    #[test]
    fn info_with_no_fragment_class_name_is_skipped() {
        let dir = tempfile::tempdir().unwrap();
        let info = TranslatedInfo {
            source_form_key: "001234:FNV.esm".into(),
            fragment_class_name: None,
            fragment_psc_text: None,
            voice_target_path: String::new(),
            voice_source_path: String::new(),
            lip_dropped: false,
            lip_regeneration_target: None,
            authoring_record_payload: None,
            warnings: Vec::new(),
        };
        let report = emit_psc_files(dir.path(), &[], &[], &[info], &[]);
        assert_eq!(report.files_written, 0);
        assert_eq!(report.files_skipped, 1);
    }

    #[test]
    fn info_with_class_name_and_text_emits_file() {
        let dir = tempfile::tempdir().unwrap();
        let info = TranslatedInfo {
            source_form_key: "001234:FNV.esm".into(),
            fragment_class_name: Some("TIF__001234".into()),
            fragment_psc_text: Some("ScriptName TIF__001234 extends TopicInfo\n".into()),
            voice_target_path: String::new(),
            voice_source_path: String::new(),
            lip_dropped: false,
            lip_regeneration_target: None,
            authoring_record_payload: None,
            warnings: Vec::new(),
        };
        let report = emit_psc_files(dir.path(), &[], &[], &[info], &[]);
        assert_eq!(report.files_written, 1);
        assert_eq!(report.files_skipped, 0);
        let out = dir
            .path()
            .join("Source")
            .join("User")
            .join("TIF__001234.psc");
        assert!(out.is_file());
    }

    #[test]
    fn mixed_kinds_all_emit() {
        let dir = tempfile::tempdir().unwrap();
        let scripts = vec![make_scpt("Script1", "body1")];
        let infos = vec![TranslatedInfo {
            source_form_key: "002:FNV.esm".into(),
            fragment_class_name: Some("Info1".into()),
            fragment_psc_text: Some("body-info".into()),
            voice_target_path: String::new(),
            voice_source_path: String::new(),
            lip_dropped: false,
            lip_regeneration_target: None,
            authoring_record_payload: None,
            warnings: Vec::new(),
        }];
        let report = emit_psc_files(dir.path(), &scripts, &[], &infos, &[]);
        assert_eq!(report.files_written, 2);
        assert_eq!(report.files_skipped, 0);
    }

    #[test]
    fn ensure_dir_is_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let nested = dir.path().join("a").join("b").join("c");
        assert!(!nested.exists());
        ensure_dir(&nested).unwrap();
        assert!(nested.is_dir());
        // Second call should not error.
        ensure_dir(&nested).unwrap();
    }
}
