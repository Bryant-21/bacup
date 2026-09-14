//! `PluginSession<'a>` — a typed, held-lock write scope for fixup execution.
//!
//! One session per fixup `run`. `open_session(target_id, source_id)` takes the
//! global plugin store lock for the session's lifetime. Each write queues a
//! `WriteEffect`; `Drop` applies them to `slot.sections` in order.

use std::collections::{BTreeSet, HashMap};
use std::sync::{Arc, MutexGuard};

use rayon::prelude::*;
use rustc_hash::{FxHashMap, FxHashSet};
use smallvec::SmallVec;
use smol_str::SmolStr;

use crate::fixups::{FixupError, FixupReport};
use crate::formkey_mapper::{FormKeyMapper, MapperSnapshot};
use crate::ids::{FormKey, SigCode};
use crate::record::Record;
use crate::schema::AuthoringSchema;
use crate::source_read::{
    TES4_FLAG_LOCALIZED, decode_record_from_parsed, decode_record_in_slot, form_key_to_read_str,
};
use crate::sym::{StringInterner, Sym};
use crate::target_write::{
    PreparedRecordBatch, add_record_in_slot, commit_prepared_record_batch_in_slot,
    encode_record_for_slot, quest_parent_form_id_from_record, replace_record_contents_in_slot,
    replace_record_in_slot, replace_records_contents_in_slot,
    replace_records_contents_in_slot_preserving_prefix, replace_records_in_slot_batch,
};
use bytes::Bytes;
use encoding_rs::WINDOWS_1252;
use esp_authoring_core::plugin_runtime::{
    CoreSection, FormIdPathsSection, FormKey as IndexedFormKey, NativePluginSlot, ParsedGroup,
    ParsedItem, ParsedRecord, ParsedSubrecord, RecordPath, RecordsSection, WriteEffect,
    effective_subrecords_for_record, ensure_core_section, ensure_form_id_paths_section,
    ensure_records_section, iter_referenced_form_ids_from_subrecords, normalize_form_key,
    plugin_handle_store_ref, record_index_entry_by_form_key,
};

const SERIAL_THRESHOLD: usize = 64;
const CELL_CHILD_GROUP: i32 = 6;
const QUEST_CHILD_GROUP: i32 = 10;

#[derive(Debug)]
pub enum SessionError {
    HandleNotFound(u64),
    RecordNotFound(String),
    PathStale,
    Other(String),
}

impl std::fmt::Display for SessionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::HandleNotFound(id) => write!(f, "unknown plugin handle: {id}"),
            Self::RecordNotFound(s) => write!(f, "record not found: {s}"),
            Self::PathStale => write!(f, "cached record path is stale"),
            Self::Other(s) => write!(f, "session error: {s}"),
        }
    }
}

impl std::error::Error for SessionError {}

pub struct PluginSession<'store> {
    store_guard: MutexGuard<'store, HashMap<u64, NativePluginSlot>>,
    target_id: u64,
    source_id: Option<u64>,
    target_form_id_cache: FxHashMap<FormKey, u32>,
    target_cached_signatures: FxHashSet<SigCode>,
    target_encoded_signatures: Option<EncodedTargetSignatures>,
    pending_target_effects: SmallVec<[WriteEffect; 4]>,
    panicked: bool,
}

struct EncodedTargetSignatures {
    masters: Vec<String>,
    core: Arc<CoreSection>,
    signatures: Arc<FxHashMap<u32, SigCode>>,
}

pub struct ReadView<'a> {
    target_slot: &'a NativePluginSlot,
    target_paths: Arc<FormIdPathsSection>,
    target_core: Arc<CoreSection>,
    source_slot: Option<&'a NativePluginSlot>,
    source_paths: Option<Arc<FormIdPathsSection>>,
}

pub enum EditOutcome {
    Changed,
    Dropped,
    Added,
    NoOp,
}

/// Read-only, parallel-scannable view over one plugin handle's raw records.
///
/// Built by [`PluginSession::handle_raw_scan`]: the core + records index
/// sections are primed once (serially, `&mut`), then this view borrows them
/// read-only so a fixup can sweep the raw subrecords of every record of a
/// signature — including in parallel — with no authoring-schema decode. Lazy
/// (index-only master) handles are materialized per record via `lazy_record`;
/// non-lazy handles are read straight from the parsed tree.
pub struct HandleRawScan<'a> {
    slot: &'a NativePluginSlot,
    core: Arc<CoreSection>,
    records: Arc<RecordsSection>,
}

impl HandleRawScan<'_> {
    pub fn referencing_form_keys_by_query(
        &self,
        queries: &[String],
    ) -> HashMap<String, Vec<String>> {
        let own_plugin = self.slot.parsed.plugin_name.as_str();
        let masters = self.slot.parsed.header.masters.as_slice();
        let own_index = (masters.len() & 0xFF) as u32;
        let mut queries_by_raw = FxHashMap::<u32, Vec<String>>::default();
        for query in queries {
            let Some(form_key) = normalize_form_key(query) else {
                continue;
            };
            let object_id = form_key.object_id & 0x00FF_FFFF;
            if form_key.plugin.eq_ignore_ascii_case(own_plugin) {
                queries_by_raw
                    .entry((own_index << 24) | object_id)
                    .or_default()
                    .push(query.clone());
                queries_by_raw
                    .entry(0xFF00_0000 | object_id)
                    .or_default()
                    .push(query.clone());
                continue;
            }
            if let Some(index) = masters
                .iter()
                .position(|master| master.eq_ignore_ascii_case(&form_key.plugin))
            {
                queries_by_raw
                    .entry(((index as u32) << 24) | object_id)
                    .or_default()
                    .push(query.clone());
            }
        }
        let schema = self.slot.parsed.game.as_deref().and_then(|game| {
            esp_authoring_core::plugin_runtime::compiled_schema_for_game(game).ok()
        });
        let entries = self
            .core
            .by_signature_form_keys
            .values()
            .flatten()
            .filter_map(|form_key| self.core.by_form_key.get(form_key))
            .collect::<Vec<_>>();
        let matches = entries
            .par_iter()
            .filter_map(|entry| {
                self.with_record(entry.raw_form_id, |record| {
                    let subrecords = effective_subrecords_for_record(record);
                    let mut matched_queries = iter_referenced_form_ids_from_subrecords(
                        record.signature.as_str(),
                        &subrecords,
                        schema.as_deref(),
                    )
                    .into_iter()
                    .filter_map(|(_, raw_form_id)| queries_by_raw.get(&raw_form_id))
                    .flatten()
                    .cloned()
                    .collect::<BTreeSet<_>>();
                    for subrecord in subrecords
                        .iter()
                        .filter(|subrecord| subrecord.signature.as_str() == "VMAD")
                    {
                        let _ = visit_vmad_form_ids(
                            &subrecord.data,
                            record.signature.as_str(),
                            &mut |raw_form_id| {
                                if let Some(queries) = queries_by_raw.get(&raw_form_id) {
                                    matched_queries.extend(queries.iter().cloned());
                                }
                            },
                        );
                    }
                    (!matched_queries.is_empty())
                        .then(|| (entry.form_key.render(), matched_queries))
                })
                .flatten()
            })
            .collect::<Vec<_>>();
        let mut result = queries
            .iter()
            .cloned()
            .map(|query| (query, Vec::new()))
            .collect::<HashMap<_, _>>();
        for (referencing, matched_queries) in matches {
            for query in matched_queries {
                result.entry(query).or_default().push(referencing.clone());
            }
        }
        for values in result.values_mut() {
            values.sort_unstable_by_key(|value| value.to_ascii_lowercase());
            values.dedup_by(|left, right| left.eq_ignore_ascii_case(right));
        }
        result
    }

    pub fn record_metadata_by_form_key(&self, form_key: &str) -> Option<(u32, String, String)> {
        let entry = record_index_entry_by_form_key(&self.core, form_key)?;
        Some((
            entry.raw_form_id,
            entry.signature.to_string(),
            entry.form_key.render(),
        ))
    }

    pub fn with_record_context<R>(
        &self,
        raw_form_id: u32,
        f: impl FnOnce(&ParsedRecord, &[String], &str) -> R,
    ) -> Option<R> {
        let masters = self.slot.parsed.header.masters.as_slice();
        let plugin_name = self.slot.parsed.plugin_name.as_str();
        self.with_record(raw_form_id, |record| f(record, masters, plugin_name))
    }

    /// Raw form_ids of every record whose signature is `sig`, in the same order
    /// (and with the same empty-plugin skip) as
    /// [`PluginSession::form_keys_of_sig_in_handle`].
    pub fn raw_form_ids_of_sig(&self, sig: SigCode) -> Vec<u32> {
        let sig_key = SmolStr::new(sig.as_str());
        let Some(keys) = self.core.by_signature_form_keys.get(&sig_key) else {
            return Vec::new();
        };
        keys.iter()
            .filter(|key| !key.plugin.is_empty())
            .filter_map(|key| {
                self.core
                    .by_form_key
                    .get(key)
                    .map(|entry| entry.raw_form_id)
            })
            .collect()
    }

    /// EditorIDs of records whose raw FormID sits at mod index 0, keyed by
    /// object id.
    ///
    /// SOURCE HANDLES ONLY. A target handle carries its masters, so its own
    /// records sit above them and the mod-index filter below drops every one of
    /// them — the map comes back empty and any caller keying off it silently
    /// does nothing. Decode the records and read `Record::eid` for a target.
    pub fn own_editor_ids(&self) -> FxHashMap<u32, String> {
        self.core
            .by_form_key
            .values()
            .filter(|entry| entry.raw_form_id >> 24 == 0 && !entry.eid.is_empty())
            .map(|entry| (entry.raw_form_id & 0x00FF_FFFF, entry.eid.to_string()))
            .collect()
    }

    pub fn with_record<R>(
        &self,
        raw_form_id: u32,
        f: impl FnOnce(&ParsedRecord) -> R,
    ) -> Option<R> {
        if self.slot.is_lazy() {
            let record = self.slot.lazy_record(raw_form_id)?;
            Some(f(&record))
        } else {
            self.records.record(&self.slot.parsed, raw_form_id).map(f)
        }
    }

    /// Materialize the record for `raw_form_id` (lazy-aware) and hand its
    /// effective (decompression-resolved) subrecords to `f`. Read-only and
    /// side-effect free, so it is safe to call from multiple threads.
    pub fn with_record_subrecords<R>(
        &self,
        raw_form_id: u32,
        f: impl FnOnce(&[ParsedSubrecord]) -> R,
    ) -> Option<R> {
        self.with_record(raw_form_id, |record| {
            f(&effective_subrecords_for_record(record))
        })
    }

    pub fn record_decoded(
        &self,
        raw_form_id: u32,
        fk: &FormKey,
        schema: &AuthoringSchema,
        interner: &StringInterner,
    ) -> Option<Result<Record, SessionError>> {
        let plugin_is_localized = (self.slot.parsed.header.flags & TES4_FLAG_LOCALIZED) != 0;
        let strings = plugin_is_localized.then(|| self.slot.strings_ref());
        self.with_record(raw_form_id, |raw_record| {
            decode_record_from_parsed(
                raw_record,
                fk,
                schema,
                &self.slot.parsed.header.masters,
                &self.slot.parsed.plugin_name,
                strings,
                plugin_is_localized,
                interner,
            )
            .map_err(|err| SessionError::Other(err.to_string()))
        })
    }
}

/// Acquires the global plugin store lock; held until the session drops.
pub fn open_session<'store>(
    target_id: u64,
    source_id: Option<u64>,
) -> Result<PluginSession<'store>, SessionError> {
    let store_guard = plugin_handle_store_ref()
        .lock()
        .map_err(|e| SessionError::Other(format!("plugin store mutex poisoned: {e}")))?;

    if !store_guard.contains_key(&target_id) {
        return Err(SessionError::HandleNotFound(target_id));
    }
    if let Some(source_id) = source_id {
        if !store_guard.contains_key(&source_id) {
            return Err(SessionError::HandleNotFound(source_id));
        }
    }

    Ok(PluginSession {
        store_guard,
        target_id,
        source_id,
        target_form_id_cache: FxHashMap::default(),
        target_cached_signatures: FxHashSet::default(),
        target_encoded_signatures: None,
        pending_target_effects: SmallVec::new(),
        panicked: false,
    })
}

impl<'store> PluginSession<'store> {
    pub fn target_id(&self) -> u64 {
        self.target_id
    }

    pub fn source_id(&self) -> Option<u64> {
        self.source_id
    }

    pub fn schema(&self) -> Result<Arc<AuthoringSchema>, SessionError> {
        schema_for_slot(self.target_slot(), "target")
    }

    pub fn source_schema(&self) -> Result<Arc<AuthoringSchema>, SessionError> {
        let source_slot = self
            .source_slot_opt()
            .ok_or_else(|| SessionError::Other("session has no source handle".into()))?;
        schema_for_slot(source_slot, "source")
    }

    pub fn target_masters(&self) -> &[String] {
        self.target_slot().parsed.header.masters.as_slice()
    }

    pub fn target_signatures(&mut self) -> Result<Vec<SigCode>, SessionError> {
        let slot = self.target_slot_mut();
        let core = ensure_core_section(slot);
        Ok(core
            .by_signature_form_keys
            .keys()
            .filter_map(|sig| SigCode::from_str(sig.as_str()).ok())
            .collect())
    }

    pub(crate) fn encoded_target_record_signatures(
        &mut self,
        target_masters: &[String],
    ) -> Arc<FxHashMap<u32, SigCode>> {
        if self.pending_target_effects.iter().any(|effect| {
            matches!(
                effect,
                WriteEffect::RecordsAddedOrRemoved | WriteEffect::MastersChanged
            )
        }) {
            self.flush_pending_effects();
        }
        let core = ensure_core_section(self.target_slot_mut());
        if let Some(cached) = &self.target_encoded_signatures {
            if cached.masters == target_masters && Arc::ptr_eq(&cached.core, &core) {
                return Arc::clone(&cached.signatures);
            }
        }

        let mut signatures =
            FxHashMap::with_capacity_and_hasher(core.by_form_key.len(), Default::default());
        for (signature, keys) in &core.by_signature_form_keys {
            let Ok(signature) = SigCode::from_str(signature.as_str()) else {
                continue;
            };
            let mut seen_raw_form_ids = FxHashSet::default();
            let mut last_plugin: Option<(&str, usize)> = None;
            for key in keys {
                if key.plugin.is_empty() {
                    continue;
                }
                let Some(entry) = core.by_form_key.get(key) else {
                    continue;
                };
                if !seen_raw_form_ids.insert(entry.raw_form_id) {
                    continue;
                }
                let encoded = if key.object_id == 0 {
                    0
                } else {
                    let plugin: &str = &key.plugin;
                    let load_index = match last_plugin {
                        Some((name, index)) if name == plugin => index,
                        _ => {
                            let index = target_masters
                                .iter()
                                .position(|master| master.eq_ignore_ascii_case(plugin))
                                .unwrap_or(target_masters.len());
                            last_plugin = Some((plugin, index));
                            index
                        }
                    };
                    if load_index > u8::MAX as usize || key.object_id > 0x00FF_FFFF {
                        continue;
                    }
                    ((load_index as u32) << 24) | key.object_id
                };
                signatures.insert(encoded, signature);
            }
        }
        let signatures = Arc::new(signatures);
        self.target_encoded_signatures = Some(EncodedTargetSignatures {
            masters: target_masters.to_vec(),
            core,
            signatures: Arc::clone(&signatures),
        });
        signatures
    }

    pub fn record_decoded(
        &mut self,
        fk: &FormKey,
        schema: &AuthoringSchema,
        interner: &StringInterner,
    ) -> Result<Record, SessionError> {
        let slot = self.target_slot_mut();
        decode_record_in_slot(slot, fk, schema, interner, None)
            .map_err(|err| SessionError::Other(err.to_string()))
    }

