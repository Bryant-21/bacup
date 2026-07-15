//! Fixup: flatten OMOD includes when MODL is present (FO76 → FO4 only).
//!
//! In vanilla FO4, OMODs are one of two shapes:
//!   * Leaf attachments — have MODL (own attachment model), no includes.
//!   * Aggregators — have includes (references to other OMODs that contribute
//!     property rows), no MODL.
//!
//! FO76 allows the hybrid `MODL + includes` pattern, where a leaf OMOD pulls
//! base stats from a parent aggregator (typically `_PARENT_mod_WEAPON_GENERIC_*`).
//! When FO4 Creation Kit loads such a record, it logs
//!     "Object mod 'X' (FORMID) is tagged with a model. Removing invalid data."
//! and strips the includes — silently dropping every inherited property.
//!
//! This fixup eliminates the hybrid shape: for each target OMOD that has both
//! MODL and `include_count > 0`, it walks the include chain, appends the
//! included OMODs' DATA properties (recursively, with cycle detection) to this
//! OMOD's own properties array, then zeroes out the includes section. The
//! resulting record matches the vanilla FO4 leaf-OMOD shape; orphaned
//! `_PARENT_*` OMODs are removed by `PruneOrphanedRecordsFixup`.

use rustc_hash::{FxHashMap, FxHashSet};

