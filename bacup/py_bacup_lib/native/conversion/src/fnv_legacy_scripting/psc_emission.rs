//! `.psc` file emission for translated FNV scripts.
//!
//! Writes each non-empty `psc_text` from the translated SCPT/QUST/INFO/SCEN
//! results to `<mod_path>/Source/User/<class_name>.psc` (UTF-8, overwriting).
//! Translation stays I/O-free so synthesizers can be unit-tested; an empty
//! `mod_path` (tests, handle-only runs) makes emission a no-op. Class names
//! derive deterministically from the source record, so reruns write identical
//! files.

use std::path::{Path, PathBuf};

use serde::Serialize;

use super::dialogue::{InfoFragmentPhase, TranslatedInfo};
use super::quest::TranslatedQuest;
use super::scene::TranslatedScene;
use super::script_synthesizer::{PackageDataAliasContract, TranslatedScript};

#[path = "slice_semantics.rs"]
mod slice_semantics;

// ---------------------------------------------------------------------------
// EmitReport
// ---------------------------------------------------------------------------

/// Summary of `.psc` emission. Returned to the caller for logging /
/// inclusion in `FnvLegacyScriptingResult`.
#[derive(Debug, Default, Clone)]
pub struct PscEmitReport {
    /// Number of .psc files successfully written.
    pub files_written: u32,
    /// Number of expected PSC files skipped because source was malformed,
    /// `psc_text` was empty, or `mod_path` was unset.
    pub files_skipped: u32,
    /// Errors encountered (one string per failed write).
    pub errors: Vec<String>,
    /// Exact generated-source manifest consumed by the FNV compile phase.
    pub generated_psc_classes: Vec<GeneratedPscClass>,
    /// Dynamic QUST data aliases required to preserve source AddScriptPackage calls.
    pub package_data_aliases: Vec<PackageDataAliasContract>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GeneratedPscClass {
    pub class_name: String,
    pub relative_source_path: String,
    pub kind: String,
    pub required: bool,
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Write every translated script's `.psc` text to `mod_path/Source/User/`.
///
/// I/O failures accumulate in `errors` instead of panicking. An empty
/// `mod_path` skips emission and counts every expected PSC in `files_skipped`.
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
        let total = scripts.len()
            + quests.len()
            + infos
                .iter()
                .filter(|info| info_has_psc_candidate(info))
                .count()
            + scenes.len();
        report.files_skipped = total as u32;
        return report;
    }

    let out_dir = mod_path.join("Scripts").join("Source").join("User");

    for s in scripts {
        write_one(
            &out_dir,
            &s.script_class_name,
            &s.psc_text,
            "scpt",
            &mut report,
        );
    }
    for q in quests {
        write_one(
            &out_dir,
            &q.fragment_class_name,
            &q.fragment_psc_text,
            "quest-fragment",
            &mut report,
        );
    }
    for i in infos {
        match (&i.fragment_class_name, &i.fragment_psc_text) {
            (Some(class_name), Some(text)) => {
                write_one(
                    &out_dir,
                    class_name,
                    text,
                    "topic-info-fragment",
                    &mut report,
                );
            }
            (None, None) => {}
            _ => report.files_skipped += 1,
        }
    }
    for s in scenes {
        write_one(
            &out_dir,
            &s.fragment_class_name,
            &s.fragment_psc_text,
            "scene-fragment",
            &mut report,
        );
    }
    report.package_data_aliases = scripts
        .iter()
        .flat_map(|script| script.package_data_aliases.iter().cloned())
        .collect();
    let compatibility_sources = slice_semantics::compatibility_sources(scripts, quests)
        .into_iter()
        .filter(|(class_name, _)| !class_name.ends_with("_TecMineHostageEscapeAlias"))
        .map(|(class_name, source)| {
            let source = if class_name.ends_with("_FnvSliceCompat") {
                audited_slice_compatibility_source(&class_name)
            } else {
                source
            };
            (class_name, source)
        })
        .collect::<Vec<_>>();
    for (class_name, source) in &compatibility_sources {
        write_one(
            &out_dir,
            class_name,
            source,
            "fnv-slice-compatibility",
            &mut report,
        );
    }

    report
        .generated_psc_classes
        .sort_by(|left, right| left.class_name.cmp(&right.class_name));
    if !report.generated_psc_classes.is_empty() {
        write_manifest(&out_dir, &mut report);
    }
    if !compatibility_sources.is_empty() {
        write_slice_integration_manifest(&out_dir, &mut report);
    }
    if !report.package_data_aliases.is_empty() {
        write_package_data_alias_manifest(&out_dir, &mut report);
    }

