use std::collections::{HashMap, HashSet};
use std::fmt;

use crate::ids::{FormKey, SigCode, SubrecordSig};
use crate::record::{FieldEntry, FieldValue, Record};
use crate::sym::{StringInterner, Sym};

#[derive(Clone, Debug)]
pub(crate) struct MusicComponentPlan {
    music_type: Record,
    tracks: Vec<Record>,
    source_record_paths: Vec<String>,
}

#[derive(Clone, Debug)]
pub(crate) enum MusicSupport {
    NotMusic,
    Supported(MusicComponentPlan),
    Unsupported(MusicContractError),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct MusicRecordEdge {
    pub source: FormKey,
    pub target: FormKey,
    pub signature: SigCode,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct MusicTrackEdge {
    pub source_music: FormKey,
    pub target_music: FormKey,
    pub source_track: FormKey,
    pub target_track: FormKey,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct MusicAssetEdge {
    pub source_track: FormKey,
    pub target_track: FormKey,
    pub source_record_path: String,
    pub target_record_path: String,
    pub source_asset_path: String,
    pub target_asset_path: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct MusicClosureReceipt {
    pub record_edges: Vec<MusicRecordEdge>,
    pub track_edges: Vec<MusicTrackEdge>,
    pub assets: Vec<MusicAssetEdge>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum MusicContractError {
    MissingMusicType,
    MultipleMusicTypes,
    UnexpectedRecord(FormKey, SigCode),
    DuplicateRecord(FormKey),
    MissingTrackList(FormKey),
    InvalidTrackList(FormKey),
    DuplicateTrackReference(FormKey),
    MissingTrack(FormKey),
    UnreferencedTrack(FormKey),
    MissingTrackPath(FormKey),
    InvalidTrackPath(FormKey, String),
    DuplicateTrackPath(String),
    MissingTargetMapping(FormKey),
    InvalidOutputPlugin,
    InvalidTargetPlugin(FormKey),
    InvalidTargetLocalId(FormKey),
    DuplicateTarget(FormKey),
    InvalidEditorId(FormKey),
}

impl fmt::Display for MusicContractError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingMusicType => formatter.write_str("music component has no MUSC record"),
            Self::MultipleMusicTypes => {
                formatter.write_str("music component contains more than one MUSC record")
            }
            Self::UnexpectedRecord(form_key, signature) => write!(
                formatter,
                "music component contains unexpected {} record at local FormID {:06X}",
                signature.as_str(),
                form_key.local
            ),
            Self::DuplicateRecord(form_key) => write!(
                formatter,
                "music component contains duplicate local FormID {:06X}",
                form_key.local
            ),
            Self::MissingTrackList(form_key) => write!(
                formatter,
                "MUSC {:06X} has no TNAM track list",
                form_key.local
            ),
            Self::InvalidTrackList(form_key) => write!(
                formatter,
                "MUSC {:06X} has an invalid TNAM track list",
                form_key.local
            ),
            Self::DuplicateTrackReference(form_key) => {
                write!(formatter, "MUSC repeats MUST {:06X}", form_key.local)
            }
            Self::MissingTrack(form_key) => write!(
                formatter,
                "MUSC references missing MUST {:06X}",
                form_key.local
            ),
            Self::UnreferencedTrack(form_key) => write!(
                formatter,
                "music component contains unreferenced MUST {:06X}",
                form_key.local
            ),
            Self::MissingTrackPath(form_key) => write!(
                formatter,
                "MUST {:06X} has no single ANAM path",
                form_key.local
            ),
            Self::InvalidTrackPath(form_key, path) => write!(
                formatter,
                "MUST {:06X} has unsafe or unsupported ANAM path {path:?}",
                form_key.local
            ),
            Self::DuplicateTrackPath(path) => {
                write!(formatter, "music component repeats track path {path:?}")
            }
            Self::MissingTargetMapping(form_key) => write!(
                formatter,
                "music component has no target mapping for {:06X}",
                form_key.local
            ),
            Self::InvalidOutputPlugin => {
                formatter.write_str("music output plugin has no usable namespace")
            }
            Self::InvalidTargetPlugin(form_key) => write!(
                formatter,
                "music target {:06X} is not owned by the output plugin",
                form_key.local
            ),
            Self::InvalidTargetLocalId(form_key) => write!(
                formatter,
                "music target local FormID is outside 000001..FFFFFF: {:08X}",
                form_key.local
            ),
            Self::DuplicateTarget(form_key) => write!(
                formatter,
                "music target local FormID is duplicated: {:06X}",
                form_key.local
            ),
            Self::InvalidEditorId(form_key) => write!(
                formatter,
                "music record {:06X} has duplicate or malformed EDID fields",
                form_key.local
            ),
        }
    }
}

impl std::error::Error for MusicContractError {}

pub(crate) fn classify_music_component(
    records: &[Record],
    interner: &StringInterner,
) -> MusicSupport {
    if records
        .iter()
        .all(|record| !matches!(record.sig.as_str(), "MUSC" | "MUST"))
    {
        return MusicSupport::NotMusic;
    }

    match build_music_component_plan(records, interner) {
        Ok(plan) => MusicSupport::Supported(plan),
        Err(error) => MusicSupport::Unsupported(error),
    }
}

pub(crate) fn lower_supported_music_component(
    plan: &MusicComponentPlan,
    target_mappings: &HashMap<FormKey, FormKey>,
    output_plugin: Sym,
    interner: &StringInterner,
) -> Result<(Vec<Record>, MusicClosureReceipt), MusicContractError> {
    let namespace = output_namespace(output_plugin, interner)?;
    let music_target = mapped_target(plan.music_type.form_key, target_mappings, output_plugin)?;
    let track_targets = plan
        .tracks
        .iter()
        .map(|track| mapped_target(track.form_key, target_mappings, output_plugin))
        .collect::<Result<Vec<_>, _>>()?;
    validate_unique_targets(std::iter::once(music_target).chain(track_targets.iter().copied()))?;

    let mut music_type = plan.music_type.clone();
    music_type.form_key = music_target;
    rewrite_editor_id(
        &mut music_type,
        &namespaced_editor_id(&namespace, &plan.music_type, "Music", interner),
        interner,
    )?;
    rewrite_track_list(&mut music_type, &track_targets)?;

    let mut records = Vec::with_capacity(plan.tracks.len() + 1);
    let mut receipt = MusicClosureReceipt {
        record_edges: Vec::with_capacity(plan.tracks.len() + 1),
        track_edges: Vec::with_capacity(plan.tracks.len()),
        assets: Vec::with_capacity(plan.tracks.len()),
    };
    receipt.record_edges.push(MusicRecordEdge {
        source: plan.music_type.form_key,
        target: music_target,
        signature: plan.music_type.sig,
    });
    records.push(music_type);

    for ((source_track, target_track), source_record_path) in plan
        .tracks
        .iter()
        .zip(track_targets)
        .zip(&plan.source_record_paths)
    {
        let paths = lower_track_paths(source_track.form_key, source_record_path, &namespace)?;
        let mut track = source_track.clone();
        track.form_key = target_track;
        rewrite_editor_id(
            &mut track,
            &namespaced_editor_id(&namespace, source_track, "MusicTrack", interner),
            interner,
        )?;
        rewrite_track_path(&mut track, &paths.target_record_path, interner)?;

        receipt.record_edges.push(MusicRecordEdge {
            source: source_track.form_key,
            target: target_track,
            signature: source_track.sig,
        });
        receipt.track_edges.push(MusicTrackEdge {
            source_music: plan.music_type.form_key,
            target_music: music_target,
            source_track: source_track.form_key,
            target_track,
        });
        receipt.assets.push(MusicAssetEdge {
            source_track: source_track.form_key,
            target_track,
            source_record_path: source_record_path.clone(),
            target_record_path: paths.target_record_path,
            source_asset_path: paths.source_asset_path,
            target_asset_path: paths.target_asset_path,
        });
        records.push(track);
    }

    Ok((records, receipt))
}

fn build_music_component_plan(
    records: &[Record],
    interner: &StringInterner,
) -> Result<MusicComponentPlan, MusicContractError> {
    let mut seen = HashSet::with_capacity(records.len());
    let mut music_type = None;
    let mut tracks = HashMap::new();
    for record in records {
        if !seen.insert(record.form_key) {
            return Err(MusicContractError::DuplicateRecord(record.form_key));
        }
        match record.sig.0 {
            value if value == *b"MUSC" => {
                if music_type.replace(record.clone()).is_some() {
                    return Err(MusicContractError::MultipleMusicTypes);
                }
            }
            value if value == *b"MUST" => {
                tracks.insert(record.form_key, record.clone());
            }
            _ => {
                return Err(MusicContractError::UnexpectedRecord(
                    record.form_key,
                    record.sig,
                ));
            }
        }
    }
    let music_type = music_type.ok_or(MusicContractError::MissingMusicType)?;
    let track_form_keys = music_track_form_keys(&music_type)?;
    let mut referenced = HashSet::with_capacity(track_form_keys.len());
    let mut ordered_tracks = Vec::with_capacity(track_form_keys.len());
    let mut source_record_paths = Vec::with_capacity(track_form_keys.len());
    let mut seen_paths = HashSet::with_capacity(track_form_keys.len());

    for track_form_key in track_form_keys {
        if !referenced.insert(track_form_key) {
            return Err(MusicContractError::DuplicateTrackReference(track_form_key));
        }
        let track = tracks
            .get(&track_form_key)
            .ok_or(MusicContractError::MissingTrack(track_form_key))?;
        let path = track_record_path(track, interner)?;
        let normalized = normalize_record_path(track.form_key, &path)?;
        if !seen_paths.insert(normalized.to_ascii_lowercase()) {
            return Err(MusicContractError::DuplicateTrackPath(path));
        }
        ordered_tracks.push(track.clone());
        source_record_paths.push(path);
    }
    if let Some(unreferenced) = tracks.keys().find(|key| !referenced.contains(key)) {
        return Err(MusicContractError::UnreferencedTrack(*unreferenced));
    }

    Ok(MusicComponentPlan {
        music_type,
        tracks: ordered_tracks,
        source_record_paths,
    })
}

fn music_track_form_keys(record: &Record) -> Result<Vec<FormKey>, MusicContractError> {
    let mut fields = record.fields.iter().filter(|field| field.sig.0 == *b"TNAM");
    let field = fields
        .next()
        .ok_or(MusicContractError::MissingTrackList(record.form_key))?;
    if fields.next().is_some() {
        return Err(MusicContractError::InvalidTrackList(record.form_key));
    }
    let FieldValue::List(values) = &field.value else {
        return Err(MusicContractError::InvalidTrackList(record.form_key));
    };
    if values.is_empty() {
        return Err(MusicContractError::InvalidTrackList(record.form_key));
    }
    values
        .iter()
        .map(|value| match value {
            FieldValue::FormKey(form_key) => Ok(*form_key),
            _ => Err(MusicContractError::InvalidTrackList(record.form_key)),
        })
        .collect()
}

fn track_record_path(
    record: &Record,
    interner: &StringInterner,
) -> Result<String, MusicContractError> {
    let mut fields = record.fields.iter().filter(|field| field.sig.0 == *b"ANAM");
    let field = fields
        .next()
        .ok_or(MusicContractError::MissingTrackPath(record.form_key))?;
    if fields.next().is_some() {
        return Err(MusicContractError::MissingTrackPath(record.form_key));
    }
    let FieldValue::String(path) = field.value else {
        return Err(MusicContractError::MissingTrackPath(record.form_key));
    };
    interner
        .resolve(path)
        .filter(|path| !path.is_empty())
        .map(str::to_owned)
        .ok_or(MusicContractError::MissingTrackPath(record.form_key))
}

struct LoweredTrackPaths {
    source_asset_path: String,
    target_record_path: String,
    target_asset_path: String,
}

fn lower_track_paths(
    form_key: FormKey,
    source_record_path: &str,
    namespace: &str,
) -> Result<LoweredTrackPaths, MusicContractError> {
    let relative = normalize_record_path(form_key, source_record_path)?;
    let stem = relative
        .rsplit_once('.')
        .map_or(relative.as_str(), |(stem, _)| stem);
    Ok(LoweredTrackPaths {
        source_asset_path: format!(r"Music\{stem}.xwm"),
        target_record_path: format!(r"Data\Music\{namespace}\{stem}.wav"),
        target_asset_path: format!(r"Music\{namespace}\{stem}.xwm"),
    })
}

fn normalize_record_path(
    form_key: FormKey,
    source_record_path: &str,
) -> Result<String, MusicContractError> {
    let normalized = source_record_path.trim().replace('/', r"\");
    let trimmed = normalized.trim_matches('\\');
    let lower = trimmed.to_ascii_lowercase();
    let relative = if lower.starts_with(r"data\music\") {
        &trimmed[r"data\music\".len()..]
    } else if lower.starts_with(r"music\") {
        &trimmed[r"music\".len()..]
    } else {
        return Err(MusicContractError::InvalidTrackPath(
            form_key,
            source_record_path.to_owned(),
        ));
    };
    let Some((_, extension)) = relative.rsplit_once('.') else {
        return Err(MusicContractError::InvalidTrackPath(
            form_key,
            source_record_path.to_owned(),
        ));
    };
    if relative.is_empty()
        || !matches!(extension.to_ascii_lowercase().as_str(), "wav" | "xwm")
        || relative
            .split('\\')
            .any(|part| part.is_empty() || part == "." || part == ".." || part.contains(':'))
    {
        return Err(MusicContractError::InvalidTrackPath(
            form_key,
            source_record_path.to_owned(),
        ));
    }
    Ok(relative.to_owned())
}

fn output_namespace(
    output_plugin: Sym,
    interner: &StringInterner,
) -> Result<String, MusicContractError> {
    let plugin = interner
        .resolve(output_plugin)
        .ok_or(MusicContractError::InvalidOutputPlugin)?;
    let stem = plugin.rsplit_once('.').map_or(plugin, |(stem, _)| stem);
    let namespace: String = stem
        .chars()
        .filter_map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '_' | '-') {
                Some(character)
            } else if character.is_ascii_whitespace() {
                Some('_')
            } else {
                None
            }
        })
        .collect();
    if namespace.is_empty() || namespace == "." || namespace == ".." {
        return Err(MusicContractError::InvalidOutputPlugin);
    }
    Ok(namespace)
}

fn namespaced_editor_id(
    namespace: &str,
    source: &Record,
    fallback_kind: &str,
    interner: &StringInterner,
) -> String {
    let namespace = namespace.replace('-', "_");
    let suffix = source
        .eid
        .and_then(|editor_id| interner.resolve(editor_id))
        .filter(|editor_id| !editor_id.is_empty())
        .map(sanitize_editor_id)
        .filter(|editor_id| !editor_id.is_empty())
        .unwrap_or_else(|| format!("{fallback_kind}_{:06X}", source.form_key.local));
    format!("{namespace}_{suffix}")
}

fn sanitize_editor_id(editor_id: &str) -> String {
    editor_id
        .chars()
        .filter_map(|character| {
            if character.is_ascii_alphanumeric() || character == '_' {
                Some(character)
            } else if character.is_ascii_whitespace() || character == '-' {
                Some('_')
            } else {
                None
            }
        })
        .collect()
}

fn mapped_target(
    source: FormKey,
    mappings: &HashMap<FormKey, FormKey>,
    output_plugin: Sym,
) -> Result<FormKey, MusicContractError> {
    let target = mappings
        .get(&source)
        .copied()
        .ok_or(MusicContractError::MissingTargetMapping(source))?;
    if target.plugin != output_plugin {
        return Err(MusicContractError::InvalidTargetPlugin(target));
    }
    if target.local == 0 || target.local > 0x00FF_FFFF {
        return Err(MusicContractError::InvalidTargetLocalId(target));
    }
    Ok(target)
}

fn validate_unique_targets(
    targets: impl IntoIterator<Item = FormKey>,
) -> Result<(), MusicContractError> {
    let mut seen = HashSet::new();
    for target in targets {
        if !seen.insert(target) {
            return Err(MusicContractError::DuplicateTarget(target));
        }
    }
    Ok(())
}

fn rewrite_editor_id(
    record: &mut Record,
    editor_id: &str,
    interner: &StringInterner,
) -> Result<(), MusicContractError> {
    let editor_id = interner.intern(editor_id);
    let mut indices = record
        .fields
        .iter()
        .enumerate()
        .filter(|(_, field)| field.sig.0 == *b"EDID")
        .map(|(index, _)| index);
    if let Some(index) = indices.next() {
        if indices.next().is_some() || !matches!(record.fields[index].value, FieldValue::String(_))
        {
            return Err(MusicContractError::InvalidEditorId(record.form_key));
        }
        record.fields[index].value = FieldValue::String(editor_id);
    } else {
        record.fields.insert(
            0,
            FieldEntry {
                sig: SubrecordSig::from_str("EDID").expect("static subrecord signature is valid"),
                value: FieldValue::String(editor_id),
            },
        );
    }
    record.eid = Some(editor_id);
    Ok(())
}

fn rewrite_track_list(
    record: &mut Record,
    target_tracks: &[FormKey],
) -> Result<(), MusicContractError> {
    let mut fields = record
        .fields
        .iter_mut()
        .filter(|field| field.sig.0 == *b"TNAM");
    let field = fields
        .next()
        .ok_or(MusicContractError::MissingTrackList(record.form_key))?;
    if fields.next().is_some() {
        return Err(MusicContractError::InvalidTrackList(record.form_key));
    }
    field.value = FieldValue::List(
        target_tracks
            .iter()
            .copied()
            .map(FieldValue::FormKey)
            .collect(),
    );
    Ok(())
}

fn rewrite_track_path(
    record: &mut Record,
    target_path: &str,
    interner: &StringInterner,
) -> Result<(), MusicContractError> {
    let mut fields = record
        .fields
        .iter_mut()
        .filter(|field| field.sig.0 == *b"ANAM");
    let field = fields
        .next()
        .ok_or(MusicContractError::MissingTrackPath(record.form_key))?;
    if fields.next().is_some() || !matches!(field.value, FieldValue::String(_)) {
        return Err(MusicContractError::MissingTrackPath(record.form_key));
    }
    field.value = FieldValue::String(interner.intern(target_path));
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use serde::Deserialize;
    use smallvec::SmallVec;

    use super::*;
    use crate::ids::SubrecordSig;
    use crate::record::{FieldEntry, RecordFlags};

    #[derive(Deserialize)]
    struct RewardFixture {
        source_plugin: String,
        music: FixtureRecord,
        tracks: Vec<FixtureTrack>,
    }

    #[derive(Deserialize)]
    struct FixtureRecord {
        local_id: u32,
        editor_id: String,
    }

    #[derive(Deserialize)]
    struct FixtureTrack {
        local_id: u32,
        editor_id: String,
        record_path: String,
    }

    fn fixture() -> RewardFixture {
        serde_json::from_str(include_str!("test_fixtures/reward_music_component.json")).unwrap()
    }

    fn field(signature: &str, value: FieldValue) -> FieldEntry {
        FieldEntry {
            sig: SubrecordSig::from_str(signature).unwrap(),
            value,
        }
    }

    fn source_records(interner: &StringInterner) -> Vec<Record> {
        let fixture = fixture();
        let plugin = interner.intern(&fixture.source_plugin);
        let track_form_keys: Vec<_> = fixture
            .tracks
            .iter()
            .map(|track| FormKey {
                local: track.local_id,
                plugin,
            })
            .collect();
        let mut music = Record::new(
            SigCode::from_str("MUSC").unwrap(),
            FormKey {
                local: fixture.music.local_id,
                plugin,
            },
        );
        let music_editor_id = interner.intern(&fixture.music.editor_id);
        music.eid = Some(music_editor_id);
        music.fields = SmallVec::from_vec(vec![
            field("EDID", FieldValue::String(music_editor_id)),
            field("FNAM", FieldValue::Uint(33)),
            field(
                "PNAM",
                FieldValue::Bytes(SmallVec::from_slice(&[2, 0, 208, 7])),
            ),
            field("WNAM", FieldValue::Float(0.0)),
            field(
                "TNAM",
                FieldValue::List(
                    track_form_keys
                        .iter()
                        .copied()
                        .map(FieldValue::FormKey)
                        .collect(),
                ),
            ),
        ]);
        let mut records = vec![music];
        for (track_fixture, form_key) in fixture.tracks.iter().zip(track_form_keys) {
            let mut track = Record::new(SigCode::from_str("MUST").unwrap(), form_key);
            let editor_id = interner.intern(&track_fixture.editor_id);
            track.eid = Some(editor_id);
            track.fields = SmallVec::from_vec(vec![
                field("EDID", FieldValue::String(editor_id)),
                field("CNAM", FieldValue::Uint(1_859_641_416)),
                field(
                    "ANAM",
                    FieldValue::String(interner.intern(&track_fixture.record_path)),
                ),
            ]);
            records.push(track);
        }
        records
    }

    fn target_mappings(records: &[Record], output_plugin: Sym) -> HashMap<FormKey, FormKey> {
        records
            .iter()
            .enumerate()
            .map(|(index, record)| {
                (
                    record.form_key,
                    FormKey {
                        local: 0x800 + index as u32,
                        plugin: output_plugin,
                    },
                )
            })
            .collect()
    }

    fn supported_plan(records: &[Record], interner: &StringInterner) -> MusicComponentPlan {
        match classify_music_component(records, interner) {
            MusicSupport::Supported(plan) => plan,
            other => panic!("expected supported music component, got {other:?}"),
        }
    }

    #[test]
    fn reward_fixture_lowers_as_a_generic_closed_component() {
        let interner = StringInterner::new();
        let source = source_records(&interner);
        let plan = supported_plan(&source, &interner);
        let output_plugin = interner.intern("Skyrim.esm");
        let mappings = target_mappings(&source, output_plugin);

        let (records, receipt) =
            lower_supported_music_component(&plan, &mappings, output_plugin, &interner).unwrap();

        assert_eq!(records.len(), 5);
        assert_eq!(receipt.record_edges.len(), 5);
        assert_eq!(receipt.track_edges.len(), 4);
        assert_eq!(receipt.assets.len(), 4);
        assert_eq!(records[0].sig.as_str(), "MUSC");
        assert!(
            records[1..]
                .iter()
                .all(|record| record.sig.as_str() == "MUST")
        );
        let target_tracks: Vec<_> = receipt
            .track_edges
            .iter()
            .map(|edge| FieldValue::FormKey(edge.target_track))
            .collect();
        assert!(records[0].fields.iter().any(|field| {
            field.sig.0 == *b"TNAM" && field.value == FieldValue::List(target_tracks.clone())
        }));
    }

    #[test]
    fn lowering_namespaces_records_and_physical_xwm_assets() {
        let interner = StringInterner::new();
        let source = source_records(&interner);
        let plan = supported_plan(&source, &interner);
        let output_plugin = interner.intern("Skyrim.esm");
        let mappings = target_mappings(&source, output_plugin);

        let (records, receipt) =
            lower_supported_music_component(&plan, &mappings, output_plugin, &interner).unwrap();

        assert_eq!(
            interner.resolve(records[0].eid.unwrap()),
            Some("Skyrim_MUSReward")
        );
        assert!(receipt.assets.iter().all(|asset| {
            asset.target_record_path.starts_with(r"Data\Music\Skyrim\")
                && asset.target_record_path.ends_with(".wav")
                && asset.target_asset_path.starts_with(r"Music\Skyrim\")
                && asset.target_asset_path.ends_with(".xwm")
        }));
        assert_eq!(
            receipt.assets[0].source_asset_path,
            r"Music\Reward\MUS_Reward_01.xwm"
        );
    }

    #[test]
    fn classification_fails_closed_for_missing_or_extra_tracks() {
        let interner = StringInterner::new();
        let mut missing = source_records(&interner);
        let removed = missing.pop().unwrap();
        assert!(matches!(
            classify_music_component(&missing, &interner),
            MusicSupport::Unsupported(MusicContractError::MissingTrack(key))
                if key == removed.form_key
        ));

        let mut extra = source_records(&interner);
        let plugin = extra[0].form_key.plugin;
        let mut track = Record::new(
            SigCode::from_str("MUST").unwrap(),
            FormKey {
                local: 0x123456,
                plugin,
            },
        );
        let path = interner.intern(r"Data\Music\Unused\Track.wav");
        track.fields.push(field("ANAM", FieldValue::String(path)));
        extra.push(track);
        assert!(matches!(
            classify_music_component(&extra, &interner),
            MusicSupport::Unsupported(MusicContractError::UnreferencedTrack(_))
        ));
    }

    #[test]
    fn lowering_rejects_colliding_or_non_owned_targets() {
        let interner = StringInterner::new();
        let source = source_records(&interner);
        let plan = supported_plan(&source, &interner);
        let output_plugin = interner.intern("Skyrim.esm");
        let mut mappings = target_mappings(&source, output_plugin);
        mappings.insert(source[4].form_key, mappings[&source[1].form_key]);
        assert!(matches!(
            lower_supported_music_component(&plan, &mappings, output_plugin, &interner),
            Err(MusicContractError::DuplicateTarget(_))
        ));

        let mut mappings = target_mappings(&source, output_plugin);
        mappings.get_mut(&source[4].form_key).unwrap().plugin = interner.intern("Fallout4.esm");
        assert!(matches!(
            lower_supported_music_component(&plan, &mappings, output_plugin, &interner),
            Err(MusicContractError::InvalidTargetPlugin(_))
        ));
    }

    #[test]
    fn source_semantics_survive_identity_and_path_lowering() {
        let interner = StringInterner::new();
        let mut source = source_records(&interner);
        source[0].flags = RecordFlags::PERSISTENT;
        source[1].warnings.push(interner.intern("fixture-warning"));
        let plan = supported_plan(&source, &interner);
        let output_plugin = interner.intern("Skyrim.esm");
        let mappings = target_mappings(&source, output_plugin);

        let (records, _) =
            lower_supported_music_component(&plan, &mappings, output_plugin, &interner).unwrap();

        assert_eq!(records[0].flags, RecordFlags::PERSISTENT);
        assert_eq!(records[1].warnings, source[1].warnings);
        assert!(
            records[0]
                .fields
                .iter()
                .any(|field| field.sig.0 == *b"FNAM" && field.value == FieldValue::Uint(33))
        );
    }

    #[test]
    fn non_music_records_are_not_claimed() {
        let interner = StringInterner::new();
        let plugin = interner.intern("Skyrim.esm");
        let record = Record::new(
            SigCode::from_str("QUST").unwrap(),
            FormKey { local: 1, plugin },
        );
        assert!(matches!(
            classify_music_component(&[record], &interner),
            MusicSupport::NotMusic
        ));
    }
}
