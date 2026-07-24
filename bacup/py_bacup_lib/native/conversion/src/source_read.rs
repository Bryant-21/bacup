//! Schema-driven source-handle decoder: reads a raw `ParsedRecord` from a
//! plugin handle and produces a typed `Record` using `AuthoringSchema`.
//!
//! `read_record` is the primary entry point. It:
//!   1. Locks the global plugin handle store and finds the slot.
//!   2. Builds (or retrieves cached) the locator index section.
//!   3. Looks up the requested form_key, fetches the raw `ParsedRecord`.
//!   4. Walks each `ParsedSubrecord`, dispatches on the schema codec, and
//!      produces a `FieldEntry` (or a warning for unknown codecs).
//!
//! Unknown codecs are recorded as warnings rather than hard errors so that
//! partial decodes can still proceed. Only truly fatal problems (missing
//! record, unparseable form_key) return `Err(RecordReadError)`.

use crate::errors::{DecodeError, RecordReadError};
use crate::ids::{FormKey, SigCode, SubrecordSig};
use crate::record::{FieldEntry, FieldValue, Record, RecordFlags};
use crate::schema::AuthoringSchema;
use crate::sym::StringInterner;
use esp_authoring_core::plugin_runtime::{
    LocalizedStringsState, NativePluginSlot, ParsedItem, ParsedRecord,
    effective_subrecords_for_record, ensure_core_section, ensure_locator_section,
    plugin_handle_store_ref,
};
use smol_str::SmolStr;
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

thread_local! {
    static NORMALIZED_INDEX_PLUGIN_NAMES: RefCell<HashMap<String, Arc<str>>> =
        RefCell::new(HashMap::new());
}

fn normalized_index_plugin_name(plugin_name: &str) -> Arc<str> {
    NORMALIZED_INDEX_PLUGIN_NAMES.with(|cache| {
        let mut cache = cache.borrow_mut();
        if let Some(normalized) = cache.get(plugin_name) {
            return normalized.clone();
        }
        let normalized_text = plugin_name.to_ascii_lowercase();
        let normalized = cache
            .get(normalized_text.as_str())
            .cloned()
            .unwrap_or_else(|| Arc::from(normalized_text.as_str()));
        cache
            .entry(normalized_text)
            .or_insert_with(|| normalized.clone());
        cache.insert(plugin_name.to_string(), normalized.clone());
        normalized
    })
}

pub(crate) const TES4_FLAG_LOCALIZED: u32 = 0x0000_0080;

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Read and decode a single record from a loaded plugin handle.
///
/// # Arguments
/// * `handle_id` — the integer handle returned by `plugin_handle_load` etc.
/// * `form_key_str` — form_key string in "PluginName.esm:XXXXXX" format.
/// * `schema` — the parsed authoring schema for the plugin's game.
/// * `interner` — per-run string interner for `Sym` allocation.
///
/// # Errors
/// Returns `RecordReadError::NotFound` when no record matches `form_key_str`.
/// Returns `RecordReadError::UnknownSignature` when the record sig has no
/// schema entry. Subrecord decode failures are captured as warnings in the
/// returned `Record` rather than propagated as errors.
pub fn read_record(
    handle_id: u64,
    form_key_str: &str,
    schema: &AuthoringSchema,
    interner: &StringInterner,
) -> Result<Record, RecordReadError> {
    read_record_relayout(handle_id, form_key_str, schema, interner, None)
}

pub fn read_record_relayout_by_form_key(
    handle_id: u64,
    fk: &FormKey,
    schema: &AuthoringSchema,
    interner: &StringInterner,
    relayout: Option<&crate::struct_relayout::StructRelayoutCtx<'_>>,
) -> Result<Record, RecordReadError> {
    let mut store = plugin_handle_store_ref().lock().unwrap();
    let slot = store
        .get_mut(&handle_id)
        .ok_or_else(|| RecordReadError::NotFound(format!("no plugin handle: {handle_id}")))?;
    decode_record_in_slot(slot, fk, schema, interner, relayout)
}

/// Same as [`read_record`], plus an optional FO76→FO4 struct byte-relayout
/// context applied to divergent `struct:` codec subrecords (see
/// `crate::struct_relayout`). The whole-plugin / asset FO76→FO4 translate path
/// passes `Some`; everything else uses [`read_record`].
pub fn read_record_relayout(
    handle_id: u64,
    form_key_str: &str,
    schema: &AuthoringSchema,
    interner: &StringInterner,
    relayout: Option<&crate::struct_relayout::StructRelayoutCtx<'_>>,
) -> Result<Record, RecordReadError> {
    let fk = parse_form_key_from_render(form_key_str, interner)?;
    read_record_relayout_by_form_key(handle_id, &fk, schema, interner, relayout)
}

pub fn plugin_name_for_handle(handle_id: u64) -> Result<String, RecordReadError> {
    let mut store = plugin_handle_store_ref().lock().unwrap();
    let slot = store
        .get_mut(&handle_id)
        .ok_or_else(|| RecordReadError::NotFound(format!("no plugin handle: {handle_id}")))?;
    Ok(slot.parsed.plugin_name.clone())
}

pub fn plugin_context_for_handle(handle_id: u64) -> Result<(String, Vec<String>), RecordReadError> {
    let mut store = plugin_handle_store_ref().lock().unwrap();
    let slot = store
        .get_mut(&handle_id)
        .ok_or_else(|| RecordReadError::NotFound(format!("no plugin handle: {handle_id}")))?;
    Ok((
        slot.parsed.plugin_name.clone(),
        slot.parsed.header.masters.clone(),
    ))
}

pub(crate) struct SourceRecordSnapshot {
    pub form_key: FormKey,
    pub raw_record: ParsedRecord,
}

pub(crate) struct SourceRecordBatchSnapshot {
    pub masters: Vec<String>,
    pub plugin_name: String,
    pub strings: Option<LocalizedStringsState>,
    pub plugin_is_localized: bool,
    pub records: Vec<SourceRecordSnapshot>,
}

pub(crate) fn snapshot_records_by_form_keys(
    handle_id: u64,
    fks: &[FormKey],
    interner: &StringInterner,
) -> Result<SourceRecordBatchSnapshot, RecordReadError> {
    let mut store = plugin_handle_store_ref().lock().unwrap();
    let slot = store
        .get_mut(&handle_id)
        .ok_or_else(|| RecordReadError::NotFound(format!("no plugin handle: {handle_id}")))?;

    let masters = slot.parsed.header.masters.clone();
    let plugin_name = slot.parsed.plugin_name.clone();
    let plugin_is_localized = (slot.parsed.header.flags & TES4_FLAG_LOCALIZED) != 0;
    let strings = plugin_is_localized.then(|| slot.strings_ref().clone());
    let mut records = Vec::with_capacity(fks.len());

    if slot.is_lazy() {
        let core = ensure_core_section(slot);
        for fk in fks {
            let lookup_fk = lookup_form_key(fk, interner)?;
            let raw_form_id = {
                let entry = core
                    .by_form_key
                    .get(&lookup_fk)
                    .ok_or_else(|| RecordReadError::NotFound(not_found_form_key(fk, interner)))?;
                entry.raw_form_id
            };
            let raw_record = slot
                .lazy_record(raw_form_id)
                .ok_or_else(|| RecordReadError::NotFound(not_found_form_key(fk, interner)))?;
            records.push(SourceRecordSnapshot {
                form_key: *fk,
                raw_record,
            });
        }
    } else {
        let locator = ensure_locator_section(slot);
        for fk in fks {
            let lookup_fk = lookup_form_key(fk, interner)?;
            let entry = locator
                .by_form_key
                .get(&lookup_fk)
                .ok_or_else(|| RecordReadError::NotFound(not_found_form_key(fk, interner)))?;
            let raw_record = locator
                .record(&slot.parsed, entry)
                .ok_or_else(|| RecordReadError::NotFound(not_found_form_key(fk, interner)))?;
            records.push(SourceRecordSnapshot {
                form_key: *fk,
                raw_record: raw_record.clone(),
            });
        }
    }

    Ok(SourceRecordBatchSnapshot {
        masters,
        plugin_name,
        strings,
        plugin_is_localized,
        records,
    })
}

fn lookup_form_key(
    fk: &FormKey,
    interner: &StringInterner,
) -> Result<esp_authoring_core::plugin_runtime::FormKey, RecordReadError> {
    let plugin_name = interner
        .resolve(fk.plugin)
        .ok_or_else(|| RecordReadError::InvalidFormKey(format!("{:06X}@<unresolved>", fk.local)))?;
    if plugin_name.is_empty() {
        return Err(RecordReadError::InvalidFormKey(format!(
            "{:06X}@<empty>",
            fk.local
        )));
    }
    Ok(esp_authoring_core::plugin_runtime::FormKey::new(
        normalized_index_plugin_name(plugin_name),
        fk.local,
    ))
}

fn not_found_form_key(fk: &FormKey, interner: &StringInterner) -> String {
    let plugin_name = interner.resolve(fk.plugin).unwrap_or("<unresolved>");
    format!("{}:{:06X}", plugin_name, fk.local)
}

pub(crate) fn decode_record_in_slot(
    slot: &mut NativePluginSlot,
    fk: &FormKey,
    schema: &AuthoringSchema,
    interner: &StringInterner,
    relayout: Option<&crate::struct_relayout::StructRelayoutCtx<'_>>,
) -> Result<Record, RecordReadError> {
    let Some(plugin_name) = interner.resolve(fk.plugin) else {
        return Err(RecordReadError::InvalidFormKey(format!(
            "{:06X}@<unresolved>",
            fk.local
        )));
    };
    if plugin_name.is_empty() {
        return Err(RecordReadError::InvalidFormKey(format!(
            "{:06X}@<empty>",
            fk.local
        )));
    }
    let lookup_fk = esp_authoring_core::plugin_runtime::FormKey::new(
        normalized_index_plugin_name(plugin_name),
        fk.local,
    );
    let not_found = || format!("{}:{:06X}", plugin_name, fk.local);

    // Index-only (lazy) handles — read-only target masters — have an empty
    // `parsed.root_items` tree, so the locator path below would never find the
    // record. Resolve raw_form_id from the self-contained CoreSection and
    // re-parse the single record from the retained buffer instead.
    if slot.is_lazy() {
        let raw_form_id = {
            let core = ensure_core_section(slot);
            let entry = core
                .by_form_key
                .get(&lookup_fk)
                .ok_or_else(|| RecordReadError::NotFound(not_found()))?;
            entry.raw_form_id
        };
        let raw_record = slot
            .lazy_record(raw_form_id)
            .ok_or_else(|| RecordReadError::NotFound(not_found()))?;
        let plugin_is_localized = (slot.parsed.header.flags & TES4_FLAG_LOCALIZED) != 0;
        let strings = plugin_is_localized.then(|| slot.strings_ref());
        return decode_record_from_parsed_relayout(
            &raw_record,
            fk,
            schema,
            &slot.parsed.header.masters,
            &slot.parsed.plugin_name,
            strings,
            plugin_is_localized,
            interner,
            relayout,
        );
    }

    let locator = ensure_locator_section(slot);
    let entry = locator
        .by_form_key
        .get(&lookup_fk)
        .ok_or_else(|| RecordReadError::NotFound(not_found()))?;

    let raw_record = locator
        .record(&slot.parsed, entry)
        .ok_or_else(|| RecordReadError::NotFound(not_found()))?;

    let plugin_is_localized = (slot.parsed.header.flags & TES4_FLAG_LOCALIZED) != 0;
    let strings = plugin_is_localized.then(|| slot.strings_ref());

    decode_record_from_parsed_relayout(
        raw_record,
        fk,
        schema,
        &slot.parsed.header.masters,
        &slot.parsed.plugin_name,
        strings,
        plugin_is_localized,
        interner,
        relayout,
    )
}