    report
}

fn info_has_psc_candidate(info: &TranslatedInfo) -> bool {
    info.fragment_class_name.is_some() || info.fragment_psc_text.is_some()
}

fn audited_slice_compatibility_source(class_name: &str) -> String {
    format!(
        "ScriptName {class_name} extends Quest\n\nInt Property RepNVNCRFame Auto\nInt Property RepNVNCRInfamy Auto\nBool Property PCCanUsePowerArmor Auto\n\nInt Function ReputationBumpForTier(Int aiTier)\n    If aiTier == 1\n        Return 1\n    ElseIf aiTier == 2\n        Return 2\n    ElseIf aiTier == 3\n        Return 4\n    ElseIf aiTier == 4\n        Return 7\n    ElseIf aiTier == 5\n        Return 12\n    EndIf\n    Return 0\nEndFunction\n\nFunction ModRepNVNCR(Int aiFameMode, Int aiTier)\n    Int amount = ReputationBumpForTier(aiTier)\n    If amount <= 0\n        Return\n    EndIf\n    If aiFameMode != 0\n        RepNVNCRFame += amount\n    Else\n        RepNVNCRInfamy += amount\n    EndIf\nEndFunction\n\nInt Function GetRepNVNCRFame()\n    Return RepNVNCRFame\nEndFunction\n\nInt Function GetRepNVNCRInfamy()\n    Return RepNVNCRInfamy\nEndFunction\n\nFunction SetPCCanUsePowerArmor(Bool abAllowed)\n    PCCanUsePowerArmor = abAllowed\nEndFunction\n\nBool Function GetPCCanUsePowerArmor()\n    Return PCCanUsePowerArmor\nEndFunction\n"
    )
}

fn write_package_data_alias_manifest(out_dir: &Path, report: &mut PscEmitReport) {
    let path = out_dir.join("fnv_package_data_alias_manifest.json");
    let bytes = match serde_json::to_vec_pretty(&report.package_data_aliases) {
        Ok(bytes) => bytes,
        Err(error) => {
            report.errors.push(format!(
                "psc_emission: serialize package data alias manifest failed: {error}"
            ));
            return;
        }
    };
    if let Err(error) = std::fs::write(&path, bytes) {
        report.errors.push(format!(
            "psc_emission: write {} failed: {error}",
            path.display()
        ));
    }
}

fn write_slice_integration_manifest(out_dir: &Path, report: &mut PscEmitReport) {
    let path = out_dir.join("fnv_slice_integration_manifest.json");
    let mut requirements = match serde_json::to_value(slice_semantics::integration_requirements()) {
        Ok(value) => value,
        Err(error) => {
            report.errors.push(format!(
                "psc_emission: serialize FNV slice integration manifest failed: {error}"
            ));
            return;
        }
    };
    if let Some(rows) = requirements.as_array_mut() {
        for row in rows {
            let source = row.get("source_record").and_then(serde_json::Value::as_str);
            if matches!(
                source,
                Some("PACK 1231B6 TecMineHostageEscape")
                    | Some("PACK 13289E TechaticupNCRRenoldsDialoguePackage")
            ) {
                row["target_record_type"] =
                    serde_json::json!("QUST unfilled data alias + SCPT VMAD");
                row["operation"] = serde_json::json!(
                    "set ALPC to the mapped PACK; bind ReferenceAlias property; ApplyToRef(actor) then EvaluatePackage(true)"
                );
                row["helper_class_suffix"] = serde_json::json!("");
            }
        }
    }
    let bytes = match serde_json::to_vec_pretty(&requirements) {
        Ok(bytes) => bytes,
        Err(error) => {
            report.errors.push(format!(
                "psc_emission: serialize FNV slice integration manifest failed: {error}"
            ));
            return;
        }
    };
    if let Err(error) = std::fs::write(&path, bytes) {
        report.errors.push(format!(
            "psc_emission: write {} failed: {error}",
            path.display()
        ));
    }
}

/// Internal: write one `.psc` file, accumulating into `report`.
/// Skips empty `text`; tries to create `out_dir` lazily.
fn write_one(out_dir: &Path, class_name: &str, text: &str, kind: &str, report: &mut PscEmitReport) {
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
        Ok(()) => {
            report.files_written += 1;
            report.generated_psc_classes.push(GeneratedPscClass {
                class_name: class_name.to_string(),
                relative_source_path: format!("Scripts/Source/User/{class_name}.psc"),
                kind: kind.to_string(),
                required: true,
            });
        }
        Err(e) => {
            report.errors.push(format!(
                "psc_emission: write {} failed: {e}",
                psc_path.display()
            ));
        }
    }
}