use crate::fixups::{Fixup, FixupConfig, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::ids::{FormKey, SigCode, SubrecordSig};
use crate::record::{FieldValue, Record};
use crate::schema::AuthoringSchema;
use crate::session::PluginSession;
use crate::sym::{StringInterner, Sym};

pub struct FlattenOmodIncludesFixup;

// OMOD DATA layout constants — mirror `rewrite_raw_object_template_formids`.
const HEADER_LEN: usize = 20;
const ATTACH_PARENT_SLOT_COUNT_LEN: usize = 4;
const ITEM_COUNT_LEN: usize = 4;
const ITEM_ROW_LEN: usize = 8;
const INCLUDE_ROW_LEN: usize = 7;
const PROPERTY_ROW_LEN: usize = 24;

// Property-row field offsets (element_codec `B,x,x,x,B,x,x,x,H,x,x,I,I,f`).
// `value_type` selects how `value_1`/`value_2` are interpreted; for the two
// FormID-bearing value types `value_1` holds the referenced record's form_id.
const PROP_VALUE_TYPE_OFFSET: usize = 0;
const PROP_VALUE_1_OFFSET: usize = 12;
const PROP_VALUE_TYPE_FORMID_INT: u8 = 4;
const PROP_VALUE_TYPE_FORMID_FLOAT: u8 = 6;
const OBJECT_TEMPLATE_MOD_ASSOCIATION_RECORD_SIGS: &[&str] = &["WEAP", "ARMO", "NPC_"];

// KYWD TNAM `type` enum value for a ModAssociation keyword (FO4
// `keyword_type_enum`). FO4 forbids these inside OMOD instance-data
// properties — CK logs "Property mod on form 'X' is attempting to use a
// ModAssociation keyword. These do not work in instance data."
const KEYWORD_TYPE_MOD_ASSOCIATION: u64 = 5;

#[derive(Clone, Copy, Debug)]
struct DataLayout {
    include_count: usize,
    property_count: usize,
    includes_start: usize,
    includes_end: usize,
    properties_start: usize,
    properties_end: usize,
}

fn parse_data_layout(bytes: &[u8]) -> Option<DataLayout> {
    if bytes.len() < HEADER_LEN + ATTACH_PARENT_SLOT_COUNT_LEN + ITEM_COUNT_LEN {
        return None;
    }
    let include_count = u32::from_le_bytes(bytes[0..4].try_into().unwrap()) as usize;
    let property_count = u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize;
    let attach_parent_slot_count =
        u32::from_le_bytes(bytes[HEADER_LEN..HEADER_LEN + 4].try_into().unwrap()) as usize;
    let attach_parent_slots_start = HEADER_LEN + ATTACH_PARENT_SLOT_COUNT_LEN;
    let attach_parent_slots_len = attach_parent_slot_count.checked_mul(4)?;
    let item_count_offset = attach_parent_slots_start.checked_add(attach_parent_slots_len)?;
    if bytes.len() < item_count_offset + ITEM_COUNT_LEN {
        return None;
    }
    let item_count = u32::from_le_bytes(
        bytes[item_count_offset..item_count_offset + 4]
            .try_into()
            .unwrap(),
    ) as usize;
    let item_rows_len = item_count.checked_mul(ITEM_ROW_LEN)?;
    let includes_start = item_count_offset
        .checked_add(ITEM_COUNT_LEN)?
        .checked_add(item_rows_len)?;
    let includes_len = include_count.checked_mul(INCLUDE_ROW_LEN)?;
    let includes_end = includes_start.checked_add(includes_len)?;
    let properties_len = property_count.checked_mul(PROPERTY_ROW_LEN)?;
    let properties_start = includes_end;
    let properties_end = properties_start.checked_add(properties_len)?;
    if properties_end > bytes.len() {
        return None;
    }
    Some(DataLayout {
        include_count,
        property_count,
        includes_start,
        includes_end,
        properties_start,
        properties_end,
    })
}

fn data_sig() -> SubrecordSig {
    SubrecordSig(*b"DATA")
}

fn modl_sig() -> SubrecordSig {
    SubrecordSig(*b"MODL")
}

fn record_has_modl(record: &Record) -> bool {
    let modl = modl_sig();
    record.fields.iter().any(|e| e.sig == modl)
}

fn record_data_bytes(record: &Record) -> Option<Vec<u8>> {
    let data = data_sig();
    record.fields.iter().find(|e| e.sig == data).and_then(|e| {
        if let FieldValue::Bytes(b) = &e.value {
            Some(b.to_vec())
        } else {
            None
        }
    })
}

/// Compute the FO4-encoded form_id `(load_index << 24) | object_id` for a
/// FormKey. Returns `None` when the plugin isn't in `target_masters` or the
/// output plugin.
fn encoded_form_id(
    fk: &FormKey,
    target_masters: &[String],
    output_plugin: Sym,
    interner: &StringInterner,
) -> Option<u32> {
    let object_id = fk.local & 0x00FF_FFFF;
    if fk.plugin == output_plugin {
        let idx = target_masters.len() as u32;
        return Some((idx << 24) | object_id);
    }
    let plugin_str = interner.resolve(fk.plugin)?;
    let idx = target_masters
        .iter()
        .position(|m| m.eq_ignore_ascii_case(plugin_str))?;
    Some(((idx as u32) << 24) | object_id)
}

/// Walk one include's chain depth-first, appending each OMOD's property rows
/// (its own, NOT its descendants' includes — those are visited recursively).
fn collect_properties_from_chain(
    head_form_id: u32,
    data_by_form_id: &FxHashMap<u32, Vec<u8>>,
    visited: &mut FxHashSet<u32>,
    out_props: &mut Vec<u8>,
) {
    if !visited.insert(head_form_id) {
        return;
    }
    let Some(bytes) = data_by_form_id.get(&head_form_id) else {
        return;
    };
    let Some(layout) = parse_data_layout(bytes) else {
        return;
    };
    out_props.extend_from_slice(&bytes[layout.properties_start..layout.properties_end]);
    for i in 0..layout.include_count {
        let row = layout.includes_start + i * INCLUDE_ROW_LEN;
        let fid = u32::from_le_bytes(bytes[row..row + 4].try_into().unwrap());
        collect_properties_from_chain(fid, data_by_form_id, visited, out_props);
    }
}

/// Build replacement DATA bytes: drop the includes section, append `inlined`
/// property rows to the existing properties, and rewrite the count headers.
fn rebuild_data_bytes(original: &[u8], layout: &DataLayout, inlined: &[u8]) -> Vec<u8> {
    debug_assert_eq!(inlined.len() % PROPERTY_ROW_LEN, 0);
    let inlined_count = inlined.len() / PROPERTY_ROW_LEN;

    let mut out = Vec::with_capacity(
        layout.includes_start + (layout.properties_end - layout.properties_start) + inlined.len(),
    );
    // Everything before the includes section (header → items rows).
    out.extend_from_slice(&original[..layout.includes_start]);
    // Existing property rows.
    out.extend_from_slice(&original[layout.properties_start..layout.properties_end]);
    // Newly inlined property rows.
    out.extend_from_slice(inlined);

    // include_count := 0
    out[0..4].copy_from_slice(&0u32.to_le_bytes());
    // property_count := old + inlined
    let new_property_count = (layout.property_count + inlined_count) as u32;
    out[4..8].copy_from_slice(&new_property_count.to_le_bytes());
    out
}

fn tnam_sig() -> SubrecordSig {
    SubrecordSig(*b"TNAM")
}

/// Extract a KYWD's TNAM `type` enum value from its decoded record. TNAM is a
/// single `uint32` codec; depending on the schema it decodes to a flat `Uint`
/// or a one-field `Struct`. Handle both. `None` if TNAM is absent/undecodable.
fn kywd_type_value(record: &Record) -> Option<u64> {
    let tnam = tnam_sig();
    let entry = record.fields.iter().find(|e| e.sig == tnam)?;
    match &entry.value {
        FieldValue::Uint(v) => Some(*v),
        FieldValue::Int(v) => u64::try_from(*v).ok(),
        FieldValue::Struct(fields) => fields.iter().find_map(|(_, v)| match v {
            FieldValue::Uint(n) => Some(*n),
            FieldValue::Int(n) => u64::try_from(*n).ok(),
            _ => None,
        }),
        _ => None,
    }
}

/// Build replacement DATA bytes with every property row whose `value_type` is
/// FormID-bearing and whose referenced keyword `is_mod_association` removed,
/// decrementing `property_count` in lockstep. Returns `None` when nothing is
/// removed (so callers skip the rewrite entirely). The header/items/includes
/// prefix and all kept property rows are byte-copied verbatim.
fn strip_mod_association_properties(
    original: &[u8],
    layout: &DataLayout,
    mut is_mod_association: impl FnMut(u32) -> bool,
) -> Option<Vec<u8>> {
    let mut kept: Vec<u8> = Vec::with_capacity(layout.properties_end - layout.properties_start);
    let mut removed = 0usize;
    for i in 0..layout.property_count {
        let row = layout.properties_start + i * PROPERTY_ROW_LEN;
        let value_type = original[row + PROP_VALUE_TYPE_OFFSET];
        let is_formid =
            value_type == PROP_VALUE_TYPE_FORMID_INT || value_type == PROP_VALUE_TYPE_FORMID_FLOAT;
        if is_formid {
            let v1_at = row + PROP_VALUE_1_OFFSET;
            let form_id = u32::from_le_bytes(original[v1_at..v1_at + 4].try_into().unwrap());
            if form_id != 0 && is_mod_association(form_id) {
                removed += 1;
                continue;
            }
        }
        kept.extend_from_slice(&original[row..row + PROPERTY_ROW_LEN]);
    }
    if removed == 0 {
        return None;
    }

    let mut out = Vec::with_capacity(layout.properties_start + kept.len());
    // Header through includes (everything before the properties section) is
    // preserved verbatim — only properties are touched.
    out.extend_from_slice(&original[..layout.properties_start]);
    out.extend_from_slice(&kept);
    // Any trailing bytes after the properties section (none in current layout,
    // but copy defensively so we never truncate unknown tail data).
    out.extend_from_slice(&original[layout.properties_end..]);

    let new_property_count = (layout.property_count - removed) as u32;
    out[4..8].copy_from_slice(&new_property_count.to_le_bytes());
    Some(out)
}

/// Resolves whether an encoded form_id (`load_index << 24 | object_id`) refers
/// to a KYWD whose TNAM type is ModAssociation. Works for both own-plugin
/// (output) keywords and FO4 master keywords; master handles are read on
/// demand. Results are memoized — most OMODs reference the same handful of
/// attach-point keywords.
struct ModAssociationResolver<'a> {
    target_masters: &'a [String],
    target_master_handle_ids: &'a [u64],
    output_plugin: Sym,
    cache: FxHashMap<u32, bool>,
}