pub(crate) fn decode_record_from_parsed(
    raw_record: &ParsedRecord,
    fk: &FormKey,
    schema: &AuthoringSchema,
    masters: &[String],
    plugin_name: &str,
    strings: Option<&LocalizedStringsState>,
    plugin_is_localized: bool,
    interner: &StringInterner,
) -> Result<Record, RecordReadError> {
    decode_record_from_parsed_relayout(
        raw_record,
        fk,
        schema,
        masters,
        plugin_name,
        strings,
        plugin_is_localized,
        interner,
        None,
    )
}

/// Same as [`decode_record_from_parsed`], plus an optional source→FO4 struct
/// byte-relayout context. When `relayout` is `Some`, every `struct:` codec
/// subrecord whose source/target field layouts diverge is rewritten from the
/// source layout into the target layout before being emitted as `Bytes` (see
/// `crate::struct_relayout`). The context controls whether all divergent FO76
/// structs or only legacy Fallout BPTD.BPND rows are eligible.
#[allow(clippy::too_many_arguments)]
pub(crate) fn decode_record_from_parsed_relayout(
    raw_record: &ParsedRecord,
    fk: &FormKey,
    schema: &AuthoringSchema,
    masters: &[String],
    plugin_name: &str,
    strings: Option<&LocalizedStringsState>,
    plugin_is_localized: bool,
    interner: &StringInterner,
    relayout: Option<&crate::struct_relayout::StructRelayoutCtx<'_>>,
) -> Result<Record, RecordReadError> {
    let sig = SigCode::from_str(raw_record.signature.as_str())
        .map_err(|_| RecordReadError::UnknownSignature(SigCode([0, 0, 0, 0])))?;

    let record_def = schema.record_def(raw_record.signature.as_str());

    let subrecords = effective_subrecords_for_record(raw_record);
    let raw_flags = raw_record.flags;
    // Preserve ALL 32 header-flag bits losslessly. `from_bits_truncate` dropped
    // every bit not in the curated `RecordFlags` set — both FO76-only bits AND
    // FO4-valid bits we never named — corrupting the header word before the
    // Class A masking pass (run.rs stage 6.5) can run. `from_bits_retain` keeps
    // the raw word; the masking pass clears unknown bits against the FO4 schema
    // valid mask, not the named constants (several of which are mislabeled).
    let flags = RecordFlags::from_bits_retain(raw_flags);
    let mut record = Record {
        sig,
        form_key: *fk,
        eid: None,
        flags,
        fields: smallvec::SmallVec::new(),
        warnings: smallvec::SmallVec::new(),
    };

    let is_pack_record = raw_record.signature.as_str() == "PACK";
    let is_qust_record = raw_record.signature.as_str() == "QUST";
    let is_scen_record = raw_record.signature.as_str() == "SCEN";
    let is_term_record = raw_record.signature.as_str() == "TERM";
    let mut in_pack_package_data = false;
    let mut after_pack_procedure_marker = false;
    let mut after_term_marker_model = false;
    // SCEN has two TNAM subrecords in schema: float32 (Timer, actions-scoped)
    // and formid (Template Scene, top-level). subrecord_def() returns the first
    // match (float32), so the Template Scene TNAM is incorrectly decoded as a
    // float and its FormKey is never remapped 00→07.
    //
    // Fix: the Template Scene TNAM sits in the record-level tail block
    // "VNAM TNAM XNAM". Every other TNAM lives inside an action block and is
    // preceded by SNAM/ENAM/ANAM — never by VNAM. So: override to formid ONLY
    // when the immediately-preceding subrecord signature is VNAM.
    let mut scen_prev_sig = "";
    for sr in subrecords.iter() {
        let subrec_sig_str = sr.signature.as_str();

        // EDID always decodes as a zstring regardless of schema.
        if subrec_sig_str == "EDID" {
            if let Ok(s) = decode_zstring(&sr.data) {
                record.eid = Some(interner.intern(&s));
            }
            // Also emit as a regular field.
        }

        let sub_sig = match SubrecordSig::from_str(subrec_sig_str) {
            Ok(s) => s,
            Err(_) => {
                // 3-byte or non-ASCII sig — record as warning, skip.
                let w = interner.intern(&format!("bad_subrec_sig:{subrec_sig_str}"));
                record.warnings.push(w);
                continue;
            }
        };

        let subrecord_def = record_def.and_then(|rd| rd.subrecord_def(subrec_sig_str));
        let codec = subrecord_def.and_then(|sd| {
            if sd.kind == "raw" {
                None
            } else {
                sd.codec.as_deref()
            }
        });
        let localized_strings =
            if plugin_is_localized && subrecord_def.is_some_and(|sd| sd.localized) {
                strings
            } else {
                None
            };

        // SCEN.TNAM: the Template Scene (formid) TNAM is always immediately
        // preceded by VNAM at the record-level tail. Action-scoped timer (float32)
        // TNAMs are surrounded by SNAM/ENAM/ANAM, never by VNAM.
        let scen_tnam_override_codec =
            is_scen_record && subrec_sig_str == "TNAM" && scen_prev_sig == "VNAM";

        // TERM reuses SNAM: the pre-XMRK row is a sound FormID, while the
        // post-XMRK rows are 24-byte marker structs.
        // QUST overloads SNAM: the record-level and alias forms are zstrings,
        // while objective-scope SNAM is uint16. Preserve all three until the
        // pair hook removes the incompatible objective form by scope.
        let force_raw_bytes = (is_pack_record
            && ((in_pack_package_data && subrec_sig_str == "CNAM")
                || (after_pack_procedure_marker && subrec_sig_str == "PNAM")))
            || (is_qust_record && subrec_sig_str == "SNAM")
            || (is_term_record
                && !sr.data.is_empty()
                && sr.data.len() % 24 == 0
                && (subrec_sig_str == "ZNAM"
                    || (subrec_sig_str == "SNAM" && after_term_marker_model)));
        let override_codec: Option<&str> = if scen_tnam_override_codec {
            Some("formid")
        } else {
            None
        };
        let effective_codec = override_codec.or(codec);

        let value = if force_raw_bytes {
            let bytes: smallvec::SmallVec<[u8; 32]> = sr.data.iter().copied().collect();
            FieldValue::Bytes(bytes)
        } else {
            match effective_codec {
                None => {
                    // No schema entry or raw kind — emit raw bytes.
                    let bytes: smallvec::SmallVec<[u8; 32]> = sr.data.iter().copied().collect();
                    FieldValue::Bytes(bytes)
                }
                Some(codec_name) => {
                    // Source→FO4 struct relayout: for an eligible divergent codec,
                    // rewrite the source-laid-out bytes into the target layout and
                    // emit them directly (the generic struct decode would emit the
                    // raw source-laid-out bytes, which the FO4 game reads at wrong
                    // offsets). Only active when a relayout ctx is supplied.
                    let relaid = relayout
                        .filter(|_| codec_name.starts_with("struct:"))
                        .and_then(|ctx| {
                            crate::struct_relayout::relayout_struct_bytes(
                                raw_record.signature.as_str(),
                                subrec_sig_str,
                                &sr.data,
                                schema,
                                raw_record.form_version,
                                ctx,
                            )
                        });
                    if let Some(bytes) = relaid {
                        FieldValue::Bytes(bytes.into_iter().collect())
                    } else {
                        match decode_subrecord(
                            raw_record.signature.as_str(),
                            subrec_sig_str,
                            codec_name,
                            &sr.data,
                            masters,
                            plugin_name,
                            localized_strings,
                            interner,
                        ) {
                            Ok(v) => v,
                            Err(DecodeError::UnknownCodec(name)) => {
                                let w = interner.intern(&format!("unknown_codec:{name}"));
                                record.warnings.push(w);
                                let bytes: smallvec::SmallVec<[u8; 32]> =
                                    sr.data.iter().copied().collect();
                                FieldValue::Bytes(bytes)
                            }
                            Err(e) => {
                                let w = interner.intern(&format!("decode_error:{e}"));
                                record.warnings.push(w);
                                FieldValue::None
                            }
                        }
                    }
                }
            }
        };

        record.fields.push(FieldEntry {
            sig: sub_sig,
            value,
        });

        if is_scen_record {
            scen_prev_sig = subrec_sig_str;
        }

        if is_term_record && subrec_sig_str == "XMRK" {
            after_term_marker_model = true;
        }

        if is_pack_record {
            match subrec_sig_str {
                "PKCU" => in_pack_package_data = true,
                "XNAM" => {
                    in_pack_package_data = false;
                    after_pack_procedure_marker = true;
                }
                _ => {}
            }
        }
    }

    Ok(record)
}

// ---------------------------------------------------------------------------
// EID index collection
// ---------------------------------------------------------------------------

/// Collect every EID-keyed record from a plugin handle into a flat list of
/// `(eid_sym, FormKey, SigCode)` tuples suitable for seeding a `MapperState`.
///
/// Uses the plugin core index, which already stores each record's decoded EDID,
/// signature, and owning plugin. This avoids a second recursive walk of the
/// parsed tree during `translate_all` mapper setup.
///
/// The `schema` parameter is unused; kept only for call-site compatibility.
pub fn collect_eid_index(
    handle_id: u64,
    _schema: &crate::schema::AuthoringSchema,
    interner: &StringInterner,
) -> Result<Vec<(crate::sym::Sym, FormKey, SigCode)>, crate::errors::RecordReadError> {
    let mut store = plugin_handle_store_ref().lock().unwrap();
    let slot = store
        .get_mut(&handle_id)
        .ok_or_else(|| RecordReadError::NotFound(format!("no plugin handle: {handle_id}")))?;

    let core = ensure_core_section(slot);
    drop(store);

    let mut out: Vec<(crate::sym::Sym, FormKey, SigCode)> =
        Vec::with_capacity(core.by_form_key.len());
    for entry in core.by_form_key.values() {
        if entry.eid.is_empty() {
            continue;
        }
        let Ok(sig) = SigCode::from_str(entry.signature.as_str()) else {
            continue;
        };
        let eid_sym = interner.intern(&entry.eid);
        out.push((eid_sym, form_key_from_index_entry(entry, interner), sig));
    }

    Ok(out)
}

// ---------------------------------------------------------------------------
// Plugin-level sig enumeration
// ---------------------------------------------------------------------------

