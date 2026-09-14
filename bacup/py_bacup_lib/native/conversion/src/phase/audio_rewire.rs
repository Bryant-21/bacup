//! `audio_rewire`: patches the inline Wwise-event content refs on records
//! `wwise_audio` already translated, and materializes the Starfield-only audio
//! defaults the translation map can't emit (its `defaults:` key is parsed but
//! never applied; see `translator/maps.rs`'s `ignored_top_level_key` lint).
//! Record layouts: `bacup/docs/starfield_target/R2-audio-records.md`.
//!
//! `wwise_audio::run_normal` inserts every synthesized `WWED` (EDID + WSED +
//! optional WTED) and writes `debug/wwise/events_manifest.json`: an array of
//! `{fo4_form_key, fo4_editor_id, role, source_rel, event, event_guid,
//! wwed_editor_id, wwed_local_formid}` rows in normal mode, or a
//! `{"mode":"placeholder", "targets":[...]}` object. This phase only patches
//! existing `MUST`/`ASPC` records; it never creates a `WWED`.
//!
//! # Join semantics
//!
//! A row's `fo4_form_key` is the FO4 source record that carried the audio ref:
//! - `music_start` / `music_stop`: the source `MUST` (from `MUST.ANAM`/`BNAM`).
//!   A start/stop pair sharing one key is one group.
//! - `ambience_loop`: the source `ASPC`, not the `SNDR` that
//!   `enumerate_audio_refs` reads the `Sound` path from. Always a solo group.
//!
//! Keys resolve to target FormKeys through the run's persistent `FormKeyMapper`
//! (populated by `translate`), never from paths or EditorIDs.
//!
//! # Rewrite targets
//!
//! Only `MUST.MTSH` and `ASPC.ASLS`. `ASPC.WED0`/`WED1` (interior/exterior
//! variants) stay zeroed: no FO4 field distinguishes the variants.
//! `fo4_to_starfield` never emits `SOUN`/`AMBS`, so `SOUN.SMLS`/`AMBS.ASAE`
//! never appear. `MUSC` and `REGN` have no content ref.
//!
//! `write_sound_event_set` replaces the whole 40-byte field with a new
//! `FieldValue::Bytes` buffer, so it doesn't matter whether the field arrived
//! as raw bytes or a decoded struct, or lost trailing zeros in an authoring
//! round-trip. `target_write::encode_value` passes `Bytes` through verbatim.
//!
//! # `FormOnlyStartForm` secondary
//!
//! Vanilla sets this FormID slot in 0 of 1111 Sound Event Sets; the inline
//! GUID is the proven mechanism. It is still written as a secondary, gated by
//! `WRITE_WWED_SECONDARY`. A WWED in the same output plugin uses
//! `target_master_names.len()` as its master index, matching the private
//! helpers in `target_write.rs` and `formkey_mapper.rs`.

use std::collections::HashMap;
use std::path::Path;

use serde_json::Value as JsonValue;
use smallvec::SmallVec;

use crate::formkey_mapper::FormKeyMapper;
use crate::ids::{FormKey, SigCode, SubrecordSig};
use crate::phase::{LogLevel, Phase, PhaseCtx, PhaseError, PhaseEvent, PhaseReport};
use crate::record::{FieldEntry, FieldValue, Record};
use crate::schema::AuthoringSchema;
use crate::source_read::{iter_form_keys_of_sig, read_record_relayout_by_form_key};
use crate::sym::StringInterner;
use crate::target_write::replace_records_native;
use crate::wwise::encode::guid_string_to_bytes;

/// Vanilla never sets the `FormOnlyStartForm` slot. `false` leaves it zero,
/// matching vanilla.
const WRITE_WWED_SECONDARY: bool = true;

const SOUND_EVENT_SET_LEN: usize = 40;

/// `ASDF` bit value 16 = `ExteriorWeatherAttenuation`.
const ASDF_EXTERIOR_WEATHER_ATTENUATION: u64 = 16;

fn sub(s: &str) -> SubrecordSig {
    SubrecordSig::from_str(s).expect("literal subrecord signature is always 4 ASCII bytes")
}

// ---------------------------------------------------------------------------
// materialize_audio_defaults: SF-mandatory fields the map's `defaults:` key
// cannot emit.
// ---------------------------------------------------------------------------

/// Ensures the SF-mandatory audio defaults are present on a translated
/// `MUSC`/`MUST`/`ASPC` record (no-op for every other signature). Returns the
/// (possibly unchanged) record plus whether anything was added/changed, so
/// callers can batch only the records that actually moved.
pub fn materialize_audio_defaults(mut record: Record) -> (Record, bool) {
    let changed = match record.sig.as_str() {
        "MUSC" => apply_musc_defaults(&mut record),
        "MUST" => apply_must_defaults(&mut record),
        "ASPC" => apply_aspc_defaults(&mut record),
        _ => false,
    };
    (record, changed)
}

fn ensure_uint_default(record: &mut Record, sig: SubrecordSig, default: u64) -> bool {
    if record.fields.iter().any(|f| f.sig == sig) {
        return false;
    }
    record.fields.push(FieldEntry {
        sig,
        value: FieldValue::Uint(default),
    });
    true
}

fn ensure_float_default(record: &mut Record, sig: SubrecordSig, default: f32) -> bool {
    if record.fields.iter().any(|f| f.sig == sig) {
        return false;
    }
    record.fields.push(FieldEntry {
        sig,
        value: FieldValue::Float(default),
    });
    true
}

fn ensure_bool_default(record: &mut Record, sig: SubrecordSig, default: bool) -> bool {
    if record.fields.iter().any(|f| f.sig == sig) {
        return false;
    }
    record.fields.push(FieldEntry {
        sig,
        value: FieldValue::Bool(default),
    });
    true
}

/// `MUSC.VNAM` (ReEvaluateInterval) + `MUSC.UNAM`
/// (StabilityInterval), both `uint64` ms, absent from FO4, carried by all 83
/// vanilla Starfield `MUSC` records.
fn apply_musc_defaults(record: &mut Record) -> bool {
    let mut changed = ensure_uint_default(record, sub("VNAM"), 0);
    changed |= ensure_uint_default(record, sub("UNAM"), 0);
    changed
}

/// `MUST.MSTF` (ConditionsCantFailAfterSuccess), `uint8` bool, Starfield-only.
fn apply_must_defaults(record: &mut Record) -> bool {
    ensure_bool_default(record, sub("MSTF"), false)
}

/// `ODTY`/`FLTV`/`BOLV`/`DEVT`/`ASDF` are present on every vanilla `ASPC` and
/// are defaulted when the map left them absent. `AEAR`
/// (ExteriorWeatherAttenuationRTPC) defaults to `1.0` because the map drops
/// FO4's non-equivalent `WNAM` (WeatherAttenuationDb, u16 dB). For the same
/// reason the `ASDF` `ExteriorWeatherAttenuation` bit (16) is forced on for
/// every ASPC, not only ones missing `ASDF`.
fn apply_aspc_defaults(record: &mut Record) -> bool {
    let mut changed = ensure_float_default(record, sub("ODTY"), 0.0);
    changed |= ensure_float_default(record, sub("FLTV"), 1.0);
    changed |= ensure_bool_default(record, sub("BOLV"), false);
    changed |= ensure_uint_default(record, sub("DEVT"), 0); // "None"
    changed |= ensure_float_default(record, sub("AEAR"), 1.0);

    let asdf = sub("ASDF");
    if let Some(entry) = record.fields.iter_mut().find(|f| f.sig == asdf) {
        if let FieldValue::Uint(v) = &mut entry.value {
            if *v & ASDF_EXTERIOR_WEATHER_ATTENUATION == 0 {
                *v |= ASDF_EXTERIOR_WEATHER_ATTENUATION;
                changed = true;
            }
        }
    } else {
        record.fields.push(FieldEntry {
            sig: asdf,
            value: FieldValue::Uint(ASDF_EXTERIOR_WEATHER_ATTENUATION),
        });
        changed = true;
    }
    changed
}

// ---------------------------------------------------------------------------
// Sound Event Set patching
// ---------------------------------------------------------------------------

/// Overwrites `field_sig` on `record` with a freshly built 40-byte Sound
/// Event Set; the old bytes are never read. Vanilla layout: `[0..16)` start
/// GUID, `[16..32)` stop GUID (zero if `stop_guid` is `None`), `[32..36)`
/// Condition FormID (always 0, no `CNDF` is synthesized), `[36..40)` `WWED`
/// FormID secondary (zero unless `wwed_raw_formid` is `Some`). Returns whether
/// the field's value changed.
fn write_sound_event_set(
    record: &mut Record,
    field_sig: SubrecordSig,
    start_guid: [u8; 16],
    stop_guid: Option<[u8; 16]>,
    wwed_raw_formid: Option<u32>,
) -> bool {
    let mut buf = [0u8; SOUND_EVENT_SET_LEN];
    buf[0..16].copy_from_slice(&start_guid);
    if let Some(stop) = stop_guid {
        buf[16..32].copy_from_slice(&stop);
    }
    if let Some(raw) = wwed_raw_formid {
        buf[36..40].copy_from_slice(&raw.to_le_bytes());
    }
    let new_value = FieldValue::Bytes(SmallVec::from_slice(&buf));

    if let Some(entry) = record.fields.iter_mut().find(|f| f.sig == field_sig) {
        if entry.value == new_value {
            return false;
        }
        entry.value = new_value;
    } else {
        record.fields.push(FieldEntry {
            sig: field_sig,
            value: new_value,
        });
    }
    true
}