fn write_manifest(out_dir: &Path, report: &mut PscEmitReport) {
    let path = out_dir.join("fnv_generated_psc_manifest.json");
    let bytes = match serde_json::to_vec_pretty(&report.generated_psc_classes) {
        Ok(bytes) => bytes,
        Err(error) => {
            report
                .errors
                .push(format!("psc_emission: serialize manifest failed: {error}"));
            return;
        }
    };
    if let Err(error) = std::fs::write(&path, bytes) {
        report.errors.push(format!(
            "psc_emission: write {} failed: {error}",
            path.display()
        ));
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
            properties: Vec::new(),
            package_data_aliases: Vec::new(),
            dedicated_topics: Vec::new(),
            compile_status: super::super::script_synthesizer::ScriptCompileStatus::SourceGeneratedPendingCompile,
            terminal_status: super::super::script_synthesizer::ScriptTerminalStatus::SourceGeneratedPendingCompile,
        }
    }

    fn make_slice_scpt(source_form_key: &str) -> TranslatedScript {
        let mut script = make_scpt("B21_S_123191", "ScriptName B21_S_123191 extends Actor\n");
        script.source_form_key = source_form_key.into();
        script
    }

    fn make_quest(source_form_key: &str, class_name: &str) -> TranslatedQuest {
        TranslatedQuest {
            source_editor_id: "SourceQuestEidMetadata".into(),
            source_form_key: source_form_key.into(),
            fragment_class_name: class_name.into(),
            fragment_psc_text: format!("ScriptName {class_name} extends Quest\n"),
            aliases: Vec::new(),
            stage_fragments: Vec::new(),
            fragment_properties: Vec::new(),
            fragment_adaptations: Vec::new(),
            unresolved_reference_names: Vec::new(),
            authoring_record_payload: None,
            warnings: Vec::new(),
        }
    }

    fn make_info(
        source_form_key: &str,
        fragment_class_name: Option<&str>,
        fragment_psc_text: Option<&str>,
    ) -> TranslatedInfo {
        TranslatedInfo {
            source_form_key: source_form_key.into(),
            fragment_class_name: fragment_class_name.map(str::to_string),
            fragment_psc_text: fragment_psc_text.map(str::to_string),
            fragment_phases: Vec::new(),
            fragment_properties: Vec::new(),
            voice_target_path: String::new(),
            voice_source_path: String::new(),
            lip_dropped: false,
            lip_regeneration_target: None,
            authoring_record_payload: None,
            warnings: Vec::new(),
        }
    }

    #[test]
    fn emits_scpt_files_under_source_user() {
        let dir = tempfile::tempdir().unwrap();
        let scripts = vec![
            make_scpt(
                "B21_S_001234",
                "ScriptName B21_S_001234 extends ObjectReference\n",
            ),
            make_scpt(
                "B21_S_001235",
                "ScriptName B21_S_001235 extends ObjectReference\n",
            ),
        ];
        let report = emit_psc_files(dir.path(), &scripts, &[], &[], &[]);
        assert_eq!(report.files_written, 2);
        assert_eq!(report.files_skipped, 0);
        assert!(report.errors.is_empty());

        let out_dir = dir.path().join("Scripts").join("Source").join("User");
        assert!(out_dir.join("B21_S_001234.psc").is_file());
        assert!(out_dir.join("B21_S_001235.psc").is_file());

        let content = std::fs::read_to_string(out_dir.join("B21_S_001234.psc")).unwrap();
        assert!(content.contains("ScriptName B21_S_001234"));
    }

    #[test]
    fn empty_mod_path_is_noop_but_counts_skipped() {
        let scripts = vec![make_scpt("B21_S_001234", "body")];
        let empty: PathBuf = PathBuf::new();
        let report = emit_psc_files(&empty, &scripts, &[], &[], &[]);
        assert_eq!(report.files_written, 0);
        assert_eq!(report.files_skipped, 1);
        assert!(report.errors.is_empty());
    }

    #[test]
    fn empty_psc_text_is_skipped() {
        let dir = tempfile::tempdir().unwrap();
        let scripts = vec![make_scpt("B21_S_001234", "")];
        let report = emit_psc_files(dir.path(), &scripts, &[], &[], &[]);
        assert_eq!(report.files_written, 0);
        assert_eq!(report.files_skipped, 1);
        // Out dir not necessarily created when no real writes happen.
        assert!(
            !dir.path()
                .join("Scripts")
                .join("Source")
                .join("User")
                .join("B21_S_001234.psc")
                .exists()
        );
    }

    #[test]
    fn info_without_fragment_is_not_a_psc_candidate() {
        let dir = tempfile::tempdir().unwrap();
        let info = make_info("001234:FNV.esm", None, None);
        let report = emit_psc_files(dir.path(), &[], &[], &[info], &[]);
        assert_eq!(report.files_written, 0);
        assert_eq!(report.files_skipped, 0);
        assert!(report.generated_psc_classes.is_empty());
    }

    #[test]
    fn malformed_or_empty_info_fragment_remains_terminally_skipped() {
        let dir = tempfile::tempdir().unwrap();
        let infos = vec![
            make_info("001234:FNV.esm", Some("TIF__001234"), None),
            make_info(
                "001235:FNV.esm",
                None,
                Some("ScriptName TIF__001235 extends TopicInfo\n"),
            ),
            make_info("001236:FNV.esm", Some("TIF__001236"), Some("")),
        ];
        let report = emit_psc_files(dir.path(), &[], &[], &infos, &[]);
        assert_eq!(report.files_written, 0);
        assert_eq!(report.files_skipped, 3);
        assert!(report.generated_psc_classes.is_empty());
    }

    #[test]
    fn exact_hostage_greeting_infos_without_fragments_do_not_inflate_psc_skips() {
        let dir = tempfile::tempdir().unwrap();
        let infos = vec![
            make_info(
                "130161:FalloutNV.esm",
                Some("TIF__130161"),
                Some("ScriptName TIF__130161 extends TopicInfo\n"),
            ),
            make_info(
                "134B9B:FalloutNV.esm",
                Some("TIF__134B9B"),
                Some("ScriptName TIF__134B9B extends TopicInfo\n"),
            ),
            make_info("15734B:FalloutNV.esm", None, None),
            make_info("15734C:FalloutNV.esm", None, None),
            make_info("15734D:FalloutNV.esm", None, None),
        ];
        let report = emit_psc_files(dir.path(), &[], &[], &infos, &[]);
        assert_eq!(report.files_written, 2);
        assert_eq!(report.files_skipped, 0);
        assert_eq!(
            report
                .generated_psc_classes
                .iter()
                .map(|entry| entry.class_name.as_str())
                .collect::<Vec<_>>(),
            ["TIF__130161", "TIF__134B9B"]
        );
    }

    #[test]
    fn info_with_class_name_and_text_emits_file() {
        let dir = tempfile::tempdir().unwrap();
        let info = TranslatedInfo {
            source_form_key: "001234:FNV.esm".into(),
            fragment_class_name: Some("TIF__001234".into()),
            fragment_psc_text: Some("ScriptName TIF__001234 extends TopicInfo\n".into()),
            fragment_phases: vec![InfoFragmentPhase::Begin],
            fragment_properties: Vec::new(),
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
            .join("Scripts")
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
            fragment_phases: vec![InfoFragmentPhase::Begin],
            fragment_properties: Vec::new(),
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

    #[test]
    fn emits_compile_manifest_with_exact_relative_paths() {
        let dir = tempfile::tempdir().unwrap();
        let report = emit_psc_files(
            dir.path(),
            &[make_scpt(
                "B21_S_001234",
                "ScriptName B21_S_001234 extends Quest\n",
            )],
            &[],
            &[],
            &[],
        );
        assert_eq!(
            report.generated_psc_classes,
            vec![GeneratedPscClass {
                class_name: "B21_S_001234".into(),
                relative_source_path: "Scripts/Source/User/B21_S_001234.psc".into(),
                kind: "scpt".into(),
                required: true,
            }]
        );
        let manifest = dir
            .path()
            .join("Scripts/Source/User/fnv_generated_psc_manifest.json");
        assert!(manifest.is_file());
        let payload: serde_json::Value =
            serde_json::from_slice(&std::fs::read(manifest).unwrap()).unwrap();
        assert_eq!(payload[0]["class_name"], "B21_S_001234");
        assert_eq!(payload[0]["required"], true);
    }

    #[test]
    fn exact_slice_manifest_filenames_and_script_names_use_the_same_compact_classes() {
        let dir = tempfile::tempdir().unwrap();
        let scripts = [
            (0x11FC64, "FNV_FO3_S_11FC64"),
            (0x123191, "FNV_FO3_S_123191"),
            (0x134491, "FNV_FO3_S_134491"),
            (0x166305, "FNV_FO3_S_166305"),
        ]
        .map(|(local, class_name)| {
            let mut script = make_scpt(
                class_name,
                &format!("ScriptName {class_name} extends Quest\n"),
            );
            script.source_form_key = format!("{local:06X}:FalloutNV.esm");
            script
        });
        let quests = [
            make_quest("06136D:FalloutNV.esm", "QF_FNV_FO3_06136D"),
            make_quest("11F935:FalloutNV.esm", "QF_FNV_FO3_11F935"),
        ];
        let infos = [
            make_info(
                "130161:FalloutNV.esm",
                Some("TIF__130161"),
                Some("ScriptName TIF__130161 extends TopicInfo\n"),
            ),
            make_info(
                "134B9B:FalloutNV.esm",
                Some("TIF__134B9B"),
                Some("ScriptName TIF__134B9B extends TopicInfo\n"),
            ),
        ];
        let report = emit_psc_files(dir.path(), &scripts, &quests, &infos, &[]);
        assert_eq!(report.files_written, 9);
        assert_eq!(report.files_skipped, 0);
        assert!(report.errors.is_empty());

        let expected = [
            "FNV_FO3_FnvSliceCompat",
            "FNV_FO3_S_11FC64",
            "FNV_FO3_S_123191",
            "FNV_FO3_S_134491",
            "FNV_FO3_S_166305",
            "QF_FNV_FO3_06136D",
            "QF_FNV_FO3_11F935",
            "TIF__130161",
            "TIF__134B9B",
        ];
        assert_eq!(
            report
                .generated_psc_classes
                .iter()
                .map(|entry| entry.class_name.as_str())
                .collect::<Vec<_>>(),
            expected
        );
        for entry in &report.generated_psc_classes {
            assert!(entry.class_name.len() <= 38);
            assert_eq!(
                entry.relative_source_path,
                format!("Scripts/Source/User/{}.psc", entry.class_name)
            );
            let source =
                std::fs::read_to_string(dir.path().join(&entry.relative_source_path)).unwrap();
            assert!(source.starts_with(&format!("ScriptName {} ", entry.class_name)));
        }
    }

    #[test]
    fn exact_hostage_slice_emits_compilable_helpers_and_runtime_contract() {
        let dir = tempfile::tempdir().unwrap();
        let mut script = make_slice_scpt("123191:FalloutNV.esm");
        script.package_data_aliases = vec![PackageDataAliasContract {
            source_script_form_key: "123191:FalloutNV.esm".into(),
            owner_quest_property: "VTechatticup".into(),
            source_quest_form_key: "11F935:FalloutNV.esm".into(),
            target_quest_form_key: "21F935:Target.esp".into(),
            actor_expression: "Self".into(),
            property_name: "TecMineHostageEscapeData".into(),
            requested_package_property: "TecMineHostageEscape".into(),
            source_package_form_key: "1231B6:FalloutNV.esm".into(),
            target_package_form_key: "2231B6:Target.esp".into(),
            alias_package_subrecord: "ALPC".into(),
            operation: "TecMineHostageEscapeData.ApplyToRef(Self); Self.EvaluatePackage(true)"
                .into(),
        }];
        let report = emit_psc_files(dir.path(), &[script], &[], &[], &[]);
        assert!(report.errors.is_empty());
        let source_root = dir.path().join("Scripts/Source/User");
        assert!(
            !source_root
                .join("B21_TecMineHostageEscapeAlias.psc")
                .exists()
        );
        let compatibility =
            std::fs::read_to_string(source_root.join("B21_FnvSliceCompat.psc")).unwrap();
        assert!(compatibility.contains("ElseIf aiTier == 3\n        Return 4"));
        assert!(compatibility.contains("RepNVNCRFame += amount"));
        assert!(compatibility.contains("RepNVNCRInfamy += amount"));
        assert!(!compatibility.contains("+= aiAmount"));
        assert!(compatibility.contains("PCCanUsePowerArmor = abAllowed"));
        let manifest: serde_json::Value = serde_json::from_slice(
            &std::fs::read(source_root.join("fnv_slice_integration_manifest.json")).unwrap(),
        )
        .unwrap();
        assert!(manifest.as_array().unwrap().iter().any(|row| {
            row["source_record"] == "PACK 13289E TechaticupNCRRenoldsDialoguePackage"
                && row["operation"]
                    == "set ALPC to the mapped PACK; bind ReferenceAlias property; ApplyToRef(actor) then EvaluatePackage(true)"
        }));
        let package_manifest: serde_json::Value = serde_json::from_slice(
            &std::fs::read(source_root.join("fnv_package_data_alias_manifest.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(package_manifest[0]["alias_package_subrecord"], "ALPC");
        assert_eq!(
            package_manifest[0]["source_package_form_key"],
            "1231B6:FalloutNV.esm"
        );
        assert_eq!(
            package_manifest[0]["target_package_form_key"],
            "2231B6:Target.esp"
        );
    }
}
