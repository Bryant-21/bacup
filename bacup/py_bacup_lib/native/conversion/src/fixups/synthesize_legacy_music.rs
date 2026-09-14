use rustc_hash::FxHashMap;

use crate::fixups::{Fixup, FixupConfig, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::ids::{FormKey, SigCode, SubrecordSig};
use crate::record::{FieldEntry, FieldValue, Record};
use crate::session::PluginSession;

const SINGLE_TRACK: u64 = 1_859_641_416;
const SILENT_TRACK: u64 = 2_712_257_749;

pub struct SynthesizeLegacyMusicFixup;

impl Fixup for SynthesizeLegacyMusicFixup {
    fn name(&self) -> &'static str {
        "synthesize_legacy_music"
    }

    fn uses_session(&self) -> bool {
        true
    }

    fn applies_to_session(&self, session: &PluginSession, _config: &FixupConfig) -> bool {
        let source_game = session
            .source_slot_opt()
            .and_then(|slot| slot.parsed.game.as_deref());
        let target_game = session.target_slot().parsed.game.as_deref();
        matches!(source_game, Some("fnv" | "fo3")) && target_game == Some("fo4")
    }

    fn run_with_session(
        &self,
        session: &mut PluginSession,
        mapper: &mut FormKeyMapper,
        config: &FixupConfig,
    ) -> Result<FixupReport, FixupError> {
        let source_schema = session
            .source_schema()
            .map_err(|error| FixupError::HandleError(error.to_string()))?;
        let target_schema = session
            .schema()
            .map_err(|error| FixupError::HandleError(error.to_string()))?;
        let musc_sig = sig("MUSC")?;
        let fnam_sig = subrecord("FNAM")?;

        let mut music_types = Vec::new();
        let mut tracks_by_path = FxHashMap::default();
        let mut added_tracks = Vec::new();
        let mut silent_track = None;
        let mut report = FixupReport::empty();

        let mut source_form_keys = session
            .source_form_keys_of_sig(musc_sig, mapper.interner)
            .map_err(|error| FixupError::HandleError(error.to_string()))?;
        source_form_keys.sort_unstable_by_key(|form_key| form_key.local);

        for source_fk in source_form_keys {
            let Some(target_fk) = mapper.lookup(source_fk) else {
                continue;
            };
            let source = session
                .source_record_decoded(&source_fk, source_schema.as_ref(), mapper.interner)
                .map_err(|error| FixupError::HandleError(error.to_string()))?;
            let legacy_path = string_field(&source, fnam_sig, mapper).unwrap_or_default();
            let matching_tracks = matching_tracks(&legacy_path, &config.legacy_music_tracks);
            let mut track_form_keys = Vec::with_capacity(matching_tracks.len().max(1));

            for track in matching_tracks {
                let key = track.track_path.to_ascii_lowercase();
                let track_fk = if let Some(existing) = tracks_by_path.get(&key).copied() {
                    existing
                } else {
                    let track_fk = mapper.allocate_generated();
                    let editor_id = generated_editor_id(
                        source.eid.and_then(|eid| mapper.interner.resolve(eid)),
                        added_tracks.len() + 1,
                    );
                    added_tracks.push(build_single_track(
                        track_fk,
                        &editor_id,
                        &track.track_path,
                        mapper,
                    )?);
                    tracks_by_path.insert(key, track_fk);
                    track_fk
                };
                track_form_keys.push(track_fk);
            }

            if track_form_keys.is_empty() {
                let track_fk = if let Some(existing) = silent_track {
                    existing
                } else {
                    let track_fk = mapper.allocate_generated();
                    added_tracks.push(build_silent_track(track_fk, mapper)?);
                    silent_track = Some(track_fk);
                    track_fk
                };
                track_form_keys.push(track_fk);
                if !source
                    .eid
                    .and_then(|eid| mapper.interner.resolve(eid))
                    .is_some_and(|editor_id| editor_id.to_ascii_lowercase().contains("nomusic"))
                {
                    report.warnings.push(mapper.interner.intern(&format!(
                        "synthesize_legacy_music:no_tracks:{:06X}:{}",
                        source_fk.local, legacy_path
                    )));
                }
            }

            let mut target = session
                .record_decoded(&target_fk, target_schema.as_ref(), mapper.interner)
                .map_err(|error| FixupError::HandleError(error.to_string()))?;
            populate_music_type(&mut target, &source, track_form_keys, mapper)?;
            music_types.push(target);
        }

        report.records_added = session
            .add_records(added_tracks, target_schema.as_ref(), mapper.interner)
            .map_err(|error| FixupError::HandleError(error.to_string()))?
            .try_into()
            .unwrap_or(u32::MAX);
        report.records_changed = session
            .replace_records_contents(music_types, target_schema.as_ref(), mapper.interner)
            .map_err(|error| FixupError::HandleError(error.to_string()))?
            .try_into()
            .unwrap_or(u32::MAX);
        Ok(report)
    }
}