// ---------------------------------------------------------------------------
// Small Record field helpers (local copies — wwise_audio.rs's are private to
// that module and this phase must not touch that file)
// ---------------------------------------------------------------------------

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

fn form_key_list_field(record: &Record, sig: &str) -> Vec<FormKey> {
    record
        .fields
        .iter()
        .find(|f| f.sig.as_str() == sig)
        .map(|f| match &f.value {
            FieldValue::List(items) => items
                .iter()
                .filter_map(|v| match v {
                    FieldValue::FormKey(fk) => Some(*fk),
                    _ => None,
                })
                .collect(),
            FieldValue::FormKey(fk) => vec![*fk],
            _ => Vec::new(),
        })
        .unwrap_or_default()
}

fn set_form_key_field(record: &mut Record, sig: SubrecordSig, target: FormKey) {
    if let Some(entry) = record.fields.iter_mut().find(|f| f.sig == sig) {
        entry.value = FieldValue::FormKey(target);
    } else {
        record.fields.push(FieldEntry {
            sig,
            value: FieldValue::FormKey(target),
        });
    }
}

// ---------------------------------------------------------------------------
// Manifest model (schema in wwise_audio.rs's module doc comment)
// ---------------------------------------------------------------------------

struct ManifestRow {
    fo4_form_key: String,
    fo4_editor_id: String,
    role: String,
    source_rel: String,
    event_guid: String,
    wwed_local_formid: String,
}

struct PlaceholderTarget {
    purpose: String,
    form_id: String,
}

fn load_manifest(mod_path: &Path) -> Result<JsonValue, PhaseError> {
    let path = mod_path
        .join("debug")
        .join("wwise")
        .join("events_manifest.json");
    let text = std::fs::read_to_string(&path).map_err(|e| {
        PhaseError::Internal(format!(
            "audio_rewire: cannot read manifest at {} ({e}); audio_rewire must run after wwise_audio",
            path.display()
        ))
    })?;
    serde_json::from_str(&text).map_err(|e| {
        PhaseError::Internal(format!(
            "audio_rewire: manifest at {} is not valid JSON: {e}",
            path.display()
        ))
    })
}

fn is_placeholder_manifest(manifest: &JsonValue) -> bool {
    manifest.get("mode").and_then(JsonValue::as_str) == Some("placeholder")
}

fn json_str_field(row: &JsonValue, key: &str) -> Result<String, PhaseError> {
    row.get(key)
        .and_then(JsonValue::as_str)
        .map(str::to_string)
        .ok_or_else(|| {
            PhaseError::Internal(format!("audio_rewire: manifest row missing {key:?}: {row}"))
        })
}

fn parse_normal_rows(manifest: &JsonValue) -> Result<Vec<ManifestRow>, PhaseError> {
    let rows = manifest.as_array().ok_or_else(|| {
        PhaseError::Internal("audio_rewire: normal-mode manifest must be a JSON array".into())
    })?;
    rows.iter()
        .map(|row| {
            Ok(ManifestRow {
                fo4_form_key: json_str_field(row, "fo4_form_key")?,
                fo4_editor_id: json_str_field(row, "fo4_editor_id")?,
                role: json_str_field(row, "role")?,
                source_rel: json_str_field(row, "source_rel")?,
                event_guid: json_str_field(row, "event_guid")?,
                wwed_local_formid: json_str_field(row, "wwed_local_formid")?,
            })
        })
        .collect()
}

fn parse_placeholder_targets(manifest: &JsonValue) -> Result<Vec<PlaceholderTarget>, PhaseError> {
    let targets = manifest
        .get("targets")
        .and_then(JsonValue::as_array)
        .ok_or_else(|| {
            PhaseError::Internal(
                "audio_rewire: placeholder manifest missing 'targets' array".into(),
            )
        })?;
    targets
        .iter()
        .map(|t| {
            Ok(PlaceholderTarget {
                purpose: json_str_field(t, "purpose")?,
                form_id: json_str_field(t, "form_id")?,
            })
        })
        .collect()
}

fn parse_hex_formid(s: &str) -> Result<u32, PhaseError> {
    let hex = s.strip_prefix("0x").unwrap_or(s);
    u32::from_str_radix(hex, 16)
        .map_err(|e| PhaseError::Internal(format!("audio_rewire: bad hex FormID {s:?}: {e}")))
}

// ---------------------------------------------------------------------------
// Grouping: mirrors wwise_audio.rs's AudioGroup, rebuilt from the manifest
// rather than from live source records.
// ---------------------------------------------------------------------------

struct AudioGroupPlan {
    fo4_form_key: String,
    is_ambience: bool,
    start_guid: [u8; 16],
    stop_guid: Option<[u8; 16]>,
    wwed_local_formid: Option<u32>,
    sample_source_rel: String,
    sample_editor_id: String,
}

#[derive(Default)]
struct GroupBuilder {
    is_ambience: bool,
    start: Option<[u8; 16]>,
    stop: Option<[u8; 16]>,
    wwed_local_formid: Option<u32>,
    sample_source_rel: String,
    sample_editor_id: String,
}

/// Groups manifest rows by `fo4_form_key` (a `music_start`/`music_stop` pair
/// collapses to one group; `ambience_loop` rows are always solo). Also returns
/// messages for rows that can't be grouped: an unknown role, or a group with
/// no start row.
fn group_manifest_rows(
    rows: &[ManifestRow],
) -> Result<(Vec<AudioGroupPlan>, Vec<String>), PhaseError> {
    let mut order: Vec<String> = Vec::new();
    let mut builders: HashMap<String, GroupBuilder> = HashMap::new();

    for row in rows {
        let guid = guid_string_to_bytes(&row.event_guid).map_err(|e| {
            PhaseError::Internal(format!(
                "audio_rewire: bad event_guid {:?} for fo4_form_key={}: {e}",
                row.event_guid, row.fo4_form_key
            ))
        })?;
        let wwed_local = parse_hex_formid(&row.wwed_local_formid)?;

        if !builders.contains_key(&row.fo4_form_key) {
            order.push(row.fo4_form_key.clone());
        }
        let b = builders.entry(row.fo4_form_key.clone()).or_default();
        b.wwed_local_formid = Some(wwed_local);
        b.sample_source_rel = row.source_rel.clone();
        b.sample_editor_id = row.fo4_editor_id.clone();
        match row.role.as_str() {
            "music_start" => {
                b.start = Some(guid);
                b.is_ambience = false;
            }
            "ambience_loop" => {
                b.start = Some(guid);
                b.is_ambience = true;
            }
            "music_stop" => {
                b.stop = Some(guid);
            }
            other => {
                return Err(PhaseError::Internal(format!(
                    "audio_rewire: unknown manifest role {other:?} for fo4_form_key={}",
                    row.fo4_form_key
                )));
            }
        }
    }

    let mut plans = Vec::new();
    let mut malformed = Vec::new();
    for key in order {
        let b = builders
            .remove(&key)
            .expect("key was just inserted into builders");
        match b.start {
            Some(start) => plans.push(AudioGroupPlan {
                fo4_form_key: key,
                is_ambience: b.is_ambience,
                start_guid: start,
                stop_guid: b.stop,
                wwed_local_formid: b.wwed_local_formid,
                sample_source_rel: b.sample_source_rel,
                sample_editor_id: b.sample_editor_id,
            }),
            None => malformed.push(format!(
                "manifest group fo4_form_key={key} has no start row (source_rel={})",
                b.sample_source_rel
            )),
        }
    }
    Ok((plans, malformed))
}

// ---------------------------------------------------------------------------
// Content-ref rewire pass (MUST.MTSH / ASPC.ASLS)
// ---------------------------------------------------------------------------

fn rewire_content_refs(
    target_handle: u64,
    schema_target: &AuthoringSchema,
    interner: &StringInterner,
    mapper: &FormKeyMapper<'_>,
    groups: &[AudioGroupPlan],
    own_index: u32,
) -> Result<(u32, Vec<String>), PhaseError> {
    let mtsh_sig = sub("MTSH");
    let asls_sig = sub("ASLS");

    let mut changed_records = Vec::new();
    let mut unmatched = Vec::new();

    for plan in groups {
        let source_fk = match FormKey::parse(&plan.fo4_form_key, interner) {
            Ok(fk) => fk,
            Err(e) => {
                unmatched.push(format!(
                    "bad fo4_form_key {:?} in manifest (source_rel={}): {e}",
                    plan.fo4_form_key, plan.sample_source_rel
                ));
                continue;
            }
        };
        let Some(target_fk) = mapper.lookup(source_fk) else {
            unmatched.push(format!(
                "no target FormKey mapping for fo4_form_key={} editor_id={} source_rel={}",
                plan.fo4_form_key, plan.sample_editor_id, plan.sample_source_rel
            ));
            continue;
        };
        let Ok(mut record) = read_record_relayout_by_form_key(
            target_handle,
            &target_fk,
            schema_target,
            interner,
            None,
        ) else {
            unmatched.push(format!(
                "target record {} not found for fo4_form_key={} source_rel={}",
                target_fk.format(interner),
                plan.fo4_form_key,
                plan.sample_source_rel
            ));
            continue;
        };

        let field_sig = match (record.sig.as_str(), plan.is_ambience) {
            ("MUST", false) => mtsh_sig,
            ("ASPC", true) => asls_sig,
            (other, _) => {
                unmatched.push(format!(
                    "target record {} has unexpected signature {other} for fo4_form_key={}",
                    target_fk.format(interner),
                    plan.fo4_form_key
                ));
                continue;
            }
        };

        let wwed_raw = if WRITE_WWED_SECONDARY {
            plan.wwed_local_formid
                .map(|local| (own_index << 24) | (local & 0x00FF_FFFF))
        } else {
            None
        };

        if write_sound_event_set(
            &mut record,
            field_sig,
            plan.start_guid,
            plan.stop_guid,
            wwed_raw,
        ) {
            changed_records.push(record);
        }
    }

    let n = changed_records.len() as u32;
    if !changed_records.is_empty() {
        replace_records_native(target_handle, changed_records, schema_target, interner)
            .map_err(|e| PhaseError::Internal(e.to_string()))?;
    }
    Ok((n, unmatched))
}