impl<'a> ModAssociationResolver<'a> {
    fn new(
        target_masters: &'a [String],
        target_master_handle_ids: &'a [u64],
        output_plugin: Sym,
    ) -> Self {
        Self {
            target_masters,
            target_master_handle_ids,
            output_plugin,
            cache: FxHashMap::default(),
        }
    }

    /// `true` only when `encoded` resolves to a KYWD with TNAM == ModAssociation.
    /// A keyword that can't be resolved (or isn't a KYWD) returns `false` so the
    /// property is LEFT in place — never guess-remove.
    fn is_mod_association(
        &mut self,
        session: &mut PluginSession,
        schema: &AuthoringSchema,
        interner: &StringInterner,
        encoded: u32,
    ) -> bool {
        if let Some(cached) = self.cache.get(&encoded) {
            return *cached;
        }
        let resolved = self.resolve(session, schema, interner, encoded);
        self.cache.insert(encoded, resolved);
        resolved
    }

    fn resolve(
        &self,
        session: &mut PluginSession,
        schema: &AuthoringSchema,
        interner: &StringInterner,
        encoded: u32,
    ) -> bool {
        let load_index = (encoded >> 24) as usize;
        let object_id = encoded & 0x00FF_FFFF;
        if object_id == 0 {
            return false;
        }
        // Output plugin: load_index == number of masters (it sits last).
        let record = if load_index == self.target_masters.len() {
            let fk = FormKey {
                local: object_id,
                plugin: self.output_plugin,
            };
            session.record_decoded(&fk, schema, interner).ok()
        } else {
            let Some(master_name) = self.target_masters.get(load_index) else {
                return false;
            };
            let Some(handle_id) = self.target_master_handle_ids.get(load_index).copied() else {
                return false;
            };
            let fk = FormKey {
                local: object_id,
                plugin: interner.intern(master_name),
            };
            session
                .record_decoded_in_handle(handle_id, &fk, schema, interner)
                .ok()
        };
        let Some(record) = record else {
            return false;
        };
        if record.sig.as_str() != "KYWD" {
            return false;
        }
        kywd_type_value(&record) == Some(KEYWORD_TYPE_MOD_ASSOCIATION)
    }
}