fn matching_tracks<'a>(
    legacy_path: &str,
    tracks: &'a [crate::run::LegacyMusicTrackRow],
) -> Vec<&'a crate::run::LegacyMusicTrackRow> {
    let wanted = normalize_legacy_path(legacy_path);
    if wanted.is_empty() {
        return Vec::new();
    }
    let names_file = has_extension(&wanted);
    tracks
        .iter()
        .filter(|track| {
            let relative = normalize_legacy_path(&track.legacy_relative_path);
            if names_file {
                paths_name_same_track(&relative, &wanted)
            } else {
                relative
                    .strip_prefix(&wanted)
                    .is_some_and(|suffix| suffix.starts_with('/'))
            }
        })
        .collect()
}

fn paths_name_same_track(left: &str, right: &str) -> bool {
    without_audio_extension(left) == without_audio_extension(right)
}

fn without_audio_extension(path: &str) -> &str {
    path.rsplit_once('.')
        .map_or(path, |(stem, extension)| match extension {
            "mp3" | "ogg" | "wav" | "xwm" => stem,
            _ => path,
        })
}

fn has_extension(path: &str) -> bool {
    path.rsplit_once('/')
        .map_or(path, |(_, name)| name)
        .rsplit_once('.')
        .is_some_and(|(_, extension)| !extension.is_empty())
}

fn normalize_legacy_path(path: &str) -> String {
    let normalized = path.trim().replace('\\', "/");
    let trimmed = normalized.trim_matches('/');
    let lower = trimmed.to_ascii_lowercase();
    for prefix in ["data/music/", "music/"] {
        if let Some(relative) = lower.strip_prefix(prefix) {
            return relative.trim_matches('/').to_string();
        }
    }
    lower
}

fn populate_music_type(
    target: &mut Record,
    source: &Record,
    track_form_keys: Vec<FormKey>,
    mapper: &FormKeyMapper,
) -> Result<(), FixupError> {
    let editor_id = target.eid.or(source.eid);
    target.eid = editor_id;
    target.fields.clear();
    if let Some(editor_id) = editor_id {
        target
            .fields
            .push(field("EDID", FieldValue::String(editor_id))?);
    }
    target.fields.push(field(
        "FNAM",
        FieldValue::Uint(if track_form_keys.len() == 1 { 1 } else { 4 }),
    )?);
    target.fields.push(field(
        "PNAM",
        FieldValue::Struct(vec![
            (mapper.interner.intern("priority"), FieldValue::Uint(90)),
            (mapper.interner.intern("ducking_db"), FieldValue::Uint(0)),
        ]),
    )?);
    target.fields.push(field("WNAM", FieldValue::Float(12.0))?);
    target.fields.push(field(
        "TNAM",
        FieldValue::List(
            track_form_keys
                .into_iter()
                .map(FieldValue::FormKey)
                .collect(),
        ),
    )?);
    Ok(())
}

fn build_single_track(
    form_key: FormKey,
    editor_id: &str,
    track_path: &str,
    mapper: &FormKeyMapper,
) -> Result<Record, FixupError> {
    let mut record = Record::new(sig("MUST")?, form_key);
    let eid = mapper.interner.intern(editor_id);
    record.eid = Some(eid);
    record.fields.push(field("EDID", FieldValue::String(eid))?);
    record
        .fields
        .push(field("CNAM", FieldValue::Uint(SINGLE_TRACK))?);
    record.fields.push(field(
        "ANAM",
        FieldValue::String(mapper.interner.intern(track_path)),
    )?);
    Ok(record)
}

