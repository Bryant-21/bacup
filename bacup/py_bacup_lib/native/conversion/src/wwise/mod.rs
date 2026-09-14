//! Wwise encode + bank-build harness for the `fo4:starfield` audio leg.
//!
//! Follows the recipe in `bacup/docs/starfield_target/R6-wwise-toolchain.md`:
//! copy Bethesda's shipped Wwise project into a scratch copy, decode/conform
//! each input, hand-emit the three `.wwu` XML documents (`wwu` submodule), run
//! one batched `generate-soundbank` and one `CopyStreamedFiles` call, then
//! stage the bank/media files under `sound/soundbanks/` keyed by FNV-1 32-bit
//! IDs. CI has no Wwise install, so tests run this against stub scripts.
//!
//! Pure `std::process` + filesystem work with no Python, so it is safe to call
//! from any phase context.

pub mod encode;
pub mod wwu;

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

/// Concrete tool locations + command templates for one `build_bank` run.
pub struct WwiseTools {
    /// `.xwm` -> `.wav` decode. `xwmaencode.exe` (ships with the FO4 CK) or
    /// an `ffmpeg` fallback; `{in}`/`{out}` placeholders.
    pub xwm_decode_cmd: Vec<String>,
    /// PCM conform to 48kHz s16le into `Originals\SFX\`; `{in}`/`{out}`.
    pub ffmpeg_cmd: Vec<String>,
    /// `WwiseConsole.exe`. Wwise 2021.1.x ONLY — bank binary version 140;
    /// 2022.1+ would migrate the project and the game's 2021.1 sound engine
    /// rejects the resulting bank version.
    pub wwise_console: PathBuf,
    /// `CopyStreamedFiles.exe` — REQUIRED. Without it every streamed sound
    /// is silent (the flat `<mediaId>.wem` files are never produced).
    pub copy_streamed_files: PathBuf,
    /// Bethesda's shipped Wwise project (`Tools\wwise\Starfield\`). Always
    /// built inside a COPY — a scratch project's bus/effect IDs desync from
    /// the game's already-loaded Init bank.
    pub wwise_project_src: PathBuf,
}

/// One synthesized Wwise event, ready for WWED authoring.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WwiseEvent {
    pub event_name: String,
    /// WWED stores this as two little-endian u64s (hi‖lo), not a
    /// Microsoft-ordered GUID. Parsed back from the generated bank JSON's
    /// `IncludedEvents[].GUID` rather than trusted from authoring time.
    pub event_guid: [u8; 16],
    pub source_rel_path: String,
    pub media_id: u32,
}

#[derive(Debug)]
pub struct WwiseBankOut {
    /// `(rel path under Data/, bytes)` — `sound/soundbanks/<bankId>.bnk`,
    /// `.json`, and one flat `sound/soundbanks/<mediaId>.wem` per event.
    pub bank_files: Vec<(String, Vec<u8>)>,
    pub events: Vec<WwiseEvent>,
}

