use std::io::Write;
use std::path::{Path, PathBuf};

use bsarchive_native::python::{extract_one_impl, list_archive_files};
use havok_native::hkx::HkxFile;

use crate::phase::{LogLevel, PhaseCtx, PhaseError, PhaseEvent};

use super::{MIN_HKX_LEN, SHORT_INPUT};

fn archive_order(path: &Path) -> (u32, String) {
    let name = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_lowercase();
    let phase = name
        .strip_prefix("seventysix - ")
        .and_then(|rest| rest.split_once("update"))
        .and_then(|(number, _)| number.parse::<u32>().ok())
        .map_or(0, |number| number + 1);
    (phase, name)
}

fn restore(
    path: &Path,
    source_root: &Path,
    archive_dirs: &[PathBuf],
    mut check_cancel: impl FnMut() -> Result<(), String>,
) -> Result<(PathBuf, usize), String> {
    let source = source_root.canonicalize().map_err(|e| e.to_string())?;
    let path = path.canonicalize().map_err(|e| e.to_string())?;
    let relative = path
        .strip_prefix(&source)
        .map_err(|_| "The damaged file is outside the selected FO76 extraction.".to_string())?;
    let mut member = relative.to_string_lossy().replace('\\', "/").to_lowercase();
    if source
        .file_name()
        .is_some_and(|name| name.eq_ignore_ascii_case("meshes"))
    {
        member = format!("meshes/{member}");
    }
    if !member.starts_with("meshes/") || !member.ends_with(".hkx") {
        return Err("The damaged file is not an extracted Havok mesh asset.".into());
    }
    let mut archives = Vec::new();
    for dir in archive_dirs {
        for entry in std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))? {
            let candidate = entry.map_err(|e| e.to_string())?.path();
            let name = candidate
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_lowercase();
            if candidate.is_file() && name.starts_with("seventysix - ") && name.ends_with(".ba2") {
                archives.push(candidate);
            }
        }
    }
    archives.sort_by_key(|path| archive_order(path));
    archives.dedup();
    for archive in archives.into_iter().rev() {
        check_cancel()?;
        let members = list_archive_files(&archive)
            .map_err(|e| format!("Cannot read {}: {e}", archive.display()))?;
        let Some(member) = members
            .iter()
            .find(|name| name.replace('\\', "/").eq_ignore_ascii_case(&member))
        else {
            continue;
        };
        // A broken newest entry must not silently fall back to an older game version.
        let bytes = extract_one_impl(&archive, member)
            .map_err(|e| format!("Cannot extract from {}: {e}", archive.display()))?;
        HkxFile::read(&bytes)
            .map_err(|e| format!("The copy in {} is also invalid: {e}", archive.display()))?;
        check_cancel()?;
        if std::fs::metadata(&path).map_err(|e| e.to_string())?.len() >= MIN_HKX_LEN as u64 {
            return Err("The file changed during recovery; it was not overwritten.".into());
        }
        let mut staged =
            tempfile::NamedTempFile::new_in(path.parent().unwrap()).map_err(|e| e.to_string())?;
        staged.write_all(&bytes).map_err(|e| e.to_string())?;
        staged.persist(&path).map_err(|e| e.to_string())?;
        return Ok((archive, bytes.len()));
    }
    Err("No installed FO76 archive contains this file. Verify the Fallout 76 installation.".into())
}