fn build_silent_track(form_key: FormKey, mapper: &FormKeyMapper) -> Result<Record, FixupError> {
    let mut record = Record::new(sig("MUST")?, form_key);
    let eid = mapper.interner.intern("LegacyMusicMissingTrack");
    record.eid = Some(eid);
    record.fields.push(field("EDID", FieldValue::String(eid))?);
    record
        .fields
        .push(field("CNAM", FieldValue::Uint(SILENT_TRACK))?);
    Ok(record)
}

fn generated_editor_id(source_editor_id: Option<&str>, index: usize) -> String {
    let base: String = source_editor_id
        .unwrap_or("LegacyMusic")
        .chars()
        .filter(|character| character.is_ascii_alphanumeric() || *character == '_')
        .collect();
    format!("{base}_Track_{index:03}")
}

fn string_field(record: &Record, sig: SubrecordSig, mapper: &FormKeyMapper) -> Option<String> {
    record
        .fields
        .iter()
        .find(|entry| entry.sig == sig)
        .and_then(|entry| match entry.value {
            FieldValue::String(value) => mapper.interner.resolve(value).map(str::to_owned),
            _ => None,
        })
}

pub(crate) fn field(signature: &str, value: FieldValue) -> Result<FieldEntry, FixupError> {
    Ok(FieldEntry {
        sig: subrecord(signature)?,
        value,
    })
}

pub(crate) fn sig(signature: &str) -> Result<SigCode, FixupError> {
    SigCode::from_str(signature).map_err(FixupError::SchemaError)
}