// ---------------------------------------------------------------------------
// materialize_audio_defaults pass — every translated MUSC/MUST/ASPC
// ---------------------------------------------------------------------------

fn materialize_defaults_pass(
    target_handle: u64,
    schema_target: &AuthoringSchema,
    interner: &StringInterner,
) -> Result<u32, PhaseError> {
    let mut changed_records = Vec::new();
    for sig_str in ["MUSC", "MUST", "ASPC"] {
        let sig = SigCode::from_str(sig_str).map_err(PhaseError::Internal)?;
        let form_keys = iter_form_keys_of_sig(target_handle, sig, interner)
            .map_err(|e| PhaseError::Internal(e.to_string()))?;
        for fk in form_keys {
            let record =
                read_record_relayout_by_form_key(target_handle, &fk, schema_target, interner, None)
                    .map_err(|e| PhaseError::Internal(e.to_string()))?;
            let (updated, changed) = materialize_audio_defaults(record);
            if changed {
                changed_records.push(updated);
            }
        }
    }
    let n = changed_records.len() as u32;
    if !changed_records.is_empty() {
        replace_records_native(target_handle, changed_records, schema_target, interner)
            .map_err(|e| PhaseError::Internal(e.to_string()))?;
    }
    Ok(n)
}

// ---------------------------------------------------------------------------
// REGN.RDMO compensation: SF REGN has no music subrecord, so the region's MUSC
// is pushed onto the exterior CELLs that list it in CELL.XCLR ("Regions"; no
// spatial polygon test). Never overrides a CELL's own direct XCMO.
// ---------------------------------------------------------------------------

fn regn_music_compensation(
    source_handle: u64,
    target_handle: u64,
    schema_source: &AuthoringSchema,
    schema_target: &AuthoringSchema,
    interner: &StringInterner,
    mapper: &FormKeyMapper<'_>,
) -> Result<(u32, Vec<String>), PhaseError> {
    let cell_sig = SigCode::from_str("CELL").map_err(PhaseError::Internal)?;
    let xcmo_sig = sub("XCMO");

    let mut changed_records = Vec::new();
    let mut unmatched = Vec::new();

    let source_cells = iter_form_keys_of_sig(source_handle, cell_sig, interner)
        .map_err(|e| PhaseError::Internal(e.to_string()))?;

    for source_cell_fk in source_cells {
        let Ok(source_cell) = read_record_relayout_by_form_key(
            source_handle,
            &source_cell_fk,
            schema_source,
            interner,
            None,
        ) else {
            continue;
        };
        let regions = form_key_list_field(&source_cell, "XCLR");
        if regions.is_empty() {
            continue;
        }

        let music_fk = regions.iter().find_map(|region_fk| {
            if region_fk.local == 0 {
                return None;
            }
            let region = read_record_relayout_by_form_key(
                source_handle,
                region_fk,
                schema_source,
                interner,
                None,
            )
            .ok()?;
            form_key_field(&region, "RDMO").filter(|fk| fk.local != 0)
        });
        let Some(music_fk) = music_fk else { continue };

        let Some(target_music_fk) = mapper.lookup(music_fk) else {
            unmatched.push(format!(
                "REGN.RDMO -> MUSC {} has no target mapping (compensating cell {})",
                music_fk.format(interner),
                source_cell_fk.format(interner)
            ));
            continue;
        };
        // The cell itself may not have been carried (excluded/dropped) —
        // nothing to compensate in that case, not an error.
        let Some(target_cell_fk) = mapper.lookup(source_cell_fk) else {
            continue;
        };

        let Ok(mut target_cell) = read_record_relayout_by_form_key(
            target_handle,
            &target_cell_fk,
            schema_target,
            interner,
            None,
        ) else {
            continue;
        };

        let has_direct_xcmo = form_key_field(&target_cell, "XCMO")
            .map(|fk| fk.local != 0)
            .unwrap_or(false);
        if has_direct_xcmo {
            continue;
        }

        set_form_key_field(&mut target_cell, xcmo_sig, target_music_fk);
        changed_records.push(target_cell);
    }

    let n = changed_records.len() as u32;
    if !changed_records.is_empty() {
        replace_records_native(target_handle, changed_records, schema_target, interner)
            .map_err(|e| PhaseError::Internal(e.to_string()))?;
    }
    Ok((n, unmatched))
}

// ---------------------------------------------------------------------------
// Phase
// ---------------------------------------------------------------------------

pub struct AudioRewirePhase;

impl Phase for AudioRewirePhase {
    fn name(&self) -> &'static str {
        "audio_rewire"
    }

    fn run(&self, ctx: &mut PhaseCtx<'_>) -> Result<PhaseReport, PhaseError> {
        let manifest = load_manifest(ctx.mod_path)?;
        if is_placeholder_manifest(&manifest) {
            run_placeholder(ctx, &manifest)
        } else {
            run_normal(ctx, &manifest)
        }
    }
}

fn run_normal(ctx: &mut PhaseCtx<'_>, manifest: &JsonValue) -> Result<PhaseReport, PhaseError> {
    ctx.check_cancel()?;
    let rows = parse_normal_rows(manifest)?;
    let (groups, mut unmatched_messages) = group_manifest_rows(&rows)?;

    let target_handle = ctx.run.target_handle_id;
    let source_handle = ctx.run.source_handle_id;
    let schema_target = ctx.run.schema_target.clone();
    let schema_source = ctx.run.schema_source.clone();
    // Same "own file" master-index convention as target_write.rs's private
    // `own_index` / formkey_mapper.rs's private `raw_formid_for_target` — see
    // the module doc comment.
    let own_index = (ctx.run.config.target_master_names.len() as u32) & 0xFF;

    let defaults_changed =
        materialize_defaults_pass(target_handle, &schema_target, &ctx.run.interner)?;
    ctx.check_cancel()?;

    let state = ctx.run.mapper_state.as_mut().ok_or_else(|| {
        PhaseError::Internal(
            "audio_rewire: mapper_state not initialized; must run after translate".into(),
        )
    })?;
    let mapper = FormKeyMapper::from_state(state, &ctx.run.interner);

    let (content_changed, mut content_unmatched) = rewire_content_refs(
        target_handle,
        &schema_target,
        mapper.interner,
        &mapper,
        &groups,
        own_index,
    )?;
    unmatched_messages.append(&mut content_unmatched);

    let (regn_changed, mut regn_unmatched) = regn_music_compensation(
        source_handle,
        target_handle,
        &schema_source,
        &schema_target,
        mapper.interner,
        &mapper,
    )?;
    unmatched_messages.append(&mut regn_unmatched);

    for msg in &unmatched_messages {
        let _ = ctx.run.event_tx.try_send(PhaseEvent::Log {
            phase: "audio_rewire",
            level: LogLevel::Warn,
            message: format!("audio_rewire: {msg}"),
        });
    }

    let records_changed = defaults_changed + content_changed + regn_changed;
    let unmatched = unmatched_messages.len() as u32;

    let _ = ctx.run.event_tx.try_send(PhaseEvent::Log {
        phase: "audio_rewire",
        level: LogLevel::Info,
        message: format!(
            "audio_rewire: defaults_materialized={defaults_changed} content_ref_rewired={content_changed} \
             regn_compensated={regn_changed} unmatched={unmatched}"
        ),
    });

    Ok(PhaseReport {
        records_changed,
        warnings: unmatched,
        items_failed: unmatched,
        ..Default::default()
    })
}

