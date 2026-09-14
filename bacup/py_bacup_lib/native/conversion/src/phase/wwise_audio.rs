//! `wwise_audio`: FO4 music/ambience content leg of the `fo4:starfield` pair.
//!
//! Enumerates FO4 music (`MUST.ANAM`/`BNAM` track paths) and ambience
//! (`ASPC.SNAM` -> `SNDR.Sound`, first path only) from the SOURCE plugin and
//! resolves each against `FO4_EXTRACTED_DIR`. The ESM always claims `.wav`
//! but the shipped asset is usually `.xwm`: strip a leading
//! `data\`/`Data\`/`\Data\`, lowercase, try `.xwm` then `.wav`. Resolved files
//! go to `crate::wwise::build_bank`; one `WWED` record is synthesized per Wwise
//! event (R2 §3), and a manifest is written for `audio_rewire`. Unresolved
//! source paths are logged and skipped, never a hard error. R2/R6 are
//! `bacup/docs/starfield_target/R2-audio-records.md` / `R6-wwise-toolchain.md`.
//!
//! Params (JSON):
//! ```json
//! {
//!   "placeholder_audio": false,
//!   "tools": {
//!     "xwm_decode_cmd": ["<path>/xwmaencode.exe", "{in}", "{out}"],
//!     "ffmpeg_cmd": ["<path>/ffmpeg.exe", "-y", "-i", "{in}", "-ar", "48000", "-ac", "2", "{out}"],
//!     "wwise_console": "<WWISEROOT>/Authoring/x64/Release/bin/WwiseConsole.exe",
//!     "copy_streamed_files": "<WWISEROOT>/Authoring/x64/Release/bin/CopyStreamedFiles.exe",
//!     "wwise_project_src": "<STARFIELD_DIR>/Tools/wwise/Starfield"
//!   }
//! }
//! ```
//! `tools` is required unless `placeholder_audio` is `true` (the alias
//! `placeholder_events` is also accepted).
//!
//! # Manifest schema: `debug/wwise/events_manifest.json`
//!
//! ## Normal mode (`placeholder_audio: false`)
//! A flat JSON array, one row per (FO4 identity, role) pair built into a
//! Wwise event, i.e. one row per `build_bank` input, like
//! `WwiseBankOut.events`:
//! ```json
//! [
//!   {
//!     "fo4_form_key": "0217A3@Fallout4.esm",
//!     "fo4_editor_id": "MUSGenesisCombatHigh03Track",
//!     "role": "music_start" | "music_stop" | "ambience_loop",
//!     "source_rel": "music/creationclub/_shared/explore_01.xwm",
//!     "event": "FO4SF_MUSIC_CREATIONCLUB__SHARED_EXPLORE_01_XWM",
//!     "event_guid": "7376FFF8-E9A2-40FB-AFE8-C0FF75148AF7",
//!     "wwed_editor_id": "FO4SF_WWED_FO4SF_MUSIC_...",
//!     "wwed_local_formid": "0x000800"
//!   }
//! ]
//! ```
//! `event_guid` is the CANONICAL dashed Wwise GUID string, not the event name
//! or FormID. `audio_rewire` recovers the exact WWED/MTSH/ASLS storage bytes
//! via `conversion_native::wwise::encode::guid_string_to_bytes` (R2 §2.2/R6
//! §2.4: each 8-byte half of the canonical GUID byte-reversed, i.e. two
//! little-endian u64s). A `music_start`/`music_stop` pair with the same
//! `fo4_form_key` shares one WWED (`wwed_editor_id`/`wwed_local_formid`
//! identical on both rows): its `WSED` is the `music_start` row's bytes and
//! `WTED` the `music_stop` row's (absent if no stop row). `ambience_loop` rows
//! are always solo (Start only, no End).
//!
//! ## Placeholder mode (`placeholder_audio: true`)
//! No source enumeration, bank files, or WWED records (R2 §9). `audio_rewire`
//! points every `CELL.XCMO`/`WRLD.ZNAM`/`CELL.XCAS`/`ASPC.BNAM` in the output
//! at one of these fixed vanilla Starfield records and leaves every Sound
//! Event Set zeroed (vanilla-legal, cannot crash):
//! ```json
//! {
//!   "mode": "placeholder",
//!   "sound_event_sets": "zeroed",
//!   "targets": [
//!     {"purpose": "music_explore", "record": "MUSC", "editor_id": "MUSGenesisPlanetA_Mountains", "form_id": "00C5D2"},
//!     {"purpose": "music_combat", "record": "MUSC", "editor_id": "MUSGenesisCombat", "form_id": "013D5F"},
//!     {"purpose": "music_city", "record": "MUSC", "editor_id": "MUSGenesisCityA_NewAtlantis", "form_id": "010DFE"},
//!     {"purpose": "music_dungeon", "record": "MUSC", "editor_id": "MUSGenesisDungeonIndustrialC", "form_id": "1804D6"},
//!     {"purpose": "music_silence", "record": "MUSC", "editor_id": "_MUSExplore_WwiseSilence", "form_id": "13E4E7"},
//!     {"purpose": "ambient_interior", "record": "ASPC", "editor_id": "Int_Space_Ship_Science_Medium", "form_id": "0013FA"},
//!     {"purpose": "reverb_default", "record": "REVB", "editor_id": "DefaultReverb", "form_id": "0C5B6E"}
//!   ]
//! }
//! ```
//! `form_id` values are Starfield.esm local FormIDs (hex, no plugin suffix;
//! the target master is always `Starfield.esm` for this pair).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde_json::{Value as JsonValue, json};
use smallvec::SmallVec;