/// Transcode + bank-build by shelling out to the Wwise toolchain. Missing
/// tools return `Err` naming the tool; this never panics.
pub fn build_bank(
    tools: &WwiseTools,
    work_dir: &Path,
    inputs: &[(String, PathBuf)],
    bank_name: &str,
) -> Result<WwiseBankOut, String> {
    if !tools.wwise_project_src.is_dir() {
        return Err(format!(
            "missing tool: wwise_project_src ({})",
            tools.wwise_project_src.display()
        ));
    }

    // Step 1 — stage the project (never edit Bethesda's copy in place).
    let proj_dir = work_dir.join("proj");
    copy_project_dir(&tools.wwise_project_src, &proj_dir)?;

    let wav_dir = work_dir.join("wav");
    let originals_dir = work_dir.join("Originals").join("SFX").join("FO4SF");
    fs::create_dir_all(&wav_dir).map_err(|e| e.to_string())?;
    fs::create_dir_all(&originals_dir).map_err(|e| e.to_string())?;

    let mut drafts = Vec::with_capacity(inputs.len());
    let mut media_id_to_source: HashMap<u32, String> = HashMap::new();
    let mut media_ids_seen: HashSet<u32> = HashSet::new();

    for (source_rel_path, audio_path) in inputs {
        let event_name = encode::sanitize_event_name(source_rel_path);
        let suffix = event_name
            .strip_prefix("FO4SF_")
            .unwrap_or(&event_name)
            .to_string();

        // Step 2 — decode .xwm -> .wav (xwmaencode preferred; ffmpeg is the
        // caller's fallback choice baked into xwm_decode_cmd). Sources that
        // are already .wav skip straight to the conform step.
        let decoded_wav = if is_xwm(audio_path) {
            let out = wav_dir.join(format!("{suffix}.wav"));
            let argv = encode::substitute_template(&tools.xwm_decode_cmd, audio_path, &out);
            encode::run_command(&argv)?;
            out
        } else {
            audio_path.clone()
        };

        // Step 3 — normalise into the project's Originals\SFX\ tree.
        let conformed = originals_dir.join(format!("{suffix}.wav"));
        let argv = encode::substitute_template(&tools.ffmpeg_cmd, &decoded_wav, &conformed);
        encode::run_command(&argv)?;

        let media_id = encode::fnv1_32(&format!("fo4sf_media_{source_rel_path}"));
        if !media_ids_seen.insert(media_id) {
            let prior = media_id_to_source
                .get(&media_id)
                .cloned()
                .unwrap_or_default();
            return Err(format!(
                "media ID collision: '{source_rel_path}' and '{prior}' both hash to {media_id}"
            ));
        }
        media_id_to_source.insert(media_id, source_rel_path.clone());

        drafts.push(wwu::EventDraft {
            event_name: event_name.clone(),
            sound_guid: encode::wwise_object_guid(&format!("sound_{event_name}")),
            source_guid: encode::wwise_object_guid(&format!("source_{event_name}")),
            event_guid: encode::wwise_object_guid(&format!("event_{event_name}")),
            action_guid: encode::wwise_object_guid(&format!("action_{event_name}")),
            media_id,
            audio_file_rel: format!("FO4SF\\{suffix}.wav"),
        });
    }

    // Steps 4-6 — emit the three .wwu documents.
    let amh_wu_guid = encode::wwise_object_guid(&format!("amh_wu_{bank_name}"));
    let amh_mixer_guid = encode::wwise_object_guid(&format!("amh_mixer_{bank_name}"));
    let events_wu_guid = encode::wwise_object_guid(&format!("events_wu_{bank_name}"));
    let bank_guid = encode::wwise_object_guid(&format!("bank_{bank_name}"));

    let amh_path = proj_dir.join("Actor-Mixer Hierarchy").join("FO4SF.wwu");
    fs::create_dir_all(amh_path.parent().unwrap()).map_err(|e| e.to_string())?;
    fs::write(
        &amh_path,
        wwu::actor_mixer_wwu(&amh_wu_guid, &amh_mixer_guid, &drafts),
    )
    .map_err(|e| e.to_string())?;

    let events_path = proj_dir.join("Events").join("FO4SF.wwu");
    fs::create_dir_all(events_path.parent().unwrap()).map_err(|e| e.to_string())?;
    fs::write(
        &events_path,
        wwu::events_wwu(&events_wu_guid, &amh_wu_guid, &drafts),
    )
    .map_err(|e| e.to_string())?;

    let soundbanks_path = proj_dir.join("SoundBanks").join("Default Work Unit.wwu");
    fs::create_dir_all(soundbanks_path.parent().unwrap()).map_err(|e| e.to_string())?;
    let existing = fs::read_to_string(&soundbanks_path).ok();
    let entry = wwu::soundbank_entry_xml(bank_name, &bank_guid, &amh_wu_guid, &events_wu_guid);
    fs::write(
        &soundbanks_path,
        wwu::append_soundbank_entry(existing.as_deref(), &entry),
    )
    .map_err(|e| e.to_string())?;

    // Step 8 — ONE generate-soundbank call for the whole batch (never per input).
    let banks_dir = work_dir.join("banks");
    fs::create_dir_all(&banks_dir).map_err(|e| e.to_string())?;
    let argv = vec![
        tools.wwise_console.to_string_lossy().to_string(),
        "generate-soundbank".to_string(),
        proj_dir.to_string_lossy().to_string(),
        "--bank".to_string(),
        bank_name.to_string(),
        "--platform".to_string(),
        "Windows".to_string(),
        "--language".to_string(),
        "SFX".to_string(),
        "--soundbank-path".to_string(),
        "Windows".to_string(),
        banks_dir.to_string_lossy().to_string(),
    ];
    encode::run_command(&argv)?;

    // Step 9 — flatten streamed media to <mediaId>.wem.
    let info_json = banks_dir.join("SoundbanksInfo.json");
    let banklist_path = work_dir.join("banklist.txt");
    fs::write(&banklist_path, bank_name).map_err(|e| e.to_string())?;
    let argv = vec![
        tools.copy_streamed_files.to_string_lossy().to_string(),
        "-info".to_string(),
        info_json.to_string_lossy().to_string(),
        "-outputpath".to_string(),
        banks_dir.to_string_lossy().to_string(),
        "-banks".to_string(),
        banklist_path.to_string_lossy().to_string(),
        "-languages".to_string(),
        "SFX".to_string(),
    ];
    encode::run_command(&argv)?;

    // Step 10 — stage bank + metadata under the FNV1_32(bank_name) stem.
    let bank_id = encode::fnv1_32(bank_name);
    let bnk_path = banks_dir.join(format!("{bank_name}.bnk"));
    let json_path = banks_dir.join(format!("{bank_name}.json"));
    let bnk_bytes = fs::read(&bnk_path)
        .map_err(|e| format!("reading generated bank '{}': {e}", bnk_path.display()))?;
    let json_bytes = fs::read(&json_path).map_err(|e| {
        format!(
            "reading generated bank metadata '{}': {e}",
            json_path.display()
        )
    })?;

    let mut bank_files = vec![
        (format!("sound/soundbanks/{bank_id}.bnk"), bnk_bytes),
        (
            format!("sound/soundbanks/{bank_id}.json"),
            json_bytes.clone(),
        ),
    ];

    // Step 11 — parse events (with GUIDs) back from the generated metadata.
    let events = parse_events_from_bank_json(&json_bytes, &drafts, &media_id_to_source)?;
    for ev in &events {
        let wem_path = banks_dir.join(format!("{}.wem", ev.media_id));
        let wem_bytes = fs::read(&wem_path)
            .map_err(|e| format!("reading streamed media '{}': {e}", wem_path.display()))?;
        bank_files.push((format!("sound/soundbanks/{}.wem", ev.media_id), wem_bytes));
    }

    Ok(WwiseBankOut { bank_files, events })
}