/// Points `CELL.XCMO`/`WRLD.ZNAM`/`CELL.XCAS`/`ASPC.BNAM` at the manifest's
/// fixed vanilla targets instead of rewiring Sound Event Sets. Every
/// `MTSH`/`ASLS` stays zeroed as the map emitted it; a zeroed set is
/// vanilla-legal (SilentTrack behaviour) and cannot crash. Only fields that
/// already hold a non-zero FormKey are touched, so CELLs/WRLDs with no FO4
/// music stay silent.
fn run_placeholder(
    ctx: &mut PhaseCtx<'_>,
    manifest: &JsonValue,
) -> Result<PhaseReport, PhaseError> {
    ctx.check_cancel()?;
    let targets = parse_placeholder_targets(manifest)?;
    let starfield_esm = ctx.run.interner.intern("Starfield.esm");

    let mut by_purpose: HashMap<String, FormKey> = HashMap::new();
    for t in &targets {
        let local = parse_hex_formid(&t.form_id)?;
        by_purpose.insert(
            t.purpose.clone(),
            FormKey {
                local,
                plugin: starfield_esm,
            },
        );
    }
    let require = |purpose: &str| -> Result<FormKey, PhaseError> {
        by_purpose.get(purpose).copied().ok_or_else(|| {
            PhaseError::Internal(format!(
                "audio_rewire: placeholder manifest missing '{purpose}' target"
            ))
        })
    };
    let music_explore = require("music_explore")?;
    let ambient_interior = require("ambient_interior")?;
    let reverb_default = require("reverb_default")?;

    let target_handle = ctx.run.target_handle_id;
    let schema_target = ctx.run.schema_target.clone();

    let defaults_changed =
        materialize_defaults_pass(target_handle, &schema_target, &ctx.run.interner)?;
    ctx.check_cancel()?;

    let mut changed = defaults_changed;

    // CELL: XCMO -> music_explore, XCAS -> ambient_interior.
    let cell_sig = SigCode::from_str("CELL").map_err(PhaseError::Internal)?;
    let mut cell_changes = Vec::new();
    for fk in iter_form_keys_of_sig(target_handle, cell_sig, &ctx.run.interner)
        .map_err(|e| PhaseError::Internal(e.to_string()))?
    {
        let Ok(mut record) = read_record_relayout_by_form_key(
            target_handle,
            &fk,
            &schema_target,
            &ctx.run.interner,
            None,
        ) else {
            continue;
        };
        let mut touched = false;
        if form_key_field(&record, "XCMO")
            .map(|f| f.local != 0)
            .unwrap_or(false)
        {
            set_form_key_field(&mut record, sub("XCMO"), music_explore);
            touched = true;
        }
        if form_key_field(&record, "XCAS")
            .map(|f| f.local != 0)
            .unwrap_or(false)
        {
            set_form_key_field(&mut record, sub("XCAS"), ambient_interior);
            touched = true;
        }
        if touched {
            cell_changes.push(record);
        }
    }
    changed += cell_changes.len() as u32;
    if !cell_changes.is_empty() {
        replace_records_native(
            target_handle,
            cell_changes,
            &schema_target,
            &ctx.run.interner,
        )
        .map_err(|e| PhaseError::Internal(e.to_string()))?;
    }

    // WRLD: ZNAM -> music_explore.
    let wrld_sig = SigCode::from_str("WRLD").map_err(PhaseError::Internal)?;
    let mut wrld_changes = Vec::new();
    for fk in iter_form_keys_of_sig(target_handle, wrld_sig, &ctx.run.interner)
        .map_err(|e| PhaseError::Internal(e.to_string()))?
    {
        let Ok(mut record) = read_record_relayout_by_form_key(
            target_handle,
            &fk,
            &schema_target,
            &ctx.run.interner,
            None,
        ) else {
            continue;
        };
        if form_key_field(&record, "ZNAM")
            .map(|f| f.local != 0)
            .unwrap_or(false)
        {
            set_form_key_field(&mut record, sub("ZNAM"), music_explore);
            wrld_changes.push(record);
        }
    }
    changed += wrld_changes.len() as u32;
    if !wrld_changes.is_empty() {
        replace_records_native(
            target_handle,
            wrld_changes,
            &schema_target,
            &ctx.run.interner,
        )
        .map_err(|e| PhaseError::Internal(e.to_string()))?;
    }

    // ASPC: BNAM -> reverb_default.
    let aspc_sig = SigCode::from_str("ASPC").map_err(PhaseError::Internal)?;
    let mut aspc_changes = Vec::new();
    for fk in iter_form_keys_of_sig(target_handle, aspc_sig, &ctx.run.interner)
        .map_err(|e| PhaseError::Internal(e.to_string()))?
    {
        let Ok(mut record) = read_record_relayout_by_form_key(
            target_handle,
            &fk,
            &schema_target,
            &ctx.run.interner,
            None,
        ) else {
            continue;
        };
        if form_key_field(&record, "BNAM")
            .map(|f| f.local != 0)
            .unwrap_or(false)
        {
            set_form_key_field(&mut record, sub("BNAM"), reverb_default);
            aspc_changes.push(record);
        }
    }
    changed += aspc_changes.len() as u32;
    if !aspc_changes.is_empty() {
        replace_records_native(
            target_handle,
            aspc_changes,
            &schema_target,
            &ctx.run.interner,
        )
        .map_err(|e| PhaseError::Internal(e.to_string()))?;
    }

    let _ = ctx.run.event_tx.try_send(PhaseEvent::Log {
        phase: "audio_rewire",
        level: LogLevel::Info,
        message: format!(
            "audio_rewire: placeholder mode — defaults_materialized={defaults_changed} \
             repointed {changed} record(s) total to vanilla targets, no Sound Event Set touched"
        ),
    });

    Ok(PhaseReport {
        records_changed: changed,
        ..Default::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formkey_mapper::{MapperOptions, MapperState};
    use serde_json::json;

    fn record_with_fields(
        sig: &str,
        local: u32,
        plugin: crate::sym::Sym,
        fields: Vec<(&str, FieldValue)>,
    ) -> Record {
        let mut record = Record::new(SigCode::from_str(sig).unwrap(), FormKey { local, plugin });
        for (sig_str, value) in fields {
            record.fields.push(FieldEntry {
                sig: sub(sig_str),
                value,
            });
        }
        record
    }

    // -----------------------------------------------------------------
    // materialize_audio_defaults
    // -----------------------------------------------------------------

    #[test]
    fn musc_defaults_added_when_absent() {
        let interner = StringInterner::new();
        let plugin = interner.intern("Starfield.esm");
        let record = record_with_fields("MUSC", 0x1000, plugin, vec![]);

        let (updated, changed) = materialize_audio_defaults(record);
        assert!(changed);
        assert_eq!(
            updated
                .fields
                .iter()
                .find(|f| f.sig == sub("VNAM"))
                .map(|f| &f.value),
            Some(&FieldValue::Uint(0))
        );
        assert_eq!(
            updated
                .fields
                .iter()
                .find(|f| f.sig == sub("UNAM"))
                .map(|f| &f.value),
            Some(&FieldValue::Uint(0))
        );
    }

    #[test]
    fn musc_defaults_unchanged_when_already_present() {
        let interner = StringInterner::new();
        let plugin = interner.intern("Starfield.esm");
        let record = record_with_fields(
            "MUSC",
            0x1000,
            plugin,
            vec![
                ("VNAM", FieldValue::Uint(2000)),
                ("UNAM", FieldValue::Uint(6000)),
            ],
        );

        let (updated, changed) = materialize_audio_defaults(record);
        assert!(!changed);
        assert_eq!(
            updated
                .fields
                .iter()
                .find(|f| f.sig == sub("VNAM"))
                .map(|f| &f.value),
            Some(&FieldValue::Uint(2000))
        );
    }

    #[test]
    fn must_defaults_add_mstf_false_when_absent() {
        let interner = StringInterner::new();
        let plugin = interner.intern("Starfield.esm");
        let record = record_with_fields("MUST", 0x1000, plugin, vec![]);

        let (updated, changed) = materialize_audio_defaults(record);
        assert!(changed);
        assert_eq!(
            updated
                .fields
                .iter()
                .find(|f| f.sig == sub("MSTF"))
                .map(|f| &f.value),
            Some(&FieldValue::Bool(false))
        );
    }

    #[test]
    fn aspc_defaults_add_all_pinned_values_when_absent() {
        let interner = StringInterner::new();
        let plugin = interner.intern("Starfield.esm");
        let record = record_with_fields("ASPC", 0x1000, plugin, vec![]);

        let (updated, changed) = materialize_audio_defaults(record);
        assert!(changed);
        let get = |s: &str| {
            updated
                .fields
                .iter()
                .find(|f| f.sig == sub(s))
                .map(|f| &f.value)
        };
        assert_eq!(get("ODTY"), Some(&FieldValue::Float(0.0)));
        assert_eq!(get("FLTV"), Some(&FieldValue::Float(1.0)));
        assert_eq!(get("BOLV"), Some(&FieldValue::Bool(false)));
        assert_eq!(get("DEVT"), Some(&FieldValue::Uint(0)));
        assert_eq!(get("AEAR"), Some(&FieldValue::Float(1.0)));
        assert_eq!(get("ASDF"), Some(&FieldValue::Uint(16)));
    }

    #[test]
    fn aspc_defaults_unchanged_when_already_present_and_asdf_bit_already_set() {
        let interner = StringInterner::new();
        let plugin = interner.intern("Starfield.esm");
        let record = record_with_fields(
            "ASPC",
            0x1000,
            plugin,
            vec![
                ("ODTY", FieldValue::Float(0.5)),
                ("FLTV", FieldValue::Float(0.8)),
                ("BOLV", FieldValue::Bool(true)),
                ("DEVT", FieldValue::Uint(3)),
                ("AEAR", FieldValue::Float(1.0)),
                ("ASDF", FieldValue::Uint(16)),
            ],
        );

        let (updated, changed) = materialize_audio_defaults(record);
        assert!(!changed);
        assert_eq!(
            updated
                .fields
                .iter()
                .find(|f| f.sig == sub("ODTY"))
                .map(|f| &f.value),
            Some(&FieldValue::Float(0.5))
        );
    }

    #[test]
    fn aspc_asdf_exterior_weather_bit_forced_on_even_when_asdf_already_present() {
        let interner = StringInterner::new();
        let plugin = interner.intern("Starfield.esm");
        // ASDF present (bit 1 = EnvironmentType) but WITHOUT bit 16 set.
        let record =
            record_with_fields("ASPC", 0x1000, plugin, vec![("ASDF", FieldValue::Uint(1))]);

        let (updated, changed) = materialize_audio_defaults(record);
        assert!(changed);
        let asdf = updated
            .fields
            .iter()
            .find(|f| f.sig == sub("ASDF"))
            .unwrap();
        assert_eq!(asdf.value, FieldValue::Uint(1 | 16));
    }

    #[test]
    fn non_audio_signature_is_untouched() {
        let interner = StringInterner::new();
        let plugin = interner.intern("Starfield.esm");
        let record = record_with_fields("STAT", 0x1000, plugin, vec![]);
        let (updated, changed) = materialize_audio_defaults(record);
        assert!(!changed);
        assert!(updated.fields.is_empty());
    }

    // -----------------------------------------------------------------
    // write_sound_event_set
    // -----------------------------------------------------------------

    #[test]
    fn write_sound_event_set_builds_full_40_bytes_with_start_stop_and_wwed_secondary() {
        let interner = StringInterner::new();
        let plugin = interner.intern("Starfield.esm");
        let mut record = record_with_fields("MUST", 0x1000, plugin, vec![]);

        let start = [0xAAu8; 16];
        let stop = [0xBBu8; 16];
        let changed = write_sound_event_set(
            &mut record,
            sub("MTSH"),
            start,
            Some(stop),
            Some(0x01_000800),
        );
        assert!(changed);

        let entry = record.fields.iter().find(|f| f.sig == sub("MTSH")).unwrap();
        let FieldValue::Bytes(bytes) = &entry.value else {
            panic!("expected FieldValue::Bytes, got {:?}", entry.value);
        };
        assert_eq!(bytes.len(), 40, "R-4: must always emit the full 40 bytes");
        assert_eq!(&bytes[0..16], &start[..]);
        assert_eq!(&bytes[16..32], &stop[..]);
        assert_eq!(
            &bytes[32..36],
            &[0, 0, 0, 0],
            "Condition FormID always 0 (no CNDF synthesis)"
        );
        assert_eq!(&bytes[36..40], &0x01_000800u32.to_le_bytes());
    }

    #[test]
    fn write_sound_event_set_zeros_stop_and_wwed_when_absent() {
        let interner = StringInterner::new();
        let plugin = interner.intern("Starfield.esm");
        let mut record = record_with_fields("ASPC", 0x1000, plugin, vec![]);

        let start = [0x11u8; 16];
        write_sound_event_set(&mut record, sub("ASLS"), start, None, None);

        let entry = record.fields.iter().find(|f| f.sig == sub("ASLS")).unwrap();
        let FieldValue::Bytes(bytes) = &entry.value else {
            panic!("expected Bytes");
        };
        assert_eq!(&bytes[16..40], &[0u8; 24][..]);
    }

    #[test]
    fn write_sound_event_set_overwrites_existing_raw_bytes_field_fully() {
        // Field arrived as raw Bytes of another length: still a full overwrite.
        let interner = StringInterner::new();
        let plugin = interner.intern("Starfield.esm");
        let mut record = record_with_fields(
            "MUST",
            0x1000,
            plugin,
            vec![(
                "MTSH",
                FieldValue::Bytes(SmallVec::from_slice(&[0xFFu8; 16])),
            )], // trailing zeros trimmed to 16 bytes
        );

        let start = [0x22u8; 16];
        let changed = write_sound_event_set(&mut record, sub("MTSH"), start, None, None);
        assert!(changed);
        let entry = record.fields.iter().find(|f| f.sig == sub("MTSH")).unwrap();
        let FieldValue::Bytes(bytes) = &entry.value else {
            panic!("expected Bytes");
        };
        assert_eq!(bytes.len(), 40);
        assert_eq!(&bytes[0..16], &start[..]);
    }

    #[test]
    fn write_sound_event_set_is_idempotent_when_value_unchanged() {
        let interner = StringInterner::new();
        let plugin = interner.intern("Starfield.esm");
        let mut record = record_with_fields("MUST", 0x1000, plugin, vec![]);
        let start = [0x33u8; 16];
        assert!(write_sound_event_set(
            &mut record,
            sub("MTSH"),
            start,
            None,
            None
        ));
        assert!(!write_sound_event_set(
            &mut record,
            sub("MTSH"),
            start,
            None,
            None
        ));
    }

    // -----------------------------------------------------------------
    // Manifest parsing / grouping
    // -----------------------------------------------------------------

    #[test]
    fn group_manifest_rows_collapses_music_start_stop_pair() {
        let rows = vec![
            ManifestRow {
                fo4_form_key: "0217A3@Fallout4.esm".into(),
                fo4_editor_id: "MUSGenesisCombatHigh03Track".into(),
                role: "music_start".into(),
                source_rel: "music/a.xwm".into(),
                event_guid: "7376FFF8-E9A2-40FB-AFE8-C0FF75148AF7".into(),
                wwed_local_formid: "0x000800".into(),
            },
            ManifestRow {
                fo4_form_key: "0217A3@Fallout4.esm".into(),
                fo4_editor_id: "MUSGenesisCombatHigh03Track".into(),
                role: "music_stop".into(),
                source_rel: "music/a_finale.xwm".into(),
                event_guid: "CFCCC7D7-D5C3-475D-8C84-DBC1309F61FC".into(),
                wwed_local_formid: "0x000800".into(),
            },
        ];
        let (groups, malformed) = group_manifest_rows(&rows).unwrap();
        assert!(malformed.is_empty());
        assert_eq!(groups.len(), 1);
        assert!(!groups[0].is_ambience);
        assert!(groups[0].stop_guid.is_some());
        assert_eq!(groups[0].wwed_local_formid, Some(0x000800));
    }

    #[test]
    fn group_manifest_rows_solo_ambience_loop() {
        let rows = vec![ManifestRow {
            fo4_form_key: "002000@Fallout4.esm".into(),
            fo4_editor_id: "AmbSpace".into(),
            role: "ambience_loop".into(),
            source_rel: "sound/fx/amb.xwm".into(),
            event_guid: "E563D9FB-C52A-492A-8FF7-6623CB5CDC31".into(),
            wwed_local_formid: "0x000801".into(),
        }];
        let (groups, malformed) = group_manifest_rows(&rows).unwrap();
        assert!(malformed.is_empty());
        assert_eq!(groups.len(), 1);
        assert!(groups[0].is_ambience);
        assert!(groups[0].stop_guid.is_none());
    }

    #[test]
    fn group_manifest_rows_rejects_unknown_role() {
        let rows = vec![ManifestRow {
            fo4_form_key: "002000@Fallout4.esm".into(),
            fo4_editor_id: "X".into(),
            role: "bogus_role".into(),
            source_rel: "x.xwm".into(),
            event_guid: "E563D9FB-C52A-492A-8FF7-6623CB5CDC31".into(),
            wwed_local_formid: "0x000801".into(),
        }];
        assert!(group_manifest_rows(&rows).is_err());
    }

    #[test]
    fn parse_hex_formid_accepts_0x_prefix_and_bare_hex() {
        assert_eq!(parse_hex_formid("0x000800").unwrap(), 0x000800);
        assert_eq!(parse_hex_formid("000800").unwrap(), 0x000800);
        assert!(parse_hex_formid("not-hex").is_err());
    }

    #[test]
    fn parse_normal_rows_reads_all_fields() {
        let manifest = json!([
            {
                "fo4_form_key": "0217A3@Fallout4.esm",
                "fo4_editor_id": "Track",
                "role": "music_start",
                "source_rel": "music/a.xwm",
                "event": "FO4SF_MUSIC_A_XWM",
                "event_guid": "7376FFF8-E9A2-40FB-AFE8-C0FF75148AF7",
                "wwed_editor_id": "FO4SF_WWED_FO4SF_MUSIC_A_XWM",
                "wwed_local_formid": "0x000800"
            }
        ]);
        let rows = parse_normal_rows(&manifest).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].role, "music_start");
        assert_eq!(rows[0].wwed_local_formid, "0x000800");
    }

    #[test]
    fn is_placeholder_manifest_detects_mode() {
        assert!(is_placeholder_manifest(
            &json!({"mode": "placeholder", "targets": []})
        ));
        assert!(!is_placeholder_manifest(&json!([])));
    }

    #[test]
    fn parse_placeholder_targets_reads_purpose_and_form_id() {
        let manifest = json!({
            "mode": "placeholder",
            "sound_event_sets": "zeroed",
            "targets": [
                {"purpose": "music_explore", "record": "MUSC", "editor_id": "X", "form_id": "00C5D2"},
                {"purpose": "reverb_default", "record": "REVB", "editor_id": "DefaultReverb", "form_id": "0C5B6E"}
            ]
        });
        let targets = parse_placeholder_targets(&manifest).unwrap();
        assert_eq!(targets.len(), 2);
        assert_eq!(targets[0].purpose, "music_explore");
        assert_eq!(targets[0].form_id, "00C5D2");
    }

    // -----------------------------------------------------------------
    // form_key_field / form_key_list_field / set_form_key_field
    // -----------------------------------------------------------------

    #[test]
    fn form_key_field_reads_and_set_form_key_field_writes() {
        let interner = StringInterner::new();
        let plugin = interner.intern("Starfield.esm");
        let target = FormKey {
            local: 0x2000,
            plugin,
        };
        let mut record = record_with_fields("CELL", 0x1000, plugin, vec![]);
        assert_eq!(form_key_field(&record, "XCMO"), None);
        set_form_key_field(&mut record, sub("XCMO"), target);
        assert_eq!(form_key_field(&record, "XCMO"), Some(target));

        let other = FormKey {
            local: 0x3000,
            plugin,
        };
        set_form_key_field(&mut record, sub("XCMO"), other);
        assert_eq!(form_key_field(&record, "XCMO"), Some(other));
        assert_eq!(
            record
                .fields
                .iter()
                .filter(|f| f.sig == sub("XCMO"))
                .count(),
            1,
            "overwrite in place, not a duplicate field"
        );
    }

    #[test]
    fn form_key_list_field_reads_formid_array() {
        let interner = StringInterner::new();
        let plugin = interner.intern("Fallout4.esm");
        let a = FormKey {
            local: 0x10,
            plugin,
        };
        let b = FormKey {
            local: 0x20,
            plugin,
        };
        let record = record_with_fields(
            "CELL",
            0x1000,
            plugin,
            vec![(
                "XCLR",
                FieldValue::List(vec![FieldValue::FormKey(a), FieldValue::FormKey(b)]),
            )],
        );
        assert_eq!(form_key_list_field(&record, "XCLR"), vec![a, b]);
        assert_eq!(
            form_key_list_field(&record, "MISSING"),
            Vec::<FormKey>::new()
        );
    }

    // -----------------------------------------------------------------
    // rewire_content_refs — needs a mapper + real plugin handle
    // -----------------------------------------------------------------

    fn test_mapper_state(target_master_names: Vec<String>) -> MapperState {
        MapperState::new(
            std::iter::empty(),
            MapperOptions {
                output_plugin_name: "Fallout4_SF.esm".into(),
                target_master_names,
                ..Default::default()
            },
        )
    }

    #[test]
    fn rewire_content_refs_patches_must_mtsh_and_reports_unmatched() {
        use esp_authoring_core::plugin_runtime::{
            plugin_handle_close_native, plugin_handle_new_native,
        };

        let target_handle = plugin_handle_new_native("Fallout4_SF.esm", Some("starfield")).unwrap();
        let interner = StringInterner::new();
        let schema_target = AuthoringSchema::for_game("starfield").unwrap();

        let fo4_plugin = interner.intern("Fallout4.esm");
        let sf_plugin = interner.intern("Fallout4_SF.esm");
        let source_must_fk = FormKey {
            local: 0x0217A3,
            plugin: fo4_plugin,
        };
        let target_must_fk = FormKey {
            local: 0x000900,
            plugin: sf_plugin,
        };

        // Seed the target plugin with a translated MUST carrying a zeroed
        // MTSH (as the translation map would have emitted it).
        let must_record = record_with_fields(
            "MUST",
            target_must_fk.local,
            target_must_fk.plugin,
            vec![
                ("CNAM", FieldValue::Uint(1859641416)), // SingleTrack
                ("MTSH", FieldValue::Bytes(SmallVec::from_slice(&[0u8; 40]))),
            ],
        );
        crate::target_write::add_record_native(
            target_handle,
            must_record,
            &schema_target,
            &interner,
        )
        .unwrap();

        let mut state = test_mapper_state(vec!["Starfield.esm".into()]);
        let mut mapper = FormKeyMapper::from_state(&mut state, &interner);
        mapper.add_mapping(source_must_fk, target_must_fk);
        // Unmatched: no mapping registered for this source form key.
        let unmapped_source_fk = FormKey {
            local: 0x099999,
            plugin: fo4_plugin,
        };

        let groups = vec![
            AudioGroupPlan {
                fo4_form_key: source_must_fk.format(&interner),
                is_ambience: false,
                start_guid: [0xAAu8; 16],
                stop_guid: Some([0xBBu8; 16]),
                wwed_local_formid: Some(0x000800),
                sample_source_rel: "music/a.xwm".into(),
                sample_editor_id: "Track".into(),
            },
            AudioGroupPlan {
                fo4_form_key: unmapped_source_fk.format(&interner),
                is_ambience: false,
                start_guid: [0xCCu8; 16],
                stop_guid: None,
                wwed_local_formid: None,
                sample_source_rel: "music/unmapped.xwm".into(),
                sample_editor_id: "Unmapped".into(),
            },
        ];

        let own_index = 1u32; // one target master (Starfield.esm)
        let (n_changed, unmatched) = rewire_content_refs(
            target_handle,
            &schema_target,
            &interner,
            &mapper,
            &groups,
            own_index,
        )
        .unwrap();

        assert_eq!(n_changed, 1);
        assert_eq!(unmatched.len(), 1);
        assert!(unmatched[0].contains("music/unmapped.xwm"));

        let patched = read_record_relayout_by_form_key(
            target_handle,
            &target_must_fk,
            &schema_target,
            &interner,
            None,
        )
        .unwrap();
        let mtsh = patched
            .fields
            .iter()
            .find(|f| f.sig == sub("MTSH"))
            .unwrap();
        let FieldValue::Bytes(bytes) = &mtsh.value else {
            panic!("expected Bytes");
        };
        assert_eq!(bytes.len(), 40);
        assert_eq!(&bytes[0..16], &[0xAAu8; 16][..]);
        assert_eq!(&bytes[16..32], &[0xBBu8; 16][..]);
        // FormOnlyStartForm secondary: own_index=1 << 24 | local 0x000800.
        assert_eq!(&bytes[36..40], &((1u32 << 24) | 0x000800u32).to_le_bytes());

        plugin_handle_close_native(target_handle);
    }

    #[test]
    fn rewire_content_refs_ambience_group_targets_aspc_asls_solo() {
        use esp_authoring_core::plugin_runtime::{
            plugin_handle_close_native, plugin_handle_new_native,
        };

        let target_handle =
            plugin_handle_new_native("Fallout4_SF2.esm", Some("starfield")).unwrap();
        let interner = StringInterner::new();
        let schema_target = AuthoringSchema::for_game("starfield").unwrap();

        let fo4_plugin = interner.intern("Fallout4.esm");
        let sf_plugin = interner.intern("Fallout4_SF2.esm");
        let source_aspc_fk = FormKey {
            local: 0x002000,
            plugin: fo4_plugin,
        };
        let target_aspc_fk = FormKey {
            local: 0x000901,
            plugin: sf_plugin,
        };

        let aspc_record = record_with_fields(
            "ASPC",
            target_aspc_fk.local,
            target_aspc_fk.plugin,
            vec![("ASLS", FieldValue::Bytes(SmallVec::from_slice(&[0u8; 40])))],
        );
        crate::target_write::add_record_native(
            target_handle,
            aspc_record,
            &schema_target,
            &interner,
        )
        .unwrap();

        let mut state = test_mapper_state(vec!["Starfield.esm".into()]);
        let mut mapper = FormKeyMapper::from_state(&mut state, &interner);
        mapper.add_mapping(source_aspc_fk, target_aspc_fk);

        let groups = vec![AudioGroupPlan {
            fo4_form_key: source_aspc_fk.format(&interner),
            is_ambience: true,
            start_guid: [0x11u8; 16],
            stop_guid: None,
            wwed_local_formid: Some(0x000801),
            sample_source_rel: "sound/fx/amb.xwm".into(),
            sample_editor_id: "AmbSpace".into(),
        }];

        let (n_changed, unmatched) = rewire_content_refs(
            target_handle,
            &schema_target,
            &interner,
            &mapper,
            &groups,
            1,
        )
        .unwrap();
        assert_eq!(n_changed, 1);
        assert!(unmatched.is_empty());

        let patched = read_record_relayout_by_form_key(
            target_handle,
            &target_aspc_fk,
            &schema_target,
            &interner,
            None,
        )
        .unwrap();
        let asls = patched
            .fields
            .iter()
            .find(|f| f.sig == sub("ASLS"))
            .unwrap();
        let FieldValue::Bytes(bytes) = &asls.value else {
            panic!("expected Bytes");
        };
        assert_eq!(&bytes[0..16], &[0x11u8; 16][..]);
        assert_eq!(
            &bytes[16..32],
            &[0u8; 16][..],
            "ambience_loop rows are always solo: stop stays zero"
        );

        plugin_handle_close_native(target_handle);
    }

    // -----------------------------------------------------------------
    // REGN compensation
    // -----------------------------------------------------------------

    #[test]
    fn regn_music_compensation_pushes_region_music_onto_uncovered_cell_xcmo() {
        use esp_authoring_core::plugin_runtime::{
            plugin_handle_close_native, plugin_handle_new_native,
        };

        let source_handle = plugin_handle_new_native("Fallout4.esm", Some("fo4")).unwrap();
        let target_handle =
            plugin_handle_new_native("Fallout4_SF3.esm", Some("starfield")).unwrap();
        let interner = StringInterner::new();
        let schema_source = AuthoringSchema::for_game("fo4").unwrap();
        let schema_target = AuthoringSchema::for_game("starfield").unwrap();

        let fo4_plugin = interner.intern("Fallout4.esm");
        let sf_plugin = interner.intern("Fallout4_SF3.esm");

        let source_regn_fk = FormKey {
            local: 0x0117EB,
            plugin: fo4_plugin,
        };
        let source_musc_fk = FormKey {
            local: 0x00C5D2,
            plugin: fo4_plugin,
        };
        let source_cell_fk = FormKey {
            local: 0x001234,
            plugin: fo4_plugin,
        };

        let regn_record = record_with_fields(
            "REGN",
            source_regn_fk.local,
            fo4_plugin,
            vec![
                // RDAT (struct:I,B,B,B,B) gates the "region_data_entries"
                // scope RDMO belongs to — type=7 ("sound", the scope RDMO
                // shares with RDSA in the real FO4 layout); override/
                // priority/unknowns all 0.
                (
                    "RDAT",
                    FieldValue::Bytes(SmallVec::from_slice(&[7, 0, 0, 0, 0, 0, 0, 0])),
                ),
                ("RDMO", FieldValue::FormKey(source_musc_fk)),
            ],
        );
        crate::target_write::add_record_native(
            source_handle,
            regn_record,
            &schema_source,
            &interner,
        )
        .unwrap();

        let cell_record = record_with_fields(
            "CELL",
            source_cell_fk.local,
            fo4_plugin,
            vec![(
                "XCLR",
                FieldValue::List(vec![FieldValue::FormKey(source_regn_fk)]),
            )],
        );
        crate::target_write::add_record_native(
            source_handle,
            cell_record,
            &schema_source,
            &interner,
        )
        .unwrap();

        let target_musc_fk = FormKey {
            local: 0x000700,
            plugin: sf_plugin,
        };
        let target_cell_fk = FormKey {
            local: 0x000701,
            plugin: sf_plugin,
        };
        let target_cell_record =
            record_with_fields("CELL", target_cell_fk.local, sf_plugin, vec![]);
        crate::target_write::add_record_native(
            target_handle,
            target_cell_record,
            &schema_target,
            &interner,
        )
        .unwrap();

        let mut state = test_mapper_state(vec!["Starfield.esm".into()]);
        let mut mapper = FormKeyMapper::from_state(&mut state, &interner);
        mapper.add_mapping(source_musc_fk, target_musc_fk);
        mapper.add_mapping(source_cell_fk, target_cell_fk);

        let (n_changed, unmatched) = regn_music_compensation(
            source_handle,
            target_handle,
            &schema_source,
            &schema_target,
            &interner,
            &mapper,
        )
        .unwrap();
        assert_eq!(n_changed, 1);
        assert!(unmatched.is_empty());

        let patched = read_record_relayout_by_form_key(
            target_handle,
            &target_cell_fk,
            &schema_target,
            &interner,
            None,
        )
        .unwrap();
        assert_eq!(form_key_field(&patched, "XCMO"), Some(target_musc_fk));

        plugin_handle_close_native(source_handle);
        plugin_handle_close_native(target_handle);
    }

    #[test]
    fn regn_music_compensation_never_overrides_cells_own_direct_xcmo() {
        use esp_authoring_core::plugin_runtime::{
            plugin_handle_close_native, plugin_handle_new_native,
        };

        let source_handle = plugin_handle_new_native("Fallout4b.esm", Some("fo4")).unwrap();
        let target_handle =
            plugin_handle_new_native("Fallout4_SF4.esm", Some("starfield")).unwrap();
        let interner = StringInterner::new();
        let schema_source = AuthoringSchema::for_game("fo4").unwrap();
        let schema_target = AuthoringSchema::for_game("starfield").unwrap();

        let fo4_plugin = interner.intern("Fallout4b.esm");
        let sf_plugin = interner.intern("Fallout4_SF4.esm");

        let source_regn_fk = FormKey {
            local: 0x0117EB,
            plugin: fo4_plugin,
        };
        let source_musc_fk = FormKey {
            local: 0x00C5D2,
            plugin: fo4_plugin,
        };
        let source_cell_fk = FormKey {
            local: 0x001234,
            plugin: fo4_plugin,
        };

        let regn_record = record_with_fields(
            "REGN",
            source_regn_fk.local,
            fo4_plugin,
            vec![
                // RDAT (struct:I,B,B,B,B) gates the "region_data_entries"
                // scope RDMO belongs to — type=7 ("sound", the scope RDMO
                // shares with RDSA in the real FO4 layout); override/
                // priority/unknowns all 0.
                (
                    "RDAT",
                    FieldValue::Bytes(SmallVec::from_slice(&[7, 0, 0, 0, 0, 0, 0, 0])),
                ),
                ("RDMO", FieldValue::FormKey(source_musc_fk)),
            ],
        );
        crate::target_write::add_record_native(
            source_handle,
            regn_record,
            &schema_source,
            &interner,
        )
        .unwrap();
        let cell_record = record_with_fields(
            "CELL",
            source_cell_fk.local,
            fo4_plugin,
            vec![(
                "XCLR",
                FieldValue::List(vec![FieldValue::FormKey(source_regn_fk)]),
            )],
        );
        crate::target_write::add_record_native(
            source_handle,
            cell_record,
            &schema_source,
            &interner,
        )
        .unwrap();

        let target_musc_fk = FormKey {
            local: 0x000700,
            plugin: sf_plugin,
        };
        let target_cell_fk = FormKey {
            local: 0x000701,
            plugin: sf_plugin,
        };
        let direct_musc_fk = FormKey {
            local: 0x000702,
            plugin: sf_plugin,
        };
        // The cell ALREADY has its own direct XCMO — must not be clobbered.
        let target_cell_record = record_with_fields(
            "CELL",
            target_cell_fk.local,
            sf_plugin,
            vec![("XCMO", FieldValue::FormKey(direct_musc_fk))],
        );
        crate::target_write::add_record_native(
            target_handle,
            target_cell_record,
            &schema_target,
            &interner,
        )
        .unwrap();

        let mut state = test_mapper_state(vec!["Starfield.esm".into()]);
        let mut mapper = FormKeyMapper::from_state(&mut state, &interner);
        mapper.add_mapping(source_musc_fk, target_musc_fk);
        mapper.add_mapping(source_cell_fk, target_cell_fk);

        let (n_changed, _unmatched) = regn_music_compensation(
            source_handle,
            target_handle,
            &schema_source,
            &schema_target,
            &interner,
            &mapper,
        )
        .unwrap();
        assert_eq!(
            n_changed, 0,
            "must not touch a cell that already has its own XCMO"
        );

        let patched = read_record_relayout_by_form_key(
            target_handle,
            &target_cell_fk,
            &schema_target,
            &interner,
            None,
        )
        .unwrap();
        assert_eq!(form_key_field(&patched, "XCMO"), Some(direct_musc_fk));

        plugin_handle_close_native(source_handle);
        plugin_handle_close_native(target_handle);
    }

    // -----------------------------------------------------------------
    // Full-phase integration: normal mode + placeholder mode
    // -----------------------------------------------------------------

    fn make_run(source_handle_id: u64, target_handle_id: u64) -> u64 {
        use crate::run::{RunConfig, RunParams, create_run};
        use crate::translator::Game;

        create_run(RunParams {
            source: Game::Fo4,
            target: Game::Starfield,
            source_handle_id,
            target_handle_id,
            master_handle_ids: vec![],
            config: RunConfig {
                output_plugin_name: "Fallout4_SF.esm".into(),
                target_master_names: vec!["Starfield.esm".into()],
                ..Default::default()
            },
        })
        .unwrap()
    }

    #[test]
    fn full_phase_normal_mode_rewires_must_and_materializes_defaults() {
        use esp_authoring_core::plugin_runtime::{
            plugin_handle_close_native, plugin_handle_new_native,
        };
        use std::sync::atomic::AtomicBool;

        use crate::run::{drop_run, with_run};

        let source_handle = plugin_handle_new_native("Fallout4c.esm", Some("fo4")).unwrap();
        let target_handle =
            plugin_handle_new_native("Fallout4_SFc.esm", Some("starfield")).unwrap();
        let run_id = make_run(source_handle, target_handle);

        let tmp = tempfile::tempdir().unwrap();
        let mod_path = tmp.path().to_path_buf();
        std::fs::create_dir_all(mod_path.join("debug").join("wwise")).unwrap();

        let source_must_fk_str = "0217A3@Fallout4c.esm";
        let manifest = json!([
            {
                "fo4_form_key": source_must_fk_str,
                "fo4_editor_id": "Track",
                "role": "music_start",
                "source_rel": "music/a.xwm",
                "event": "FO4SF_MUSIC_A_XWM",
                "event_guid": "7376FFF8-E9A2-40FB-AFE8-C0FF75148AF7",
                "wwed_editor_id": "FO4SF_WWED_FO4SF_MUSIC_A_XWM",
                "wwed_local_formid": "0x000800"
            }
        ]);
        std::fs::write(
            mod_path
                .join("debug")
                .join("wwise")
                .join("events_manifest.json"),
            serde_json::to_string(&manifest).unwrap(),
        )
        .unwrap();

        let result = with_run(run_id, |run| -> Result<PhaseReport, crate::run::RunError> {
            let target_must_fk = FormKey {
                local: 0x000900,
                plugin: run.interner.intern("Fallout4_SFc.esm"),
            };
            let must_record = record_with_fields(
                "MUST",
                target_must_fk.local,
                target_must_fk.plugin,
                vec![("MTSH", FieldValue::Bytes(SmallVec::from_slice(&[0u8; 40])))],
            );
            crate::target_write::add_record_native(
                run.target_handle_id,
                must_record,
                &run.schema_target,
                &run.interner,
            )
            .unwrap();

            let musc_record = record_with_fields(
                "MUSC",
                0x000901,
                run.interner.intern("Fallout4_SFc.esm"),
                vec![],
            );
            crate::target_write::add_record_native(
                run.target_handle_id,
                musc_record,
                &run.schema_target,
                &run.interner,
            )
            .unwrap();

            run.mapper_state = Some(test_mapper_state(vec!["Starfield.esm".into()]));
            {
                let state = run.mapper_state.as_mut().unwrap();
                let mut mapper = FormKeyMapper::from_state(state, &run.interner);
                let source_fk = FormKey::parse(source_must_fk_str, &run.interner).unwrap();
                mapper.add_mapping(source_fk, target_must_fk);
            }

            let cancel = AtomicBool::new(false);
            let mut ctx = PhaseCtx {
                run,
                mod_path: &mod_path,
                source_extracted_dir: &mod_path,
                target_extracted_dir: None,
                target_data_dir: None,
                params: &JsonValue::Null,
                cancel: &cancel,
            };
            AudioRewirePhase
                .run(&mut ctx)
                .map_err(|e| crate::run::RunError::InvalidConfig(e.to_string()))
        })
        .unwrap();

        let report = result;
        // defaults pass: MUST gains MSTF, MUSC gains VNAM/UNAM (2 records) +
        // content-ref pass: MUST's MTSH is rewired (1 record) = 3.
        assert_eq!(report.records_changed, 3);
        assert_eq!(report.warnings, 0);

        with_run(run_id, |run| -> Result<(), crate::run::RunError> {
            let target_must_fk = FormKey {
                local: 0x000900,
                plugin: run.interner.intern("Fallout4_SFc.esm"),
            };
            let patched = read_record_relayout_by_form_key(
                run.target_handle_id,
                &target_must_fk,
                &run.schema_target,
                &run.interner,
                None,
            )
            .unwrap();
            let mtsh = patched
                .fields
                .iter()
                .find(|f| f.sig == sub("MTSH"))
                .unwrap();
            let FieldValue::Bytes(bytes) = &mtsh.value else {
                panic!("expected Bytes");
            };
            assert_ne!(
                &bytes[0..16],
                &[0u8; 16][..],
                "start GUID must be non-zero after rewire"
            );
            Ok(())
        })
        .unwrap();

        drop_run(run_id).unwrap();
        plugin_handle_close_native(source_handle);
        plugin_handle_close_native(target_handle);
    }

    #[test]
    fn full_phase_placeholder_mode_repoints_cell_xcmo_and_touches_no_sound_event_set() {
        use esp_authoring_core::plugin_runtime::{
            plugin_handle_close_native, plugin_handle_new_native,
        };
        use std::sync::atomic::AtomicBool;

        use crate::run::{drop_run, with_run};

        let source_handle = plugin_handle_new_native("Fallout4d.esm", Some("fo4")).unwrap();
        let target_handle =
            plugin_handle_new_native("Fallout4_SFd.esm", Some("starfield")).unwrap();
        let run_id = make_run(source_handle, target_handle);

        let tmp = tempfile::tempdir().unwrap();
        let mod_path = tmp.path().to_path_buf();
        std::fs::create_dir_all(mod_path.join("debug").join("wwise")).unwrap();

        let manifest = json!({
            "mode": "placeholder",
            "sound_event_sets": "zeroed",
            "targets": [
                {"purpose": "music_explore", "record": "MUSC", "editor_id": "MUSGenesisPlanetA_Mountains", "form_id": "00C5D2"},
                {"purpose": "music_combat", "record": "MUSC", "editor_id": "MUSGenesisCombat", "form_id": "013D5F"},
                {"purpose": "music_city", "record": "MUSC", "editor_id": "MUSGenesisCityA_NewAtlantis", "form_id": "010DFE"},
                {"purpose": "music_dungeon", "record": "MUSC", "editor_id": "MUSGenesisDungeonIndustrialC", "form_id": "1804D6"},
                {"purpose": "music_silence", "record": "MUSC", "editor_id": "_MUSExplore_WwiseSilence", "form_id": "13E4E7"},
                {"purpose": "ambient_interior", "record": "ASPC", "editor_id": "Int_Space_Ship_Science_Medium", "form_id": "0013FA"},
                {"purpose": "reverb_default", "record": "REVB", "editor_id": "DefaultReverb", "form_id": "0C5B6E"}
            ]
        });
        std::fs::write(
            mod_path
                .join("debug")
                .join("wwise")
                .join("events_manifest.json"),
            serde_json::to_string(&manifest).unwrap(),
        )
        .unwrap();

        let result = with_run(run_id, |run| -> Result<PhaseReport, crate::run::RunError> {
            let stale_musc_fk = FormKey {
                local: 0x000905,
                plugin: run.interner.intern("Fallout4_SFd.esm"),
            };
            let cell_record = record_with_fields(
                "CELL",
                0x000906,
                run.interner.intern("Fallout4_SFd.esm"),
                vec![("XCMO", FieldValue::FormKey(stale_musc_fk))],
            );
            crate::target_write::add_record_native(
                run.target_handle_id,
                cell_record,
                &run.schema_target,
                &run.interner,
            )
            .unwrap();

            // A cell with no music reference at all must stay untouched.
            let silent_cell = record_with_fields(
                "CELL",
                0x000907,
                run.interner.intern("Fallout4_SFd.esm"),
                vec![],
            );
            crate::target_write::add_record_native(
                run.target_handle_id,
                silent_cell,
                &run.schema_target,
                &run.interner,
            )
            .unwrap();

            let cancel = AtomicBool::new(false);
            let mut ctx = PhaseCtx {
                run,
                mod_path: &mod_path,
                source_extracted_dir: &mod_path,
                target_extracted_dir: None,
                target_data_dir: None,
                params: &JsonValue::Null,
                cancel: &cancel,
            };
            AudioRewirePhase
                .run(&mut ctx)
                .map_err(|e| crate::run::RunError::InvalidConfig(e.to_string()))
        })
        .unwrap();

        let report = result;
        assert_eq!(
            report.records_changed, 1,
            "only the cell with a pre-existing XCMO is repointed"
        );

        with_run(run_id, |run| -> Result<(), crate::run::RunError> {
            let cell_fk = FormKey {
                local: 0x000906,
                plugin: run.interner.intern("Fallout4_SFd.esm"),
            };
            let patched = read_record_relayout_by_form_key(
                run.target_handle_id,
                &cell_fk,
                &run.schema_target,
                &run.interner,
                None,
            )
            .unwrap();
            let xcmo = form_key_field(&patched, "XCMO").unwrap();
            assert_eq!(
                xcmo.local, 0x00C5D2,
                "repointed at the vanilla music_explore target"
            );

            let silent_fk = FormKey {
                local: 0x000907,
                plugin: run.interner.intern("Fallout4_SFd.esm"),
            };
            let silent = read_record_relayout_by_form_key(
                run.target_handle_id,
                &silent_fk,
                &run.schema_target,
                &run.interner,
                None,
            )
            .unwrap();
            assert_eq!(
                form_key_field(&silent, "XCMO"),
                None,
                "cell with no music reference stays silent"
            );
            Ok(())
        })
        .unwrap();

        drop_run(run_id).unwrap();
        plugin_handle_close_native(source_handle);
        plugin_handle_close_native(target_handle);
    }

    #[test]
    fn full_phase_placeholder_mode_materializes_musc_defaults() {
        // run_placeholder() must run materialize_defaults_pass like run_normal()
        // does: placeholder audio is the only CI-safe path (real Wwise needs a
        // licensed local install).
        use esp_authoring_core::plugin_runtime::{
            plugin_handle_close_native, plugin_handle_new_native,
        };
        use std::sync::atomic::AtomicBool;

        use crate::run::{drop_run, with_run};

        let source_handle = plugin_handle_new_native("Fallout4e.esm", Some("fo4")).unwrap();
        let target_handle =
            plugin_handle_new_native("Fallout4_SFe.esm", Some("starfield")).unwrap();
        let run_id = make_run(source_handle, target_handle);

        let tmp = tempfile::tempdir().unwrap();
        let mod_path = tmp.path().to_path_buf();
        std::fs::create_dir_all(mod_path.join("debug").join("wwise")).unwrap();

        let manifest = json!({
            "mode": "placeholder",
            "sound_event_sets": "zeroed",
            "targets": [
                {"purpose": "music_explore", "record": "MUSC", "editor_id": "MUSGenesisPlanetA_Mountains", "form_id": "00C5D2"},
                {"purpose": "ambient_interior", "record": "ASPC", "editor_id": "Int_Space_Ship_Science_Medium", "form_id": "0013FA"},
                {"purpose": "reverb_default", "record": "REVB", "editor_id": "DefaultReverb", "form_id": "0C5B6E"}
            ]
        });
        std::fs::write(
            mod_path
                .join("debug")
                .join("wwise")
                .join("events_manifest.json"),
            serde_json::to_string(&manifest).unwrap(),
        )
        .unwrap();

        let result = with_run(run_id, |run| -> Result<PhaseReport, crate::run::RunError> {
            let musc_record = record_with_fields(
                "MUSC",
                0x000908,
                run.interner.intern("Fallout4_SFe.esm"),
                vec![],
            );
            crate::target_write::add_record_native(
                run.target_handle_id,
                musc_record,
                &run.schema_target,
                &run.interner,
            )
            .unwrap();

            let cancel = AtomicBool::new(false);
            let mut ctx = PhaseCtx {
                run,
                mod_path: &mod_path,
                source_extracted_dir: &mod_path,
                target_extracted_dir: None,
                target_data_dir: None,
                params: &JsonValue::Null,
                cancel: &cancel,
            };
            AudioRewirePhase
                .run(&mut ctx)
                .map_err(|e| crate::run::RunError::InvalidConfig(e.to_string()))
        })
        .unwrap();

        let report = result;
        assert_eq!(
            report.records_changed, 1,
            "the defaults pass must materialize VNAM/UNAM on the MUSC in placeholder mode too"
        );

        with_run(run_id, |run| -> Result<(), crate::run::RunError> {
            let musc_fk = FormKey {
                local: 0x000908,
                plugin: run.interner.intern("Fallout4_SFe.esm"),
            };
            let patched = read_record_relayout_by_form_key(
                run.target_handle_id,
                &musc_fk,
                &run.schema_target,
                &run.interner,
                None,
            )
            .unwrap();
            // Checks presence only. The generated Starfield MUSC schema has a
            // spurious "VNAM: uint8 / UNAM: formid" pair ahead of the real uint64
            // pair, and `RecordDef::subrecord_def` resolves the first match, so
            // UNAM=0 reads back as FieldValue::None. `musc_defaults_added_when_absent`
            // checks the exact u64 values.
            assert!(
                patched.fields.iter().any(|f| f.sig == sub("VNAM")),
                "VNAM must be materialized"
            );
            assert!(
                patched.fields.iter().any(|f| f.sig == sub("UNAM")),
                "UNAM must be materialized"
            );
            Ok(())
        })
        .unwrap();

        drop_run(run_id).unwrap();
        plugin_handle_close_native(source_handle);
        plugin_handle_close_native(target_handle);
    }
}