use crate::formkey_mapper::FormKeyMapper;
use crate::ids::{FormKey, SigCode, SubrecordSig};
use crate::phase::{LogLevel, Phase, PhaseCtx, PhaseError, PhaseEvent, PhaseReport};
use crate::record::{FieldEntry, FieldValue, Record};
use crate::sym::StringInterner;
use crate::target_write::add_record_native;
use crate::wwise::{self, WwiseBankOut, WwiseEvent, WwiseTools};

/// Synthetic plugin symbol seeding fresh `FormKeyMapper` allocations for WWED
/// records — mirrors the `__synth_eczn__` convention used by encounter-zone
/// synthesis. Never a real plugin name, so it can never collide with a real
/// source FormKey.
const SYNTH_WWISE_PLUGIN: &str = "__synth_wwise_audio__";

/// Placeholder targets (R2 §9): vanilla Starfield records confirmed present
/// and resolvable, safe to point CELL.XCMO/WRLD.ZNAM/CELL.XCAS/ASPC.BNAM at
/// when no Wwise toolchain is available.
struct VanillaTarget {
    purpose: &'static str,
    record: &'static str,
    editor_id: &'static str,
    form_id: &'static str,
}

const PLACEHOLDER_TARGETS: &[VanillaTarget] = &[
    VanillaTarget {
        purpose: "music_explore",
        record: "MUSC",
        editor_id: "MUSGenesisPlanetA_Mountains",
        form_id: "00C5D2",
    },
    VanillaTarget {
        purpose: "music_combat",
        record: "MUSC",
        editor_id: "MUSGenesisCombat",
        form_id: "013D5F",
    },
    VanillaTarget {
        purpose: "music_city",
        record: "MUSC",
        editor_id: "MUSGenesisCityA_NewAtlantis",
        form_id: "010DFE",
    },
    VanillaTarget {
        purpose: "music_dungeon",
        record: "MUSC",
        editor_id: "MUSGenesisDungeonIndustrialC",
        form_id: "1804D6",
    },
    VanillaTarget {
        purpose: "music_silence",
        record: "MUSC",
        editor_id: "_MUSExplore_WwiseSilence",
        form_id: "13E4E7",
    },
    VanillaTarget {
        purpose: "ambient_interior",
        record: "ASPC",
        editor_id: "Int_Space_Ship_Science_Medium",
        form_id: "0013FA",
    },
    VanillaTarget {
        purpose: "reverb_default",
        record: "REVB",
        editor_id: "DefaultReverb",
        form_id: "0C5B6E",
    },
];

// ---------------------------------------------------------------------------
// Audio reference model
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AudioRole {
    MusicStart,
    MusicStop,
    AmbienceLoop,
}

impl AudioRole {
    fn as_str(self) -> &'static str {
        match self {
            AudioRole::MusicStart => "music_start",
            AudioRole::MusicStop => "music_stop",
            AudioRole::AmbienceLoop => "ambience_loop",
        }
    }
}

/// One FO4 audio reference discovered on a source `MUST` or `ASPC` record,
/// before path resolution.
#[derive(Debug, Clone)]
struct AudioRef {
    fo4_form_key: FormKey,
    fo4_editor_id: String,
    role: AudioRole,
    /// Raw FO4-recorded path (backslash form, claims `.wav`, may actually
    /// ship as `.xwm` on disk — see `resolve_source_audio`).
    source_rel_path: String,
}

fn eid_of(record: &Record, interner: &StringInterner) -> String {
    record
        .eid
        .and_then(|sym| interner.resolve(sym))
        .unwrap_or("")
        .to_string()
}

fn string_field(record: &Record, sig: &str, interner: &StringInterner) -> Option<String> {
    record
        .fields
        .iter()
        .find(|f| f.sig.as_str() == sig)
        .and_then(|f| match &f.value {
            FieldValue::String(sym) => interner.resolve(*sym).map(str::to_string),
            _ => None,
        })
}

fn form_key_field(record: &Record, sig: &str) -> Option<FormKey> {
    record
        .fields
        .iter()
        .find(|f| f.sig.as_str() == sig)
        .and_then(|f| match &f.value {
            FieldValue::FormKey(fk) => Some(*fk),
            _ => None,
        })
}

/// FO4 `MUST.ANAM` ("Track FileName" start) and `MUST.BNAM` ("Finale
/// FileName" stop) -> up to two `AudioRef`s (R2 §7.6).
fn audio_refs_from_must(record: &Record, interner: &StringInterner) -> Vec<AudioRef> {
    let eid = eid_of(record, interner);
    let mut out = Vec::new();
    if let Some(path) = string_field(record, "ANAM", interner) {
        out.push(AudioRef {
            fo4_form_key: record.form_key,
            fo4_editor_id: eid.clone(),
            role: AudioRole::MusicStart,
            source_rel_path: path,
        });
    }
    if let Some(path) = string_field(record, "BNAM", interner) {
        out.push(AudioRef {
            fo4_form_key: record.form_key,
            fo4_editor_id: eid,
            role: AudioRole::MusicStop,
            source_rel_path: path,
        });
    }
    out
}