    pub fn record_decoded_in_handle(
        &mut self,
        handle_id: u64,
        fk: &FormKey,
        schema: &AuthoringSchema,
        interner: &StringInterner,
    ) -> Result<Record, SessionError> {
        let slot = self
            .store_guard
            .get_mut(&handle_id)
            .ok_or(SessionError::HandleNotFound(handle_id))?;
        decode_record_in_slot(slot, fk, schema, interner, None)
            .map_err(|err| SessionError::Other(err.to_string()))
    }

    /// Prime (once, serially) the core + records index sections for `handle_id`
    /// and hand back a read-only [`HandleRawScan`] over it. Fixups that only need
    /// raw subrecords of every record of a signature use this to skip the full
    /// authoring-schema decode entirely — and, because the returned view is
    /// `Sync`, to scan those records in parallel.
    pub fn handle_raw_scan(&mut self, handle_id: u64) -> Result<HandleRawScan<'_>, SessionError> {
        let (core, records) = {
            let slot = self
                .store_guard
                .get_mut(&handle_id)
                .ok_or(SessionError::HandleNotFound(handle_id))?;
            (ensure_core_section(slot), ensure_records_section(slot))
        };
        let slot = self
            .store_guard
            .get(&handle_id)
            .ok_or(SessionError::HandleNotFound(handle_id))?;
        Ok(HandleRawScan {
            slot,
            core,
            records,
        })
    }

    pub(crate) fn handle_load_order(
        &self,
        handle_id: u64,
    ) -> Result<(&[String], &str), SessionError> {
        let slot = self
            .store_guard
            .get(&handle_id)
            .ok_or(SessionError::HandleNotFound(handle_id))?;
        Ok((&slot.parsed.header.masters, &slot.parsed.plugin_name))
    }

    pub fn source_record_decoded(
        &mut self,
        fk: &FormKey,
        schema: &AuthoringSchema,
        interner: &StringInterner,
    ) -> Result<Record, SessionError> {
        let source_id = self
            .source_id
            .ok_or_else(|| SessionError::Other("session has no source handle".into()))?;
        let slot = self
            .store_guard
            .get_mut(&source_id)
            .ok_or(SessionError::HandleNotFound(source_id))?;
        decode_record_in_slot(slot, fk, schema, interner, None)
            .map_err(|err| SessionError::Other(err.to_string()))
    }

    pub fn record_exists_in_handle(
        &mut self,
        handle_id: u64,
        form_key_str: &str,
    ) -> Result<bool, SessionError> {
        let slot = self
            .store_guard
            .get_mut(&handle_id)
            .ok_or(SessionError::HandleNotFound(handle_id))?;
        let core = ensure_core_section(slot);
        Ok(record_index_entry_by_form_key(&core, form_key_str).is_some())
    }

    pub fn record_signature_in_handle(
        &mut self,
        handle_id: u64,
        form_key_str: &str,
    ) -> Result<Option<String>, SessionError> {
        let slot = self
            .store_guard
            .get_mut(&handle_id)
            .ok_or(SessionError::HandleNotFound(handle_id))?;
        let core = ensure_core_section(slot);
        Ok(record_index_entry_by_form_key(&core, form_key_str)
            .map(|entry| entry.signature.to_string()))
    }

    pub fn referencing_form_keys_by_query_in_handle(
        &mut self,
        handle_id: u64,
        queries: &[String],
    ) -> Result<HashMap<String, Vec<String>>, SessionError> {
        if queries.is_empty() {
            return Ok(HashMap::new());
        }
        Ok(self
            .handle_raw_scan(handle_id)?
            .referencing_form_keys_by_query(queries))
    }

    pub fn local_object_ids_in_handle(
        &mut self,
        handle_id: u64,
    ) -> Result<FxHashSet<u32>, SessionError> {
        let slot = self
            .store_guard
            .get_mut(&handle_id)
            .ok_or(SessionError::HandleNotFound(handle_id))?;
        let core = ensure_core_section(slot);
        Ok(core
            .by_form_key
            .values()
            .filter_map(|entry| {
                let object_id = entry.form_key.object_id;
                (object_id != 0 && object_id <= 0x00FF_FFFF).then_some(object_id)
            })
            .collect())
    }

    pub fn own_record_signatures_in_handle(
        &mut self,
        handle_id: u64,
    ) -> Result<FxHashMap<u32, SigCode>, SessionError> {
        let slot = self
            .store_guard
            .get_mut(&handle_id)
            .ok_or(SessionError::HandleNotFound(handle_id))?;
        let plugin_name = slot.parsed.plugin_name.clone();
        let core = ensure_core_section(slot);
        Ok(core
            .by_form_key
            .values()
            .filter_map(|entry| {
                if !entry.form_key.plugin.eq_ignore_ascii_case(&plugin_name) {
                    return None;
                }
                let object_id = entry.form_key.object_id;
                let signature = SigCode::from_str(entry.signature.as_str()).ok()?;
                (object_id != 0 && object_id <= 0x00FF_FFFF).then_some((object_id, signature))
            })
            .collect())
    }

    pub(crate) fn target_slot(&self) -> &NativePluginSlot {
        self.store_guard
            .get(&self.target_id)
            .expect("target handle validated in open_session")
    }

    pub(crate) fn target_slot_mut(&mut self) -> &mut NativePluginSlot {
        self.store_guard
            .get_mut(&self.target_id)
            .expect("target handle validated in open_session")
    }

    pub(crate) fn source_slot_opt(&self) -> Option<&NativePluginSlot> {
        self.source_id.and_then(|id| self.store_guard.get(&id))
    }

    pub(crate) fn record_effect(&mut self, effect: WriteEffect) {
        if matches!(
            effect,
            WriteEffect::RecordsAddedOrRemoved | WriteEffect::MastersChanged
        ) {
            self.target_encoded_signatures = None;
        }
        if let (
            Some(WriteEffect::RecordContents { form_ids: tail_ids }),
            WriteEffect::RecordContents { form_ids },
        ) = (self.pending_target_effects.last_mut(), &effect)
        {
            tail_ids.extend(form_ids.iter().copied());
            return;
        }
        self.pending_target_effects.push(effect);
    }

    pub(crate) fn flush_pending_effects(&mut self) {
        let effects = std::mem::take(&mut self.pending_target_effects);
        if effects.is_empty() {
            return;
        }

        self.target_form_id_cache.clear();
        self.target_cached_signatures.clear();
        if let Some(slot) = self.store_guard.get_mut(&self.target_id) {
            for effect in &effects {
                slot.apply_write_effect(effect);
            }
        }
    }

    fn invalidate_target_indexes_after_structural_write(&mut self) {
        self.target_form_id_cache.clear();
        self.target_cached_signatures.clear();
        self.target_encoded_signatures = None;
        self.target_slot_mut().invalidate_sections();
    }

    pub(crate) fn refresh_target_read_indexes(&mut self) {
        self.flush_pending_effects();
        self.invalidate_target_indexes_after_structural_write();
    }

    /// Return every FormKey of `sig` in the target plugin. Caches the core
    /// section if not already built.
    pub fn form_keys_of_sig(
        &mut self,
        sig: SigCode,
        interner: &StringInterner,
    ) -> Result<Vec<FormKey>, SessionError> {
        let populate_cache = !self.target_cached_signatures.contains(&sig);
        let (form_keys, raw_form_ids) = {
            let slot = self.target_slot_mut();
            let core = ensure_core_section(slot);
            let sig_key = SmolStr::new(sig.as_str());
            core.by_signature_form_keys
                .get(&sig_key)
                .map(|keys| {
                    let mut form_keys = Vec::with_capacity(keys.len());
                    let mut raw_form_ids = populate_cache.then(|| Vec::with_capacity(keys.len()));
                    let mut last_plugin: Option<(&str, Sym)> = None;
                    let mut seen_raw_form_ids = FxHashSet::default();
                    for key in keys {
                        if key.plugin.is_empty() {
                            continue;
                        }
                        let Some(entry) = core.by_form_key.get(key) else {
                            continue;
                        };
                        if !seen_raw_form_ids.insert(entry.raw_form_id) {
                            continue;
                        }
                        let plugin_str: &str = &key.plugin;
                        let plugin_sym = match last_plugin {
                            Some((name, sym)) if name == plugin_str => sym,
                            _ => {
                                let sym = interner.intern(plugin_str);
                                last_plugin = Some((plugin_str, sym));
                                sym
                            }
                        };
                        form_keys.push(FormKey {
                            local: key.object_id,
                            plugin: plugin_sym,
                        });
                        if let Some(raw_form_ids) = raw_form_ids.as_mut() {
                            raw_form_ids.push(entry.raw_form_id);
                        }
                    }
                    (form_keys, raw_form_ids.unwrap_or_default())
                })
                .unwrap_or_default()
        };

        if populate_cache {
            self.target_form_id_cache.reserve(form_keys.len());
            self.target_form_id_cache
                .extend(form_keys.iter().copied().zip(raw_form_ids));
            self.target_cached_signatures.insert(sig);
        }
        Ok(form_keys)
    }

    pub fn source_form_keys_of_sig(
        &mut self,
        sig: SigCode,
        interner: &StringInterner,
    ) -> Result<Vec<FormKey>, SessionError> {
        let source_id = self
            .source_id
            .ok_or_else(|| SessionError::Other("session has no source handle".into()))?;

        let plugin_idx_keys = {
            let slot = self
                .store_guard
                .get_mut(&source_id)
                .ok_or(SessionError::HandleNotFound(source_id))?;
            let core = ensure_core_section(slot);
            let sig_key = SmolStr::new(sig.as_str());
            core.by_signature_form_keys
                .get(&sig_key)
                .map(|keys| {
                    keys.iter()
                        .filter_map(|key| {
                            core.by_form_key
                                .get(key)
                                .map(|entry| (key.clone(), entry.raw_form_id))
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default()
        };

        // Same direct-construction path as `form_keys_of_sig`, avoiding a
        // render()+parse round-trip that would allocate two Strings per record.
        let mut out = Vec::with_capacity(plugin_idx_keys.len());
        let mut last_plugin: Option<(*const str, Sym)> = None;
        for (index_fk, _raw_form_id) in &plugin_idx_keys {
            if index_fk.plugin.is_empty() {
                continue;
            }
            let plugin_str: &str = &index_fk.plugin;
            let plugin_sym = match last_plugin {
                Some((ptr, sym)) if std::ptr::eq(ptr, plugin_str as *const str) => sym,
                _ => {
                    let sym = interner.intern(plugin_str);
                    last_plugin = Some((plugin_str as *const str, sym));
                    sym
                }
            };
            out.push(FormKey {
                local: index_fk.object_id,
                plugin: plugin_sym,
            });
        }
        Ok(out)
    }

    pub fn form_keys_of_sig_in_handle(
        &mut self,
        handle_id: u64,
        sig: SigCode,
        interner: &StringInterner,
    ) -> Result<Vec<FormKey>, SessionError> {
        let plugin_idx_keys = {
            let slot = self
                .store_guard
                .get_mut(&handle_id)
                .ok_or(SessionError::HandleNotFound(handle_id))?;
            let core = ensure_core_section(slot);
            let sig_key = SmolStr::new(sig.as_str());
            core.by_signature_form_keys
                .get(&sig_key)
                .map(|keys| {
                    keys.iter()
                        .filter_map(|key| {
                            core.by_form_key
                                .get(key)
                                .map(|entry| (key.clone(), entry.raw_form_id))
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default()
        };

        let mut out = Vec::with_capacity(plugin_idx_keys.len());
        let mut last_plugin: Option<(*const str, Sym)> = None;
        for (index_fk, _raw_form_id) in &plugin_idx_keys {
            if index_fk.plugin.is_empty() {
                continue;
            }
            let plugin_str: &str = &index_fk.plugin;
            let plugin_sym = match last_plugin {
                Some((ptr, sym)) if std::ptr::eq(ptr, plugin_str as *const str) => sym,
                _ => {
                    let sym = interner.intern(plugin_str);
                    last_plugin = Some((plugin_str as *const str, sym));
                    sym
                }
            };
            out.push(FormKey {
                local: index_fk.object_id,
                plugin: plugin_sym,
            });
        }
        Ok(out)
    }

    /// Look up a record by its raw form_id. O(depth) using the form_id index;
    /// builds the index on first call.
    pub fn record_mut(&mut self, form_id: u32) -> Result<&mut ParsedRecord, SessionError> {
        let path =
            {
                let slot = self.target_slot_mut();
                let paths = ensure_form_id_paths_section(slot);
                paths.by_form_id.get(&form_id).cloned().ok_or_else(|| {
                    SessionError::RecordNotFound(format!("form_id 0x{form_id:08X}"))
                })?
            };

        let slot = self.target_slot_mut();
        walk_to_record_mut(&mut slot.parsed.root_items, &path)
    }

    /// Immutable lookup by raw form_id. Same complexity as `record_mut`.
    pub fn record(&mut self, form_id: u32) -> Result<&ParsedRecord, SessionError> {
        let path =
            {
                let slot = self.target_slot_mut();
                let paths = ensure_form_id_paths_section(slot);
                paths.by_form_id.get(&form_id).cloned().ok_or_else(|| {
                    SessionError::RecordNotFound(format!("form_id 0x{form_id:08X}"))
                })?
            };

        let slot = self.target_slot();
        walk_to_record(&slot.parsed.root_items, &path)
    }

    /// Cheap byte-level scan: returns `true` if *any* subrecord data byte of
    /// the target record identified by `fk` equals `byte_value`. Used as a
    /// pre-filter so fixups can skip the expensive `record_decoded` round-trip
    /// for records that obviously can't contain a FormID with a given master
    /// byte. False positives are fine (the caller still does the full check);
    /// the contract is zero false negatives.
    pub fn record_bytes_contain_byte(
        &mut self,
        fk: &FormKey,
        byte_value: u8,
    ) -> Result<bool, SessionError> {
        let raw_form_id = self.raw_form_id_for_form_key(fk)?;
        let record = self.record(raw_form_id)?;
        for sr in &record.subrecords {
            if sr.data.contains(&byte_value) {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Cheap presence check: returns `true` if the target record identified by
    /// `fk` carries at least one subrecord whose signature is in `sigs`. Reads
    /// the raw parsed subrecords (no full schema decode), so callers can skip the
    /// expensive `record_decoded` round-trip for records that can't be affected.
    pub fn record_has_any_subrecord(
        &mut self,
        fk: &FormKey,
        sigs: &[&str],
    ) -> Result<bool, SessionError> {
        let raw_form_id = self.raw_form_id_for_form_key(fk)?;
        let record = self.record(raw_form_id)?;
        Ok(record
            .subrecords
            .iter()
            .any(|sr| sigs.contains(&sr.signature.as_str())))
    }

    pub fn first_subrecord_bytes(
        &mut self,
        fk: &FormKey,
        sig: &str,
    ) -> Result<Option<Vec<u8>>, SessionError> {
        let raw_form_id = self.raw_form_id_for_form_key(fk)?;
        let record = self.record(raw_form_id)?;
        Ok(record
            .subrecords
            .iter()
            .find(|sr| sr.signature.as_str() == sig)
            .map(|sr| sr.data.to_vec()))
    }

    pub fn patch_subrecord_bytes(
        &mut self,
        fk: &FormKey,
        sig: &str,
        f: impl FnOnce(&mut [u8]) -> bool,
    ) -> Result<bool, SessionError> {
        let raw_form_id = self.raw_form_id_for_form_key(fk)?;

        let changed = {
            let record = self.record_mut(raw_form_id)?;
            let subrecord = record
                .subrecords
                .iter_mut()
                .find(|subrecord| subrecord.signature.as_str() == sig)
                .ok_or_else(|| SessionError::Other(format!("subrecord {sig} not in {fk:?}")))?;
            let original_edid = if sig == "EDID" {
                Some(decode_editor_id(&subrecord.data))
            } else {
                None
            };
            let mut buf = subrecord.data.to_vec();
            let changed = f(&mut buf);
            if changed {
                if let Some(original_edid) = original_edid {
                    let patched_edid = decode_editor_id(&buf);
                    if patched_edid != original_edid {
                        return Err(SessionError::Other(
                            "content-only EDID patch would stale core indexes".into(),
                        ));
                    }
                }
                subrecord.data = Bytes::from(buf);
            }
            changed
        };

        if changed {
            self.target_slot_mut().clear_record_count_cache();
            self.record_effect(WriteEffect::RecordContents {
                form_ids: smallvec::smallvec![raw_form_id],
            });
        }

        Ok(changed)
    }

    pub fn rewrite_placed_leveled_reference(
        &mut self,
        fk: &FormKey,
        expected_name_raw: u32,
        replacement_name_raw: u32,
        xlib_raw: Option<u32>,
    ) -> Result<bool, SessionError> {
        let raw_form_id = self.raw_form_id_for_form_key(fk)?;
        let changed = {
            let record = self.record_mut(raw_form_id)?;
            let Some(name_index) = record
                .subrecords
                .iter()
                .position(|subrecord| subrecord.signature.as_str() == "NAME")
            else {
                return Ok(false);
            };
            let name = &record.subrecords[name_index].data;
            if name.len() < 4
                || u32::from_le_bytes([name[0], name[1], name[2], name[3]]) != expected_name_raw
            {
                return Ok(false);
            }

            let replacement_name = replacement_name_raw.to_le_bytes();
            let mut changed = name.as_ref() != replacement_name;
            if changed {
                record.subrecords[name_index].data = Bytes::copy_from_slice(&replacement_name);
            }

            match xlib_raw {
                Some(xlib_raw) => {
                    let xlib = xlib_raw.to_le_bytes();
                    let xlib_indices = record
                        .subrecords
                        .iter()
                        .enumerate()
                        .filter_map(|(index, subrecord)| {
                            (subrecord.signature.as_str() == "XLIB").then_some(index)
                        })
                        .collect::<SmallVec<[usize; 2]>>();
                    let canonical = xlib_indices.as_slice() == [name_index + 1]
                        && record.subrecords[name_index + 1].data.as_ref() == xlib;
                    if !canonical {
                        record
                            .subrecords
                            .retain(|subrecord| subrecord.signature.as_str() != "XLIB");
                        let name_index = record
                            .subrecords
                            .iter()
                            .position(|subrecord| subrecord.signature.as_str() == "NAME")
                            .expect("NAME was validated above");
                        record.subrecords.insert(
                            name_index + 1,
                            ParsedSubrecord {
                                signature: SmolStr::new("XLIB"),
                                data: Bytes::copy_from_slice(&xlib),
                                semantic_type: None,
                            },
                        );
                        changed = true;
                    }
                }
                None => {
                    let before = record.subrecords.len();
                    record
                        .subrecords
                        .retain(|subrecord| subrecord.signature.as_str() != "XLIB");
                    changed |= before != record.subrecords.len();
                }
            }

            if changed {
                record.raw_payload = None;
            }
            changed
        };

        if changed {
            self.target_slot_mut().clear_record_count_cache();
            self.record_effect(WriteEffect::RecordContents {
                form_ids: smallvec::smallvec![raw_form_id],
            });
        }
        Ok(changed)
    }

    /// Patch the raw bytes of EVERY subrecord of `fk` whose signature is `sig`,
    /// invoking `f(&mut buf)` per occurrence (returning whether it mutated). Unlike
    /// `patch_subrecord_bytes` (first match only), this visits all occurrences —
    /// needed for records that repeat a signature (e.g. SCEN HTID, one per scene
    /// action). Returns the number of subrecords actually changed. Operates on the
    /// raw parsed bytes, so it is independent of any decode-time disambiguation.
    pub fn patch_all_subrecords_bytes(
        &mut self,
        fk: &FormKey,
        sig: &str,
        mut f: impl FnMut(&mut Vec<u8>) -> bool,
    ) -> Result<u32, SessionError> {
        let raw_form_id = self.raw_form_id_for_form_key(fk)?;
        let changed = {
            let record = self.record_mut(raw_form_id)?;
            let mut changed = 0u32;
            for subrecord in record
                .subrecords
                .iter_mut()
                .filter(|s| s.signature.as_str() == sig)
            {
                let mut buf = subrecord.data.to_vec();
                if f(&mut buf) {
                    subrecord.data = Bytes::from(buf);
                    changed += 1;
                }
            }
            changed
        };
        if changed > 0 {
            self.target_slot_mut().clear_record_count_cache();
            self.record_effect(WriteEffect::RecordContents {
                form_ids: smallvec::smallvec![raw_form_id],
            });
        }
        Ok(changed)
    }

    pub fn replace_record(
        &mut self,
        record: Record,
        schema: &AuthoringSchema,
        interner: &StringInterner,
    ) -> Result<(), SessionError> {
        replace_record_in_slot(self.target_slot_mut(), record, schema, interner)
            .map_err(|err| SessionError::Other(err.to_string()))?;
        self.invalidate_target_indexes_after_structural_write();
        self.record_effect(WriteEffect::RecordsAddedOrRemoved);
        Ok(())
    }

    pub fn replace_record_contents(
        &mut self,
        record: Record,
        schema: &AuthoringSchema,
        interner: &StringInterner,
    ) -> Result<bool, SessionError> {
        let replaced_form_id =
            replace_record_contents_in_slot(self.target_slot_mut(), record, schema, interner)
                .map_err(|err| SessionError::Other(err.to_string()))?;
        if let Some(raw_form_id) = replaced_form_id {
            self.record_effect(WriteEffect::RecordContents {
                form_ids: smallvec::smallvec![raw_form_id],
            });
        }
        Ok(replaced_form_id.is_some())
    }

    pub fn replace_records(
        &mut self,
        records: Vec<Record>,
        schema: &AuthoringSchema,
        interner: &StringInterner,
    ) -> Result<(), SessionError> {
        if records.is_empty() {
            return Ok(());
        }

        let result =
            replace_records_in_slot_batch(self.target_slot_mut(), records, schema, interner);
        self.invalidate_target_indexes_after_structural_write();
        self.record_effect(WriteEffect::RecordsAddedOrRemoved);
        result.map_err(|err| SessionError::Other(err.to_string()))?;
        Ok(())
    }

    pub fn replace_records_contents(
        &mut self,
        records: Vec<Record>,
        schema: &AuthoringSchema,
        interner: &StringInterner,
    ) -> Result<usize, SessionError> {
        if records.is_empty() {
            return Ok(0);
        }

        let replaced_form_ids =
            replace_records_contents_in_slot(self.target_slot_mut(), records, schema, interner)
                .map_err(|err| SessionError::Other(err.to_string()))?;
        let replaced = replaced_form_ids.len();
        if !replaced_form_ids.is_empty() {
            self.record_effect(WriteEffect::RecordContents {
                form_ids: replaced_form_ids,
            });
        }
        Ok(replaced)
    }

    pub fn replace_records_contents_preserving_prefix(
        &mut self,
        records: Vec<Record>,
        schema: &AuthoringSchema,
        interner: &StringInterner,
    ) -> Result<usize, SessionError> {
        if records.is_empty() {
            return Ok(0);
        }
        let outcome = replace_records_contents_in_slot_preserving_prefix(
            self.target_slot_mut(),
            records,
            schema,
            interner,
        );
        let replaced = outcome.applied_form_ids.len();
        if !outcome.applied_form_ids.is_empty() {
            self.record_effect(WriteEffect::RecordContents {
                form_ids: outcome.applied_form_ids,
            });
        }
        if let Some(error) = outcome.error {
            return Err(SessionError::Other(error.to_string()));
        }
        Ok(replaced)
    }

    pub fn add_record(
        &mut self,
        record: Record,
        schema: &AuthoringSchema,
        interner: &StringInterner,
    ) -> Result<(), SessionError> {
        add_record_in_slot(self.target_slot_mut(), record, schema, interner)
            .map_err(|err| SessionError::Other(err.to_string()))?;
        self.invalidate_target_indexes_after_structural_write();
        self.record_effect(WriteEffect::RecordsAddedOrRemoved);
        Ok(())
    }

    pub fn add_records(
        &mut self,
        records: Vec<Record>,
        schema: &AuthoringSchema,
        interner: &StringInterner,
    ) -> Result<usize, SessionError> {
        if records.is_empty() {
            return Ok(0);
        }

        let inserted = records.len();
        {
            let slot = self.target_slot_mut();
            for record in records {
                add_record_in_slot(slot, record, schema, interner)
                    .map_err(|err| SessionError::Other(err.to_string()))?;
            }
        }

        self.invalidate_target_indexes_after_structural_write();
        self.record_effect(WriteEffect::RecordsAddedOrRemoved);
        Ok(inserted)
    }

    /// Add records under their parent QUST's quest-child groups, in one pass.
    ///
    /// `add_records` (via `ensure_top_group_and_add`) would put a DIAL/SCEN in a
    /// top-level group; xEdit then shows it at plugin root and the game never ties
    /// it to the quest. The parent quest is read from the record (DIAL
    /// `QSTI`/`QNAM`, SCEN `PNAM`); records whose quest is absent from the output
    /// are skipped, so the count can be below `records.len()`.
    ///
    /// Batched because the per-record `esp_authoring_core` entry point walks the
    /// whole tree twice and every structural insert invalidates the target read
    /// indexes, which is quadratic for thousands of records.
    pub fn add_quest_child_records(
        &mut self,
        records: Vec<Record>,
        schema: &AuthoringSchema,
        interner: &StringInterner,
    ) -> Result<usize, SessionError> {
        if records.is_empty() {
            return Ok(0);
        }

        let inserted = {
            let slot = self.target_slot_mut();
            let mut encoded: Vec<(u32, ParsedRecord)> = Vec::with_capacity(records.len());
            let mut next_object_id = slot.parsed.header.next_object_id & 0x00FF_FFFF;
            for record in records {
                let Some(parsed) = encode_record_for_slot(slot, record, schema, interner)
                    .map_err(|err| SessionError::Other(err.to_string()))?
                else {
                    continue;
                };
                let Some(parent_quest_form_id) = quest_parent_form_id_from_record(&parsed) else {
                    continue;
                };
                if parent_quest_form_id == 0 {
                    continue;
                }
                let object_id = parsed.form_id & 0x00FF_FFFF;
                if object_id != 0 && object_id >= next_object_id {
                    next_object_id = object_id + 1;
                }
                encoded.push((parent_quest_form_id, parsed));
            }
            if encoded.is_empty() {
                return Ok(0);
            }
            slot.parsed.header.next_object_id = next_object_id & 0x00FF_FFFF;

            let header_size = slot.parsed.header_size;
            let Some(quest_group) =
                find_top_group_mut_in_items(&mut slot.parsed.root_items, b"QUST")
            else {
                return Ok(0);
            };

            // One walk of the QUST top group instead of one per record: which
            // quests exist, and where their child groups already sit.
            let mut quest_positions: FxHashMap<[u8; 4], usize> = FxHashMap::default();
            let mut child_group_positions: FxHashMap<[u8; 4], usize> = FxHashMap::default();
            for (index, item) in quest_group.children.iter().enumerate() {
                match item {
                    ParsedItem::Record(record) if record.signature.as_str() == "QUST" => {
                        quest_positions.insert(record.form_id.to_le_bytes(), index);
                    }
                    ParsedItem::Group(group) if group.group_type == QUEST_CHILD_GROUP => {
                        child_group_positions.insert(group.label, index);
                    }
                    _ => {}
                }
            }

            let mut inserted = 0usize;
            let mut pending: FxHashMap<[u8; 4], Vec<ParsedRecord>> = FxHashMap::default();
            for (parent_quest_form_id, parsed) in encoded {
                let label = parent_quest_form_id.to_le_bytes();
                if !quest_positions.contains_key(&label)
                    && !child_group_positions.contains_key(&label)
                {
                    continue;
                }
                pending.entry(label).or_default().push(parsed);
            }

            // Existing groups first: appending to them cannot shift any index.
            let mut fresh_groups: Vec<([u8; 4], Vec<ParsedRecord>)> = Vec::new();
            for (label, batch) in pending {
                match child_group_positions.get(&label) {
                    Some(&index) => {
                        let ParsedItem::Group(group) = &mut quest_group.children[index] else {
                            continue;
                        };
                        inserted += batch.len();
                        group
                            .children
                            .extend(batch.into_iter().map(ParsedItem::Record));
                    }
                    None => fresh_groups.push((label, batch)),
                }
            }
            // Then the new groups, back to front, so each insert leaves the
            // still-unused quest positions ahead of it untouched.
            fresh_groups.sort_by_key(|(label, _)| {
                std::cmp::Reverse(quest_positions.get(label).copied().unwrap_or(usize::MAX))
            });
            for (label, batch) in fresh_groups {
                let Some(&quest_index) = quest_positions.get(&label) else {
                    continue;
                };
                inserted += batch.len();
                quest_group.children.insert(
                    quest_index + 1,
                    ParsedItem::Group(ParsedGroup {
                        label,
                        group_type: QUEST_CHILD_GROUP,
                        tail: Bytes::from(vec![0u8; header_size.saturating_sub(16)]),
                        children: batch.into_iter().map(ParsedItem::Record).collect(),
                    }),
                );
            }

            if inserted > 0 {
                slot.clear_record_count_cache();
            }
            inserted
        };

        if inserted > 0 {
            self.invalidate_target_indexes_after_structural_write();
            self.record_effect(WriteEffect::RecordsAddedOrRemoved);
        }
        Ok(inserted)
    }

    pub(crate) fn commit_prepared_record_batch(&mut self, batch: PreparedRecordBatch) -> usize {
        let inserted = batch.len();
        if inserted == 0 {
            return 0;
        }
        commit_prepared_record_batch_in_slot(self.target_slot_mut(), batch);
        self.invalidate_target_indexes_after_structural_write();
        self.record_effect(WriteEffect::RecordsAddedOrRemoved);
        inserted
    }

    pub fn insert_placed_child_into_cell_group(
        &mut self,
        cell_form_id: u32,
        group_type: i32,
        record: Record,
        schema: &AuthoringSchema,
        interner: &StringInterner,
    ) -> Result<bool, SessionError> {
        let inserted = {
            let slot = self.target_slot_mut();
            let Some(parsed_record) = encode_record_for_slot(slot, record, schema, interner)
                .map_err(|err| SessionError::Other(err.to_string()))?
            else {
                return Ok(false);
            };
            let header_size = slot.parsed.header_size;
            let Some(cell_child_group) =
                find_cell_child_group_mut_in_items(&mut slot.parsed.root_items, cell_form_id)
            else {
                return Ok(false);
            };
            let section = ensure_cell_section_group_mut(
                cell_child_group,
                group_type,
                cell_form_id,
                header_size,
            );
            section.children.push(ParsedItem::Record(parsed_record));
            slot.clear_record_count_cache();
            true
        };

        if inserted {
            self.invalidate_target_indexes_after_structural_write();
            self.record_effect(WriteEffect::RecordsAddedOrRemoved);
        }
        Ok(inserted)
    }

    pub fn remove_record(&mut self, fk: &FormKey) -> Result<bool, SessionError> {
        let raw_form_id = match self.raw_form_id_for_form_key(fk) {
            Ok(raw_form_id) => raw_form_id,
            Err(SessionError::RecordNotFound(_)) => return Ok(false),
            Err(err) => return Err(err),
        };

        let removed =
            remove_record_from_items(&mut self.target_slot_mut().parsed.root_items, raw_form_id);
        if removed {
            self.target_slot_mut().clear_record_count_cache();
            self.invalidate_target_indexes_after_structural_write();
            self.record_effect(WriteEffect::RecordsAddedOrRemoved);
        }
        Ok(removed)
    }

    pub fn remove_records(&mut self, fks: &[FormKey]) -> Result<usize, SessionError> {
        // One DFS pass removes the first match per queued id. Placed refs sit deep
        // under WRLD/CELL groups, so a per-id tree scan goes quadratic on large drop
        // lists (e.g. resolve_placed_leveled_bases). DFS order and the multiset
        // count give the same result as per-id removal, duplicate ids included.
        let mut pending: std::collections::HashMap<u32, usize> =
            std::collections::HashMap::with_capacity(fks.len());
        for fk in fks {
            match self.raw_form_id_for_form_key(fk) {
                Ok(raw_form_id) => *pending.entry(raw_form_id).or_insert(0) += 1,
                Err(SessionError::RecordNotFound(_)) => {}
                Err(err) => return Err(err),
            }
        }

        let mut removed = 0usize;
        if !pending.is_empty() {
            let slot = self.target_slot_mut();
            removed = remove_records_from_items(&mut slot.parsed.root_items, &mut pending);
        }
        if removed > 0 {
            self.target_slot_mut().clear_record_count_cache();
            self.invalidate_target_indexes_after_structural_write();
            self.record_effect(WriteEffect::RecordsAddedOrRemoved);
        }
        Ok(removed)
    }

    pub fn read_view<'s>(&'s mut self) -> Result<ReadView<'s>, SessionError> {
        self.build_read_view(true)
    }

    pub fn target_read_view<'s>(&'s mut self) -> Result<ReadView<'s>, SessionError> {
        self.build_read_view(false)
    }

    fn build_read_view<'s>(
        &'s mut self,
        include_source: bool,
    ) -> Result<ReadView<'s>, SessionError> {
        {
            let slot = self.target_slot_mut();
            let _ = ensure_form_id_paths_section(slot);
            let _ = ensure_core_section(slot);
        }
        let target_paths = {
            let slot = self.target_slot_mut();
            ensure_form_id_paths_section(slot)
        };
        let target_core = {
            let slot = self.target_slot_mut();
            ensure_core_section(slot)
        };
        let source_paths = if include_source {
            if let Some(source_id) = self.source_id {
                self.store_guard
                    .get_mut(&source_id)
                    .map(ensure_form_id_paths_section)
            } else {
                None
            }
        } else {
            None
        };
        let target_slot = self.target_slot();
        let source_slot = include_source
            .then_some(self.source_id)
            .flatten()
            .and_then(|id| self.store_guard.get(&id));

        Ok(ReadView {
            target_slot,
            target_paths,
            target_core,
            source_slot,
            source_paths,
        })
    }

    pub fn map_apply_by_sig<E: Send>(
        &mut self,
        sig: SigCode,
        mapper: &mut FormKeyMapper,
        decide: impl Fn(&ReadView, &MapperSnapshot, &FormKey) -> Option<E> + Sync,
        mut apply: impl FnMut(
            &mut Self,
            &mut FormKeyMapper,
            &FormKey,
            E,
        ) -> Result<EditOutcome, FixupError>,
    ) -> Result<FixupReport, FixupError> {
        use rayon::prelude::*;

        let snapshot = mapper.as_read_snapshot();
        let edits: Vec<(FormKey, E)> = {
            let view = self
                .read_view()
                .map_err(|err| FixupError::Other(err.to_string()))?;
            let candidates = view.form_keys_of_sig(sig, mapper.interner);

            if candidates.len() < SERIAL_THRESHOLD {
                candidates
                    .into_iter()
                    .filter_map(|fk| decide(&view, &snapshot, &fk).map(|edit| (fk, edit)))
                    .collect()
            } else {
                candidates
                    .par_iter()
                    .filter_map(|fk| decide(&view, &snapshot, fk).map(|edit| (*fk, edit)))
                    .collect()
            }
        };

        let mut report = FixupReport::empty();
        for (fk, edit) in edits {
            match apply(self, mapper, &fk, edit)? {
                EditOutcome::Changed => report.records_changed += 1,
                EditOutcome::Dropped => report.records_dropped += 1,
                EditOutcome::Added => report.records_added += 1,
                EditOutcome::NoOp => {}
            }
        }
        Ok(report)
    }

    pub(crate) fn raw_form_id_for_form_key(&mut self, fk: &FormKey) -> Result<u32, SessionError> {
        if let Some(raw_form_id) = self.target_form_id_cache.get(fk).copied() {
            return Ok(raw_form_id);
        }

        let raw_form_id = {
            let slot = self.target_slot_mut();
            let core = ensure_core_section(slot);
            let mut matches = core
                .by_form_key
                .values()
                .filter(|entry| entry.form_key.object_id == fk.local)
                .map(|entry| entry.raw_form_id);
            let first = matches
                .next()
                .ok_or_else(|| SessionError::RecordNotFound(format!("{fk:?}")))?;
            if matches.next().is_some() {
                return Err(SessionError::Other(format!(
                    "ambiguous form key {fk:?}; enumerate via form_keys_of_sig before mutating"
                )));
            }
            first
        };

        self.target_form_id_cache.insert(*fk, raw_form_id);
        Ok(raw_form_id)
    }

    pub(crate) fn indexed_raw_form_id_and_signature(
        &mut self,
        fk: &FormKey,
        interner: &StringInterner,
    ) -> Result<(u32, SigCode), SessionError> {
        let plugin = interner
            .resolve(fk.plugin)
            .ok_or_else(|| SessionError::RecordNotFound(format!("{fk:?}")))?;
        let indexed_fk = IndexedFormKey::new(Arc::from(plugin), fk.local);
        let (raw_form_id, signature) = {
            let slot = self.target_slot_mut();
            let core = ensure_core_section(slot);
            let entry = core
                .by_form_key
                .get(&indexed_fk)
                .ok_or_else(|| SessionError::RecordNotFound(format!("{fk:?}")))?;
            (entry.raw_form_id, entry.signature.clone())
        };
        let signature = SigCode::from_str(signature.as_str())
            .map_err(|err| SessionError::Other(err.to_string()))?;
        self.target_form_id_cache.insert(*fk, raw_form_id);
        Ok((raw_form_id, signature))
    }

    pub(crate) fn parent_cell_form_id_for_record(
        &mut self,
        raw_form_id: u32,
    ) -> Result<Option<u32>, SessionError> {
        let path = {
            let slot = self.target_slot_mut();
            let paths = ensure_form_id_paths_section(slot);
            paths.by_form_id.get(&raw_form_id).cloned()
        };
        let Some(path) = path else {
            return Ok(None);
        };

        let slot = self.target_slot();
        Ok(parent_cell_form_id_from_path(
            &slot.parsed.root_items,
            &path,
        ))
    }
}

impl ReadView<'_> {
    pub fn record(&self, form_id: u32) -> Option<&ParsedRecord> {
        let path = self.target_paths.by_form_id.get(&form_id)?;
        walk_to_record(&self.target_slot.parsed.root_items, path).ok()
    }

    pub fn record_decoded(
        &self,
        fk: &FormKey,
        schema: &AuthoringSchema,
        interner: &StringInterner,
    ) -> Result<Record, SessionError> {
        let form_key_str = form_key_to_read_str(fk, interner);
        let entry = record_index_entry_by_form_key(&self.target_core, &form_key_str)
            .ok_or_else(|| SessionError::RecordNotFound(form_key_str.clone()))?;
        let raw_record = self
            .record(entry.raw_form_id)
            .ok_or_else(|| SessionError::RecordNotFound(form_key_str.clone()))?;
        let plugin_is_localized = (self.target_slot.parsed.header.flags & TES4_FLAG_LOCALIZED) != 0;
        let strings = plugin_is_localized.then(|| self.target_slot.strings_ref());
        decode_record_from_parsed(
            raw_record,
            fk,
            schema,
            &self.target_slot.parsed.header.masters,
            &self.target_slot.parsed.plugin_name,
            strings,
            plugin_is_localized,
            interner,
        )
        .map_err(|err| SessionError::Other(err.to_string()))
    }

    pub fn record_has_any_subrecord(
        &self,
        fk: &FormKey,
        sigs: &[&str],
        interner: &StringInterner,
    ) -> bool {
        self.record_parsed(fk, interner).is_some_and(|record| {
            record
                .subrecords
                .iter()
                .any(|subrecord| sigs.contains(&subrecord.signature.as_str()))
        })
    }

    pub fn source_record(&self, form_id: u32) -> Option<&ParsedRecord> {
        let source_slot = self.source_slot?;
        let path = self.source_paths.as_ref()?.by_form_id.get(&form_id)?;
        walk_to_record(&source_slot.parsed.root_items, path).ok()
    }

    /// Raw `ParsedRecord` lookup by FormKey (no schema decode) — the raw-lane
    /// counterpart of `record_decoded`, for store2 sweep visitors.
    pub fn record_parsed(&self, fk: &FormKey, interner: &StringInterner) -> Option<&ParsedRecord> {
        let form_key_str = form_key_to_read_str(fk, interner);
        let entry = record_index_entry_by_form_key(&self.target_core, &form_key_str)?;
        self.record(entry.raw_form_id)
    }

    pub fn form_keys_of_sig(&self, sig: SigCode, interner: &StringInterner) -> Vec<FormKey> {
        let sig_key = SmolStr::new(sig.as_str());
        let Some(keys) = self.target_core.by_signature_form_keys.get(&sig_key) else {
            return Vec::new();
        };
        // Direct construction avoids a render()+parse round-trip, which
        // dominates whole-FO76 fixup wall-clock at scale.
        let mut out = Vec::with_capacity(keys.len());
        let mut last_plugin: Option<(*const str, Sym)> = None;
        for index_fk in keys {
            if index_fk.plugin.is_empty() {
                continue;
            }
            let plugin_str: &str = &index_fk.plugin;
            let plugin_sym = match last_plugin {
                Some((ptr, sym)) if std::ptr::eq(ptr, plugin_str as *const str) => sym,
                _ => {
                    let sym = interner.intern(plugin_str);
                    last_plugin = Some((plugin_str as *const str, sym));
                    sym
                }
            };
            out.push(FormKey {
                local: index_fk.object_id,
                plugin: plugin_sym,
            });
        }
        out
    }

    pub fn target_signatures(&self) -> Vec<SigCode> {
        self.target_core
            .by_signature_form_keys
            .keys()
            .filter_map(|sig| SigCode::from_str(sig.as_str()).ok())
            .collect()
    }
}

impl Drop for PluginSession<'_> {
    fn drop(&mut self) {
        let panicking = std::thread::panicking();
        if panicking || self.panicked {
            if let Some(slot) = self.store_guard.get_mut(&self.target_id) {
                slot.invalidate_sections();
            }
            return;
        }

        self.flush_pending_effects();
    }
}

fn schema_for_slot(
    slot: &NativePluginSlot,
    label: &str,
) -> Result<Arc<AuthoringSchema>, SessionError> {
    let game = slot
        .parsed
        .game
        .as_deref()
        .ok_or_else(|| SessionError::Other(format!("{label} slot has no game set")))?;
    AuthoringSchema::for_game(game).map_err(|err| {
        SessionError::Other(format!(
            "{label} schema unavailable for game {game:?}: {err}"
        ))
    })
}

fn walk_to_record_mut<'a>(
    items: &'a mut Vec<ParsedItem>,
    path: &RecordPath,
) -> Result<&'a mut ParsedRecord, SessionError> {
    let mut current = items.as_mut_slice();
    for &group_index in &path.group_indices {
        let item = current
            .get_mut(group_index as usize)
            .ok_or(SessionError::PathStale)?;
        let ParsedItem::Group(group) = item else {
            return Err(SessionError::PathStale);
        };
        current = group.children.as_mut_slice();
    }

    match current.get_mut(path.record_index as usize) {
        Some(ParsedItem::Record(record)) => Ok(record),
        _ => Err(SessionError::PathStale),
    }
}