/// Return every unique record signature that has at least one record in `handle_id`.
///
/// # Errors
/// Returns `RecordReadError::NotFound` when `handle_id` is not a loaded handle.
pub fn source_signatures(
    handle_id: u64,
    interner: &StringInterner,
) -> Result<Vec<SigCode>, RecordReadError> {
    let mut store = plugin_handle_store_ref().lock().unwrap();
    let slot = store
        .get_mut(&handle_id)
        .ok_or_else(|| RecordReadError::NotFound(format!("no plugin handle: {handle_id}")))?;

    let core = ensure_core_section(slot);
    let mut sigs: Vec<SigCode> = core
        .by_signature_form_keys
        .keys()
        .filter_map(|k| SigCode::from_str(k.as_str()).ok())
        .collect();
    // `by_signature_form_keys` is a std HashMap (RandomState), so `.keys()` yields a
    // different order each process. translate_all drives lstring-id allocation in
    // record/sig-processing order, so a random sig order made the localized string
    // table — and thus the output ESM — byte-non-reproducible. Sort for determinism.
    sigs.sort_unstable();
    drop(core);
    drop(store);

    let _ = interner; // kept for API consistency with caller
    Ok(sigs)
}

// ---------------------------------------------------------------------------
// Sig-based iteration
// ---------------------------------------------------------------------------

/// Return every FormKey in `handle_id` whose record signature matches `sig`.
///
/// Uses the `CoreSection` index so no linear scan of root_items is needed on
/// the hot path. Returns an empty `Vec` when the signature is present but has
/// no records; returns `Err(RecordReadError::NotFound)` only when the handle
/// itself does not exist.
///
/// # Errors
/// Returns `RecordReadError::NotFound` when `handle_id` is not a loaded handle.
pub fn iter_form_keys_of_sig(
    handle_id: u64,
    sig: SigCode,
    interner: &StringInterner,
) -> Result<Vec<FormKey>, RecordReadError> {
    let mut store = plugin_handle_store_ref().lock().unwrap();
    let slot = store
        .get_mut(&handle_id)
        .ok_or_else(|| RecordReadError::NotFound(format!("no plugin handle: {handle_id}")))?;

    let core = ensure_core_section(slot);
    let sig_key = SmolStr::new(sig.as_str());
    let out = if let Some(index_form_keys) = core.by_signature_form_keys.get(&sig_key) {
        let mut out = Vec::with_capacity(index_form_keys.len());
        for index_fk in index_form_keys {
            if let Some(entry) = core.by_form_key.get(index_fk) {
                out.push(form_key_from_index_entry(entry, interner));
            }
        }
        out
    } else {
        Vec::new()
    };

    drop(core);
    drop(store);
    Ok(out)
}

/// True when a raw source CELL record has its `DATA` interior flag set
/// (bit 0 = `IsInteriorCell`). Reads the raw `DATA` subrecord's first byte;
/// missing DATA yields `false`. Takes an already-fetched record so callers can
/// classify a whole batch from one snapshot instead of one snapshot per cell
/// (each snapshot deep-clones the localized-strings table).
pub(crate) fn raw_cell_is_interior(raw: &ParsedRecord) -> bool {
    let subrecords = effective_subrecords_for_record(raw);
    subrecords
        .iter()
        .find(|s| s.signature.as_str() == "DATA")
        .and_then(|s| s.data.first())
        .map(|first| first & 0x01 != 0)
        .unwrap_or(false)
}

/// Placed-child object ids for one interior cell, split by section group.
#[derive(Default, Debug)]
pub(crate) struct InteriorCellChildren {
    pub persistent: Vec<u32>,
    pub temporary: Vec<u32>,
}

/// Single pass over the source tree collecting, for every Cell-Children(6)
/// group whose cell object id is in `cell_object_ids`, the Persistent(8) and
/// Temporary(9) child record object ids.
///
/// Child→cell parentage is encoded only by group nesting, so this is the one
/// authoritative walk.
pub(crate) fn collect_interior_cell_children(
    handle_id: u64,
    cell_object_ids: &rustc_hash::FxHashSet<u32>,
) -> Result<rustc_hash::FxHashMap<u32, InteriorCellChildren>, RecordReadError> {
    const CELL_CHILD_GROUP: i32 = 6;
    const PERSISTENT_GROUP: i32 = 8;
    const TEMPORARY_GROUP: i32 = 9;

    let store = plugin_handle_store_ref().lock().unwrap();
    let slot = store
        .get(&handle_id)
        .ok_or_else(|| RecordReadError::NotFound(format!("no plugin handle: {handle_id}")))?;

    fn walk(
        items: &[ParsedItem],
        set: &rustc_hash::FxHashSet<u32>,
        out: &mut rustc_hash::FxHashMap<u32, InteriorCellChildren>,
    ) {
        for item in items {
            let ParsedItem::Group(group) = item else {
                continue;
            };
            if group.group_type == CELL_CHILD_GROUP {
                let cell_obj = u32::from_le_bytes(group.label) & 0x00FF_FFFF;
                if set.contains(&cell_obj) {
                    let entry = out.entry(cell_obj).or_default();
                    for section_item in &group.children {
                        let ParsedItem::Group(section) = section_item else {
                            continue;
                        };
                        let bucket = match section.group_type {
                            PERSISTENT_GROUP => &mut entry.persistent,
                            TEMPORARY_GROUP => &mut entry.temporary,
                            _ => continue,
                        };
                        for child in &section.children {
                            if let ParsedItem::Record(record) = child {
                                bucket.push(record.form_id & 0x00FF_FFFF);
                            }
                        }
                    }
                }
            }
            walk(&group.children, set, out);
        }
    }

    let mut out = rustc_hash::FxHashMap::default();
    walk(&slot.parsed.root_items, cell_object_ids, &mut out);
    Ok(out)
}

fn form_key_from_index_entry(
    entry: &esp_authoring_core::plugin_runtime::RecordIndexEntry,
    interner: &StringInterner,
) -> FormKey {
    FormKey {
        local: entry.object_id,
        plugin: interner.intern(entry.master_plugin.as_ref()),
    }
}

// ---------------------------------------------------------------------------
// Internal codec dispatch
// ---------------------------------------------------------------------------

pub(crate) fn decode_subrecord(
    record_sig: &str,
    subrecord_sig: &str,
    codec: &str,
    data: &[u8],
    masters: &[String],
    plugin_name: &str,
    localized_strings: Option<&LocalizedStringsState>,
    interner: &StringInterner,
) -> Result<FieldValue, DecodeError> {
    // Simple scalar codecs first.
    match codec {
        "zstring" => {
            let s = decode_zstring(data)?;
            Ok(FieldValue::String(interner.intern(&s)))
        }
        "lstring" => {
            // When not localized, lstring is just a zstring.
            // When localized it's a u32 string-table ID. Resolve it to text
            // for conversion outputs; keep the numeric fallback when the
            // string table is unavailable.
            if data.len() == 4 {
                if let Some(strings) = localized_strings {
                    let id = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
                    if id == 0 {
                        return Ok(FieldValue::String(interner.intern("")));
                    }
                    if let Some(text) = resolve_localized_string(strings, id) {
                        return Ok(FieldValue::String(interner.intern(text)));
                    }
                    return Ok(FieldValue::Uint(id as u64));
                }
            }
            let s = decode_zstring(data)?;
            Ok(FieldValue::String(interner.intern(&s)))
        }
        "bool" => {
            if data.is_empty() {
                return Ok(FieldValue::None);
            }
            Ok(FieldValue::Bool(data[0] != 0))
        }
        "int8" => {
            expect_at_least(data, 1)?;
            Ok(FieldValue::Int(data[0] as i8 as i64))
        }
        "uint8" => {
            expect_at_least(data, 1)?;
            Ok(FieldValue::Uint(data[0] as u64))
        }
        "int16" => {
            expect_at_least(data, 2)?;
            Ok(FieldValue::Int(
                i16::from_le_bytes([data[0], data[1]]) as i64
            ))
        }
        "uint16" => {
            expect_at_least(data, 2)?;
            Ok(FieldValue::Uint(
                u16::from_le_bytes([data[0], data[1]]) as u64
            ))
        }
        "int32" => {
            expect_at_least(data, 4)?;
            Ok(FieldValue::Int(
                i32::from_le_bytes([data[0], data[1], data[2], data[3]]) as i64,
            ))
        }
        "uint32" => {
            expect_at_least(data, 4)?;
            Ok(FieldValue::Uint(
                u32::from_le_bytes([data[0], data[1], data[2], data[3]]) as u64,
            ))
        }
        "int64" => {
            expect_at_least(data, 8)?;
            Ok(FieldValue::Int(i64::from_le_bytes(
                data[..8].try_into().unwrap(),
            )))
        }
        "uint64" => {
            expect_at_least(data, 8)?;
            Ok(FieldValue::Uint(u64::from_le_bytes(
                data[..8].try_into().unwrap(),
            )))
        }
        "float32" => {
            expect_at_least(data, 4)?;
            Ok(FieldValue::Float(f32::from_le_bytes([
                data[0], data[1], data[2], data[3],
            ])))
        }
        "formid" => {
            expect_at_least(data, 4)?;
            let raw = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
            if raw == 0 {
                return Ok(FieldValue::None);
            }
            let fk_str = resolve_form_id(raw, masters, plugin_name);
            if fk_str.is_empty() {
                return Ok(FieldValue::None);
            }
            match parse_form_key_str(&fk_str, interner) {
                Some((_, fk)) => Ok(FieldValue::FormKey(fk)),
                None => Err(DecodeError::UnresolvableFormId(raw)),
            }
        }
        "formid_array" => {
            if data.len() % 4 != 0 {
                let bytes: smallvec::SmallVec<[u8; 32]> = data.iter().copied().collect();
                return Ok(FieldValue::Bytes(bytes));
            }

            let mut items = Vec::with_capacity(data.len() / 4);
            for chunk in data.chunks_exact(4) {
                let raw = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
                if raw == 0 {
                    items.push(FieldValue::None);
                    continue;
                }
                let fk_str = resolve_form_id(raw, masters, plugin_name);
                if fk_str.is_empty() {
                    continue;
                }
                match parse_form_key_str(&fk_str, interner) {
                    Some((_, fk)) => items.push(FieldValue::FormKey(fk)),
                    None => return Err(DecodeError::UnresolvableFormId(raw)),
                }
            }
            Ok(FieldValue::List(items))
        }
        "array_struct:I,I,I" if record_sig == "REGN" && subrecord_sig == "RDWT" => {
            decode_regn_rdwt(data, masters, plugin_name, interner)
        }
        "array_struct:I,I,I" if record_sig == "COBJ" && subrecord_sig == "FVPA" => {
            decode_cobj_fvpa(data, masters, plugin_name, interner)
        }
        "array_struct:I,I,I" if record_sig == "LCTN" && subrecord_sig == "LCUN" => {
            decode_lctn_lcun(data, masters, plugin_name, interner)
        }
        "array_struct:I,I,B,B,B,B" if record_sig == "LCTN" && subrecord_sig == "LCEP" => {
            decode_lctn_lcep(data, masters, plugin_name, interner)
        }
        // FO76 LLKC: array_struct:I,I,I (keyword FK, chance, curve_table FK).
        // Drop curve_table (FO4 has no per-keyword curve table) and decode to
        // List[Struct[(keyword=FK, chance=Uint)]] so the keyword FK is remapped
        // 00→07 by the normal FormKey walker. Field names match fo4.rs LLKC
        // schema: filter_keyword_chances_keyword / filter_keyword_chances_chance.
        "array_struct:I,I,I" if record_sig == "LVLI" && subrecord_sig == "LLKC" => {
            decode_lvli_llkc_fo76(data, masters, plugin_name, interner)
        }
        // FO4 LLKC: array_struct:I,I (keyword FK, chance). No curve_table.
        "array_struct:I,I" if record_sig == "LVLI" && subrecord_sig == "LLKC" => {
            decode_lvli_llkc_fo4(data, masters, plugin_name, interner)
        }
        other if other.starts_with("struct:") => {
            if record_sig == "REGN"
                && subrecord_sig == "RDOT"
                && (data.len() % 74 == 0 || data.len() % 76 == 0)
                && let Some(value) =
                    crate::fo76_rdot::decode_fo76_regn_rdot(data, masters, plugin_name, interner)
            {
                return Ok(value);
            }
            // Struct codecs: emit raw bytes; full struct decode is not implemented yet.
            let bytes: smallvec::SmallVec<[u8; 32]> = data.iter().copied().collect();
            Ok(FieldValue::Bytes(bytes))
        }
        other => Err(DecodeError::UnknownCodec(other.to_string())),
    }
}