impl Fixup for FlattenOmodIncludesFixup {
    fn name(&self) -> &'static str {
        "flatten_omod_includes"
    }

    fn uses_session(&self) -> bool {
        true
    }

    fn applies_to_session(&self, session: &PluginSession, _config: &FixupConfig) -> bool {
        let target_is_fo4 = session
            .target_slot()
            .parsed
            .game
            .as_deref()
            .is_some_and(|game| game.eq_ignore_ascii_case("fo4"));
        let source_is_fo76 = session
            .source_slot_opt()
            .and_then(|slot| slot.parsed.game.as_deref())
            .is_some_and(|game| game.eq_ignore_ascii_case("fo76"));
        target_is_fo4 && source_is_fo76
    }

    fn run_with_session(
        &self,
        session: &mut PluginSession,
        mapper: &mut FormKeyMapper,
        config: &FixupConfig,
    ) -> Result<FixupReport, FixupError> {
        let omod_sig =
            SigCode::from_str("OMOD").map_err(|e| FixupError::SchemaError(e.to_string()))?;
        let target_schema = session
            .schema()
            .map_err(|e| FixupError::HandleError(e.to_string()))?;
        let target_masters = session.target_masters().to_vec();
        let output_plugin_name = session.target_slot().parsed.plugin_name.clone();
        let output_plugin = mapper.interner.intern(&output_plugin_name);

        let omod_fks = session
            .form_keys_of_sig(omod_sig, mapper.interner)
            .map_err(|e| FixupError::HandleError(e.to_string()))?;
        if omod_fks.is_empty() {
            return Ok(FixupReport::empty());
        }

        // Pass 1 — snapshot every OMOD's DATA bytes keyed by encoded form_id,
        // and remember which records are flatten candidates (MODL + includes).
        let mut data_by_form_id: FxHashMap<u32, Vec<u8>> = FxHashMap::default();
        let mut candidates: Vec<FormKey> = Vec::new();
        let mut report = FixupReport::empty();

        for fk in &omod_fks {
            let record = match session.record_decoded(fk, target_schema.as_ref(), mapper.interner) {
                Ok(r) => r,
                Err(e) => {
                    let w = mapper
                        .interner
                        .intern(&format!("flatten_omod_includes:read_err:{e}"));
                    report.warnings.push(w);
                    continue;
                }
            };
            let Some(data) = record_data_bytes(&record) else {
                continue;
            };
            let Some(encoded_id) =
                encoded_form_id(fk, &target_masters, output_plugin, mapper.interner)
            else {
                continue;
            };
            let Some(layout) = parse_data_layout(&data) else {
                continue;
            };
            let is_candidate = record_has_modl(&record) && layout.include_count > 0;
            data_by_form_id.insert(encoded_id, data);
            if is_candidate {
                candidates.push(*fk);
            }
        }

        // Pass 2 — flatten each candidate.
        let mut changed_records: Vec<Record> = Vec::with_capacity(candidates.len());
        for fk in candidates {
            let mut record =
                match session.record_decoded(&fk, target_schema.as_ref(), mapper.interner) {
                    Ok(r) => r,
                    Err(e) => {
                        let w = mapper
                            .interner
                            .intern(&format!("flatten_omod_includes:read_err:{e}"));
                        report.warnings.push(w);
                        continue;
                    }
                };
            let data_subrec = data_sig();
            let Some(data_idx) = record.fields.iter().position(|e| e.sig == data_subrec) else {
                continue;
            };
            let original_data = match &record.fields[data_idx].value {
                FieldValue::Bytes(b) => b.to_vec(),
                _ => continue,
            };
            let Some(layout) = parse_data_layout(&original_data) else {
                continue;
            };

            // Walk each direct include; mark self as visited so a cyclic
            // include row can't pull our own properties in twice.
            let mut visited: FxHashSet<u32> = FxHashSet::default();
            if let Some(self_id) =
                encoded_form_id(&fk, &target_masters, output_plugin, mapper.interner)
            {
                visited.insert(self_id);
            }
            let mut inlined: Vec<u8> = Vec::new();
            for i in 0..layout.include_count {
                let row = layout.includes_start + i * INCLUDE_ROW_LEN;
                let inc_fid = u32::from_le_bytes(original_data[row..row + 4].try_into().unwrap());
                collect_properties_from_chain(
                    inc_fid,
                    &data_by_form_id,
                    &mut visited,
                    &mut inlined,
                );
            }

            let new_data = rebuild_data_bytes(&original_data, &layout, &inlined);
            record.fields[data_idx].value =
                FieldValue::Bytes(smallvec::SmallVec::from_vec(new_data));
            changed_records.push(record);
        }

        report.records_changed = session
            .replace_records_contents(changed_records, target_schema.as_ref(), mapper.interner)
            .map_err(|e| FixupError::HandleError(e.to_string()))?
            .try_into()
            .unwrap_or(u32::MAX);

        // Pass 3 — strip ModAssociation property-mod rows from EVERY OMOD.
        // Runs after flatten so a flattened candidate's inlined parent rows are
        // also scrubbed. Re-reads each OMOD from the (now flattened) session.
        let mut strip_changed: Vec<Record> = Vec::new();
        let mut resolver = ModAssociationResolver::new(
            &target_masters,
            &config.target_master_handle_ids,
            output_plugin,
        );
        for fk in &omod_fks {
            let mut record =
                match session.record_decoded(fk, target_schema.as_ref(), mapper.interner) {
                    Ok(r) => r,
                    Err(_) => continue, // already warned in pass 1
                };
            let data_subrec = data_sig();
            let Some(data_idx) = record.fields.iter().position(|e| e.sig == data_subrec) else {
                continue;
            };
            let original_data = match &record.fields[data_idx].value {
                FieldValue::Bytes(b) => b.to_vec(),
                _ => continue,
            };
            let Some(layout) = parse_data_layout(&original_data) else {
                continue;
            };
            if layout.property_count == 0 {
                continue;
            }
            // Pre-resolve each FormID-bearing property's keyword type. Done in a
            // separate step because the resolver borrows `session` mutably and
            // the rebuild closure must not.
            let mut mod_assoc_form_ids: FxHashSet<u32> = FxHashSet::default();
            for i in 0..layout.property_count {
                let row = layout.properties_start + i * PROPERTY_ROW_LEN;
                let value_type = original_data[row + PROP_VALUE_TYPE_OFFSET];
                if value_type != PROP_VALUE_TYPE_FORMID_INT
                    && value_type != PROP_VALUE_TYPE_FORMID_FLOAT
                {
                    continue;
                }
                let v1_at = row + PROP_VALUE_1_OFFSET;
                let form_id =
                    u32::from_le_bytes(original_data[v1_at..v1_at + 4].try_into().unwrap());
                if form_id == 0 || mod_assoc_form_ids.contains(&form_id) {
                    continue;
                }
                if resolver.is_mod_association(
                    session,
                    target_schema.as_ref(),
                    mapper.interner,
                    form_id,
                ) {
                    mod_assoc_form_ids.insert(form_id);
                }
            }
            if mod_assoc_form_ids.is_empty() {
                continue;
            }
            let Some(new_data) = strip_mod_association_properties(&original_data, &layout, |fid| {
                mod_assoc_form_ids.contains(&fid)
            }) else {
                continue;
            };
            record.fields[data_idx].value =
                FieldValue::Bytes(smallvec::SmallVec::from_vec(new_data));
            strip_changed.push(record);
        }

        let stripped = session
            .replace_records_contents(strip_changed, target_schema.as_ref(), mapper.interner)
            .map_err(|e| FixupError::HandleError(e.to_string()))?;
        report.records_changed = report
            .records_changed
            .saturating_add(u32::try_from(stripped).unwrap_or(u32::MAX));

        // Pass 4 — strip the same ModAssociation property rows from decoded
        // object-template instance data (WEAP/ARMO/NPC_ OBTE row groups).
        let mut obte_changed: Vec<Record> = Vec::new();
        for sig_name in OBJECT_TEMPLATE_MOD_ASSOCIATION_RECORD_SIGS {
            let sig =
                SigCode::from_str(sig_name).map_err(|e| FixupError::SchemaError(e.to_string()))?;
            let form_keys = session
                .form_keys_of_sig(sig, mapper.interner)
                .map_err(|e| FixupError::HandleError(e.to_string()))?;
            for fk in form_keys {
                let mut record =
                    match session.record_decoded(&fk, target_schema.as_ref(), mapper.interner) {
                        Ok(r) => r,
                        Err(_) => continue,
                    };
                let mut should_strip = |form_id: u32| {
                    resolver.is_mod_association(
                        session,
                        target_schema.as_ref(),
                        mapper.interner,
                        form_id,
                    )
                };
                if strip_record_object_template_mod_association_properties(
                    &mut record,
                    mapper.interner,
                    &mut should_strip,
                ) {
                    obte_changed.push(record);
                }
            }
        }

        let obte_stripped = session
            .replace_records_contents(obte_changed, target_schema.as_ref(), mapper.interner)
            .map_err(|e| FixupError::HandleError(e.to_string()))?;
        report.records_changed = report
            .records_changed
            .saturating_add(u32::try_from(obte_stripped).unwrap_or(u32::MAX));

        Ok(report)
    }
}