fn is_xwm(path: &Path) -> bool {
    path.extension()
        .map(|e| e.to_string_lossy().eq_ignore_ascii_case("xwm"))
        .unwrap_or(false)
}

/// Recursive project copy, skipping any `GeneratedSoundBanks` directory
/// (stale prior bank output — never carried into a fresh scratch copy).
fn copy_project_dir(src: &Path, dst: &Path) -> Result<(), String> {
    fs::create_dir_all(dst).map_err(|e| e.to_string())?;
    for entry in fs::read_dir(src).map_err(|e| format!("reading '{}': {e}", src.display()))? {
        let entry = entry.map_err(|e| e.to_string())?;
        let file_type = entry.file_type().map_err(|e| e.to_string())?;
        let name = entry.file_name();
        if file_type.is_dir() {
            if name
                .to_string_lossy()
                .eq_ignore_ascii_case("GeneratedSoundBanks")
            {
                continue;
            }
            copy_project_dir(&entry.path(), &dst.join(&name))?;
        } else {
            fs::copy(entry.path(), dst.join(&name)).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

fn parse_events_from_bank_json(
    json_bytes: &[u8],
    drafts: &[wwu::EventDraft],
    media_id_to_source: &HashMap<u32, String>,
) -> Result<Vec<WwiseEvent>, String> {
    let root: Value =
        serde_json::from_slice(json_bytes).map_err(|e| format!("parsing bank metadata: {e}"))?;
    let included = root
        .pointer("/SoundBanksInfo/SoundBanks/0/IncludedEvents")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            "bank metadata missing SoundBanksInfo.SoundBanks[0].IncludedEvents".to_string()
        })?;

    let by_name: HashMap<&str, &wwu::EventDraft> =
        drafts.iter().map(|d| (d.event_name.as_str(), d)).collect();

    let mut events = Vec::with_capacity(drafts.len());
    for item in included {
        let name = item
            .get("Name")
            .and_then(Value::as_str)
            .ok_or_else(|| "IncludedEvents entry missing Name".to_string())?;
        let guid_str = item
            .get("GUID")
            .and_then(Value::as_str)
            .ok_or_else(|| format!("event '{name}' missing GUID in bank metadata"))?;
        let draft = by_name
            .get(name)
            .ok_or_else(|| format!("bank metadata has unexpected event '{name}'"))?;
        let event_guid = encode::guid_string_to_bytes(guid_str)?;
        let source_rel_path = media_id_to_source
            .get(&draft.media_id)
            .cloned()
            .ok_or_else(|| format!("no source path recorded for media id {}", draft.media_id))?;
        events.push(WwiseEvent {
            event_name: name.to_string(),
            event_guid,
            source_rel_path,
            media_id: draft.media_id,
        });
    }
    if events.len() != drafts.len() {
        return Err(format!(
            "bank metadata included {} events, expected {}",
            events.len(),
            drafts.len()
        ));
    }
    Ok(events)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_path(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("src")
            .join("test_fixtures")
            .join("wwise")
            .join(name)
    }

    fn write_file(path: &Path, content: &[u8]) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    }

    /// A minimal stand-in for Bethesda's shipped `Tools\wwise\Starfield\`
    /// project: just enough shape for `build_bank` to copy + append into.
    fn make_fake_project(dir: &Path) {
        write_file(
            &dir.join("Starfield.wproj"),
            br#"<WwiseDocument Type="Project" SchemaVersion="103" WwiseVersion="v2021.1.10"/>"#,
        );
        write_file(
            &dir.join("SoundBanks").join("Default Work Unit.wwu"),
            br#"<?xml version="1.0" encoding="utf-8"?>
<WwiseDocument Type="WorkUnit" ID="{99999999-0000-0000-0000-000000000000}" SchemaVersion="103">
 <AudioObjects>
  <WorkUnit Name="Default Work Unit" ID="{99999999-0000-0000-0000-000000000000}" PersistMode="Standalone">
   <ChildrenList>
   </ChildrenList>
  </WorkUnit>
 </AudioObjects>
</WwiseDocument>
"#,
        );
        write_file(&dir.join("GeneratedSoundBanks").join("stale.bnk"), b"stale");
    }

    fn make_tools(project_src: &Path) -> WwiseTools {
        WwiseTools {
            xwm_decode_cmd: vec![
                fixture_path("xwm_decode_stub.cmd")
                    .to_string_lossy()
                    .to_string(),
                "{in}".to_string(),
                "{out}".to_string(),
            ],
            ffmpeg_cmd: vec![
                fixture_path("ffmpeg_stub.cmd")
                    .to_string_lossy()
                    .to_string(),
                "{in}".to_string(),
                "{out}".to_string(),
            ],
            wwise_console: fixture_path("wwise_console_stub.cmd"),
            copy_streamed_files: fixture_path("copy_streamed_files_stub.cmd"),
            wwise_project_src: project_src.to_path_buf(),
        }
    }

    #[test]
    fn build_bank_end_to_end_over_two_inputs() {
        let root = tempfile::tempdir().unwrap();
        let project_src = root.path().join("project_src");
        make_fake_project(&project_src);

        let inputs_dir = root.path().join("inputs");
        let xwm_in = inputs_dir.join("Track.xwm");
        write_file(&xwm_in, b"fake xwm bytes");
        let wav_in = inputs_dir.join("Beep.wav");
        write_file(&wav_in, b"fake wav bytes");

        let tools = make_tools(&project_src);
        let work_dir = root.path().join("work");
        fs::create_dir_all(&work_dir).unwrap();

        let inputs = vec![
            ("music/Track.xwm".to_string(), xwm_in),
            ("sound/fx/Beep.wav".to_string(), wav_in),
        ];

        let out = build_bank(&tools, &work_dir, &inputs, "Fallout4_SF")
            .expect("build_bank should succeed against stub tools");

        assert_eq!(out.events.len(), 2);
        let names: HashSet<_> = out.events.iter().map(|e| e.event_name.clone()).collect();
        assert!(names.contains("FO4SF_MUSIC_TRACK_XWM"));
        assert!(names.contains("FO4SF_SOUND_FX_BEEP_WAV"));
        for ev in &out.events {
            assert_ne!(
                ev.event_guid, [0u8; 16],
                "event GUID should be parsed, not zero"
            );
        }

        let bank_id = encode::fnv1_32("Fallout4_SF");
        assert_eq!(bank_id, 3768386522);
        let paths: HashSet<_> = out.bank_files.iter().map(|(p, _)| p.clone()).collect();
        assert!(paths.contains(&format!("sound/soundbanks/{bank_id}.bnk")));
        assert!(paths.contains(&format!("sound/soundbanks/{bank_id}.json")));
        for ev in &out.events {
            assert!(paths.contains(&format!("sound/soundbanks/{}.wem", ev.media_id)));
        }

        // GeneratedSoundBanks must never be carried into the scratch copy.
        assert!(!work_dir.join("proj").join("GeneratedSoundBanks").exists());

        // Exactly one generate-soundbank invocation for the whole batch.
        let counter_path = work_dir.join("banks").join("_wwiseconsole_call_count.txt");
        let count = fs::read_to_string(&counter_path).unwrap();
        assert_eq!(count.trim(), "1");
    }

    #[test]
    fn build_bank_reports_missing_copy_streamed_files_by_name() {
        let root = tempfile::tempdir().unwrap();
        let project_src = root.path().join("project_src");
        make_fake_project(&project_src);

        let inputs_dir = root.path().join("inputs");
        let wav_in = inputs_dir.join("Beep.wav");
        write_file(&wav_in, b"fake wav bytes");

        let mut tools = make_tools(&project_src);
        tools.copy_streamed_files = root.path().join("does_not_exist_copystreamedfiles.exe");

        let work_dir = root.path().join("work");
        fs::create_dir_all(&work_dir).unwrap();

        let inputs = vec![("sound/fx/Beep.wav".to_string(), wav_in)];
        let err = build_bank(&tools, &work_dir, &inputs, "Fallout4_SF")
            .expect_err("missing CopyStreamedFiles must fail, not panic");
        assert!(
            err.contains("does_not_exist_copystreamedfiles.exe"),
            "error should name the missing tool: {err}"
        );
    }
}