/// FO4 `SNDR`'s first repeated `ANAM` ("Sound") path (R2 §8.4: "resolve the
/// SNDR's first Sound path").
fn sndr_first_sound_path(record: &Record, interner: &StringInterner) -> Option<String> {
    record
        .fields
        .iter()
        .find(|f| f.sig.as_str() == "ANAM")
        .and_then(|f| match &f.value {
            FieldValue::String(sym) => interner.resolve(*sym).map(str::to_string),
            _ => None,
        })
}

/// Walks the source FO4 plugin for `MUST` and `ASPC`/`SNDR` audio references.
fn enumerate_audio_refs(ctx: &PhaseCtx<'_>) -> Result<Vec<AudioRef>, PhaseError> {
    let handle = ctx.run.source_handle_id;
    let schema = ctx.run.schema_source.as_ref();
    let interner = &ctx.run.interner;
    let mut refs = Vec::new();

    let must_sig = SigCode::from_str("MUST").map_err(PhaseError::Internal)?;
    for fk in crate::source_read::iter_form_keys_of_sig(handle, must_sig, interner)
        .map_err(|e| PhaseError::Internal(e.to_string()))?
    {
        let record = crate::source_read::read_record_relayout_by_form_key(
            handle, &fk, schema, interner, None,
        )
        .map_err(|e| PhaseError::Internal(e.to_string()))?;
        refs.extend(audio_refs_from_must(&record, interner));
    }

    let aspc_sig = SigCode::from_str("ASPC").map_err(PhaseError::Internal)?;
    for fk in crate::source_read::iter_form_keys_of_sig(handle, aspc_sig, interner)
        .map_err(|e| PhaseError::Internal(e.to_string()))?
    {
        let record = crate::source_read::read_record_relayout_by_form_key(
            handle, &fk, schema, interner, None,
        )
        .map_err(|e| PhaseError::Internal(e.to_string()))?;
        let Some(sndr_fk) = form_key_field(&record, "SNAM") else {
            continue;
        };
        let Ok(sndr_record) = crate::source_read::read_record_relayout_by_form_key(
            handle, &sndr_fk, schema, interner, None,
        ) else {
            continue;
        };
        let Some(path) = sndr_first_sound_path(&sndr_record, interner) else {
            continue;
        };
        refs.push(AudioRef {
            fo4_form_key: fk,
            fo4_editor_id: eid_of(&record, interner),
            role: AudioRole::AmbienceLoop,
            source_rel_path: path,
        });
    }

    Ok(refs)
}

// ---------------------------------------------------------------------------
// Path resolution
// ---------------------------------------------------------------------------

fn strip_data_prefix(raw: &str) -> &str {
    let mut s = raw;
    if let Some(rest) = s.strip_prefix('\\') {
        s = rest;
    }
    if let Some(rest) = s.strip_prefix('/') {
        s = rest;
    }
    for prefix in ["data\\", "data/"] {
        if s.len() >= prefix.len() && s[..prefix.len()].eq_ignore_ascii_case(prefix) {
            return &s[prefix.len()..];
        }
    }
    s
}