pub fn build(
    ctx: &PhaseCtx<'_>,
    target: &Path,
) -> Result<super::assets::BehaviorGenerationReport, PhaseError> {
    let generate = || super::assets::build_profiled(ctx.mod_path, ctx.source_extracted_dir, target);
    let error = match generate() {
        Ok(report) => return Ok(report),
        Err(error) => error,
    };
    let Some(path) = error.strip_suffix(SHORT_INPUT).map(PathBuf::from) else {
        return Err(PhaseError::Internal(error));
    };
    let size = std::fs::metadata(&path)
        .map(|m| format!("{} bytes", m.len()))
        .unwrap_or_else(|_| "unavailable".into());
    let log = |level, message| {
        let _ = ctx.run.event_tx.try_send(PhaseEvent::Log {
            phase: "postprocess_havok_assets",
            level,
            message,
        });
    };
    log(
        LogLevel::Warn,
        format!(
            "Damaged FO76 HKX: {} (size: {size}; minimum header: 64 bytes). Attempting archive recovery once. Original error: {error}",
            path.display()
        ),
    );
    let dirs: Vec<_> = ctx
        .params
        .get("source_archive_dirs")
        .and_then(|v| v.as_array())
        .into_iter()
        .flatten()
        .filter_map(|v| v.as_str())
        .map(PathBuf::from)
        .collect();
    let recovery = restore(&path, ctx.source_extracted_dir, &dirs, || {
        ctx.check_cancel().map_err(|e| e.to_string())
    });
    ctx.check_cancel()?;
    let (archive, bytes) = recovery.map_err(|reason| PhaseError::Internal(format!(
        "Damaged animation file\nFile: {}\nSize: {size}; a Havok header needs at least 64 bytes.\nAutomatic repair failed: {reason}\nRe-extract the affected FO76 file and overwrite the damaged copy, then run conversion again. If re-extraction fails, verify Fallout 76's installed files first.", path.display()
    )))?;
    log(
        LogLevel::Info,
        format!(
            "Recovered {} from {}: {size} -> {bytes} bytes; validated HKX. Retrying Havok postprocessing once.",
            path.display(),
            archive.display()
        ),
    );
    generate().map_err(|retry| PhaseError::Internal(format!(
        "Animation recovery could not complete conversion\nRepaired file: {}\nArchive: {}\nHavok postprocessing failed again after one retry: {retry}\nVerify Fallout 76's installed files and re-extract the affected source assets before running conversion again.", path.display(), archive.display()
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::phase::{run_phase, DispatchParams, PhaseReport};
    use crate::run::{create_run, drop_run, run_slot, RunConfig, RunParams};
    use crate::translator::Game;
    use bsarchive_native::fo4::{Archive, ArchiveKey, ArchiveOptions, Chunk, File};
    use bsarchive_native::prelude::*;

    const CLIP: &str = "actors/_1stperson/animations/a.hkx";

    struct Fixture {
        _temp: tempfile::TempDir,
        source: PathBuf,
        target: PathBuf,
        output: PathBuf,
        archives: PathBuf,
        valid: Vec<u8>,
    }

    impl Fixture {
        fn new(clips: &[&str]) -> Self {
            let temp = tempfile::tempdir().unwrap();
            let source = temp.path().join("source");
            let target = temp.path().join("target");
            let output = temp.path().join("output");
            let archives = temp.path().join("Data");
            std::fs::create_dir_all(&archives).unwrap();
            let valid = std::fs::read(Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../../../py_creation_lib/native/havok/tests/fixtures/fo76_antiairturret_ragdoll.hkx")).unwrap();
            let target_file = target.join("Meshes/actors/character/behaviors/weaponbehavior.hkx");
            std::fs::create_dir_all(target_file.parent().unwrap()).unwrap();
            std::fs::write(target_file, &valid).unwrap();
            for clip in clips {
                let path = source.join("Meshes").join(clip);
                std::fs::create_dir_all(path.parent().unwrap()).unwrap();
                std::fs::write(path, []).unwrap();
            }
            let animations: std::collections::BTreeMap<_, _> =
                clips.iter().map(|clip| (*clip, *clip)).collect();
            let plan = serde_json::json!({
                "projects": {"actors/_1stperson": {
                    "source": "actors/_1stperson", "destination": "actors/_1stperson", "vanilla": true
                }},
                "routes": [{
                    "source": "actors/_1stperson/behaviors/furniture_audit.hkx",
                    "destination": "actors/_1stperson/behaviors/furniture_audit.hkx",
                    "project": "actors/_1stperson", "dependencies": {}, "animations": animations,
                    "draw_event": null
                }]
            });
            let plan_path = output.join(super::super::PLAN_PATH);
            std::fs::create_dir_all(plan_path.parent().unwrap()).unwrap();
            std::fs::write(plan_path, serde_json::to_vec(&plan).unwrap()).unwrap();
            Self {
                _temp: temp,
                source,
                target,
                output,
                archives,
                valid,
            }
        }

        fn archive(&self, name: &str, clips: &[&str], bytes: &[u8]) -> PathBuf {
            let archive: Archive = clips
                .iter()
                .map(|clip| {
                    let member = format!("meshes/{}", clip.replace('/', "\\").to_uppercase());
                    let key: ArchiveKey = member.as_bytes().into();
                    let file: File = [Chunk::from_decompressed(bytes)].into_iter().collect();
                    (key, file)
                })
                .collect();
            let path = self.archives.join(name);
            archive
                .write(
                    &mut std::fs::File::create(&path).unwrap(),
                    &ArchiveOptions::builder().strings(true).build(),
                )
                .unwrap();
            path
        }

        fn run(&self) -> (Result<PhaseReport, PhaseError>, Vec<PhaseEvent>) {
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
            let result = run_phase(
                id,
                "postprocess_havok_assets",
                DispatchParams {
                    mod_path: self.output.clone(),
                    source_extracted_dir: self.source.clone(),
                    target_extracted_dir: Some(self.target.clone()),
                    target_data_dir: None,
                    params: serde_json::json!({"source_archive_dirs": [self.archives]}),
                },
            );
            let events = run_slot(id).unwrap().events.try_iter().collect();
            drop_run(id).unwrap();
            (result, events)
        }
    }

    #[test]
    fn every_source_reader_reports_short_input_below_a_packfile_header() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("clip.hkx");
        for size in [0usize, 1, 4, 8, 63] {
            std::fs::write(&path, vec![0; size]).unwrap();
            let guarded = super::super::source_bytes(&path).unwrap_err();
            assert!(guarded.ends_with(SHORT_INPUT), "{size}: {guarded}");
            // read() reaches the packfile decoder, which used to report its own
            // truncation error and so never qualified for archive recovery.
            let via_read = super::super::read(&path).unwrap_err();
            assert!(via_read.ends_with(SHORT_INPUT), "{size}: {via_read}");
        }
        std::fs::write(&path, vec![0; MIN_HKX_LEN]).unwrap();
        assert!(super::super::source_bytes(&path).is_ok());
    }

    #[test]
    fn phase_recovers_short_inputs_from_newest_archive_and_finishes() {
        let fixture = Fixture::new(&[CLIP]);
        fixture.archive("SeventySix - Animations.ba2", &[CLIP], b"bad");
        fixture.archive("SeventySix - 2UpdateMain.ba2", &[CLIP], b"bad");
        fixture.archive("SeventySix - 11UpdateMain.ba2", &[CLIP], &fixture.valid);
        let clip = fixture.source.join("Meshes").join(CLIP);
        for size in [0usize, 1, 4, 63] {
            std::fs::write(&clip, vec![0; size]).unwrap();
            let original = super::super::assets::build_profiled(
                &fixture.output,
                &fixture.source,
                &fixture.target,
            )
            .err()
            .unwrap();
            assert!(original.ends_with(SHORT_INPUT), "{original}");
            let (report, events) = fixture.run();
            assert_eq!(report.unwrap().assets_written, 1);
            assert_eq!(std::fs::read(&clip).unwrap(), fixture.valid);
            HkxFile::read(
                &std::fs::read(
                    fixture
                        .output
                        .join("data/Meshes/actors/B21_FO76/Source/fo4rig")
                        .join(CLIP),
                )
                .unwrap(),
            )
            .unwrap();
            assert!(events.iter().any(|event| matches!(event, PhaseEvent::Log { level: LogLevel::Warn, message, .. }
                if message.contains(&format!("size: {size} bytes")) && message.contains("Attempting archive recovery once"))));
            assert!(events.iter().any(|event| matches!(event, PhaseEvent::Log { level: LogLevel::Info, message, .. }
                if message.contains("11UpdateMain.ba2") && message.contains("Retrying Havok postprocessing once"))));
        }
    }

    #[test]
    fn phase_retries_once_and_leaves_a_second_damaged_file_untouched() {
        let second = "actors/_1stperson/animations/z.hkx";
        let fixture = Fixture::new(&[CLIP, second]);
        fixture.archive(
            "SeventySix - 11UpdateMain.ba2",
            &[CLIP, second],
            &fixture.valid,
        );
        let (result, events) = fixture.run();
        let error = result.unwrap_err().to_string();
        assert!(error.contains("failed again after one retry"), "{error}");
        assert!(error.contains("z.hkx"), "{error}");
        assert_eq!(
            std::fs::read(fixture.source.join("Meshes").join(CLIP)).unwrap(),
            fixture.valid
        );
        assert_eq!(
            std::fs::metadata(fixture.source.join("Meshes").join(second))
                .unwrap()
                .len(),
            0
        );
        assert_eq!(
            events
                .iter()
                .filter(|event| matches!(event, PhaseEvent::Log { message, .. }
            if message.contains("Attempting archive recovery once")))
                .count(),
            1
        );
    }

    #[test]
    fn broken_archive_and_cancellation_keep_source_untouched() {
        {
            let fixture = Fixture::new(&[CLIP]);
            fixture.archive("SeventySix - Animations.ba2", &[CLIP], &fixture.valid);
            fixture.archive("SeventySix - 11UpdateMain.ba2", &[CLIP], b"bad");
            let error = fixture.run().0.unwrap_err().to_string();
            assert!(error.contains("Damaged animation file"), "{error}");
            assert!(
                error.contains("11UpdateMain.ba2 is also invalid"),
                "{error}"
            );
            assert!(error.contains("Size: 0 bytes"), "{error}");
            assert_eq!(
                std::fs::metadata(fixture.source.join("Meshes").join(CLIP))
                    .unwrap()
                    .len(),
                0
            );
        }
        {
            let fixture = Fixture::new(&[CLIP]);
            fixture.archive("SeventySix - Animations.ba2", &[CLIP], &fixture.valid);
            let clip = fixture.source.join("Meshes").join(CLIP);
            let mut checks = 0;
            let error = restore(&clip, &fixture.source, &[fixture.archives.clone()], || {
                checks += 1;
                if checks == 2 {
                    Err("cancelled".into())
                } else {
                    Ok(())
                }
            })
            .unwrap_err();
            assert_eq!(error, "cancelled");
            assert_eq!(std::fs::metadata(clip).unwrap().len(), 0);
        }
    }

    #[test]
    fn missing_owner_and_outside_source_errors_never_modify_sources() {
        {
            let fixture = Fixture::new(&[CLIP]);
            fixture.archive(
                "SeventySix - Animations.ba2",
                &["unrelated.hkx"],
                &fixture.valid,
            );
            let error = fixture.run().0.unwrap_err().to_string();
            assert!(
                error.contains("No installed FO76 archive contains this file"),
                "{error}"
            );
            assert!(
                error.contains("Re-extract the affected FO76 file"),
                "{error}"
            );
        }
        {
            let temp = tempfile::tempdir().unwrap();
            let source = temp.path().join("source");
            std::fs::create_dir(&source).unwrap();
            let external = temp.path().join("outside.hkx");
            std::fs::write(&external, []).unwrap();
            let error = restore(&external, &source, &[], || Ok(())).unwrap_err();
            assert!(error.contains("outside the selected"));
            assert_eq!(std::fs::metadata(external).unwrap().len(), 0);
        }
    }

    #[test]
    fn updates_follow_base_archives_and_sort_numerically() {
        let mut names = [
            "SeventySix - Animations.ba2",
            "SeventySix - 11UpdateMain.ba2",
            "SeventySix - 02UpdateMain.ba2",
        ];
        names.sort_by_key(|name| archive_order(Path::new(name)));
        assert_eq!(
            names,
            [
                "SeventySix - Animations.ba2",
                "SeventySix - 02UpdateMain.ba2",
                "SeventySix - 11UpdateMain.ba2"
            ]
        );
    }
}