const REGN_RDWT_ROW_LEN: usize = 12;

fn decode_regn_rdwt(
    data: &[u8],
    masters: &[String],
    plugin_name: &str,
    interner: &StringInterner,
) -> Result<FieldValue, DecodeError> {
    if data.len() % REGN_RDWT_ROW_LEN != 0 {
        let bytes: smallvec::SmallVec<[u8; 32]> = data.iter().copied().collect();
        return Ok(FieldValue::Bytes(bytes));
    }

    let weather_sym = interner.intern("WeatherTypesWeather");
    let chance_sym = interner.intern("WeatherTypesChance");
    let global_sym = interner.intern("WeatherTypesGlobal");
    let mut rows = Vec::with_capacity(data.len() / REGN_RDWT_ROW_LEN);
    for row in data.chunks_exact(REGN_RDWT_ROW_LEN) {
        let raw_weather = u32::from_le_bytes(row[0..4].try_into().unwrap());
        let chance = u32::from_le_bytes(row[4..8].try_into().unwrap());
        let raw_global = u32::from_le_bytes(row[8..12].try_into().unwrap());

        rows.push(FieldValue::Struct(vec![
            (
                weather_sym,
                decode_nullable_form_id_value(raw_weather, masters, plugin_name, interner)?,
            ),
            (chance_sym, FieldValue::Uint(chance as u64)),
            (
                global_sym,
                decode_nullable_form_id_value(raw_global, masters, plugin_name, interner)?,
            ),
        ]));
    }

    Ok(FieldValue::List(rows))
}

const COBJ_FVPA_FO76_ROW_LEN: usize = 12;

/// FO76 COBJ `FVPA` rows are `I,I,I` (component FormID, count, curve_table→CURV);
/// FO4's are `I,I` (component, count) — FO4 has no per-component curve table.
/// Decode to a List of `{component, count}` structs, DROPPING the FO76
/// curve_table dword, so the FO4 encoder re-packs clean 8-byte rows and the
/// component FormKey is remapped via the normal FormKey path (not the raw
/// byte-offset struct-FK remap, which would mis-stride the 12→8 byte rows).
fn decode_cobj_fvpa(
    data: &[u8],
    masters: &[String],
    plugin_name: &str,
    interner: &StringInterner,
) -> Result<FieldValue, DecodeError> {
    if data.is_empty() || data.len() % COBJ_FVPA_FO76_ROW_LEN != 0 {
        let bytes: smallvec::SmallVec<[u8; 32]> = data.iter().copied().collect();
        return Ok(FieldValue::Bytes(bytes));
    }

    let component_sym = interner.intern("components_component");
    let count_sym = interner.intern("components_count");
    let mut rows = Vec::with_capacity(data.len() / COBJ_FVPA_FO76_ROW_LEN);
    for row in data.chunks_exact(COBJ_FVPA_FO76_ROW_LEN) {
        let raw_component = u32::from_le_bytes(row[0..4].try_into().unwrap());
        let count = u32::from_le_bytes(row[4..8].try_into().unwrap());
        rows.push(FieldValue::Struct(vec![
            (
                component_sym,
                decode_nullable_form_id_value(raw_component, masters, plugin_name, interner)?,
            ),
            (count_sym, FieldValue::Uint(count as u64)),
        ]));
    }

    Ok(FieldValue::List(rows))
}

const LCTN_LCEP_ROW_LEN: usize = 12; // I,I,B,B,B,B
const LCTN_LCUN_ROW_LEN: usize = 12; // I,I,I

/// FO76 LCTN `LCEP` "Master Enable Parent References" rows are `I,I,B,B,B,B`
/// (Ref FK, EnableParent FK, flags + 3 unknown bytes) — identical layout to FO4.
/// `array_struct:` codecs are NOT decoded by the generic decoder (it only handles
/// `struct:`), so without this special-case LCEP falls through to a raw `Bytes`
/// blob and its two FormKeys are never remapped 00→07 (the struct-FK byte-offset
/// remap also skips it: `struct_field_layout` returns empty for `array_struct:`).
/// Decoding to a List of Structs with the FK fields as `FormKey` routes them
/// through the normal FormKey remap. Same pattern as `decode_cobj_fvpa` /
/// `decode_regn_rdwt`.
///
/// Every emitted struct field MUST be named to match the FO4 target schema's
/// field ids, because `target_normalize::normalize_struct_pairs` DROPS any
/// decoded struct field whose name matches no target field. The schema models
/// the 4 trailing bytes as 4 SEPARATE uint8 fields (`flags`, `unknown_u8_3/4/5`),
/// emitted as 4 `Uint(u8)` fields with those exact ids — not one combined
/// `Bytes` blob, which normalize would drop, collapsing 12B rows to 8B.
/// Emitting `Uint`s is safe: normalize pre-encodes each scalar to its target
/// field width via `encode_fixed_scalar` (uint8 → 1 byte) before the generic
/// Struct encoder runs, so each row re-encodes to exactly 12B.
fn decode_lctn_lcep(
    data: &[u8],
    masters: &[String],
    plugin_name: &str,
    interner: &StringInterner,
) -> Result<FieldValue, DecodeError> {
    decode_lctn_enable_parent_rows(
        data,
        masters,
        plugin_name,
        interner,
        "master_enable_parent_references_ref",
        "master_enable_parent_references_enable_parent",
        "master_enable_parent_references_flags",
    )
}

/// Shared decoder for LCEP (`master_*`) and ACEP (`added_*`) — identical
/// `I,I,B,B,B,B` layout, differing only in the schema field-id prefix.
fn decode_lctn_enable_parent_rows(
    data: &[u8],
    masters: &[String],
    plugin_name: &str,
    interner: &StringInterner,
    ref_id: &str,
    parent_id: &str,
    flags_id: &str,
) -> Result<FieldValue, DecodeError> {
    if data.is_empty() || data.len() % LCTN_LCEP_ROW_LEN != 0 {
        let bytes: smallvec::SmallVec<[u8; 32]> = data.iter().copied().collect();
        return Ok(FieldValue::Bytes(bytes));
    }
    let ref_sym = interner.intern(ref_id);
    let parent_sym = interner.intern(parent_id);
    let flags_sym = interner.intern(flags_id);
    // The 3 generic trailing-byte field ids are shared across record types; they
    // are matched within THIS subrecord's field list by normalize, so they are
    // unambiguous here.
    let u8_3_sym = interner.intern("unknown_u8_3");
    let u8_4_sym = interner.intern("unknown_u8_4");
    let u8_5_sym = interner.intern("unknown_u8_5");
    let mut rows = Vec::with_capacity(data.len() / LCTN_LCEP_ROW_LEN);
    for row in data.chunks_exact(LCTN_LCEP_ROW_LEN) {
        let raw_ref = u32::from_le_bytes(row[0..4].try_into().unwrap());
        let raw_parent = u32::from_le_bytes(row[4..8].try_into().unwrap());
        rows.push(FieldValue::Struct(vec![
            (
                ref_sym,
                decode_nullable_form_id_value(raw_ref, masters, plugin_name, interner)?,
            ),
            (
                parent_sym,
                decode_nullable_form_id_value(raw_parent, masters, plugin_name, interner)?,
            ),
            (flags_sym, FieldValue::Uint(u64::from(row[8]))),
            (u8_3_sym, FieldValue::Uint(u64::from(row[9]))),
            (u8_4_sym, FieldValue::Uint(u64::from(row[10]))),
            (u8_5_sym, FieldValue::Uint(u64::from(row[11]))),
        ]));
    }
    Ok(FieldValue::List(rows))
}

/// FO76 LCTN `LCUN` "Master Unique NPCs" rows are `I,I,I` (NPC FK, ActorRef FK,
/// Location FK) — identical layout to FO4. Decode to a List of Structs so the
/// three FormKeys are remapped 00→07 via the normal FormKey path (see
/// `decode_lctn_lcep` for why `array_struct:` otherwise stays raw Bytes).
fn decode_lctn_lcun(
    data: &[u8],
    masters: &[String],
    plugin_name: &str,
    interner: &StringInterner,
) -> Result<FieldValue, DecodeError> {
    if data.is_empty() || data.len() % LCTN_LCUN_ROW_LEN != 0 {
        let bytes: smallvec::SmallVec<[u8; 32]> = data.iter().copied().collect();
        return Ok(FieldValue::Bytes(bytes));
    }
    let npc_sym = interner.intern("master_unique_npcs_npc");
    let actor_sym = interner.intern("master_unique_npcs_actor_ref");
    let loc_sym = interner.intern("master_unique_npcs_location");
    let mut rows = Vec::with_capacity(data.len() / LCTN_LCUN_ROW_LEN);
    for row in data.chunks_exact(LCTN_LCUN_ROW_LEN) {
        let raw_npc = u32::from_le_bytes(row[0..4].try_into().unwrap());
        let raw_actor = u32::from_le_bytes(row[4..8].try_into().unwrap());
        let raw_loc = u32::from_le_bytes(row[8..12].try_into().unwrap());
        rows.push(FieldValue::Struct(vec![
            (
                npc_sym,
                decode_nullable_form_id_value(raw_npc, masters, plugin_name, interner)?,
            ),
            (
                actor_sym,
                decode_nullable_form_id_value(raw_actor, masters, plugin_name, interner)?,
            ),
            (
                loc_sym,
                decode_nullable_form_id_value(raw_loc, masters, plugin_name, interner)?,
            ),
        ]));
    }
    Ok(FieldValue::List(rows))
}

const LVLI_LLKC_FO76_ROW_LEN: usize = 12; // I,I,I
const LVLI_LLKC_FO4_ROW_LEN: usize = 8; // I,I