/// Strip a leading `data\`/`Data\`/`\Data\`, lowercase, normalize slashes,
/// then try `.xwm` first and `.wav` second under `source_extracted_dir`
/// (R2 §7.6: the ESM always claims `.wav`; the shipped asset is usually
/// `.xwm`). Returns `(normalized_rel_key, absolute_path)` on success —
/// `normalized_rel_key` is also the `build_bank` input key / dedup key.
fn resolve_source_audio(source_extracted_dir: &Path, raw: &str) -> Option<(String, PathBuf)> {
    let stripped = strip_data_prefix(raw);
    let normalized = stripped.replace('\\', "/").to_ascii_lowercase();
    let stem = match normalized.rfind('.') {
        Some(idx) => &normalized[..idx],
        None => normalized.as_str(),
    };
    for ext in ["xwm", "wav"] {
        let rel = format!("{stem}.{ext}");
        let candidate = source_extracted_dir.join(Path::new(&rel));
        if candidate.is_file() {
            return Some((rel, candidate));
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Join resolved refs with build_bank's returned events
// ---------------------------------------------------------------------------

/// One `AudioRef` successfully resolved to a file AND matched back to its
/// synthesized Wwise event.
#[derive(Debug, Clone)]
struct JoinedRef {
    fo4_form_key: FormKey,
    fo4_editor_id: String,
    role: AudioRole,
    rel_key: String,
    event_name: String,
    event_guid: [u8; 16],
}

fn join_refs_with_events(
    resolved: &[(AudioRef, String)],
    bank_out: &WwiseBankOut,
) -> Vec<JoinedRef> {
    let events_by_key: HashMap<&str, &WwiseEvent> = bank_out
        .events
        .iter()
        .map(|e| (e.source_rel_path.as_str(), e))
        .collect();
    resolved
        .iter()
        .filter_map(|(r, key)| {
            let ev = events_by_key.get(key.as_str())?;
            Some(JoinedRef {
                fo4_form_key: r.fo4_form_key,
                fo4_editor_id: r.fo4_editor_id.clone(),
                role: r.role,
                rel_key: key.clone(),
                event_name: ev.event_name.clone(),
                event_guid: ev.event_guid,
            })
        })
        .collect()
}

/// Groups joined refs by FO4 identity: a `music_start`/`music_stop` pair (or
/// a solo `ambience_loop`) sharing one `fo4_form_key` becomes one WWED.
struct AudioGroup {
    fo4_form_key: FormKey,
    fo4_editor_id: String,
    start: Option<JoinedRef>,
    stop: Option<JoinedRef>,
}

fn group_joined_refs(joined: Vec<JoinedRef>) -> Vec<AudioGroup> {
    let mut order: Vec<FormKey> = Vec::new();
    let mut groups: HashMap<FormKey, AudioGroup> = HashMap::new();
    for r in joined {
        let group = groups.entry(r.fo4_form_key).or_insert_with(|| {
            order.push(r.fo4_form_key);
            AudioGroup {
                fo4_form_key: r.fo4_form_key,
                fo4_editor_id: r.fo4_editor_id.clone(),
                start: None,
                stop: None,
            }
        });
        match r.role {
            AudioRole::MusicStart | AudioRole::AmbienceLoop => group.start = Some(r),
            AudioRole::MusicStop => group.stop = Some(r),
        }
    }
    order
        .into_iter()
        .filter_map(|fk| groups.remove(&fk))
        .collect()
}

// ---------------------------------------------------------------------------
// WWED synthesis plan
// ---------------------------------------------------------------------------

struct ManifestRow {
    fo4_form_key: FormKey,
    fo4_editor_id: String,
    role: AudioRole,
    source_rel: String,
    event: String,
    event_guid: String,
}

struct WwedPlanEntry {
    wwed_form_key: FormKey,
    wwed_editor_id: String,
    start_guid: [u8; 16],
    stop_guid: Option<[u8; 16]>,
    manifest_rows: Vec<ManifestRow>,
}

/// R2 §2.2 / R6 §2.4: the ESM-stored 16 bytes are two little-endian u64
/// halves of the canonical GUID (hi‖lo). `WwiseEvent.event_guid` is already
/// in this order (produced by `crate::wwise::encode::guid_string_to_bytes`);
/// this is the inverse, formatting the canonical dashed string for the manifest.
fn canonical_guid_string(bytes: [u8; 16]) -> String {
    let hi = u64::from_le_bytes(bytes[0..8].try_into().unwrap());
    let lo = u64::from_le_bytes(bytes[8..16].try_into().unwrap());
    let hex = format!("{hi:016X}{lo:016X}");
    format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    )
}

/// Builds one WWED per audio group. `allocate_wwed_fk` decouples FormKey
/// allocation (real path: `FormKeyMapper`; tests: a trivial counter) from
/// this otherwise-pure planning logic.
fn build_wwed_plan(
    groups: &[AudioGroup],
    allocate_wwed_fk: &mut dyn FnMut(&str) -> FormKey,
) -> Vec<WwedPlanEntry> {
    let mut out = Vec::with_capacity(groups.len());
    for group in groups {
        let Some(start) = &group.start else { continue };
        let wwed_editor_id = format!("FO4SF_WWED_{}", start.event_name);
        let wwed_form_key = allocate_wwed_fk(&wwed_editor_id);

        let mut rows = vec![ManifestRow {
            fo4_form_key: group.fo4_form_key,
            fo4_editor_id: group.fo4_editor_id.clone(),
            role: start.role,
            source_rel: start.rel_key.clone(),
            event: start.event_name.clone(),
            event_guid: canonical_guid_string(start.event_guid),
        }];

        let stop_guid = group.stop.as_ref().map(|stop| {
            rows.push(ManifestRow {
                fo4_form_key: group.fo4_form_key,
                fo4_editor_id: group.fo4_editor_id.clone(),
                role: stop.role,
                source_rel: stop.rel_key.clone(),
                event: stop.event_name.clone(),
                event_guid: canonical_guid_string(stop.event_guid),
            });
            stop.event_guid
        });

        out.push(WwedPlanEntry {
            wwed_form_key,
            wwed_editor_id,
            start_guid: start.event_guid,
            stop_guid,
            manifest_rows: rows,
        });
    }
    out
}

fn plan_to_manifest_json(plan: &[WwedPlanEntry], interner: &StringInterner) -> JsonValue {
    let mut rows_json = Vec::new();
    for entry in plan {
        for row in &entry.manifest_rows {
            rows_json.push(json!({
                "fo4_form_key": row.fo4_form_key.format(interner),
                "fo4_editor_id": row.fo4_editor_id,
                "role": row.role.as_str(),
                "source_rel": row.source_rel,
                "event": row.event,
                "event_guid": row.event_guid,
                "wwed_editor_id": entry.wwed_editor_id,
                "wwed_local_formid": format!("0x{:06X}", entry.wwed_form_key.local),
            }));
        }
    }
    JsonValue::Array(rows_json)
}

/// R2 §2.1: `WWED` = `EDID` + `WSED` (Start, 16 B) + optional `WTED` (End,
/// 16 B). `struct:` codec subrecords reach the encoder as raw `FieldValue::Bytes`
/// passthrough (see `target_write::encode_value`), so no named struct keys
/// are needed here.
fn build_wwed_record(
    target_fk: FormKey,
    wwed_eid: &str,
    start_guid: [u8; 16],
    stop_guid: Option<[u8; 16]>,
    interner: &StringInterner,
) -> Result<Record, PhaseError> {
    let wwed_sig = SigCode::from_str("WWED").map_err(PhaseError::Internal)?;
    let edid_sig = SubrecordSig::from_str("EDID").map_err(PhaseError::Internal)?;
    let wsed_sig = SubrecordSig::from_str("WSED").map_err(PhaseError::Internal)?;
    let wted_sig = SubrecordSig::from_str("WTED").map_err(PhaseError::Internal)?;

    let mut record = Record::new(wwed_sig, target_fk);
    let eid_sym = interner.intern(wwed_eid);
    record.eid = Some(eid_sym);
    record.fields.push(FieldEntry {
        sig: edid_sig,
        value: FieldValue::String(eid_sym),
    });
    record.fields.push(FieldEntry {
        sig: wsed_sig,
        value: FieldValue::Bytes(SmallVec::from_slice(&start_guid)),
    });
    if let Some(stop) = stop_guid {
        record.fields.push(FieldEntry {
            sig: wted_sig,
            value: FieldValue::Bytes(SmallVec::from_slice(&stop)),
        });
    }
    Ok(record)
}

// ---------------------------------------------------------------------------
// Params / IO helpers
// ---------------------------------------------------------------------------

fn is_placeholder(params: &JsonValue) -> bool {
    params
        .get("placeholder_audio")
        .or_else(|| params.get("placeholder_events"))
        .and_then(JsonValue::as_bool)
        .unwrap_or(false)
}

fn parse_tools(params: &JsonValue) -> Result<WwiseTools, PhaseError> {
    let tools = params
        .get("tools")
        .ok_or_else(|| PhaseError::BadParams("wwise_audio: missing 'tools' params".into()))?;

    let str_vec = |key: &str| -> Result<Vec<String>, PhaseError> {
        tools
            .get(key)
            .and_then(JsonValue::as_array)
            .ok_or_else(|| {
                PhaseError::BadParams(format!("wwise_audio: tools.{key} missing or not an array"))
            })?
            .iter()
            .map(|v| {
                v.as_str().map(str::to_string).ok_or_else(|| {
                    PhaseError::BadParams(format!(
                        "wwise_audio: tools.{key} entries must be strings"
                    ))
                })
            })
            .collect()
    };
    let path_val = |key: &str| -> Result<PathBuf, PhaseError> {
        tools
            .get(key)
            .and_then(JsonValue::as_str)
            .map(PathBuf::from)
            .ok_or_else(|| {
                PhaseError::BadParams(format!("wwise_audio: tools.{key} missing or not a string"))
            })
    };

    Ok(WwiseTools {
        xwm_decode_cmd: str_vec("xwm_decode_cmd")?,
        ffmpeg_cmd: str_vec("ffmpeg_cmd")?,
        wwise_console: path_val("wwise_console")?,
        copy_streamed_files: path_val("copy_streamed_files")?,
        wwise_project_src: path_val("wwise_project_src")?,
    })
}

fn bank_name_from_output_plugin(output_plugin_name: &str) -> String {
    Path::new(output_plugin_name)
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| output_plugin_name.to_string())
}

fn write_manifest(mod_path: &Path, manifest: &JsonValue) -> Result<(), PhaseError> {
    let dir = mod_path.join("debug").join("wwise");
    std::fs::create_dir_all(&dir).map_err(|e| PhaseError::Internal(e.to_string()))?;
    let text =
        serde_json::to_string_pretty(manifest).map_err(|e| PhaseError::Internal(e.to_string()))?;
    std::fs::write(dir.join("events_manifest.json"), text)
        .map_err(|e| PhaseError::Internal(e.to_string()))?;
    Ok(())
}

fn placeholder_manifest_json() -> JsonValue {
    let targets: Vec<JsonValue> = PLACEHOLDER_TARGETS
        .iter()
        .map(|t| {
            json!({
                "purpose": t.purpose,
                "record": t.record,
                "editor_id": t.editor_id,
                "form_id": t.form_id,
            })
        })
        .collect();
    json!({
        "mode": "placeholder",
        "sound_event_sets": "zeroed",
        "targets": targets,
    })
}

// ---------------------------------------------------------------------------
// Phase
// ---------------------------------------------------------------------------

pub struct WwiseAudioPhase;

impl Phase for WwiseAudioPhase {
    fn name(&self) -> &'static str {
        "wwise_audio"
    }

    fn run(&self, ctx: &mut PhaseCtx<'_>) -> Result<PhaseReport, PhaseError> {
        if is_placeholder(ctx.params) {
            return run_placeholder(ctx);
        }
        run_normal(ctx)
    }
}