fn walk_to_record<'a>(
    items: &'a [ParsedItem],
    path: &RecordPath,
) -> Result<&'a ParsedRecord, SessionError> {
    let mut current = items;
    for &group_index in &path.group_indices {
        let item = current
            .get(group_index as usize)
            .ok_or(SessionError::PathStale)?;
        let ParsedItem::Group(group) = item else {
            return Err(SessionError::PathStale);
        };
        current = group.children.as_slice();
    }

    match current.get(path.record_index as usize) {
        Some(ParsedItem::Record(record)) => Ok(record),
        _ => Err(SessionError::PathStale),
    }
}

fn parent_cell_form_id_from_path(items: &[ParsedItem], path: &RecordPath) -> Option<u32> {
    let mut current = items;
    let mut parent_cell = None;
    for &group_index in &path.group_indices {
        let ParsedItem::Group(group) = current.get(group_index as usize)? else {
            return None;
        };
        if group.group_type == CELL_CHILD_GROUP {
            parent_cell = Some(u32::from_le_bytes(group.label));
        }
        current = group.children.as_slice();
    }
    parent_cell
}

fn find_top_group_mut_in_items<'a>(
    items: &'a mut [ParsedItem],
    label: &[u8; 4],
) -> Option<&'a mut ParsedGroup> {
    items.iter_mut().find_map(|item| match item {
        ParsedItem::Group(group) if group.group_type == 0 && group.label == *label => Some(group),
        _ => None,
    })
}