/// FO76 LVLI `LLKC` "Filter Keyword Chances" rows are `I,I,I` (keyword FK,
/// chance u32, curve_table FK). FO4's schema has `I,I` (keyword, chance) — no
/// per-keyword curve table. Decode to List[Struct[(keyword=FK, chance=Uint)]],
/// dropping the curve_table so the keyword FormKey is remapped 00→07 via the
/// normal FormKey walker (same pattern as `decode_cobj_fvpa`).
///
/// Field names must match fo4.rs LLKC schema ids exactly, otherwise
/// `target_normalize::normalize_struct_pairs` silently drops them and corrupts
/// the output row stride.
fn decode_lvli_llkc_fo76(
    data: &[u8],
    masters: &[String],
    plugin_name: &str,
    interner: &StringInterner,
) -> Result<FieldValue, DecodeError> {
    if data.is_empty() || data.len() % LVLI_LLKC_FO76_ROW_LEN != 0 {
        let bytes: smallvec::SmallVec<[u8; 32]> = data.iter().copied().collect();
        return Ok(FieldValue::Bytes(bytes));
    }
    let keyword_sym = interner.intern("filter_keyword_chances_keyword");
    let chance_sym = interner.intern("filter_keyword_chances_chance");
    let mut rows = Vec::with_capacity(data.len() / LVLI_LLKC_FO76_ROW_LEN);
    for row in data.chunks_exact(LVLI_LLKC_FO76_ROW_LEN) {
        let raw_keyword = u32::from_le_bytes(row[0..4].try_into().unwrap());
        let chance = u32::from_le_bytes(row[4..8].try_into().unwrap());
        // row[8..12] is curve_table FK — dropped (FO4 has no curve table per keyword)
        rows.push(FieldValue::Struct(vec![
            (
                keyword_sym,
                decode_nullable_form_id_value(raw_keyword, masters, plugin_name, interner)?,
            ),
            (chance_sym, FieldValue::Uint(chance as u64)),
        ]));
    }
    Ok(FieldValue::List(rows))
}

/// FO4 LVLI `LLKC` "Filter Keyword Chances" rows are `I,I` (keyword FK,
/// chance u32). Decode identically to the FO76 variant minus the curve_table
/// drop, using the same target schema field names.
fn decode_lvli_llkc_fo4(
    data: &[u8],
    masters: &[String],
    plugin_name: &str,
    interner: &StringInterner,
) -> Result<FieldValue, DecodeError> {
    if data.is_empty() || data.len() % LVLI_LLKC_FO4_ROW_LEN != 0 {
        let bytes: smallvec::SmallVec<[u8; 32]> = data.iter().copied().collect();
        return Ok(FieldValue::Bytes(bytes));
    }
    let keyword_sym = interner.intern("filter_keyword_chances_keyword");
    let chance_sym = interner.intern("filter_keyword_chances_chance");
    let mut rows = Vec::with_capacity(data.len() / LVLI_LLKC_FO4_ROW_LEN);
    for row in data.chunks_exact(LVLI_LLKC_FO4_ROW_LEN) {
        let raw_keyword = u32::from_le_bytes(row[0..4].try_into().unwrap());
        let chance = u32::from_le_bytes(row[4..8].try_into().unwrap());
        rows.push(FieldValue::Struct(vec![
            (
                keyword_sym,
                decode_nullable_form_id_value(raw_keyword, masters, plugin_name, interner)?,
            ),
            (chance_sym, FieldValue::Uint(chance as u64)),
        ]));
    }
    Ok(FieldValue::List(rows))
}

fn decode_nullable_form_id_value(
    raw: u32,
    masters: &[String],
    plugin_name: &str,
    interner: &StringInterner,
) -> Result<FieldValue, DecodeError> {
    if raw == 0 {
        return Ok(FieldValue::Uint(0));
    }
    let fk_str = resolve_form_id(raw, masters, plugin_name);
    if fk_str.is_empty() {
        return Ok(FieldValue::Uint(0));
    }
    parse_form_key_str(&fk_str, interner)
        .map(|(_, fk)| FieldValue::FormKey(fk))
        .ok_or(DecodeError::UnresolvableFormId(raw))
}