fn run_placeholder(ctx: &mut PhaseCtx<'_>) -> Result<PhaseReport, PhaseError> {
    write_manifest(ctx.mod_path, &placeholder_manifest_json())?;
    let _ = ctx.run.event_tx.try_send(PhaseEvent::Log {
        phase: "wwise_audio",
        level: LogLevel::Info,
        message: "wwise_audio: placeholder mode — vanilla event targets only, no bank files"
            .to_string(),
    });
    Ok(PhaseReport::default())
}

fn run_normal(ctx: &mut PhaseCtx<'_>) -> Result<PhaseReport, PhaseError> {
    let tools = parse_tools(ctx.params)?;
    let refs = enumerate_audio_refs(ctx)?;
    ctx.check_cancel()?;

    let mut resolved: Vec<(AudioRef, String)> = Vec::new();
    let mut inputs: Vec<(String, PathBuf)> = Vec::new();
    let mut seen_keys: HashMap<String, ()> = HashMap::new();
    let mut unresolved = 0u32;

    for r in refs {
        match resolve_source_audio(ctx.source_extracted_dir, &r.source_rel_path) {
            Some((key, abs)) => {
                if seen_keys.insert(key.clone(), ()).is_none() {
                    inputs.push((key.clone(), abs));
                }
                resolved.push((r, key));
            }
            None => {
                unresolved += 1;
                let _ = ctx.run.event_tx.try_send(PhaseEvent::Log {
                    phase: "wwise_audio",
                    level: LogLevel::Warn,
                    message: format!("wwise_audio: unresolved audio path: {}", r.source_rel_path),
                });
            }
        }
    }

    if inputs.is_empty() {
        write_manifest(ctx.mod_path, &JsonValue::Array(Vec::new()))?;
        return Ok(PhaseReport {
            warnings: unresolved,
            items_failed: unresolved,
            ..Default::default()
        });
    }

    let work_dir = ctx.mod_path.join("debug").join("wwise").join("_work");
    let bank_name = bank_name_from_output_plugin(&ctx.run.config.output_plugin_name);
    let bank_out =
        wwise::build_bank(&tools, &work_dir, &inputs, &bank_name).map_err(PhaseError::Internal)?;

    let data_root = ctx.mod_path.join("data");
    let mut assets_written = 0u32;
    for (rel, bytes) in &bank_out.bank_files {
        let dest = data_root.join(rel.replace('/', std::path::MAIN_SEPARATOR_STR));
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent).map_err(|e| PhaseError::Internal(e.to_string()))?;
        }
        std::fs::write(&dest, bytes).map_err(|e| PhaseError::Internal(e.to_string()))?;
        assets_written += 1;
    }

    let joined = join_refs_with_events(&resolved, &bank_out);
    let groups = group_joined_refs(joined);

    let synth_plugin_sym = ctx.run.interner.intern(SYNTH_WWISE_PLUGIN);
    let wwed_sig = SigCode::from_str("WWED").map_err(PhaseError::Internal)?;
    let mut wwed_local_counter = 0u32;
    let state = ctx.run.mapper_state.as_mut().ok_or_else(|| {
        PhaseError::Internal(
            "wwise_audio: mapper_state not initialized; must run after translate".into(),
        )
    })?;
    let mut mapper = FormKeyMapper::from_state(state, &ctx.run.interner);
    let plan = {
        let mut allocate = |eid: &str| -> FormKey {
            wwed_local_counter += 1;
            let source_fk = FormKey {
                local: wwed_local_counter,
                plugin: synth_plugin_sym,
            };
            let eid_sym = mapper.interner.intern(eid);
            mapper.allocate_or_resolve(source_fk, Some(eid_sym), wwed_sig)
        };
        build_wwed_plan(&groups, &mut allocate)
    };

    let mut records_added = 0u32;
    for entry in &plan {
        let record = build_wwed_record(
            entry.wwed_form_key,
            &entry.wwed_editor_id,
            entry.start_guid,
            entry.stop_guid,
            &ctx.run.interner,
        )?;
        add_record_native(
            ctx.run.target_handle_id,
            record,
            ctx.run.schema_target.as_ref(),
            &ctx.run.interner,
        )
        .map_err(|e| PhaseError::Internal(e.to_string()))?;
        records_added += 1;
    }

    let manifest = plan_to_manifest_json(&plan, &ctx.run.interner);
    write_manifest(ctx.mod_path, &manifest)?;

    let _ = ctx.run.event_tx.try_send(PhaseEvent::Log {
        phase: "wwise_audio",
        level: LogLevel::Info,
        message: format!(
            "wwise_audio: events={} wwed_records={records_added} unresolved={unresolved}",
            bank_out.events.len()
        ),
    });

    Ok(PhaseReport {
        records_added,
        assets_written,
        warnings: unresolved,
        items_failed: unresolved,
        ..Default::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------
    // Path resolution
    // -----------------------------------------------------------------

    #[test]
    fn resolve_source_audio_prefers_xwm_over_wav() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("music/creationclub/_shared");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("explore_01.xwm"), b"xwm").unwrap();

        let (key, path) = resolve_source_audio(
            tmp.path(),
            r"data\Music\CreationClub\_Shared\Explore_01.wav",
        )
        .expect("should resolve to the .xwm sibling");
        assert_eq!(key, "music/creationclub/_shared/explore_01.xwm");
        assert!(path.is_file());
    }

    #[test]
    fn resolve_source_audio_falls_back_to_wav() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("sound/fx");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("boom.wav"), b"wav").unwrap();

        let (key, _path) =
            resolve_source_audio(tmp.path(), r"Sound\fx\boom.wav").expect("should resolve to .wav");
        assert_eq!(key, "sound/fx/boom.wav");
    }

    #[test]
    fn resolve_source_audio_returns_none_when_neither_extension_exists() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(resolve_source_audio(tmp.path(), r"data\Music\missing.wav").is_none());
    }

    // -----------------------------------------------------------------
    // GUID codec — self-inverse of wwise::encode::guid_string_to_bytes
    // -----------------------------------------------------------------

    #[test]
    fn canonical_guid_string_round_trips_through_wwise_encode() {
        let bytes: [u8; 16] = [
            0xF8, 0xFF, 0x40, 0x76, 0xA2, 0xE9, 0xFB, 0x73, 0xAF, 0xE8, 0xC0, 0xFF, 0x75, 0x14,
            0x8A, 0xF7,
        ];
        let guid = canonical_guid_string(bytes);
        let round_tripped = wwise::encode::guid_string_to_bytes(&guid).unwrap();
        assert_eq!(round_tripped, bytes);
    }

    // -----------------------------------------------------------------
    // AudioRef extraction from a decoded Record (no plugin handle needed)
    // -----------------------------------------------------------------

    #[test]
    fn audio_refs_from_must_reads_anam_and_bnam() {
        let interner = StringInterner::new();
        let plugin = interner.intern("Fallout4.esm");
        let mut record = Record::new(
            SigCode::from_str("MUST").unwrap(),
            FormKey {
                local: 0x0217A3,
                plugin,
            },
        );
        let eid = interner.intern("MUSGenesisCombatHigh03Track");
        record.eid = Some(eid);
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("ANAM").unwrap(),
            value: FieldValue::String(interner.intern(r"data\Music\Track.wav")),
        });
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("BNAM").unwrap(),
            value: FieldValue::String(interner.intern(r"data\Music\Track_Finale.wav")),
        });

        let refs = audio_refs_from_must(&record, &interner);
        assert_eq!(refs.len(), 2);
        assert_eq!(refs[0].role, AudioRole::MusicStart);
        assert_eq!(refs[0].source_rel_path, r"data\Music\Track.wav");
        assert_eq!(refs[1].role, AudioRole::MusicStop);
        assert_eq!(refs[1].fo4_editor_id, "MUSGenesisCombatHigh03Track");
    }

    // -----------------------------------------------------------------
    // Manifest generation from a stubbed WwiseBankOut
    // -----------------------------------------------------------------

    fn is_valid_wwed_eid(eid: &str) -> bool {
        eid.starts_with("FO4SF_WWED_")
            && eid
                .chars()
                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
    }

    #[test]
    fn manifest_from_stubbed_bank_out_has_one_row_per_input_and_valid_wwed_eids() {
        let interner = StringInterner::new();
        let plugin = interner.intern("Fallout4.esm");
        let music_fk = FormKey {
            local: 0x1000,
            plugin,
        };
        let amb_fk = FormKey {
            local: 0x2000,
            plugin,
        };

        let resolved = vec![
            (
                AudioRef {
                    fo4_form_key: music_fk,
                    fo4_editor_id: "MusicTrackA".to_string(),
                    role: AudioRole::MusicStart,
                    source_rel_path: "music/a.xwm".to_string(),
                },
                "music/a.xwm".to_string(),
            ),
            (
                AudioRef {
                    fo4_form_key: music_fk,
                    fo4_editor_id: "MusicTrackA".to_string(),
                    role: AudioRole::MusicStop,
                    source_rel_path: "music/a_finale.xwm".to_string(),
                },
                "music/a_finale.xwm".to_string(),
            ),
            (
                AudioRef {
                    fo4_form_key: amb_fk,
                    fo4_editor_id: "AmbSpace".to_string(),
                    role: AudioRole::AmbienceLoop,
                    source_rel_path: "sound/fx/amb.xwm".to_string(),
                },
                "sound/fx/amb.xwm".to_string(),
            ),
        ];

        let bank_out = WwiseBankOut {
            bank_files: vec![],
            events: vec![
                WwiseEvent {
                    event_name: "FO4SF_MUSIC_A_XWM".to_string(),
                    event_guid: [1u8; 16],
                    source_rel_path: "music/a.xwm".to_string(),
                    media_id: 1,
                },
                WwiseEvent {
                    event_name: "FO4SF_MUSIC_A_FINALE_XWM".to_string(),
                    event_guid: [2u8; 16],
                    source_rel_path: "music/a_finale.xwm".to_string(),
                    media_id: 2,
                },
                WwiseEvent {
                    event_name: "FO4SF_SOUND_FX_AMB_XWM".to_string(),
                    event_guid: [3u8; 16],
                    source_rel_path: "sound/fx/amb.xwm".to_string(),
                    media_id: 3,
                },
            ],
        };

        let joined = join_refs_with_events(&resolved, &bank_out);
        assert_eq!(joined.len(), 3, "one joined ref per input");

        let groups = group_joined_refs(joined);
        assert_eq!(
            groups.len(),
            2,
            "music pair collapses to 1 group, ambience is its own group"
        );

        let mut next_local = 0x900u32;
        let mut allocate = |_eid: &str| -> FormKey {
            let fk = FormKey {
                local: next_local,
                plugin,
            };
            next_local += 1;
            fk
        };
        let plan = build_wwed_plan(&groups, &mut allocate);
        assert_eq!(plan.len(), 2, "one WWED per group");

        for entry in &plan {
            assert!(
                is_valid_wwed_eid(&entry.wwed_editor_id),
                "bad WWED EditorID: {}",
                entry.wwed_editor_id
            );
        }

        let manifest = plan_to_manifest_json(&plan, &interner);
        let rows = manifest.as_array().expect("manifest is a JSON array");
        assert_eq!(rows.len(), 3, "one manifest row per input");

        // Re-parse to prove it's valid JSON end-to-end.
        let text = serde_json::to_string(&manifest).unwrap();
        let reparsed: JsonValue = serde_json::from_str(&text).unwrap();
        assert_eq!(reparsed.as_array().unwrap().len(), 3);

        // The music pair shares one WWED identity across both rows.
        let music_rows: Vec<&JsonValue> = rows
            .iter()
            .filter(|r| r["fo4_form_key"] == "001000@Fallout4.esm")
            .collect();
        assert_eq!(music_rows.len(), 2);
        assert_eq!(
            music_rows[0]["wwed_editor_id"],
            music_rows[1]["wwed_editor_id"]
        );
        assert_eq!(
            music_rows[0]["wwed_local_formid"],
            music_rows[1]["wwed_local_formid"]
        );
        let roles: Vec<&str> = music_rows
            .iter()
            .map(|r| r["role"].as_str().unwrap())
            .collect();
        assert!(roles.contains(&"music_start"));
        assert!(roles.contains(&"music_stop"));

        let amb_row = rows
            .iter()
            .find(|r| r["fo4_form_key"] == "002000@Fallout4.esm")
            .expect("ambience row present");
        assert_eq!(amb_row["role"], "ambience_loop");
        assert_eq!(
            amb_row["event_guid"].as_str().unwrap().matches('-').count(),
            4
        );
    }

    // -----------------------------------------------------------------
    // Placeholder mode
    // -----------------------------------------------------------------

    #[test]
    fn placeholder_manifest_events_all_come_from_the_vanilla_target_list_and_writes_no_bank_files()
    {
        use crate::run::{RunConfig, RunError, RunParams, create_run, drop_run, with_run};
        use crate::translator::Game;
        use std::sync::atomic::AtomicBool;

        let tmp = tempfile::tempdir().unwrap();
        let mod_path = tmp.path().join("mod");
        let source = tmp.path().join("source");

        let id = create_run(RunParams {
            source: Game::Fo4,
            target: Game::Fo4,
            source_handle_id: 9999,
            target_handle_id: 9998,
            master_handle_ids: vec![],
            config: RunConfig {
                output_plugin_name: "Fallout4_SF.esm".into(),
                ..Default::default()
            },
        })
        .unwrap();

        let params = json!({ "placeholder_audio": true });

        let report = with_run(id, |run| -> Result<PhaseReport, RunError> {
            let cancel = AtomicBool::new(false);
            let mut ctx = PhaseCtx {
                run,
                mod_path: &mod_path,
                source_extracted_dir: &source,
                target_extracted_dir: None,
                target_data_dir: None,
                params: &params,
                cancel: &cancel,
            };
            WwiseAudioPhase
                .run(&mut ctx)
                .map_err(|e| RunError::InvalidConfig(e.to_string()))
        })
        .unwrap();

        assert_eq!(report.records_added, 0);
        assert_eq!(
            report.assets_written, 0,
            "placeholder mode writes no bank files"
        );
        assert!(!mod_path.join("data/sound/soundbanks").exists());

        let manifest_text =
            std::fs::read_to_string(mod_path.join("debug/wwise/events_manifest.json")).unwrap();
        let manifest: JsonValue = serde_json::from_str(&manifest_text).unwrap();
        assert_eq!(manifest["mode"], "placeholder");
        let targets = manifest["targets"].as_array().unwrap();
        assert_eq!(targets.len(), PLACEHOLDER_TARGETS.len());

        let vanilla_form_ids: std::collections::HashSet<&str> =
            PLACEHOLDER_TARGETS.iter().map(|t| t.form_id).collect();
        for target in targets {
            let form_id = target["form_id"].as_str().unwrap();
            assert!(
                vanilla_form_ids.contains(form_id),
                "every placeholder target must come from the R2 §9 R-8 vanilla list: {form_id}"
            );
        }
        // Spot-check the exact FormIDs from the dispatch contract.
        let form_ids: Vec<&str> = targets
            .iter()
            .map(|t| t["form_id"].as_str().unwrap())
            .collect();
        for expected in [
            "00C5D2", "013D5F", "010DFE", "1804D6", "13E4E7", "0013FA", "0C5B6E",
        ] {
            assert!(form_ids.contains(&expected), "missing {expected}");
        }

        drop_run(id).unwrap();
    }

    #[test]
    fn bank_name_strips_esm_extension() {
        assert_eq!(
            bank_name_from_output_plugin("Fallout4_SF.esm"),
            "Fallout4_SF"
        );
    }

    #[test]
    fn is_placeholder_accepts_either_param_name() {
        assert!(is_placeholder(&json!({ "placeholder_audio": true })));
        assert!(is_placeholder(&json!({ "placeholder_events": true })));
        assert!(!is_placeholder(&json!({})));
        assert!(!is_placeholder(&json!({ "placeholder_audio": false })));
    }
}