fn find_cell_child_group_mut_in_items(
    items: &mut [ParsedItem],
    cell_form_id: u32,
) -> Option<&mut ParsedGroup> {
    let label = cell_form_id.to_le_bytes();
    for item in items {
        let ParsedItem::Group(group) = item else {
            continue;
        };
        if group.group_type == CELL_CHILD_GROUP && group.label == label {
            return Some(group);
        }
        if let Some(found) = find_cell_child_group_mut_in_items(&mut group.children, cell_form_id) {
            return Some(found);
        }
    }
    None
}

fn ensure_cell_section_group_mut(
    parent: &mut ParsedGroup,
    group_type: i32,
    cell_form_id: u32,
    header_size: usize,
) -> &mut ParsedGroup {
    let label = cell_form_id.to_le_bytes();
    if let Some(index) = parent.children.iter().position(|item| {
        matches!(
            item,
            ParsedItem::Group(group) if group.group_type == group_type && group.label == label
        )
    }) {
        let ParsedItem::Group(group) = &mut parent.children[index] else {
            unreachable!("position predicate selected a group");
        };
        return group;
    }

    parent.children.push(ParsedItem::Group(ParsedGroup {
        label,
        group_type,
        tail: Bytes::from(vec![0u8; header_size.saturating_sub(16)]),
        children: Vec::new(),
    }));
    let ParsedItem::Group(group) = parent
        .children
        .last_mut()
        .expect("newly inserted cell section group")
    else {
        unreachable!("inserted item is a group");
    };
    group
}

fn remove_records_from_items(
    items: &mut Vec<ParsedItem>,
    pending: &mut std::collections::HashMap<u32, usize>,
) -> usize {
    let mut removed = 0usize;
    items.retain_mut(|item| {
        if pending.is_empty() {
            return true;
        }
        match item {
            ParsedItem::Record(record) => {
                let id = record.form_id & 0xFFFF_FFFF;
                match pending.get_mut(&id) {
                    Some(count) => {
                        *count -= 1;
                        if *count == 0 {
                            pending.remove(&id);
                        }
                        removed += 1;
                        false
                    }
                    None => true,
                }
            }
            ParsedItem::Group(group) => {
                removed += remove_records_from_items(&mut group.children, pending);
                true
            }
            _ => true,
        }
    });
    removed
}

fn remove_record_from_items(items: &mut Vec<ParsedItem>, form_id: u32) -> bool {
    let target = form_id & 0xFFFF_FFFF;
    let mut index = 0;
    while index < items.len() {
        match &items[index] {
            ParsedItem::Record(record) if (record.form_id & 0xFFFF_FFFF) == target => {
                items.remove(index);
                return true;
            }
            ParsedItem::Group(_) => {
                if let ParsedItem::Group(group) = &mut items[index] {
                    if remove_record_from_items(&mut group.children, form_id) {
                        return true;
                    }
                }
                index += 1;
            }
            _ => index += 1,
        }
    }
    false
}

struct VmadCursor<'a> {
    data: &'a [u8],
    offset: usize,
}