fn resolve_localized_string<'a>(
    strings: &'a LocalizedStringsState,
    string_id: u32,
) -> Option<&'a str> {
    let default_language = strings.default_language.trim();
    if !default_language.is_empty() {
        if let Some(text) = strings
            .by_language
            .get(default_language)
            .and_then(|table| table.get(&string_id))
        {
            return Some(text.as_str());
        }
    }
    if default_language != "en" {
        if let Some(text) = strings
            .by_language
            .get("en")
            .and_then(|table| table.get(&string_id))
        {
            return Some(text.as_str());
        }
    }
    let mut languages: Vec<&String> = strings.by_language.keys().collect();
    languages.sort();
    for language in languages {
        if let Some(text) = strings
            .by_language
            .get(language)
            .and_then(|table| table.get(&string_id))
        {
            return Some(text.as_str());
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn decode_zstring(data: &[u8]) -> Result<String, DecodeError> {
    // Strip trailing NUL(s) and decode as UTF-8 with CP1252 fallback.
    let end = data.iter().position(|&b| b == 0).unwrap_or(data.len());
    let slice = &data[..end];
    // Try UTF-8 first; fall back to Latin-1 (same as Windows-1252 for printable range).
    match std::str::from_utf8(slice) {
        Ok(s) => Ok(s.to_string()),
        Err(_) => Ok(slice.iter().map(|&b| b as char).collect()),
    }
}

fn expect_at_least(data: &[u8], n: usize) -> Result<(), DecodeError> {
    if data.len() < n {
        Err(DecodeError::Truncated {
            expected: n,
            got: data.len(),
        })
    } else {
        Ok(())
    }
}

/// Resolve a raw 32-bit FormID to a "Plugin:XXXXXX" string using the plugin's
/// master list.
fn resolve_form_id(raw: u32, masters: &[String], plugin_name: &str) -> String {
    if raw == 0 {
        return String::new();
    }
    let master_index = ((raw >> 24) & 0xFF) as usize;
    let object_id = raw & 0x00FF_FFFF;
    let own_index = masters.len();
    let plugin = if master_index < masters.len() {
        masters[master_index].as_str()
    } else if master_index == own_index || master_index == 0xFF {
        plugin_name
    } else {
        plugin_name
    };
    format!("{plugin}:{object_id:06X}")
}

/// Parse a "Plugin:XXXXXX" or "XXXXXX@Plugin" form key string into a
/// `conversion::ids::FormKey`. Returns a `(String, FormKey)` pair where the
/// string is the interned plugin name.
pub(crate) fn parse_form_key_str(s: &str, interner: &StringInterner) -> Option<(String, FormKey)> {
    // Support both "Plugin:XXXXXX" and "XXXXXX@Plugin" formats.
    if let Some((plugin, hex)) = s.rsplit_once(':') {
        if let Ok(local) = u32::from_str_radix(hex.trim(), 16) {
            let plugin = plugin.trim().to_string();
            let sym = interner.intern(&plugin);
            return Some((plugin, FormKey { local, plugin: sym }));
        }
    }
    if let Some((hex, plugin)) = s.split_once('@') {
        if let Ok(local) = u32::from_str_radix(hex.trim(), 16) {
            let plugin = plugin.trim().to_string();
            let sym = interner.intern(&plugin);
            return Some((plugin, FormKey { local, plugin: sym }));
        }
    }
    None
}

/// Convert a "Plugin:XXXXXX" rendered form_key (plugin_index style) to a
/// `conversion::ids::FormKey` (used throughout the converter).
fn parse_form_key_from_render(
    render: &str,
    interner: &StringInterner,
) -> Result<FormKey, RecordReadError> {
    parse_form_key_str(render, interner)
        .map(|(_, fk)| fk)
        .ok_or_else(|| RecordReadError::InvalidFormKey(render.to_string()))
}

/// Convert a `conversion::ids::FormKey` to the "Plugin:XXXXXX" string that
/// `read_record` accepts as its `form_key_str` argument.
///
/// Returns an empty string when the form_key's plugin `Sym` cannot be resolved
/// (should not happen within a single conversion run).
pub fn form_key_to_read_str(fk: &FormKey, interner: &StringInterner) -> String {
    match interner.resolve(fk.plugin) {
        Some(plugin) => format!("{plugin}:{:06X}", fk.local),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;
    use esp_authoring_core::plugin_runtime::ParsedSubrecord;

    fn parsed_subrecord(signature: &str, data: Vec<u8>) -> ParsedSubrecord {
        ParsedSubrecord {
            signature: SmolStr::new(signature),
            data: Bytes::from(data),
            semantic_type: None,
        }
    }

    #[test]
    fn normalized_index_plugin_names_are_interned_across_case_variants() {
        let original = normalized_index_plugin_name("SeventySix.esm");
        let mixed = normalized_index_plugin_name("seventysix.ESM");

        assert_eq!(original.as_ref(), "seventysix.esm");
        assert!(Arc::ptr_eq(&original, &mixed));
    }

    #[test]
    fn fo76_term_znam_marker_parameters_stay_raw_bytes() {
        let mut marker_parameters = Vec::new();
        marker_parameters.extend_from_slice(&1.0_f32.to_le_bytes());
        marker_parameters.extend_from_slice(&(-59.0_f32).to_le_bytes());
        marker_parameters.extend_from_slice(&1.0_f32.to_le_bytes());
        marker_parameters.extend_from_slice(&0.0_f32.to_le_bytes());
        marker_parameters.extend_from_slice(&0_u32.to_le_bytes());
        marker_parameters.extend_from_slice(&[0xFF, 1, 0, 0]);
        let raw_record = ParsedRecord {
            signature: SmolStr::new_static("TERM"),
            form_id: 0x0072_6E6C,
            flags: 0,
            version_control: 0,
            form_version: Some(208),
            version2: Some(1),
            subrecords: vec![
                parsed_subrecord("EDID", b"Storm_UpperAtrium_ClinicTerminal\0".to_vec()),
                parsed_subrecord("XMRK", b"Markers\\MarkerDeskTerminal3rdP.nif\0".to_vec()),
                parsed_subrecord("ZNAM", marker_parameters.clone()),
            ],
            raw_payload: None,
            parse_error: None,
        };
        let schema = AuthoringSchema::for_game("fo76").expect("fo76 schema");
        let interner = StringInterner::new();
        let fk = FormKey {
            local: 0x0072_6E6C,
            plugin: interner.intern("SeventySix.esm"),
        };

        let record = decode_record_from_parsed(
            &raw_record,
            &fk,
            &schema,
            &[],
            "SeventySix.esm",
            None,
            false,
            &interner,
        )
        .unwrap();
        let znam = record
            .fields
            .iter()
            .find(|field| field.sig.as_str() == "ZNAM")
            .expect("ZNAM marker parameters");
        let FieldValue::Bytes(bytes) = &znam.value else {
            panic!("TERM ZNAM must not decode as a FormKey: {:?}", znam.value);
        };
        assert_eq!(bytes.as_slice(), marker_parameters.as_slice());
    }

    #[test]
    fn fo4_term_distinguishes_sound_from_marker_snam_by_position() {
        let mut marker_parameters = Vec::new();
        marker_parameters.extend_from_slice(&1.0_f32.to_le_bytes());
        marker_parameters.extend_from_slice(&(-59.0_f32).to_le_bytes());
        marker_parameters.extend_from_slice(&1.0_f32.to_le_bytes());
        marker_parameters.extend_from_slice(&0.0_f32.to_le_bytes());
        marker_parameters.extend_from_slice(&0_u32.to_le_bytes());
        marker_parameters.extend_from_slice(&[0xFF, 1, 0, 0]);

        for marker in [marker_parameters, vec![0; 24]] {
            let raw_record = ParsedRecord {
                signature: SmolStr::new_static("TERM"),
                form_id: 0x0072_6E6C,
                flags: 0,
                version_control: 0,
                form_version: Some(131),
                version2: Some(1),
                subrecords: vec![
                    parsed_subrecord("EDID", b"TestTerminal\0".to_vec()),
                    parsed_subrecord("SNAM", 0x0009_80FB_u32.to_le_bytes().to_vec()),
                    parsed_subrecord("XMRK", b"Markers\\MarkerDeskTerminal3rdP.nif\0".to_vec()),
                    parsed_subrecord("SNAM", marker.clone()),
                ],
                raw_payload: None,
                parse_error: None,
            };
            let schema = AuthoringSchema::for_game("fo4").expect("fo4 schema");
            let interner = StringInterner::new();
            let fk = FormKey {
                local: 0x0072_6E6C,
                plugin: interner.intern("SeventySix.esm"),
            };

            let record = decode_record_from_parsed(
                &raw_record,
                &fk,
                &schema,
                &[],
                "SeventySix.esm",
                None,
                false,
                &interner,
            )
            .unwrap();
            let snam = record
                .fields
                .iter()
                .filter(|field| field.sig.as_str() == "SNAM")
                .collect::<Vec<_>>();

            assert_eq!(snam.len(), 2);
            assert!(matches!(snam[0].value, FieldValue::FormKey(_)));
            let FieldValue::Bytes(bytes) = &snam[1].value else {
                panic!(
                    "post-XMRK TERM SNAM must stay marker bytes: {:?}",
                    snam[1].value
                );
            };
            assert_eq!(bytes.as_slice(), marker.as_slice());
        }
    }

    #[test]
    fn decode_zstring_strips_null_terminator() {
        let data = b"TestWeap\x00";
        assert_eq!(decode_zstring(data).unwrap(), "TestWeap");
    }

    #[test]
    fn decode_lcep_yields_list_of_structs_with_formkey_leaves() {
        let mut interner = StringInterner::new();
        // one 12-byte LCEP row: Ref=0x0018116E, EnableParent=0x00000000 (NULL),
        // then 4 flag bytes 01 02 03 04.
        let mut data = Vec::new();
        data.extend_from_slice(&0x0018_116Eu32.to_le_bytes());
        data.extend_from_slice(&0u32.to_le_bytes());
        data.extend_from_slice(&[1, 2, 3, 4]);
        let v = decode_subrecord(
            "LCTN",
            "LCEP",
            "array_struct:I,I,B,B,B,B",
            &data,
            &[],
            "SeventySix.esm",
            None,
            &mut interner,
        )
        .unwrap();
        let FieldValue::List(rows) = v else {
            panic!("LCEP must decode to List, got non-List")
        };
        assert_eq!(rows.len(), 1);
        let FieldValue::Struct(fields) = &rows[0] else {
            panic!("row must be Struct")
        };
        // 6 fields: Ref FK + EnableParent FK + 4 schema-named uint8 (flags +
        // unknown_u8_3/4/5). The tail is NOT one combined Bytes blob — that name
        // (`flag_bytes`) matched no FO4 schema field so target_normalize dropped
        // it, collapsing 12B rows and malforming LCEP.
        assert_eq!(
            fields.len(),
            6,
            "Ref FK + EnableParent FK + 4 uint8 tail fields"
        );
        // Ref resolved to a FormKey (source-local id 0x18116E -> own plugin).
        assert!(
            matches!(fields[0].1, FieldValue::FormKey(_)),
            "Ref is a FormKey leaf for #28 remap"
        );
        // NULL EnableParent decodes to Uint(0) (re-encodes as 4 zero bytes).
        assert!(matches!(fields[1].1, FieldValue::Uint(0)));
        // Tail bytes 01 02 03 04 -> four Uint(u8) fields matching the schema ids,
        // so normalize keeps them and re-encodes each to 1 byte (uint8 codec).
        assert!(matches!(fields[2].1, FieldValue::Uint(1)), "flags = 0x01");
        assert!(
            matches!(fields[3].1, FieldValue::Uint(2)),
            "unknown_u8_3 = 0x02"
        );
        assert!(
            matches!(fields[4].1, FieldValue::Uint(3)),
            "unknown_u8_4 = 0x03"
        );
        assert!(
            matches!(fields[5].1, FieldValue::Uint(4)),
            "unknown_u8_5 = 0x04"
        );
        // Field names must match the FO4 schema field ids (else normalize drops them).
        assert_eq!(
            interner.resolve(fields[2].0),
            Some("master_enable_parent_references_flags")
        );
        assert_eq!(interner.resolve(fields[3].0), Some("unknown_u8_3"));
    }

    #[test]
    fn decode_lcun_yields_list_of_three_formkey_structs() {
        let mut interner = StringInterner::new();
        // one 12-byte LCUN row: NPC=0x0018928E, ActorRef=0x00189250, Location=0.
        let mut data = Vec::new();
        data.extend_from_slice(&0x0018_928Eu32.to_le_bytes());
        data.extend_from_slice(&0x0018_9250u32.to_le_bytes());
        data.extend_from_slice(&0u32.to_le_bytes());
        let v = decode_subrecord(
            "LCTN",
            "LCUN",
            "array_struct:I,I,I",
            &data,
            &[],
            "SeventySix.esm",
            None,
            &mut interner,
        )
        .unwrap();
        let FieldValue::List(rows) = v else {
            panic!("LCUN must decode to List")
        };
        let FieldValue::Struct(fields) = &rows[0] else {
            panic!("row must be Struct")
        };
        assert_eq!(fields.len(), 3);
        assert!(matches!(fields[0].1, FieldValue::FormKey(_)), "NPC FK leaf");
        assert!(
            matches!(fields[1].1, FieldValue::FormKey(_)),
            "ActorRef FK leaf"
        );
        assert!(matches!(fields[2].1, FieldValue::Uint(0)), "NULL Location");
    }

    #[test]
    fn decode_lcep_malformed_length_falls_back_to_bytes() {
        let mut interner = StringInterner::new();
        let data = vec![0u8; 7]; // not a multiple of 12
        let v = decode_subrecord(
            "LCTN",
            "LCEP",
            "array_struct:I,I,B,B,B,B",
            &data,
            &[],
            "SeventySix.esm",
            None,
            &mut interner,
        )
        .unwrap();
        assert!(
            matches!(v, FieldValue::Bytes(_)),
            "malformed row count -> raw Bytes, never panic"
        );
    }

    #[test]
    fn decode_zstring_no_null_is_fine() {
        let data = b"Hello";
        assert_eq!(decode_zstring(data).unwrap(), "Hello");
    }

    #[test]
    fn decode_zstring_empty_is_empty_string() {
        assert_eq!(decode_zstring(&[]).unwrap(), "");
    }

    #[test]
    fn decode_zstring_nul_only_is_empty() {
        assert_eq!(decode_zstring(&[0]).unwrap(), "");
    }

    #[test]
    fn decode_pack_package_data_cnam_keeps_raw_payloads() {
        let schema = AuthoringSchema::for_game("fo76").expect("fo76 schema");
        let interner = StringInterner::new();
        let fk = FormKey {
            local: 0x001234,
            plugin: interner.intern("SeventySix.esm"),
        };
        let raw_record = ParsedRecord {
            signature: SmolStr::new("PACK"),
            form_id: 0x0000_1234,
            flags: 0,
            version_control: 0,
            form_version: Some(155),
            version2: None,
            subrecords: vec![
                parsed_subrecord("EDID", b"TestPackage\0".to_vec()),
                parsed_subrecord("PKCU", 2_u32.to_le_bytes().repeat(3)),
                parsed_subrecord("ANAM", b"Float\0".to_vec()),
                parsed_subrecord("CNAM", 12.5_f32.to_le_bytes().to_vec()),
                parsed_subrecord("ANAM", b"Bool\0".to_vec()),
                parsed_subrecord("CNAM", vec![1]),
                parsed_subrecord("XNAM", vec![0]),
            ],
            raw_payload: None,
            parse_error: None,
        };

        let record = decode_record_from_parsed(
            &raw_record,
            &fk,
            &schema,
            &[],
            "SeventySix.esm",
            None,
            false,
            &interner,
        )
        .unwrap();

        let cnam_payloads: Vec<Vec<u8>> = record
            .fields
            .iter()
            .filter(|field| field.sig.as_str() == "CNAM")
            .map(|field| match &field.value {
                FieldValue::Bytes(bytes) => bytes.to_vec(),
                other => panic!("PACK package-data CNAM should stay raw, got {other:?}"),
            })
            .collect();
        assert_eq!(
            cnam_payloads,
            vec![12.5_f32.to_le_bytes().to_vec(), vec![1]]
        );
    }

    #[test]
    fn decode_pack_procedure_tree_pnam_keeps_raw_payloads() {
        let schema = AuthoringSchema::for_game("fo76").expect("fo76 schema");
        let interner = StringInterner::new();
        let fk = FormKey {
            local: 0x001235,
            plugin: interner.intern("SeventySix.esm"),
        };
        let raw_record = ParsedRecord {
            signature: SmolStr::new("PACK"),
            form_id: 0x0000_1235,
            flags: 0,
            version_control: 0,
            form_version: Some(155),
            version2: None,
            subrecords: vec![
                parsed_subrecord("EDID", b"TestPackage\0".to_vec()),
                parsed_subrecord("PKCU", 0_u32.to_le_bytes().repeat(3)),
                parsed_subrecord("XNAM", vec![0]),
                parsed_subrecord("ANAM", b"Procedure\0".to_vec()),
                parsed_subrecord("PRCB", vec![0; 8]),
                parsed_subrecord("PNAM", b"Patrol\0".to_vec()),
                parsed_subrecord("FNAM", vec![0; 4]),
            ],
            raw_payload: None,
            parse_error: None,
        };

        let record = decode_record_from_parsed(
            &raw_record,
            &fk,
            &schema,
            &[],
            "SeventySix.esm",
            None,
            false,
            &interner,
        )
        .unwrap();

        let pnam = record
            .fields
            .iter()
            .find(|field| field.sig.as_str() == "PNAM")
            .expect("PNAM");
        match &pnam.value {
            FieldValue::Bytes(bytes) => assert_eq!(bytes.as_slice(), b"Patrol\0"),
            other => panic!("PACK procedure-tree PNAM should stay raw, got {other:?}"),
        }
    }

    #[test]
    fn decode_lstring_resolves_loaded_localized_string() {
        let mut interner = StringInterner::new();
        let mut strings = LocalizedStringsState::default();
        strings.default_language = "en".to_string();
        strings
            .by_language
            .entry("en".to_string())
            .or_default()
            .insert(0x0003_4695, "Resolved Name".to_string());

        let data = 0x0003_4695u32.to_le_bytes();
        let value = decode_subrecord(
            "WEAP",
            "FULL",
            "lstring",
            &data,
            &[],
            "test.esm",
            Some(&strings),
            &mut interner,
        )
        .unwrap();

        match value {
            FieldValue::String(sym) => {
                assert_eq!(interner.resolve(sym), Some("Resolved Name"));
            }
            other => panic!("expected resolved string, got {other:?}"),
        }
    }

    #[test]
    fn decode_nonlocalized_four_byte_lstring_as_inline_text() {
        let mut interner = StringInterner::new();

        let value = decode_subrecord(
            "MISC",
            "FULL",
            "lstring",
            b"Cup\0",
            &[],
            "FalloutNV.esm",
            None,
            &mut interner,
        )
        .unwrap();

        match value {
            FieldValue::String(sym) => assert_eq!(interner.resolve(sym), Some("Cup")),
            other => panic!("expected inline string, got {other:?}"),
        }
    }

    #[test]
    fn decode_lstring_keeps_unresolved_localized_id() {
        let mut interner = StringInterner::new();
        let strings = LocalizedStringsState {
            default_language: "en".to_string(),
            ..LocalizedStringsState::default()
        };

        let data = 0x0003_4695u32.to_le_bytes();
        let value = decode_subrecord(
            "WEAP",
            "FULL",
            "lstring",
            &data,
            &[],
            "test.esm",
            Some(&strings),
            &mut interner,
        )
        .unwrap();

        assert_eq!(value, FieldValue::Uint(0x0003_4695));
    }

    #[test]
    fn decode_uint32_round_trip() {
        let data = 42u32.to_le_bytes();
        let mut interner = StringInterner::new();
        let v = decode_subrecord(
            "WEAP",
            "DATA",
            "uint32",
            &data,
            &[],
            "test.esm",
            None,
            &mut interner,
        )
        .unwrap();
        assert_eq!(v, FieldValue::Uint(42));
    }

    #[test]
    fn decode_int32_round_trip() {
        let data = (-7i32).to_le_bytes();
        let mut interner = StringInterner::new();
        let v = decode_subrecord(
            "WEAP",
            "DATA",
            "int32",
            &data,
            &[],
            "test.esm",
            None,
            &mut interner,
        )
        .unwrap();
        assert_eq!(v, FieldValue::Int(-7));
    }

    #[test]
    fn decode_float32_round_trip() {
        let data = 1.5f32.to_le_bytes();
        let mut interner = StringInterner::new();
        let v = decode_subrecord(
            "WEAP",
            "DATA",
            "float32",
            &data,
            &[],
            "test.esm",
            None,
            &mut interner,
        )
        .unwrap();
        assert_eq!(v, FieldValue::Float(1.5));
    }

    #[test]
    fn decode_bool_true_and_false() {
        let mut interner = StringInterner::new();
        assert_eq!(
            decode_subrecord(
                "WEAP",
                "DATA",
                "bool",
                &[1],
                &[],
                "test.esm",
                None,
                &mut interner
            )
            .unwrap(),
            FieldValue::Bool(true)
        );
        assert_eq!(
            decode_subrecord(
                "WEAP",
                "DATA",
                "bool",
                &[0],
                &[],
                "test.esm",
                None,
                &mut interner
            )
            .unwrap(),
            FieldValue::Bool(false)
        );
    }

    #[test]
    fn unknown_codec_returns_error() {
        let mut interner = StringInterner::new();
        let err = decode_subrecord(
            "WEAP",
            "DATA",
            "exotic_codec_xyz",
            &[],
            &[],
            "test.esm",
            None,
            &mut interner,
        );
        assert!(matches!(err, Err(DecodeError::UnknownCodec(_))));
    }

    #[test]
    fn resolve_form_id_own_record() {
        let masters = vec!["Fallout4.esm".to_string()];
        // Master index 0x01 = own plugin (masters.len() == 1 → own_index == 1)
        let s = resolve_form_id(0x01000800, &masters, "MyMod.esp");
        assert_eq!(s, "MyMod.esp:000800");
    }

    #[test]
    fn resolve_form_id_master_record() {
        let masters = vec!["Fallout4.esm".to_string()];
        let s = resolve_form_id(0x001ABCDE, &masters, "MyMod.esp");
        assert_eq!(s, "Fallout4.esm:1ABCDE");
    }

    #[test]
    fn resolve_form_id_zero_returns_empty() {
        let s = resolve_form_id(0, &[], "test.esm");
        assert!(s.is_empty());
    }

    #[test]
    fn formid_array_decodes_to_formkey_list() {
        let masters = vec!["Fallout4.esm".to_string()];
        let mut data = Vec::new();
        data.extend_from_slice(&0x0012_3456_u32.to_le_bytes());
        data.extend_from_slice(&0_u32.to_le_bytes());
        data.extend_from_slice(&0x0100_0800_u32.to_le_bytes());
        let mut interner = StringInterner::new();

        let value = decode_subrecord(
            "WEAP",
            "KWDA",
            "formid_array",
            &data,
            &masters,
            "MyMod.esp",
            None,
            &mut interner,
        )
        .unwrap();

        let FieldValue::List(items) = value else {
            panic!("formid_array should decode to a list");
        };
        assert_eq!(items.len(), 3);
        let FieldValue::FormKey(master_fk) = &items[0] else {
            panic!("first item should be a FormKey");
        };
        assert_eq!(
            form_key_to_read_str(master_fk, &interner),
            "Fallout4.esm:123456"
        );
        assert_eq!(items[1], FieldValue::None);
        let FieldValue::FormKey(own_fk) = &items[2] else {
            panic!("third item should be a FormKey");
        };
        assert_eq!(form_key_to_read_str(own_fk, &interner), "MyMod.esp:000800");
    }

    #[test]
    fn regn_rdwt_decodes_weather_formids() {
        let mut data = Vec::new();
        data.extend_from_slice(&0x007E_BA4B_u32.to_le_bytes());
        data.extend_from_slice(&95_u32.to_le_bytes());
        data.extend_from_slice(&0_u32.to_le_bytes());
        data.extend_from_slice(&0x007E_E02B_u32.to_le_bytes());
        data.extend_from_slice(&5_u32.to_le_bytes());
        data.extend_from_slice(&0_u32.to_le_bytes());
        let mut interner = StringInterner::new();

        let value = decode_subrecord(
            "REGN",
            "RDWT",
            "array_struct:I,I,I",
            &data,
            &[],
            "SeventySix.esm",
            None,
            &mut interner,
        )
        .unwrap();

        let FieldValue::List(rows) = value else {
            panic!("RDWT should decode to weather rows");
        };
        assert_eq!(rows.len(), 2);
        let FieldValue::Struct(first) = &rows[0] else {
            panic!("RDWT row should decode to a struct");
        };
        let weather = field_value(first, &interner, "WeatherTypesWeather");
        let FieldValue::FormKey(weather_fk) = weather else {
            panic!("weather should decode to a FormKey");
        };
        assert_eq!(
            form_key_to_read_str(weather_fk, &interner),
            "SeventySix.esm:7EBA4B"
        );
        assert_eq!(
            field_value(first, &interner, "WeatherTypesChance"),
            &FieldValue::Uint(95)
        );
        assert_eq!(
            field_value(first, &interner, "WeatherTypesGlobal"),
            &FieldValue::Uint(0)
        );
    }

    #[test]
    fn cobj_fvpa_drops_fo76_curve_table_and_decodes_components() {
        // Two FO76 rows of (component, count, curve_table).
        let mut data = Vec::new();
        data.extend_from_slice(&0x0001_FAC2_u32.to_le_bytes()); // component (Fallout4 CMPO)
        data.extend_from_slice(&4_u32.to_le_bytes()); // count
        data.extend_from_slice(&0x0001_FA94_u32.to_le_bytes()); // curve_table (dropped)
        data.extend_from_slice(&0_u32.to_le_bytes()); // null component
        data.extend_from_slice(&2_u32.to_le_bytes()); // count
        data.extend_from_slice(&0x0007_1234_u32.to_le_bytes()); // curve_table (dropped)
        let mut interner = StringInterner::new();

        let value = decode_subrecord(
            "COBJ",
            "FVPA",
            "array_struct:I,I,I",
            &data,
            &["Fallout4.esm".to_string()],
            "SeventySix.esm",
            None,
            &mut interner,
        )
        .unwrap();

        let FieldValue::List(rows) = value else {
            panic!("FVPA should decode to component rows");
        };
        assert_eq!(rows.len(), 2);
        let FieldValue::Struct(first) = &rows[0] else {
            panic!("FVPA row should decode to a struct");
        };
        // The dropped curve_table means each struct has exactly two fields.
        assert_eq!(first.len(), 2);
        let FieldValue::FormKey(component_fk) =
            field_value(first, &interner, "components_component")
        else {
            panic!("component should decode to a FormKey");
        };
        assert_eq!(
            form_key_to_read_str(component_fk, &interner),
            "Fallout4.esm:01FAC2"
        );
        assert_eq!(
            field_value(first, &interner, "components_count"),
            &FieldValue::Uint(4)
        );
        // A zero component decodes to a null Uint(0), count preserved.
        let FieldValue::Struct(second) = &rows[1] else {
            panic!("FVPA row should decode to a struct");
        };
        assert_eq!(
            field_value(second, &interner, "components_component"),
            &FieldValue::Uint(0)
        );
        assert_eq!(
            field_value(second, &interner, "components_count"),
            &FieldValue::Uint(2)
        );
    }

    #[test]
    fn cobj_fvpa_keeps_malformed_payload_raw() {
        let mut interner = StringInterner::new();

        let value = decode_subrecord(
            "COBJ",
            "FVPA",
            "array_struct:I,I,I",
            &[1, 2, 3, 4, 5, 6, 7, 8], // 8 bytes: not a multiple of 12
            &[],
            "SeventySix.esm",
            None,
            &mut interner,
        )
        .unwrap();

        assert!(matches!(value, FieldValue::Bytes(_)));
    }

    #[test]
    fn regn_rdwt_keeps_malformed_payload_raw() {
        let mut interner = StringInterner::new();

        let value = decode_subrecord(
            "REGN",
            "RDWT",
            "array_struct:I,I,I",
            &[1, 2, 3],
            &[],
            "SeventySix.esm",
            None,
            &mut interner,
        )
        .unwrap();

        assert!(matches!(value, FieldValue::Bytes(_)));
    }

    #[test]
    fn parse_form_key_str_plugin_colon_hex() {
        let mut interner = StringInterner::new();
        let (plugin, fk) = parse_form_key_str("test.esm:000800", &mut interner).unwrap();
        assert_eq!(plugin, "test.esm");
        assert_eq!(fk.local, 0x800);
    }

    #[test]
    fn parse_form_key_str_hex_at_plugin() {
        let mut interner = StringInterner::new();
        let (plugin, fk) = parse_form_key_str("000800@test.esm", &mut interner).unwrap();
        assert_eq!(plugin, "test.esm");
        assert_eq!(fk.local, 0x800);
    }

    #[test]
    fn struct_codec_emits_bytes() {
        let data = [1u8, 2, 3, 4];
        let mut interner = StringInterner::new();
        let v = decode_subrecord(
            "WEAP",
            "DATA",
            "struct:I",
            &data,
            &[],
            "test.esm",
            None,
            &mut interner,
        )
        .unwrap();
        assert!(matches!(v, FieldValue::Bytes(_)));
    }

    // The read_record round-trip test uses a fixture plugin loaded via the Python
    // integration test (test_native_record_io.py). The Rust-level fixture test is
    // omitted here because NativePluginSlot construction requires private
    // plugin_runtime internals; the Python test covers the same path end-to-end.

    #[test]
    fn form_key_to_read_str_formats_correctly() {
        let mut interner = StringInterner::new();
        let fk = parse_form_key_str("fo4_minimal_weap.esm:000800", &mut interner)
            .map(|(_, fk)| fk)
            .unwrap();
        let s = form_key_to_read_str(&fk, &interner);
        assert_eq!(s, "fo4_minimal_weap.esm:000800");
    }

    #[test]
    fn iter_form_keys_of_sig_unknown_handle_returns_not_found() {
        let mut interner = StringInterner::new();
        let sig = SigCode::from_str("WEAP").unwrap();
        let result = iter_form_keys_of_sig(u64::MAX, sig, &mut interner);
        assert!(
            matches!(result, Err(RecordReadError::NotFound(_))),
            "expected NotFound for unknown handle"
        );
    }

    // Integration test: iter_form_keys_of_sig on fixture plugin returns exactly
    // one WEAP FormKey. This test requires a loaded plugin handle (Python runtime),
    // so the full path is verified in test_native_record_io.py. The unit-level
    // coverage above confirms the error path and the render helper.

    // ------------------------------------------------------------------
    // SCEN.TNAM Template Scene formid decode
    // ------------------------------------------------------------------

    /// Build a minimal SCEN ParsedRecord with an actions-block TNAM (float32)
    /// followed by the top-level Template Scene TNAM (formid pointing into
    /// SeventySix.esm's own-plugin slot at master index 0 since the source has
    /// no masters in this test fixture).
    ///
    /// Real FO76 layout: the template TNAM sits in the record-level tail block
    /// "VNAM TNAM XNAM". Action-scoped TNAMs are preceded by SNAM/ENAM/ANAM.
    fn scen_with_template_tnam(template_raw_formid: u32) -> ParsedRecord {
        let subrecords = vec![
            parsed_subrecord("EDID", b"TestScene\0".to_vec()),
            // Actions block: ANAM (action-type) + TNAM (timer float32 = 1.0)
            parsed_subrecord("ANAM", 1u16.to_le_bytes().to_vec()),
            parsed_subrecord("TNAM", 1.0f32.to_le_bytes().to_vec()),
            // Record-level tail: VNAM then Template Scene TNAM (formid) then XNAM
            parsed_subrecord("VNAM", vec![0u8; 4]),
            parsed_subrecord("TNAM", template_raw_formid.to_le_bytes().to_vec()),
            parsed_subrecord("XNAM", vec![0u8; 4]),
        ];
        // keep raw_payload absent
        ParsedRecord {
            signature: SmolStr::new("SCEN"),
            form_id: 0x0000_1234,
            flags: 0,
            version_control: 0,
            form_version: Some(155),
            version2: None,
            subrecords,
            raw_payload: None,
            parse_error: None,
        }
    }

    #[test]
    fn scen_template_tnam_decodes_as_formkey_not_float() {
        // The Template Scene TNAM (preceded by VNAM) must decode as a FormKey
        // so that its FormID is remapped 00→07 by the FK walker.
        // A raw formid of 0x00405B4 with no masters means own-plugin (SeventySix.esm).
        let template_raw = 0x0040_5B40u32; // e.g. local=0x0405B4, master-byte 0x00
        let raw_record = scen_with_template_tnam(template_raw);
        let schema = AuthoringSchema::for_game("fo76").expect("fo76 schema");
        let interner = StringInterner::new();
        let fk = FormKey {
            local: 0x001234,
            plugin: interner.intern("SeventySix.esm"),
        };

        let record = decode_record_from_parsed(
            &raw_record,
            &fk,
            &schema,
            &[],
            "SeventySix.esm",
            None,
            false,
            &interner,
        )
        .unwrap();

        // First TNAM (actions timer) must stay as float or bytes (not a FormKey).
        let first_tnam = record
            .fields
            .iter()
            .find(|f| f.sig.as_str() == "TNAM")
            .expect("first TNAM must be present");
        assert!(
            !matches!(first_tnam.value, FieldValue::FormKey(_)),
            "actions TNAM must NOT be a FormKey; got {:?}",
            first_tnam.value
        );

        // Last TNAM (Template Scene) must be a FormKey — not a float.
        let last_tnam = record
            .fields
            .iter()
            .rev()
            .find(|f| f.sig.as_str() == "TNAM")
            .expect("Template Scene TNAM must be present");
        assert!(
            matches!(last_tnam.value, FieldValue::FormKey(_)),
            "Template Scene TNAM must be a FormKey for FK remapping; got {:?}",
            last_tnam.value
        );
    }

    #[test]
    fn scen_actions_tnam_before_template_tnam_stays_non_formkey() {
        // When an action TNAM (float32, preceded by ANAM not VNAM) and a
        // Template Scene TNAM (preceded by VNAM) both exist, only the one
        // preceded by VNAM becomes a FormKey.
        let template_raw = 0x0040_5B40u32;
        let raw_record = scen_with_template_tnam(template_raw);
        let schema = AuthoringSchema::for_game("fo76").expect("fo76 schema");
        let interner = StringInterner::new();
        let fk = FormKey {
            local: 0x001234,
            plugin: interner.intern("SeventySix.esm"),
        };
        let record = decode_record_from_parsed(
            &raw_record,
            &fk,
            &schema,
            &[],
            "SeventySix.esm",
            None,
            false,
            &interner,
        )
        .unwrap();

        // Collect all TNAM fields in order.
        let tnam_values: Vec<_> = record
            .fields
            .iter()
            .filter(|f| f.sig.as_str() == "TNAM")
            .collect();
        assert_eq!(tnam_values.len(), 2, "fixture has 2 TNAMs");

        // First TNAM (actions timer, float32 1.0) must NOT be a FormKey.
        assert!(
            !matches!(tnam_values[0].value, FieldValue::FormKey(_)),
            "actions TNAM[0] must not be FormKey; got {:?}",
            tnam_values[0].value
        );

        // Last TNAM (Template Scene) must be a FormKey.
        assert!(
            matches!(tnam_values[1].value, FieldValue::FormKey(_)),
            "Template Scene TNAM[1] must be FormKey; got {:?}",
            tnam_values[1].value
        );
    }

    #[test]
    fn scen_timer_only_tnam_not_preceded_by_vnam_stays_float() {
        // Population danger case: a SCEN that has only action-scoped timer TNAMs
        // (no Template Scene). The preceding sig is ANAM, not VNAM — none should
        // be decoded as a formid. This is the 51-record class found in the census.
        let timer_float = 5.0f32;
        let raw_record = ParsedRecord {
            signature: SmolStr::new("SCEN"),
            form_id: 0x0000_ABCD,
            flags: 0,
            version_control: 0,
            form_version: Some(155),
            version2: None,
            subrecords: vec![
                parsed_subrecord("EDID", b"TimerOnlyScene\0".to_vec()),
                // Action block 1: SNAM ... TNAM (timer) ANAM
                parsed_subrecord("SNAM", vec![0u8; 4]),
                parsed_subrecord("TNAM", timer_float.to_le_bytes().to_vec()),
                // Action block 2: ANAM ... TNAM (timer) ANAM
                parsed_subrecord("ANAM", 1u16.to_le_bytes().to_vec()),
                parsed_subrecord("TNAM", timer_float.to_le_bytes().to_vec()),
                // Record tail without a Template TNAM (no VNAM TNAM block)
                parsed_subrecord("PNAM", vec![0u8; 4]),
                parsed_subrecord("XNAM", vec![0u8; 4]),
            ],
            raw_payload: None,
            parse_error: None,
        };
        let schema = AuthoringSchema::for_game("fo76").expect("fo76 schema");
        let interner = StringInterner::new();
        let fk = FormKey {
            local: 0x00ABCD,
            plugin: interner.intern("SeventySix.esm"),
        };
        let record = decode_record_from_parsed(
            &raw_record,
            &fk,
            &schema,
            &[],
            "SeventySix.esm",
            None,
            false,
            &interner,
        )
        .unwrap();

        // All TNAMs must remain non-FormKey (float or bytes).
        for field in record.fields.iter().filter(|f| f.sig.as_str() == "TNAM") {
            assert!(
                !matches!(field.value, FieldValue::FormKey(_)),
                "timer-only TNAM must NOT be a FormKey; got {:?}",
                field.value
            );
        }
    }

    // ------------------------------------------------------------------
    // LVLI.LLKC keyword FormID decode
    // ------------------------------------------------------------------

    #[test]
    fn lvli_llkc_fo76_three_field_rows_decode_keyword_formkey() {
        // FO76 LLKC rows: [keyword FK u32, chance u32, curve_table FK u32]
        // Expected: decode to List[Struct[(keyword=FK, chance=Uint)]] dropping
        // curve_table, so the keyword FK is remapped 00→07 by the walker.
        let mut data = Vec::new();
        // Row 1: keyword=0x001234 (master 0 = Fallout4.esm), chance=50, curve=0x0007_AAAA
        data.extend_from_slice(&0x0000_1234u32.to_le_bytes());
        data.extend_from_slice(&50u32.to_le_bytes());
        data.extend_from_slice(&0x0007_AAAAu32.to_le_bytes());
        let mut interner = StringInterner::new();

        let value = decode_subrecord(
            "LVLI",
            "LLKC",
            "array_struct:I,I,I",
            &data,
            &["Fallout4.esm".to_string()],
            "SeventySix.esm",
            None,
            &mut interner,
        )
        .unwrap();

        let FieldValue::List(rows) = value else {
            panic!("LLKC must decode to List, got non-List");
        };
        assert_eq!(rows.len(), 1);
        let FieldValue::Struct(fields) = &rows[0] else {
            panic!("LLKC row must be Struct");
        };
        // Must have exactly 2 fields (keyword + chance), curve_table dropped.
        assert_eq!(fields.len(), 2, "curve_table must be dropped");
        assert!(
            matches!(fields[0].1, FieldValue::FormKey(_)),
            "keyword must be a FormKey for 00→07 remap; got {:?}",
            fields[0].1
        );
        assert_eq!(fields[1].1, FieldValue::Uint(50), "chance preserved");
    }

    #[test]
    fn lvli_llkc_fo4_two_field_rows_decode_keyword_formkey() {
        // FO4 LLKC rows: [keyword FK u32, chance u32]
        let mut data = Vec::new();
        data.extend_from_slice(&0x0001_ABCDu32.to_le_bytes()); // Fallout4.esm:1ABCD
        data.extend_from_slice(&75u32.to_le_bytes());
        let mut interner = StringInterner::new();

        let value = decode_subrecord(
            "LVLI",
            "LLKC",
            "array_struct:I,I",
            &data,
            &["Fallout4.esm".to_string()],
            "SeventySix.esm",
            None,
            &mut interner,
        )
        .unwrap();

        let FieldValue::List(rows) = value else {
            panic!("LLKC (FO4) must decode to List");
        };
        assert_eq!(rows.len(), 1);
        let FieldValue::Struct(fields) = &rows[0] else {
            panic!("row must be Struct");
        };
        assert_eq!(fields.len(), 2);
        assert!(matches!(fields[0].1, FieldValue::FormKey(_)), "keyword FK");
        assert_eq!(fields[1].1, FieldValue::Uint(75));
    }

    #[test]
    fn lvli_llkc_malformed_length_falls_back_to_bytes() {
        let mut interner = StringInterner::new();
        let data = vec![0u8; 7]; // not multiple of 8 or 12
        let v = decode_subrecord(
            "LVLI",
            "LLKC",
            "array_struct:I,I",
            &data,
            &[],
            "SeventySix.esm",
            None,
            &mut interner,
        )
        .unwrap();
        assert!(matches!(v, FieldValue::Bytes(_)));
    }

    fn field_value<'a>(
        fields: &'a [(crate::sym::Sym, FieldValue)],
        interner: &StringInterner,
        name: &str,
    ) -> &'a FieldValue {
        fields
            .iter()
            .find_map(|(key, value)| (interner.resolve(*key) == Some(name)).then_some(value))
            .unwrap_or_else(|| panic!("missing field {name}"))
    }
}