pub(crate) fn subrecord(signature: &str) -> Result<SubrecordSig, FixupError> {
    SubrecordSig::from_str(signature).map_err(FixupError::SchemaError)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formkey_mapper::{MapperOptions, MapperState};
    use crate::record::Record;
    use crate::session::open_session;
    use crate::sym::StringInterner;
    use esp_authoring_core::plugin_runtime::plugin_handle_new_native;

    #[test]
    fn directory_paths_match_descendant_tracks_case_insensitively() {
        let rows = vec![crate::run::LegacyMusicTrackRow {
            legacy_relative_path: "Explore/Explore_01.mp3".into(),
            source_path: "C:/FNV/Music/Explore/Explore_01.mp3".into(),
            asset_path: "Music/Port/FNV/Explore/Explore_01.xwm".into(),
            track_path: "Data\\Music\\Port\\FNV\\Explore\\Explore_01.xwm".into(),
        }];
        assert_eq!(matching_tracks("EXPLORE\\", &rows).len(), 1);
        assert_eq!(matching_tracks("Music/Explore", &rows).len(), 1);
        assert_eq!(matching_tracks("Battle", &rows).len(), 0);
    }

    #[test]
    fn file_paths_match_only_the_named_track() {
        let rows = vec![crate::run::LegacyMusicTrackRow {
            legacy_relative_path: "SCR/DeathStinger.ogg".into(),
            source_path: "C:/FNV/Music/SCR/DeathStinger.ogg".into(),
            asset_path: "Music/Port/FNV/SCR/DeathStinger.xwm".into(),
            track_path: "Data\\Music\\Port\\FNV\\SCR\\DeathStinger.xwm".into(),
        }];
        assert_eq!(matching_tracks("SCR/DeathStinger.mp3", &rows).len(), 1);
        assert_eq!(matching_tracks("SCR/DeathStinger.wav", &rows).len(), 1);
        assert_eq!(matching_tracks("SCR/Other.mp3", &rows).len(), 0);
    }

    #[test]
    fn creates_fo4_tracks_and_populates_the_converted_music_type() {
        let interner = StringInterner::new();
        let source_name = "FalloutNV.esm";
        let target_name = "FalloutNV.esm";
        let source_handle = plugin_handle_new_native(source_name, Some("fnv")).unwrap();
        let target_handle = plugin_handle_new_native(target_name, Some("fo4")).unwrap();
        let source_fk = FormKey {
            local: 0x90908,
            plugin: interner.intern(source_name),
        };
        let target_fk = FormKey {
            local: 0x90908,
            plugin: interner.intern(target_name),
        };
        let source_no_music_fk = FormKey {
            local: 0x8A6D4,
            plugin: interner.intern(source_name),
        };
        let target_no_music_fk = FormKey {
            local: 0x8A6D4,
            plugin: interner.intern(target_name),
        };
        let editor_id = interner.intern("DefaultExplore");
        let no_music_editor_id = interner.intern("1NoMusic");

        let mut source_record = Record::new(SigCode::from_str("MUSC").unwrap(), source_fk);
        source_record.eid = Some(editor_id);
        source_record
            .fields
            .push(field("EDID", FieldValue::String(editor_id)).unwrap());
        source_record
            .fields
            .push(field("FNAM", FieldValue::String(interner.intern("Explore\\"))).unwrap());
        let mut target_record = Record::new(SigCode::from_str("MUSC").unwrap(), target_fk);
        target_record.eid = Some(editor_id);
        target_record
            .fields
            .push(field("EDID", FieldValue::String(editor_id)).unwrap());

        let mut source_no_music =
            Record::new(SigCode::from_str("MUSC").unwrap(), source_no_music_fk);
        source_no_music.eid = Some(no_music_editor_id);
        source_no_music
            .fields
            .push(field("EDID", FieldValue::String(no_music_editor_id)).unwrap());
        let mut target_no_music =
            Record::new(SigCode::from_str("MUSC").unwrap(), target_no_music_fk);
        target_no_music.eid = Some(no_music_editor_id);
        target_no_music
            .fields
            .push(field("EDID", FieldValue::String(no_music_editor_id)).unwrap());

        for (handle, records) in [
            (source_handle, vec![source_record, source_no_music]),
            (target_handle, vec![target_record, target_no_music]),
        ] {
            let mut session = open_session(handle, None).unwrap();
            let schema = session.schema().unwrap();
            session
                .add_records(records, schema.as_ref(), &interner)
                .unwrap();
        }

        let mut mapper_state = MapperState::new(
            std::iter::empty(),
            MapperOptions {
                output_plugin_name: target_name.into(),
                generated_object_id_floor: 0xA0000,
                ..Default::default()
            },
        );
        let mut mapper = FormKeyMapper::from_state(&mut mapper_state, &interner);
        mapper.add_mapping(source_fk, target_fk);
        mapper.add_mapping(source_no_music_fk, target_no_music_fk);
        let config = FixupConfig {
            legacy_music_tracks: vec![crate::run::LegacyMusicTrackRow {
                legacy_relative_path: "Explore/Explore_01.mp3".into(),
                source_path: "C:/FNV/Music/Explore/Explore_01.mp3".into(),
                asset_path: "Music/FalloutNV/FNV/Explore/Explore_01.xwm".into(),
                track_path: "Data\\Music\\FalloutNV\\FNV\\Explore\\Explore_01.xwm".into(),
            }],
            ..Default::default()
        };
        let mut session = open_session(target_handle, Some(source_handle)).unwrap();
        let report = SynthesizeLegacyMusicFixup
            .run_with_session(&mut session, &mut mapper, &config)
            .unwrap();
        assert_eq!(report.records_added, 2);
        assert_eq!(report.records_changed, 2);
        assert!(report.warnings.is_empty());

        let schema = session.schema().unwrap();
        let music_type = session
            .record_decoded(&target_fk, schema.as_ref(), &interner)
            .unwrap();
        let tracks = music_type
            .fields
            .iter()
            .find(|entry| entry.sig.as_str() == "TNAM")
            .map(|entry| &entry.value);
        let Some(FieldValue::List(track_values)) = tracks else {
            panic!("converted MUSC has no TNAM track list");
        };
        let [FieldValue::FormKey(track_fk)] = track_values.as_slice() else {
            panic!("converted MUSC did not reference exactly one MUST");
        };
        let track = session
            .record_decoded(track_fk, schema.as_ref(), &interner)
            .unwrap();
        let track_path = track
            .fields
            .iter()
            .find(|entry| entry.sig.as_str() == "ANAM")
            .and_then(|entry| match entry.value {
                FieldValue::String(path) => interner.resolve(path),
                _ => None,
            });
        assert_eq!(
            track_path,
            Some("Data\\Music\\FalloutNV\\FNV\\Explore\\Explore_01.xwm")
        );
        let no_music = session
            .record_decoded(&target_no_music_fk, schema.as_ref(), &interner)
            .unwrap();
        assert!(
            no_music
                .fields
                .iter()
                .any(|entry| entry.sig.as_str() == "TNAM"),
            "intentional no-music records still need a valid silent MUST list"
        );
    }
}