impl VmadCursor<'_> {
    fn advance(&mut self, amount: usize) -> Option<()> {
        let next = self.offset.checked_add(amount)?;
        if next > self.data.len() {
            return None;
        }
        self.offset = next;
        Some(())
    }

    fn read_u8(&mut self) -> Option<u8> {
        let value = *self.data.get(self.offset)?;
        self.offset += 1;
        Some(value)
    }

    fn read_u16(&mut self) -> Option<u16> {
        let bytes = self.data.get(self.offset..self.offset.checked_add(2)?)?;
        self.offset += 2;
        Some(u16::from_le_bytes([bytes[0], bytes[1]]))
    }

    fn read_u32(&mut self) -> Option<u32> {
        let bytes = self.data.get(self.offset..self.offset.checked_add(4)?)?;
        self.offset += 4;
        Some(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    fn read_i32(&mut self) -> Option<i32> {
        self.read_u32().map(|value| value as i32)
    }

    fn skip_string(&mut self) -> Option<()> {
        let length = self.read_u16()? as usize;
        self.advance(length)
    }
}

fn visit_vmad_form_ids(
    data: &[u8],
    record_signature: &str,
    visitor: &mut impl FnMut(u32),
) -> Option<()> {
    let mut cursor = VmadCursor { data, offset: 0 };
    let version = cursor.read_u16()?;
    let object_format = cursor.read_u16()?;
    let script_count = cursor.read_u16()? as usize;
    if version == 0 || !matches!(object_format, 1 | 2) {
        return None;
    }
    for _ in 0..script_count {
        visit_vmad_script(&mut cursor, object_format, visitor)?;
    }
    if cursor.offset == data.len() {
        return Some(());
    }
    match record_signature {
        "INFO" | "PACK" | "SCEN" => {
            cursor.advance(2)?;
            visit_vmad_script(&mut cursor, object_format, visitor)
        }
        "PERK" | "TERM" => {
            cursor.advance(1)?;
            visit_vmad_script(&mut cursor, object_format, visitor)
        }
        "QUST" => visit_vmad_quest_fragments(&mut cursor, object_format, visitor),
        _ => Some(()),
    }
}

fn visit_vmad_quest_fragments(
    cursor: &mut VmadCursor<'_>,
    object_format: u16,
    visitor: &mut impl FnMut(u32),
) -> Option<()> {
    cursor.advance(1)?;
    let fragment_count = cursor.read_u16()? as usize;
    let script_name_length = cursor.read_u16()? as usize;
    if script_name_length > 0 {
        cursor.advance(script_name_length)?;
        cursor.advance(1)?;
        let property_count = cursor.read_u16()? as usize;
        for _ in 0..property_count {
            visit_vmad_property(cursor, object_format, visitor)?;
        }
    }
    for _ in 0..fragment_count {
        cursor.advance(9)?;
        cursor.skip_string()?;
        cursor.skip_string()?;
    }
    let alias_count = cursor.read_u16()? as usize;
    for _ in 0..alias_count {
        visit_vmad_object(cursor, object_format, visitor)?;
        cursor.advance(2)?;
        let alias_object_format = cursor.read_u16()?;
        let alias_script_count = cursor.read_u16()? as usize;
        for _ in 0..alias_script_count {
            visit_vmad_script(cursor, alias_object_format, visitor)?;
        }
    }
    Some(())
}

fn visit_vmad_script(
    cursor: &mut VmadCursor<'_>,
    object_format: u16,
    visitor: &mut impl FnMut(u32),
) -> Option<()> {
    cursor.skip_string()?;
    cursor.advance(1)?;
    let property_count = cursor.read_u16()? as usize;
    for _ in 0..property_count {
        visit_vmad_property(cursor, object_format, visitor)?;
    }
    Some(())
}

fn visit_vmad_property(
    cursor: &mut VmadCursor<'_>,
    object_format: u16,
    visitor: &mut impl FnMut(u32),
) -> Option<()> {
    cursor.skip_string()?;
    let property_type = cursor.read_u8()?;
    cursor.advance(1)?;
    visit_vmad_property_value(cursor, property_type, object_format, visitor)
}

fn visit_vmad_property_value(
    cursor: &mut VmadCursor<'_>,
    property_type: u8,
    object_format: u16,
    visitor: &mut impl FnMut(u32),
) -> Option<()> {
    match property_type {
        0 | 6 => Some(()),
        1 => visit_vmad_object(cursor, object_format, visitor),
        2 => cursor.skip_string(),
        3 | 4 => cursor.advance(4),
        5 => cursor.advance(1),
        7 => visit_vmad_struct(cursor, object_format, visitor),
        11 => {
            let count = cursor.read_i32()?;
            if count < 0 {
                return None;
            }
            for _ in 0..count {
                visit_vmad_object(cursor, object_format, visitor)?;
            }
            Some(())
        }
        12 => {
            let count = cursor.read_i32()?;
            if count < 0 {
                return None;
            }
            for _ in 0..count {
                cursor.skip_string()?;
            }
            Some(())
        }
        13 | 14 => {
            let count = cursor.read_i32()?;
            if count < 0 {
                return None;
            }
            cursor.advance((count as usize).checked_mul(4)?)
        }
        15 => {
            let count = cursor.read_i32()?;
            if count < 0 {
                return None;
            }
            cursor.advance(count as usize)
        }
        16 => cursor.advance(4),
        17 => {
            let count = cursor.read_i32()?;
            if count < 0 {
                return None;
            }
            for _ in 0..count {
                visit_vmad_struct(cursor, object_format, visitor)?;
            }
            Some(())
        }
        _ => None,
    }
}

fn visit_vmad_struct(
    cursor: &mut VmadCursor<'_>,
    object_format: u16,
    visitor: &mut impl FnMut(u32),
) -> Option<()> {
    let member_count = cursor.read_i32()?;
    if member_count < 0 {
        return None;
    }
    for _ in 0..member_count {
        cursor.skip_string()?;
        let member_type = cursor.read_u8()?;
        cursor.advance(1)?;
        visit_vmad_property_value(cursor, member_type, object_format, visitor)?;
    }
    Some(())
}

fn visit_vmad_object(
    cursor: &mut VmadCursor<'_>,
    object_format: u16,
    visitor: &mut impl FnMut(u32),
) -> Option<()> {
    let raw_form_id = if object_format == 2 {
        cursor.advance(4)?;
        cursor.read_u32()?
    } else {
        let raw_form_id = cursor.read_u32()?;
        cursor.advance(4)?;
        raw_form_id
    };
    if raw_form_id != 0 {
        visitor(raw_form_id);
    }
    Some(())
}

fn decode_editor_id(data: &[u8]) -> String {
    let mut end = data.len();
    while end > 0 && data[end - 1] == 0 {
        end -= 1;
    }
    let (decoded, _, _) = WINDOWS_1252.decode(&data[..end]);
    decoded.into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::formkey_mapper::{MapperOptions, MapperState};
    use crate::ids::SubrecordSig;
    use crate::record::{FieldEntry, FieldValue, RecordFlags};
    use crate::schema::AuthoringSchema;
    use esp_authoring_core::plugin_runtime::{
        ParsedGroup, ParsedSubrecord, build_vmad_bytes_from_payload, ensure_assets_section,
        ensure_refs_section, plugin_handle_debug_section_loaded_native, plugin_handle_new_native,
    };
    use rayon::prelude::*;
    use smallvec::SmallVec;

    fn create_test_plugin_handle() -> Option<u64> {
        plugin_handle_new_native("SessionTest.esp", Some("fo4")).ok()
    }

    fn load_fixture_with_weap() -> Option<u64> {
        load_fixture_with_weap_count(1, "SessionTest.esp")
    }

    fn load_fixture_with_weap_count(count: usize, plugin_name: &str) -> Option<u64> {
        let handle = plugin_handle_new_native(plugin_name, Some("fo4")).ok()?;
        let mut store = plugin_handle_store_ref().lock().ok()?;
        let slot = store.get_mut(&handle)?;
        let mut children = Vec::with_capacity(count);
        for index in 0..count {
            let form_id = 0x0000_0800 + index as u32;
            let edid = format!("SessionWeap{index}\0");
            children.push(ParsedItem::Record(ParsedRecord {
                signature: SmolStr::new("WEAP"),
                form_id,
                flags: 0,
                version_control: 0,
                form_version: None,
                version2: None,
                subrecords: vec![
                    ParsedSubrecord {
                        signature: SmolStr::new("EDID"),
                        data: Bytes::from(edid.into_bytes()),
                        semantic_type: None,
                    },
                    ParsedSubrecord {
                        signature: SmolStr::new("DNAM"),
                        data: Bytes::from(vec![1, 2, 3, 4]),
                        semantic_type: None,
                    },
                ],
                raw_payload: None,
                parse_error: None,
            }));
        }
        slot.parsed.root_items = vec![ParsedItem::Group(ParsedGroup {
            label: *b"WEAP",
            group_type: 0,
            tail: Bytes::new(),
            children,
        })];
        slot.invalidate_sections();
        Some(handle)
    }

    fn load_fixture_with_cell_child_group() -> Option<u64> {
        let handle = plugin_handle_new_native("SessionCellTest.esp", Some("fo4")).ok()?;
        let mut store = plugin_handle_store_ref().lock().ok()?;
        let slot = store.get_mut(&handle)?;
        let cell_form_id = 0x0000_0800;
        slot.parsed.root_items = vec![ParsedItem::Group(ParsedGroup {
            label: *b"CELL",
            group_type: 0,
            tail: Bytes::new(),
            children: vec![
                ParsedItem::Record(ParsedRecord {
                    signature: SmolStr::new("CELL"),
                    form_id: cell_form_id,
                    flags: 0,
                    version_control: 0,
                    form_version: None,
                    version2: None,
                    subrecords: Vec::new(),
                    raw_payload: None,
                    parse_error: None,
                }),
                ParsedItem::Group(ParsedGroup {
                    label: cell_form_id.to_le_bytes(),
                    group_type: CELL_CHILD_GROUP,
                    tail: Bytes::new(),
                    children: vec![ParsedItem::Group(ParsedGroup {
                        label: cell_form_id.to_le_bytes(),
                        group_type: 8,
                        tail: Bytes::new(),
                        children: vec![ParsedItem::Record(ParsedRecord {
                            signature: SmolStr::new("REFR"),
                            form_id: 0x0000_0801,
                            flags: 0,
                            version_control: 0,
                            form_version: None,
                            version2: None,
                            subrecords: Vec::new(),
                            raw_payload: None,
                            parse_error: None,
                        })],
                    })],
                }),
            ],
        })];
        slot.invalidate_sections();
        Some(handle)
    }

    fn load_source_fixture_with_armo() -> Option<u64> {
        let handle = plugin_handle_new_native("SourceFixture.esm", Some("fo4")).ok()?;
        let mut store = plugin_handle_store_ref().lock().ok()?;
        let slot = store.get_mut(&handle)?;
        slot.parsed.root_items = vec![ParsedItem::Group(ParsedGroup {
            label: *b"ARMO",
            group_type: 0,
            tail: Bytes::new(),
            children: vec![ParsedItem::Record(ParsedRecord {
                signature: SmolStr::new("ARMO"),
                form_id: 0x0000_0900,
                flags: 0,
                version_control: 0,
                form_version: None,
                version2: None,
                subrecords: vec![ParsedSubrecord {
                    signature: SmolStr::new("EDID"),
                    data: Bytes::from_static(b"SourceArmor\0"),
                    semantic_type: None,
                }],
                raw_payload: None,
                parse_error: None,
            })],
        })];
        slot.invalidate_sections();
        Some(handle)
    }

    #[test]
    fn session_record_decoded_returns_full_record() {
        let Some(handle) = load_fixture_with_weap() else {
            return;
        };
        let mut session = open_session(handle, None).unwrap();
        let interner = StringInterner::new();
        let schema = AuthoringSchema::for_game("fo4").unwrap();
        let weap_sig = SigCode::from_str("WEAP").unwrap();
        let fks = session.form_keys_of_sig(weap_sig, &interner).unwrap();

        let record = session.record_decoded(&fks[0], &schema, &interner).unwrap();

        assert_eq!(record.sig, weap_sig);
        assert_eq!(record.form_key, fks[0]);
        assert_eq!(
            record.eid.and_then(|sym| interner.resolve(sym)),
            Some("SessionWeap0")
        );
    }

    #[test]
    fn session_source_record_decoded_uses_source_handle() {
        let Some(target) = create_test_plugin_handle() else {
            return;
        };
        let Some(source) = load_source_fixture_with_armo() else {
            return;
        };
        let mut session = open_session(target, Some(source)).unwrap();
        let interner = StringInterner::new();
        let schema = AuthoringSchema::for_game("fo4").unwrap();
        let fk = FormKey::parse("000900@SourceFixture.esm", &interner).unwrap();

        let record = session
            .source_record_decoded(&fk, &schema, &interner)
            .unwrap();

        assert_eq!(record.sig, SigCode::from_str("ARMO").unwrap());
        assert_eq!(
            record.eid.and_then(|sym| interner.resolve(sym)),
            Some("SourceArmor")
        );
    }

    #[test]
    fn session_handle_specific_reads_use_requested_handle() {
        let Some(target) = create_test_plugin_handle() else {
            return;
        };
        let Some(other) = load_fixture_with_weap_count(1, "OtherFixture.esm") else {
            return;
        };
        let mut session = open_session(target, None).unwrap();
        let interner = StringInterner::new();
        let schema = AuthoringSchema::for_game("fo4").unwrap();
        let weap_sig = SigCode::from_str("WEAP").unwrap();

        let fks = session
            .form_keys_of_sig_in_handle(other, weap_sig, &interner)
            .unwrap();
        let record = session
            .record_decoded_in_handle(other, &fks[0], &schema, &interner)
            .unwrap();

        assert_eq!(fks[0].local, 0x800);
        assert_eq!(
            record.eid.and_then(|sym| interner.resolve(sym)),
            Some("SessionWeap0")
        );
    }

    #[test]
    fn session_schema_accessors_follow_target_and_source_games() {
        let Some(target) = create_test_plugin_handle() else {
            return;
        };
        let Some(source) = load_source_fixture_with_armo() else {
            return;
        };
        {
            let mut store = plugin_handle_store_ref().lock().unwrap();
            store.get_mut(&source).unwrap().parsed.game = Some("fo76".to_string());
        }

        let session = open_session(target, Some(source)).unwrap();

        assert!(session.schema().unwrap().record_def("WEAP").is_some());
        assert!(
            session
                .source_schema()
                .unwrap()
                .record_def("ARMO")
                .is_some()
        );
    }

    #[test]
    fn target_masters_and_signatures_come_from_target_slot() {
        let Some(handle) = load_fixture_with_weap() else {
            return;
        };
        {
            let mut store = plugin_handle_store_ref().lock().unwrap();
            store.get_mut(&handle).unwrap().parsed.header.masters =
                vec!["Fallout4.esm".to_string(), "DLCRobot.esm".to_string()];
            store.get_mut(&handle).unwrap().invalidate_sections();
        }

        let mut session = open_session(handle, None).unwrap();
        let mut signatures = session.target_signatures().unwrap();
        signatures.sort_by_key(|sig| sig.as_str().to_string());

        assert_eq!(session.target_masters(), &["Fallout4.esm", "DLCRobot.esm"]);
        assert_eq!(signatures, vec![SigCode::from_str("WEAP").unwrap()]);
    }

    #[test]
    fn open_session_rejects_unknown_handle() {
        let result = open_session(u64::MAX, None);
        assert!(matches!(result, Err(SessionError::HandleNotFound(_))));
    }

    #[test]
    fn record_effect_coalesces_record_contents() {
        let Some(handle) = create_test_plugin_handle() else {
            return;
        };
        let mut session = open_session(handle, None).unwrap();
        session.record_effect(WriteEffect::RecordContents {
            form_ids: smallvec::smallvec![0x800],
        });
        session.record_effect(WriteEffect::RecordContents {
            form_ids: smallvec::smallvec![0x801, 0x802],
        });

        assert_eq!(session.pending_target_effects.len(), 1);
        match &session.pending_target_effects[0] {
            WriteEffect::RecordContents { form_ids } => {
                assert_eq!(form_ids.as_slice(), &[0x800u32, 0x801, 0x802]);
            }
            _ => panic!("expected coalesced RecordContents"),
        }
    }

    #[test]
    fn drop_applies_pending_effects() {
        let Some(handle) = create_test_plugin_handle() else {
            return;
        };

        {
            let mut session = open_session(handle, None).unwrap();
            let _ = ensure_core_section(session.target_slot_mut());
            session.record_effect(WriteEffect::RecordsAddedOrRemoved);
        }

        let store = plugin_handle_store_ref().lock().unwrap();
        let slot = store.get(&handle).unwrap();
        assert!(
            !slot.has_core_section(),
            "drop should have invalidated core"
        );
    }

    #[test]
    fn form_keys_of_sig_returns_indexed_weap() {
        let Some(handle) = load_fixture_with_weap() else {
            return;
        };
        let mut session = open_session(handle, None).unwrap();
        let weap_sig = SigCode::from_str("WEAP").unwrap();
        let interner = StringInterner::new();

        let fks = session.form_keys_of_sig(weap_sig, &interner).unwrap();
        let repeated = session.form_keys_of_sig(weap_sig, &interner).unwrap();

        assert!(!fks.is_empty(), "fixture has at least one WEAP");
        assert_eq!(fks[0].local, 0x800);
        assert_eq!(repeated, fks);
        assert!(session.target_cached_signatures.contains(&weap_sig));
        assert!(
            fks.iter()
                .all(|fk| session.target_form_id_cache.contains_key(fk))
        );
    }

    #[test]
    fn form_keys_of_sig_returns_duplicate_record_identity_once() {
        let Some(handle) = load_fixture_with_weap() else {
            return;
        };
        {
            let mut store = plugin_handle_store_ref().lock().unwrap();
            let slot = store.get_mut(&handle).unwrap();
            let ParsedItem::Group(group) = &mut slot.parsed.root_items[0] else {
                panic!("fixture root must be a group");
            };
            group.children.push(group.children[0].clone());
            slot.invalidate_sections();
        }
        let mut session = open_session(handle, None).unwrap();
        let interner = StringInterner::new();

        let fks = session
            .form_keys_of_sig(SigCode::from_str("WEAP").unwrap(), &interner)
            .unwrap();

        assert_eq!(fks.len(), 1);
        assert_eq!(fks[0].local, 0x800);
    }

    fn legacy_encoded_signatures(
        session: &mut PluginSession,
        masters: &[String],
    ) -> FxHashMap<u32, SigCode> {
        use crate::fixups::rewrite_raw_object_template_formids::encode_target_form_id;
        let interner = StringInterner::new();
        let mut signatures = FxHashMap::default();
        for sig in session.target_signatures().unwrap() {
            for fk in session.form_keys_of_sig(sig, &interner).unwrap() {
                if let Some(encoded) = encode_target_form_id(fk, &interner, masters) {
                    signatures.insert(encoded, sig);
                }
            }
        }
        signatures
    }

    #[test]
    fn encoded_signatures_match_legacy_for_masters_aliases_and_duplicate_ids() {
        for game in ["fo76", "skyrimse", "fnv", "fo3", "fo4"] {
            let handle = load_fixture_with_weap().unwrap();
            let mut session = open_session(handle, None).unwrap();
            let slot = session.target_slot_mut();
            slot.parsed.game = Some(game.to_string());
            slot.parsed.header.masters = vec!["Fallout4.esm".into(), "DlcRobot.esm".into()];
            let ParsedItem::Group(group) = &mut slot.parsed.root_items[0] else {
                panic!()
            };
            let template = group.children[0].clone();
            group.children.clear();
            for (raw, sig) in [
                (0, "TES4"),
                (0x800, "MISC"),
                (0x0100_0800, "STAT"),
                (0x0200_0800, "REFR"),
                (0xFF00_0800, "REFR"),
                (0x0200_0800, "REFR"),
                (0x0300_0801, "ACHR"),
                (0xFF00_0802, "WEAP"),
                (0x0100_0803, "STAT"),
            ] {
                let ParsedItem::Record(mut record) = template.clone() else {
                    panic!()
                };
                record.form_id = raw;
                record.signature = sig.into();
                group.children.push(ParsedItem::Record(record));
            }
            slot.invalidate_sections();
            for masters in [
                vec![],
                vec!["fallout4.ESM".into(), "dlcROBOT.esm".into()],
                vec!["DlcRobot.esm".into(), "Fallout4.esm".into()],
                (0..257).map(|i| format!("Master{i}.esm")).collect(),
            ] {
                let expected = legacy_encoded_signatures(&mut session, &masters);
                let actual = session.encoded_target_record_signatures(&masters);
                assert_eq!(*actual, expected, "{game}: {masters:?}");
                let repeated = session.encoded_target_record_signatures(&masters);
                assert!(Arc::ptr_eq(&actual, &repeated));
            }
            drop(session);
            assert!(esp_authoring_core::plugin_runtime::plugin_handle_close_native(handle));
        }
    }

    #[test]
    fn encoded_signatures_survive_body_edits_and_invalidate_on_identity_changes() {
        let handle = load_fixture_with_weap().unwrap();
        let mut session = open_session(handle, None).unwrap();
        let first = session.encoded_target_record_signatures(&[]);
        session.record_mut(0x800).unwrap().subrecords[1].data = Bytes::from_static(&[9, 8, 7, 6]);
        session.record_effect(WriteEffect::RecordContents {
            form_ids: smallvec::smallvec![0x800],
        });
        session.flush_pending_effects();
        assert!(Arc::ptr_eq(
            &first,
            &session.encoded_target_record_signatures(&[])
        ));
        session.record_effect(WriteEffect::HeaderOnly);
        session.flush_pending_effects();
        assert!(Arc::ptr_eq(
            &first,
            &session.encoded_target_record_signatures(&[])
        ));

        let mut added = session.record(0x800).unwrap().clone();
        added.form_id = 0x801;
        added.signature = "STAT".into();
        session
            .target_slot_mut()
            .parsed
            .root_items
            .push(ParsedItem::Record(added));
        session.record_effect(WriteEffect::RecordsAddedOrRemoved);
        let inserted = session.encoded_target_record_signatures(&[]);
        assert!(!Arc::ptr_eq(&first, &inserted));
        assert_eq!(inserted.get(&0x801).unwrap().as_str(), "STAT");

        let interner = StringInterner::new();
        let fk = FormKey::parse("000800@SessionTest.esp", &interner).unwrap();
        assert!(session.remove_record(&fk).unwrap());
        let removed = session.encoded_target_record_signatures(&[]);
        assert!(!removed.contains_key(&0x800));
        assert_eq!(*removed, legacy_encoded_signatures(&mut session, &[]));

        session.target_slot_mut().parsed.header.masters = vec!["Fallout4.esm".into()];
        session.record_effect(WriteEffect::MastersChanged);
        let masters = session.target_masters().to_vec();
        let remastered = session.encoded_target_record_signatures(&masters);
        assert!(!Arc::ptr_eq(&removed, &remastered));
        assert_eq!(
            *remastered,
            legacy_encoded_signatures(&mut session, &masters)
        );

        session.target_slot_mut().invalidate_sections();
        let refreshed = session.encoded_target_record_signatures(&masters);
        assert!(!Arc::ptr_eq(&remastered, &refreshed));
        assert_eq!(*refreshed, *remastered);
        drop(session);
        assert!(esp_authoring_core::plugin_runtime::plugin_handle_close_native(handle));
    }

    #[test]
    #[ignore = "requires PLACED_INDEX_CORPUS and optional PLACED_INDEX_REPORT"]
    fn encoded_signatures_match_complete_plugin_corpus() {
        use crate::run::OwnedPluginHandle;
        let path = std::path::PathBuf::from(std::env::var_os("PLACED_INDEX_CORPUS").unwrap());
        let handle = OwnedPluginHandle::load(&path, "fo4", None).unwrap();
        let mut session = open_session(handle.id(), None).unwrap();
        let masters = session.target_masters().to_vec();
        let _ = session.target_signatures().unwrap();
        let started = std::time::Instant::now();
        let expected = legacy_encoded_signatures(&mut session, &masters);
        let legacy_seconds = started.elapsed().as_secs_f64();
        let started = std::time::Instant::now();
        let actual = session.encoded_target_record_signatures(&masters);
        let build_seconds = started.elapsed().as_secs_f64();
        assert_eq!(*actual, expected);
        let started = std::time::Instant::now();
        for _ in 0..10 {
            assert!(Arc::ptr_eq(
                &actual,
                &session.encoded_target_record_signatures(&masters)
            ));
        }
        let reuse_seconds = started.elapsed().as_secs_f64() / 10.;
        let mut entries: Vec<_> = actual.iter().collect();
        entries.sort_by_key(|(raw, _)| **raw);
        let mut digest = blake3::Hasher::new();
        for (raw, sig) in entries {
            digest.update(&raw.to_le_bytes());
            digest.update(&sig.0);
        }
        let report = serde_json::json!({
            "plugin": path, "entries": actual.len(), "identical": true,
            "legacy_seconds": legacy_seconds, "build_seconds": build_seconds,
            "reuse_seconds": reuse_seconds, "digest": digest.finalize().to_hex().to_string(),
        });
        eprintln!("{report}");
        if let Some(path) = std::env::var_os("PLACED_INDEX_REPORT") {
            std::fs::write(path, serde_json::to_vec_pretty(&report).unwrap()).unwrap();
        }
    }

    #[test]
    fn indexed_record_identity_distinguishes_same_local_id_across_plugins() {
        let handle = plugin_handle_new_native("SessionExactIndex.esp", Some("fo4")).unwrap();
        {
            let mut store = plugin_handle_store_ref().lock().unwrap();
            let slot = store.get_mut(&handle).unwrap();
            slot.parsed.header.masters = vec!["Fallout4.esm".to_string()];
            slot.parsed.root_items = vec![
                ParsedItem::Record(ParsedRecord {
                    signature: SmolStr::new("MISC"),
                    form_id: 0x0000_0800,
                    flags: 0,
                    version_control: 0,
                    form_version: None,
                    version2: None,
                    subrecords: Vec::new(),
                    raw_payload: None,
                    parse_error: None,
                }),
                ParsedItem::Record(ParsedRecord {
                    signature: SmolStr::new("WEAP"),
                    form_id: 0x0100_0800,
                    flags: 0,
                    version_control: 0,
                    form_version: None,
                    version2: None,
                    subrecords: Vec::new(),
                    raw_payload: None,
                    parse_error: None,
                }),
            ];
            slot.invalidate_sections();
        }
        let interner = StringInterner::new();
        let own_fk = FormKey::parse("000800@SessionExactIndex.esp", &interner).unwrap();
        let master_fk = FormKey::parse("000800@Fallout4.esm", &interner).unwrap();
        let mut session = open_session(handle, None).unwrap();

        let own = session
            .indexed_raw_form_id_and_signature(&own_fk, &interner)
            .unwrap();
        let master = session
            .indexed_raw_form_id_and_signature(&master_fk, &interner)
            .unwrap();

        assert_eq!(own, (0x0100_0800, SigCode::from_str("WEAP").unwrap()));
        assert_eq!(master, (0x0000_0800, SigCode::from_str("MISC").unwrap()));
        assert!(session.target_cached_signatures.is_empty());
        assert_eq!(session.target_form_id_cache.len(), 2);
    }

    #[test]
    fn record_mut_returns_record_by_form_id() {
        let Some(handle) = load_fixture_with_weap() else {
            return;
        };
        let mut session = open_session(handle, None).unwrap();

        let record = session.record_mut(0x0000_0800).unwrap();

        assert_eq!(record.signature.as_str(), "WEAP");
    }

    #[test]
    fn record_returns_record_by_form_id() {
        let Some(handle) = load_fixture_with_weap() else {
            return;
        };
        let mut session = open_session(handle, None).unwrap();

        let record = session.record(0x0000_0800).unwrap();

        assert_eq!(record.signature.as_str(), "WEAP");
    }

    #[test]
    fn record_mut_uses_form_id_paths_cache_on_second_call() {
        let Some(handle) = load_fixture_with_weap() else {
            return;
        };
        let mut session = open_session(handle, None).unwrap();

        let _ = session.record_mut(0x0000_0800).unwrap();
        assert!(
            session.target_slot().has_form_id_paths_section(),
            "form_id_paths cached after first lookup"
        );

        let record = session.record_mut(0x0000_0800).unwrap();

        assert_eq!(record.signature.as_str(), "WEAP");
    }

    #[test]
    fn patch_subrecord_bytes_via_session_invalidates_only_contents() {
        let Some(handle) = load_fixture_with_weap() else {
            return;
        };

        {
            let mut session = open_session(handle, None).unwrap();
            let _ = session.record_mut(0x0000_0800).unwrap();
            let interner = StringInterner::new();
            let weap_sig = SigCode::from_str("WEAP").unwrap();
            let fks = session.form_keys_of_sig(weap_sig, &interner).unwrap();
            let changed = session
                .patch_subrecord_bytes(&fks[0], "DNAM", |bytes| {
                    if bytes.len() >= 4 {
                        bytes[0] = 0xAB;
                        true
                    } else {
                        false
                    }
                })
                .unwrap();
            assert!(changed);
            assert!(session.target_slot().has_form_id_paths_section());
        }

        let store = plugin_handle_store_ref().lock().unwrap();
        let slot = store.get(&handle).unwrap();
        assert!(slot.has_form_id_paths_section());
    }

    #[test]
    fn patch_subrecord_bytes_falls_back_to_index_when_cache_is_cold() {
        let Some(handle) = load_fixture_with_weap() else {
            return;
        };
        let interner = StringInterner::new();
        let fk = FormKey::parse("000800@SessionTest.esp", &interner).unwrap();

        let mut session = open_session(handle, None).unwrap();
        let changed = session
            .patch_subrecord_bytes(&fk, "DNAM", |bytes| {
                bytes[1] = 0xFE;
                true
            })
            .unwrap();

        assert!(changed);
        assert_eq!(
            session.record(0x0000_0800).unwrap().subrecords[1]
                .data
                .as_ref(),
            &[1, 0xFE, 3, 4]
        );
    }

    #[test]
    fn patch_subrecord_bytes_rejects_edid_value_change() {
        let Some(handle) = load_fixture_with_weap() else {
            return;
        };
        let interner = StringInterner::new();
        let fk = FormKey::parse("000800@SessionTest.esp", &interner).unwrap();

        let mut session = open_session(handle, None).unwrap();
        let err = session
            .patch_subrecord_bytes(&fk, "EDID", |bytes| {
                bytes[0] = b'X';
                true
            })
            .unwrap_err();

        assert!(err.to_string().contains("EDID"));
        assert_eq!(
            session.record(0x0000_0800).unwrap().subrecords[0]
                .data
                .as_ref(),
            b"SessionWeap0\0"
        );
    }

    #[test]
    fn rewrite_placed_leveled_reference_upserts_and_removes_xlib() {
        let Some(handle) = load_fixture_with_cell_child_group() else {
            return;
        };
        let interner = StringInterner::new();
        let fk = FormKey::parse("000801@SessionCellTest.esp", &interner).unwrap();
        let original_list = 0x0100_1000_u32;
        let dummy = 0x0009_28C8_u32;
        let leveled_list = 0x0100_2000_u32;
        let concrete = 0x000A_BCDE_u32;

        let mut session = open_session(handle, None).unwrap();
        session.record_mut(0x0000_0801).unwrap().subrecords.extend([
            ParsedSubrecord {
                signature: SmolStr::new("XLIB"),
                data: Bytes::copy_from_slice(&0x0100_3000_u32.to_le_bytes()),
                semantic_type: None,
            },
            ParsedSubrecord {
                signature: SmolStr::new("NAME"),
                data: Bytes::copy_from_slice(&original_list.to_le_bytes()),
                semantic_type: None,
            },
            ParsedSubrecord {
                signature: SmolStr::new("XLIB"),
                data: Bytes::copy_from_slice(&0x0100_4000_u32.to_le_bytes()),
                semantic_type: None,
            },
        ]);

        assert!(
            session
                .rewrite_placed_leveled_reference(&fk, original_list, dummy, Some(leveled_list))
                .unwrap()
        );
        assert!(
            !session
                .rewrite_placed_leveled_reference(&fk, dummy, dummy, Some(leveled_list))
                .unwrap(),
            "reapplying the same runtime layout is idempotent"
        );
        let record = session.record(0x0000_0801).unwrap();
        let name_index = record
            .subrecords
            .iter()
            .position(|subrecord| subrecord.signature.as_str() == "NAME")
            .unwrap();
        assert_eq!(
            record.subrecords[name_index].data.as_ref(),
            &dummy.to_le_bytes()
        );
        assert_eq!(record.subrecords[name_index + 1].signature.as_str(), "XLIB");
        assert_eq!(
            record
                .subrecords
                .iter()
                .filter(|subrecord| subrecord.signature.as_str() == "XLIB")
                .count(),
            1
        );
        assert_eq!(
            record.subrecords[name_index + 1].data.as_ref(),
            &leveled_list.to_le_bytes()
        );

        assert!(
            session
                .rewrite_placed_leveled_reference(&fk, dummy, concrete, None)
                .unwrap()
        );
        let record = session.record(0x0000_0801).unwrap();
        assert!(
            record
                .subrecords
                .iter()
                .all(|subrecord| subrecord.signature.as_str() != "XLIB")
        );
        assert_eq!(
            record
                .subrecords
                .iter()
                .find(|subrecord| subrecord.signature.as_str() == "NAME")
                .unwrap()
                .data
                .as_ref(),
            &concrete.to_le_bytes()
        );
    }

    #[test]
    fn replace_record_via_session_rewrites_edid() {
        let Some(handle) = load_fixture_with_weap() else {
            return;
        };
        let schema = AuthoringSchema::for_game("fo4").expect("fo4 schema");
        let interner = StringInterner::new();
        let edid_sig = SubrecordSig::from_str("EDID").unwrap();
        let edid_sym = interner.intern("SessionWeapUpdated");
        let mut session = open_session(handle, None).unwrap();
        let weap_sig = SigCode::from_str("WEAP").unwrap();
        let fk = session.form_keys_of_sig(weap_sig, &interner).unwrap()[0];
        let record = Record {
            sig: weap_sig,
            form_key: fk,
            eid: Some(edid_sym),
            flags: RecordFlags::empty(),
            fields: smallvec::smallvec![FieldEntry {
                sig: edid_sig,
                value: FieldValue::String(edid_sym),
            }],
            warnings: SmallVec::new(),
        };
        session
            .replace_record(record, &schema, &interner)
            .expect("replace should succeed");

        let updated = session.record(0x0000_0800).unwrap();
        let edid = updated
            .subrecords
            .iter()
            .find(|subrecord| subrecord.signature.as_str() == "EDID")
            .expect("EDID present");
        assert_eq!(edid.data.as_ref(), b"SessionWeapUpdated\0");
    }

    fn structural_replacement_record(
        signature: &str,
        local: u32,
        plugin_name: &str,
        editor_id: &str,
        interner: &StringInterner,
    ) -> Record {
        let editor_id = interner.intern(editor_id);
        Record {
            sig: SigCode::from_str(signature).unwrap(),
            form_key: FormKey {
                local,
                plugin: interner.intern(plugin_name),
            },
            eid: Some(editor_id),
            flags: RecordFlags::empty(),
            fields: smallvec::smallvec![FieldEntry {
                sig: SubrecordSig::from_str("EDID").unwrap(),
                value: FieldValue::String(editor_id),
            }],
            warnings: SmallVec::new(),
        }
    }

    fn raw_tree_fingerprint(items: &[ParsedItem], output: &mut Vec<(String, u32, Vec<u8>)>) {
        for item in items {
            match item {
                ParsedItem::Record(record) => output.push((
                    record.signature.to_string(),
                    record.form_id,
                    record
                        .subrecords
                        .iter()
                        .find(|subrecord| subrecord.signature.as_str() == "EDID")
                        .map(|subrecord| subrecord.data.to_vec())
                        .unwrap_or_default(),
                )),
                ParsedItem::Group(group) => raw_tree_fingerprint(&group.children, output),
            }
        }
    }

    fn handle_tree_fingerprint(handle: u64) -> Vec<(String, u32, Vec<u8>)> {
        let store = plugin_handle_store_ref().lock().unwrap();
        let mut output = Vec::new();
        raw_tree_fingerprint(&store.get(&handle).unwrap().parsed.root_items, &mut output);
        output
    }

    #[test]
    fn structural_batch_matches_sequential_order_upsert_and_signature_semantics() {
        let sequential =
            load_fixture_with_weap_count(3, "SessionStructuralSequential.esp").unwrap();
        let batched = load_fixture_with_weap_count(3, "SessionStructuralBatched.esp").unwrap();
        let schema = AuthoringSchema::for_game("fo4").unwrap();
        let interner = StringInterner::new();
        let replacements = |plugin_name: &str| {
            vec![
                structural_replacement_record(
                    "WEAP",
                    0x800,
                    plugin_name,
                    "SessionWeapAUpdated",
                    &interner,
                ),
                structural_replacement_record(
                    "ARMO",
                    0x802,
                    plugin_name,
                    "SessionWeapCNowArmor",
                    &interner,
                ),
                structural_replacement_record(
                    "WEAP",
                    0x803,
                    plugin_name,
                    "SessionWeapDUpserted",
                    &interner,
                ),
            ]
        };

        {
            let mut session = open_session(sequential, None).unwrap();
            for record in replacements("SessionStructuralSequential.esp") {
                session.replace_record(record, &schema, &interner).unwrap();
            }
        }
        {
            let mut session = open_session(batched, None).unwrap();
            session
                .replace_records(
                    replacements("SessionStructuralBatched.esp"),
                    &schema,
                    &interner,
                )
                .unwrap();
        }

        let sequential = handle_tree_fingerprint(sequential);
        let batched = handle_tree_fingerprint(batched);
        assert_eq!(sequential, batched);
        assert_eq!(
            batched
                .iter()
                .filter(|(signature, _, _)| signature == "WEAP")
                .map(|(_, form_id, _)| *form_id)
                .collect::<Vec<_>>(),
            vec![0x801, 0x800, 0x803]
        );
        assert!(
            batched
                .iter()
                .any(|(signature, form_id, _)| { signature == "ARMO" && *form_id == 0x802 })
        );
    }

    #[test]
    fn replace_record_contents_via_session_preserves_indexes() {
        let Some(handle) = load_fixture_with_weap() else {
            return;
        };
        let schema = AuthoringSchema::for_game("fo4").expect("fo4 schema");
        let interner = StringInterner::new();
        let weap_sig = SigCode::from_str("WEAP").unwrap();
        let edid_sig = SubrecordSig::from_str("EDID").unwrap();
        let dnam_sig = SubrecordSig::from_str("DNAM").unwrap();
        let edid_sym = interner.intern("SessionWeap0");
        let mut dnam = SmallVec::<[u8; 32]>::new();
        dnam.extend_from_slice(&[9, 8, 7, 6]);

        {
            let mut session = open_session(handle, None).unwrap();
            let fk = session.form_keys_of_sig(weap_sig, &interner).unwrap()[0];
            let _ = ensure_form_id_paths_section(session.target_slot_mut());
            let record = Record {
                sig: weap_sig,
                form_key: fk,
                eid: Some(edid_sym),
                flags: RecordFlags::empty(),
                fields: smallvec::smallvec![
                    FieldEntry {
                        sig: edid_sig,
                        value: FieldValue::String(edid_sym),
                    },
                    FieldEntry {
                        sig: dnam_sig,
                        value: FieldValue::Bytes(dnam),
                    },
                ],
                warnings: SmallVec::new(),
            };

            assert!(
                session
                    .replace_record_contents(record, &schema, &interner)
                    .unwrap()
            );
            assert!(session.target_slot().has_core_section());
            assert!(session.target_slot().has_form_id_paths_section());

            let updated = session.record(0x0000_0800).unwrap();
            let dnam = updated
                .subrecords
                .iter()
                .find(|subrecord| subrecord.signature.as_str() == "DNAM")
                .expect("DNAM present");
            assert_eq!(dnam.data.as_ref(), &[9, 8, 7, 6]);
        }

        let store = plugin_handle_store_ref().lock().unwrap();
        let slot = store.get(&handle).unwrap();
        assert!(slot.has_core_section());
        assert!(slot.has_form_id_paths_section());
    }

    #[test]
    fn replace_record_contents_via_session_invalidates_content_derived_sections() {
        let Some(handle) = load_fixture_with_weap() else {
            return;
        };
        let schema = AuthoringSchema::for_game("fo4").expect("fo4 schema");
        let interner = StringInterner::new();
        let weap_sig = SigCode::from_str("WEAP").unwrap();
        let edid_sig = SubrecordSig::from_str("EDID").unwrap();
        let dnam_sig = SubrecordSig::from_str("DNAM").unwrap();
        let edid_sym = interner.intern("SessionWeap0");
        let mut dnam = SmallVec::<[u8; 32]>::new();
        dnam.extend_from_slice(&[0xAA, 0xBB, 0xCC, 0xDD]);

        {
            let mut store = plugin_handle_store_ref().lock().unwrap();
            let slot = store.get_mut(&handle).unwrap();
            let _ = ensure_core_section(slot);
            let _ = ensure_form_id_paths_section(slot);
            let _ = ensure_refs_section(slot);
            let _ = ensure_assets_section(slot);
        }
        assert!(plugin_handle_debug_section_loaded_native(handle, "refs").unwrap());
        assert!(plugin_handle_debug_section_loaded_native(handle, "assets").unwrap());

        {
            let mut session = open_session(handle, None).unwrap();
            let fk = session.form_keys_of_sig(weap_sig, &interner).unwrap()[0];
            let record = Record {
                sig: weap_sig,
                form_key: fk,
                eid: Some(edid_sym),
                flags: RecordFlags::empty(),
                fields: smallvec::smallvec![
                    FieldEntry {
                        sig: edid_sig,
                        value: FieldValue::String(edid_sym),
                    },
                    FieldEntry {
                        sig: dnam_sig,
                        value: FieldValue::Bytes(dnam),
                    },
                ],
                warnings: SmallVec::new(),
            };

            assert!(
                session
                    .replace_record_contents(record, &schema, &interner)
                    .unwrap()
            );
        }

        {
            let store = plugin_handle_store_ref().lock().unwrap();
            let slot = store.get(&handle).unwrap();
            assert!(slot.has_core_section());
            assert!(slot.has_form_id_paths_section());
        }
        assert!(!plugin_handle_debug_section_loaded_native(handle, "refs").unwrap());
        assert!(!plugin_handle_debug_section_loaded_native(handle, "assets").unwrap());
    }

    #[test]
    fn replace_record_contents_via_session_returns_false_for_signature_mismatch() {
        let Some(handle) = load_fixture_with_weap() else {
            return;
        };
        let schema = AuthoringSchema::for_game("fo4").expect("fo4 schema");
        let interner = StringInterner::new();
        let edid_sig = SubrecordSig::from_str("EDID").unwrap();
        let edid_sym = interner.intern("SessionAmmo");
        let fk = FormKey::parse("000800@SessionTest.esp", &interner).unwrap();
        let record = Record {
            sig: SigCode::from_str("AMMO").unwrap(),
            form_key: fk,
            eid: Some(edid_sym),
            flags: RecordFlags::empty(),
            fields: smallvec::smallvec![FieldEntry {
                sig: edid_sig,
                value: FieldValue::String(edid_sym),
            }],
            warnings: SmallVec::new(),
        };

        let mut session = open_session(handle, None).unwrap();
        assert!(
            !session
                .replace_record_contents(record, &schema, &interner)
                .unwrap()
        );
    }

    #[test]
    fn replace_record_contents_via_session_returns_false_for_missing_record() {
        let Some(handle) = load_fixture_with_weap() else {
            return;
        };
        let schema = AuthoringSchema::for_game("fo4").expect("fo4 schema");
        let interner = StringInterner::new();
        let edid_sig = SubrecordSig::from_str("EDID").unwrap();
        let edid_sym = interner.intern("SessionWeap1");
        let fk = FormKey::parse("000801@SessionTest.esp", &interner).unwrap();
        let record = Record {
            sig: SigCode::from_str("WEAP").unwrap(),
            form_key: fk,
            eid: Some(edid_sym),
            flags: RecordFlags::empty(),
            fields: smallvec::smallvec![FieldEntry {
                sig: edid_sig,
                value: FieldValue::String(edid_sym),
            }],
            warnings: SmallVec::new(),
        };

        let mut session = open_session(handle, None).unwrap();
        assert!(
            !session
                .replace_record_contents(record, &schema, &interner)
                .unwrap()
        );
    }

    #[test]
    fn add_record_via_session_inserts_without_relocking() {
        let Some(handle) = create_test_plugin_handle() else {
            return;
        };
        let schema = AuthoringSchema::for_game("fo4").expect("fo4 schema");
        let interner = StringInterner::new();
        let edid_sig = SubrecordSig::from_str("EDID").unwrap();
        let edid_sym = interner.intern("SessionAmmo");
        let fk = FormKey::parse("000800@SessionTest.esp", &interner).unwrap();
        let record = Record {
            sig: SigCode::from_str("AMMO").unwrap(),
            form_key: fk,
            eid: Some(edid_sym),
            flags: RecordFlags::empty(),
            fields: smallvec::smallvec![FieldEntry {
                sig: edid_sig,
                value: FieldValue::String(edid_sym),
            }],
            warnings: SmallVec::new(),
        };

        let mut session = open_session(handle, None).unwrap();
        session
            .add_record(record, &schema, &interner)
            .expect("add should succeed");

        let inserted = session.record(0x0000_0800).unwrap();
        assert_eq!(inserted.signature.as_str(), "AMMO");
    }

    #[test]
    fn insert_placed_child_into_cell_group_preserves_cell_topology() {
        let Some(handle) = load_fixture_with_cell_child_group() else {
            return;
        };
        let schema = AuthoringSchema::for_game("fo4").expect("fo4 schema");
        let interner = StringInterner::new();
        let edid_sig = SubrecordSig::from_str("EDID").unwrap();
        let edid_sym = interner.intern("InsertedCellChild");
        let fk = FormKey::parse("000802@SessionCellTest.esp", &interner).unwrap();
        let record = Record {
            sig: SigCode::from_str("AMMO").unwrap(),
            form_key: fk,
            eid: Some(edid_sym),
            flags: RecordFlags::empty(),
            fields: smallvec::smallvec![FieldEntry {
                sig: edid_sig,
                value: FieldValue::String(edid_sym),
            }],
            warnings: SmallVec::new(),
        };

        let mut session = open_session(handle, None).unwrap();
        assert_eq!(
            session.parent_cell_form_id_for_record(0x0000_0801).unwrap(),
            Some(0x0000_0800)
        );
        assert!(
            session
                .insert_placed_child_into_cell_group(0x0000_0800, 9, record, &schema, &interner)
                .unwrap()
        );
        assert_eq!(
            session.parent_cell_form_id_for_record(0x0000_0802).unwrap(),
            Some(0x0000_0800)
        );
        assert_eq!(
            session.record(0x0000_0802).unwrap().signature.as_str(),
            "AMMO"
        );
    }

    #[test]
    fn add_record_is_visible_to_indexed_reads_in_same_session() {
        let Some(handle) = create_test_plugin_handle() else {
            return;
        };
        let schema = AuthoringSchema::for_game("fo4").expect("fo4 schema");
        let interner = StringInterner::new();
        let edid_sig = SubrecordSig::from_str("EDID").unwrap();
        let ammo_sig = SigCode::from_str("AMMO").unwrap();
        let edid_sym = interner.intern("SessionAmmoIndexed");
        let fk = FormKey::parse("000801@SessionTest.esp", &interner).unwrap();
        let record = Record {
            sig: ammo_sig,
            form_key: fk,
            eid: Some(edid_sym),
            flags: RecordFlags::empty(),
            fields: smallvec::smallvec![FieldEntry {
                sig: edid_sig,
                value: FieldValue::String(edid_sym),
            }],
            warnings: SmallVec::new(),
        };

        let mut session = open_session(handle, None).unwrap();
        session
            .add_record(record, &schema, &interner)
            .expect("add should succeed");

        let fks = session.form_keys_of_sig(ammo_sig, &interner).unwrap();
        assert_eq!(fks, vec![fk]);

        let inserted = session.record_decoded(&fk, &schema, &interner).unwrap();
        assert_eq!(inserted.sig, ammo_sig);
        assert_eq!(inserted.form_key, fk);
        assert_eq!(
            inserted.eid.and_then(|sym| interner.resolve(sym)),
            Some("SessionAmmoIndexed")
        );
    }

    #[test]
    fn add_records_via_session_inserts_multiple_records() {
        let Some(handle) = create_test_plugin_handle() else {
            return;
        };
        let schema = AuthoringSchema::for_game("fo4").expect("fo4 schema");
        let interner = StringInterner::new();
        let ammo_sig = SigCode::from_str("AMMO").unwrap();
        let edid_sig = SubrecordSig::from_str("EDID").unwrap();

        let mut records = Vec::new();
        for (local, editor_id) in [
            (0x000801, "SessionAmmoBatchA"),
            (0x000802, "SessionAmmoBatchB"),
        ] {
            let edid_sym = interner.intern(editor_id);
            records.push(Record {
                sig: ammo_sig,
                form_key: FormKey::parse(&format!("{local:06X}@SessionTest.esp"), &interner)
                    .unwrap(),
                eid: Some(edid_sym),
                flags: RecordFlags::empty(),
                fields: smallvec::smallvec![FieldEntry {
                    sig: edid_sig,
                    value: FieldValue::String(edid_sym),
                }],
                warnings: SmallVec::new(),
            });
        }

        let mut session = open_session(handle, None).unwrap();
        let inserted = session
            .add_records(records, &schema, &interner)
            .expect("batch add should succeed");
        assert_eq!(inserted, 2);

        let fks = session.form_keys_of_sig(ammo_sig, &interner).unwrap();
        assert_eq!(fks.len(), 2);
        assert!(session.record(0x0000_0801).is_ok());
        assert!(session.record(0x0000_0802).is_ok());
    }

    #[test]
    fn local_object_ids_in_handle_returns_indexed_object_ids() {
        let Some(handle) = load_fixture_with_weap_count(2, "MasterFixture.esm") else {
            return;
        };
        let Some(target_handle) = create_test_plugin_handle() else {
            return;
        };

        let mut session = open_session(target_handle, None).unwrap();
        let object_ids = session
            .local_object_ids_in_handle(handle)
            .expect("master object ID query should succeed");

        assert!(object_ids.contains(&0x000800));
        assert!(object_ids.contains(&0x000801));
        assert!(!object_ids.contains(&0x000802));
    }

    #[test]
    fn remove_record_via_session_drops_record() {
        let Some(handle) = load_fixture_with_weap() else {
            return;
        };
        let interner = StringInterner::new();

        {
            let mut session = open_session(handle, None).unwrap();
            let weap_sig = SigCode::from_str("WEAP").unwrap();
            let fk = session.form_keys_of_sig(weap_sig, &interner).unwrap()[0];
            assert!(session.remove_record(&fk).unwrap());
        }

        let mut session = open_session(handle, None).unwrap();
        assert!(matches!(
            session.record(0x0000_0800),
            Err(SessionError::RecordNotFound(_))
        ));
    }

    #[test]
    fn read_view_iterates_form_keys_in_parallel() {
        let Some(handle) = load_fixture_with_weap_count(128, "ManyWeaps.esp") else {
            return;
        };
        let mut session = open_session(handle, None).unwrap();
        let interner = StringInterner::new();
        let view = session.read_view().unwrap();
        let weap_sig = SigCode::from_str("WEAP").unwrap();
        let fks = view.form_keys_of_sig(weap_sig, &interner);

        let count = fks
            .par_iter()
            .filter(|fk| view.record(fk.local).is_some())
            .count();

        assert!(count > 50);
    }

    #[test]
    fn read_view_dropping_unblocks_mutation() {
        let Some(handle) = load_fixture_with_weap() else {
            return;
        };
        let mut session = open_session(handle, None).unwrap();
        {
            let interner = StringInterner::new();
            let view = session.read_view().unwrap();
            let _ = view.form_keys_of_sig(SigCode::from_str("WEAP").unwrap(), &interner);
        }

        let record = session.record_mut(0x0000_0800).unwrap();
        record.flags = 42;
        assert_eq!(session.record(0x0000_0800).unwrap().flags, 42);
    }

    #[test]
    fn read_view_exposes_target_signatures() {
        let Some(handle) = load_fixture_with_weap() else {
            return;
        };
        let mut session = open_session(handle, None).unwrap();
        let mut signatures = session.read_view().unwrap().target_signatures();
        signatures.sort_by_key(|sig| sig.as_str().to_string());

        assert_eq!(signatures, vec![SigCode::from_str("WEAP").unwrap()]);
    }

    #[test]
    fn read_view_record_decoded_matches_session_record_decoded() {
        let Some(handle) = load_fixture_with_weap() else {
            return;
        };
        let interner = StringInterner::new();
        let mut session = open_session(handle, None).unwrap();
        let schema = AuthoringSchema::for_game("fo4").unwrap();
        let fk = session
            .form_keys_of_sig(SigCode::from_str("WEAP").unwrap(), &interner)
            .unwrap()[0];

        let expected = session.record_decoded(&fk, &schema, &interner).unwrap();
        let actual = {
            let view = session.read_view().unwrap();
            view.record_decoded(&fk, &schema, &interner).unwrap()
        };

        assert_eq!(actual.form_key, expected.form_key);
        assert_eq!(actual.sig, expected.sig);
        assert_eq!(actual.fields.len(), expected.fields.len());
        assert_eq!(actual.eid, expected.eid);
    }

    #[test]
    fn handle_raw_scan_exposes_editor_ids_and_decodes_records() {
        let Some(handle) = load_fixture_with_weap_count(2, "RawScan.esp") else {
            return;
        };
        let interner = StringInterner::new();
        let schema = AuthoringSchema::for_game("fo4").unwrap();
        let mut session = open_session(handle, None).unwrap();
        let fks = session
            .form_keys_of_sig(SigCode::from_str("WEAP").unwrap(), &interner)
            .unwrap();
        let scan = session.handle_raw_scan(handle).unwrap();
        let raw_form_ids = scan.raw_form_ids_of_sig(SigCode::from_str("WEAP").unwrap());

        assert_eq!(raw_form_ids, vec![0x800, 0x801]);
        assert_eq!(scan.own_editor_ids().get(&0x800).unwrap(), "SessionWeap0");
        let decoded = scan
            .record_decoded(raw_form_ids[0], &fks[0], &schema, &interner)
            .unwrap()
            .unwrap();
        assert_eq!(decoded.form_key, fks[0]);
        assert_eq!(decoded.sig, SigCode::from_str("WEAP").unwrap());
    }

    #[test]
    fn targeted_reference_scan_returns_only_matching_carriers() {
        let Some(handle) = plugin_handle_new_native("TargetedRefs.esp", Some("fo4")).ok() else {
            return;
        };
        {
            let mut store = plugin_handle_store_ref().lock().unwrap();
            let slot = store.get_mut(&handle).unwrap();
            slot.parsed.root_items = vec![
                ParsedItem::Group(ParsedGroup {
                    label: *b"KYWD",
                    group_type: 0,
                    tail: Bytes::new(),
                    children: vec![ParsedItem::Record(ParsedRecord {
                        signature: SmolStr::new("KYWD"),
                        form_id: 0x0000_0800,
                        flags: 0,
                        version_control: 0,
                        form_version: None,
                        version2: None,
                        subrecords: Vec::new(),
                        raw_payload: None,
                        parse_error: None,
                    })],
                }),
                ParsedItem::Group(ParsedGroup {
                    label: *b"REFR",
                    group_type: 0,
                    tail: Bytes::new(),
                    children: vec![
                        ParsedItem::Record(ParsedRecord {
                            signature: SmolStr::new("REFR"),
                            form_id: 0x0000_0801,
                            flags: 0,
                            version_control: 0,
                            form_version: None,
                            version2: None,
                            subrecords: vec![ParsedSubrecord {
                                signature: SmolStr::new("NAME"),
                                data: Bytes::copy_from_slice(&0x0000_0800_u32.to_le_bytes()),
                                semantic_type: None,
                            }],
                            raw_payload: None,
                            parse_error: None,
                        }),
                        ParsedItem::Record(ParsedRecord {
                            signature: SmolStr::new("REFR"),
                            form_id: 0x0000_0802,
                            flags: 0,
                            version_control: 0,
                            form_version: None,
                            version2: None,
                            subrecords: Vec::new(),
                            raw_payload: None,
                            parse_error: None,
                        }),
                    ],
                }),
            ];
            slot.invalidate_sections();
        }

        let mut session = open_session(handle, None).unwrap();
        let query = "TargetedRefs.esp:000800".to_string();
        let matches = session
            .referencing_form_keys_by_query_in_handle(handle, std::slice::from_ref(&query))
            .unwrap();

        drop(session);
        assert_eq!(matches[&query], vec!["TargetedRefs.esp:000801"]);
        assert!(!plugin_handle_debug_section_loaded_native(handle, "refs").unwrap());
    }

    #[test]
    fn targeted_reference_scan_finds_vmad_property_carriers() {
        let Some(handle) = plugin_handle_new_native("TargetedVmad.esp", Some("fo4")).ok() else {
            return;
        };
        let vmad = build_vmad_bytes_from_payload(
            &serde_json::json!({
                "Version": 6,
                "Object Format": 2,
                "Scripts": [{
                    "ScriptName": "B21:StoryEventOnActivate",
                    "Flags": 0,
                    "Properties": [{
                        "propertyName": "StoryEventKeyword",
                        "Type": "Object",
                        "Flags": 1,
                        "Value": {
                            "Alias": -1,
                            "FormID": {
                                "reference": {
                                    "plugin": "TargetedVmad.esp",
                                    "object_id": "000800"
                                }
                            }
                        }
                    }]
                }]
            }),
            &[],
            "TargetedVmad.esp",
        )
        .unwrap();
        {
            let mut store = plugin_handle_store_ref().lock().unwrap();
            let slot = store.get_mut(&handle).unwrap();
            slot.parsed.root_items = vec![
                ParsedItem::Group(ParsedGroup {
                    label: *b"KYWD",
                    group_type: 0,
                    tail: Bytes::new(),
                    children: vec![ParsedItem::Record(ParsedRecord {
                        signature: SmolStr::new("KYWD"),
                        form_id: 0x0000_0800,
                        flags: 0,
                        version_control: 0,
                        form_version: None,
                        version2: None,
                        subrecords: Vec::new(),
                        raw_payload: None,
                        parse_error: None,
                    })],
                }),
                ParsedItem::Group(ParsedGroup {
                    label: *b"REFR",
                    group_type: 0,
                    tail: Bytes::new(),
                    children: vec![ParsedItem::Record(ParsedRecord {
                        signature: SmolStr::new("REFR"),
                        form_id: 0x0000_0801,
                        flags: 0,
                        version_control: 0,
                        form_version: None,
                        version2: None,
                        subrecords: vec![ParsedSubrecord {
                            signature: SmolStr::new("VMAD"),
                            data: Bytes::from(vmad),
                            semantic_type: None,
                        }],
                        raw_payload: None,
                        parse_error: None,
                    })],
                }),
            ];
            slot.invalidate_sections();
        }

        let mut session = open_session(handle, None).unwrap();
        let query = "TargetedVmad.esp:000800".to_string();
        let matches = session
            .referencing_form_keys_by_query_in_handle(handle, std::slice::from_ref(&query))
            .unwrap();

        drop(session);
        assert_eq!(matches[&query], vec!["TargetedVmad.esp:000801"]);
        assert!(!plugin_handle_debug_section_loaded_native(handle, "refs").unwrap());
    }

    #[test]
    fn map_apply_by_sig_serial_for_small_inputs() {
        let Some(handle) = load_fixture_with_weap_count(2, "SmallWeaps.esp") else {
            return;
        };
        let interner = StringInterner::new();
        let mut state = MapperState::new(
            std::iter::empty(),
            MapperOptions {
                output_plugin_name: "SmallWeaps.esp".into(),
                ..Default::default()
            },
        );
        let mut mapper = FormKeyMapper::from_state(&mut state, &interner);
        let mut session = open_session(handle, None).unwrap();

        let report = session
            .map_apply_by_sig(
                SigCode::from_str("WEAP").unwrap(),
                &mut mapper,
                |_view, _snapshot, _fk| Some(()),
                |session, _mapper, fk, ()| {
                    let changed = session
                        .patch_subrecord_bytes(fk, "DNAM", |bytes| {
                            bytes[0] = 0xFF;
                            true
                        })
                        .map_err(|err| FixupError::HandleError(err.to_string()))?;
                    Ok(if changed {
                        EditOutcome::Changed
                    } else {
                        EditOutcome::NoOp
                    })
                },
            )
            .unwrap();

        assert_eq!(report.records_changed, 2);
    }

    #[test]
    fn map_apply_by_sig_parallel_matches_serial() {
        let interner = StringInterner::new();
        let mut state_parallel = MapperState::new(
            std::iter::empty(),
            MapperOptions {
                output_plugin_name: "ParallelWeaps.esp".into(),
                ..Default::default()
            },
        );
        let mut state_serial = MapperState::new(
            std::iter::empty(),
            MapperOptions {
                output_plugin_name: "SerialWeaps.esp".into(),
                ..Default::default()
            },
        );
        let Some(parallel_handle) = load_fixture_with_weap_count(128, "ParallelWeaps.esp") else {
            return;
        };
        let Some(serial_handle) = load_fixture_with_weap_count(128, "SerialWeaps.esp") else {
            return;
        };
        let mut parallel_mapper = FormKeyMapper::from_state(&mut state_parallel, &interner);
        let mut serial_mapper = FormKeyMapper::from_state(&mut state_serial, &interner);
        let mut parallel_session = open_session(parallel_handle, None).unwrap();
        let mut serial_session = open_session(serial_handle, None).unwrap();
        let sig = SigCode::from_str("WEAP").unwrap();

        let parallel_report = parallel_session
            .map_apply_by_sig(
                sig,
                &mut parallel_mapper,
                |view, _snapshot, fk| view.record(fk.local).map(|_| ()),
                |session, _mapper, fk, ()| {
                    let changed = session
                        .patch_subrecord_bytes(fk, "DNAM", |bytes| {
                            bytes[0] = 0xEE;
                            true
                        })
                        .map_err(|err| FixupError::HandleError(err.to_string()))?;
                    Ok(if changed {
                        EditOutcome::Changed
                    } else {
                        EditOutcome::NoOp
                    })
                },
            )
            .unwrap();

        let mut serial_changed = 0;
        for fk in serial_session.form_keys_of_sig(sig, &interner).unwrap() {
            if serial_session
                .patch_subrecord_bytes(&fk, "DNAM", |bytes| {
                    bytes[0] = 0xEE;
                    true
                })
                .unwrap()
            {
                serial_changed += 1;
            }
        }
        let _ = &mut serial_mapper;

        assert_eq!(parallel_report.records_changed, serial_changed);
    }
}