fn strip_record_object_template_mod_association_properties<F>(
    record: &mut Record,
    interner: &StringInterner,
    is_mod_association: &mut F,
) -> bool
where
    F: FnMut(u32) -> bool,
{
    record.fields.iter_mut().fold(false, |changed, entry| {
        if entry.sig.0 == *b"OBTE" {
            strip_obte_mod_association_properties(&mut entry.value, interner, is_mod_association)
                | changed
        } else {
            changed
        }
    })
}

fn strip_obte_mod_association_properties<F>(
    value: &mut FieldValue,
    interner: &StringInterner,
    is_mod_association: &mut F,
) -> bool
where
    F: FnMut(u32) -> bool,
{
    let FieldValue::List(templates) = value else {
        return false;
    };
    templates.iter_mut().fold(false, |changed, template| {
        let FieldValue::Struct(fields) = template else {
            return changed;
        };
        strip_template_mod_association_properties(fields, interner, is_mod_association) | changed
    })
}

fn strip_template_mod_association_properties<F>(
    fields: &mut [(crate::sym::Sym, FieldValue)],
    interner: &StringInterner,
    is_mod_association: &mut F,
) -> bool
where
    F: FnMut(u32) -> bool,
{
    let Some(properties_index) = field_index_canonical(fields, "properties", interner) else {
        return false;
    };
    let FieldValue::List(properties) = &mut fields[properties_index].1 else {
        return false;
    };
    let before = properties.len();
    properties.retain(
        |property| match decoded_formid_property_value_1(property, interner) {
            Some(form_id) => !is_mod_association(form_id),
            None => true,
        },
    );
    before != properties.len()
}

fn decoded_formid_property_value_1(
    property: &FieldValue,
    interner: &StringInterner,
) -> Option<u32> {
    let FieldValue::Struct(fields) = property else {
        return None;
    };
    let value_type =
        field_index_canonical(fields, "valuetype", interner).map(|index| &fields[index].1)?;
    if !decoded_value_type_is_formid_bearing(value_type, interner) {
        return None;
    }
    field_index_canonical(fields, "value1", interner)
        .and_then(|index| field_value_to_u32(&fields[index].1))
}

fn decoded_value_type_is_formid_bearing(value: &FieldValue, interner: &StringInterner) -> bool {
    match value {
        FieldValue::Uint(value) => {
            *value == u64::from(PROP_VALUE_TYPE_FORMID_INT)
                || *value == u64::from(PROP_VALUE_TYPE_FORMID_FLOAT)
        }
        FieldValue::Int(value) => {
            *value == i64::from(PROP_VALUE_TYPE_FORMID_INT)
                || *value == i64::from(PROP_VALUE_TYPE_FORMID_FLOAT)
        }
        FieldValue::String(value) => interner.resolve(*value).is_some_and(|name| {
            let name = canonical_field_name(name);
            name == "formidint" || name == "formidfloat"
        }),
        _ => false,
    }
}

fn field_value_to_u32(value: &FieldValue) -> Option<u32> {
    match value {
        FieldValue::Uint(value) => u32::try_from(*value).ok(),
        FieldValue::Int(value) => u32::try_from(*value).ok(),
        FieldValue::Float(value) if value.is_finite() => {
            let rounded = value.round();
            (0.0..=u32::MAX as f32)
                .contains(&rounded)
                .then_some(rounded as u32)
        }
        FieldValue::Bytes(bytes) if bytes.len() >= 4 => {
            Some(u32::from_le_bytes(bytes[0..4].try_into().unwrap()))
        }
        FieldValue::Struct(fields) => fields
            .iter()
            .find_map(|(_, candidate)| field_value_to_u32(candidate)),
        _ => None,
    }
}

fn field_index_canonical(
    fields: &[(crate::sym::Sym, FieldValue)],
    name: &str,
    interner: &StringInterner,
) -> Option<usize> {
    let wanted = canonical_field_name(name);
    fields.iter().position(|(field_name, _)| {
        interner
            .resolve(*field_name)
            .is_some_and(|field_name| canonical_field_name(field_name) == wanted)
    })
}

fn canonical_field_name(name: &str) -> String {
    name.chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build an OMOD DATA blob with the given includes/properties.
    /// `attach_parent_slots` and `items` arrays are empty.
    fn make_data(includes: &[(u32, u8, u8, u8)], properties: &[[u8; PROPERTY_ROW_LEN]]) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&(includes.len() as u32).to_le_bytes()); // include_count
        out.extend_from_slice(&(properties.len() as u32).to_le_bytes()); // property_count
        out.extend_from_slice(&[0, 0]); // bool1, bool2
        out.extend_from_slice(b"WEAP"); // form_type
        out.extend_from_slice(&[0, 0]); // max_rank, level_offset
        out.extend_from_slice(&0u32.to_le_bytes()); // attach_point
        out.extend_from_slice(&0u32.to_le_bytes()); // attach_parent_slot_count = 0
        out.extend_from_slice(&0u32.to_le_bytes()); // item_count = 0
        for (fid, min_lvl, opt, dnu) in includes {
            out.extend_from_slice(&fid.to_le_bytes());
            out.push(*min_lvl);
            out.push(*opt);
            out.push(*dnu);
        }
        for prop in properties {
            out.extend_from_slice(prop);
        }
        out
    }

    fn prop_row(value_type: u8, function_type: u8, prop_id: u16, v1: u32, v2: u32) -> [u8; 24] {
        let mut row = [0u8; 24];
        row[0] = value_type;
        row[4] = function_type;
        row[8..10].copy_from_slice(&prop_id.to_le_bytes());
        row[12..16].copy_from_slice(&v1.to_le_bytes());
        row[16..20].copy_from_slice(&v2.to_le_bytes());
        // step = 0.0
        row
    }

    fn decoded_prop_row(
        interner: &mut StringInterner,
        value_type: FieldValue,
        v1: u32,
    ) -> FieldValue {
        FieldValue::Struct(vec![
            (interner.intern("ValueType"), value_type),
            (
                interner.intern("FunctionType"),
                FieldValue::String(interner.intern("MULADD")),
            ),
            (interner.intern("Property"), FieldValue::Uint(31)),
            (interner.intern("Value1"), FieldValue::Uint(u64::from(v1))),
            (interner.intern("Value2"), FieldValue::Uint(2)),
        ])
    }

    #[test]
    fn parse_data_layout_handles_simple_record() {
        let data = make_data(
            &[(0x07851142, 0, 0, 1)],
            &[prop_row(4, 2, 31, 0x00025018, 0x00000002)],
        );
        let layout = parse_data_layout(&data).expect("layout");
        assert_eq!(layout.include_count, 1);
        assert_eq!(layout.property_count, 1);
        assert_eq!(layout.includes_end - layout.includes_start, INCLUDE_ROW_LEN);
        assert_eq!(
            layout.properties_end - layout.properties_start,
            PROPERTY_ROW_LEN
        );
    }

    #[test]
    fn rebuild_drops_includes_and_appends_inlined_props() {
        let original = make_data(
            &[(0x07851142, 0, 0, 1)],
            &[prop_row(4, 2, 31, 0x00025018, 0x00000002)],
        );
        let layout = parse_data_layout(&original).unwrap();
        let parent_prop_a = prop_row(1, 1, 35, 0x3dcccccd, 0);
        let parent_prop_b = prop_row(1, 1, 47, 0xbe19999a, 0);
        let inlined: Vec<u8> = parent_prop_a
            .iter()
            .chain(parent_prop_b.iter())
            .copied()
            .collect();
        let rebuilt = rebuild_data_bytes(&original, &layout, &inlined);

        let new_layout = parse_data_layout(&rebuilt).expect("rebuilt layout");
        assert_eq!(new_layout.include_count, 0);
        assert_eq!(new_layout.property_count, 3); // 1 original + 2 inlined
        assert_eq!(
            new_layout.properties_end - new_layout.properties_start,
            3 * PROPERTY_ROW_LEN
        );
        // First 24 bytes of properties = our original prop
        let original_prop_start = new_layout.properties_start;
        assert_eq!(
            &rebuilt[original_prop_start..original_prop_start + PROPERTY_ROW_LEN],
            &original[layout.properties_start..layout.properties_end]
        );
    }

    #[test]
    fn collect_properties_walks_parent_chain() {
        // grandparent: 1 prop
        let grandparent_id = 0x07AAAAAA;
        let grandparent = make_data(&[], &[prop_row(1, 1, 7, 0x3f000000, 0)]);
        // parent: 1 prop + include of grandparent
        let parent_id = 0x07851142;
        let parent = make_data(
            &[(grandparent_id, 0, 0, 1)],
            &[prop_row(1, 1, 35, 0x3dcccccd, 0)],
        );

        let mut data_by_id = FxHashMap::default();
        data_by_id.insert(grandparent_id, grandparent);
        data_by_id.insert(parent_id, parent);

        let mut visited = FxHashSet::default();
        let mut out = Vec::new();
        collect_properties_from_chain(parent_id, &data_by_id, &mut visited, &mut out);

        // Should have collected 2 property rows: parent's, then grandparent's.
        assert_eq!(out.len(), 2 * PROPERTY_ROW_LEN);
        let parent_prop_id = u16::from_le_bytes(out[8..10].try_into().unwrap());
        assert_eq!(parent_prop_id, 35);
        let gp_prop_id = u16::from_le_bytes(
            out[PROPERTY_ROW_LEN + 8..PROPERTY_ROW_LEN + 10]
                .try_into()
                .unwrap(),
        );
        assert_eq!(gp_prop_id, 7);
    }

    #[test]
    fn collect_properties_breaks_cycles() {
        // A includes B; B includes A — must not recurse forever, and must not
        // double-count A's properties.
        let a_id = 0x07AAA001;
        let b_id = 0x07BBB001;
        let a = make_data(&[(b_id, 0, 0, 1)], &[prop_row(1, 1, 11, 0, 0)]);
        let b = make_data(&[(a_id, 0, 0, 1)], &[prop_row(1, 1, 22, 0, 0)]);

        let mut data_by_id = FxHashMap::default();
        data_by_id.insert(a_id, a);
        data_by_id.insert(b_id, b);

        let mut visited = FxHashSet::default();
        let mut out = Vec::new();
        collect_properties_from_chain(a_id, &data_by_id, &mut visited, &mut out);

        assert_eq!(out.len(), 2 * PROPERTY_ROW_LEN);
        let first = u16::from_le_bytes(out[8..10].try_into().unwrap());
        let second = u16::from_le_bytes(
            out[PROPERTY_ROW_LEN + 8..PROPERTY_ROW_LEN + 10]
                .try_into()
                .unwrap(),
        );
        // Order is DFS from A: A (id 11) then B (id 22).
        assert_eq!(first, 11);
        assert_eq!(second, 22);
    }

    #[test]
    fn collect_properties_self_include_is_no_op() {
        let a_id = 0x07AAA001;
        // A includes A itself.
        let a = make_data(&[(a_id, 0, 0, 1)], &[prop_row(1, 1, 11, 0, 0)]);
        let mut data_by_id = FxHashMap::default();
        data_by_id.insert(a_id, a);
        let mut visited = FxHashSet::default();
        let mut out = Vec::new();
        collect_properties_from_chain(a_id, &data_by_id, &mut visited, &mut out);
        // Only A's own properties — exactly once.
        assert_eq!(out.len(), PROPERTY_ROW_LEN);
    }

    #[test]
    fn missing_include_target_is_silently_skipped() {
        let a_id = 0x07AAA001;
        let missing_id = 0x07FFFFFF;
        let a = make_data(&[(missing_id, 0, 0, 1)], &[prop_row(1, 1, 11, 0, 0)]);
        let mut data_by_id = FxHashMap::default();
        data_by_id.insert(a_id, a);
        let mut visited = FxHashSet::default();
        let mut out = Vec::new();
        collect_properties_from_chain(a_id, &data_by_id, &mut visited, &mut out);
        // A's own props collected; the dangling include is ignored.
        assert_eq!(out.len(), PROPERTY_ROW_LEN);
    }

    #[test]
    fn strip_mod_association_removes_only_flagged_formid_props_and_decrements_count() {
        // Property rows:
        //  0: formid_int (vt 4) → ModAssociation keyword 0x07001111  → REMOVE
        //  1: formid_int (vt 4) → ordinary keyword     0x07002222  → KEEP
        //  2: int        (vt 0) → not a formid                       → KEEP
        //  3: formid_float (vt 6) → ModAssociation keyword 0x07003333 → REMOVE
        let kept_kywd = 0x0700_2222u32;
        let mod_assoc_a = 0x0700_1111u32;
        let mod_assoc_b = 0x0700_3333u32;
        let row_remove_a = prop_row(PROP_VALUE_TYPE_FORMID_INT, 2, 31, mod_assoc_a, 7);
        let row_keep_kywd = prop_row(PROP_VALUE_TYPE_FORMID_INT, 2, 32, kept_kywd, 9);
        let row_keep_int = prop_row(0, 1, 33, 0x1234_5678, 0xCAFE);
        let row_remove_b = prop_row(PROP_VALUE_TYPE_FORMID_FLOAT, 2, 34, mod_assoc_b, 0);
        let original = make_data(
            &[],
            &[row_remove_a, row_keep_kywd, row_keep_int, row_remove_b],
        );
        let layout = parse_data_layout(&original).expect("layout");
        assert_eq!(layout.property_count, 4);

        let is_mod_assoc = |fid: u32| fid == mod_assoc_a || fid == mod_assoc_b;
        let rebuilt =
            strip_mod_association_properties(&original, &layout, is_mod_assoc).expect("changed");

        // Re-decodes cleanly with no short/trailing bytes.
        let new_layout = parse_data_layout(&rebuilt).expect("rebuilt layout");
        // Exactly the two ModAssociation rows removed; count decremented in lockstep.
        assert_eq!(new_layout.property_count, 2);
        assert_eq!(
            new_layout.properties_end - new_layout.properties_start,
            2 * PROPERTY_ROW_LEN
        );
        assert_eq!(
            new_layout.properties_end,
            rebuilt.len(),
            "no trailing bytes after the properties section"
        );

        // The two kept rows are byte-identical and in original order.
        let p0 = new_layout.properties_start;
        assert_eq!(&rebuilt[p0..p0 + PROPERTY_ROW_LEN], &row_keep_kywd[..]);
        assert_eq!(
            &rebuilt[p0 + PROPERTY_ROW_LEN..p0 + 2 * PROPERTY_ROW_LEN],
            &row_keep_int[..]
        );
        // The non-property prefix is preserved verbatim EXCEPT property_count
        // (bytes 4..8), which is decremented in lockstep with the removed rows.
        assert_eq!(&rebuilt[..4], &original[..4]); // include_count unchanged
        assert_eq!(
            &rebuilt[8..layout.properties_start],
            &original[8..layout.properties_start]
        );
    }

    #[test]
    fn strip_mod_association_no_match_returns_none() {
        let original = make_data(
            &[],
            &[
                prop_row(PROP_VALUE_TYPE_FORMID_INT, 2, 31, 0x0700_2222, 7),
                prop_row(0, 1, 33, 0x1234_5678, 0),
            ],
        );
        let layout = parse_data_layout(&original).expect("layout");
        // No form_id resolves to ModAssociation → no rewrite.
        assert!(strip_mod_association_properties(&original, &layout, |_| false).is_none());
    }

    #[test]
    fn strip_decoded_obte_mod_association_property_rows() {
        use crate::ids::FormKey;
        use crate::record::FieldEntry;

        let mut interner = StringInterner::new();
        let mod_assoc = 0x0737_D0B2;
        let ordinary_keyword = 0x0700_2222;
        let formid_int = FieldValue::String(interner.intern("FormIDInt"));
        let float_value = FieldValue::String(interner.intern("Float"));
        let mut record = Record::new(
            SigCode::from_str("WEAP").unwrap(),
            FormKey {
                local: 0x12DBB3,
                plugin: interner.intern("SeventySix.esm"),
            },
        );
        record.fields.push(FieldEntry {
            sig: SubrecordSig(*b"OBTE"),
            value: FieldValue::List(vec![FieldValue::Struct(vec![(
                interner.intern("Properties"),
                FieldValue::List(vec![
                    decoded_prop_row(&mut interner, formid_int.clone(), mod_assoc),
                    decoded_prop_row(
                        &mut interner,
                        FieldValue::Uint(u64::from(PROP_VALUE_TYPE_FORMID_INT)),
                        ordinary_keyword,
                    ),
                    decoded_prop_row(&mut interner, float_value, mod_assoc),
                ]),
            )])]),
        });

        let mut is_mod_association = |form_id: u32| form_id == mod_assoc;
        assert!(strip_record_object_template_mod_association_properties(
            &mut record,
            &interner,
            &mut is_mod_association,
        ));
        let FieldValue::List(templates) = &record.fields[0].value else {
            panic!("expected OBTE template list");
        };
        let FieldValue::Struct(template_fields) = &templates[0] else {
            panic!("expected template struct");
        };
        let properties_index =
            field_index_canonical(template_fields, "properties", &interner).expect("properties");
        let FieldValue::List(properties) = &template_fields[properties_index].1 else {
            panic!("expected properties list");
        };
        assert_eq!(properties.len(), 2);
        assert_eq!(
            decoded_formid_property_value_1(&properties[0], &interner),
            Some(ordinary_keyword)
        );
        assert_eq!(
            decoded_formid_property_value_1(&properties[1], &interner),
            None
        );
    }

    #[test]
    fn kywd_type_value_reads_flat_and_struct_tnam() {
        use crate::ids::FormKey;
        use crate::record::FieldEntry;
        use crate::sym::StringInterner;

        let interner = StringInterner::new();
        let fk = FormKey {
            local: 1,
            plugin: interner.intern("out.esp"),
        };
        let mut flat = Record::new(SigCode::from_str("KYWD").unwrap(), fk);
        flat.fields.push(FieldEntry {
            sig: SubrecordSig(*b"TNAM"),
            value: FieldValue::Uint(KEYWORD_TYPE_MOD_ASSOCIATION),
        });
        assert_eq!(kywd_type_value(&flat), Some(KEYWORD_TYPE_MOD_ASSOCIATION));

        let mut structured = Record::new(SigCode::from_str("KYWD").unwrap(), fk);
        structured.fields.push(FieldEntry {
            sig: SubrecordSig(*b"TNAM"),
            value: FieldValue::Struct(vec![(
                interner.intern("type"),
                FieldValue::Uint(KEYWORD_TYPE_MOD_ASSOCIATION),
            )]),
        });
        assert_eq!(
            kywd_type_value(&structured),
            Some(KEYWORD_TYPE_MOD_ASSOCIATION)
        );

        // A keyword with no TNAM (or non-ModAssociation) is not flagged.
        let bare = Record::new(SigCode::from_str("KYWD").unwrap(), fk);
        assert_eq!(kywd_type_value(&bare), None);
    }
}
